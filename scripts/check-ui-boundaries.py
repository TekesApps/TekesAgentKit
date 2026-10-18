#!/usr/bin/env python3
"""Static ownership/pin guard; runs without the optional TekesUI snapshot."""
import hashlib
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[1]
ui = root / 'ui'
workspace = tomllib.loads((ui / 'Cargo.toml').read_text())['workspace']
assert 'ui' in tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['exclude']
assert workspace['package']['publish'] is False
assert workspace['package']['version'] == '0.0.0'
for name, dependency in workspace['dependencies'].items():
    if name.startswith('tekes-ui-'):
        assert dependency['path'].startswith('.deps/tekesui/crates/')
components = tomllib.loads((ui / 'components/Cargo.toml').read_text())['dependencies']
assert set(components) == {'tekes-agent-presentation', 'tekes-ui-controls', 'tekes-ui-scene', 'tekes-ui-tokens'}
consumer = tomllib.loads((ui / 'consumer/Cargo.toml').read_text())['dependencies']
assert set(consumer) == {'tekes-agent-components-ui'}
# Direct dependencies must go through the reviewed workspace or crates.io; no host sibling fallback.
for path in ui.glob('*/Cargo.toml'):
    manifest = tomllib.loads(path.read_text())
    assert manifest['package']['publish']['workspace']
    for dependency in manifest.get('dependencies', {}).values():
        if isinstance(dependency, dict):
            assert not {'path', 'git'}.intersection(dependency), path
lock = json.loads((ui / 'tekesui-source.json').read_text())
assert lock['kind'] == 'development-snapshot-not-release'
assert len(lock['baseCommit']) == 40
for name, digest in lock['files'].items():
    relative = Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts and str(relative) == name
    assert len(digest) == 64
assert hashlib.sha256(json.dumps(lock['files'], sort_keys=True, separators=(',', ':')).encode()).hexdigest() == lock['sha256']
for directory in ('components', 'consumer'):
    for source in (ui / directory / 'src').rglob('*.rs'):
        text = source.read_text()
        assert all(name not in text for name in ('objc2', 'tekes_kernel', 'tekes_ui_gallery')), source
assert not list(ui.glob('**/*.swift'))
print('UI_BOUNDARIES_OK static-only; optional snapshot and native acceptance remain separate')
