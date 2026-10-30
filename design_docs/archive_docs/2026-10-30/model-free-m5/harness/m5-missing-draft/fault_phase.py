#!/usr/bin/env python3
"""Dedicated fault phase; caller must provide the reviewed mount namespace."""
import json, os, signal, subprocess, sys
from pathlib import Path
import namespace_phase as phase
import acceptance_phase
from verify_missing import target_address, verify_all_scope, verify_click_interval

OUT = Path('/m5/output')
COPY = Path('/m5/fault-copy')
CUSTODY = Path('/m5/custody')
ENDPOINT = CUSTODY / 'app.sock'

def read(path):
    if path.is_symlink() or not path.is_file(): raise ValueError('required regular evidence absent: ' + str(path))
    return json.loads(path.read_text())

def graph_inventory(session):
    tree = phase.tree_hashes(COPY / 'sessions' / session)
    # The fault intentionally changes the sealed owner store, which is a
    # sibling of memory/ rather than its child. Keep every other session byte.
    excluded = 'memory.sealed-sources-v1/'
    return {key:value for key,value in tree.items() if not key.startswith(excluded)}

def validate_fault(fault, inp):
    if fault['schema'] != 'turnstone.m5-missing-fault/v1' or fault['run_id'] != inp['run_id']:
        raise ValueError('fault receipt identity differs')
    for key in ('a1_target','a2_target'):
        if fault[key] != inp[key]: raise ValueError('fault target differs')
    for n in ('a1','a2'):
        if fault[n+'_raw_manifest'] != inp[n+'_content']['raw_manifest']: raise ValueError('raw identity differs')
    if fault['a1_raw_manifest'] == fault['a2_raw_manifest']: raise ValueError('shared raw bytes')
    if not all(fault[k] is True for k in ('manifest_preserved','capture_receipts_preserved',
                 'a1_raw_absent_after_reopen','a1_owner_refused','a2_owner_unchanged')):
        raise ValueError('owner proof incomplete')
    for key in ('deleted_ciphertext_hash','capture_receipts_hash'):
        value = fault[key]
        if not isinstance(value,str) or not value.startswith('blake3:') or len(value) != 71 or any(x not in '0123456789abcdef' for x in value[7:]):
            raise ValueError('invalid owner digest')
    if fault['capture_receipts_hash'] != 'blake3:' + inp['sealed_capture_receipts']['blake3']:
        raise ValueError('injector receipts differ from intact inspection baseline')

def validate_gui(inp, all_scope, before_click, before, after, runtime):
    def reader(obs, target):
        found = [r for r in obs['readers'] if r['target'] == target]
        if len(found) != 1: raise ValueError('exact open reader missing or ambiguous')
        return found[0]
    a, b = reader(before, inp['a1_target']), reader(after, inp['a2_target'])
    verify_all_scope(all_scope, inp['a1_target'], before_click)
    verify_click_interval(before_click, before, inp['a1_target'])
    if a['phase'] != 'Error' or a['content'] is not None or not a['error']:
        raise ValueError('exact A1 open did not refuse')
    if b['phase'] != 'Ready' or b['content'] != inp['a2_content']:
        raise ValueError('A2 did not reopen unchanged')
    if before['session'] != after['session'] or before['session'] != runtime['session']:
        raise ValueError('GUI session changed')
    if runtime['schema'] != 'turnstone.model-free-runtime/v1' or runtime['network_offline'] is not True or runtime['offline_acquisition_attempts'] != 0:
        raise ValueError('network activity or runtime evidence missing')

def run(config):
    if set(config) != {'memory_bytes','tasks_max','cpu_percent','devices','run_id','principal','origin_port','timeout_seconds'}:
        raise ValueError('unexpected config fields')
    if (config['memory_bytes'],config['tasks_max'],config['cpu_percent'],config['timeout_seconds']) != (3*1024**3,128,150,300):
        raise ValueError('fault limits differ')
    inp = read(Path('/m5/input/missing-input.json'))
    if inp['run_id'] != config['run_id']: raise ValueError('run identity mismatch')
    if (COPY / 'm5-fault-copy-marker').read_text() != 'turnstone-m5-fault-copy/v1\n' + config['run_id'] + '\n':
        raise ValueError('fault copy marker differs')
    acceptance_phase.validate_devices(config)
    subprocess.run(['ip','link','set','lo','up'], check=True, timeout=5)
    dropped = phase.drop_setup_capabilities()
    net = phase.interfaces(); net['setup_capabilities'] = dropped; net['cgroup'] = phase.verify_cgroup(config)
    if not phase.refused('127.0.0.1', config['origin_port']): raise ValueError('origin reachable')
    phase.write(OUT / 'namespace.json', net)
    if (CUSTODY / 'fixture-marker').read_text() != 'eidetic-retention-20261019\n' or ENDPOINT.exists():
        raise ValueError('fixture marker or endpoint invalid')
    env = {'PATH':'/usr/bin','WAYLAND_DISPLAY':'wayland-0','RUST_LOG':'warn,wgpu_core::instance=debug',
           'XDG_RUNTIME_DIR':'/m5/run/runtime','XDG_DATA_HOME':'/m5/run/data','XDG_CONFIG_HOME':'/m5/run/config',
           'XDG_CACHE_HOME':'/m5/run/cache','MERE_ROOT':'/m5/run/mere','TURNSTONE_ROOT':'/m5/run/profile',
           'GRAPHSHELL_APP_ENDPOINT':str(ENDPOINT),
           'TURNSTONE_CAPTURE_DIR':str(OUT),'TURNSTONE_SCENARIO':'/m5/input/missing.scn','TURNSTONE_OFFLINE':'1',
           'TURNSTONE_M5_EXPECTED_A1_URI':target_address(inp['a1_target']),
           'EIDETIC_CUSTODY_FIXTURE_ROOT':str(CUSTODY),'EIDETIC_CUSTODY_FIXTURE_ENDPOINT':str(ENDPOINT)}
    children, results, custodian, peer = [], {}, None, None
    try:
        custodian = phase.Child(['/m5/input/custodian','retained_custody_receipt_host','--ignored','--exact','--nocapture'], OUT / 'custody.log', env)
        children.append(custodian)
        peer = phase.wait_ready(custodian.proc, lambda: phase.fixture_ready(ENDPOINT, custodian.proc.pid),30)
        probe_env = dict(env, TURNSTONE_M5_ROUTE_RECEIPT=str(OUT / 'route.json'))
        probe = phase.Child(['/m5/input/test-binary','inference_service::tests::model_free_fixture_refuses_inference_route',
                             '--ignored','--exact','--nocapture'], OUT / 'route.log', probe_env)
        children.append(probe); probe.proc.wait(timeout=30)
        results['route_probe'] = probe.result(); probe.log.close(); children.remove(probe)
        route = read(OUT / 'route.json')
        principal = {'profile':route.get('profile'),'public_key':route.get('public_key')}
        if probe.proc.returncode != 0 or route.get('outcome') != 'closed-during-open' or principal != config['principal']:
            raise ValueError('fixture principal or inference route differs')
        if phase.fixture_ready(ENDPOINT, custodian.proc.pid) != peer: raise ValueError('custody peer changed')
        session = inp['a1_admission']['key']['session']
        before = graph_inventory(session)
        fault = phase.Child(['/m5/input/test-binary','trail_memory::research_owner_tests::model_free_inject_missing_capture_copy',
                             '--ignored','--exact','--nocapture'],OUT / 'fault.log',env)
        children.append(fault); fault.proc.wait(timeout=90)
        results['fault'] = fault.result(); fault.log.close(); children.remove(fault)
        if fault.proc.returncode != 0 or 'test result: ok. 1 passed;' not in (OUT / 'fault.log').read_text():
            raise ValueError('fault helper did not pass exactly one test')
        fault_receipt = read(OUT / 'fault.json')
        validate_fault(fault_receipt, inp)
        post = phase.Child(['/m5/input/test-binary','trail_memory::research_owner_tests::model_free_verify_missing_capture_copy',
                            '--ignored','--exact','--nocapture'],OUT / 'fault-postcheck.log',env)
        children.append(post); post.proc.wait(timeout=90)
        results['postcheck'] = post.result(); post.log.close(); children.remove(post)
        if post.proc.returncode != 0 or 'test result: ok. 1 passed;' not in (OUT / 'fault-postcheck.log').read_text():
            raise ValueError('independent owner postcheck did not pass exactly one test')
        check = read(OUT / 'fault-postcheck.json')
        if check['schema'] != 'turnstone.m5-missing-postcheck/v1' or check['run_id'] != inp['run_id'] or check['capture_receipts_hash'] != fault_receipt['capture_receipts_hash']:
            raise ValueError('postcheck receipt identity differs')
        if not all(check[k] is True for k in ('capture_receipts_preserved','a1_raw_absent','a1_owner_refused','a2_owner_unchanged')):
            raise ValueError('independent owner postcheck incomplete')
        after = graph_inventory(session)
        phase.write(OUT / 'graph-integrity.json', {'before':before,'after':after,'unchanged':before == after})
        if before != after: raise ValueError('graph changed during fault')
        if custodian.proc.poll() is not None: raise ValueError('custody exited before GUI')
        app = phase.Child(['/m5/input/turnstone'], OUT / 'app.log', env)
        children.append(app); app.proc.wait(timeout=config['timeout_seconds'])
        results['app'] = app.result(); app.log.close(); children.remove(app)
        if app.proc.returncode != 0 or not (OUT / 'scenario.done').read_text().startswith('RESULT ok\n'):
            raise ValueError('fault GUI scenario failed')
        if custodian.proc.poll() is not None or phase.fixture_ready(ENDPOINT, custodian.proc.pid) != peer:
            raise ValueError('custody ended or changed during GUI')
        runtime = read(OUT / 'missing_runtime.runtime.json')
        if runtime['pid'] != results['app']['pid'] or runtime['custody_principal'] != principal:
            raise ValueError('fault app runtime identity differs')
        validate_gui(inp, read(OUT / 'missing_all_scope.saved-sources.json'), read(OUT / 'missing_before_click.saved-sources.json'), read(OUT / 'missing_A1_open.saved-sources.json'),
                     read(OUT / 'missing_A2_open.saved-sources.json'), runtime)
        frames = {}
        for name in ('missing_before_click','missing_A1_peek','missing_A1_refusal','missing_A2_ready'):
            frames[name] = phase.validate_png(OUT / (name + '.png'))
        phase.write(OUT / 'phase-checked.json', {'schema':'turnstone.m5-missing-phase/v1',
            'frames':frames,'principal':principal,'status':'collection-only-not-headed-approval'})
        results['custody'] = custodian.stop(); children.remove(custodian)
        if results['custody']['exit_code'] != -signal.SIGTERM: raise ValueError('unexpected custody exit')
        if ENDPOINT.exists():
            st = ENDPOINT.lstat()
            if (st.st_dev, st.st_ino) != (peer['device'], peer['inode']):
                raise ValueError('custody socket replaced; cleanup withheld')
        phase.remove_own_socket(CUSTODY, ENDPOINT, custodian)
        results['custody_ready'] = peer
        phase.write(OUT / 'processes.json', results)
    except BaseException as exc:
        results['failure'] = repr(exc)
        raise
    finally:
        for child in reversed(children):
            try: results.setdefault('cleanup', []).append(child.stop())
            except BaseException as exc: results.setdefault('cleanup_errors', []).append(repr(exc))
        if custodian is not None and peer is not None and custodian.proc.poll() is not None:
            try:
                if ENDPOINT.exists():
                    st = ENDPOINT.lstat()
                    if (st.st_dev, st.st_ino) != (peer['device'], peer['inode']):
                        raise ValueError('custody socket replaced; cleanup withheld')
                    phase.remove_own_socket(CUSTODY, ENDPOINT, custodian)
                results['socket_cleanup'] = 'owned socket absent'
            except BaseException as exc: results.setdefault('cleanup_errors', []).append(repr(exc))
        phase.write(OUT / 'processes-final.json', results)

def main():
    if sys.argv != ['/m5/input/fault_phase.py','/m5/input/config.json']: raise SystemExit('invalid entry')
    run(read(Path('/m5/input/config.json')))

if __name__ == '__main__': main()
