import json, os, tempfile, unittest
from pathlib import Path
import run_missing as run

class FaultLedger(unittest.TestCase):
    def fixture(self, root):
        for rel in run.FAULT_LEDGER_REQUIRED:
            path=root/rel; path.parent.mkdir(parents=True,exist_ok=True); path.write_text('{}')
        def write(rel,value): (root/rel).write_text(json.dumps(value))
        write('primary-integrity.json',{'unchanged':True,'before':{'a':'hash'},'after':{'a':'hash'}})
        write('launcher.cleanup.json',{'normal':True,'ownership_observed':True,'cleanup':[{'cgroup_empty_verified':True}]})
        write('output/processes-final.json',{'socket_cleanup':'owned socket absent','cleanup_errors':[]})
        for name in run.FROZEN: (root/'frozen'/name).write_text('binary')
        write('inputs.json',{'frozen':run.regular_tree(root/'frozen')})
        (root/'copy').mkdir(); (root/'copy'/'private-store').write_text('private')

    def test_completed_ledger_binds_artifacts_excludes_stores_and_binaries(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); self.fixture(root); run.complete_collection(root,'run')
            value=json.loads((root/'collection.json').read_text())
            self.assertEqual(value['run_id'],'run')
            for rel in run.FAULT_LEDGER_REQUIRED: self.assertEqual(value['files'][rel],run.native.sha(root/rel))
            self.assertNotIn('copy/private-store',value['files'])
            for name in run.FROZEN: self.assertNotIn('frozen/'+name,value['files'])

    def test_missing_file_cleanup_and_primary_fail_before_completion(self):
        for bad in ('missing','cleanup','primary','process'):
            with self.subTest(bad=bad), tempfile.TemporaryDirectory() as d:
                root=Path(d); self.fixture(root)
                if bad=='missing': (root/'output/phase-checked.json').unlink()
                elif bad=='cleanup': (root/'launcher.cleanup.json').write_text('{"normal":false}')
                elif bad=='primary': (root/'primary-integrity.json').write_text('{"unchanged":false}')
                else: (root/'output/processes-final.json').write_text('{"failure":"failed"}')
                with self.assertRaises(ValueError): run.complete_collection(root,'run')
                self.assertFalse((root/'collection.json').exists())

    def test_artifact_links_and_linked_parent_refused(self):
        for kind in ('symlink','hardlink','parent'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as d:
                root=Path(d)/'run'; root.mkdir(); self.fixture(root)
                if kind=='symlink': (root/'output/link').symlink_to(root/'inputs.json')
                elif kind=='hardlink': os.link(root/'inputs.json',root/'output/link')
                else:
                    moved=Path(d)/'outside'; (root/'output').rename(moved); (root/'output').symlink_to(moved,target_is_directory=True)
                with self.assertRaises(ValueError): run.complete_collection(root,'run')
                self.assertFalse((root/'collection.json').exists())

    def test_frozen_module_and_executable_drift_prevent_completion(self):
        for kind in ('script','binary','missing_binary'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as d:
                root=Path(d); self.fixture(root)
                if kind=='script': (root/'frozen/missing.scn').write_text('changed')
                elif kind=='binary': (root/'frozen/turnstone').write_text('changed')
                else: (root/'frozen/turnstone').unlink()
                with self.assertRaises(ValueError): run.finish_collection(root,'run')
                self.assertFalse((root/'collection.json').exists())
                self.assertIn('frozen',json.loads((root/'failure.json').read_text())['error'])

if __name__=='__main__': unittest.main()
