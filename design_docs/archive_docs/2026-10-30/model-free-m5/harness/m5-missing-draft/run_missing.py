#!/usr/bin/env python3
"""Collect the reviewed missing-source case on an isolated disposable copy."""
import argparse, json, os, re, shutil, stat, time, uuid
from pathlib import Path
import run_native as native
import namespace_phase as phase
import run_acceptance as acceptance
from verify_missing import verify_missing

READY_FOR_FAULT_COLLECTION = True
LIMITS = {'memory_bytes': 3 * 1024**3, 'tasks_max': 128, 'cpu_percent': 150}
TREES = ('profile', 'mere', 'data', 'config', 'cache')
FROZEN = ('turnstone', 'custodian', 'test-binary')
SOURCE_PATHS = ('src/trail_memory/fault_helper.rs','src/trail_memory/m5_inspection.rs',
                'src/trail_memory/research_owner_tests.rs','src/shell/citation_event_history.rs',
                'src/shell/mod.rs','src/shell/shell.rs','src/shell/shell_new.rs',
                'src/shell/effects/shell_refresh_diagnostic_inspection.rs',
                'src/shell/drive/shell_scenario_assert_no_surface.rs',
                'src/research_scenario_privacy.rs','src/app/saved_source_arms.rs')

def load(path):
    if path.is_symlink() or not path.is_file(): raise ValueError('missing regular evidence: ' + str(path))
    return json.loads(path.read_text())

def regular_tree(path):
    tree = phase.tree_hashes(path)
    for entry in path.rglob('*'):
        st = entry.lstat()
        if stat.S_ISREG(st.st_mode) and st.st_nlink != 1:
            raise phase.TreeReadError(str(entry.relative_to(path)), 'hard-linked file', tree)
    return tree

def collection_tree(root):
    """Inventory every original entry, including only recognized stale fixture sockets."""
    if root.is_symlink() or not root.is_dir(): raise ValueError('collection root is not plain')
    values = {}
    for path in sorted(root.rglob('*')):
        rel = path.relative_to(root); mode = path.lstat(); name = str(rel)
        if stat.S_ISDIR(mode.st_mode): values[name + '/'] = 'directory'
        elif stat.S_ISREG(mode.st_mode):
            if mode.st_nlink != 1: raise ValueError('hard-linked collection file: ' + name)
            values[name] = native.sha(path)
        elif stat.S_ISSOCK(mode.st_mode):
            parts = rel.parts
            if len(parts) != 5 or parts[:3] != ('sandbox','custody','custodian') or parts[4] != 'custody.sock' or not parts[3].startswith('run-'):
                raise ValueError('unexpected collection socket: ' + name)
            try: valid = str(uuid.UUID(parts[3][4:])) == parts[3][4:]
            except ValueError: valid = False
            if not valid: raise ValueError('invalid custody runtime socket: ' + name)
            values[name] = {'socket': {'device':mode.st_dev, 'inode':mode.st_ino, 'mode':mode.st_mode}}
        else: raise ValueError('nonregular collection entry: ' + name)
    return values

def copy_custody(source, dest):
    """Copy persistent synthetic-wallet state; preserve original runtime sockets in place."""
    if source.is_symlink() or not source.is_dir() or dest.exists(): raise ValueError('invalid custody copy')
    if {p.name for p in source.iterdir()} != {'fixture-marker','fixture.lock','custodian'}:
        raise ValueError('unexpected custody root entries')
    state = source / 'custodian'
    if state.is_symlink() or not state.is_dir(): raise ValueError('invalid custodian state')
    sockets = {}
    for item in state.iterdir():
        if item.name in ('data','vault','initialized','client.json'): continue
        if item.is_symlink() or not item.is_dir() or not item.name.startswith('run-'):
            raise ValueError('unexpected custodian runtime entry')
        try: valid = str(uuid.UUID(item.name[4:])) == item.name[4:]
        except ValueError: valid = False
        entries = list(item.iterdir())
        if not valid or len(entries) != 1 or entries[0].name != 'custody.sock':
            raise ValueError('invalid custodian runtime directory')
        st = entries[0].lstat()
        if not stat.S_ISSOCK(st.st_mode): raise ValueError('invalid custodian runtime socket')
        sockets[item.name + '/custody.sock'] = {'device':st.st_dev,'inode':st.st_ino,'mode':st.st_mode}
    dest.mkdir(mode=0o700); (dest / 'custodian').mkdir(mode=0o700)
    trees = {name:copy_tree(state / name, dest / 'custodian' / name) for name in ('data','vault')}
    files = {}
    for name in ('fixture-marker','fixture.lock','custodian/initialized','custodian/client.json'):
        src, target = source / name, dest / name
        st = src.lstat()
        if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1: raise ValueError('invalid custody state file: ' + name)
        files[name] = native.freeze_file(src, target)
        if (src.stat().st_dev,src.stat().st_ino) == (target.stat().st_dev,target.stat().st_ino):
            raise ValueError('custody copy shares inode')
    return {'trees':trees,'files':files,'runtime_sockets':sockets}

def copy_tree(source, dest):
    before = regular_tree(source)
    shutil.copytree(source, dest, symlinks=False, copy_function=shutil.copy2)
    if regular_tree(source) != before or regular_tree(dest) != before:
        raise ValueError('copy differs or source changed')
    for src in source.rglob('*'):
        if src.is_file():
            dst = dest / src.relative_to(source)
            a, b = src.stat(), dst.stat()
            if (a.st_dev, a.st_ino) == (b.st_dev, b.st_ino): raise ValueError('copy shares inode')
    return before

def ready_reader(observation, marker):
    readers = [r for r in observation['readers'] if r['phase'] == 'Ready'
               and r['content'] is not None and marker in r['content']['text']]
    if len(readers) != 1: raise ValueError('intact reader absent or ambiguous: ' + marker)
    return readers[0]

def bind_input(run_id, citation, old, current, inspection):
    a, b = ready_reader(old, 'M5 A1 archive'), ready_reader(current, 'M5 A2 archive')
    if inspection.get('schema') != 'turnstone.m5-store-inspection/v1' or inspection.get('session') != old['session']:
        raise ValueError('independent inspection identity differs')
    graph = inspection.get('graph')
    if not graph or a['target']['key']['graph'] != graph or b['target']['key']['graph'] != graph:
        raise ValueError('inspection graph differs')
    baseline = inspection['sealed_capture_receipts']
    digest = baseline.get('blake3')
    if set(baseline) != {'schema','count','blake3'} or baseline['schema'] != 'turnstone.m5-sealed-capture-receipts/v1' or type(baseline['count']) is not int or baseline['count'] < 1 or not isinstance(digest,str) or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
        raise ValueError('invalid independent capture receipt baseline')
    if ready_reader(citation, 'M5 A1 archive')['target'] != a['target']:
        raise ValueError('Knot citation differs from intact A1 target')
    if any(o['session'] != old['session'] for o in (citation, current)):
        raise ValueError('observation sessions differ')
    def owner(reader):
        matches = [item for item in inspection['captures'] if item['admission']['key'] == reader['target']['key']
                   and item['content'] == reader['content']]
        if len(matches) != 1: raise ValueError('independent owner match absent or ambiguous')
        if reader['target']['text_hash'] != reader['content']['text_hash']: raise ValueError('target hash differs')
        return matches[0]['admission']
    aa, bb = owner(a), owner(b)
    if a['target']['selector'] is None or aa['key']['annotation_manifest'] == bb['key']['annotation_manifest']:
        raise ValueError('A1 selector or distinct version missing')
    if a['content']['raw_manifest'] == b['content']['raw_manifest']:
        raise ValueError('A2 shares raw payload')
    all_admissions = old['admissions']
    if all_admissions != current['admissions'] or aa not in citation['admissions']:
        raise ValueError('capture inventory changed across observations')
    if len(inspection['captures']) != len(all_admissions): raise ValueError('owner capture inventory incomplete')
    if any(item['key']['graph'] != graph or item['key']['session'] != old['session'] for item in all_admissions):
        raise ValueError('capture admission outside independently inspected graph')
    if sorted(json.dumps(x['admission'], sort_keys=True) for x in inspection['captures']) != sorted(json.dumps(x, sort_keys=True) for x in all_admissions):
        raise ValueError('owner inventory differs')
    return {'schema':'turnstone.m5-missing-input/v1', 'run_id':str(run_id),
            'a1_admission':aa, 'a1_target':a['target'], 'a1_content':a['content'],
            'a2_admission':bb, 'a2_target':b['target'], 'a2_content':b['content'], 'all_admissions':all_admissions,
            'sealed_capture_receipts':baseline}

def validate_collection_ledger(root, collection):
    ledger = collection.get('files')
    if not isinstance(ledger,dict) or not ledger: raise ValueError('completed artifact ledger missing')
    required = {'inputs.json','processes.json','sandbox/seed/06_note_citation.saved-sources.json',
                'sandbox/reopen/22_reopen_A1.saved-sources.json','sandbox/reopen/23_reopen_A2.saved-sources.json',
                'sandbox/reopen/structured-inspection.json','sandbox/reopen/withdrawal-inspection.json',
                'sandbox/reopen/inspection-source-hashes.json','sandbox/reopen/inspection-integrity.json',
                'sandbox/reopen/withdrawal_inspection-integrity.json','sandbox/origin-info.json',
                'seed-launcher.cleanup.json','reopen-launcher.cleanup.json',
                'seed-launcher.json','reopen-launcher.json','sandbox/seed/namespace.json',
                'sandbox/reopen/namespace.json'}
    # origin-info is an output outside phase dirs; the collector must include it.
    if not required <= set(ledger): raise ValueError('required collection artifact is unbound: ' + repr(sorted(required-set(ledger))))
    for rel, digest in ledger.items():
        if not isinstance(rel,str) or Path(rel).is_absolute() or any(p in ('', '.', '..') for p in Path(rel).parts):
            raise ValueError('unsafe ledger path')
        path = root / rel
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root) or native.sha(path) != digest:
            raise ValueError('collection ledger mismatch: ' + rel)
    expected_profile = {'root':'sandbox/profile','inventory':'sandbox/reopen/inspection-source-hashes.json'}
    if collection.get('primary_profile') != expected_profile or regular_tree(root / expected_profile['root']) != load(root / expected_profile['inventory']):
        raise ValueError('primary profile differs from completed inspection')
    return ledger

def collection_evidence(root):
    inputs, collection, proc = (load(root / name) for name in ('inputs.json', 'collection.json', 'processes.json'))
    if inputs.get('kind') != 'model-free-acceptance-collection' or collection.get('schema') != 'turnstone.m5-collection/v1' or collection.get('status') != 'collected-awaiting-semantic-and-headed-verification':
        raise ValueError('successful intact collection required')
    ledger = validate_collection_ledger(root, collection)
    scopes = {}
    for name in ('seed', 'reopen'):
        p = proc[name]
        if p['exit_code'] != 0 or not p['start_ns'] < p['exit_ns']: raise ValueError('intact app failed')
        check = root / 'sandbox' / name / 'phase-checked.json'
        if native.sha(check) != collection['phase_checks'][name]: raise ValueError('phase check hash changed')
        cleanup = load(root / (name + '-launcher.cleanup.json'))
        if not cleanup['normal'] or not cleanup['ownership_observed'] or not any(x.get('cgroup_empty_verified') is True for x in cleanup['cleanup']):
            raise ValueError('intact scope cleanup not verified: ' + name)
        launch=load(root/(name+'-launcher.json'))
        namespace=load(root/'sandbox'/name/'namespace.json')
        children=([proc['seed'],proc['custody_seed'],proc['origin']] if name=='seed' else
                  [proc['reopen'],proc['custody_reopen'],proc['inspection'],proc['withdrawal_inspection']])
        scopes[name]=(acceptance.validate_phase_scope(launch,cleanup,namespace,children),launch)
    if (scopes['seed'][0][0] == scopes['reopen'][0][0] or scopes['seed'][0][1] == scopes['reopen'][0][1]
        or not scopes['seed'][1]['exit_ns'] < scopes['reopen'][1]['start_ns']):
        raise ValueError('intact scopes are not distinct and ordered')
    if not proc['seed']['exit_ns'] < proc['reopen']['start_ns'] < proc['inspection']['start_ns'] or proc['inspection']['exit_code'] != 0:
        raise ValueError('intact process order or inspection missing')
    if not proc['reopen']['exit_ns'] < proc['inspection']['start_ns']:
        raise ValueError('intact restart not established')
    if not proc['custody_seed']['exit_ns'] < proc['custody_reopen']['start_ns'] or proc['custody_reopen']['public_identity'] != proc['custody_seed']['public_identity']:
        raise ValueError('intact custody identity/order differs')
    if not proc['inspection']['exit_ns'] < proc['custody_reopen']['exit_ns']:
        raise ValueError('custody was not released after inspection')
    if not proc['origin']['exit_ns'] < proc['reopen']['start_ns']: raise ValueError('origin remained active')
    for n in FROZEN:
        if native.sha(root / 'frozen' / n) != inputs['files'][n] or inputs['builds'][n]['binary_sha256'] != inputs['files'][n]:
            raise ValueError('frozen executable/build mismatch: ' + n)
    provenance = root / 'frozen/build-provenance.json'
    if native.sha(provenance) != inputs['provenance_sha256']: raise ValueError('build provenance changed')
    receipts = {n:root / 'frozen' / (n + '-build.json') for n in FROZEN}
    provenance_doc = load(provenance)
    inventories = {n:root / 'frozen' / (n + '-source-inventory.json') for n in FROZEN}
    builds = acceptance.verify_builds(provenance_doc, receipts,
        {n:inputs['files'][n] for n in FROZEN}, {n:inputs['builds'][n]['source_hash'] for n in FROZEN}, inventories)
    if builds != inputs['builds']:
        raise ValueError('build evidence changed')
    for n in FROZEN:
        path = root / 'frozen' / (n + '-source-inventory.json')
        rel = str(path.relative_to(root))
        if rel not in ledger or native.sha(path) != inputs['builds'][n]['source_hash'] or inputs['builds'][n].get('source_inventory') != rel or provenance_doc['artifacts'][n].get('source_inventory_sha256') != native.sha(path):
            raise ValueError('source inventory unbound or differs: ' + n)
    for name in ('inspection', 'withdrawal_inspection'):
        if not load(root / 'sandbox' / 'reopen' / (name + '-integrity.json'))['passed']:
            raise ValueError('inspection integrity failed: ' + name)
    return inputs, proc

def validate_fault_scope(root, out, output, observed, launch):
    fault_scope = acceptance.validate_phase_scope(launch, load(out/'launcher.cleanup.json'),
        load(output/'namespace.json'), [observed[n] for n in ('route_probe','fault','postcheck','app','custody')])
    for name in ('seed','reopen'):
        previous = load(root/(name+'-launcher.cleanup.json'))
        prior_launch = load(root/(name+'-launcher.json'))
        if fault_scope[0] == previous['scope'] or fault_scope[1] == previous['invocation'] or not prior_launch['exit_ns'] < launch['start_ns']:
            raise ValueError('fault scope not distinct and ordered')

def verify_source_registration(root, source_root, registration_lock):
    here = Path(__file__).resolve().parent
    inventories = [load(root / 'frozen' / (name + '-source-inventory.json')) for name in ('turnstone','test-binary')]
    locked = load(registration_lock)
    if locked.get('schema') != 'turnstone.m5-missing-registered-source/v1' or set(locked.get('files',{})) != set(SOURCE_PATHS):
        raise ValueError('reviewed source registration lock missing or incomplete')
    content = {}
    for name in SOURCE_PATHS:
        path = source_root / name
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(source_root):
            raise ValueError('registered source missing or unsafe: ' + name)
        digest = native.sha(path)
        if locked['files'][name] != digest:
            raise ValueError('registered source differs from reviewed lock: ' + name)
        if any(doc['files'].get(name) != {'sha256':digest} for doc in inventories):
            raise ValueError('registered source differs from build inventory: ' + name)
        content[name] = path.read_text()
    for source, draft in (('src/trail_memory/fault_helper.rs','fault_helper.rs'),('src/shell/citation_event_history.rs','citation_event_history.rs')):
        if native.sha(source_root / source) != native.sha(here / draft):
            raise ValueError('compiled source differs from staged draft: ' + source)
    markers = {
        'src/shell/citation_event_history.rs': (r'fn\s+event_address\s*\(',r'AppEvent::AddressOpened',r'AppEvent::WorkbenchTileOpened',r'turnstone\.m5-reader-open-events/v1'),
        'src/trail_memory/m5_inspection.rs': (r'mod\s+missing_fault\s*;',r'fn\s+inject_missing_capture_copy\s*\(',r'missing_fault::inject\s*\(',r'fn\s+verify_missing_capture_copy\s*\(',r'missing_fault::verify_preservation\s*\('),
        'src/trail_memory/research_owner_tests.rs': (r'fn\s+model_free_inject_missing_capture_copy\s*\(',r'm5_inspection::inject_missing_capture_copy\s*\(',r'fn\s+model_free_verify_missing_capture_copy\s*\(',r'm5_inspection::verify_missing_capture_copy\s*\('),
        'src/shell/mod.rs': (r'mod\s+citation_event_history\s*;',),
        'src/shell/shell.rs': (r'address_event_history\s*:',r'reader_event_history\s*:'),
        'src/shell/shell_new.rs': (r'address_event_history\s*:',r'reader_event_history\s*:'),
        'src/shell/effects/shell_refresh_diagnostic_inspection.rs': (r'event_address\s*\(',r'address_event_history\.record\s*\(',r'reader_event_history\.record\s*\(',r'capture_selected\s*\(',r'TURNSTONE_M5_EXPECTED_A1_URI'),
        'src/shell/drive/shell_scenario_assert_no_surface.rs': (r'"reader_open_events"\s*:',r'reader_event_history\.reader_observation\s*\(&selected',r'"address_events"\s*:',r'"reader_scope"\s*:',r'turnstone\.m5-reader-scope/v1',r'address_event_history\.observation\s*\(&selected',r'saved_source_admitted\s*\(key\)'),
    }
    for path, patterns in markers.items():
        if any(not re.search(pattern, content[path]) for pattern in patterns):
            raise ValueError('required integration registration absent: ' + path)

def namespace_command(copyroot, frozen, output, devices):
    gpu = Path(devices['render']['path']); info = devices['sysfs']
    cmd = ['bwrap','--die-with-parent','--new-session','--unshare-user','--uid','0','--gid','0',
           '--cap-add','CAP_NET_ADMIN','--unshare-net','--unshare-pid','--unshare-ipc',
           '--proc','/proc','--dev','/dev','--ro-bind','/usr','/usr',
           '--symlink','usr/lib','/lib','--symlink','usr/lib64','/lib64',
           '--dir','/m5','--dir','/m5/input','--dir','/m5/run','--dir','/m5/run/runtime',
           '--bind',str(copyroot / 'profile'),'/m5/fault-copy',
           '--bind',str(copyroot / 'profile'),'/m5/run/profile',
           '--bind',str(copyroot / 'custody'),'/m5/custody',
           '--bind',str(output),'/m5/output',
           '--bind',devices['wayland']['path'],'/m5/run/runtime/wayland-0']
    for name in ('mere','data','config','cache'):
        cmd += ['--bind',str(copyroot / name),'/m5/run/' + name]
    cmd += ['--dir','/dev/dri','--dev-bind',str(gpu),'/dev/dri/' + gpu.name,
            '--dir','/sys','--dir','/sys/dev','--dir','/sys/dev/char','--dir','/sys/devices']
    root = Path(info['root']); ancestor = Path('/sys/devices')
    if native.validate_render_sysfs_target(Path(info['target']), gpu.name) != root: raise ValueError('invalid sysfs root')
    for part in root.relative_to(ancestor).parts[:-1]:
        ancestor /= part; cmd += ['--dir',str(ancestor)]
    cmd += ['--ro-bind',str(root),str(root),'--symlink',os.path.relpath(info['target'],'/sys/dev/char'),
            '/sys/dev/char/' + info['devnum'],'--ro-bind','/sys/fs/cgroup','/sys/fs/cgroup']
    for name in (*FROZEN, 'missing.scn', 'missing-input.json', 'fault_phase.py', 'namespace_phase.py', 'acceptance_phase.py', 'verify_missing.py', 'config.json'):
        cmd += ['--ro-bind',str(frozen / name),'/m5/input/' + name]
    return cmd + ['--chdir','/m5/output','/usr/bin/python3','/m5/input/fault_phase.py','/m5/input/config.json']

def verify_bound_case(case, bound, ledger):
    """Receipt-side integration; the caller supplies the validated ledger reader."""
    verify_missing(case, bound, ledger)
    primary, graph, processes = (bound(case[name]) for name in ('primary_integrity','graph_integrity','processes'))
    if not primary['unchanged'] or primary['before'] != primary['after']:
        raise ValueError('primary tree changed')
    if not graph['unchanged'] or graph['before'] != graph['after']:
        raise ValueError('graph changed during owner fault')
    fault, post, app, custody = processes['fault'], processes['postcheck'], processes['app'], processes['custody']
    if fault['exit_code'] != 0 or post['exit_code'] != 0 or app['exit_code'] != 0 or not fault['exit_ns'] < post['start_ns'] < post['exit_ns'] < app['start_ns'] < app['exit_ns'] < custody['exit_ns']:
        raise ValueError('fault process order differs')
    original, check = bound(case['fault']), bound(case['postcheck'])
    if check['schema'] != 'turnstone.m5-missing-postcheck/v1' or check['run_id'] != case['run_id'] or check['capture_receipts_hash'] != original['capture_receipts_hash'] or not all(check[k] is True for k in ('capture_receipts_preserved','a1_raw_absent','a1_owner_refused','a2_owner_unchanged')):
        raise ValueError('independent owner postcheck differs')


FAULT_LEDGER_REQUIRED = (
    'inputs.json', 'primary-integrity.json', 'launcher.json', 'launcher.log', 'launcher.cleanup.json',
    *("frozen/" + name for name in ('registration-lock.json','missing-input.json','config.json',
        'missing.scn','fault_phase.py','namespace_phase.py','acceptance_phase.py','verify_missing.py',
        'run_missing.py','run_native.py','run_acceptance.py','fault_helper.rs','citation_event_history.rs','citation-observer.patch')),
    *("output/" + name for name in ('namespace.json','route.json','phase-checked.json','processes.json',
        'processes-final.json','fault.json','fault-postcheck.json','graph-integrity.json','scenario.done',
        'missing_runtime.runtime.json','missing_all_scope.saved-sources.json',
        'missing_before_click.saved-sources.json','missing_A1_open.saved-sources.json','missing_A2_open.saved-sources.json',
        'missing_before_click.png','missing_A1_peek.png','missing_A1_refusal.png','missing_A2_ready.png')),
)

def complete_collection(out, run_id):
    """Publish the artifact ledger only after cleanup and primary integrity."""
    if out.is_symlink() or not out.is_dir() or (out / 'collection.json').exists():
        raise ValueError('invalid or already completed fault output')
    ledger = {}
    def add(path):
        st = path.lstat()
        if not stat.S_ISREG(st.st_mode) or st.st_nlink != 1:
            raise ValueError('nonregular or linked fault artifact: ' + str(path))
        rel = str(path.relative_to(out))
        if rel in ledger: raise ValueError('duplicate fault artifact: ' + rel)
        ledger[rel] = native.sha(path)
    def walk(directory):
        if not stat.S_ISDIR(directory.lstat().st_mode):
            raise ValueError('linked or non-directory fault artifact parent: ' + str(directory))
        for path in sorted(directory.iterdir()):
            mode = path.lstat().st_mode
            if stat.S_ISDIR(mode): walk(path)
            elif path.parent == out / 'frozen' and path.name in FROZEN:
                if not stat.S_ISREG(mode) or path.stat().st_nlink != 1:
                    raise ValueError('invalid frozen executable')
            else: add(path)
    for name in ('inputs.json','primary-integrity.json','launcher.json','launcher.log','launcher.cleanup.json'):
        add(out / name)
    for name in ('frozen','output'): walk(out / name)
    if not set(FAULT_LEDGER_REQUIRED) <= set(ledger):
        raise ValueError('required fault artifact missing')
    expected_frozen = load(out / 'inputs.json').get('frozen')
    if not isinstance(expected_frozen, dict) or not all(name in expected_frozen for name in FROZEN) or regular_tree(out / 'frozen') != expected_frozen:
        raise ValueError('frozen inputs or executables differ from pre-launch inventory')
    primary = load(out / 'primary-integrity.json')
    cleanup = load(out / 'launcher.cleanup.json')
    final = load(out / 'output/processes-final.json')
    if primary.get('unchanged') is not True or primary.get('before') is None or primary['before'] != primary.get('after'):
        raise ValueError('primary integrity not established')
    if cleanup.get('normal') is not True or cleanup.get('ownership_observed') is not True or not any(item.get('cgroup_empty_verified') is True for item in cleanup.get('cleanup', [])):
        raise ValueError('owned scope cleanup not established')
    if final.get('cleanup_errors') or final.get('failure') or final.get('socket_cleanup') != 'owned socket absent':
        raise ValueError('fault process cleanup not established')
    # Fail if records changed while being checked; successful source runs are immutable.
    if any(native.sha(out / rel) != expected for rel, expected in ledger.items()):
        raise ValueError('fault artifact changed during completion')
    if regular_tree(out / 'frozen') != expected_frozen:
        raise ValueError('frozen inputs changed during completion')
    native.record(out / 'collection.json', {'schema':'turnstone.m5-missing-collection/v1',
        'status':'collected-awaiting-headed-and-semantic-verification','run_id':run_id,'files':ledger})

def finish_collection(out, run_id):
    try:
        complete_collection(out, run_id)
    except BaseException as exc:
        native.record(out / 'failure.json', {'error':repr(exc), 'time_ns':time.time_ns()})
        raise

def execute(args):
    root = args.collection.resolve(strict=True)
    out = args.output.parent.resolve(strict=True) / args.output.name
    if out.exists() or out.is_relative_to(root) or root.is_relative_to(out): raise ValueError('output must be fresh and separate')
    out.mkdir(mode=0o700, parents=True)
    before = None
    try:
        before = collection_tree(root)
        inputs, processes = collection_evidence(root)
        copyroot, frozen, output = out / 'copy', out / 'frozen', out / 'output'
        for d in (copyroot, frozen, output): d.mkdir(mode=0o700)
        source_root = args.source_root.resolve(strict=True)
        native.freeze_file(args.registration_lock, frozen / 'registration-lock.json')
        verify_source_registration(root, source_root, frozen / 'registration-lock.json')
        copied = {name:copy_tree(root / 'sandbox' / name, copyroot / name) for name in TREES}
        copied['custody'] = copy_custody(root / 'sandbox/custody', copyroot / 'custody')
        run_id = uuid.uuid4()
        (copyroot / 'profile' / 'm5-fault-copy-marker').write_text('turnstone-m5-fault-copy/v1\n' + str(run_id) + '\n')
        run = root / 'sandbox'
        data = bind_input(run_id, load(run / 'seed/06_note_citation.saved-sources.json'),
                          load(run / 'reopen/22_reopen_A1.saved-sources.json'),
                          load(run / 'reopen/23_reopen_A2.saved-sources.json'),
                          load(run / 'reopen/structured-inspection.json'))
        native.record(frozen / 'missing-input.json', data)
        here = Path(__file__).resolve().parent
        for name in FROZEN: native.freeze_file(root / 'frozen' / name, frozen / name)
        for name in ('missing.scn','fault_phase.py','namespace_phase.py','acceptance_phase.py','verify_missing.py',
                     'run_missing.py','run_native.py','run_acceptance.py','fault_helper.rs','citation_event_history.rs','citation-observer.patch'):
            native.freeze_file(here / name, frozen / name)
        devices = native.device_identity(args.wayland_socket, args.gpu_device)
        if devices != inputs['devices']: raise ValueError('display or render identity changed')
        config = {**LIMITS, 'devices':devices, 'run_id':str(run_id), 'principal':processes['custody_reopen']['public_identity'],
                  'origin_port':load(run / 'origin-info.json')['port'], 'timeout_seconds':300}
        native.record(frozen / 'config.json', config)
        native.record(out / 'inputs.json', {'run_id':str(run_id),'copied':copied,
            'collection_hashes':before, 'frozen':phase.tree_hashes(frozen), 'devices':devices})
        if collection_tree(root) != before: raise ValueError('primary changed before phase')
        if native.device_identity(args.wayland_socket, args.gpu_device) != devices: raise ValueError('device changed before launch')
        cmd = namespace_command(copyroot, frozen, output, devices)
        unit = 'm5-missing-' + uuid.uuid4().hex
        result = native.run_scope(native.cgroup_scope(cmd, LIMITS['memory_bytes'], LIMITS['tasks_max'], LIMITS['cpu_percent'], unit, 600),
                                  unit, out / 'launcher.log', 610)
        native.record(out / 'launcher.json', result)
        if native.device_identity(args.wayland_socket, args.gpu_device) != devices: raise ValueError('device changed during phase')
        verify_source_registration(root, source_root, frozen / 'registration-lock.json')
        observed = load(output / 'processes.json')
        final = load(output / 'processes-final.json')
        if final.get('cleanup_errors') or final.get('socket_cleanup') != 'owned socket absent' or final.get('failure'):
            raise ValueError('fault process cleanup was not established')
        if not processes['inspection']['exit_ns'] < observed['fault']['start_ns'] < observed['fault']['exit_ns'] < observed['postcheck']['start_ns'] < observed['postcheck']['exit_ns'] < observed['app']['start_ns']:
            raise ValueError('fault process sequence differs')
        validate_fault_scope(root, out, output, observed, result)
    except BaseException as exc:
        native.record(out / 'failure.json', {'error':repr(exc), 'time_ns':time.time_ns()})
        raise
    finally:
        try:
            after = collection_tree(root)
            valid = before is not None and after == before
            native.record(out / 'primary-integrity.json', {'before':before,'after':after,'unchanged':valid})
            if not valid: raise ValueError('primary collection changed or initial inventory absent')
        except BaseException as exc:
            evidence = {'error':repr(exc), 'before':before}
            if isinstance(exc, phase.TreeReadError): evidence.update(exc.evidence)
            native.record(out / 'primary-integrity-failure.json', evidence)
            raise

    finish_collection(out, str(run_id))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('collection','output','wayland-socket','gpu-device','source-root','registration-lock'): parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    if not READY_FOR_FAULT_COLLECTION: raise SystemExit('DRAFT ONLY: fault helper has not been integrated, compiled or reviewed')
    execute(args)

if __name__ == '__main__': main()
