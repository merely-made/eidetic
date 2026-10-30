#!/usr/bin/env python3
"""Staged phase supervisor; run only inside the reviewed isolated cgroup/namespace.

Not an independent isolation boundary. The parent launcher must verify mount,
network and cgroup limits before invoking this module. Source bodies enter only
through files copied into the private fixture directory.
"""
import ctypes, hashlib, json, os, shutil, signal, socket, stat, struct, subprocess, sys, time
from pathlib import Path

class PhaseError(RuntimeError): pass

def write(path, obj):
    temp = path.with_suffix(path.suffix + '.tmp')
    temp.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + '\n')
    os.replace(temp, path)

def validate_png(path):
    from PIL import Image
    with Image.open(path) as im:
        if im.format != 'PNG' or im.width < 16 or im.height < 16 or im.width * im.height > 100_000_000:
            raise PhaseError('GUI frame dimensions or format invalid')
        im.load()
        if im.convert('RGBA').getchannel('A').getbbox() is None:
            raise PhaseError('GUI frame is entirely transparent')
        return [im.width, im.height]

def wait_ready(proc, predicate, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if proc.poll() is not None: raise PhaseError('process exited before readiness')
        value = predicate()
        if value is not None: return value
        time.sleep(.05)
    raise PhaseError('readiness deadline exceeded')

def json_for_pid(path, pid):
    try:
        value = json.loads(path.read_text())
        return value if value.get('pid') == pid else None
    except (OSError, ValueError): return None

class Child:
    def __init__(self, argv, log, env, stdin=None):
        self.argv, self.start_ns, self.exit_ns = argv, time.time_ns(), None
        self.log = log.open('xb')
        self.proc = subprocess.Popen(argv, env=env, stdin=stdin, stdout=self.log,
                                     stderr=subprocess.STDOUT, start_new_session=True)
    def result(self):
        if self.proc.poll() is None: raise PhaseError('process has not exited')
        if self.exit_ns is None: self.exit_ns = time.time_ns()
        return {'argv': self.argv, 'pid': self.proc.pid, 'start_ns': self.start_ns,
                'exit_ns': self.exit_ns, 'exit_code': self.proc.returncode,
                'pid_namespace': os.readlink('/proc/self/ns/pid')}
    def stop(self, graceful_stdin=False):
        if self.proc.poll() is None:
            if graceful_stdin and self.proc.stdin:
                self.proc.stdin.write(b'\n'); self.proc.stdin.flush(); self.proc.stdin.close()
            else: os.killpg(self.proc.pid, signal.SIGTERM)
            try: self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL); self.proc.wait(timeout=5)
        value = self.result(); self.log.close(); return value

def drop_setup_capabilities():
    """Drop all setup capabilities before starting any supervised child."""
    class Header(ctypes.Structure):
        _fields_ = [('version', ctypes.c_uint32), ('pid', ctypes.c_int)]
    class Data(ctypes.Structure):
        _fields_ = [('effective', ctypes.c_uint32), ('permitted', ctypes.c_uint32),
                    ('inheritable', ctypes.c_uint32)]
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS
        raise OSError(ctypes.get_errno(), 'PR_SET_NO_NEW_PRIVS failed')
    header, data = Header(0x20080522, 0), (Data * 2)()
    if libc.capset(ctypes.byref(header), ctypes.byref(data)) != 0:
        raise OSError(ctypes.get_errno(), 'capset failed')
    fields = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    for name in ('CapInh', 'CapPrm', 'CapEff', 'CapAmb'):
        if int(fields[name].strip(), 16) != 0: raise PhaseError('setup capability remained: ' + name)
    if fields['NoNewPrivs'].strip() != '1': raise PhaseError('no_new_privs not enforced')
    return {'effective':'0', 'permitted':'0', 'inheritable':'0', 'ambient':'0', 'no_new_privs':True}

def interfaces():
    addr = json.loads(subprocess.check_output(['ip', '-json', 'address', 'show'], text=True))
    routes = json.loads(subprocess.check_output(['ip', '-json', 'route', 'show', 'table', 'all'], text=True))
    if any(item.get('ifname') != 'lo' for item in addr): raise PhaseError('external interface visible')
    if any(r.get('dev') not in (None, 'lo') for r in routes): raise PhaseError('external route visible')
    return {'addresses': addr, 'routes': routes, 'netns': os.readlink('/proc/self/ns/net'),
            'mountns': os.readlink('/proc/self/ns/mnt'), 'userns': os.readlink('/proc/self/ns/user'),
            'pidns': os.readlink('/proc/self/ns/pid'), 'ipcns': os.readlink('/proc/self/ns/ipc')}

def fixture_ready(path, expected_pid):
    if not path.exists(): return None
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
        sock.settimeout(1)
        try:
            sock.connect(str(path))
            pid, uid, gid = struct.unpack('3i', sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        except OSError:
            return None
    if pid != expected_pid: raise PhaseError('custody socket belongs to a different process')
    info = path.stat()
    if not stat.S_ISSOCK(info.st_mode): raise PhaseError('custody endpoint is not a socket')
    return {'pid': pid, 'uid': uid, 'gid': gid, 'device': info.st_dev, 'inode': info.st_ino}

def remove_own_socket(root, path, child):
    if child.proc.poll() is None: raise PhaseError('custody still running')
    if path.parent != root or path.name != 'app.sock': raise PhaseError('unsafe socket path')
    try: info = path.lstat()
    except FileNotFoundError: return
    if not stat.S_ISSOCK(info.st_mode): raise PhaseError('fixture endpoint replaced by non-socket')
    path.unlink()

def refused(host, port):
    with socket.socket() as s:
        s.settimeout(1)
        return s.connect_ex((host, port)) != 0

class TreeReadError(PhaseError):
    def __init__(self, path, reason, partial):
        super().__init__(reason + ': ' + path)
        self.evidence = {'path': path, 'reason': reason, 'partial_tree': dict(partial)}


def tree_hashes(root):
    result, current = {}, '.'
    try:
        if root.is_symlink() or not root.is_dir():
            raise TreeReadError('.', 'root is not a plain directory', result)
        for path in sorted(root.rglob('*')):
            current = str(path.relative_to(root))
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode): raise TreeReadError(current, 'symlink', result)
            if stat.S_ISDIR(mode):
                result[current + '/'] = 'directory'
            elif stat.S_ISREG(mode):
                h = hashlib.sha256()
                with path.open('rb') as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b''): h.update(chunk)
                result[current] = h.hexdigest()
            else: raise TreeReadError(current, 'nonregular mode ' + oct(stat.S_IFMT(mode)), result)
    except OSError as exc:
        raise TreeReadError(current, repr(exc), result) from exc
    return result

def inspection_integrity(profile, original, authoritative, authoritative_before,
                         working, working_before, cleanup_error=None):
    snapshots = {}
    for name, path in (('primary', profile), ('authoritative', authoritative), ('working', working)):
        try: snapshots[name] = {'tree': tree_hashes(path)}
        except TreeReadError as exc: snapshots[name] = {'error': repr(exc), **exc.evidence}
        except BaseException as exc: snapshots[name] = {'error': repr(exc)}
    after = snapshots['working'].get('tree')
    added = {} if after is None else {name: value for name, value in after.items() if name not in working_before}
    empty = hashlib.sha256(b'').hexdigest()
    allowed = after is not None and all(Path(name).name in ('.fjall-owner.lock', '.turnstone-session-writer.lock')
                  and value == empty for name, value in added.items())
    stable = after is not None and all(after.get(name) == value for name, value in working_before.items())
    primary_stable = snapshots['primary'].get('tree') == original
    evidence_stable = snapshots['authoritative'].get('tree') == authoritative_before
    return {'before': working_before, 'snapshots': snapshots, 'cleanup_error': cleanup_error,
        'coordination_additions': added, 'coordination_only': allowed,
        'working_existing_bytes_unchanged': stable, 'primary_unchanged': primary_stable,
        'authoritative_copy_unchanged': evidence_stable,
        'passed': bool(allowed and stable and primary_stable and evidence_stable and cleanup_error is None)}

def verify_cgroup(config):
    lines = Path('/proc/self/cgroup').read_text().splitlines()
    entry = next((line.split(':', 2)[2] for line in lines if line.startswith('0::')), None)
    if entry is None: raise PhaseError('cgroup v2 membership unavailable')
    root = (Path('/sys/fs/cgroup') / entry.lstrip('/')).resolve()
    if not root.is_relative_to(Path('/sys/fs/cgroup')): raise PhaseError('invalid cgroup path')
    memory = (root / 'memory.max').read_text().strip()
    swap = (root / 'memory.swap.max').read_text().strip()
    tasks = (root / 'pids.max').read_text().strip()
    quota, period = (root / 'cpu.max').read_text().split()
    if memory == 'max' or int(memory) > config['memory_bytes']: raise PhaseError('memory cap missing')
    if swap != '0': raise PhaseError('swap cap missing')
    if tasks == 'max' or int(tasks) > config['tasks_max']: raise PhaseError('task cap missing')
    if quota == 'max' or int(quota) * 100 > config['cpu_percent'] * int(period):
        raise PhaseError('CPU cap missing')
    return {'path': entry, 'memory_max': memory, 'memory_swap_max': swap,
            'pids_max': tasks, 'cpu_max': quota + ' ' + period}

def phase(config):
    phase_name = config['phase']
    if phase_name not in ('seed', 'reopen'): raise PhaseError('unknown phase')
    out = Path(config['output']).resolve()
    if not (out / 'm5-disposable-marker').is_file(): raise PhaseError('missing disposable marker')
    fixture = out / 'fixture-source'
    custody_root = out / 'custody'
    endpoint = custody_root / 'app.sock'
    run = out / phase_name
    run.mkdir(exist_ok=False)
    if phase_name == 'reopen':
        seed_result = out / 'seed' / 'op_Sum.structured-research.json'
        if seed_result.is_symlink() or not seed_result.is_file():
            raise PhaseError('saved seed result observation missing')
        shutil.copy2(seed_result, run / 'seed_sum.structured-research.json')
        if seed_result.read_bytes() != (run / 'seed_sum.structured-research.json').read_bytes():
            raise PhaseError('saved seed result observation changed while copying')
    control = out / 'control'; control.mkdir(exist_ok=True)
    subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True, timeout=5)
    dropped = drop_setup_capabilities()
    net = interfaces(); net['setup_capabilities'] = dropped
    net['cgroup'] = verify_cgroup(config); write(run / 'namespace.json', net)
    # Supplied by the parent launcher, constructed from an explicit allowlist.
    env = dict(config['environment'])
    if 'HOME' in env or 'CODEX_HOME' in env: raise PhaseError('home environment must be absent')
    env.update({'XDG_DATA_HOME': str(out / 'data'),
                'XDG_CONFIG_HOME': str(out / 'config'), 'XDG_CACHE_HOME': str(out / 'cache'),
                'XDG_RUNTIME_DIR': str(out / 'runtime'), 'MERE_ROOT': str(out / 'mere'),
                'TURNSTONE_ROOT': str(out / 'profile'), 'TURNSTONE_KNOT_ROOT': str(out / 'profile/notes'),
                'GRAPHSHELL_APP_ENDPOINT': str(endpoint), 'TURNSTONE_CAPTURE_DIR': str(run),
                'TURNSTONE_SCENARIO': config['scenario'], 'TURNSTONE_OFFLINE': '1' if phase_name == 'reopen' or config.get('preflight') is True else '0',
                'M5_SELECTION_PACKAGE': '/m5/run/selected.mereclip'})
    children, receipts = [], {}
    try:
        if phase_name == 'seed':
            if not fixture.is_dir(): raise PhaseError('fixture source copy missing')
            ready = run / 'origin-ready.json'
            origin = Child([sys.executable, config['fixture_server'], '--ready', str(ready), '--log',
                            str(out / 'fixture-requests.json'), '--source-dir', str(fixture)], run / 'origin.log', env)
            children.append(origin)
            origin_info = wait_ready(origin.proc, lambda: json_for_pid(ready, origin.proc.pid), 15)
            env['M5_ORIGIN'] = 'http://127.0.0.1:' + str(origin_info['port'])
            write(out / 'origin-info.json', origin_info)
        else:
            if fixture.exists(): raise PhaseError('fixture source remains in reopen environment')
            origin_info = json.loads((out / 'origin-info.json').read_text())
            if not refused('127.0.0.1', origin_info['port']): raise PhaseError('origin reachable on reopen')
            env['M5_ORIGIN'] = 'http://127.0.0.1:' + str(origin_info['port'])
        if phase_name == 'seed':
            if custody_root.exists(): raise PhaseError('fixture custody root already exists')
            custody_root.mkdir(mode=0o700)
            (custody_root / 'fixture-marker').write_text('eidetic-retention-20261019\n')
        else:
            if (custody_root / 'fixture-marker').read_text() != 'eidetic-retention-20261019\n':
                raise PhaseError('fixture custody marker mismatch')
            if endpoint.exists(): raise PhaseError('stale endpoint was not cleaned after seed')
        custody_env = dict(env)
        custody_env.update({'EIDETIC_CUSTODY_FIXTURE_ROOT': str(custody_root),
                            'EIDETIC_CUSTODY_FIXTURE_ENDPOINT': str(endpoint)})
        custodian = Child([config['custodian'], 'retained_custody_receipt_host', '--ignored', '--exact', '--nocapture'],
                          run / 'custody.log', custody_env)
        children.append(custodian)
        custody_info = wait_ready(custodian.proc, lambda: fixture_ready(endpoint, custodian.proc.pid), 30)
        probe_env = dict(custody_env)
        probe_env['TURNSTONE_M5_ROUTE_RECEIPT'] = str(run / 'inference-route.json')
        probe = Child([config['test_binary'], 'inference_service::tests::model_free_fixture_refuses_inference_route',
                       '--ignored', '--exact', '--nocapture'], run / 'route-probe.log', probe_env)
        children.append(probe)
        try: probe.proc.wait(timeout=30)
        except subprocess.TimeoutExpired: raise PhaseError('route probe timed out')
        receipts['route_probe'] = probe.result(); probe.log.close(); children.remove(probe)
        if probe.proc.returncode != 0 or not (run / 'inference-route.json').is_file():
            raise PhaseError('custody-only route probe failed')
        if custodian.proc.poll() is not None: raise PhaseError('custodian exited during route probe')
        custody_after = fixture_ready(endpoint, custodian.proc.pid)
        if custody_after != custody_info: raise PhaseError('custody peer changed during route probe')
        receipts['custody_peer_after_probe'] = custody_after
        app = Child([config['application']], run / 'app.log', env)
        children.append(app)
        deadline = time.monotonic() + config['timeout_seconds']
        origin_stopped = False
        while app.proc.poll() is None:
            if phase_name == 'seed' and not origin_stopped and (control / 'stop-origin').exists():
                receipts['origin'] = origin.stop(); children.remove(origin)
                if not refused('127.0.0.1', origin_info['port']): raise PhaseError('origin still reachable')
                shutil.copy2(out / 'fixture-requests.json', run / 'requests-at-stop.json')
                (control / 'origin-stopped').write_text('origin stopped and refused\n')
                origin_stopped = True
            if custodian.proc.poll() is not None: raise PhaseError('custodian exited while app active')
            if time.monotonic() >= deadline: raise PhaseError('app deadline exceeded')
            time.sleep(.1)
        receipts['app'] = app.result(); app.log.close(); children.remove(app)
        if app.proc.returncode != 0: raise PhaseError('app failed')
        done = (run / 'scenario.done').read_text()
        if not done.startswith('RESULT ok\n'): raise PhaseError('scenario failed')
        if phase_name == 'seed':
            if not origin_stopped:
                receipts['origin'] = origin.stop(); children.remove(origin)
                if not refused('127.0.0.1', origin_info['port']): raise PhaseError('origin still reachable')
                shutil.copy2(out / 'fixture-requests.json', run / 'requests-at-stop.json')
            # Only the explicitly named, freshly copied fixture directory is removed.
            if fixture.parent != out or fixture.name != 'fixture-source': raise PhaseError('unsafe fixture source')
            shutil.rmtree(fixture)
        if phase_name == 'reopen':
            profile = out / 'profile'
            copy = out / 'inspection-copy'
            before = tree_hashes(profile)
            shutil.copytree(profile, copy, symlinks=False)
            if tree_hashes(copy) != before or tree_hashes(profile) != before:
                raise PhaseError('inspection copy differs or source changed')
            (copy / 'm5-inspection-copy-marker').write_text('m5-isolated-copy/v1\n')
            write(run / 'inspection-source-hashes.json', before)
            inspect_env = dict(env)
            inspect_env.update({'TURNSTONE_M5_INSPECTION_COPY': str(copy),
                'TURNSTONE_M5_FIXTURE_ROOT': str(custody_root),
                'TURNSTONE_M5_SOURCE_OBSERVATION': config['source_observation'],
                'TURNSTONE_M5_SOURCE_PROFILE': '/m5/run/profile',
                'TURNSTONE_M5_SELECTED_PACKAGE': '/m5/run/selected.mereclip',
                'TURNSTONE_M5_SELECTED_QUOTE': 'only this linked finding',
                'TURNSTONE_M5_NOTE_TEXT': 'M5 authored Redshank note',
                'TURNSTONE_M5_STRUCTURED_OBSERVATION': config['inspection_observation'],
                'TURNSTONE_M5_INSPECTION_OUTPUT': str(run / 'structured-inspection.json')})
            authoritative_before = tree_hashes(copy)
            for key, observation, seed_observation, output in (
                ('inspection', config['inspection_observation'], '/m5/run/seed/op_Sum.structured-research.json', 'structured-inspection.json'),
                ('withdrawal_inspection', '/m5/run/seed/withdraw_historical.structured-research.json', '/m5/run/seed/withdraw_before.structured-research.json', 'withdrawal-inspection.json'),
            ):
                working = out / (key + '-working-copy')
                shutil.copytree(copy, working, symlinks=False)
                working_before = tree_hashes(working)
                if working_before != authoritative_before:
                    raise PhaseError('working copy differs before ' + key)
                inspect_env['TURNSTONE_M5_INSPECTION_COPY'] = str(working)
                inspect_env['TURNSTONE_M5_STRUCTURED_OBSERVATION'] = observation
                inspect_env['TURNSTONE_M5_SEED_OBSERVATION'] = seed_observation
                inspect_env['TURNSTONE_M5_INSPECTION_OUTPUT'] = str(run / output)
                reader = None
                try:
                    reader = Child([config['test_binary'], 'trail_memory::research_owner_tests::model_free_inspect_exited_structured_copy',
                        '--ignored', '--exact', '--nocapture'], run / (key + '.log'), inspect_env)
                    children.append(reader)
                    try: reader.proc.wait(timeout=config['inspection_timeout_seconds'])
                    except subprocess.TimeoutExpired: raise PhaseError(key + ' deadline exceeded')
                    receipts[key] = reader.result()
                    if reader.proc.returncode != 0: raise PhaseError(key + ' failed')
                finally:
                    cleanup_error = None
                    if reader is not None:
                        try:
                            if reader.proc.poll() is None: receipts[key] = reader.stop()
                            else: reader.log.close()
                            children.remove(reader)
                        except BaseException as exc:
                            cleanup_error = repr(exc)
                    # Record all three integrity attempts even if reader cleanup
                    # or one tree walk fails. A failed walk cannot qualify.
                    integrity = inspection_integrity(profile, before, copy,
                        authoritative_before, working, working_before, cleanup_error)
                    write(run / (key + '-integrity.json'), integrity)
                    if not integrity['passed']:
                        raise PhaseError('inspection integrity not established during ' + key)
        receipts['custody'] = custodian.stop(); children.remove(custodian)
        if receipts['custody']['exit_code'] != -signal.SIGTERM: raise PhaseError('unexpected custody exit')
        receipts['custody']['stop_requested'] = 'SIGTERM'
        remove_own_socket(custody_root, endpoint, custodian)
        receipts['custody_ready'] = custody_info
        write(run / 'processes.json', receipts)
    except BaseException as exc:
        receipts['failure'] = repr(exc)
        raise
    finally:
        for child in reversed(children):
            try: receipts.setdefault('cleanup', []).append(child.stop())
            except BaseException as exc: receipts.setdefault('cleanup_errors', []).append(repr(exc))
        write(run / 'processes-final.json', receipts)

if __name__ == '__main__':
    # Staged only: prevent direct execution before the containment review.
    raise SystemExit('DRAFT ONLY: namespace supervisor requires integrated cgroup and mount review')
