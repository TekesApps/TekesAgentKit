#!/usr/bin/env python3
"""Install an exact DSH runtime in a new host-selected directory, leaving existing installs alone."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument('--version', default='0.2.1-rc.2', choices=['0.2.1-rc.2'])
parser.add_argument('--node-root', type=Path, required=True)
parser.add_argument('--destination', type=Path, required=True)
args = parser.parse_args()
node_root = args.node_root.resolve(strict=True)
node = node_root / 'bin/node'
npm = (node_root / 'bin/npm').resolve(strict=True)
assert node.is_file(), 'explicit Node runtime required'
root = args.destination.absolute()
root.mkdir(parents=True, exist_ok=False)
(root / 'node').symlink_to(node_root, target_is_directory=True)
(root / 'package.json').write_text(json.dumps({
    'name': 'agentkit-dsh-runtime', 'private': True,
    'dependencies': {'@deepseek-ai/dsh': args.version},
}, indent=2) + '\n')
metadata = json.load(urllib.request.urlopen(
    'https://registry.npmjs.org/@deepseek-ai%2fdsh/' + args.version, timeout=30))
env = dict(os.environ, PATH=str(node_root / 'bin') + os.pathsep + os.environ.get('PATH', ''))
subprocess.run([str(node), str(npm), 'install', '--no-audit', '--no-fund', '--save-exact'], cwd=root, env=env, check=True)
installed = json.loads((root / 'node_modules/@deepseek-ai/dsh/package.json').read_text())
assert installed['version'] == args.version
lock = (root / 'package-lock.json').read_bytes()
assert json.loads(lock)['packages']['node_modules/@deepseek-ai/dsh']['integrity'] == metadata['dist']['integrity']
evidence = {
    'package': '@deepseek-ai/dsh', 'version': args.version,
    'dist': metadata['dist'],
    'node': subprocess.check_output([str(node), '--version'], text=True).strip(),
    'nodeRoot': str(node_root),
    'packageLockSha256': hashlib.sha256(lock).hexdigest(),
}
(root / 'agentkit-runtime.json').write_text(json.dumps(evidence, indent=2) + '\n')
print('DSH_RUNTIME_INSTALLED ' + str(root))
