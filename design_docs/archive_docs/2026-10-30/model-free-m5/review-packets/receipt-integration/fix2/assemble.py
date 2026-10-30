#!/usr/bin/env python3
"""Copy verified M5 collection evidence to a separate, draft-only receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import types

VERIFY_SHA256 = 'd40f0bf2ce2f3ed94a0d2cefc231e20ce8dff9add7e94fc2c6d7a58229073e03'
verify_path = Path(__file__).with_name('verify.py')
if verify_path.is_symlink() or not verify_path.is_file(): raise ValueError('missing validation module')
verify_bytes = verify_path.read_bytes()
if hashlib.sha256(verify_bytes).hexdigest() != VERIFY_SHA256: raise ValueError('validation module changed')
verify = types.ModuleType('m5_pinned_receipt_verify')
verify.__file__ = str(verify_path)
exec(compile(verify_bytes, str(verify_path), 'exec'), verify.__dict__)
OPERATIONS, FAULT_REQUIRED, INTACT_REQUIRED, TOOLS = verify.OPERATIONS, verify.FAULT_REQUIRED, verify.INTACT_REQUIRED, verify.TOOLS
COMMANDS = verify.COMMANDS


def read(root, rel):
    return json.loads(verify.regular(root, rel).read_text())


def fault_ledger(collection):
    if collection.get('schema') != 'turnstone.m5-missing-collection/v1' or collection.get('status') != 'collected-awaiting-headed-and-semantic-verification':
        raise ValueError('completed fault collection required')
    ledger = collection.get('files')
    if not isinstance(ledger, dict) or not FAULT_REQUIRED <= set(ledger):
        raise ValueError('fault ledger missing required evidence')
    return ledger


def check_binary(root, inputs, name):
    actual = verify.sha(verify.regular(root, 'frozen/' + name))
    if actual != inputs['files'][name] or actual != inputs['builds'][name]['binary_sha256']:
        raise ValueError('frozen binary differs: ' + name)


def check_builds(root, ledger):
    inputs = read(root, 'inputs.json')
    for name in COMMANDS:
        check_binary(root, inputs, name)
    verify.check_collector_bindings(inputs, ledger, lambda rel: read(root,rel),
                                    {n:verify.sha(verify.regular(root,'frozen/'+n)) for n in COMMANDS})
    return inputs


def intact_evidence(root, collection):
    if collection.get('schema') != 'turnstone.m5-collection/v1' or collection.get('status') != 'collected-awaiting-semantic-and-headed-verification':
        raise ValueError('completed intact collection required')
    required = set(INTACT_REQUIRED)
    required |= {'sandbox/seed/op_' + op + ext for op in OPERATIONS for ext in ('.structured-research.json','.png')}
    ledger = verify.validate_ledger(root, collection.get('files'), required)
    check_builds(root, ledger)
    expected = {'root':'sandbox/profile','inventory':'sandbox/reopen/inspection-source-hashes.json'}
    if collection.get('primary_profile') != expected:
        raise ValueError('intact profile inventory differs')
    for phase in ('seed','reopen'):
        check = 'sandbox/' + phase + '/phase-checked.json'
        if ledger[check] != collection['phase_checks'][phase]: raise ValueError('phase check differs')
        cleanup = read(root, phase + '-launcher.cleanup.json')
        if not cleanup['normal'] or not cleanup['ownership_observed'] or not any(x.get('cgroup_empty_verified') is True for x in cleanup['cleanup']):
            raise ValueError('scope cleanup unverified')
    for name in ('inspection','withdrawal_inspection'):
        if read(root, 'sandbox/reopen/' + name + '-integrity.json')['passed'] is not True:
            raise ValueError('owner inspection integrity unverified')
    return ledger


def checked_write(src, dst, expected):
    data = src.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected: raise ValueError('evidence changed while copying')
    dst.parent.mkdir(parents=True, exist_ok=True)
    with dst.open('xb') as out: out.write(data)
    if verify.sha(dst) != expected: raise ValueError('copy hash differs')


def bind_profile_source(profile, inventory, relative):
    path = verify.regular(profile, relative)
    digest = inventory.get(relative)
    if not isinstance(digest, str) or verify.sha(path) != digest:
        raise ValueError('selected source differs from inspected owner inventory')
    return path, digest


def check_tree(root, expected):
    root = Path(root)
    if root.is_symlink() or not root.is_dir(): raise ValueError('invalid inventory root')
    actual = {}
    def visit(directory):
        with os.scandir(directory) as entries:
            for entry in entries:
                path = Path(entry.path)
                rel = str(path.relative_to(root))
                mode = entry.stat(follow_symlinks=False).st_mode
                if stat.S_ISDIR(mode):
                    actual[rel + '/'] = 'directory'
                    visit(path)
                elif stat.S_ISREG(mode) and entry.stat(follow_symlinks=False).st_nlink == 1:
                    actual[rel] = verify.sha(path)
                else:
                    raise ValueError('nonregular or hard-linked inventory entry: ' + rel)
    visit(root)
    if actual != expected: raise ValueError('inventory tree differs')


def assemble(intact, fault, review, output):
    intact, fault = Path(intact).resolve(strict=True), Path(fault).resolve(strict=True)
    output = Path(output).parent.resolve(strict=True) / Path(output).name
    if output.exists() or output.is_relative_to(intact) or output.is_relative_to(fault) or intact.is_relative_to(output) or fault.is_relative_to(output):
        raise ValueError('receipt output must be new and separate')
    fault_collection_file = verify.regular(fault, 'collection.json')
    fault_collection_data = fault_collection_file.read_bytes()
    fault_collection_hash = hashlib.sha256(fault_collection_data).hexdigest()
    fault_doc = json.loads(fault_collection_data)
    fault_files = fault_ledger(fault_doc)
    verify.validate_ledger(fault, fault_files, FAULT_REQUIRED)
    intact_doc = read(intact, 'collection.json')
    intact_files = intact_evidence(intact, intact_doc)
    inspection = read(intact, 'sandbox/reopen/structured-inspection.json')
    source_relative = inspection['selection']['source_relative_path']
    profile_inventory = read(intact, 'sandbox/reopen/inspection-source-hashes.json')
    check_tree(intact / 'sandbox/profile', profile_inventory)
    selected_source, selected_digest = bind_profile_source(intact / 'sandbox/profile', profile_inventory, source_relative)
    fault_input = read(fault, 'inputs.json')
    integrity = read(fault, 'primary-integrity.json')
    if not integrity.get('unchanged') or integrity.get('before') != integrity.get('after') or integrity.get('before') != fault_input.get('collection_hashes'):
        raise ValueError('fault primary integrity differs')
    if fault_doc.get('run_id') != fault_input.get('run_id') or fault_doc['run_id'] != read(fault, 'frozen/missing-input.json').get('run_id'):
        raise ValueError('fault run identity differs')
    cleanup = read(fault, 'launcher.cleanup.json')
    if not cleanup['normal'] or not cleanup['ownership_observed'] or not any(x.get('cgroup_empty_verified') is True for x in cleanup['cleanup']):
        raise ValueError('fault scope cleanup unverified')
    final = read(fault, 'output/processes-final.json')
    if final.get('failure') or final.get('cleanup_errors') or final.get('socket_cleanup') != 'owned socket absent':
        raise ValueError('fault process cleanup unverified')
    # The intact input inventory contains every file as it stood before the
    # fault. This joins the independently collected runs, not merely run IDs.
    snapshot = fault_input['collection_hashes']
    if snapshot.get('sandbox/profile/' + source_relative) != selected_digest:
        raise ValueError('fault copy did not originate from inspected source')
    for rel in ('collection.json', *intact_files):
        source = verify.regular(intact, rel)
        if snapshot.get(rel) != verify.sha(source): raise ValueError('fault refers to another intact collection: ' + rel)
    review = Path(review)
    if review.is_symlink() or not review.is_file(): raise ValueError('human review missing')
    review_data = json.loads(review.read_text())
    if not review_data.get('reviewer') or not review_data.get('reviewed_frames'): raise ValueError('human review incomplete')
    tools = {}
    for name, (rel, digest) in TOOLS.items():
        source = (Path(__file__).parent / rel).resolve(strict=True)
        if verify.sha(source) != digest: raise ValueError('reviewed validation module changed: ' + name)
        tools[name] = (source,digest)
    tools['integration-verify.py'] = (verify_path, VERIFY_SHA256)
    output.mkdir(mode=0o700)
    files = {}
    for label, root, ledger in (('intact',intact,intact_files), ('fault',fault,fault_files)):
        for rel, digest in ledger.items():
            dst = label + '/' + rel
            checked_write(verify.regular(root, rel), output / dst, digest)
            files[dst] = digest
        source = verify.regular(root,'collection.json')
        dst = label + '/collection.json'
        digest = snapshot['collection.json'] if label == 'intact' else fault_collection_hash
        checked_write(source, output / dst, digest); files[dst] = digest
    selected_path = 'intact/sandbox/profile/' + source_relative
    checked_write(selected_source, output / selected_path, selected_digest); files[selected_path] = selected_digest
    for name, (source,digest) in tools.items():
        dst = 'tools/' + name; checked_write(source, output / dst,digest); files[dst] = digest
    data = review.read_bytes(); (output / 'review.json').write_bytes(data); files['review.json'] = hashlib.sha256(data).hexdigest()
    # Recheck actual frozen binaries after all copying before recording the
    # assembly-time binary observation; they are intentionally not copied.
    check_builds(intact, intact_files)
    manifest = {'schema':'turnstone.m5-receipt-integration-draft/v1','status':'draft',
                'fault_run_id':fault_doc['run_id'],
                'binary_hashes_checked_at_assembly':{n:verify.sha(verify.regular(intact,'frozen/'+n)) for n in COMMANDS},
                'files':files}
    (output / 'manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    return manifest


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('intact','fault','review','output'): p.add_argument('--'+name, type=Path, required=True)
    a=p.parse_args(); assemble(a.intact,a.fault,a.review,a.output)
    print('Draft evidence copied; semantic and headed acceptance remain unverified.')

if __name__ == '__main__': main()
