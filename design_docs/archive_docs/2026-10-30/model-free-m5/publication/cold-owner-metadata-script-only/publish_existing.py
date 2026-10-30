import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent
bundles = json.loads((root / 'bundles.json').read_text())
receipt = {}

def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()

try:
    for name in bundles:
        item = bundles[name]
        bundle = root / (name + '.bundle')
        data = bundle.read_bytes()
        assert len(data) == item['bytes']
        assert hashlib.sha256(data).hexdigest() == item['sha256']
        repo = Path('/home/markik/Code/repos') / name
        assert git(repo, 'remote', 'get-url', 'origin') == f'https://github.com/merely-made/{name}.git'
        before = {'head': git(repo, 'rev-parse', 'HEAD'), 'status': git(repo, 'status', '--porcelain=v1')}
        receipt[name] = {'before': before, 'bundle': item}
        git(repo, 'fetch', 'origin')
        assert git(repo, 'rev-parse', 'origin/main') in (item['base'], item['head']), 'upstream changed'
        git(repo, 'bundle', 'verify', str(bundle))
        ref = 'refs/codex/m5-publication-20261030-cold-owner-4c72'
        git(repo, 'fetch', str(bundle), 'HEAD:' + ref)
        assert git(repo, 'rev-parse', ref) == item['head']
        git(repo, 'merge-base', '--is-ancestor', 'origin/main', ref)
        git(repo, 'push', 'origin', ref + ':refs/heads/main')
        published = git(repo, 'ls-remote', '--exit-code', 'origin', 'refs/heads/main').split()[0]
        assert published == item['head']
        receipt[name]['published'] = published
        after = {'head': git(repo, 'rev-parse', 'HEAD'), 'status': git(repo, 'status', '--porcelain=v1')}
        receipt[name]['after'] = after
        assert before == after, 'checkout state changed during publication'
        print(name, published, 'published; checkout unchanged', flush=True)
except BaseException as exc:
    receipt['error'] = repr(exc)
    raise
finally:
    (root / 'publication.json').write_text(json.dumps(receipt, indent=2) + '\n')
