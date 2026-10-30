import importlib.util, json, socket, tempfile, unittest, uuid
from pathlib import Path
spec=importlib.util.spec_from_file_location('fault_runner',Path(__file__).with_name('run_missing.py'))
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

class CustodyCopyTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(); self.root=Path(self.tmp.name)
        self.custody=self.root/'sandbox/custody'; state=self.custody/'custodian'; self.run=state/('run-'+str(uuid.uuid4())); self.run.mkdir(parents=True)
        for name in ('data','vault'): (state/name).mkdir(); (state/name/'item').write_text(name)
        for p in (self.custody/'fixture-marker',self.custody/'fixture.lock',state/'initialized',state/'client.json'): p.write_text('x')
        self.sock=socket.socket(socket.AF_UNIX); self.sock.bind(str(self.run/'custody.sock'))
    def tearDown(self): self.sock.close(); self.tmp.cleanup()
    def test_socket_inventory_and_persistent_copy(self):
        before=m.collection_tree(self.root); target=self.root.parent/(self.root.name+'-copy'); receipt=m.copy_custody(self.custody,target)
        try:
            self.assertEqual(m.collection_tree(self.root),before)
            self.assertEqual((target/'custodian/data/item').read_text(),'data')
            self.assertFalse((target/'custodian'/self.run.name).exists())
            self.assertEqual(len(receipt['runtime_sockets']),1)
        finally:
            import shutil; shutil.rmtree(target)
    def test_unknown_socket_or_symlink_refused(self):
        bad=self.root/'other.sock'; s=socket.socket(socket.AF_UNIX)
        try:
            s.bind(str(bad))
            with self.assertRaises(ValueError): m.collection_tree(self.root)
        finally: s.close(); bad.unlink()
        (self.run/'link').symlink_to(self.root/'sandbox')
        with self.assertRaises(ValueError): m.collection_tree(self.root)
    def test_copy_rejects_unknown_runtime_content_and_hardlinks(self):
        (self.run/'unexpected').write_text('x')
        target=self.root.parent/(self.root.name+'-copy')
        with self.assertRaises(ValueError): m.copy_custody(self.custody,target)
        (self.run/'unexpected').unlink()
        original=self.custody/'custodian/data/item'; (self.root/'hardlink').hardlink_to(original)
        with self.assertRaises(ValueError): m.collection_tree(self.root)
    def test_outer_socket_is_not_allowed_in_completed_collection(self):
        sock=socket.socket(socket.AF_UNIX)
        try:
            sock.bind(str(self.custody/'app.sock'))
            with self.assertRaises(ValueError): m.collection_tree(self.root)
            with self.assertRaises(ValueError): m.copy_custody(self.custody,self.root.parent/(self.root.name+'-copy'))
        finally: sock.close()
    def test_socket_identity_change_is_detected(self):
        before=m.collection_tree(self.root); self.sock.close(); (self.run/'custody.sock').unlink()
        self.sock=socket.socket(socket.AF_UNIX); self.sock.bind(str(self.run/'custody.sock'))
        self.assertNotEqual(m.collection_tree(self.root),before)

if __name__=='__main__': unittest.main()
