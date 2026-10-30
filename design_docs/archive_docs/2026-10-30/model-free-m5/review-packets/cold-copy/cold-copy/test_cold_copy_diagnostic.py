import copy, importlib.util, json, socket, tempfile, unittest, uuid
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('cold_copy_diagnostic', Path(__file__).with_name('cold-copy-diagnostic.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

class ColdReaderTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.root = Path(self.tmp.name)
        session, graph, node = (str(uuid.uuid4()) for _ in range(3))
        manifest, version = 'blake3:'+'a'*64, 'blake3:'+'b'*64
        self.clip={'session':session,'graph':graph,'node':node,'manifest':manifest}
        source={'kind':'fleece','owner':'turnstone/'+session+'/research/v1','node':node,
                'source':'mere://retained-text/'+'/'.join((session,graph,node,manifest[7:],version[7:]))}
        self.search={'session':session,'retained_admissions':[self.clip], 'search':{'session':session,'coverage':'Complete','hits':[{'source':source,'version':version,'producer':'Selection','selector':{'start':0,'end':24}}]}}
        para={'Paragraph':{'spans':[{'Text':'only this linked finding'}]}}
        export={'Paragraph':{'spans':[{'Link':{'predicate':None,'spans':[{'Text':'Export selected clip'}],'title':None,'url':'mere://clip/'+manifest[7:]+'/export'}}]}}
        metadata=[{'MetadataRow':{'label':'Source anchor (utf8-bytes)','value':'file:///m5/run/profile/notes/research-'+str(uuid.uuid4())+'.djot'}},
                  {'MetadataRow':{'label':'Original selection','value':'only this linked finding'}}]
        document={'address':'mere://clip/'+manifest[7:],'content_type':'application/vnd.mere.clip+json',
            'blocks':[{'Heading':{'level':1,'spans':[{'Text':'Knot selection'}]}},{'Heading':{'level':2,'spans':[{'Text':'Selected passage'}]}},{'Quote':{'blocks':[para]}},para,*metadata,export]}
        self.opened={'session':session,'retained_admissions':[self.clip], 'retained_readers':[{'node':node,'engine':'fleece.retained-clip','target':{'clip':self.clip,'text_hash':version,'selector':{'start':0,'end':24}},'rendered_document':document}]}
    def tearDown(self): self.tmp.cleanup()
    def run_check(self, search=None, opened=None):
        (self.root/'case.saved-sources.json').write_text(json.dumps(search or self.search))
        (self.root/'case_reader.saved-sources.json').write_text(json.dumps(opened or self.opened))
        with patch.object(m.phase, 'validate_png', return_value=(1,1)):
            return m.validate_reader(self.root,'case','Selection','only this linked finding')
    def test_full_binding_and_unproven_active_state(self):
        out=self.run_check(); self.assertFalse(out['active_reader_proven'])
        self.assertTrue(out['visual_review_required'])
    def test_matching_stale_reader_does_not_claim_active_identity(self):
        opened=copy.deepcopy(self.opened)
        other=copy.deepcopy(opened['retained_readers'][0]); other['node']=str(uuid.uuid4())
        other['target']['clip']['node']=other['node']
        opened['retained_readers'].append(other)
        out=self.run_check(opened=opened)
        self.assertFalse(out['active_reader_proven'])
        self.assertTrue(out['visual_review_required'])
    def test_rejects_mutated_identity_and_extra_body(self):
        for part in ('session','graph','node','manifest'):
            bad=copy.deepcopy(self.opened); bad['retained_readers'][0]['target']['clip'][part]='wrong'
            with self.subTest(part=part), self.assertRaises(ValueError): self.run_check(opened=bad)
        for field in ('text_hash','selector'):
            bad=copy.deepcopy(self.opened); bad['retained_readers'][0]['target'][field]='wrong'
            with self.subTest(field=field), self.assertRaises(ValueError): self.run_check(opened=bad)
        bad=copy.deepcopy(self.opened); bad['session']=str(uuid.uuid4())
        with self.assertRaises(ValueError): self.run_check(opened=bad)
        for field in ('source','node','owner'):
            bad=copy.deepcopy(self.search); bad['search']['hits'][0]['source'][field]='wrong'
            with self.subTest(hit_field=field), self.assertRaises(ValueError): self.run_check(search=bad)
        bad=copy.deepcopy(self.search); bad['search']['hits'][0]['version']='blake3:'+'c'*64
        with self.assertRaises(ValueError): self.run_check(search=bad)
        bad=copy.deepcopy(self.opened); bad['retained_readers'][0]['rendered_document']['blocks'].insert(4,{'Paragraph':{'spans':[{'Text':'private canary'}]}})
        with self.assertRaises(ValueError): self.run_check(opened=bad)
        bad=copy.deepcopy(self.opened); bad['retained_readers'][0]['rendered_document']['blocks'][5]['MetadataRow']['value'] += ' private canary'
        with self.assertRaises(ValueError): self.run_check(opened=bad)
    def test_inventory_only_expected_stale_custody_sockets(self):
        custody=self.root/'custody'; state=custody/'custodian'; run=state/('run-'+str(uuid.uuid4())); run.mkdir(parents=True)
        a=socket.socket(socket.AF_UNIX); b=socket.socket(socket.AF_UNIX)
        try:
            a.bind(str(custody/'app.sock')); b.bind(str(run/'custody.sock'))
            found=m.stale_socket_inventory(custody)
            self.assertEqual(len(found),2)
            (run/'unexpected').write_text('x')
            with self.assertRaises(ValueError): m.stale_socket_inventory(custody)
        finally: a.close(); b.close()
    def test_rejects_symlink_parent_and_nonregular_file(self):
        real=self.root/'real'; real.mkdir(); (real/'x').write_text('x')
        (self.root/'link').symlink_to(real, target_is_directory=True)
        with self.assertRaises(ValueError): m.require_plain_file(self.root/'link/x')
        with self.assertRaises(ValueError): m.require_plain_file(real)
        m.require_plain_file(real/'x')

if __name__=='__main__': unittest.main()
