#!/usr/bin/env python3
"""Capture exact guarded build, artifact hash and source stability for preflight."""
import argparse, hashlib, json, os, re, subprocess, uuid
from pathlib import Path
import run_native as native

GATE = Path('/home/markik/Code/testing/custody-contract-20261029/m2-queued-gate.py')
RECEIPTS = GATE.parent
TARGET = Path('/home/markik/Code/target/debug')

def inventory_bytes(root):
    raw = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root)
    entries = {}
    for name in sorted(set(raw.split(b'\0'))):
        if not name: continue
        rel = os.fsdecode(name); path = root / rel
        if path.is_symlink(): entries[rel] = {'symlink':os.readlink(path)}
        elif path.is_file(): entries[rel] = {'sha256':native.sha(path)}
        elif not path.exists(): entries[rel] = {'missing':True}
        else: raise ValueError('unexpected source type: ' + rel)
    if not entries: raise ValueError('empty source inventory')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    content = json.dumps({'head':head, 'files':entries}, sort_keys=True, separators=(',', ':')).encode()
    return content

def inventory(root):
    return hashlib.sha256(inventory_bytes(root)).hexdigest()

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--kind', choices=('turnstone', 'test-binary', 'custodian'), required=True)
parser.add_argument('--source-root', required=True, type=Path)
parser.add_argument('--output', required=True, type=Path)
a = parser.parse_args(); root = a.source_root.resolve(); output = a.output.resolve()
if output.exists(): raise SystemExit('output must be new')
output.mkdir(mode=0o700, parents=True)
commands = {
    'turnstone':['cargo','build','--locked','--offline','-p','turnstone','--features','authoring,inference','--bin','turnstone'],
    'test-binary':['cargo','test','--locked','--offline','-p','turnstone','--features','authoring,inference','--lib','--no-run'],
    'custodian':['cargo','test','--locked','--offline','-p','mere-resident','--features','custody-receipt-host','--test','custody_payload','--no-run']}
try:
    source_bytes = inventory_bytes(root)
    before = hashlib.sha256(source_bytes).hexdigest(); name = 'm5-provenance-' + a.kind + '-' + uuid.uuid4().hex[:12]
    result = subprocess.run(['python3', str(GATE), name, str(root), *commands[a.kind]], check=False)
    receipt_path = RECEIPTS / (name + '.json'); receipt = json.loads(receipt_path.read_text())
    after = inventory(root)
    if result.returncode != 0 or receipt['exit_code'] != 0 or before != after:
        raise ValueError('guarded build failed or source changed')
    log = (RECEIPTS / receipt['log']).read_text()
    if a.kind == 'turnstone': binary = TARGET / 'turnstone'
    else:
        matches = re.findall(r'Executable (?:tests/custody_payload.rs|unittests src/lib.rs) \(([^)]+)\)', log)
        if len(matches) != 1: raise ValueError('could not identify unique executable')
        binary = Path(matches[0])
    if not binary.is_file() or binary.is_symlink(): raise ValueError('build executable missing')
    inventory_path = output / 'source-inventory.json'
    inventory_path.write_bytes(source_bytes)
    native.record(output / 'build.json', {'kind':a.kind, 'receipt':str(receipt_path),
        'receipt_sha256':native.sha(receipt_path), 'binary':str(binary), 'binary_sha256':native.sha(binary),
        'source_before':before, 'source_after':after,
        'source_inventory':str(inventory_path), 'source_inventory_sha256':native.sha(inventory_path)})
except BaseException as exc:
    native.record(output / 'failure.json', {'error':repr(exc)})
    raise
