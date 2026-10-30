#!/usr/bin/env python3
"""M5 receipt verifier draft. Fails closed until owner-specific checks land."""
import hashlib, json, sys
from pathlib import Path

OPERATIONS = {'SourceLookup', 'CountByProducer', 'CountBySource', 'SourceTimeline',
              'RecordTimeline', 'TagMembership', 'PropertyLookup', 'GroupValues',
              'JoinEntity', 'JoinSource', 'Compare', 'Sum'}
NEGATIVES = {'old-current', 'missing', 'withdrawn', 'scope-switch',
             'model-unavailable', 'over-limit', 'no-match'}

def require(value, message):
    if not value: raise ValueError(message)

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def safe(base, rel):
    require(isinstance(rel, str) and rel and not Path(rel).is_absolute(), 'invalid relative path')
    p = (base / rel).resolve()
    require(p.is_relative_to(base) and p.is_file(), 'missing or unsafe path: ' + rel)
    return p

def load(base, rel): return json.loads(safe(base, rel).read_text())

def verify_selection(full_text, quote, start, end, selected_only):
    # Python strings index Unicode scalars, matching the documented selector.
    require(type(start) is int and type(end) is int and 0 <= start < end <= len(full_text), 'invalid scalar range')
    require(full_text[start:end] == quote and quote, 'selected quote/range differs from actual source')
    require(selected_only == quote, 'onward selected payload contains unselected text or changed quote')

def verify_copied_selection(base, files, evidence, durable):
    selection = durable['selection']
    source_rel = Path(selection['source_relative_path'])
    require(not source_rel.is_absolute() and all(part not in ('.', '..') for part in source_rel.parts),
            'unsafe copied source path')
    source_file = evidence['copied_source']
    require(source_file in files and Path(source_file) == Path(evidence['inspection_root']) / source_rel,
            'copied source path is not bound to inspection root')
    package_file = evidence['exported_package']
    require(package_file in files, 'selected package is not bound')
    raw_source = safe(base, source_file).read_bytes()
    full_text = raw_source.decode('utf-8')
    package_raw = safe(base, package_file).read_bytes()
    package = json.loads(package_raw)
    require(package == selection['package'] and len(package['payloads']) == 1,
            'exported package differs from inspected selection')
    selected_bytes = bytes(package['payloads'][0]['bytes'])
    quote = selection['quote']
    verify_selection(full_text, quote, selection['scalar_start'], selection['scalar_end'], selected_bytes.decode('utf-8'))
    start, end = selection['byte_start'], selection['byte_end']
    require(type(start) is int and type(end) is int and 0 <= start < end <= len(raw_source)
            and raw_source[start:end] == selected_bytes == quote.encode('utf-8'),
            'selected bytes do not match exact copied source range')
    require(selection['source_hash'] and selection['admission'] and selection['clip'],
            'selection identity absent')
    for canary in (b'M5 private preface zebra', b'M5 private suffix otter'):
        require(canary in raw_source and canary not in package_raw and canary not in selected_bytes,
                'selected-only package disclosed private source text')

def verify_observation(obs, operation):
    require(obs['schema'] == 'turnstone.structured-observation/v1', 'wrong observation schema')
    result = obs['result']
    require(result['query']['operation'] == operation, 'operation identity mismatch')
    require(obs['result_generation'] and obs['session'] and result['snapshot']['scope_generation'], 'missing scope identity')
    require(obs['busy'] is False and result['operation_version'], 'busy or unversioned result')
    require(isinstance(result['snapshot']['complete'], bool), 'missing completeness')
    records = result['snapshot']['records']
    ids = [record['id'] for record in records]
    require(len(ids) == len(set(ids)), 'duplicate input assertion')
    for row in result['rows']:
        require(len(row['record_ids']) == len(set(row['record_ids'])), 'duplicate row contributor')
        require(set(row['record_ids']) <= set(ids), 'row refers to unknown input assertion')
    return result

def canon(value): return json.dumps(value, sort_keys=True, separators=(',', ':'))

def verify_distinct_comparison(left, right, definitions):
    same_identity = all(left[k] == right[k] for k in ('entity', 'property', 'schema_revision', 'context'))
    same_status = left['status'] is not None and left['status'] == right['status']
    a, b = left['valid_time'], right['valid_time']
    overlaps = a is not None and b is not None and a['start'] < b['end'] and b['start'] < a['end']
    definitions = [d for d in definitions
                   if d['id'] == left['property'] and d['revision'] == left['schema_revision']]
    require(len(definitions) == 1, 'distinct comparison definition missing or ambiguous')
    multiple_values = (same_identity and same_status and overlaps and left['value'] != right['value']
                       and definitions[0]['cardinality'] == 'Multiple')
    disjoint = same_identity and same_status and a is not None and b is not None and not overlaps
    require(not same_identity or disjoint or multiple_values, 'distinct comparison contradicts inputs')


def verify_disagreement(left, right, definitions):
    same_identity = all(left[k] == right[k] for k in ('entity', 'property', 'schema_revision', 'context'))
    same_status = left['status'] is not None and left['status'] == right['status']
    a, b = left['valid_time'], right['valid_time']
    overlaps = a is not None and b is not None and a['start'] < b['end'] and b['start'] < a['end']
    definitions = [d for d in definitions if d['id'] == left['property'] and d['revision'] == left['schema_revision']]
    require(len(definitions) == 1 and definitions[0]['cardinality'] == 'Single'
            and same_identity and same_status and overlaps and left['value'] != right['value'],
            'disagreement contradicts inputs or cardinality')


def verify_operation_semantics(observations):
    """M5 fixture checks, separate from the product evaluator."""
    for name in OPERATIONS:
        require(name in observations and observations[name], 'operation evidence absent: ' + name)
    def result(name):
        return observations[name][0]['result']
    lookup = result('SourceLookup')
    require(lookup['snapshot']['complete'], 'source scope incomplete')
    sources = lookup['snapshot']['sources']
    require({'capture', 'selection', 'note'} <= {source['producer'] for source in sources},
            'actual capture, selection and note producer kinds are required')
    source_ids = {canon(source['source']) for source in sources}
    require(len(source_ids) == len(sources), 'source snapshot contains duplicate projections')
    require({canon(row['key']['Source']) for row in lookup['rows']} == source_ids, 'source lookup omitted an input')
    kind = result('CountByProducer')
    expected_counts = {}
    for source in kind['snapshot']['sources']:
        producer = source['producer']; expected_counts[producer] = expected_counts.get(producer, 0) + 1
    actual_counts = {row['key']['Text']: row['count'] for row in kind['rows']}
    require(actual_counts == expected_counts, 'producer counts differ from input inventory')
    count_source = result('CountBySource')
    count_inputs = {record['id']: record for record in count_source['snapshot']['records']}
    contributed = []
    for row in count_source['rows']:
        ids = row['record_ids']; contributed.extend(ids)
        require(row['count'] == len(ids), 'per-source count differs from contributors')
        for record_id in ids:
            evidence = next(iter(count_inputs[record_id]['evidence'].values()))
            require(canon(evidence['source']) == canon(row['key']['Source']), 'source count contains another source')
    require(len(contributed) == len(set(contributed)) and set(contributed) == set(count_inputs),
            'source counts omitted or repeated assertions')
    timeline = result('SourceTimeline')
    by_source = {canon(source['source']): source for source in timeline['snapshot']['sources']}
    require(len(timeline['rows']) == len(by_source), 'source timeline omitted an input')
    for row in timeline['rows']:
        require(len(row['sources']) == 1, 'source timeline identity ambiguous')
        require(row['key'] == {'Time': by_source[canon(row['sources'][0])]['captured_at']}, 'source timeline time differs')
    timeline = result('RecordTimeline')
    by_record = {record['id']: record for record in timeline['snapshot']['records']}
    require(len(timeline['rows']) == len(by_record), 'record timeline omitted an assertion')
    for row in timeline['rows']:
        require(len(row['record_ids']) == 1 and row['key'] == {'Time': by_record[row['record_ids'][0]]['recorded_at']}, 'record timeline time differs')
    lookup = result('PropertyLookup')
    require({row['record_ids'][0] for row in lookup['rows'] if len(row['record_ids']) == 1} ==
            {record['id'] for record in lookup['snapshot']['records']}, 'property lookup omitted an assertion')
    group = result('GroupValues')
    require(any(len(row['record_ids']) >= 2 for row in group['rows']), 'equal group has no repeated contribution')
    records = {r['id']: r for r in result('JoinEntity')['snapshot']['records']}
    entity_rows = result('JoinEntity')['rows']
    require(entity_rows, 'no entity join exercised')
    for row in entity_rows:
        require(len(row['record_ids']) == 2 and records[row['record_ids'][0]]['entity'] == records[row['record_ids'][1]]['entity'], 'entity join is not explicit identity equality')
    source_records = {r['id']: r for r in result('JoinSource')['snapshot']['records']}
    source_rows = result('JoinSource')['rows']
    require(source_rows, 'no source join exercised')
    for row in source_rows:
        pair = [source_records[i] for i in row['record_ids']]
        require(len(pair) == 2, 'source join contribution count differs')
        left = next(iter(pair[0]['evidence'].values()))['source']
        right = next(iter(pair[1]['evidence'].values()))['source']
        require(canon(left) == canon(right), 'source join is not explicit identity equality')
    compared = result('Compare')
    comparisons = {row['key']['Comparison'] for row in compared['rows']}
    require({'Equal', 'Disagreement', 'Distinct', 'NonComparable'} <= comparisons, 'comparison classes incomplete')
    comparison_inputs = {record['id']: record for record in compared['snapshot']['records']}
    for row in compared['rows']:
        require(len(row['record_ids']) == 2, 'comparison requires two explicit assertions')
        left, right = (comparison_inputs[record_id] for record_id in row['record_ids'])
        same_identity = all(left[k] == right[k] for k in ('entity', 'property', 'schema_revision', 'context'))
        same_status = left['status'] is not None and left['status'] == right['status']
        a, b = left['valid_time'], right['valid_time']
        overlaps = a is not None and b is not None and a['start'] < b['end'] and b['start'] < a['end']
        if row['key']['Comparison'] == 'Equal':
            require(same_identity and same_status and overlaps and left['value'] == right['value'], 'equal comparison contradicts inputs')
        if row['key']['Comparison'] == 'Disagreement':
            verify_disagreement(left, right, compared['snapshot']['definitions'])
        if row['key']['Comparison'] == 'NonComparable':
            left_decimal = left['value'].get('Decimal')
            right_decimal = right['value'].get('Decimal')
            different_scale = (left_decimal is not None and right_decimal is not None
                               and left_decimal['scale'] != right_decimal['scale'])
            require(same_identity and (not same_status or a is None or b is None or (overlaps and different_scale)), 'noncomparable case not evidenced by fixture')
        if row['key']['Comparison'] == 'Distinct':
            verify_distinct_comparison(left, right, compared['snapshot']['definitions'])
    total = result('Sum')
    require(len(total['rows']) == 1 and total['rows'][0]['key'] == {'Value': {'Integer': 19}}, 'expected compatible sum 19')
    contributions = {r['id']: r for r in total['snapshot']['records']}
    ids = total['rows'][0]['record_ids']
    require(len(ids) == 2 and sorted(contributions[i]['value']['Integer'] for i in ids) == [7, 12], 'sum contributors differ')
    tag = result('TagMembership')
    require(tag['snapshot']['complete'] and tag['query']['filters']['tag'] and not tag['rows'], 'untagged absence is not bounded and complete')
    require(all(not source['tags'] for source in tag['snapshot']['sources']), 'untagged fixture assertion differs')

def verify_old_and_current(old, current, inspection):
    def reader_for(observation, marker):
        matches = [reader for reader in observation['readers']
                   if reader['phase'] == 'Ready' and reader['content'] is not None
                   and marker in reader['content']['text']]
        require(len(matches) == 1, 'expected one exact source reader: ' + marker)
        reader = matches[0]
        target, content = reader['target'], reader['content']
        require(target['text_hash'] == content['text_hash'], 'reader target and content hash differ')
        owners = [item for item in inspection['captures']
                  if item['admission']['key'] == target['key'] and item['content'] == content]
        require(len(owners) == 1, 'exact reader version absent from copied owner')
        return reader
    before = reader_for(old, 'M5 A1 archive')
    after = reader_for(current, 'M5 A2 archive')
    a, b = before['target'], after['target']
    require(all(a['key'][k] == b['key'][k] for k in ('session', 'graph', 'node')),
            'source versions are not from the same graph member')
    require(a['key']['annotation_manifest'] != b['key']['annotation_manifest']
            and a['text_hash'] != b['text_hash'], 'source version or content hash did not change')
    require(before['content']['captured_at_ms'] < after['content']['captured_at_ms'],
            'source version chronology was not established')


def verify_negatives(negative, bound, ledger):
    require(set(negative) == NEGATIVES, 'missing negative evidence')
    for case, path in negative.items():
        item = bound(path)
        require(item['case'] == case and item['frame'] in ledger, 'unbound negative frame: ' + case)
        bound(item['observation'])
    old_current = bound(negative['old-current'])
    verify_old_and_current(bound(old_current['old_observation']),
                           bound(old_current['current_observation']),
                           bound(old_current['owner_inspection']))
    no_match = bound(bound(negative['no-match'])['observation'])['result']
    require(no_match['snapshot']['complete'] and no_match['snapshot']['exclusion_count'] == 0 and not no_match['rows'],
            'no-match is not a complete empty result')
    over = bound(bound(negative['over-limit'])['observation'])
    require(over['schema'] == 'turnstone.structured-status/v1' and not over['busy'] and over['error'], 'limit refusal not observed')
    require(not any('No matching records in this scope' in row for row in over['visible_rows']), 'limit refusal presented as no match')
    switch = bound(negative['scope-switch'])
    switched = bound(switch['observation'])
    require(switched['schema'] == 'turnstone.structured-status/v1' and not switched['held_result'], 'scope switch retained an old result')
    require(not any('M5 foreign sentinel copperfield' in row for row in switched['visible_rows']), 'foreign row visible in scope')
    previous = bound(switch['before_observation'])
    search = bound(switch['empty_search'])['search']
    fresh = bound(switch['fresh_observation'])
    require(search['schema'] == 'turnstone.research-search-summary/v1' and search['selected'] == []
            and search['coverage'] == 'Complete' and search['scope'] == [] and search['total'] == 0
            and not search['inputs'] and not search['hits'], 'empty chosen search scope is not complete')
    require(previous['result']['snapshot']['sources'] and not fresh['result']['snapshot']['sources']
            and not fresh['result']['snapshot']['records'] and not fresh['result']['rows']
            and fresh['result']['snapshot']['complete'], 'fresh empty scope retained previous contributions')
    require(previous['session'] == search['session'] == fresh['session'], 'scope evidence crossed sessions')
    missing = bound(negative['missing'])
    observed = bound(missing['observation'])
    matches = [reader for reader in observed['readers'] if reader['target'] == missing['exact_target']]
    require(len(matches) == 1 and matches[0]['phase'] == 'Error' and matches[0]['content'] is None,
            'exact missing source did not refuse')
    withdrawal = bound(negative['withdrawn'])
    durable = bound(withdrawal['observation'])
    record = durable['records'][withdrawal['record_id']]
    require(record['withdrawn'] is True, 'graph owner does not mark withdrawn')
    old = bound(withdrawal['historical_observation'])
    fresh = bound(withdrawal['fresh_observation'])
    require(old['historical'] is True and withdrawal['record_id'] in {r['id'] for r in old['result']['snapshot']['records']},
            'withdrawn input not preserved in historical replay')
    require(withdrawal['record_id'] not in {r['id'] for r in fresh['result']['snapshot']['records']},
            'withdrawn input entered fresh result')
    runtime = bound(bound(negative['model-unavailable'])['observation'])
    require(runtime['inference']['client_compiled'] and runtime['inference']['generation_requests'] == 0,
            'generation client absent or request was sent')
    require(runtime['network_offline'] and runtime['offline_acquisition_attempts'] == 0, 'offline acquisition was attempted')
    # A model-load claim needs an independent custody-only resident/process
    # inventory, rather than substituting the client's unknown counter.
    require(runtime['inference']['resident_model_loads'] is None, 'invalid model counter contract')

def verify(base):
    base = base.resolve()
    manifest = load(base, 'manifest.json')
    require(manifest['schema'] == 'turnstone.model-free-m5-draft/v1', 'wrong receipt schema')
    files = manifest['files']
    require(isinstance(files, dict) and files, 'empty file ledger')
    for rel, expected in files.items():
        require(digest(safe(base, rel)) == expected, 'file hash mismatch ' + rel)
    def bound(rel):
        require(rel in files, 'unbound evidence ' + str(rel))
        return load(base, rel)
    processes = bound(manifest['processes'])
    seed, reopen = processes['seed'], processes['reopen']
    require(seed['exit_code'] == reopen['exit_code'] == 0, 'app failed')
    require(seed['pid_namespace'] != reopen['pid_namespace'] and seed['start_ns'] < seed['exit_ns'] < reopen['start_ns'] < reopen['exit_ns'], 'app process restart unproven')
    a, b = processes['custody_seed'], processes['custody_reopen']
    require(a['pid_namespace'] != b['pid_namespace'] and a['exit_ns'] < b['start_ns'], 'custody process restart unproven')
    require(a['public_identity'] and a['public_identity'] == b['public_identity'] and a['binary_sha256'] == b['binary_sha256'], 'custody identity or executable changed')
    require(processes['origin']['exit_ns'] < reopen['start_ns'], 'origin alive on reopen')
    require(processes['inspection']['start_ns'] > reopen['exit_ns'] and processes['inspection']['exit_code'] == 0, 'post-exit inspection missing')
    ops = manifest['operations']
    require(set(ops) == OPERATIONS, 'all twelve operations required')
    observed = {}
    for operation, paths in ops.items():
        require(isinstance(paths, list) and paths, 'missing operation observation: ' + operation)
        observed[operation] = [bound(path) for path in paths]
        for value in observed[operation]: verify_observation(value, operation)
    verify_operation_semantics(observed)
    replay = bound(manifest['replay'])
    durable = bound(manifest['durable_inspection'])
    seed_result = bound(replay['seed_observation'])['result']
    cold_result = bound(replay['cold_observation'])['result']
    require(seed_result == cold_result == durable['result'], 'replay/durable result changed')
    require(durable['manifest'] == replay['manifest'], 'durable manifest differs')
    seed_observation, cold_observation = bound(replay['seed_observation']), bound(replay['cold_observation'])
    require(seed_observation['source_selection']['mode'] in ('all', 'chosen')
            and cold_observation['source_selection'] == {'mode': 'unknown-historical'},
            'source selection scope is unavailable or misrepresented')
    require(seed_observation['content_manifest'] == cold_observation['content_manifest'], 'replay identity changed')
    require(cold_observation['historical'] is True, 'replay not marked historical')
    evidence = bound(manifest['selection'])
    verify_copied_selection(base, files, evidence, durable)
    negative = manifest['negatives']
    verify_negatives(negative, bound, files)
    reviews = bound(manifest['headed_review'])
    require(reviews['reviewer'] and reviews['reviewed_frames'] and all(p in files for p in reviews['reviewed_frames']), 'headed review missing or unbound')
    # Presence and equality checks cannot replace owner-specific independent
    # validation of source version/hash and private package bytes, semantic
    # outcomes per operation, route namespace, counter instrumentation, and
    # explicit negative outcome wording. No receipt may pass until implemented.
    raise ValueError('DRAFT ONLY: owner-specific evidence, semantic outcomes, isolation and negative controls are not fully verified')

def main():
    if len(sys.argv) != 2: raise SystemExit('usage: verify_receipt.py RECEIPT_DIR')
    try: verify(Path(sys.argv[1]))
    except (ValueError, KeyError, TypeError, OSError, json.JSONDecodeError) as exc:
        print('FAIL: ' + str(exc), file=sys.stderr); raise SystemExit(1)
if __name__ == '__main__': main()
