#!/usr/bin/env python3
"""Reviewed-container entry for actual seed/reopen collection."""
import json, stat, sys
from pathlib import Path
import namespace_phase


def validate_devices(config):
    devices = config['devices']
    sock = Path('/m5/run/runtime/wayland-0').stat()
    render = Path('/dev/dri') / Path(devices['render']['path']).name
    dev = render.stat()
    if not stat.S_ISSOCK(sock.st_mode) or (sock.st_dev, sock.st_ino) != (devices['wayland']['device'], devices['wayland']['inode']):
        raise SystemExit('mounted Wayland socket identity differs')
    if not stat.S_ISCHR(dev.st_mode) or (dev.st_dev, dev.st_ino, dev.st_rdev) != (devices['render']['device'], devices['render']['inode'], devices['render']['rdev']):
        raise SystemExit('mounted render device identity differs')
    sysinfo = devices['sysfs']
    sysroot = Path(sysinfo['root']); target = Path('/sys/dev/char') / sysinfo['devnum']
    if target.resolve(strict=True) != Path(sysinfo['target']): raise SystemExit('render sysfs target changed')
    rootstat, targetstat = sysroot.stat(), target.stat()
    if (rootstat.st_dev, rootstat.st_ino, targetstat.st_dev, targetstat.st_ino) != (sysinfo['device'], sysinfo['inode'], sysinfo['target_device'], sysinfo['target_inode']):
        raise SystemExit('mounted render sysfs identity differs')


def check_run(config):
    phase = config['phase']
    run = Path('/m5/run') / phase
    processes = json.loads((run / 'processes.json').read_text())
    probe = json.loads((run / 'inference-route.json').read_text())
    if processes.get('custody_ready') != processes.get('custody_peer_after_probe'):
        raise ValueError('custody peer changed during probe')
    principal = {'profile': probe.get('profile'), 'public_key': probe.get('public_key')}
    if not principal['profile'] or not principal['public_key'] or probe.get('outcome') != 'closed-during-open':
        raise ValueError('custody identity or inference route differs')
    names = [phase + '_runtime.runtime.json']
    if phase == 'reopen': names.append('model_unavailable.runtime.json')
    for name in names:
        runtime = json.loads((run / name).read_text())
        if runtime.get('pid') != processes['app']['pid'] or runtime.get('custody_principal') != principal:
            raise ValueError('app runtime identity differs')
        activity = runtime['inference']
        if not activity.get('client_compiled') or activity.get('corpus_publications') != 0 or activity.get('generation_requests') != 0:
            raise ValueError('generation or publication observed')
        if activity.get('resident_model_loads') is not None:
            raise ValueError('unknown resident counter misrepresented')
        if phase == 'reopen' and (runtime.get('network_offline') is not True or runtime.get('offline_acquisition_attempts') != 0):
            raise ValueError('offline reopen made acquisition attempts')
    if Path('/m5/run/fixture-source').exists():
        raise ValueError('original source bodies remain after phase')
    frames = sorted(run.glob('*.png'))
    if not frames: raise ValueError('headed frames absent')
    dimensions = {p.name: namespace_phase.validate_png(p) for p in frames}
    if phase == 'reopen':
        seed = json.loads(Path('/m5/run/seed/processes.json').read_text())
        if not seed['origin']['exit_ns'] < processes['app']['start_ns']:
            raise ValueError('origin was not stopped before reopen')
        if not processes['app']['exit_ns'] < processes['inspection']['start_ns'] or processes['inspection']['exit_code'] != 0:
            raise ValueError('post-exit owner inspection missing')
    if phase == 'reopen' and (processes['withdrawal_inspection']['exit_code'] != 0 or
        not processes['inspection']['exit_ns'] < processes['withdrawal_inspection']['start_ns']):
        raise ValueError('sequential withdrawal inspection missing')
    namespace_phase.write(run / 'phase-checked.json', {'schema':'turnstone.m5-phase/v1',
        'phase':phase, 'custody_principal':principal, 'runtime_files':names,
        'frames':dimensions, 'status':'collection-only-not-semantic-or-headed-approval'})


def main():
    if sys.argv != ['/m5/input/acceptance_phase.py', '/m5/input/config.json']:
        raise SystemExit('unexpected entry arguments')
    config = json.loads(Path('/m5/input/config.json').read_text())
    if config.get('preflight') or config.get('phase') not in ('seed','reopen') or config.get('output') != '/m5/run':
        raise ValueError('invalid acceptance configuration')
    validate_devices(config)
    namespace_phase.phase(config)
    check_run(config)


if __name__ == '__main__': main()
