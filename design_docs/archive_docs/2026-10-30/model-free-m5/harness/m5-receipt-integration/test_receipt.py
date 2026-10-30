import hashlib
import importlib
import json
from pathlib import Path
import tempfile
import unittest


def module(name):
    try:
        return importlib.import_module(name)
    except ModuleNotFoundError as exc:
        if exc.name == name:
            raise AssertionError('receipt integration module missing') from exc
        raise


def sha(data):
    return hashlib.sha256(data).hexdigest()


class ReceiptChecks(unittest.TestCase):
    def test_ledger_refuses_changed_file(self):
        verify = module('verify')
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'evidence.json').write_bytes(b'new')
            with self.assertRaises(ValueError):
                verify.validate_ledger(root, {'evidence.json': sha(b'old')}, {'evidence.json'})

    def test_ledger_rejects_parent_symlink_and_missing_required_file(self):
        verify = module('verify')
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / 'root'; root.mkdir()
            outside = Path(d) / 'outside'; outside.mkdir()
            (outside / 'x').write_bytes(b'x')
            (root / 'link').symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError):
                verify.validate_ledger(root, {'link/x': sha(b'x')}, {'link/x'})
            with self.assertRaises(ValueError):
                verify.validate_ledger(root, {}, {'missing'})

    def test_module_hash_mismatch_refuses_before_code_runs(self):
        verify = module('verify')
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'module.py'
            path.write_text("raise RuntimeError('executed')\n")
            with self.assertRaises(ValueError):
                verify.load_pinned_module(path, sha(b'different'), 'frozen_test')

    def test_receipt_rejects_missing_or_wrong_schema(self):
        verify = module('verify')
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for manifest in ({}, {'schema':'turnstone.m5-receipt/v1','status':'draft'},
                             {'schema':'wrong','status':'collected-awaiting-verification'}):
                (root/'manifest.json').write_text(json.dumps(manifest))
                with self.assertRaises(ValueError): verify.verify_receipt(root)

    def test_missing_fault_file_ledger_refuses(self):
        assemble = module('assemble')
        with self.assertRaisesRegex(ValueError, 'ledger'):
            assemble.fault_ledger({'schema':'turnstone.m5-missing-collection/v1',
                                   'status':'collected-awaiting-headed-and-semantic-verification',
                                   'run_id':'test'})


class PhaseIdentityChecks(unittest.TestCase):
    def test_reused_inode_allowed_only_with_distinct_verified_scopes_and_lifetimes(self):
        import copy
        v = module('verify')
        def phase(name, invocation, start, end):
            scope = 'm5-' + name + '.scope'
            launcher = {'scope':scope, 'ownership_observed':True, 'exit_code':0, 'timed_out':False,
                        'start_ns':start, 'exit_ns':end,
                        'argv':['--unit',scope.removesuffix('.scope'),'bwrap','--unshare-user','--unshare-net','--unshare-pid','--unshare-ipc']}
            cleanup = {'scope':scope, 'invocation':invocation, 'normal':True, 'ownership_observed':True,
                       'cleanup':[{'cgroup_empty_verified':True}]}
            net = {'pidns':'pid:[123]', 'cgroup':{'path':'/user/app/' + scope}}
            child = {'pid':3, 'pid_namespace':'pid:[123]', 'start_ns':start+1, 'exit_ns':end-1}
            return launcher, cleanup, net, child
        a = phase('seed','a'*32,0,10); b = phase('reopen','b'*32,11,20)
        aid = v.check_phase_identity(*a[:3], [a[3]])
        bid = v.check_phase_identity(*b[:3], [b[3]])
        v.check_distinct_phases([(*aid,a[0]), (*bid,b[0])])
        for field in ('child', 'cgroup', 'unshare', 'cleanup', 'unit'):
            launch, clean, net, child = copy.deepcopy(b)
            if field == 'child': child['pid_namespace'] = 'pid:[other]'
            if field == 'cgroup': net['cgroup']['path'] = '/user/app/other.scope'
            if field == 'unshare': launch['argv'].remove('--unshare-pid')
            if field == 'cleanup': clean['cleanup'] = []
            if field == 'unit': launch['argv'][1] = 'm5-other'
            with self.assertRaises(ValueError): v.check_phase_identity(launch, clean, net, [child])
        with self.assertRaises(ValueError): v.check_distinct_phases([(*aid,a[0]), (aid[0],bid[1],b[0])])
        with self.assertRaises(ValueError): v.check_distinct_phases([(*aid,a[0]), (bid[0],aid[1],b[0])])
        overlap = copy.deepcopy(b[0]); overlap['start_ns'] = 9
        with self.assertRaises(ValueError): v.check_distinct_phases([(*aid,a[0]), (*bid,overlap)])


class AdmissionChecks(unittest.TestCase):
    def test_fault_ledger_requires_real_outputs(self):
        assemble = module('assemble')
        doc = {'schema':'turnstone.m5-missing-collection/v1',
               'status':'collected-awaiting-headed-and-semantic-verification',
               'run_id':'test', 'files':{'irrelevant':sha(b'irrelevant')}}
        with self.assertRaises(ValueError):
            assemble.fault_ledger(doc)

    def test_no_output_is_created_for_unledgered_fault(self):
        assemble = module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root = Path(d); intact = root/'intact'; intact.mkdir(); fault = root/'fault'; fault.mkdir()
            (fault/'collection.json').write_text(json.dumps({'schema':'turnstone.m5-missing-collection/v1',
                'status':'collected-awaiting-headed-and-semantic-verification','run_id':'x'}))
            with self.assertRaises(ValueError):
                assemble.assemble(intact, fault, root/'review.json', root/'receipt')
            self.assertFalse((root/'receipt').exists())

class ProfileEvidence(unittest.TestCase):
    def test_source_not_in_owner_inventory_is_rejected(self):
        assemble = module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'profile').mkdir(); (root/'profile'/'note').write_text('bytes')
            with self.assertRaises(ValueError):
                assemble.bind_profile_source(root/'profile', {'other':'a'*64}, 'note')

    def test_source_changed_since_inspection_is_rejected(self):
        assemble = module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'profile').mkdir(); (root/'profile'/'note').write_text('new')
            with self.assertRaises(ValueError):
                assemble.bind_profile_source(root/'profile', {'note':sha(b'old')}, 'note')

class SemanticCoverage(unittest.TestCase):
    def test_group_rejects_unequal_values_and_omissions(self):
        v=module('verify')
        records=[{'id':'a','entity':'e','property':'p','schema_revision':1,'context':None,'status':'s','valid_time':None,'value':{'Integer':1}},
                 {'id':'b','entity':'e','property':'p','schema_revision':1,'context':None,'status':'s','valid_time':None,'value':{'Integer':2}}]
        with self.assertRaises(ValueError):
            v.check_group_rows(records,[{'record_ids':['a','b'],'key':{'Value':{'Integer':1}},'group':{k:records[0][k] for k in ('entity','property','schema_revision','context','status','valid_time','value')}}])
        with self.assertRaises(ValueError):
            v.check_group_rows(records,[{'record_ids':['a'],'key':{'Value':{'Integer':1}},'group':{k:records[0][k] for k in ('entity','property','schema_revision','context','status','valid_time','value')}}])

    def test_join_rejects_omitted_or_duplicate_pairs(self):
        v=module('verify')
        records=[{'id':x,'entity':'e'} for x in ('a','b','c')]
        with self.assertRaises(ValueError):
            v.check_pair_rows(records,[{'record_ids':['a','b']}],lambda a,b: a['entity']==b['entity'])
        with self.assertRaises(ValueError):
            v.check_pair_rows(records,[{'record_ids':['a','b']},{'record_ids':['a','b']}],lambda a,b: a['entity']==b['entity'])

class BuildBinding(unittest.TestCase):
    def test_tampered_binary_is_refused(self):
        assemble = module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'frozen').mkdir(); (root/'frozen/turnstone').write_bytes(b'changed')
            inputs={'files':{'turnstone':sha(b'original')},'builds':{'turnstone':{'binary_sha256':sha(b'original')}}}
            with self.assertRaises(ValueError): assemble.check_binary(root, inputs, 'turnstone')

class PhaseChecks(unittest.TestCase):
    def test_cleanup_without_empty_scope_is_rejected(self):
        v=module('verify')
        with self.assertRaises(ValueError):
            v.check_cleanup({'normal':True,'ownership_observed':True,'cleanup':[{'cgroup_empty_verified':False}]})

    def test_unknown_model_loads_are_not_zero(self):
        v=module('verify')
        base={'pid':8,'custody_principal':{'p':'v'},'network_offline':True,'offline_acquisition_attempts':0,
              'inference':{'client_compiled':True,'generation_requests':0,'corpus_publications':0,'resident_model_loads':0}}
        with self.assertRaises(ValueError): v.check_runtime(base, 8, {'p':'v'}, True)

class InventoryBinding(unittest.TestCase):
    def test_profile_missing_or_extra_owner_file_is_refused(self):
        assemble=module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'one').write_bytes(b'a'); (root/'extra').write_bytes(b'b')
            with self.assertRaises(ValueError): assemble.check_tree(root, {'one':sha(b'a')})

    def test_nested_symlink_is_refused(self):
        assemble=module('assemble')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'inside').mkdir(); (root/'inside'/'link').symlink_to(root)
            with self.assertRaises(ValueError): assemble.check_tree(root, {'inside/':'directory'})

class NamespaceEvidence(unittest.TestCase):
    def test_external_interface_is_rejected(self):
        v=module('verify')
        net={'addresses':[{'ifname':'lo'},{'ifname':'eth0'}], 'routes':[],
             'setup_capabilities':{'effective':'0','permitted':'0','inheritable':'0','ambient':'0','no_new_privs':True},
             'cgroup':{'memory_max':'3221225472','memory_swap_max':'0','pids_max':'128','cpu_max':'150000 100000'},
             'pidns':'pid:[1]','netns':'net:[1]','mountns':'mnt:[1]'}
        with self.assertRaises(ValueError): v.check_namespace(net)

class ValidSemanticExamples(unittest.TestCase):
    def test_exact_group_and_pair_coverage_is_accepted(self):
        v=module('verify')
        records=[{'id':x,'entity':'e','property':'p','schema_revision':1,'context':None,'status':'s','valid_time':None,'value':{'Integer':1}} for x in ('a','b')]
        v.check_group_rows(records,[{'record_ids':['a','b'],'key':{'Value':{'Integer':1}},'group':{k:records[0][k] for k in ('entity','property','schema_revision','context','status','valid_time','value')}}])
        v.check_pair_rows(records,[{'record_ids':['a','b']}],lambda a,b:a['entity']==b['entity'])

    def test_ledger_with_exact_hash_is_accepted(self):
        v=module('verify')
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); (root/'x').write_bytes(b'abc')
            self.assertEqual(v.validate_ledger(root,{'x':sha(b'abc')},{'x'}),{'x':sha(b'abc')})


class OwnerInventorySemantics(unittest.TestCase):
    def test_owner_projection_detects_omitted_or_altered_record(self):
        v=module('verify')
        definition={'id':'p','revision':'1','unit':None}
        source={'source':{'source':'a'},'version':'1'}
        record={'id':'r','property':'p','schema_revision':'1','evidence':{'Passage':{'source':source['source'],'version':'1'}}}
        snap={'complete':True,'exclusion_count':0,'exclusions':[], 'sources':[source], 'records':[record],
              'definitions':[definition], 'lifecycle':[{'record_id':'r','state':'Active'}]}
        owner={'schema':'turnstone.m5-store-inspection/v1','session':'s','graph':'g','result':{'snapshot':snap},
               'records':{'r':{'record':record,'definition':definition,'graph':'g','withdrawn':False}}}
        v.check_owner_inventory(owner,expected_records=1)
        altered=json.loads(json.dumps(owner)); altered['records']['r']['record']['property']='other'
        with self.assertRaises(ValueError): v.check_owner_inventory(altered,expected_records=1)
        missing=json.loads(json.dumps(owner)); missing['records']={}
        with self.assertRaises(ValueError): v.check_owner_inventory(missing,expected_records=1)

    def test_query_filters_apply_to_owner_inputs_not_snapshot_inventory(self):
        v=module('verify')
        source1={'source':{'source':'a'},'version':'1','producer':'capture','tags':[{'owner':'o','concept':'c'}]}
        source2={'source':{'source':'b'},'version':'1','producer':'note','tags':[]}
        records=[{'id':'a','entity':'x','property':'p','schema_revision':'1','status':'active','valid_time':{'start':1,'end':5},'evidence':{'Passage':{'source':source1['source'],'version':'1'}}},
                 {'id':'b','entity':'x','property':'p','schema_revision':'1','status':'active','valid_time':{'start':5,'end':9},'evidence':{'Passage':{'source':source2['source'],'version':'1'}}}]
        snapshot={'complete':True,'exclusion_count':0,'exclusions':[], 'sources':[source1,source2], 'records':records,'definitions':[{'id':'p','revision':'1','unit':'kg'}],
                  'lifecycle':[{'record_id':'a','state':'Active'},{'record_id':'b','state':'Active'}]}
        filters={'source':None,'entity':'x','property':'p','schema_revision':'1','status':'active','valid_time':{'start':4,'end':5},'unit':'kg','tag':None}
        sources, selected=v.filtered_inputs(snapshot,filters)
        self.assertEqual([r['id'] for r in selected],['a'])
        self.assertEqual(len(sources),2)
        filters['tag']={'owner':'o','concept':'c'}
        sources, selected=v.filtered_inputs(snapshot,filters)
        self.assertEqual([s['source']['source'] for s in sources],['a'])
        self.assertEqual([r['id'] for r in selected],['a'])
        filters['surprise']='unhandled'
        with self.assertRaises(ValueError): v.filtered_inputs(snapshot,filters)

class WithdrawalInventory(unittest.TestCase):
    def test_current_inventory_must_remove_only_the_withdrawn_record(self):
        v=module('verify')
        source={'source':{'source':'a'},'version':'1'}
        definition={'id':'p','revision':'1','unit':None}
        def record(n): return {'id':n,'property':'p','schema_revision':'1','evidence':{'Passage':{'source':source['source'],'version':'1'}}}
        a,b=record('a'),record('b')
        def snapshot(records):
            return {'complete':True,'exclusion_count':0,'exclusions':[],'sources':[source], 'records':records,'definitions':[definition],
                    'lifecycle':[{'record_id':r['id'],'state':'Active'} for r in records]}
        historical=snapshot([a,b]); current=snapshot([a])
        v.check_withdrawal_inventory(historical,current,'b')
        with self.assertRaises(ValueError): v.check_withdrawal_inventory(historical,snapshot([]),'b')
        with self.assertRaises(ValueError): v.check_withdrawal_inventory(historical,historical,'b')

class OperationOwnerBinding(unittest.TestCase):
    def test_operation_snapshot_cannot_omit_or_replace_owner_source(self):
        v=module('verify')
        source={'source':{'source':'a'},'version':'1'}
        definition={'id':'p','revision':'1'}
        record={'id':'r','property':'p','schema_revision':'1','evidence':{'Passage':{'source':source['source'],'version':'1'}}}
        snap={'complete':True,'exclusion_count':0,'exclusions':[],'sources':[source], 'records':[record],
              'definitions':[definition], 'lifecycle':[{'record_id':'r','state':'Active'}]}
        owner={'session':'s','graph':'g','result':{'snapshot':snap}}
        observed={'session':'s','source_selection':{'mode':'all'},'graph_revisions':[{'graph':'g'}],'result':{'snapshot':snap}}
        v.check_prewithdraw_scope(observed,observed,owner)
        bad=json.loads(json.dumps(observed)); bad['result']['snapshot']['sources']=[]
        with self.assertRaises(ValueError): v.check_prewithdraw_scope(bad,observed,owner)
        bad=json.loads(json.dumps(observed)); bad['graph_revisions'][0]['graph']='other'
        with self.assertRaises(ValueError): v.check_prewithdraw_scope(bad,observed,owner)

class SelectedSourceBinding(unittest.TestCase):
    def test_inspected_hash_must_equal_the_exported_anchor_hash(self):
        v=module('verify')
        anchor={'units':'utf8-bytes','selector':{'source_hash':'b3hash','exact_quote':'abc','byte_span':{'start':2,'end':5}}}
        clip={'clip_id':'clip','anchors':[anchor]}
        selection={'source_hash':'b3hash','quote':'abc','byte_start':2,'byte_end':5,'clip':clip,'package':{'clip':clip}}
        v.check_selection_anchor(selection)
        bad=json.loads(json.dumps(selection)); bad['source_hash']='different'
        with self.assertRaises(ValueError): v.check_selection_anchor(bad)
        bad=json.loads(json.dumps(selection)); bad['package']['clip']['anchors'][0]['selector']['byte_span']['end']=6
        with self.assertRaises(ValueError): v.check_selection_anchor(bad)

class LinearRowCoverage(unittest.TestCase):
    def test_duplicate_property_row_and_wrong_value_are_rejected(self):
        v=module('verify')
        records=[{'id':'a','value':{'Integer':1}},{'id':'b','value':{'Integer':2}}]
        good=[{'record_ids':['a'],'key':{'Value':{'Integer':1}}},{'record_ids':['b'],'key':{'Value':{'Integer':2}}}]
        v.check_property_rows(records,good)
        with self.assertRaises(ValueError): v.check_property_rows(records,[good[0],good[0]])
        bad=json.loads(json.dumps(good)); bad[1]['key']={'Value':{'Integer':1}}
        with self.assertRaises(ValueError): v.check_property_rows(records,bad)

class ProducerEvidence(unittest.TestCase):
    def test_counts_bind_exact_source_contributors(self):
        v=module('verify')
        sources=[{'producer':'capture','source':{'source':'a'}},{'producer':'capture','source':{'source':'b'}}]
        rows=[{'key':{'Text':'capture'},'count':2,'record_ids':[], 'sources':[sources[0]['source'],sources[1]['source']]}]
        v.check_producer_rows(sources,rows)
        wrong=json.loads(json.dumps(rows)); wrong[0]['sources'][1]={'source':'other'}
        with self.assertRaises(ValueError): v.check_producer_rows(sources,wrong)
        with self.assertRaises(ValueError): v.check_producer_rows(sources,rows+rows)


class CompleteResultSemantics(unittest.TestCase):
    def test_sum_rejects_omission_wrong_sources_units_and_overflow(self):
        v=module('verify')
        def rec(name,value,source='a',prop='p',revision='1'):
            return {'id':name,'property':prop,'schema_revision':revision,'value':value,
                    'evidence':{'Passage':{'source':{'source':source}}}}
        records=[rec('a',{'Integer':7}),rec('b',{'Integer':12},'b')]
        row={'key':{'Value':{'Integer':19}},'record_ids':['a','b'],'sources':[{'source':'a'},{'source':'b'}]}
        defs=[{'id':'p','revision':'1','unit':None,'value_type':'Integer'}]
        v.check_sum_rows(records,defs,[row])
        with self.assertRaises(ValueError): v.check_sum_rows(records+[rec('c',{'Integer':0})],defs,[row])
        wrong=json.loads(json.dumps(row)); wrong['sources']=[{'source':'a'}]
        with self.assertRaises(ValueError): v.check_sum_rows(records,defs,[wrong])
        wrongrecords=[rec('a',{'Decimal':{'coefficient':7,'scale':0,'unit':'kg'}}),rec('b',{'Decimal':{'coefficient':12,'scale':0,'unit':'m'}})]
        with self.assertRaises(ValueError): v.check_sum_rows(wrongrecords,[{'id':'p','revision':'1','unit':'kg','value_type':'Decimal'}],[row])
        with self.assertRaises(ValueError): v.check_sum_rows([rec('a',{'Integer':2**63-1}),rec('b',{'Integer':1})],defs,[row])
        wrong=json.loads(json.dumps(row)); wrong['record_ids']=['a','a']
        with self.assertRaises(ValueError): v.check_sum_rows(records,defs,[wrong])

    def test_group_key_is_not_just_displayed_value(self):
        v=module('verify')
        r={'id':'a','entity':'e','property':'p','schema_revision':'1','context':'ctx','status':'active','valid_time':None,'value':{'Integer':1}}
        row={'record_ids':['a'],'key':{'Value':{'Integer':1}},'group':{k:r[k] for k in ('entity','property','schema_revision','context','status','valid_time','value')}}
        v.check_group_rows([r],[row])
        row['group']['context']='wrong'
        with self.assertRaises(ValueError): v.check_group_rows([r],[row])

    def test_count_source_rejects_duplicate_or_wrong_source_references(self):
        v=module('verify')
        src={'source':'a'}; records=[{'id':'a','evidence':{'Passage':{'source':src}}},{'id':'b','evidence':{'Passage':{'source':src}}}]
        row={'key':{'Source':src},'record_ids':['a','b'],'count':2,'sources':[src]}
        v.check_count_source(records,[row])
        one={'key':{'Source':src},'record_ids':['a'],'count':1,'sources':[src]}
        two={'key':{'Source':src},'record_ids':['b'],'count':1,'sources':[src]}
        with self.assertRaises(ValueError): v.check_count_source(records,[one,two])
        wrong=json.loads(json.dumps(row)); wrong['sources']=[{'source':'foreign'}]
        with self.assertRaises(ValueError): v.check_count_source(records,[wrong])

class ProvenanceBindings(unittest.TestCase):
    def test_mismatched_frozen_script_or_source_build_link_is_refused(self):
        v=module('verify')
        binaries=('turnstone','test-binary','custodian')
        doc={'schema':'turnstone.m5-build-provenance/v1','artifacts':{}}
        inputs={'kind':'model-free-acceptance-collection','files':{'seed.scn':'a'*64},'builds':{},'provenance_sha256':'b'*64}
        ledger={'frozen/seed.scn':'a'*64,'frozen/build-provenance.json':'b'*64}
        for n in ('reopen.scn','run_native.py','namespace_phase.py','fixture_server.py','acceptance_phase.py','run_acceptance.py'):
            inputs['files'][n]='a'*64; ledger['frozen/'+n]='a'*64
        receipts={}
        for n in binaries:
            inputs['files'][n]='1'*64
            receipt='frozen/'+n+'-build.json'; inventory='frozen/'+n+'-source-inventory.json'
            ledger[receipt]='2'*64; ledger[inventory]='3'*64
            receipts[receipt]={'exit_code':0,'command':v.COMMANDS[n]}
            doc['artifacts'][n]={'receipt_sha256':'2'*64,'binary_sha256':'1'*64,'source_before':'3'*64,'source_after':'3'*64,'source_inventory_sha256':'3'*64}
            inputs['builds'][n]={'receipt_sha256':'2'*64,'binary_sha256':'1'*64,'source_hash':'3'*64,'source_inventory':inventory,'command':v.COMMANDS[n]}
        values={'frozen/build-provenance.json':doc,**receipts}
        v.check_collector_bindings(inputs,ledger,values.__getitem__,{n:'1'*64 for n in binaries})
        bad=dict(ledger); bad['frozen/seed.scn']='c'*64
        with self.assertRaises(ValueError): v.check_collector_bindings(inputs,bad,values.__getitem__,{n:'1'*64 for n in binaries})
        altered=json.loads(json.dumps(doc)); altered['artifacts']['turnstone']['source_after']='4'*64
        with self.assertRaises(ValueError): v.check_collector_bindings(inputs,ledger,{'frozen/build-provenance.json':altered,**receipts}.__getitem__,{n:'1'*64 for n in binaries})

class ComparisonEvidence(unittest.TestCase):
    def fixture(self):
        import uuid
        sid='00000000-0000-0000-0000-000000000001'; gid='00000000-0000-0000-0000-000000000002'; node='00000000-0000-0000-0000-000000000003'
        def side(n,text):
            key={'session':sid,'graph':gid,'node':node,'annotation_manifest':'sealed-v1:blake3:'+n*64}
            target={'key':key,'text_hash':'blake3:'+n*64,'selector':None}
            source={'kind':'fleece','owner':'turnstone/'+sid+'/research/v1','node':node,
                    'source':'mere://saved-source/sealed-v1/'+sid+'/'+gid+'/'+node+'/'+n*64+'/'+n*64}
            return target,{'source':source,'version':target['text_hash'],'text_hash':target['text_hash']},{'text':text,'text_hash':target['text_hash']}
        left,old,a=side('a','M5 A1 archive\n');right,new,b=side('b','M5 A2 archive\n')
        comp={'schema':'turnstone.comparison-observation/v1','session':sid,'request':str(uuid.UUID(int=4)),
              'left':left,'right':right,'operation':'eidetic.line-comparison/v1','relationship':'Explicit','old':old,'new':new,
              'settings':{'max_output_spans':3},'spans':[{'kind':'Delete','old':{'start':0,'end':len(a['text'])},'new':None},
                 {'kind':'Insert','old':None,'new':{'start':0,'end':len(b['text'])}}]}
        return comp,{'target':left,'content':a},{'target':right,'content':b}

    def test_comparison_binds_both_versions_and_span_coverage(self):
        v=module('verify'); comparison,old,new=self.fixture()
        v.check_source_comparison(comparison,old,new)
        bad=json.loads(json.dumps(comparison)); bad['new']['version']='other'
        with self.assertRaises(ValueError): v.check_source_comparison(bad,old,new)
        bad=json.loads(json.dumps(comparison)); bad['spans'][1]['new']['end']-=1
        with self.assertRaises(ValueError): v.check_source_comparison(bad,old,new)
        bad=json.loads(json.dumps(comparison)); bad['left']=bad['right']
        with self.assertRaises(ValueError): v.check_source_comparison(bad,old,new)

class NegativeQueryBinding(unittest.TestCase):
    def fixture(self):
        filters={k:None for k in ('source','entity','property','schema_revision','status','valid_time','unit','tag')}
        snapshot={'complete':True,'exclusion_count':0,'exclusions':[],'sources':[],'records':[], 'definitions':[],'lifecycle':[]}
        baseline={'schema':'turnstone.structured-observation/v1','historical':False,'busy':False,'result_generation':'generation','context_generation':'gen','session':'s','source_selection':{'mode':'all'},'graph_revisions':[{'graph':'g'}],'result':{'snapshot':snapshot}}
        unmatched=json.loads(json.dumps(baseline)); unmatched['result']['query']={'operation':'PropertyLookup','filters':dict(filters,property='m5:absent')}; unmatched['result']['rows']=[]
        refusal={'schema':'turnstone.structured-status/v1','session':'s','busy':False,'visible':True,'error':'Structured record limit exceeded',
                 'limit_refusal':{'stage':'worker-refusal','request_id':'00000000-0000-0000-0000-000000000001','context_generation':'gen','scope_revision':1,'source_selection':{'mode':'all'},
                                  'graph_revisions':[{'graph':'g'}], 'query':{'operation':'PropertyLookup','filters':filters},'settings':{'max_records':1},'error':'Structured record limit exceeded'}}
        return baseline,unmatched,refusal

    def test_no_match_requires_intended_query_and_owner_scope(self):
        v=module('verify'); base,value,_=self.fixture()
        v.check_no_match(value,base,'g')
        bad=json.loads(json.dumps(value)); bad['result']['query']['filters']['property']='another'
        with self.assertRaises(ValueError): v.check_no_match(bad,base,'g')
        bad=json.loads(json.dumps(value)); bad['graph_revisions'][0]['graph']='foreign'
        with self.assertRaises(ValueError): v.check_no_match(bad,base,'g')
        bad=json.loads(json.dumps(value)); bad['source_selection']={'mode':'chosen','members':[]}
        with self.assertRaises(ValueError): v.check_no_match(bad,base,'g')

    def test_over_limit_requires_same_scope_setting_and_exact_refusal(self):
        v=module('verify'); base,_,value=self.fixture()
        base['result']['snapshot']['records']=[{'id':str(i)} for i in range(13)]
        v.check_limit_refusal(value,base,'g')
        for field,wrong in [('error','custody unavailable'),('stage','dispatched')]:
            bad=json.loads(json.dumps(value)); bad['limit_refusal'][field]=wrong
            with self.assertRaises(ValueError): v.check_limit_refusal(bad,base,'g')
        bad=json.loads(json.dumps(value)); bad['limit_refusal']['settings']['max_records']=100
        with self.assertRaises(ValueError): v.check_limit_refusal(bad,base,'g')
        bad=json.loads(json.dumps(value)); bad['limit_refusal']['query']['operation']='Sum'
        with self.assertRaises(ValueError): v.check_limit_refusal(bad,base,'g')

class CitationEvidence(unittest.TestCase):
    def test_note_citation_binds_source_quote_and_position(self):
        v=module('verify'); comp,old,_=ComparisonEvidence().fixture()
        text=old['content']['text']; start=3; end=5; target=dict(old['target'],selector={'start':start,'end':end})
        uri=v.full_capture_source(target)['source']; exact=text[start:end]
        import base64
        portable_id='ni:///sha-256;'+base64.urlsafe_b64encode(hashlib.sha256(text.encode()).digest()).decode().rstrip('=')
        data={'schema':'knot.clip.insert/v2','source_url':uri+f'?start={start}&end={end}',
              'selectors':[{'type':'text-position','artifact_role':'observed-representation','start':start,'end':end},
                           {'type':'text-quote','artifact_role':'observed-representation','exact':exact}],
              'evidence':[{'role':'observed-representation','canonical_uri':uri,'content':{'transport':target['text_hash'],'byte_size':len(text.encode()),'portable_id':portable_id}}]}
        note='```knot.clip.provenance\n'+json.dumps(data)+'\n```'
        v.check_citation_text(note,target,text,exact)
        data['selectors'][0]['end']+=1
        with self.assertRaises(ValueError): v.check_citation_text('```knot.clip.provenance\n'+json.dumps(data)+'\n```',target,text,exact)

class ReviewCoverage(unittest.TestCase):
    def test_review_requires_outcomes_and_exact_hashes_for_each_frame(self):
        v=module('verify'); frames={'a.png','b.png'}; ledger={'a.png':'a'*64,'b.png':'b'*64}
        review={'reviewer':'reviewer','reviewed_frames':list(frames),'outcomes':{x:'pass' for x in frames},'sha256':dict(ledger)}
        v.check_review_coverage(review,frames,ledger)
        import copy
        for mutation in ('missing_frame','failed','missing_digest','wrong_digest'):
            bad=copy.deepcopy(review)
            if mutation == 'missing_frame': bad['reviewed_frames'].remove('a.png')
            if mutation == 'failed': bad['outcomes']['a.png']='fail'
            if mutation == 'missing_digest': del bad['sha256']['a.png']
            if mutation == 'wrong_digest': bad['sha256']['a.png']='c'*64
            with self.assertRaises(ValueError): v.check_review_coverage(bad,frames,ledger)

class ContributorSourceCoverage(unittest.TestCase):
    def test_row_sources_match_all_and_only_record_sources(self):
        v=module('verify'); src={'source':'a'}; records=[{'id':'r','evidence':{'Passage':{'source':src}}}]
        v.check_row_sources(records,[{'record_ids':['r'],'sources':[src]}])
        with self.assertRaises(ValueError): v.check_row_sources(records,[{'record_ids':['r'],'sources':[]}])
        with self.assertRaises(ValueError): v.check_row_sources(records,[{'record_ids':['r'],'sources':[src,src]}])

class RequiredFrames(unittest.TestCase):
    def test_exact_comparison_and_scope_fresh_review_are_mandatory(self):
        v=module('verify'); frames=v.required_review_frames()
        for required in ('intact/sandbox/seed/exact_pair.png','intact/sandbox/seed/scope_empty_fresh.png','intact/sandbox/seed/05_note_citation.png','intact/sandbox/reopen/24_reopen_selection_reader.png','intact/sandbox/reopen/25_reopen_note_reader.png','fault/output/missing_A1_refusal.png','fault/output/missing_A1_peek.png'):
            self.assertIn(required,frames)
            listed=frames-{required}
            review={'reviewer':'r','reviewed_frames':list(listed),'outcomes':{x:'pass' for x in listed}}
            with self.assertRaises(ValueError): v.check_review_coverage(review,frames,frames)

class ComparisonClassCoverage(unittest.TestCase):
    def test_unknown_comparison_class_is_rejected(self):
        v=module('verify')
        v.check_comparison_classes([{'key':{'Comparison':'Equal'}}])
        with self.assertRaises(ValueError): v.check_comparison_classes([{'key':{'Comparison':'Maybe'}}])


class RenderedCitation(unittest.TestCase):
    def test_citation_reader_requires_actual_rendered_quote(self):
        v=module('verify'); reader={'phase':'Ready','rendered_document':{'blocks':[{'text':'naïve — 東京 🐦'}]}}
        v.check_rendered_quote(reader,'naïve — 東京 🐦')
        with self.assertRaises(ValueError): v.check_rendered_quote({'phase':'Ready','rendered_document':{'blocks':[]}},'naïve — 東京 🐦')


class RetainedSourceSearch(unittest.TestCase):
    def test_search_hit_must_match_the_inspected_admission_and_version(self):
        v=module('verify'); session='s'; graph='g'; node='n'; digest='a'*64; version='blake3:'+'b'*64
        admission={'session':session,'graph':graph,'node':node,'manifest':'blake3:'+digest}
        source={'kind':'fleece','owner':'turnstone/s/research/v1','node':'n','source':'mere://retained-text/s/g/n/'+digest+'/'+version[7:]}
        owner={'session':session,'graph':graph,'selection':{'selected':True,'admission':admission},'result':{'snapshot':{'sources':[{'producer':'selection','source':source,'version':version}]}}}
        obs={'session':session,'retained_admissions':[admission], 'search':{'schema':'turnstone.research-search-summary/v1','session':session,'coverage':'Complete','hits':[{'source':source,'version':version,'producer':'Selection'}]}}
        v.check_retained_search(obs,owner,'Selection')
        bad=json.loads(json.dumps(obs)); bad['search']['hits'][0]['version']='other'
        with self.assertRaises(ValueError): v.check_retained_search(bad,owner,'Selection')
        bad=json.loads(json.dumps(obs)); bad['retained_admissions']=[]
        with self.assertRaises(ValueError): v.check_retained_search(bad,owner,'Selection')


class FullPassageCitation(unittest.TestCase):
    def test_full_fixture_passage_required_and_truncation_rejected(self):
        v=module('verify'); _,old,_=ComparisonEvidence().fixture()
        full='M5 A1 archive: naïve — 東京 🐦. Evidence amount is 12 units.'
        self.assertEqual(len(full),57)
        old['content']['text']='prefix '+full+' suffix'
        target=dict(old['target'],selector={'start':7,'end':64})
        import base64
        raw=old['content']['text'].encode(); uri=v.full_capture_source(target)['source']
        data={'schema':'knot.clip.insert/v2','source_url':uri+'?start=7&end=64',
              'selectors':[{'type':'text-position','artifact_role':'observed-representation','start':7,'end':64},
                           {'type':'text-quote','artifact_role':'observed-representation','exact':full}],
              'evidence':[{'role':'observed-representation','canonical_uri':uri,'content':{'transport':target['text_hash'],
                  'byte_size':len(raw),'portable_id':'ni:///sha-256;'+base64.urlsafe_b64encode(hashlib.sha256(raw).digest()).decode().rstrip('=')}}]}
        note='```knot.clip.provenance\n'+json.dumps(data)+'\n```'
        v.check_citation_text(note,target,old['content']['text'],v.A1_FULL_PASSAGE)
        with self.assertRaises(ValueError): v.check_citation_text(note,target,old['content']['text'],'naïve — 東京 🐦')
        wrong=dict(target,selector={'start':8,'end':64})
        with self.assertRaises(ValueError): v.check_citation_text(note,wrong,old['content']['text'],v.A1_FULL_PASSAGE)

class ColdRetainedReader(unittest.TestCase):
    def fixture(self,kind='Selection'):
        session='s'; graph='g'; node='n'; digest='a'*64; version='blake3:'+'b'*64
        admission={'session':session,'graph':graph,'node':node,'manifest':'blake3:'+digest}
        source={'kind':'fleece','owner':'turnstone/s/research/v1','node':'n','source':'mere://retained-text/s/g/n/'+digest+'/'+version[7:]}
        text='only this linked finding' if kind=='Selection' else 'M5 authored Redshank note'
        anchor={'units':'utf8-bytes' if kind=='Selection' else 'milliseconds',
                'source':'file:///fixture/source', 'selector':{'exact_quote':text} if kind=='Selection' else {'offset_ms':0}}
        title='Knot selection' if kind=='Selection' else 'Redshank note'
        item={'selected':True,'admission':admission,'quote':text,'text':text,'clip':{'title':title,'anchors':[anchor]}}
        owner={'session':session,'graph':graph,kind.lower():item,'result':{'snapshot':{'sources':[{'producer':kind.lower(),'source':source,'version':version}]}}}
        selector={'start':0,'end':len(text)}
        search={'session':session,'retained_admissions':[admission], 'search':{'schema':'turnstone.research-search-summary/v1','session':session,'coverage':'Complete','hits':[{'source':source,'version':version,'producer':kind,'selector':selector}]}}
        reader={'session':session,'retained_readers':[{'node':'live','engine':'fleece.retained-clip',
                  'target':{'clip':admission,'text_hash':version,'selector':selector},'rendered_document':{'address':'mere://clip/'+digest,'content_type':'application/vnd.mere.clip+json',
                  'blocks':[{'Heading':{'level':1,'spans':[{'Text':title}]}},
                    {'Heading':{'level':2,'spans':[{'Text':'Selected passage'}]}},
                    {'Quote':{'blocks':[{'Paragraph':{'spans':[{'Text':text}]}}]}},
                    {'Paragraph':{'spans':[{'Text':text}]}},
                    *([{'Badge':{'text':'Source recording unavailable in this clip'}}] if kind=='Note' else []),
                    {'MetadataRow':{'label':'Source anchor ('+anchor['units']+')','value':anchor['source']}},
                    {'MetadataRow':{'label':'Original selection' if kind=='Selection' else 'Original note position','value':text if kind=='Selection' else '0.000 seconds'}},
                    {'Paragraph':{'spans':[{'Link':{'predicate':None,'spans':[{'Text':'Export selected clip'}], 'title':None,'url':'mere://clip/'+digest+'/export'}}]}}]}}]}
        return owner,search,reader

    def test_reader_must_open_exact_owner_version_and_body(self):
        v=module('verify')
        for kind in ('Selection','Note'):
            owner,search,reader=self.fixture(kind)
            v.check_retained_reader(search,reader,owner,kind)
            missing=json.loads(json.dumps(reader)); missing['retained_readers']=[]
            with self.assertRaises(ValueError): v.check_retained_reader(search,missing,owner,kind)
            wrong=json.loads(json.dumps(reader)); wrong['retained_readers'][0]['target']['text_hash']='blake3:'+'c'*64
            with self.assertRaises(ValueError): v.check_retained_reader(search,wrong,owner,kind)
            wrong=json.loads(json.dumps(reader)); wrong['retained_readers'][0]['rendered_document']['blocks'][3]['Paragraph']['spans'][0]['Text']+=' extra'
            with self.assertRaises(ValueError): v.check_retained_reader(search,wrong,owner,kind)
            wrong=json.loads(json.dumps(reader)); wrong['retained_readers'][0]['target']['selector']['end']-=1
            with self.assertRaises(ValueError): v.check_retained_reader(search,wrong,owner,kind)
            missing_selector=json.loads(json.dumps(search)); del missing_selector['search']['hits'][0]['selector']
            with self.assertRaises(ValueError): v.check_retained_reader(missing_selector,reader,owner,kind)

    def test_rejects_extra_or_changed_rendered_content(self):
        v=module('verify')
        for kind in ('Selection','Note'):
            owner,search,reader=self.fixture(kind)
            blocks=reader['retained_readers'][0]['rendered_document']['blocks']
            mutations=[lambda b: b.insert(4,{'Paragraph':{'spans':[{'Text':'private canary'}]}}),
                       lambda b: b[2]['Quote']['blocks'][0]['Paragraph']['spans'][0].update(Text='wrong quote'),
                       lambda b: b[0]['Heading']['spans'][0].update(Text='private canary'),
                       lambda b: b[-2]['MetadataRow'].update(value='private canary'),
                       lambda b: b[-1]['Paragraph']['spans'][0]['Link'].update(url='mere://clip/wrong/export')]
            for mutate in mutations:
                altered=json.loads(json.dumps(reader))
                mutate(altered['retained_readers'][0]['rendered_document']['blocks'])
                with self.assertRaises(ValueError): v.check_retained_reader(search,altered,owner,kind)
            wrong=json.loads(json.dumps(search)); wrong['search']['hits'][0]['source']['source']+='stale'
            with self.assertRaises(ValueError): v.check_retained_reader(wrong,reader,owner,kind)


if __name__ == '__main__':
    unittest.main()
