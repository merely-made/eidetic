#!/usr/bin/env python3
"""Run only the minimal, empty-profile M5 GUI/custody startup preflight."""
import json, os, stat, sys
from pathlib import Path
import namespace_phase

config = json.loads(Path('/m5/input/config.json').read_text())
if sys.argv != ['/m5/input/preflight_phase.py', '/m5/input/config.json']:
    raise SystemExit('unexpected preflight arguments')
if config.get('preflight') is not True or config.get('phase') != 'seed':
    raise SystemExit('invalid preflight configuration')
scenario = Path('/m5/input/seed.scn').read_text()
if scenario != '# Bounded GUI and custody startup only; contains no retained content.\nsettle 60\nrecord-runtime gui_preflight\ncapture gui_preflight\n':
    raise SystemExit('preflight scenario is not the fixed empty-profile check')
if any(Path('/m5/run/fixture-source').iterdir()):
    raise SystemExit('preflight fixture source is not empty')
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
namespace_phase.phase(config)
run = Path('/m5/run/seed')
runtime = json.loads((run / 'gui_preflight.runtime.json').read_text())
probe = json.loads((run / 'inference-route.json').read_text())
processes = json.loads((run / 'processes.json').read_text())
if runtime.get('pid') != processes['app']['pid'] or runtime.get('network_offline') is not True:
    raise SystemExit('GUI runtime identity or offline state differs')
if processes.get('custody_peer_after_probe') != processes.get('custody_ready'):
    raise SystemExit('fixture peer changed during route probe')
if runtime.get('custody_principal') != {'profile': probe.get('profile'), 'public_key': probe.get('public_key')}:
    raise SystemExit('GUI and probe custody principals differ')
activity = runtime['inference']
if not activity.get('client_compiled') or activity.get('corpus_publications') != 0 or activity.get('generation_requests') != 0:
    raise SystemExit('GUI did not start idle with inference client present')
if runtime.get('offline_acquisition_attempts') != 0 or probe.get('outcome') != 'closed-during-open':
    raise SystemExit('unexpected acquisition or inference route')
frame = run / 'gui_preflight.png'
dimensions = namespace_phase.validate_png(frame)
namespace_phase.write(run / 'preflight-checked.json', {'schema': 'turnstone.m5-preflight/v1',
    'app_pid': runtime['pid'], 'custody_pid': processes['custody_ready']['pid'],
    'custody_principal': runtime['custody_principal'], 'route_probe': 'closed-during-open',
    'frame': 'gui_preflight.png', 'frame_dimensions': dimensions,
    'snapshot': {'offline_acquisition_attempts': 0, 'corpus_publications': 0, 'generation_requests': 0}})
