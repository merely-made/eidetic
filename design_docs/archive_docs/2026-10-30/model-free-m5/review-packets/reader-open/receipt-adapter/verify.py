#!/usr/bin/env python3
"""Validation primitives for a draft receipt; never grants final acceptance."""
import hashlib
import json
from pathlib import Path
import types

OPERATIONS = {'SourceLookup','CountByProducer','CountBySource','SourceTimeline','RecordTimeline',
              'TagMembership','PropertyLookup','GroupValues','JoinEntity','JoinSource','Compare','Sum'}
A1_FULL_PASSAGE = 'M5 A1 archive: naïve — 東京 🐦. Evidence amount is 12 units.'
COMMANDS = {
    'turnstone':['cargo','build','--locked','--offline','-p','turnstone','--features','authoring,inference','--bin','turnstone'],
    'test-binary':['cargo','test','--locked','--offline','-p','turnstone','--features','authoring,inference','--lib','--no-run'],
    'custodian':['cargo','test','--locked','--offline','-p','mere-resident','--features','custody-receipt-host','--test','custody_payload','--no-run'],
}
FAULT_REQUIRED = {'inputs.json','primary-integrity.json','launcher.json','launcher.cleanup.json',
                  'launcher.log','output/app.log','output/custody.log','output/fault.log',
                  'output/fault-postcheck.log','output/route.log','output/scenario.done',
                  'output/processes.json','output/processes-final.json','output/namespace.json',
                  'output/phase-checked.json','output/fault.json','output/fault-postcheck.json',
                  'output/graph-integrity.json','output/route.json',
                  'output/missing_all_scope.saved-sources.json','output/missing_before_click.saved-sources.json',
                  'output/missing_A1_open.saved-sources.json','output/missing_A2_open.saved-sources.json',
                  'output/missing_runtime.runtime.json','output/missing_before_click.png',
                  'output/missing_A1_peek.png','output/missing_A1_refusal.png','output/missing_A2_ready.png',
                  'frozen/registration-lock.json','frozen/missing-input.json','frozen/config.json',
                  'frozen/run_missing.py','frozen/run_native.py','frozen/run_acceptance.py',
                  'frozen/fault_phase.py','frozen/namespace_phase.py','frozen/acceptance_phase.py',
                  'frozen/verify_missing.py','frozen/missing.scn','frozen/fault_helper.rs',
                  'frozen/citation_event_history.rs','frozen/citation-observer.patch'}
INTACT_REQUIRED = {'inputs.json','processes.json','seed-launcher.cleanup.json','reopen-launcher.cleanup.json',
                   'sandbox/seed/phase-checked.json','sandbox/reopen/phase-checked.json',
                   'sandbox/reopen/inspection-source-hashes.json','sandbox/reopen/structured-inspection.json',
                   'sandbox/reopen/withdrawal-inspection.json','sandbox/reopen/inspection-integrity.json',
                   'sandbox/reopen/withdrawal_inspection-integrity.json','sandbox/selected.mereclip',
                   'sandbox/seed/06_note_citation.saved-sources.json',
                   'sandbox/seed/04_exact_A1.saved-sources.json','sandbox/seed/exact_pair.comparison.json',
                   'sandbox/seed/10_selection_admitted.saved-sources.json',
                   'sandbox/seed/redshank_note_admitted.saved-sources.json',
                   'sandbox/reopen/24_reopen_selection.saved-sources.json','sandbox/reopen/25_reopen_note.saved-sources.json',
                   'sandbox/reopen/24_reopen_selection_reader.saved-sources.json','sandbox/reopen/25_reopen_note_reader.saved-sources.json',
                   'sandbox/reopen/22_reopen_A1.saved-sources.json','sandbox/reopen/23_reopen_A2.saved-sources.json'}
TOOLS = {
    'verify_receipt.py': ('../m5-draft/verify_receipt.py','c7890e11e3000694949247d1dd38b5206c1046d9657aa02848cddc4972f46019'),
    'verify_missing.py': ('../m5-missing-draft/verify_missing.py','ed2cde69988c15b44167b8c916124c8f2b7da8ca86c939f77a219f9f440fa939'),
    'run_missing.py': ('../m5-missing-draft/run_missing.py','9556edae322fa658f7f4ad3dc1828253f63c2294ee176fd79ebada976eaf9e95'),
}


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def regular(base, rel):
    base = Path(base).resolve(strict=True)
    if not isinstance(rel, str) or not rel or '\\' in rel:
        raise ValueError('invalid relative path')
    parts = rel.split('/')
    if any(part in ('', '.', '..') for part in parts) or Path(rel).is_absolute():
        raise ValueError('unsafe relative path: ' + rel)
    current = base
    for part in parts:
        current = current / part
        if current.is_symlink():
            raise ValueError('symlink in evidence path: ' + rel)
    if not current.is_file() or not current.resolve(strict=True).is_relative_to(base):
        raise ValueError('evidence is not a confined regular file: ' + rel)
    return current


def validate_ledger(root, ledger, required):
    if not isinstance(ledger, dict) or not ledger or not set(required) <= set(ledger):
        raise ValueError('ledger missing required evidence')
    for rel, digest in ledger.items():
        if not isinstance(digest, str) or len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid ledger digest')
        if sha(regular(root, rel)) != digest:
            raise ValueError('ledger hash differs: ' + rel)
    return ledger


def check_collector_bindings(inputs, ledger, bound, binary_hashes):
    if inputs.get('kind') != 'model-free-acceptance-collection':
        raise ValueError('wrong collection input kind')
    binaries = set(COMMANDS)
    required = binaries | {'seed.scn','reopen.scn','run_native.py','namespace_phase.py','fixture_server.py','acceptance_phase.py','run_acceptance.py'}
    if not required <= set(inputs['files']) or set(binary_hashes) != binaries:
        raise ValueError('collector frozen inputs incomplete')
    for name,digest in inputs['files'].items():
        if name in binaries:
            if binary_hashes[name] != digest: raise ValueError('assembled binary check differs: ' + name)
        elif ledger.get('frozen/'+name) != digest:
            raise ValueError('collector frozen input differs: ' + name)
    provenance_path = 'frozen/build-provenance.json'
    if ledger.get(provenance_path) != inputs['provenance_sha256']:
        raise ValueError('build provenance unbound')
    provenance = bound(provenance_path)
    if provenance.get('schema') != 'turnstone.m5-build-provenance/v1': raise ValueError('build schema differs')
    for name, command in COMMANDS.items():
        saved, entry = provenance['artifacts'][name], inputs['builds'][name]
        receipt_path = 'frozen/' + name + '-build.json'
        inventory_path = 'frozen/' + name + '-source-inventory.json'
        if receipt_path not in ledger or inventory_path not in ledger: raise ValueError('build evidence unbound')
        receipt, source = bound(receipt_path), ledger[inventory_path]
        if (receipt.get('exit_code') != 0 or receipt.get('command') != command or entry.get('command') != command
            or entry.get('receipt_sha256') != ledger[receipt_path] or saved.get('receipt_sha256') != ledger[receipt_path]
            or entry.get('source_inventory') != inventory_path or entry.get('source_hash') != source
            or saved.get('source_before') != source or saved.get('source_after') != source
            or saved.get('source_inventory_sha256') != source or saved.get('binary_sha256') != inputs['files'][name]
            or entry.get('binary_sha256') != inputs['files'][name]):
            raise ValueError('build/source evidence differs: ' + name)


def load_pinned_module(path, expected, name):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError('validation module missing')
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError('validation module hash differs')
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(data, str(path), 'exec'), module.__dict__)
    return module


def refuse_unreviewed_acceptance():
    raise ValueError('DRAFT ONLY: final receipt integration awaits scoped review and actual evidence')


def check_group_rows(records, rows):
    def key(record):
        return json.dumps([record[k] for k in ('entity','property','schema_revision','context','status','valid_time','value')],sort_keys=True)
    expected = {}
    for record in records:
        expected.setdefault(key(record),set()).add(record['id'])
    by_id = {record['id']:record for record in records}
    actual = {}
    for row in rows:
        ids = row['record_ids']
        if not ids or len(set(ids)) != len(ids): raise ValueError('invalid group contributors')
        record = by_id[ids[0]]
        group = key(record)
        expected_group = {k:record[k] for k in ('entity','property','schema_revision','context','status','valid_time','value')}
        if group in actual or any(key(by_id[i]) != group for i in ids) or row['key'] != {'Value':record['value']} or row['group'] != expected_group:
            raise ValueError('group values or identity differ')
        actual[group] = set(ids)
    if expected != actual: raise ValueError('group results incomplete')


def check_pair_rows(records, rows, predicate):
    expected = {frozenset((a['id'],b['id'])) for i,a in enumerate(records) for b in records[i+1:] if predicate(a,b)}
    pairs = [frozenset(row['record_ids']) for row in rows]
    if any(len(pair) != 2 for pair in pairs) or len(pairs) != len(set(pairs)) or set(pairs) != expected:
        raise ValueError('pair results incomplete or duplicated')


def check_comparison_classes(rows):
    if any(row['key'].get('Comparison') not in ('Equal','Disagreement','Distinct','NonComparable') for row in rows):
        raise ValueError('unknown comparison class')


def check_property_rows(records, rows):
    by_id = {r['id']:r for r in records}
    ids = []
    for row in rows:
        if len(row['record_ids']) != 1: raise ValueError('property row contributor differs')
        record_id = row['record_ids'][0]
        if record_id not in by_id or row['key'] != {'Value':by_id[record_id]['value']}:
            raise ValueError('property row value differs')
        ids.append(record_id)
    if len(ids) != len(set(ids)) or set(ids) != set(by_id): raise ValueError('property rows incomplete or repeated')


def evidence_source(record):
    return next(iter(record['evidence'].values()))['source']


def check_row_sources(records, rows):
    by_id = {r['id']:r for r in records}
    for row in rows:
        ids = row['record_ids']
        if not ids: continue
        expected = {canonical(evidence_source(by_id[record_id])) for record_id in ids}
        actual = [canonical(source) for source in row['sources']]
        if len(actual) != len(set(actual)) or set(actual) != expected:
            raise ValueError('row source contributors differ')


def check_sum_rows(records, definitions, rows):
    if not records:
        if rows: raise ValueError('sum of empty inputs has rows')
        return
    if len(rows) != 1: raise ValueError('sum requires exactly one row')
    row = rows[0]
    ids = [r['id'] for r in records]
    if len(ids) != len(set(ids)) or len(row['record_ids']) != len(set(row['record_ids'])) or set(row['record_ids']) != set(ids):
        raise ValueError('sum contributors differ from eligible inputs')
    sources = {canonical(evidence_source(r)) for r in records}
    refs = [canonical(s) for s in row['sources']]
    if len(refs) != len(set(refs)) or set(refs) != sources: raise ValueError('sum source contributors differ')
    identities = {(r['property'],r['schema_revision']) for r in records}
    if len(identities) != 1: raise ValueError('sum property schemas differ')
    key = next(iter(identities))
    matches = [d for d in definitions if (d['id'],d['revision']) == key]
    if len(matches) != 1: raise ValueError('sum definition missing or ambiguous')
    first = records[0]['value']
    if set(first) == {'Integer'}:
        if matches[0]['value_type'] != 'Integer': raise ValueError('sum definition type differs')
        def numeric(record):
            value = record['value']
            if set(value) != {'Integer'} or type(value['Integer']) is not int: raise ValueError('sum numeric types differ')
            return value['Integer']
        def result(n): return {'Integer':n}
    elif set(first) == {'Decimal'}:
        shape = first['Decimal']; unit, scale = shape['unit'], shape['scale']
        if matches[0]['value_type'] != 'Decimal' or matches[0]['unit'] != unit or type(scale) is not int or not 0 <= scale <= 18: raise ValueError('sum unit or scale differs')
        def numeric(record):
            value = record['value']
            if set(value) != {'Decimal'}: raise ValueError('sum numeric types differ')
            d = value['Decimal']
            if d['unit'] != unit or d['scale'] != scale or type(d['coefficient']) is not int: raise ValueError('sum unit or scale differs')
            return d['coefficient']
        def result(n): return {'Decimal':{'coefficient':n,'scale':scale,'unit':unit}}
    else:
        raise ValueError('sum input is not numeric')
    total = 0
    for record in sorted(records,key=lambda r:r['id']):
        value = numeric(record)
        if not -(2**63) <= value < 2**63: raise ValueError('sum input is out of range')
        total += value
        if not -(2**63) <= total < 2**63: raise ValueError('sum overflow')
    if row['key'] != {'Value':result(total)}: raise ValueError('sum value differs')


def check_count_source(records, rows):
    expected = {}
    for record in records:
        expected.setdefault(canonical(evidence_source(record)),set()).add(record['id'])
    actual = {}
    for row in rows:
        source = row['key']['Source']; source_key = canonical(source); ids = row['record_ids']
        if source_key in actual or len(ids) != len(set(ids)) or row['count'] != len(ids) or row['sources'] != [source]:
            raise ValueError('source count row differs')
        actual[source_key] = set(ids)
    if actual != expected: raise ValueError('source count contributors differ')


def check_producer_rows(sources, rows):
    expected = {}
    for source in sources:
        expected.setdefault(source['producer'], set()).add(canonical(source['source']))
    actual = {}
    for row in rows:
        producer = row['key']['Text']
        refs = [canonical(ref) for ref in row['sources']]
        if producer in actual or len(refs) != len(set(refs)) or row['record_ids'] or row['count'] != len(refs):
            raise ValueError('producer count row differs')
        actual[producer] = set(refs)
    if actual != expected: raise ValueError('producer source contributors differ')


def check_cleanup(cleanup):
    if cleanup.get('normal') is not True or cleanup.get('ownership_observed') is not True or not any(x.get('cgroup_empty_verified') is True for x in cleanup.get('cleanup', [])):
        raise ValueError('owned scope cleanup not established')


def check_runtime(runtime, pid, principal, offline):
    activity = runtime['inference']
    if runtime.get('pid') != pid or runtime.get('custody_principal') != principal:
        raise ValueError('runtime process or principal differs')
    if activity.get('client_compiled') is not True or activity.get('generation_requests') != 0 or activity.get('corpus_publications') != 0 or activity.get('resident_model_loads') is not None:
        raise ValueError('runtime inference counters differ')
    if offline and (runtime.get('network_offline') is not True or runtime.get('offline_acquisition_attempts') != 0):
        raise ValueError('offline runtime acquisition differs')


def check_namespace(net):
    if not net.get('addresses') or any(x.get('ifname') != 'lo' for x in net['addresses']) or any(x.get('dev') not in (None,'lo') for x in net['routes']):
        raise ValueError('namespace has an external network interface or route')
    caps = net['setup_capabilities']
    if any(caps.get(k) != '0' for k in ('effective','permitted','inheritable','ambient')) or caps.get('no_new_privs') is not True:
        raise ValueError('namespace setup capabilities remain')
    group = net['cgroup']
    quota, period = (int(x) for x in group['cpu_max'].split())
    if int(group['memory_max']) > 3*1024**3 or group['memory_swap_max'] != '0' or int(group['pids_max']) > 128 or quota*100 > 150*period:
        raise ValueError('namespace resource cap differs')
    if any(not net.get(k) for k in ('pidns','netns','mountns')):
        raise ValueError('namespace identity missing')


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',',':'))


def snapshot_inventory(snapshot):
    if snapshot.get('complete') is not True or snapshot.get('exclusion_count') != 0 or snapshot.get('exclusions') != []:
        raise ValueError('owner snapshot is incomplete')
    def unique(values, key):
        result = {}
        for item in values:
            k = key(item)
            if k in result: raise ValueError('duplicate snapshot identity')
            result[k] = item
        return result
    sources = unique(snapshot['sources'], lambda s:canonical(s['source']))
    records = unique(snapshot['records'], lambda r:r['id'])
    definitions = unique(snapshot['definitions'], lambda d:(d['id'],d['revision']))
    lifecycle = unique(snapshot['lifecycle'],lambda x:x['record_id'])
    if set(lifecycle) != set(records) or any(x['state'] != 'Active' for x in lifecycle.values()):
        raise ValueError('snapshot lifecycle differs')
    for r in records.values():
        if (r['property'],r['schema_revision']) not in definitions:
            raise ValueError('record definition missing')
        variant = r['evidence']
        if len(variant) != 1 or next(iter(variant)) not in ('Passage','Field'):
            raise ValueError('unsupported source evidence')
        evidence = next(iter(variant.values()))
        source = sources.get(canonical(evidence['source']))
        if source is None or source['version'] != evidence['version']:
            raise ValueError('record source version differs')
    return sources, records, definitions, lifecycle


def check_owner_inventory(owner, expected_records=14):
    if owner.get('schema') != 'turnstone.m5-store-inspection/v1' or not owner.get('session') or not owner.get('graph'):
        raise ValueError('owner inspection identity missing')
    sources, records, definitions, lifecycle = snapshot_inventory(owner['result']['snapshot'])
    if len(records) != expected_records or set(owner['records']) != set(records):
        raise ValueError('owner record inventory incomplete')
    for record_id, record in records.items():
        item = owner['records'][record_id]
        if item['record'] != record or item['definition'] != definitions[(record['property'],record['schema_revision'])] or item['graph'] != owner['graph'] or type(item['withdrawn']) is not bool:
            raise ValueError('owner record or definition differs')
    return sources, records, definitions, lifecycle


def check_prewithdraw_scope(observation, seed_sum, owner):
    if observation['session'] != owner['session'] or seed_sum['session'] != owner['session']:
        raise ValueError('operation session differs from owner')
    if observation['source_selection'] != seed_sum['source_selection'] or seed_sum['source_selection']['mode'] not in ('all','chosen'):
        raise ValueError('operation selected a different source scope')
    revisions = observation['graph_revisions']
    if len(revisions) != 1 or revisions[0]['graph'] != owner['graph']:
        raise ValueError('operation graph differs from owner')
    if snapshot_inventory(observation['result']['snapshot']) != snapshot_inventory(owner['result']['snapshot']):
        raise ValueError('operation inputs differ from owner inventory')


def filtered_inputs(snapshot, filters):
    names = {'source','entity','property','schema_revision','status','valid_time','unit','tag'}
    if not isinstance(filters,dict) or set(filters) != names:
        raise ValueError('unknown or incomplete query filters')
    sources, records, definitions, _ = snapshot_inventory(snapshot)
    chosen_sources = [s for s in sources.values()
                      if (filters['source'] is None or s['source'] == filters['source'])
                      and (filters['tag'] is None or filters['tag'] in s['tags'])]
    selected_source_ids = {canonical(s['source']) for s in chosen_sources}
    chosen_records = []
    for r in records.values():
        evidence = next(iter(r['evidence'].values()))
        definition = definitions[(r['property'],r['schema_revision'])]
        if canonical(evidence['source']) not in selected_source_ids: continue
        if any(filters[k] is not None and r[k] != filters[k] for k in ('entity','property','schema_revision','status')): continue
        if filters['unit'] is not None and definition['unit'] != filters['unit']: continue
        interval = filters['valid_time']
        if interval is not None:
            if type(interval.get('start')) is not int or type(interval.get('end')) is not int or interval['start'] >= interval['end']:
                raise ValueError('invalid query time interval')
            value = r['valid_time']
            if value is None or not (value['start'] < interval['end'] and interval['start'] < value['end']): continue
        chosen_records.append(r)
    return chosen_sources, chosen_records


def check_withdrawal_inventory(historical, current, withdrawn_id):
    old_sources, old_records, old_definitions, old_lifecycle = snapshot_inventory(historical)
    sources, records, definitions, lifecycle = snapshot_inventory(current)
    if withdrawn_id not in old_records:
        raise ValueError('withdrawn record missing from historical owner')
    expected_records = {k:v for k,v in old_records.items() if k != withdrawn_id}
    expected_lifecycle = {k:v for k,v in old_lifecycle.items() if k != withdrawn_id}
    used_definitions = {(r['property'],r['schema_revision']) for r in expected_records.values()}
    expected_definitions = {k:v for k,v in old_definitions.items() if k in used_definitions}
    if (sources != old_sources or records != expected_records or lifecycle != expected_lifecycle or definitions != expected_definitions):
        raise ValueError('current owner inputs differ after withdrawal')


def check_selection_anchor(selection):
    clip, package_clip = selection['clip'], selection['package']['clip']
    if clip['clip_id'] != package_clip['clip_id'] or clip['anchors'] != package_clip['anchors'] or len(clip['anchors']) != 1:
        raise ValueError('exported selection anchor differs from owner')
    anchor = clip['anchors'][0]
    selector = anchor['selector']
    if (anchor['units'] != 'utf8-bytes' or selector['source_hash'] != selection['source_hash'] or not selection['source_hash']
        or selector['exact_quote'] != selection['quote'] or selector['byte_span'] != {'start':selection['byte_start'],'end':selection['byte_end']}):
        raise ValueError('selection anchor does not bind inspected source')


def full_capture_source(target):
    import uuid
    key = target['key']
    for name in ('session','graph','node'):
        if str(uuid.UUID(key[name])) != key[name]: raise ValueError('noncanonical capture identity')
    annotation, text = key['annotation_manifest'], target['text_hash']
    if not annotation.startswith('sealed-v1:blake3:') or not text.startswith('blake3:'):
        raise ValueError('capture hash scheme differs')
    ann, digest = annotation.removeprefix('sealed-v1:blake3:'),text.removeprefix('blake3:')
    if any(len(s) != 64 or any(c not in '0123456789abcdef' for c in s) for s in (ann,digest)):
        raise ValueError('capture hash encoding differs')
    address = 'mere://saved-source/sealed-v1/' + '/'.join((key['session'],key['graph'],key['node'],ann,digest))
    return {'kind':'fleece','owner':'turnstone/'+key['session']+'/research/v1','source':address,'node':key['node']}


def check_source_comparison(comparison, old_reader, new_reader):
    import uuid
    if comparison['schema'] != 'turnstone.comparison-observation/v1' or comparison['operation'] != 'eidetic.line-comparison/v1' or comparison['relationship'] != 'Explicit':
        raise ValueError('explicit source comparison missing')
    if str(uuid.UUID(comparison['request'])) != comparison['request']: raise ValueError('comparison request identity differs')
    for name, observation, identity in (('left',old_reader,'old'),('right',new_reader,'new')):
        target, content = observation['target'], observation['content']
        full_target = dict(target, selector=None)
        source = {'source':full_capture_source(full_target),'version':target['text_hash'],'text_hash':target['text_hash']}
        if comparison[name] != full_target or comparison[identity] != source or target['text_hash'] != content['text_hash'] or comparison['session'] != target['key']['session']:
            raise ValueError('comparison exact source differs')
    texts = (old_reader['content']['text'],new_reader['content']['text'])
    positions = [0,0]
    spans = comparison['spans']
    if not spans or len(spans) > comparison['settings']['max_output_spans']: raise ValueError('comparison spans missing')
    for span in spans:
        kind = span['kind']
        if kind not in ('Equal','Delete','Insert'): raise ValueError('comparison span kind differs')
        pieces = []
        for i, side in enumerate(('old','new')):
            part = span[side]
            present = kind == 'Equal' or (kind == 'Delete' and i == 0) or (kind == 'Insert' and i == 1)
            if not present:
                if part is not None: raise ValueError('comparison span has unexpected side')
                pieces.append(None); continue
            if not isinstance(part,dict) or type(part.get('start')) is not int or type(part.get('end')) is not int or part['start'] != positions[i] or not part['start'] < part['end'] <= len(texts[i]):
                raise ValueError('comparison span does not cover source in order')
            if part['end'] != len(texts[i]) and texts[i][part['end']-1] != '\n':
                raise ValueError('comparison span ends within a line')
            pieces.append(texts[i][part['start']:part['end']]); positions[i] = part['end']
        if kind == 'Equal' and pieces[0] != pieces[1]: raise ValueError('equal comparison span differs')
    if positions != [len(text) for text in texts]: raise ValueError('comparison spans omit source text')
    if texts[0] == texts[1] or not any(span['kind'] in ('Delete','Insert') for span in spans):
        raise ValueError('comparison did not demonstrate a version change')


def check_citation_text(note, target, text, quote):
    selector = target['selector']
    if (not isinstance(selector,dict) or type(selector.get('start')) is not int or type(selector.get('end')) is not int
        or not 0 <= selector['start'] < selector['end'] <= len(text) or text[selector['start']:selector['end']] != quote):
        raise ValueError('citation passage selector differs from source')
    source = full_capture_source(target)['source']
    address = source + '?start=' + str(selector['start']) + '&end=' + str(selector['end'])
    marker, close = '```knot.clip.provenance\n', '\n```'
    rest, citations = note, []
    while marker in rest:
        body = rest.split(marker,1)[1]
        if close not in body: raise ValueError('citation provenance fence not closed')
        data,rest = body.split(close,1)
        item = json.loads(data)
        if item.get('schema') == 'knot.clip.insert/v2' and item.get('source_url') == address:
            citations.append(item)
    if len(citations) != 1: raise ValueError('exact note citation absent or ambiguous')
    citation = citations[0]
    selectors = citation['selectors']
    positions = [s for s in selectors if s.get('type') == 'text-position' and s.get('artifact_role') == 'observed-representation']
    quotes = [s for s in selectors if s.get('type') == 'text-quote' and s.get('artifact_role') == 'observed-representation']
    if len(positions) != 1 or positions[0]['start'] != selector['start'] or positions[0]['end'] != selector['end'] or len(quotes) != 1 or quotes[0]['exact'] != quote:
        raise ValueError('note citation provenance selector differs')
    evidence = [e for e in citation['evidence'] if e.get('role') == 'observed-representation' and e.get('canonical_uri') == source]
    if len(evidence) != 1: raise ValueError('note citation evidence identity differs')
    reference = evidence[0]
    digest = reference.get('content',{}).get('transport') or ('blake3:'+reference.get('digest',''))
    if digest != target['text_hash']: raise ValueError('note citation content hash differs')
    raw = text.encode('utf-8')
    portable = reference.get('content')
    if portable is not None:
        import base64
        identity = 'ni:///sha-256;' + base64.urlsafe_b64encode(hashlib.sha256(raw).digest()).decode('ascii').rstrip('=')
        if portable.get('byte_size') != len(raw) or portable.get('portable_id') != identity:
            raise ValueError('note citation portable identity differs')
    elif reference.get('byte_size') != len(raw):
        raise ValueError('note citation evidence size differs')


def ready_reader(observation, marker):
    matches = [r for r in observation['readers'] if r.get('phase') == 'Ready' and r.get('content') is not None and marker in r['content']['text']]
    if len(matches) != 1: raise ValueError('saved reader missing or ambiguous: ' + marker)
    return matches[0]


def check_rendered_quote(reader, quote):
    document = reader.get('rendered_document')
    if reader.get('phase') != 'Ready' or not isinstance(document,dict) or quote not in json.dumps(document,ensure_ascii=False):
        raise ValueError('saved passage quote absent from rendered reader')


def check_retained_search(observation, owner, kind):
    label = kind.lower()
    item = owner[label]
    admission = item['admission']
    if observation['session'] != owner['session'] or admission not in observation['retained_admissions'] or item.get('selected') is not True:
        raise ValueError('retained admission differs from inspected owner')
    if admission['session'] != owner['session'] or admission['graph'] != owner['graph']:
        raise ValueError('retained admission scope differs')
    candidates = [s for s in owner['result']['snapshot']['sources'] if s['producer'] == label
                  and s['source'].get('node') == admission['node']]
    if len(candidates) != 1: raise ValueError('retained owner source missing or ambiguous')
    source = candidates[0]
    manifest = admission['manifest']
    version = source['version']
    if not manifest.startswith('blake3:') or not version.startswith('blake3:'):
        raise ValueError('retained hash scheme differs')
    expected_address = 'mere://retained-text/' + '/'.join((admission['session'],admission['graph'],admission['node'],manifest[7:],version[7:]))
    identity = {'kind':'fleece','owner':'turnstone/'+admission['session']+'/research/v1','node':admission['node'],'source':expected_address}
    if source['source'] != identity: raise ValueError('retained source identity differs')
    search = observation['search']
    if not isinstance(search,dict) or search.get('schema') != 'turnstone.research-search-summary/v1' or search.get('session') != owner['session'] or search.get('coverage') != 'Complete':
        raise ValueError('retained source search incomplete')
    matches = [hit for hit in search['hits'] if hit.get('source') == identity and hit.get('version') == version and hit.get('producer') == kind]
    if len(matches) != 1: raise ValueError('retained source search hit missing or ambiguous')
    return matches[0]


def check_retained_reader(search_observation, reader_observation, owner, kind):
    hit = check_retained_search(search_observation,owner,kind)
    if 'selector' not in hit: raise ValueError('search hit selector not observed')
    item = owner[kind.lower()]
    admission = item['admission']
    expected_text = item['quote'] if kind == 'Selection' else item['text']
    expected_fixture = 'only this linked finding' if kind == 'Selection' else 'M5 authored Redshank note'
    if expected_text != expected_fixture or reader_observation['session'] != owner['session']:
        raise ValueError('retained reader scope or owner text differs')
    matches = [r for r in reader_observation['retained_readers']
               if r.get('target',{}).get('clip') == admission and r.get('target',{}).get('text_hash') == hit['version']]
    if len(matches) != 1: raise ValueError('exact retained reader missing or ambiguous')
    reader = matches[0]
    if reader.get('engine') != 'fleece.retained-clip' or reader['target'].get('selector') != hit['selector']:
        raise ValueError('retained reader engine or selected range differs from search')
    selector = hit['selector']
    if selector is not None and (not isinstance(selector,dict) or type(selector.get('start')) is not int or type(selector.get('end')) is not int
        or not 0 <= selector['start'] < selector['end'] <= len(expected_text) or expected_text[selector['start']:selector['end']] != expected_fixture):
        raise ValueError('retained reader selector does not name owner text')
    document = reader.get('rendered_document')
    expected_body = {'Paragraph':{'spans':[{'Text':expected_fixture}]}}
    if (not isinstance(document,dict) or document.get('address') != 'mere://clip/'+admission['manifest'][7:]
        or document.get('content_type') != 'application/vnd.mere.clip+json' or not isinstance(document.get('blocks'),list)
        or len(document['blocks']) < 2 or document['blocks'][1] != expected_body):
        raise ValueError('retained reader did not render the inspected text')


def check_review_coverage(review, required_frames, ledger):
    if not review.get('reviewer') or not required_frames <= set(review.get('reviewed_frames', [])) or not required_frames <= set(ledger):
        raise ValueError('headed review does not cover required frames')
    if any(review.get('outcomes',{}).get(frame) != 'pass' for frame in required_frames):
        raise ValueError('headed frame outcomes incomplete')


def required_review_frames():
    seed, reopen, fault = 'intact/sandbox/seed/', 'intact/sandbox/reopen/', 'fault/output/'
    frames = {seed+'op_'+op+'.png' for op in OPERATIONS}
    frames |= {seed+name+'.png' for name in ('01_authored_note','04_exact_A1','05_note_citation',
                '07_selected_knot','08_selected_clip','10_selection_admitted','exact_pair',
                'redshank_note_private','redshank_private_action_absent','redshank_note_shareable','redshank_note_admitted',
                'withdraw_before','withdraw_inspected_active','withdraw_history','withdraw_fresh','withdraw_historical',
                'scope_before','scope_empty_selector','scope_empty_search','scope_empty_before_run','scope_empty_fresh',
                'negative_no_match','negative_over_limit')}
    frames |= {seed+'authored_'+str(i)+'.png' for i in range(14)}
    frames |= {reopen+name+'.png' for name in ('22_reopen_A1','23_reopen_A2','24_reopen_selection','24_reopen_selection_reader',
                                                '25_reopen_note','25_reopen_note_reader','cold_sum','model_unavailable')}
    frames |= {fault+name+'.png' for name in ('missing_before_click','missing_A1_peek','missing_A1_refusal','missing_A2_ready')}
    return frames


def empty_filters(property_id=None):
    return {'source':None,'entity':None,'property':property_id,'schema_revision':None,'status':None,'valid_time':None,'unit':None,'tag':None}


def check_no_match(observation, current, graph):
    if (observation.get('schema') != 'turnstone.structured-observation/v1' or observation.get('historical') is not False
        or observation.get('busy') is not False or not observation.get('result_generation')
        or observation['session'] != current['session'] or observation['source_selection'] != current['source_selection']
        or len(observation['graph_revisions']) != 1 or observation['graph_revisions'][0]['graph'] != graph):
        raise ValueError('no-match scope differs from owner')
    result = observation['result']
    if result['query'] != {'operation':'PropertyLookup','filters':empty_filters('m5:absent')}:
        raise ValueError('no-match query differs')
    if snapshot_inventory(result['snapshot']) != snapshot_inventory(current['result']['snapshot']):
        raise ValueError('no-match input differs from current owner')
    _, selected = filtered_inputs(result['snapshot'], result['query']['filters'])
    if selected or result['rows']: raise ValueError('no-match has eligible inputs or rows')


def check_limit_refusal(status, current, graph):
    import uuid
    item = status['limit_refusal']
    if (status.get('schema') != 'turnstone.structured-status/v1' or status.get('busy') is not False or status.get('visible') is not True
        or status['session'] != current['session'] or status.get('error') != 'Structured record limit exceeded'
        or item.get('error') != status['error'] or item.get('stage') != 'worker-refusal'
        or item['source_selection'] != current['source_selection'] or len(item['graph_revisions']) != 1
        or item['graph_revisions'][0]['graph'] != graph):
        raise ValueError('record limit refusal scope or cause differs')
    if str(uuid.UUID(item['request_id'])) != item['request_id']: raise ValueError('invalid limit request identity')
    if not item.get('context_generation') or item['context_generation'] != current.get('context_generation') or type(item.get('scope_revision')) is not int:
        raise ValueError('limit request scope identity missing')
    if item['query'] != {'operation':'PropertyLookup','filters':empty_filters()} or item['settings']['max_records'] != 1:
        raise ValueError('record limit attempt differs')
    if len(current['result']['snapshot']['records']) <= 1:
        raise ValueError('independent current input does not exceed record limit')


def verify_draft(base):
    """Run available checks, then refuse acceptance pending the final review gate."""
    import ast
    import copy
    base = Path(base).resolve(strict=True)
    manifest = json.loads(regular(base, 'manifest.json').read_text())
    if manifest.get('schema') != 'turnstone.m5-receipt-integration-draft/v1' or manifest.get('status') != 'draft':
        raise ValueError('wrong receipt schema')
    files = validate_ledger(base, manifest.get('files'), {'review.json','intact/collection.json','fault/collection.json',
                            'tools/verify_receipt.py','tools/verify_missing.py','tools/run_missing.py'})
    def bound(rel):
        if rel not in files: raise ValueError('unbound receipt evidence: ' + str(rel))
        return json.loads(regular(base, rel).read_text())
    # Recheck both original ledgers against their exact copied bytes.
    for prefix, required in (('intact', INTACT_REQUIRED), ('fault', FAULT_REQUIRED)):
        doc = bound(prefix + '/collection.json')
        source_files = doc.get('files')
        if not isinstance(source_files, dict) or not set(required) <= set(source_files):
            raise ValueError('original ledger incomplete')
        for rel, digest in source_files.items():
            if files.get(prefix + '/' + rel) != digest:
                raise ValueError('original ledger link differs: ' + rel)
    intact_collection = bound('intact/collection.json')
    if intact_collection.get('schema') != 'turnstone.m5-collection/v1' or intact_collection.get('status') != 'collected-awaiting-semantic-and-headed-verification':
        raise ValueError('intact collection not complete')
    fault_collection = bound('fault/collection.json')
    if fault_collection.get('schema') != 'turnstone.m5-missing-collection/v1' or fault_collection.get('status') != 'collected-awaiting-headed-and-semantic-verification':
        raise ValueError('fault collection not complete')
    fault_input = bound('fault/inputs.json')
    if fault_input['run_id'] != manifest['fault_run_id'] or fault_collection['run_id'] != manifest['fault_run_id']:
        raise ValueError('fault run identity differs')
    intact_input = bound('intact/inputs.json')
    check_collector_bindings(intact_input, intact_collection['files'], lambda rel:bound('intact/'+rel),
                             manifest['binary_hashes_checked_at_assembly'])
    if fault_input['devices'] != intact_input['devices'] or bound('fault/frozen/config.json')['devices'] != intact_input['devices']:
        raise ValueError('fault display or render identity differs')
    for name in ('turnstone','custodian','test-binary'):
        if fault_input['frozen'][name] != intact_input['files'][name]: raise ValueError('fault binary differs from intact')
    for rel,digest in fault_input['frozen'].items():
        if rel.endswith('/') or rel in ('turnstone','custodian','test-binary'): continue
        if files.get('fault/frozen/' + rel) != digest: raise ValueError('fault frozen input differs: ' + rel)
    snapshot = fault_input['collection_hashes']
    if snapshot.get('collection.json') != files['intact/collection.json']:
        raise ValueError('fault points to a different intact collection')
    for rel,digest in intact_collection['files'].items():
        if snapshot.get(rel) != digest: raise ValueError('fault snapshot differs: ' + rel)
    for phase in ('seed','reopen'):
        check = 'intact/sandbox/' + phase + '/phase-checked.json'
        if intact_collection['phase_checks'][phase] != files[check]: raise ValueError('phase receipt differs')
        check_cleanup(bound('intact/' + phase + '-launcher.cleanup.json'))
        check_namespace(bound('intact/sandbox/' + phase + '/namespace.json'))
    check_cleanup(bound('fault/launcher.cleanup.json'))
    check_namespace(bound('fault/output/namespace.json'))
    primary_integrity = bound('fault/primary-integrity.json')
    if primary_integrity.get('unchanged') is not True or primary_integrity.get('before') != snapshot or primary_integrity.get('after') != snapshot:
        raise ValueError('primary integrity not joined to fault snapshot')
    processes = bound('intact/processes.json')
    seed_app, reopen_app = processes['seed'], processes['reopen']
    a, b = processes['custody_seed'], processes['custody_reopen']
    if (seed_app['exit_code'] != 0 or reopen_app['exit_code'] != 0 or seed_app['pid_namespace'] == reopen_app['pid_namespace']
        or not seed_app['start_ns'] < seed_app['exit_ns'] < reopen_app['start_ns'] < reopen_app['exit_ns']
        or a['pid_namespace'] == b['pid_namespace'] or not a['exit_ns'] < b['start_ns']
        or a['public_identity'] != b['public_identity'] or a['binary_sha256'] != b['binary_sha256']
        or not processes['origin']['exit_ns'] < reopen_app['start_ns'] < reopen_app['exit_ns'] < processes['inspection']['start_ns']
        or processes['inspection']['exit_code'] != 0 or processes['withdrawal_inspection']['exit_code'] != 0
        or not processes['inspection']['exit_ns'] < processes['withdrawal_inspection']['start_ns']):
        raise ValueError('intact process evidence differs')
    fault_process = bound('fault/output/processes.json')
    fault_final = bound('fault/output/processes-final.json')
    if fault_final.get('failure') or fault_final.get('cleanup_errors') or fault_final.get('socket_cleanup') != 'owned socket absent':
        raise ValueError('fault child cleanup differs')
    if not processes['withdrawal_inspection']['exit_ns'] < fault_process['fault']['start_ns'] or fault_process['app']['pid_namespace'] in (seed_app['pid_namespace'],reopen_app['pid_namespace']):
        raise ValueError('fault process was not distinct and post-inspection')
    fault_check = bound('fault/output/phase-checked.json')
    route = bound('fault/output/route.json')
    if fault_check['principal'] != a['public_identity'] or route.get('outcome') != 'closed-during-open' or {'profile':route.get('profile'),'public_key':route.get('public_key')} != a['public_identity']:
        raise ValueError('fault custody principal or route differs')
    check_runtime(bound('fault/output/missing_runtime.runtime.json'),fault_process['app']['pid'],a['public_identity'],True)
    for phase, proc in (('seed',seed_app),('reopen',reopen_app)):
        phase_check = bound('intact/sandbox/' + phase + '/phase-checked.json')
        principal = phase_check['custody_principal']
        if principal != a['public_identity']: raise ValueError('phase principal differs')
        route = bound('intact/sandbox/' + phase + '/inference-route.json')
        if route.get('outcome') != 'closed-during-open' or {'profile':route.get('profile'),'public_key':route.get('public_key')} != principal:
            raise ValueError('inference route differs')
        for runtime in phase_check['runtime_files']:
            check_runtime(bound('intact/sandbox/' + phase + '/' + runtime),proc['pid'],principal,phase=='reopen')
        if not bound('intact/sandbox/reopen/' + ('inspection' if phase == 'seed' else 'withdrawal_inspection') + '-integrity.json')['passed']:
            raise ValueError('owner integrity failed')
    legacy = load_pinned_module(regular(base,'tools/verify_receipt.py'), TOOLS['verify_receipt.py'][1], 'm5_receipt_semantics')
    missing = load_pinned_module(regular(base,'tools/verify_missing.py'), TOOLS['verify_missing.py'][1], 'm5_missing_semantics')
    runner_path = regular(base,'tools/run_missing.py')
    data = runner_path.read_bytes()
    if hashlib.sha256(data).hexdigest() != TOOLS['run_missing.py'][1]:
        raise ValueError('fault verifier module changed')
    tree = ast.parse(data, filename=str(runner_path))
    funcs = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'verify_bound_case']
    if len(funcs) != 1: raise ValueError('fault verifier entry missing')
    # Compile only the reviewed function; importing run_missing would import
    # mutable launcher modules and could run outside this receipt's contract.
    scope = {'verify_missing':missing.verify_missing}
    exec(compile(ast.Module(body=funcs, type_ignores=[]), str(runner_path), 'exec'), scope)
    seed, reopen, fault = 'intact/sandbox/seed/', 'intact/sandbox/reopen/', 'fault/output/'
    original = bound(seed+'op_Sum.structured-research.json')
    owner = bound(reopen+'structured-inspection.json')
    check_owner_inventory(owner)
    check_prewithdraw_scope(original,original,owner)
    observed = {}
    for op in legacy.OPERATIONS:
        value = bound(seed+'op_'+op+'.structured-research.json')
        legacy.verify_observation(value, op)
        check_prewithdraw_scope(value,original,owner)
        selected_sources, selected_records = filtered_inputs(value['result']['snapshot'],value['result']['query']['filters'])
        filtered = copy.deepcopy(value)
        filtered['result']['snapshot']['sources'] = selected_sources
        filtered['result']['snapshot']['records'] = selected_records
        observed[op] = [filtered]
    if any(s['tags'] for s in owner['result']['snapshot']['sources']):
        raise ValueError('untagged fixture source inventory differs')
    legacy.verify_operation_semantics(observed)
    for op in legacy.OPERATIONS:
        result = observed[op][0]['result']
        check_row_sources(result['snapshot']['records'],result['rows'])
    lookup = observed['PropertyLookup'][0]['result']
    check_property_rows(lookup['snapshot']['records'],lookup['rows'])
    for name in ('SourceLookup','SourceTimeline'):
        result = observed[name][0]['result']
        refs = [canonical(row['sources'][0]) for row in result['rows'] if len(row['sources']) == 1]
        if len(refs) != len(result['rows']) or len(refs) != len(set(refs)) or set(refs) != {canonical(s['source']) for s in result['snapshot']['sources']}:
            raise ValueError('source rows incomplete or repeated')
        if name == 'SourceLookup' and any(row['key'] != {'Source':row['sources'][0]} or row['record_ids'] for row in result['rows']):
            raise ValueError('source lookup row identity differs')
    timeline = observed['RecordTimeline'][0]['result']
    timeline_ids = [row['record_ids'][0] for row in timeline['rows'] if len(row['record_ids']) == 1]
    if len(timeline_ids) != len(timeline['rows']) or len(timeline_ids) != len(set(timeline_ids)) or set(timeline_ids) != {r['id'] for r in timeline['snapshot']['records']}:
        raise ValueError('record timeline incomplete or repeated')
    group = observed['GroupValues'][0]['result']
    check_group_rows(group['snapshot']['records'], group['rows'])
    sum_result = observed['Sum'][0]['result']
    check_sum_rows(sum_result['snapshot']['records'],sum_result['snapshot']['definitions'],sum_result['rows'])
    count_result = observed['CountBySource'][0]['result']
    check_count_source(count_result['snapshot']['records'],count_result['rows'])
    def source(record):
        return json.dumps(next(iter(record['evidence'].values()))['source'], sort_keys=True)
    for op, predicate in (('JoinEntity',lambda a,b:a['entity']==b['entity']),
                          ('JoinSource',lambda a,b:source(a)==source(b)),('Compare',lambda a,b:True)):
        result = observed[op][0]['result']
        check_pair_rows(result['snapshot']['records'],result['rows'],predicate)
        if op == 'Compare': check_comparison_classes(result['rows'])
        if op != 'Compare' and any(set(row['key'].get('Pair',[])) != set(row['record_ids']) for row in result['rows']):
            raise ValueError('join row key differs from contributors')
    counts = observed['CountByProducer'][0]['result']
    check_producer_rows(counts['snapshot']['sources'],counts['rows'])
    cold = bound(reopen+'cold_sum.structured-research.json')
    if not (original['result'] == cold['result'] == owner['result'] and original['content_manifest'] == cold['content_manifest'] == owner['manifest'] and original['source_selection']['mode'] in ('all','chosen') and cold['historical'] is True and cold['source_selection'] == {'mode':'unknown-historical'}):
        raise ValueError('historical replay differs from owner')
    source_rel = owner['selection']['source_relative_path']
    profile_source = 'intact/sandbox/profile/' + source_rel
    inventory = bound(reopen+'inspection-source-hashes.json')
    if files.get(profile_source) != inventory.get(source_rel): raise ValueError('selected source not bound to owner inventory')
    check_selection_anchor(owner['selection'])
    legacy.verify_copied_selection(base, files, {'inspection_root':'intact/sandbox/profile',
        'copied_source':profile_source,'exported_package':'intact/sandbox/selected.mereclip'}, owner)
    legacy.verify_old_and_current(bound(reopen+'22_reopen_A1.saved-sources.json'),bound(reopen+'23_reopen_A2.saved-sources.json'),owner)
    old = ready_reader(bound(reopen+'22_reopen_A1.saved-sources.json'),'M5 A1 archive')
    new = ready_reader(bound(reopen+'23_reopen_A2.saved-sources.json'),'M5 A2 archive')
    check_source_comparison(bound(seed+'exact_pair.comparison.json'),old,new)
    for label in ('04_exact_A1','06_note_citation'):
        saved = bound(seed+label+'.saved-sources.json')
        citation_reader = ready_reader(saved,'M5 A1 archive')
        if saved['session'] != owner['session'] or citation_reader['target'] != old['target'] or citation_reader['content'] != old['content']:
            raise ValueError('saved citation reader differs from exact owner version')
        check_rendered_quote(citation_reader,'naïve — 東京 🐦')
    check_rendered_quote(old,'naïve — 東京 🐦')
    note_text = regular(base,profile_source).read_text()
    if 'M5 authored research note' not in note_text: raise ValueError('authored note body missing')
    check_citation_text(note_text, old['target'], old['content']['text'], A1_FULL_PASSAGE)
    for path,kind in ((seed+'10_selection_admitted.saved-sources.json','Selection'),
                      (seed+'redshank_note_admitted.saved-sources.json','Note'),
                      (reopen+'24_reopen_selection.saved-sources.json','Selection'),
                      (reopen+'25_reopen_note.saved-sources.json','Note')):
        check_retained_search(bound(path),owner,kind)
    for search,reader,kind in (('24_reopen_selection','24_reopen_selection_reader','Selection'),
                               ('25_reopen_note','25_reopen_note_reader','Note')):
        check_retained_reader(bound(reopen+search+'.saved-sources.json'),bound(reopen+reader+'.saved-sources.json'),owner,kind)
    # Reuse the reviewed negative checks with derived descriptors; their
    # evidence remains bound directly to the original collection ledgers.
    withdraw = bound(reopen+'withdrawal-inspection.json')
    historical = bound(seed+'withdraw_historical.structured-research.json')
    before_withdraw = bound(seed+'withdraw_before.structured-research.json')
    current_withdraw = bound(seed+'withdraw_fresh.structured-research.json')
    check_owner_inventory(withdraw)
    check_prewithdraw_scope(before_withdraw,original,owner)
    if not (historical['result'] == before_withdraw['result'] == withdraw['result'] and historical['content_manifest'] == before_withdraw['content_manifest'] == withdraw['manifest']):
        raise ValueError('withdrawal historical replay differs from owner')
    ids = [r['id'] for r in historical['result']['snapshot']['records'] if r['property'] == 'm5:withdraw']
    if len(ids) != 1: raise ValueError('withdrawal identity ambiguous')
    if withdraw['session'] != owner['session'] or withdraw['graph'] != owner['graph'] or withdraw['records'] != owner['records']:
        raise ValueError('withdrawal owner inventory differs from historical Sum owner')
    if {k for k,v in withdraw['records'].items() if v['withdrawn']} != set(ids):
        raise ValueError('graph owner withdrawal differs')
    if current_withdraw['session'] != owner['session'] or current_withdraw['source_selection'] != original['source_selection']:
        raise ValueError('current withdrawal scope differs')
    check_withdrawal_inventory(historical['result']['snapshot'],current_withdraw['result']['snapshot'],ids[0])
    no_match = bound(seed+'negative_no_match.structured-research.json')
    check_no_match(no_match,current_withdraw,owner['graph'])
    check_limit_refusal(bound(seed+'negative_over_limit.structured-status.json'),no_match,owner['graph'])
    f = bound(fault+'fault.json')
    virtual = {
      '@old-current':{'case':'old-current','frame':reopen+'23_reopen_A2.png','observation':reopen+'23_reopen_A2.saved-sources.json','old_observation':reopen+'22_reopen_A1.saved-sources.json','current_observation':reopen+'23_reopen_A2.saved-sources.json','owner_inspection':reopen+'structured-inspection.json'},
      '@scope-switch':{'case':'scope-switch','frame':seed+'scope_empty_before_run.png','observation':seed+'scope_empty_before_run.structured-status.json','before_observation':seed+'scope_before.structured-research.json','empty_search':seed+'scope_empty_search.saved-sources.json','fresh_observation':seed+'scope_empty_fresh.structured-research.json'},
      '@withdrawn':{'case':'withdrawn','frame':seed+'withdraw_history.png','observation':reopen+'withdrawal-inspection.json','record_id':ids[0],'historical_observation':seed+'withdraw_historical.structured-research.json','fresh_observation':seed+'withdraw_fresh.structured-research.json'},
      '@no-match':{'case':'no-match','frame':seed+'negative_no_match.png','observation':seed+'negative_no_match.structured-research.json'},
      '@over-limit':{'case':'over-limit','frame':seed+'negative_over_limit.png','observation':seed+'negative_over_limit.structured-status.json'},
      '@model-unavailable':{'case':'model-unavailable','frame':reopen+'model_unavailable.png','observation':reopen+'model_unavailable.runtime.json'},
      '@missing':{'case':'missing','frame':fault+'missing_A1_refusal.png','observation':fault+'missing_A1_open.saved-sources.json','exact_target':f['a1_target']},
    }
    def mixed(rel): return virtual[rel] if rel in virtual else bound(rel)
    legacy.verify_negatives({name:'@'+name for name in legacy.NEGATIVES}, mixed, files)
    case = {'case':'missing','run_id':manifest['fault_run_id'],'fault':fault+'fault.json',
       'intact_a1':reopen+'22_reopen_A1.saved-sources.json','intact_a2':reopen+'23_reopen_A2.saved-sources.json',
       'fault_a1':fault+'missing_A1_open.saved-sources.json','fault_a2':fault+'missing_A2_open.saved-sources.json',
       'before_observation':fault+'missing_before_click.saved-sources.json','all_scope_observation':fault+'missing_all_scope.saved-sources.json',
       'runtime':fault+'missing_runtime.runtime.json','intact_inspection':reopen+'structured-inspection.json',
       'primary_integrity':'fault/primary-integrity.json','graph_integrity':fault+'graph-integrity.json',
       'processes':fault+'processes.json','postcheck':fault+'fault-postcheck.json',
       'before_click_frame':fault+'missing_before_click.png','peek_frame':fault+'missing_A1_peek.png','refusal_frame':fault+'missing_A1_refusal.png','a2_frame':fault+'missing_A2_ready.png'}
    scope['verify_bound_case'](case, bound, files)
    review = bound('review.json')
    check_review_coverage(review, required_review_frames(), files)
    refuse_unreviewed_acceptance()


def main():
    import sys
    if len(sys.argv) != 2: raise SystemExit('usage: verify.py RECEIPT')
    try: verify_draft(Path(sys.argv[1]))
    except (ValueError,KeyError,TypeError,OSError,json.JSONDecodeError) as exc:
        print('FAIL: ' + str(exc),file=sys.stderr); raise SystemExit(1)

if __name__ == '__main__': main()
