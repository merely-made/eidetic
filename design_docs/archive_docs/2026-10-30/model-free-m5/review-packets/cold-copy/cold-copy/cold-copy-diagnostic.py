#!/usr/bin/env python3
"""Bounded cold UI diagnostic on a copy of failed Q; no acceptance claim."""
import json, re, shutil, stat, sys, time, uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
SDD = HERE.parent
sys.path.insert(0, str(SDD / 'acceptance-draft'))
import run_native as native
import namespace_phase as phase
import run_acceptance as acceptance

SOURCE = Path('/home/markik/Code/testing/m5-intact-collection-20261030-q')
OUT = Path('/home/markik/Code/testing/m5-cold-copy-diagnostic-20261030-b')
SCENARIO = HERE / 'cold-read-diagnostic.scn'
ROOTS = ('profile', 'mere', 'custody/custodian/data', 'custody/custodian/vault')
FILES = ('custody/fixture-marker', 'custody/fixture.lock', 'custody/custodian/initialized',
         'custody/custodian/client.json', 'origin-info.json',
         'seed/op_Sum.structured-research.json')
FROZEN = ('turnstone', 'custodian', 'test-binary', 'fixture_server.py',
          'namespace_phase.py', 'acceptance_phase.py')

def require_plain_directory(path):
    path = Path(path)
    if not path.is_absolute(): raise ValueError('path must be absolute')
    for parent in reversed((path, *path.parents)):
        if not stat.S_ISDIR(parent.lstat().st_mode):
            raise ValueError('directory component is not plain: ' + str(parent))

def require_plain_file(path):
    path = Path(path)
    require_plain_directory(path.parent)
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError('file is not regular: ' + str(path))

def stale_socket_inventory(custody):
    require_plain_directory(custody)
    state = custody / 'custodian'; require_plain_directory(state)
    found = {}
    def socket_entry(path):
        meta = path.lstat()
        if not stat.S_ISSOCK(meta.st_mode): raise ValueError('unexpected custody socket type')
        found[str(path.relative_to(custody))] = {'device':meta.st_dev, 'inode':meta.st_ino, 'mode':meta.st_mode}
    endpoint = custody / 'app.sock'
    if endpoint.exists() or endpoint.is_symlink(): socket_entry(endpoint)
    for item in state.iterdir():
        if item.name in ('data','vault','initialized','client.json'): continue
        if not item.name.startswith('run-'): raise ValueError('unexpected custody state entry')
        try: valid = str(uuid.UUID(item.name[4:])) == item.name[4:]
        except ValueError: valid = False
        if not valid: raise ValueError('invalid custody runtime name')
        require_plain_directory(item)
        entries = list(item.iterdir())
        if len(entries) != 1 or entries[0].name != 'custody.sock':
            raise ValueError('unexpected custody runtime content')
        socket_entry(entries[0])
    return found

def inventory(base):
    require_plain_directory(base)
    for name in ROOTS: require_plain_directory(base / name)
    for name in FILES: require_plain_file(base / name)
    return {'trees': {name: phase.tree_hashes(base / name) for name in ROOTS},
            'files': {name: native.sha(base / name) for name in FILES}}

def validate_reader(run, prefix, producer, expected):
    search_path = run / (prefix + '.saved-sources.json')
    opened_path = run / (prefix + '_reader.saved-sources.json')
    require_plain_file(search_path); require_plain_file(opened_path)
    search = json.loads(search_path.read_text()); opened = json.loads(opened_path.read_text())
    result = search['search']
    if (result['coverage'] != 'Complete' or result['session'] != search['session']
        or opened['session'] != search['session']):
        raise ValueError('search and reader sessions or coverage differ')
    hits = [h for h in result['hits'] if h['producer'] == producer]
    if len(hits) != 1: raise ValueError('expected one ' + producer + ' hit')
    hit = hits[0]; source = hit['source']; version = hit['version']
    if not isinstance(version, str) or not re.fullmatch(r'blake3:[0-9a-f]{64}', version):
        raise ValueError('invalid source version')
    uri = source.get('source', ''); prefix_uri = 'mere://retained-text/'
    parts = uri.removeprefix(prefix_uri).split('/') if uri.startswith(prefix_uri) else []
    if len(parts) != 5 or any(str(uuid.UUID(value)) != value for value in parts[:3]):
        raise ValueError('invalid retained source identity')
    session, graph, node, manifest, text_hash = parts
    if (session != search['session'] or source.get('kind') != 'fleece'
        or source.get('owner') != 'turnstone/' + session + '/research/v1'
        or source.get('node') != node or not re.fullmatch(r'[0-9a-f]{64}', manifest)
        or text_hash != version[7:]):
        raise ValueError('accepted retained identity differs')
    clip = {'session':session, 'graph':graph, 'node':node, 'manifest':'blake3:'+manifest}
    if search['retained_admissions'].count(clip) != 1 or opened['retained_admissions'].count(clip) != 1:
        raise ValueError('accepted hit lacks unique current admission')
    matches = [r for r in opened['retained_readers'] if r['target']['clip'] == clip]
    if len(matches) != 1: raise ValueError('exact retained reader missing or ambiguous')
    reader = matches[0]; target = reader['target']
    if (reader['node'] != node or reader['engine'] != 'fleece.retained-clip'
        or target['text_hash'] != version or target['selector'] != hit['selector']):
        raise ValueError('reader target differs from accepted hit')
    selector = hit['selector']
    if (not isinstance(selector,dict) or type(selector.get('start')) is not int
        or type(selector.get('end')) is not int or not 0 <= selector['start'] < selector['end'] <= len(expected)
        or expected[selector['start']:selector['end']] != expected):
        raise ValueError('selector differs from full expected text')
    document = reader['rendered_document']; blocks = document.get('blocks')
    paragraph = {'Paragraph':{'spans':[{'Text':expected}]}}
    title = 'Knot selection' if producer == 'Selection' else 'Redshank note'
    head = [{'Heading':{'level':1,'spans':[{'Text':title}]}},
            {'Heading':{'level':2,'spans':[{'Text':'Selected passage'}]}},
            {'Quote':{'blocks':[paragraph]}}, paragraph]
    export = {'Paragraph':{'spans':[{'Link':{'predicate':None, 'spans':[{'Text':'Export selected clip'}],
        'title':None, 'url':'mere://clip/'+manifest+'/export'}}]}}
    if (document.get('address') != 'mere://clip/'+manifest
        or document.get('content_type') != 'application/vnd.mere.clip+json'
        or not isinstance(blocks,list) or len(blocks) < 5 or blocks[:4] != head or blocks[-1] != export):
        raise ValueError('rendered body contains extra or different content')
    if producer == 'Selection':
        metadata = blocks[4:-1]
        if (len(metadata) != 2 or metadata[1] != {'MetadataRow':{'label':'Original selection','value':expected}}
            or not isinstance(metadata[0],dict) or set(metadata[0]) != {'MetadataRow'}
            or set(metadata[0]['MetadataRow']) != {'label','value'}
            or metadata[0]['MetadataRow']['label'] != 'Source anchor (utf8-bytes)'
            or not re.fullmatch(r'file:///m5/run/profile/notes/research-[0-9a-f-]{36}\.djot', metadata[0]['MetadataRow']['value'])):
            raise ValueError('selection metadata differs')
    else:
        metadata = [{'Badge':{'text':'Source recording unavailable in this clip'}},
            {'MetadataRow':{'label':'Source anchor (milliseconds)','value':'file:///m5/run/fixture-source/episode.mp3'}},
            {'MetadataRow':{'label':'Original note position','value':'0.000 seconds'}}]
        if blocks[4:-1] != metadata: raise ValueError('note metadata differs')
    phase.validate_png(run / (prefix + '_reader.png'))
    return {'producer':producer, 'node':node, 'manifest':clip['manifest'],
            'text_hash':version, 'selector':selector, 'active_reader_proven':False,
            'visual_review_required':True}

def main():
    source = SOURCE / 'sandbox'
    require_plain_directory(SOURCE)
    require_plain_file(source / 'm5-disposable-marker')
    require_plain_file(SOURCE / 'reopen-launcher.cleanup.json')
    require_plain_file(SOURCE / 'frozen/reopen-config.json')
    require_plain_file(SCENARIO)
    for name in FROZEN: require_plain_file(SOURCE / 'frozen' / name)
    require_plain_directory(OUT.parent)
    if OUT.exists() or SOURCE.is_symlink():
        raise ValueError('output exists or source is not the expected marked root')
    cleanup = json.loads((SOURCE / 'reopen-launcher.cleanup.json').read_text())
    if not any(item.get('cgroup_empty_verified') is True for item in cleanup.get('cleanup', [])):
        raise ValueError('source reopen scope not verified empty')
    sock = source / 'custody/app.sock'
    if sock.exists() or sock.is_socket():
        if not stat.S_ISSOCK(sock.lstat().st_mode): raise ValueError('unexpected custody endpoint type')
    original = inventory(source)
    original_sockets = stale_socket_inventory(source / 'custody')
    frozen_source = {name: native.sha(SOURCE / 'frozen' / name) for name in FROZEN}
    config_source = native.sha(SOURCE / 'frozen/reopen-config.json')
    scenario_hash = native.sha(SCENARIO)
    OUT.mkdir(mode=0o700)
    run, frozen = OUT / 'sandbox', OUT / 'frozen'
    run.mkdir(mode=0o700); frozen.mkdir(mode=0o700)
    error = None
    try:
        (run / 'm5-disposable-marker').write_text('m5-disposable/v1\n')
        for name in ('data', 'config', 'cache', 'runtime', 'control', 'seed'):
            (run / name).mkdir(mode=0o700)
        for name in ('profile', 'mere'):
            shutil.copytree(source / name, run / name, symlinks=False)
        (run / 'custody').mkdir(mode=0o700)
        (run / 'custody/custodian').mkdir(mode=0o700)
        for name in ('data','vault'):
            shutil.copytree(source / 'custody/custodian' / name, run / 'custody/custodian' / name, symlinks=False)
        for name in FILES:
            shutil.copy2(source / name, run / name)
        if inventory(run) != original or inventory(source) != original:
            raise ValueError('initial copy differs or source changed')
        for name in FROZEN:
            if native.freeze_file(SOURCE / 'frozen' / name, frozen / name) != frozen_source[name]:
                raise ValueError('frozen binary or helper changed: ' + name)
        if native.freeze_file(SCENARIO, frozen / 'reopen.scn') != scenario_hash:
            raise ValueError('scenario changed during copy')
        cfg = json.loads((SOURCE / 'frozen/reopen-config.json').read_text())
        if cfg['phase'] != 'reopen' or cfg['output'] != '/m5/run': raise ValueError('unexpected config')
        devices = native.device_identity(Path('/run/user/1000/wayland-0'), Path('/dev/dri/renderD128'))
        if devices != cfg['devices']: raise ValueError('device identity changed')
        cfg['timeout_seconds'] = 180
        cfg_path = frozen / 'reopen-config.json'; native.record(cfg_path, cfg)
        cmd = acceptance.make_command(run, frozen, devices, cfg_path, 'reopen')
        unit = 'm5-cold-copy-' + uuid.uuid4().hex
        scope = native.cgroup_scope(cmd, cfg['memory_bytes'], cfg['tasks_max'], cfg['cpu_percent'], unit, 600)
        try:
            native.run_scope(scope, unit, OUT / 'reopen-launcher.log', 610)
            raise ValueError('diagnostic did not stop intentionally')
        except RuntimeError as exc:
            if 'scope launcher failed with exit 1' not in str(exc): raise
        cleanup = json.loads((OUT / 'reopen-launcher.cleanup.json').read_text())
        if not any(item.get('cgroup_empty_verified') is True for item in cleanup.get('cleanup', [])):
            raise ValueError('diagnostic scope not verified empty')
        done = (run / 'reopen/scenario.done').read_text()
        fails = [line for line in done.splitlines() if line.startswith('FAIL:')]
        if len(fails) != 1 or 'm5-intentional-diagnostic-stop' not in fails[0]:
            raise ValueError('unexpected scenario failure: ' + repr(fails))
        selection = validate_reader(run / 'reopen', '24_reopen_selection', 'Selection', 'only this linked finding')
        note = validate_reader(run / 'reopen', '25_reopen_note', 'Note', 'M5 authored Redshank note')
        for name in ('22_reopen_A1.png', '23_reopen_A2.png', 'model_unavailable.png'):
            phase.validate_png(run / 'reopen' / name)
        native.record(OUT / 'diagnostic.json', {'status':'diagnostic-only-awaiting-visual-review',
            'source':str(SOURCE), 'scenario_sha256':scenario_hash, 'frozen_sha256':frozen_source,
            'source_config_sha256':config_source, 'scope':unit, 'intentional_stop':True,
            'selection_reader':selection, 'note_reader':note})
    except BaseException as exc:
        error = repr(exc)
        native.record(OUT / 'failure.json', {'error':error, 'time_ns':time.time_ns()})
        raise
    finally:
        try:
            after = inventory(source)
            sockets_after = stale_socket_inventory(source / 'custody')
            unchanged = after == original and sockets_after == original_sockets and all(native.sha(SOURCE / 'frozen' / n) == h for n,h in frozen_source.items()) and native.sha(SOURCE / 'frozen/reopen-config.json') == config_source
            native.record(OUT / 'source-integrity.json', {'source':str(SOURCE), 'before':original,
                'after':after, 'sockets_before':original_sockets, 'sockets_after':sockets_after,
                'unchanged':unchanged, 'error':error})
            if not unchanged: raise ValueError('source changed during diagnostic')
        except BaseException as exc:
            native.record(OUT / 'source-integrity-failure.json', {'error':repr(exc)})
            raise

if __name__ == '__main__': main()
