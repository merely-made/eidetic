#!/usr/bin/env python3
"""Isolated two-process acceptance collection; semantic verification is separate."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import time
import uuid
import run_native as native

READY_FOR_ACCEPTANCE = True
COMMANDS = {
    'turnstone': ['cargo', 'build', '--locked', '--offline', '-p', 'turnstone', '--features', 'authoring,inference', '--bin', 'turnstone'],
    'test-binary': ['cargo', 'test', '--locked', '--offline', '-p', 'turnstone', '--features', 'authoring,inference', '--lib', '--no-run'],
    'custodian': ['cargo', 'test', '--locked', '--offline', '-p', 'mere-resident', '--features', 'custody-receipt-host', '--test', 'custody_payload', '--no-run'],
}


def inventory(root):
    raw = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root)
    entries = {}
    for name in sorted(set(raw.split(b'\0'))):
        if not name:
            continue
        rel = os.fsdecode(name)
        path = root / rel
        if path.is_symlink():
            entries[rel] = {'symlink': os.readlink(path)}
        elif path.is_file():
            entries[rel] = {'sha256': native.sha(path)}
        elif not path.exists():
            entries[rel] = {'missing': True}
        else:
            raise ValueError('unexpected source type: ' + rel)
    if not entries:
        raise ValueError('empty source inventory')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    content = json.dumps({'head': head, 'files': entries}, sort_keys=True, separators=(',', ':')).encode()
    return hashlib.sha256(content).hexdigest()


def verify_builds(provenance, receipts, binaries, sources, inventories):
    if provenance.get('schema') != 'turnstone.m5-build-provenance/v1':
        raise ValueError('invalid build provenance schema')
    evidence = {}
    for name, path in receipts.items():
        receipt = json.loads(path.read_text())
        saved = provenance['artifacts'][name]
        if receipt.get('exit_code') != 0 or saved['receipt_sha256'] != native.sha(path) or saved['binary_sha256'] != binaries[name]:
            raise ValueError('receipt or binary differs: ' + name)
        if receipt['command'] != COMMANDS[name]:
            raise ValueError('unexpected build command: ' + name)
        if saved.get('source_before') != sources[name] or saved.get('source_after') != sources[name]:
            raise ValueError('current source differs from build: ' + name)
        inventory_path = inventories[name]
        inventory_document = json.loads(inventory_path.read_text())
        if native.sha(inventory_path) != sources[name] or saved.get('source_inventory_sha256') != sources[name]:
            raise ValueError('source inventory does not bind build: ' + name)
        if not inventory_document.get('head') or not inventory_document.get('files'):
            raise ValueError('empty source inventory document: ' + name)
        evidence[name] = {'source_inventory': 'frozen/' + inventory_path.name, 'receipt_sha256': native.sha(path), 'binary_sha256': binaries[name],
                          'source_hash': sources[name], 'command': receipt['command']}
    return evidence


def fixture_files(root):
    if root.is_symlink() or not root.is_dir():
        raise ValueError('invalid fixture directory')
    result = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError('nonregular fixture entry')
        if path.is_file():
            result[str(path.relative_to(root))] = native.sha(path)
    if not result:
        raise ValueError('fixture is empty')
    return result


def make_command(sandbox, frozen, devices, config, phase):
    command = native.namespace_command(sandbox, frozen, Path(devices['wayland']['path']),
                                       Path(devices['render']['path']), phase, config)
    # Keep the reviewed mount allowlist, replacing only the guarded entry point.
    tail = ['/usr/bin/python3', '/m5/input/namespace_phase.py', '/m5/input/config.json']
    if command[-3:] != tail:
        raise ValueError('namespace command entry changed')
    return command[:-3] + ['--ro-bind', str(frozen / 'acceptance_phase.py'), '/m5/input/acceptance_phase.py',
                           '/usr/bin/python3', '/m5/input/acceptance_phase.py', '/m5/input/config.json']


def validate_phase_scope(launch, cleanup, namespace, children):
    """Bind supervised child lifetimes to one owned isolated phase scope."""
    scope = launch.get('scope')
    invocation = cleanup.get('invocation')
    argv = launch.get('argv', [])
    if (not isinstance(scope, str) or not scope.startswith('m5-') or not scope.endswith('.scope')
        or not isinstance(invocation, str) or not re.fullmatch(r'[0-9a-f]{32}', invocation)
        or launch.get('exit_code') != 0 or launch.get('timed_out') is not False
        or launch.get('ownership_observed') is not True or cleanup.get('ownership_observed') is not True
        or cleanup.get('normal') is not True or cleanup.get('scope') != scope
        or not any(item.get('cgroup_empty_verified') is True for item in cleanup.get('cleanup', []))
        or not namespace.get('cgroup', {}).get('path', '').endswith('/' + scope)
        or not all(flag in argv for flag in ('bwrap','--unshare-user','--unshare-net','--unshare-pid','--unshare-ipc'))
        or '--unit' not in argv or argv[argv.index('--unit')+1] + '.scope' != scope):
        raise ValueError('phase scope or isolation evidence differs')
    pidns = namespace.get('pidns')
    if not isinstance(pidns, str) or not re.fullmatch(r'pid:\[\d+\]', pidns):
        raise ValueError('phase PID namespace missing')
    for child in children:
        if (child.get('pid_namespace') != pidns or not isinstance(child.get('pid'), int) or child['pid'] <= 0
            or not launch['start_ns'] <= child['start_ns'] < child['exit_ns'] <= launch['exit_ns']):
            raise ValueError('child outside its phase namespace or lifetime')
    return scope, invocation


def aggregate(out, hashes):
    sandbox = out / 'sandbox'
    runs = {name: json.loads((sandbox / name / 'processes.json').read_text()) for name in ('seed', 'reopen')}
    checks = {name: json.loads((sandbox / name / 'phase-checked.json').read_text()) for name in runs}
    seed, reopen = runs['seed'], runs['reopen']
    phase_scopes = {}
    for name, run in runs.items():
        launch = json.loads((out / (name + '-launcher.json')).read_text())
        cleanup = json.loads((out / (name + '-launcher.cleanup.json')).read_text())
        namespace = json.loads((sandbox / name / 'namespace.json').read_text())
        children = [run['app'], run['custody']]
        if name == 'seed': children.append(run['origin'])
        else: children.extend((run['inspection'], run['withdrawal_inspection']))
        phase_scopes[name] = (validate_phase_scope(launch, cleanup, namespace, children), launch)
    if (phase_scopes['seed'][0][0] == phase_scopes['reopen'][0][0]
        or phase_scopes['seed'][0][1] == phase_scopes['reopen'][0][1]
        or not phase_scopes['seed'][1]['exit_ns'] < phase_scopes['reopen'][1]['start_ns']):
        raise ValueError('phase scopes are not distinct and ordered')
    if not seed['app']['exit_ns'] < reopen['app']['start_ns']:
        raise ValueError('distinct app processes not demonstrated')
    if not seed['custody']['exit_ns'] < reopen['custody']['start_ns']:
        raise ValueError('distinct custody processes not demonstrated')
    if checks['seed']['custody_principal'] != checks['reopen']['custody_principal']:
        raise ValueError('reopened custody identity changed')
    for name in runs:
        runs[name]['custody']['public_identity'] = checks[name]['custody_principal']
        runs[name]['custody']['binary_sha256'] = hashes['custodian']
    native.record(out / 'processes.json', {'seed': seed['app'], 'reopen': reopen['app'],
        'custody_seed': seed['custody'], 'custody_reopen': reopen['custody'],
        'origin': seed['origin'], 'inspection': reopen['inspection'], 'withdrawal_inspection': reopen['withdrawal_inspection']})
    # Bind every generated phase artifact, the authoritative profile inventory,
    # and frozen build/source evidence before a fault-copy driver consumes them.
    import importlib.util
    checker = out / 'frozen' / 'namespace_phase.py'
    if checker.is_symlink() or native.sha(checker) != hashes['namespace_phase.py']:
        raise ValueError('frozen profile checker changed')
    spec = importlib.util.spec_from_file_location('m5_frozen_profile_checker', checker)
    namespace_phase = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(namespace_phase)
    inventory_path = sandbox / 'reopen' / 'inspection-source-hashes.json'
    expected_profile = json.loads(inventory_path.read_text())
    if namespace_phase.tree_hashes(sandbox / 'profile') != expected_profile:
        raise ValueError('primary profile differs from completed inspection inventory')
    paths = [out / 'inputs.json', out / 'processes.json', sandbox / 'selected.mereclip',
             sandbox / 'origin-info.json', sandbox / 'fixture-requests.json']
    for phase in runs:
        paths.extend((sandbox / phase).iterdir())
        paths.extend(out.glob(phase + '-launcher.*'))
    paths.extend(path for path in (out / 'frozen').iterdir() if path.suffix in ('.json', '.py', '.scn'))
    ledger = {}
    for path in sorted(set(paths)):
        if path.is_symlink() or not path.is_file():
            raise ValueError('nonregular collection artifact: ' + str(path))
        ledger[str(path.relative_to(out))] = native.sha(path)
    native.record(out / 'collection.json', {'schema': 'turnstone.m5-collection/v1',
        'status': 'collected-awaiting-semantic-and-headed-verification', 'files': ledger,
        'primary_profile': {'root': 'sandbox/profile',
                            'inventory': 'sandbox/reopen/inspection-source-hashes.json'},
        'phase_checks': {name: native.sha(sandbox / name / 'phase-checked.json') for name in runs}})



def execute(args):
    if not 1 <= args.seed_timeout <= 450 or not 1 <= args.reopen_timeout <= 300:
        raise ValueError('seed/reopen app timeouts exceed 450/300 seconds within the 600-second scope')
    out = args.output.resolve()
    if out.exists():
        raise ValueError('output must be a new directory')
    out.mkdir(mode=0o700, parents=True)
    sandbox, frozen = out / 'sandbox', out / 'frozen'
    sandbox.mkdir(mode=0o700)
    frozen.mkdir(mode=0o700)
    try:
        (sandbox / 'm5-disposable-marker').write_text('m5-disposable/v1\n')
        for name in ('data', 'config', 'cache', 'runtime', 'mere', 'profile', 'control'):
            (sandbox / name).mkdir(mode=0o700)
        here = Path(__file__).resolve().parent
        devices = native.device_identity(args.wayland_socket, args.gpu_device)
        inputs = {'turnstone': args.application, 'custodian': args.custodian, 'test-binary': args.test_binary,
                  'seed.scn': args.seed, 'reopen.scn': args.reopen,
                  **{name: here / name for name in ('run_native.py', 'namespace_phase.py', 'fixture_server.py',
                                                    'acceptance_phase.py', 'run_acceptance.py')}}
        hashes = {name: native.freeze_file(path.resolve(), frozen / name) for name, path in inputs.items()}
        roots = {'turnstone': args.source_root.resolve(), 'test-binary': args.source_root.resolve(),
                 'custodian': args.custody_source_root.resolve()}
        sources = {name: inventory(root) for name, root in roots.items()}
        receipts = {'turnstone': args.app_build_receipt, 'test-binary': args.test_build_receipt,
                    'custodian': args.custody_build_receipt}
        frozen_receipts = {}
        for name, path in receipts.items():
            destination = frozen / (name + '-build.json')
            native.freeze_file(path, destination)
            frozen_receipts[name] = destination
        frozen_provenance = frozen / 'build-provenance.json'
        native.freeze_file(args.build_provenance, frozen_provenance)
        provenance = json.loads(frozen_provenance.read_text())
        inventories = {'turnstone': args.app_source_inventory, 'test-binary': args.test_source_inventory,
                       'custodian': args.custody_source_inventory}
        frozen_inventories = {}
        for name, path in inventories.items():
            destination = frozen / (name + '-source-inventory.json')
            native.freeze_file(path, destination)
            frozen_inventories[name] = destination
        evidence = verify_builds(provenance, frozen_receipts, hashes, sources, frozen_inventories)
        fixtures = fixture_files(args.fixtures)
        shutil.copytree(args.fixtures, frozen / 'fixtures')
        shutil.copytree(frozen / 'fixtures', sandbox / 'fixture-source')
        if fixtures != fixture_files(frozen / 'fixtures') or fixtures != fixture_files(sandbox / 'fixture-source'):
            raise ValueError('fixture changed while copying')
        native.record(out / 'inputs.json', {'kind': 'model-free-acceptance-collection', 'files': hashes,
            'fixtures': fixtures, 'devices': devices, 'builds': evidence,
            'provenance_sha256': native.sha(frozen_provenance)})
        for phase in ('seed', 'reopen'):
            if any(inventory(root) != sources[name] for name, root in roots.items()):
                raise ValueError('source changed before phase')
            if native.device_identity(args.wayland_socket, args.gpu_device) != devices:
                raise ValueError('display/render device changed')
            config = {'phase': phase, 'output': '/m5/run', 'scenario': '/m5/input/' + phase + '.scn',
                'fixture_server': '/m5/input/fixture_server.py', 'custodian': '/m5/input/custodian',
                'application': '/m5/input/turnstone', 'test_binary': '/m5/input/test-binary',
                'inspection_observation': '/m5/run/reopen/cold_sum.structured-research.json',
                'source_observation': '/m5/run/seed/redshank_note_admitted.saved-sources.json',
                'timeout_seconds': args.seed_timeout if phase == 'seed' else args.reopen_timeout, 'inspection_timeout_seconds': 90,
                'memory_bytes': 3 * 1024**3, 'tasks_max': 128, 'cpu_percent': 150, 'devices': devices,
                'environment': {'PATH': '/usr/bin', 'WAYLAND_DISPLAY': 'wayland-0',
                                'RUST_LOG': 'warn,wgpu_core::instance=debug'}}
            config_path = frozen / (phase + '-config.json')
            native.record(config_path, config)
            cmd = make_command(sandbox, frozen, devices, config_path, phase)
            unit = 'm5-acceptance-' + phase + '-' + uuid.uuid4().hex
            scope = native.cgroup_scope(cmd, config['memory_bytes'], config['tasks_max'],
                                        config['cpu_percent'], unit, 600)
            result = native.run_scope(scope, unit, out / (phase + '-launcher.log'), 610)
            native.record(out / (phase + '-launcher.json'), result)
        if any(inventory(root) != sources[name] for name, root in roots.items()):
            raise ValueError('source changed during collection')
        aggregate(out, hashes)
    except BaseException as exc:
        native.record(out / 'failure.json', {'error': repr(exc), 'time_ns': time.time_ns()})
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('output', 'application', 'custodian', 'test-binary', 'seed', 'reopen', 'fixtures',
                 'source-root', 'custody-source-root', 'wayland-socket', 'gpu-device',
                 'app-build-receipt', 'test-build-receipt', 'custody-build-receipt', 'build-provenance',
                 'app-source-inventory', 'test-source-inventory', 'custody-source-inventory'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--seed-timeout', type=int, default=450)
    parser.add_argument('--reopen-timeout', type=int, default=300)
    args = parser.parse_args()
    if not READY_FOR_ACCEPTANCE:
        raise SystemExit('DRAFT ONLY: acceptance collection awaits scoped review')
    execute(args)


if __name__ == '__main__':
    main()
