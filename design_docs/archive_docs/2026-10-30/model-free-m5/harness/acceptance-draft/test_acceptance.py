import copy
import json
from pathlib import Path
import tempfile
import unittest
import run_acceptance as run


class Admission(unittest.TestCase):
    def test_phase_binding_rejects_wrong_namespace_and_missing_isolation(self):
        launch={'scope':'m5-one.scope','start_ns':1,'exit_ns':10,'exit_code':0,'timed_out':False,
                'ownership_observed':True,'argv':['--unit','m5-one','bwrap','--unshare-user','--unshare-net','--unshare-pid','--unshare-ipc']}
        cleanup={'scope':'m5-one.scope','invocation':'a'*32,'normal':True,'ownership_observed':True,
                 'cleanup':[{'cgroup_empty_verified':True}]}
        ns={'pidns':'pid:[1]','cgroup':{'path':'/app/m5-one.scope'}}
        child={'pid':7,'pid_namespace':'pid:[1]','start_ns':2,'exit_ns':9}
        self.assertEqual(run.validate_phase_scope(launch,cleanup,ns,[child]),('m5-one.scope','a'*32))
        for bad in ({**child,'pid_namespace':'pid:[2]'}, {**child,'exit_ns':11}):
            with self.assertRaises(ValueError): run.validate_phase_scope(launch,cleanup,ns,[bad])
        wrong=copy.deepcopy(launch); wrong['argv'].remove('--unshare-pid')
        with self.assertRaises(ValueError): run.validate_phase_scope(wrong,cleanup,ns,[child])
        wrong=copy.deepcopy(cleanup); wrong['cleanup']=[{'cgroup_empty_verified':False}]
        with self.assertRaises(ValueError): run.validate_phase_scope(launch,wrong,ns,[child])
    def test_changed_binary_source_and_command_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            receipts, sources, binaries, artifacts, inventories = {}, {}, {}, {}, {}
            for name, command in run.COMMANDS.items():
                path = root / (name + '.json')
                path.write_text(json.dumps({'exit_code': 0, 'command': command}))
                inv = root / (name + '-inventory.json')
                inv.write_text(json.dumps({'head':'commit', 'files':{'src/helper.rs':{'sha256':'d'*64}}}))
                inventories[name] = inv
                receipts[name], sources[name], binaries[name] = path, run.native.sha(inv), 'b' * 64
                artifacts[name] = {'receipt_sha256':run.native.sha(path), 'binary_sha256':'b'*64,
                                   'source_before':sources[name], 'source_after':sources[name],
                                   'source_inventory_sha256':sources[name]}
            provenance = {'schema':'turnstone.m5-build-provenance/v1', 'artifacts':artifacts}
            self.assertEqual(set(run.verify_builds(provenance, receipts, binaries, sources, inventories)), set(run.COMMANDS))
            for field, value in [('binary_sha256','c'*64), ('source_before','c'*64), ('source_after','c'*64)]:
                wrong = copy.deepcopy(provenance)
                wrong['artifacts']['turnstone'][field] = value
                with self.assertRaises(ValueError): run.verify_builds(wrong, receipts, binaries, sources, inventories)
            path = receipts['custodian']
            path.write_text(json.dumps({'exit_code':0, 'command':run.COMMANDS['custodian']+['--all-features']}))
            provenance['artifacts']['custodian']['receipt_sha256'] = run.native.sha(path)
            with self.assertRaises(ValueError): run.verify_builds(provenance, receipts, binaries, sources, inventories)

    def test_fixture_links_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'a.html').write_text('fixture')
            self.assertEqual(set(run.fixture_files(root)), {'a.html'})
            (root/'alias').symlink_to(root/'a.html')
            with self.assertRaises(ValueError): run.fixture_files(root)

    def test_collection_does_not_claim_semantic_acceptance(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for phase, start, ns in [('seed',10,'pid:[1]'), ('reopen',30,'pid:[1]')]:
                directory = root/'sandbox'/phase
                directory.mkdir(parents=True)
                process = {'pid':1, 'start_ns':start, 'exit_ns':start+5, 'exit_code':0, 'pid_namespace':ns}
                result = {'app':process, 'custody':process, 'origin':process, 'inspection':process,
                          'withdrawal_inspection':process}
                (directory/'processes.json').write_text(json.dumps(result))
                (directory/'phase-checked.json').write_text(json.dumps({'custody_principal':{'profile':'family','public_key':[1]}}))
                scope = 'm5-' + phase + '.scope'
                launch = {'scope':scope,'start_ns':start-1,'exit_ns':start+6,'exit_code':0,
                          'timed_out':False,'ownership_observed':True,
                          'argv':['--unit','m5-'+phase,'bwrap','--unshare-user','--unshare-net','--unshare-pid','--unshare-ipc']}
                cleanup = {'scope':scope,'invocation':('a' if phase=='seed' else 'b')*32,
                           'normal':True,'ownership_observed':True,'cleanup':[{'cgroup_empty_verified':True}]}
                (root/(phase+'-launcher.json')).write_text(json.dumps(launch))
                (root/(phase+'-launcher.cleanup.json')).write_text(json.dumps(cleanup))
                (directory/'namespace.json').write_text(json.dumps({'pidns':ns,'cgroup':{'path':'/scope/'+scope}}))
            profile = root/'sandbox/profile'; profile.mkdir()
            (profile/'owner').write_text('owner bytes')
            (root/'frozen').mkdir()
            (root/'inputs.json').write_text('{}')
            for name in ('selected.mereclip', 'origin-info.json', 'fixture-requests.json'):
                (root/'sandbox'/name).write_text('{}')
            import shutil
            shutil.copy2(Path(run.__file__).with_name('namespace_phase.py'), root/'frozen/namespace_phase.py')
            bound_hashes = {'custodian':'hash', 'namespace_phase.py':run.native.sha(root/'frozen/namespace_phase.py')}
            import namespace_phase
            (root/'sandbox/reopen/inspection-source-hashes.json').write_text(json.dumps(namespace_phase.tree_hashes(profile)))
            run.aggregate(root, bound_hashes)
            collected = json.loads((root/'collection.json').read_text())
            self.assertIn('awaiting', collected['status'])
            self.assertEqual(collected['files']['sandbox/reopen/processes.json'],
                             run.native.sha(root/'sandbox/reopen/processes.json'))
            self.assertIn(collected['primary_profile']['inventory'], collected['files'])
            self.assertIn('sandbox/selected.mereclip', collected['files'])
            # A namespace inode may be reused after the prior phase exits.
            launch_path = root/'reopen-launcher.json'
            clean_path = root/'reopen-launcher.cleanup.json'
            ns_path = root/'sandbox/reopen/namespace.json'
            originals = [p.read_text() for p in (launch_path, clean_path, ns_path)]
            for path, field, value in [(launch_path,'start_ns',12), (clean_path,'invocation','a'*32),
                                       (clean_path,'scope','m5-other.scope'), (ns_path,'cgroup',{'path':'/wrong'})]:
                obj=json.loads(path.read_text()); obj[field]=value; path.write_text(json.dumps(obj))
                with self.assertRaises(ValueError): run.aggregate(root, bound_hashes)
                for p, original in zip((launch_path,clean_path,ns_path),originals): p.write_text(original)
            wrong = root/'sandbox/reopen/phase-checked.json'
            wrong.write_text(json.dumps({'custody_principal':{'profile':'other','public_key':[1]}}))
            with self.assertRaises(ValueError):run.aggregate(root, bound_hashes)


class Integrity(unittest.TestCase):
    def test_failure_and_symlink_are_recorded_without_hiding_other_trees(self):
        import namespace_phase as phase
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            trees=[]
            for name in ('primary','authoritative','working'):
                p=root/name;p.mkdir();(p/'a-owner').write_text('evidence');trees.append(p)
            before=[phase.tree_hashes(p) for p in trees]
            result=phase.inspection_integrity(trees[0],before[0],trees[1],before[1],trees[2],before[2])
            self.assertTrue(result['passed'])
            (trees[2]/'injected').symlink_to(trees[2]/'a-owner')
            result=phase.inspection_integrity(trees[0],before[0],trees[1],before[1],trees[2],before[2],'stop failed')
            self.assertFalse(result['passed'])
            self.assertIn('error',result['snapshots']['working'])
            self.assertEqual(result['snapshots']['working']['path'], 'injected')
            self.assertEqual(result['snapshots']['working']['reason'], 'symlink')
            self.assertIn('a-owner', result['snapshots']['working']['partial_tree'])
            self.assertTrue(result['primary_unchanged'])
            self.assertTrue(result['authoritative_copy_unchanged'])
            self.assertEqual(result['cleanup_error'],'stop failed')

if __name__ == '__main__': unittest.main()
