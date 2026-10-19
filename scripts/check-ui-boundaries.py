#!/usr/bin/env python3
"""Static ownership/pin guard; runs without the optional TekesUI snapshot."""
import hashlib
import json
import argparse
from pathlib import Path
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
root = parser.parse_args().root.resolve(strict=True)
ui = root / 'ui'
workspace = tomllib.loads((ui / 'Cargo.toml').read_text())['workspace']
lock = json.loads((ui / 'tekesui-source.json').read_text())
assert 'ui' in tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['exclude']
assert workspace['package']['publish'] is False
assert workspace['package']['version'] == '0.0.0'
for name, dependency in workspace['dependencies'].items():
    if name.startswith('tekes-ui-'):
        if lock['kind'] == 'published-git-revision':
            assert dependency == {'git': lock['repository'], 'rev': lock['rev']}
        else:
            assert dependency['path'].startswith('.deps/tekesui/crates/')
components = tomllib.loads((ui / 'components/Cargo.toml').read_text())['dependencies']
assert set(components) == {'tekes-agent-contract', 'tekes-agent-presentation', 'tekes-ui-controls', 'tekes-ui-scene', 'tekes-ui-tokens'}
bindings = tomllib.loads((ui / 'bindings/Cargo.toml').read_text())['dependencies']
# uuid allocates immutable session/create and attachment-operation identities;
# bindings still cannot import a renderer, platform backend or harness adapter.
assert set(bindings) == {'tekes-agent-components-ui', 'tekes-agent-contract', 'tekes-agent-presentation', 'tekes-agent-session', 'tekes-agent-web', 'serde_json', 'tokio', 'async-trait', 'uuid', 'base64'}
consumer = tomllib.loads((ui / 'consumer/Cargo.toml').read_text())['dependencies']
assert set(consumer) == {'tekes-agent-components-ui', 'tekes-agent-ui-bindings'}
assert set(tomllib.loads((ui / 'consumer/Cargo.toml').read_text()).get('dev-dependencies', {})) == {'tekes-ui-controls'}
# Direct dependencies must go through the reviewed workspace or crates.io; no host sibling fallback.
for path in ui.glob('*/Cargo.toml'):
    manifest = tomllib.loads(path.read_text())
    assert manifest['package']['publish']['workspace']
    targets = manifest.get('target', {})
    if path.parent.name == 'demo':
        assert targets == {'cfg(target_os = "macos")': {'dependencies': {
            'rfd': {'version': '=0.17.2', 'default-features': False}}}}
    else:
        assert not targets, 'system adapters must remain in the demo host'
    for dependency in [*manifest.get('dependencies', {}).values(), *manifest.get('dev-dependencies', {}).values()]:
        if isinstance(dependency, dict):
            assert not {'path', 'git'}.intersection(dependency), path
assert lock['kind'] in {'development-snapshot-not-release', 'published-git-revision'}
if lock['kind'] == 'published-git-revision':
    import re
    assert re.fullmatch('[0-9a-f]{40}', lock['rev'])
    assert lock['baseCommit'] == lock['rev']
    assert lock['repository'] == 'https://github.com/TekesApps/TekesUI.git'
assert len(lock['baseCommit']) == 40
for name, digest in lock['files'].items():
    relative = Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts and str(relative) == name
    assert len(digest) == 64
assert hashlib.sha256(json.dumps(lock['files'], sort_keys=True, separators=(',', ':')).encode()).hexdigest() == lock['sha256']
for directory in ('components', 'bindings', 'consumer'):
    for source in (ui / directory / 'src').rglob('*.rs'):
        text = source.read_text()
        assert all(name not in text for name in ('objc2', 'tekes_kernel', 'tekes_ui_gallery')), source
assert not list(ui.glob('**/*.swift'))
print('UI_BOUNDARIES_OK static-only; optional snapshot and native acceptance remain separate')
