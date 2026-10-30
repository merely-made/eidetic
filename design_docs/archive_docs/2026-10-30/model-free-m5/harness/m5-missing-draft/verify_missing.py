"""Staged integration function. `bound(path)` reads only hash-ledger-bound JSON.

The caller must separately validate every process, mount, screenshot and tree
receipt in report.md; this function alone is not an M5 acceptance verifier.
"""
import uuid


def require(value, message):
    if not value:
        raise ValueError(message)


def target_address(target):
    key, selector = target['key'], target['selector']
    for name in ('session','graph','node'):
        require(str(uuid.UUID(key[name])) == key[name], 'noncanonical target UUID')
    annotation, text = key['annotation_manifest'], target['text_hash']
    require(annotation.startswith('sealed-v1:blake3:') and text.startswith('blake3:'), 'target hash scheme')
    ann, text = annotation.removeprefix('sealed-v1:blake3:'), text.removeprefix('blake3:')
    require(all(len(s) == 64 and all(c in '0123456789abcdef' for c in s) for s in (ann,text)), 'target hash encoding')
    require(selector is not None and type(selector['start']) is int and type(selector['end']) is int
            and 0 <= selector['start'] < selector['end'], 'invalid scalar selector')
    return ('mere://saved-source/sealed-v1/' + key['session'] + '/' + key['graph'] + '/' + key['node'] + '/'
            + ann + '/' + text + '?start=' + str(selector['start']) + '&end=' + str(selector['end']))


def verify_click_interval(before, after, target, raw_manifest):
    key = target['key']
    require(before['session'] == after['session'] == key['session'] and before['generation'] == after['generation']
            and before['generation'], 'click crossed session or source generation')
    require(before['admissions'] == after['admissions'] and before['admissions'], 'capture graph scope changed')
    for observation in (before, after):
        scope = observation['reader_scope']
        require(scope['schema'] == 'turnstone.m5-reader-scope/v1' and 'selected' in scope and scope['selected'] is None,
                'reader scope is not all admitted')
        require(all(item['key']['session'] == key['session'] and item['key']['graph'] == key['graph']
                    for item in observation['admissions']), 'capture admission crossed graph scope')
        require(all(item['target']['key']['session'] == key['session'] and item['target']['key']['graph'] == key['graph']
                    for item in observation['readers']), 'reader crossed graph scope')
    require(any(item['key'] == key for item in before['admissions']), 'A1 absent from all-admitted graph')
    prior = [reader for reader in before['readers'] if reader['target'] == target]
    current = [reader for reader in after['readers'] if reader['target'] == target]
    require(len(prior) <= 1 and len(current) == 1, 'A1 reader missing or ambiguous')
    reader = current[0]
    missing = 'sealed source custody or integrity refusal: no local source resolved the sealed blob for manifest ' + raw_manifest
    fields = {'phase','content','engine','rendered_document','error','url','node','target'}
    require(fields <= reader.keys(), 'A1 reader fields missing')
    require(reader['phase'] == 'Error' and reader['content'] is None and reader['engine'] is None
            and reader['rendered_document'] is None and reader['error'] == missing
            and reader['url'] == target_address(target), 'exact missing-blob refusal absent')
    if prior:
        previous_reader = prior[0]
        require(fields <= previous_reader.keys(), 'prior A1 reader fields missing')
        require(previous_reader['phase'] == 'Error' and previous_reader['content'] is None
                and previous_reader['engine'] is None and previous_reader['rendered_document'] is None
                and previous_reader['error'] == 'Unlock the profile to read private saved sources'
                and previous_reader['node'] == reader['node'] and previous_reader['url'] == reader['url'],
                'pre-existing A1 reader is not the exact stale locked-profile refusal')
    def history(obs, field, schema):
        h = obs[field]; events = h['events']; total, dropped = h['total'], h['dropped']
        require(h['schema'] == schema and h['overflow'] is False
                and type(total) is int and type(dropped) is int and dropped == 0
                and isinstance(events,list) and dropped == max(0, total - 128) and len(events) == total - dropped,
                field + ' history incomplete')
        require(all(isinstance(e,list) and len(e) == 2 and type(e[0]) is int and e[0] == dropped+i+1
                    and (e[1] is None or isinstance(e[1],dict)) for i,e in enumerate(events)), field + ' sequence invalid')
        return h
    addresses_before = history(before, 'address_events', 'turnstone.m5-address-events/v1')
    addresses_after = history(after, 'address_events', 'turnstone.m5-address-events/v1')
    require(addresses_before == addresses_after, 'unexpected address open during citation and Read')
    start = history(before, 'reader_open_events', 'turnstone.m5-reader-open-events/v1')
    end = history(after, 'reader_open_events', 'turnstone.m5-reader-open-events/v1')
    require(end['dropped'] == start['dropped'] and end['total'] == start['total'] + 1,
            'citation and Read interval has dropped, missing or extra reader opens')
    require(end['events'][:-1] == start['events'] and end['events'][-1][1] == target,
            'ordinary Read did not open the exact A1 target')
    def ready(obs):
        result = {}
        for reader in obs['readers']:
            if reader['phase'] != 'Ready': continue
            node = reader['node']
            require(node not in result, 'duplicate reader node')
            result[node] = (reader['target'],reader['content'])
        return result
    previous = ready(before)
    require(all(node in previous and previous[node] == identity for node,identity in ready(after).items()),
            'a new or changed fallback reader became ready during the click')


def verify_all_scope(scope_observation, target, before_click):
    search = scope_observation['search']
    require(isinstance(search,dict) and search.get('schema') == 'turnstone.research-search-summary/v1'
            and 'selected' in search and search['selected'] is None, 'ordinary All admitted selection was not observed')
    require(scope_observation['reader_scope']['schema'] == 'turnstone.m5-reader-scope/v1'
            and scope_observation['reader_scope']['selected'] is None,
            'search and reader selections differ')
    require(scope_observation['session'] == search['session'] == before_click['session'] == target['key']['session']
            and scope_observation['generation'] == before_click['generation'], 'scope observation crossed session or generation')
    require(scope_observation['admissions'] == before_click['admissions']
            and all(item['key']['session'] == target['key']['session']
                    and item['key']['graph'] == target['key']['graph'] for item in scope_observation['admissions']),
            'All admitted selection belongs to a different graph scope')


def verify_missing(case, bound, ledger):
    require(case['case'] == 'missing', 'wrong case')
    fault = bound(case['fault'])
    old = bound(case['intact_a1'])
    current = bound(case['intact_a2'])
    failed = bound(case['fault_a1'])
    before_click = bound(case['before_observation'])
    all_scope = bound(case['all_scope_observation'])
    survived = bound(case['fault_a2'])
    runtime = bound(case['runtime'])
    inspection = bound(case['intact_inspection'])
    require(fault['schema'] == 'turnstone.m5-missing-fault/v1', 'fault receipt schema')
    require(fault['run_id'] == case['run_id'], 'fault run identity')
    require(fault['a1_raw_manifest'] != fault['a2_raw_manifest'], 'shared raw payload')
    require(all(fault[k] is True for k in ('manifest_preserved', 'capture_receipts_preserved', 'a1_raw_absent_after_reopen',
                                            'a1_owner_refused', 'a2_owner_unchanged')), 'owner fault incomplete')
    digest = fault['deleted_ciphertext_hash']
    require(isinstance(digest, str) and digest.startswith('blake3:') and len(digest) == 71
            and all(c in '0123456789abcdef' for c in digest[7:]), 'deleted bytes hash absent')
    digest = fault['capture_receipts_hash']
    require(isinstance(digest, str) and digest.startswith('blake3:') and len(digest) == 71
            and all(c in '0123456789abcdef' for c in digest[7:]), 'receipt hash absent')
    baseline = inspection['sealed_capture_receipts']
    require(inspection['schema'] == 'turnstone.m5-store-inspection/v1' and inspection['session'] == old['session']
            and baseline['schema'] == 'turnstone.m5-sealed-capture-receipts/v1'
            and type(baseline['count']) is int and baseline['count'] > 0
            and digest == 'blake3:' + baseline['blake3'], 'intact receipt baseline differs')

    def reader(obs, target, phase):
        found = [r for r in obs['readers'] if r['target'] == target]
        require(len(found) == 1 and found[0]['phase'] == phase, 'exact reader phase missing or ambiguous')
        return found[0]

    a1, a2 = fault['a1_target'], fault['a2_target']
    verify_all_scope(all_scope, a1, before_click)
    verify_click_interval(before_click, failed, a1, fault['a1_raw_manifest'])
    require(inspection['graph'] == a1['key']['graph'] == a2['key']['graph'], 'inspected graph differs')
    before1, before2 = reader(old, a1, 'Ready'), reader(current, a2, 'Ready')
    refusal, after2 = reader(failed, a1, 'Error'), reader(survived, a2, 'Ready')
    for target, original in ((a1,before1),(a2,before2)):
        owners = [item for item in inspection['captures'] if item['admission']['key'] == target['key']
                  and item['content'] == original['content']]
        require(len(owners) == 1 and original['content']['text_hash'] == target['text_hash'],
                'exact intact owner version missing or ambiguous')
    require(before1['content']['raw_manifest'] == fault['a1_raw_manifest'], 'A1 owner identity differs')
    require(before2['content']['raw_manifest'] == fault['a2_raw_manifest'], 'A2 owner identity differs')
    require(refusal['content'] is None and refusal['error'], 'A1 returned content or lacked a refusal')
    require(after2['content'] == before2['content'], 'A2 content changed')
    require(all(obs['session'] == old['session'] for obs in (current, before_click, failed, survived)), 'session changed')
    require(a1['key']['annotation_manifest'] != a2['key']['annotation_manifest'], 'versions equal')
    require(a1['text_hash'] != a2['text_hash'] and a1['selector'] is not None, 'A1 exact selector/version absent')
    require(runtime['schema'] == 'turnstone.model-free-runtime/v1' and runtime['session'] == old['session'], 'runtime identity')
    require(runtime['network_offline'] is True and runtime['offline_acquisition_attempts'] == 0,
            'offline fetch attempts or network availability')
    require(all(case[k] in ledger for k in ('before_click_frame', 'peek_frame', 'refusal_frame', 'a2_frame')),
            'headed frames are not ledger-bound')
