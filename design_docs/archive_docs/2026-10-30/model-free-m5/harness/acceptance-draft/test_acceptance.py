import copy
import json
from pathlib import Path
import tempfile
import unittest
import run_acceptance as run


class Admission(unittest.TestCase):
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
            for phase, start, ns in [('seed',10,'pid:[1]'), ('reopen',30,'pid:[2]')]:
                directory = root/'sandbox'/phase
                directory.mkdir(parents=True)
                process = {'start_ns':start, 'exit_ns':start+5, 'exit_code':0, 'pid_namespace':ns}
                result = {'app':process, 'custody':process, 'origin':process, 'inspection':process,
                          'withdrawal_inspection':process}
                (directory/'processes.json').write_text(json.dumps(result))
                (directory/'phase-checked.json').write_text(json.dumps({'custody_principal':{'profile':'family','public_key':[1]}}))
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
