#!/usr/bin/env python3
"""M5 staged launcher. Execution remains disabled pending containment review."""
import argparse, hashlib, json, os, shutil, signal, stat, subprocess, time, uuid
from pathlib import Path

READY_FOR_HOST_INTAKE = False

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def record(path, value):
    temp = path.with_suffix(path.suffix + '.tmp')
    temp.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
    os.replace(temp, path)

def freeze_file(src, dest):
    if not src.is_file() or dest.exists() or src.is_symlink(): raise ValueError('invalid freeze source/destination')
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    if sha(src) != sha(dest): raise ValueError('input changed while freezing')
    return sha(dest)

def tracked_sources(root):
    names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard',
                                     '--', 'src', 'scenarios', 'Cargo.toml', 'Cargo.lock'], cwd=root).split(b'\0')
    result = {}
    for name in sorted(set(names)):
        if not name: continue
        rel = os.fsdecode(name); p = root / rel
        if p.is_file() and not p.is_symlink(): result[rel] = sha(p)
    if not result: raise ValueError('empty source inventory')
    return result

def cgroup_scope(command, memory_bytes, tasks, cpu_percent, unit, runtime_seconds):
    if memory_bytes <= 0 or tasks <= 0 or cpu_percent <= 0 or not 1 <= runtime_seconds <= 600: raise ValueError('invalid cgroup limits')
    if not unit.startswith('m5-') or not unit.replace('-', '').isalnum(): raise ValueError('invalid scope name')
    return ['systemd-run', '--user', '--scope', '--collect', '--unit', unit, '--slice=app.slice',
            '--property', 'RuntimeMaxSec=' + str(runtime_seconds) + 's', '--property', 'TimeoutStopSec=3s',
            '--property', 'KillMode=control-group', '--property', 'MemoryMax=' + str(memory_bytes), '--property', 'MemorySwapMax=0',
            '--property', 'TasksMax=' + str(tasks), '--property', 'CPUQuota=' + str(cpu_percent) + '%',
            '--', *command]

def scope_not_present(unit):
    result = subprocess.run(['systemctl', '--user', 'show', '--property=LoadState', '--value', unit + '.scope'],
                            capture_output=True, text=True, timeout=5, check=False)
    return result.returncode == 0 and result.stdout.strip() == 'not-found'

def scope_invocation(unit):
    try:
        result = subprocess.run(['systemctl', '--user', 'show', '--property=InvocationID', '--value', unit + '.scope'],
                                capture_output=True, text=True, timeout=5, check=False)
        value = result.stdout.strip()
        return value if result.returncode == 0 and len(value) == 32 and all(c in '0123456789abcdef' for c in value) else None
    except (OSError, subprocess.SubprocessError): return None

def scope_contains_child(unit, root_pid):
    """Prove ownership from this launcher's live descendant cgroup membership."""
    pending, seen = [root_pid], set()
    while pending and len(seen) < 512:
        pid = pending.pop()
        if pid in seen: continue
        seen.add(pid)
        try:
            group = Path(f'/proc/{pid}/cgroup').read_text().splitlines()
            if any(line.startswith('0::') and line.split(':', 2)[2].endswith('/' + unit + '.scope')
                   for line in group): return True
            children = Path(f'/proc/{pid}/task/{pid}/children').read_text().split()
            pending.extend(int(child) for child in children)
        except (OSError, ValueError): pass
    return False

def scope_empty(unit):
    parent = Path('/sys/fs/cgroup/user.slice') / f'user-{os.getuid()}.slice' / f'user@{os.getuid()}.service' / 'app.slice'
    if not parent.is_dir(): return None
    path = parent / (unit + '.scope')
    try:
        events = (path / 'cgroup.events').read_text().splitlines()
    except FileNotFoundError: return True
    except OSError: return None
    return 'populated 0' in events if any(line.startswith('populated ') for line in events) else None

def wait_scope_empty(unit, earliest, deadline):
    while time.monotonic() < deadline:
        empty = scope_empty(unit)
        if time.monotonic() >= earliest and empty is True: return True
        time.sleep(.1)
    return scope_empty(unit) is True and time.monotonic() >= earliest

def run_scope(cmd, unit, log, timeout):
    """Supervise and clean only a uniquely named, verified owned scope."""
    if not scope_not_present(unit): raise ValueError('scope name already exists or cannot be checked')
    limits = [part for part in cmd if part.startswith('RuntimeMaxSec=') and part.endswith('s')]
    if len(limits) != 1: raise ValueError('scope runtime backstop missing')
    runtime = int(limits[0].removeprefix('RuntimeMaxSec=').removesuffix('s'))
    if not 1 <= runtime <= 600 or 'TimeoutStopSec=3s' not in cmd or 'KillMode=control-group' not in cmd or '--slice=app.slice' not in cmd:
        raise ValueError('scope backstop properties invalid')
    start = time.time_ns(); started_monotonic = time.monotonic(); proc = None; owned = False; invocation = None; cleanup = []; normal = False
    with log.open('xb') as stream:
        try:
            proc = subprocess.Popen(cmd, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
            deadline = time.monotonic() + timeout
            while proc.poll() is None:
                if not owned and scope_contains_child(unit, proc.pid):
                    invocation = scope_invocation(unit)
                    owned = invocation is not None
                if time.monotonic() >= deadline: raise TimeoutError('scope deadline exceeded')
                time.sleep(.1)
            code = proc.wait(timeout=5)
            if code != 0: raise RuntimeError('scope launcher failed with exit ' + str(code))
            if not owned: raise RuntimeError('scope ownership was not observed')
            normal = True
            return {'argv': cmd, 'launcher_pid': proc.pid, 'start_ns': start, 'exit_ns': time.time_ns(),
                    'exit_code': code, 'timed_out': False, 'scope': unit + '.scope', 'ownership_observed': True}
        finally:
            if not normal and proc is not None:
                if not owned and scope_contains_child(unit, proc.pid):
                    invocation = scope_invocation(unit)
                    owned = invocation is not None
                if owned:
                    for sig in ('TERM', 'KILL'):
                        if scope_invocation(unit) != invocation:
                            cleanup.append({'scope_signal':sig, 'reason':'invocation changed or exited'})
                            break
                        try:
                            result = subprocess.run(['systemctl', '--user', 'kill', '--kill-who=all',
                                '--signal=' + sig, unit + '.scope'], stdout=stream, stderr=subprocess.STDOUT,
                                timeout=5, check=False)
                            cleanup.append({'scope_signal':sig, 'exit_code':result.returncode})
                        except BaseException as exc: cleanup.append({'scope_signal':sig, 'error':repr(exc)})
                        if sig == 'TERM':
                            try: proc.wait(timeout=3)
                            except subprocess.TimeoutExpired: pass
                else:
                    cleanup.append({'scope_signal':'withheld', 'reason':'ownership not observed'})
                if proc.poll() is None:
                    try: os.killpg(proc.pid, signal.SIGTERM); cleanup.append({'process_group':'TERM'})
                    except ProcessLookupError: pass
                    try: proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        try: os.killpg(proc.pid, signal.SIGKILL); cleanup.append({'process_group':'KILL'})
                        except ProcessLookupError: pass
                        try: proc.wait(timeout=3)
                        except subprocess.TimeoutExpired: cleanup.append({'process_group':'unreaped'})
            if proc is not None:
                backstop_end = started_monotonic + runtime + 3 + 5
                earliest = started_monotonic if owned else backstop_end
                try: verified_empty = wait_scope_empty(unit, earliest, backstop_end + 2)
                except BaseException as exc:
                    verified_empty = False
                    cleanup.append({'empty_check_error':repr(exc)})
                cleanup.append({'cgroup_empty_verified':verified_empty, 'backstop_seconds':runtime + 3})
            failed_empty_check = proc is not None and normal and not verified_empty
            if failed_empty_check: normal = False
            record(log.with_suffix('.cleanup.json'), {'scope':unit + '.scope', 'normal':normal,
                'ownership_observed':owned, 'invocation':invocation, 'cleanup':cleanup})
            if failed_empty_check: raise RuntimeError('scope termination could not be verified')

def validate_render_sysfs_target(target, render_name):
    devices = Path('/sys/devices')
    if not target.is_absolute() or not target.is_relative_to(devices) or target.name != render_name or target.parent.name != 'drm':
        raise ValueError('render sysfs target is not the selected device')
    root = target.parent.parent
    if root == devices or not root.is_relative_to(devices): raise ValueError('render device subtree too broad')
    return root

def device_identity(wayland_socket, gpu_device):
    runtime = os.environ.get('XDG_RUNTIME_DIR'); display = os.environ.get('WAYLAND_DISPLAY')
    if not runtime or not display or '/' in display or display in ('.', '..'):
        raise ValueError('host Wayland display unavailable')
    expected = Path(runtime) / display
    if wayland_socket != expected or wayland_socket.is_symlink(): raise ValueError('unexpected Wayland path')
    socket_stat = wayland_socket.stat()
    if not stat.S_ISSOCK(socket_stat.st_mode): raise ValueError('Wayland path is not a socket')
    if gpu_device.parent != Path('/dev/dri') or not gpu_device.name.startswith('renderD') or not gpu_device.name[7:].isdigit() or gpu_device.is_symlink():
        raise ValueError('invalid render device path')
    device_stat = gpu_device.stat()
    if not stat.S_ISCHR(device_stat.st_mode) or os.major(device_stat.st_rdev) != 226 or os.minor(device_stat.st_rdev) < 128:
        raise ValueError('invalid DRM render device')
    devnum = str(os.major(device_stat.st_rdev)) + ':' + str(os.minor(device_stat.st_rdev))
    link = Path('/sys/dev/char') / devnum
    if not link.is_symlink(): raise ValueError('render sysfs link missing')
    target = link.resolve(strict=True)
    root = validate_render_sysfs_target(target, gpu_device.name)
    sysstat = root.stat(); targetstat = target.stat()
    return {'wayland': {'path':str(wayland_socket), 'device':socket_stat.st_dev, 'inode':socket_stat.st_ino},
            'render': {'path':str(gpu_device), 'device':device_stat.st_dev, 'inode':device_stat.st_ino,
                       'rdev':device_stat.st_rdev},
            'sysfs': {'root':str(root), 'target':str(target), 'devnum':devnum,
                      'device':sysstat.st_dev, 'inode':sysstat.st_ino,
                      'target_device':targetstat.st_dev, 'target_inode':targetstat.st_ino}}

def namespace_command(sandbox, frozen, wayland_socket, gpu_device, phase, config, preflight=False):
    """Construct a narrow mount namespace; review on the actual host before use."""
    if phase not in ('seed', 'reopen'): raise ValueError('invalid phase')
    for path in (sandbox, frozen, wayland_socket, gpu_device):
        if not path.is_absolute() or not path.exists(): raise ValueError('missing namespace input ' + str(path))
    # Do not bind frozen/ as a directory: it also contains original fixture bodies.
    command = ['bwrap', '--die-with-parent', '--new-session', '--unshare-user', '--uid', '0', '--gid', '0', '--cap-add', 'CAP_NET_ADMIN',
               '--unshare-net', '--unshare-pid', '--unshare-ipc',
               '--proc', '/proc', '--dev', '/dev',
               '--ro-bind', '/usr', '/usr',
               '--symlink', 'usr/lib', '/lib', '--symlink', 'usr/lib64', '/lib64',
               '--dir', '/m5', '--dir', '/m5/input', '--bind', str(sandbox), '/m5/run',
               '--dir', '/dev/dri', '--dev-bind', str(gpu_device), '/dev/dri/' + gpu_device.name,
               '--bind', str(wayland_socket), '/m5/run/runtime/wayland-0']
    device_stat = gpu_device.stat()
    devnum = str(os.major(device_stat.st_rdev)) + ':' + str(os.minor(device_stat.st_rdev))
    target = (Path('/sys/dev/char') / devnum).resolve(strict=True)
    gpu_root = validate_render_sysfs_target(target, gpu_device.name)
    command += ['--dir', '/sys', '--dir', '/sys/dev', '--dir', '/sys/dev/char', '--dir', '/sys/devices']
    ancestor = Path('/sys/devices')
    for part in gpu_root.relative_to(ancestor).parts[:-1]:
        ancestor = ancestor / part
        command += ['--dir', str(ancestor)]
    command += ['--ro-bind', str(gpu_root), str(gpu_root),
                '--symlink', os.path.relpath(target, '/sys/dev/char'), '/sys/dev/char/' + devnum,
                '--ro-bind', '/sys/fs/cgroup', '/sys/fs/cgroup']
    for name in ('turnstone', 'custodian', 'test-binary', 'namespace_phase.py', 'fixture_server.py', phase + '.scn'):
        path = frozen / name
        if not path.is_file(): raise ValueError('missing frozen input ' + str(path))
        command += ['--ro-bind', str(path), '/m5/input/' + name]
    entry = '/m5/input/namespace_phase.py'
    if preflight:
        path = frozen / 'preflight_phase.py'
        if not path.is_file(): raise ValueError('missing frozen preflight entry')
        command += ['--ro-bind', str(path), '/m5/input/preflight_phase.py']
        entry = '/m5/input/preflight_phase.py'
    command += ['--ro-bind', str(config), '/m5/input/config.json', '--chdir', '/m5/run',
                '/usr/bin/python3', entry, '/m5/input/config.json']
    return command

def verify_source_unchanged(root, before):
    if tracked_sources(root) != before: raise ValueError('source set or contents changed')

def execute(options):
    out = options.output.resolve()
    if out.exists(): raise ValueError('output directory already exists')
    out.mkdir(mode=0o700, parents=True)
    sandbox, frozen = out / 'sandbox', out / 'frozen'
    sandbox.mkdir(mode=0o700); frozen.mkdir(mode=0o700)
    (sandbox / 'm5-disposable-marker').write_text('m5-disposable/v1\n')
    for name in ('home', 'data', 'config', 'cache', 'runtime', 'mere', 'profile', 'control'):
        (sandbox / name).mkdir(mode=0o700)
    inputs = {'turnstone': options.application, 'custodian': options.custodian,
              'test-binary': options.test_binary, 'seed.scn': options.seed,
              'reopen.scn': options.reopen, 'namespace_phase.py': Path(__file__).with_name('namespace_phase.py'),
              'fixture_server.py': Path(__file__).with_name('fixture_server.py')}
    hashes = {name: freeze_file(path.resolve(), frozen / name) for name, path in inputs.items()}
    shutil.copytree(options.fixtures, frozen / 'fixtures')
    fixture_hashes = {str(p.relative_to(frozen)): sha(p) for p in (frozen / 'fixtures').rglob('*') if p.is_file()}
    shutil.copytree(frozen / 'fixtures', sandbox / 'fixture-source')
    source_before = tracked_sources(options.source_root)
    record(out / 'inputs.json', {'files': hashes, 'fixtures': fixture_hashes, 'source': source_before,
                               'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=options.source_root, text=True).strip()})
    try:
        for phase in ('seed', 'reopen'):
            verify_source_unchanged(options.source_root, source_before)
            config = {'phase': phase, 'output': '/m5/run', 'scenario': '/m5/input/' + phase + '.scn',
                      'fixture_server': '/m5/input/fixture_server.py', 'custodian': '/m5/input/custodian',
                      'application': '/m5/input/turnstone', 'test_binary': '/m5/input/test-binary',
                      'inspection_observation': options.inspection_observation,
                      'source_observation': options.source_observation,
                      'timeout_seconds': options.timeout, 'inspection_timeout_seconds': 60,
                      'memory_bytes': options.memory_bytes, 'tasks_max': options.tasks,
                      'cpu_percent': options.cpu_percent,
                      'environment': {'PATH': '/usr/bin', 'WAYLAND_DISPLAY': 'wayland-0', 'RUST_LOG': 'warn'}}
            config_path = frozen / (phase + '-config.json'); record(config_path, config)
            ns = namespace_command(sandbox, frozen, options.wayland_socket, options.gpu_device, phase, config_path)
            unit = 'm5-' + phase + '-' + uuid.uuid4().hex
            cmd = cgroup_scope(ns, options.memory_bytes, options.tasks, options.cpu_percent, unit, min(options.timeout + 120, 600))
            result = run_scope(cmd, unit, out / (phase + '-launcher.log'), options.timeout + 120)
            record(out / (phase + '-launcher.json'), result)
            if result['exit_code'] or result['timed_out']: raise ValueError(phase + ' launcher failed')
        verify_source_unchanged(options.source_root, source_before)
    except BaseException as exc:
        record(out / 'failure.json', {'error': repr(exc), 'time_ns': time.time_ns()})
        raise

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('output', 'application', 'custodian', 'test-binary', 'seed', 'reopen',
                 'fixtures', 'source-root', 'wayland-socket', 'gpu-device'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--inspection-observation', required=True)
    parser.add_argument('--source-observation', required=True)
    parser.add_argument('--memory-bytes', type=int, required=True)
    parser.add_argument('--tasks', type=int, required=True)
    parser.add_argument('--cpu-percent', type=int, required=True)
    parser.add_argument('--timeout', type=int, default=600)
    options = parser.parse_args()
    if not READY_FOR_HOST_INTAKE:
        raise SystemExit('DRAFT ONLY: no processes started; complete and review intake gates first')
    execute(options)
if __name__ == '__main__': main()
