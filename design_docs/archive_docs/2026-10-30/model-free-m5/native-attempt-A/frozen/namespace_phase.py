#!/usr/bin/env python3
"""Staged phase supervisor; run only inside the reviewed isolated cgroup/namespace.

Not an independent isolation boundary. The parent launcher must verify mount,
network and cgroup limits before invoking this module. Source bodies enter only
through files copied into the private fixture directory.
"""
import hashlib, json, os, shutil, signal, socket, stat, struct, subprocess, sys, time
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

def tree_hashes(root):
    result = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink(): raise PhaseError('symlink in inspection source tree')
        if path.is_file():
            h = hashlib.sha256()
            with path.open('rb') as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b''): h.update(chunk)
            result[str(path.relative_to(root))] = h.hexdigest()
    return result

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
    net = interfaces(); net['cgroup'] = verify_cgroup(config); write(run / 'namespace.json', net)
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
            reader = Child([config['test_binary'], 'trail_memory::research_owner_tests::model_free_inspect_exited_structured_copy',
                '--ignored', '--exact', '--nocapture'], run / 'inspection.log', inspect_env)
            children.append(reader)
            try: reader.proc.wait(timeout=config['inspection_timeout_seconds'])
            except subprocess.TimeoutExpired: raise PhaseError('inspection deadline exceeded')
            receipts['inspection'] = reader.result(); reader.log.close(); children.remove(reader)
            if reader.proc.returncode != 0: raise PhaseError('independent inspection failed')
            if tree_hashes(profile) != before: raise PhaseError('original profile changed during inspection')
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
