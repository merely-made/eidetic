import copy, json, os, tempfile, unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import fault_phase
import run_missing as run
import verify_missing as verify


def evidence():
    key = {'session':'s','graph':'g','node':'n','annotation_manifest':'sealed-v1:a'}
    a = {'key':key, 'occurrence':0, 'error':None}
    b = {'key':dict(key, annotation_manifest='sealed-v1:b'), 'occurrence':1, 'error':None}
    t1 = {'key':key,'text_hash':'ta','selector':{'start':0,'end':1}}
    t2 = {'key':b['key'],'text_hash':'tb','selector':{'start':0,'end':1}}
    c1 = {'text':'M5 A1 archive','text_hash':'ta','raw_manifest':'raw-a'}
    c2 = {'text':'M5 A2 archive','text_hash':'tb','raw_manifest':'raw-b'}
    r1 = {'node':'reader-a','target':t1,'phase':'Ready','content':c1}
    r2 = {'node':'reader-b','target':t2,'phase':'Ready','content':c2}
    citation = {'session':'s','admissions':[a],'readers':[r1]}
    old = {'session':'s','admissions':[a,b],'readers':[r1]}
    current = {'session':'s','admissions':[a,b],'readers':[r2]}
    owner = {'schema':'turnstone.m5-store-inspection/v1','session':'s','graph':'g',
             'sealed_capture_receipts':{'schema':'turnstone.m5-sealed-capture-receipts/v1','count':2,'blake3':'b'*64},
             'captures':[{'admission':a,'content':c1},{'admission':b,'content':c2}]}
    return citation, old, current, owner


def click_observations(data):
    common = {'session':'s','generation':'generation-1','admissions':copy.deepcopy(data['all_admissions']),
              'reader_scope':{'schema':'turnstone.m5-reader-scope/v1','selected':None}}
    addresses = {'schema':'turnstone.m5-address-events/v1','total':0,'dropped':0,'overflow':False,'events':[]}
    reader_opens = {'schema':'turnstone.m5-reader-open-events/v1','total':0,'dropped':0,'overflow':False,'events':[]}
    all_scope = dict(copy.deepcopy(common), search={'schema':'turnstone.research-search-summary/v1','session':'s','selected':None})
    before = dict(copy.deepcopy(common), readers=[], address_events=addresses, reader_open_events=reader_opens)
    after = dict(copy.deepcopy(common), readers=[{'node':'reader-a','target':data['a1_target'],'phase':'Error','content':None,'error':'missing'}],
                 address_events=copy.deepcopy(addresses),
                 reader_open_events=dict(reader_opens,total=1,events=[[1,data['a1_target']]]))
    return all_scope, before, after


class Binding(unittest.TestCase):
    def test_citation_can_predate_a2_but_must_match(self):
        parts = evidence()
        result = run.bind_input('run', *parts)
        self.assertEqual(result['a1_target'], parts[0]['readers'][0]['target'])
        wrong = copy.deepcopy(parts[0]); wrong['readers'][0]['target']['text_hash'] = 'changed'
        with self.assertRaises(ValueError): run.bind_input('run', wrong, *parts[1:])

    def test_shared_payload_and_omitted_owner_fail(self):
        citation, old, current, owner = evidence()
        current['readers'][0]['content']['raw_manifest'] = 'raw-a'
        owner['captures'][1]['content']['raw_manifest'] = 'raw-a'
        with self.assertRaises(ValueError): run.bind_input('run', citation, old, current, owner)
        citation, old, current, owner = evidence(); owner['captures'].pop()
        with self.assertRaises(ValueError): run.bind_input('run', citation, old, current, owner)

    def test_exact_error_and_unchanged_a2(self):
        data = run.bind_input('run', *evidence())
        all_scope, before, error = click_observations(data)
        ready = {'session':'s','readers':[{'node':'reader-b','target':data['a2_target'],'phase':'Ready','content':data['a2_content']}]}
        runtime = {'schema':'turnstone.model-free-runtime/v1','session':'s','network_offline':True,'offline_acquisition_attempts':0}
        fault_phase.validate_gui(data, all_scope, before, error, ready, runtime)
        wrong = copy.deepcopy(error); wrong['readers'][0]['target']['selector'] = None
        with self.assertRaises(ValueError): fault_phase.validate_gui(data, all_scope, before, wrong, ready, runtime)
        wrong = copy.deepcopy(ready); wrong['readers'][0]['content']['text'] = 'fallback'
        with self.assertRaises(ValueError): fault_phase.validate_gui(data, all_scope, before, error, wrong, runtime)
        runtime['offline_acquisition_attempts'] = 1
        with self.assertRaises(ValueError): fault_phase.validate_gui(data, all_scope, before, error, ready, runtime)

    def test_click_interval_rejects_redacted_extra_lost_or_fallback(self):
        data = run.bind_input('run', *evidence())
        _, before, after = click_observations(data)
        verify.verify_click_interval(before, after, data['a1_target'])
        for replacement in (None, {'other':'target'}):
            changed = copy.deepcopy(after); changed['reader_open_events']['events'][0][1] = replacement
            with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        changed = copy.deepcopy(after); changed['reader_open_events']['events'].append([2,None]); changed['reader_open_events']['total'] = 2
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        for field in ('address_events','reader_open_events'):
            changed = copy.deepcopy(after); changed[field]['overflow'] = True
            with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
            changed = copy.deepcopy(after); changed[field]['schema'] = 'wrong'
            with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        changed = copy.deepcopy(after); changed['reader_open_events'] = copy.deepcopy(before['reader_open_events'])
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        changed = copy.deepcopy(after); changed['address_events']['total'] = 1; changed['address_events']['events'] = [[1,None]]
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        changed = copy.deepcopy(after); changed['reader_open_events']['events'][0][0] = True
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        full_before, dropped_after = copy.deepcopy(before), copy.deepcopy(after)
        full_before['reader_open_events'] = {'schema':'turnstone.m5-reader-open-events/v1','total':128,'dropped':0,'overflow':False,
                                              'events':[[i,None] for i in range(1,129)]}
        dropped_after['reader_open_events'] = {'schema':'turnstone.m5-reader-open-events/v1','total':129,'dropped':1,'overflow':False,
                                                'events':[[i,None] for i in range(2,129)] + [[129,data['a1_target']]]}
        with self.assertRaises(ValueError): verify.verify_click_interval(full_before, dropped_after, data['a1_target'])
        full_before, dropped_after = copy.deepcopy(before), copy.deepcopy(after)
        already_dropped = {'schema':'turnstone.m5-address-events/v1','total':129,'dropped':1,'overflow':False,
                           'events':[[i,None] for i in range(2,130)]}
        full_before['address_events'] = already_dropped; dropped_after['address_events'] = copy.deepcopy(already_dropped)
        with self.assertRaises(ValueError): verify.verify_click_interval(full_before, dropped_after, data['a1_target'])
        changed = copy.deepcopy(after); changed['readers'].append({'node':'fallback','phase':'Ready','target':data['a2_target'],'content':data['a2_content']})
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        stale = copy.deepcopy(before); stale['readers'].append({'node':'old-a1','phase':'Error','target':data['a1_target'],'content':None})
        with self.assertRaises(ValueError): verify.verify_click_interval(stale, after, data['a1_target'])

    def test_target_address_uses_exact_canonical_identity_and_selector(self):
        target = {'key':{'session':'00000000-0000-0000-0000-000000000001',
                         'graph':'00000000-0000-0000-0000-000000000002',
                         'node':'00000000-0000-0000-0000-000000000003',
                         'annotation_manifest':'sealed-v1:blake3:'+'a'*64},
                  'text_hash':'blake3:'+'b'*64, 'selector':{'start':2,'end':5}}
        self.assertEqual(verify.target_address(target), 'mere://saved-source/sealed-v1/' +
                         '/'.join(target['key'][k] for k in ('session','graph','node')) + '/' + 'a'*64 + '/' + 'b'*64 + '?start=2&end=5')
        target['selector']['end'] = 2
        with self.assertRaises(ValueError): verify.target_address(target)

    def test_hidden_fallback_scope_is_rejected_even_when_reader_is_filtered_out(self):
        data = run.bind_input('run', *evidence())
        all_scope, before, after = click_observations(data)
        verify.verify_all_scope(all_scope, data['a1_target'], before)
        verify.verify_click_interval(before, after, data['a1_target'])
        # A filtered observation may omit an alternate reader entirely. It
        # cannot pass merely because both visible reader lists look benign.
        for which in ('before','after'):
            changed_before, changed_after = copy.deepcopy(before), copy.deepcopy(after)
            (changed_before if which == 'before' else changed_after)['reader_scope']['selected'] = 'restricted'
            with self.assertRaises(ValueError): verify.verify_click_interval(changed_before, changed_after, data['a1_target'])
        changed = copy.deepcopy(all_scope); changed['search']['selected'] = []
        with self.assertRaises(ValueError): verify.verify_all_scope(changed, data['a1_target'], before)
        changed = copy.deepcopy(after); changed['generation'] = 'other'
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])
        changed = copy.deepcopy(after); changed['admissions'][0]['key']['graph'] = 'foreign'
        with self.assertRaises(ValueError): verify.verify_click_interval(before, changed, data['a1_target'])

    def test_receipt_verifier_requires_owner_and_primary_invariants(self):
        citation, old, current, owner = evidence(); inp = run.bind_input('run', citation, old, current, owner)
        a1, a2 = inp['a1_target'], inp['a2_target']
        fault = {'schema':'turnstone.m5-missing-fault/v1','run_id':'run','a1_target':a1,'a2_target':a2,
                 'a1_raw_manifest':'raw-a','a2_raw_manifest':'raw-b','deleted_ciphertext_hash':'blake3:'+'a'*64,
                 'capture_receipts_hash':'blake3:'+'b'*64,'manifest_preserved':True,'capture_receipts_preserved':True,
                 'a1_raw_absent_after_reopen':True,'a1_owner_refused':True,'a2_owner_unchanged':True}
        all_scope, before_click, error = click_observations(inp)
        ready = {'session':'s','readers':[{'node':'reader-b','target':a2,'phase':'Ready','content':inp['a2_content']}]}
        runtime = {'schema':'turnstone.model-free-runtime/v1','session':'s','network_offline':True,'offline_acquisition_attempts':0}
        values = {'fault':fault,'old':old,'current':current,'error':error,'ready':ready,'runtime':runtime,'inspection':owner,'before_click':before_click,'all_scope':all_scope,
                  'primary':{'unchanged':True,'before':{'a':'b'},'after':{'a':'b'}},
                  'graph':{'unchanged':True,'before':{'g':'h'},'after':{'g':'h'}},
                  'postcheck':{'schema':'turnstone.m5-missing-postcheck/v1','run_id':'run','capture_receipts_hash':'blake3:'+'b'*64,
                               'capture_receipts_preserved':True,'a1_raw_absent':True,'a1_owner_refused':True,'a2_owner_unchanged':True},
                  'processes':{'fault':{'exit_code':0,'exit_ns':2},'postcheck':{'exit_code':0,'start_ns':3,'exit_ns':4},
                               'app':{'exit_code':0,'start_ns':5,'exit_ns':6},'custody':{'exit_ns':7}}}
        case = {'case':'missing','run_id':'run','fault':'fault','intact_a1':'old','intact_a2':'current',
                'fault_a1':'error','fault_a2':'ready','before_observation':'before_click','all_scope_observation':'all_scope','runtime':'runtime','intact_inspection':'inspection','primary_integrity':'primary',
                'graph_integrity':'graph','processes':'processes','postcheck':'postcheck','before_click_frame':'f1','peek_frame':'peek','refusal_frame':'f2','a2_frame':'f3'}
        fault_phase.validate_fault(fault, inp)
        run.verify_bound_case(case, values.__getitem__, {'f1','peek','f2','f3'})
        with self.assertRaises(ValueError): run.verify_bound_case(case, values.__getitem__, {'f1','f2','f3'})
        bad = copy.deepcopy(fault); bad['deleted_ciphertext_hash'] = 'a'*64
        with self.assertRaises(ValueError): fault_phase.validate_fault(bad, inp)
        values['fault']['capture_receipts_preserved'] = False
        with self.assertRaises(ValueError): run.verify_bound_case(case, values.__getitem__, {'f1','peek','f2','f3'})
        values['fault']['capture_receipts_preserved'] = True; values['primary']['after']['a'] = 'changed'
        with self.assertRaises(ValueError): run.verify_bound_case(case, values.__getitem__, {'f1','peek','f2','f3'})


class Isolation(unittest.TestCase):
    def test_collection_ledger_refuses_changed_observation_and_primary(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); profile=root/'sandbox/profile'; profile.mkdir(parents=True); (profile/'graph').write_text('graph')
            paths = ('inputs.json','processes.json','sandbox/seed/06_note_citation.saved-sources.json',
                'sandbox/reopen/22_reopen_A1.saved-sources.json','sandbox/reopen/23_reopen_A2.saved-sources.json',
                'sandbox/reopen/structured-inspection.json','sandbox/reopen/withdrawal-inspection.json',
                'sandbox/reopen/inspection-source-hashes.json','sandbox/reopen/inspection-integrity.json',
                'sandbox/reopen/withdrawal_inspection-integrity.json','sandbox/origin-info.json',
                'seed-launcher.cleanup.json','reopen-launcher.cleanup.json')
            for name in paths:
                p=root/name; p.parent.mkdir(parents=True,exist_ok=True); p.write_text('{}')
            (root/'sandbox/reopen/inspection-source-hashes.json').write_text(json.dumps(run.regular_tree(profile)))
            collection={'files':{name:run.native.sha(root/name) for name in paths},
                        'primary_profile':{'root':'sandbox/profile','inventory':'sandbox/reopen/inspection-source-hashes.json'}}
            run.validate_collection_ledger(root,collection)
            observation=root/'sandbox/reopen/22_reopen_A1.saved-sources.json'; observation.write_text('{"changed":true}')
            with self.assertRaises(ValueError): run.validate_collection_ledger(root,collection)
            observation.write_text('{}'); (profile/'graph').write_text('changed')
            with self.assertRaises(ValueError): run.validate_collection_ledger(root,collection)

    def test_registration_lock_and_both_build_inventories(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)/'collection'; (root/'frozen').mkdir(parents=True)
            source=Path(d)/'source'; source.mkdir()
            texts = {
                'src/trail_memory/m5_inspection.rs':'mod missing_fault; fn inject_missing_capture_copy() { missing_fault::inject(); } fn verify_missing_capture_copy() { missing_fault::verify_preservation(); }',
                'src/trail_memory/research_owner_tests.rs':'fn model_free_inject_missing_capture_copy() { m5_inspection::inject_missing_capture_copy(); } fn model_free_verify_missing_capture_copy() { m5_inspection::verify_missing_capture_copy(); }',
                'src/shell/mod.rs':'mod citation_event_history;', 'src/shell/shell.rs':'address_event_history: reader_event_history:', 'src/shell/shell_new.rs':'address_event_history: reader_event_history:',
                'src/shell/effects/shell_refresh_diagnostic_inspection.rs':'event_address( address_event_history.record( reader_event_history.record( capture_selected( TURNSTONE_M5_EXPECTED_A1_URI',
                'src/shell/drive/shell_scenario_assert_no_surface.rs':'"reader_open_events": reader_event_history.reader_observation(&selected "address_events": "reader_scope": turnstone.m5-reader-scope/v1 address_event_history.observation(&selected, saved_source_admitted(key)',
            }
            here = Path(run.__file__).parent
            texts['src/trail_memory/fault_helper.rs'] = (here/'fault_helper.rs').read_text()
            texts['src/shell/citation_event_history.rs'] = (here/'citation_event_history.rs').read_text()
            for path in run.SOURCE_PATHS:
                full=source/path; full.parent.mkdir(parents=True,exist_ok=True); full.write_text(texts.get(path,'source'))
            hashes={p:run.native.sha(source/p) for p in run.SOURCE_PATHS}
            inventory={'head':'commit','files':{p:{'sha256':h} for p,h in hashes.items()}}
            for name in ('turnstone','test-binary'):
                (root/'frozen'/f'{name}-source-inventory.json').write_text(json.dumps(inventory))
            lock=Path(d)/'lock.json'; lock.write_text(json.dumps({'schema':'turnstone.m5-missing-registered-source/v1','files':hashes}))
            run.verify_source_registration(root,source,lock)
            (source/'src/shell/shell.rs').write_text('address_event_history: changed')
            with self.assertRaises(ValueError): run.verify_source_registration(root,source,lock)
            (source/'src/shell/shell.rs').write_text(texts['src/shell/shell.rs'])
            changed=copy.deepcopy(inventory); changed['files']['src/shell/shell.rs']['sha256']='0'*64
            (root/'frozen/test-binary-source-inventory.json').write_text(json.dumps(changed))
            with self.assertRaises(ValueError): run.verify_source_registration(root,source,lock)

    def test_copy_rejects_links_and_is_independent(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); src = root/'source'; src.mkdir(); (src/'a').write_text('one')
            run.copy_tree(src, root/'copy')
            (root/'copy/a').write_text('two'); self.assertEqual((src/'a').read_text(),'one')
            (src/'link').symlink_to(src/'a')
            with self.assertRaises(run.phase.TreeReadError): run.copy_tree(src, root/'next')
            (src/'link').unlink(); os.link(src/'a',src/'hard')
            with self.assertRaises(run.phase.TreeReadError): run.copy_tree(src, root/'last')

    def test_command_exposes_only_named_copy_and_frozen_inputs(self):
        devices = {'wayland':{'path':'/display'},'render':{'path':'/dev/dri/renderD128'},
                   'sysfs':{'root':'/sys/devices/pci/dev','target':'/sys/devices/pci/dev/drm/renderD128','devnum':'226:128'}}
        cmd = run.namespace_command(Path('/copy'),Path('/frozen'),Path('/output'),devices)
        self.assertNotIn('/primary',cmd); self.assertNotIn('/frozen/fixtures',cmd)
        self.assertIn('/copy/profile',cmd); self.assertIn('/m5/run/profile',cmd)
        self.assertEqual(cmd.count('/copy/profile'),2)
        self.assertNotIn('/frozen', [cmd[i+1] for i,v in enumerate(cmd[:-1]) if v == '--ro-bind'])
        scope = run.native.cgroup_scope(cmd,3*1024**3,128,150,'m5-test',600)
        for item in ('MemoryMax=3221225472','MemorySwapMax=0','TasksMax=128','CPUQuota=150%','RuntimeMaxSec=600s'):
            self.assertIn(item,scope)

    def test_primary_hash_finally_on_collection_failure(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)/'collection'; root.mkdir(); (root/'evidence').write_text('intact')
            out = Path(d)/'output'
            args = SimpleNamespace(collection=root, output=out, wayland_socket=Path('/x'), gpu_device=Path('/y'))
            with patch.object(run,'collection_evidence',side_effect=ValueError('simulated')):
                with self.assertRaises(ValueError): run.execute(args)
            state = json.loads((out/'primary-integrity.json').read_text())
            self.assertTrue(state['unchanged']); self.assertTrue((out/'failure.json').is_file())

    def test_primary_change_on_failure_is_recorded(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)/'collection'; root.mkdir(); path=root/'evidence'; path.write_text('before')
            out=Path(d)/'output'; args=SimpleNamespace(collection=root,output=out,wayland_socket=Path('/x'),gpu_device=Path('/y'))
            def change(_): path.write_text('after'); raise ValueError('simulated')
            with patch.object(run,'collection_evidence',side_effect=change):
                with self.assertRaises(ValueError): run.execute(args)
            self.assertFalse(json.loads((out/'primary-integrity.json').read_text())['unchanged'])
            self.assertTrue((out/'primary-integrity-failure.json').is_file())

if __name__ == '__main__': unittest.main()
