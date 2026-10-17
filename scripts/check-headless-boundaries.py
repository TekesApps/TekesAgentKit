#!/usr/bin/env python3
"""Keep headless production dependencies independent of UI, server internals and SQLite."""
import json
from pathlib import Path
import subprocess
root=Path(__file__).resolve().parent.parent
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--no-deps','--format-version','1'],cwd=root))
for package in metadata['packages']:
    if package['name'] not in {'tekes-agent-contract','tekes-agent-session','tekes-agent-endpoint-kernel'}: continue
    for dependency in package['dependencies']:
        if dependency['kind']=='dev': continue
        name=dependency['name']
        assert not any(part in name for part in ('tekes-ui','rusqlite','sqlite','sqlx')), (package['name'],name)
        assert name not in {'schema','endpoint','engine','store','tekes-agent-components','tekes-agent-presentation'}, (package['name'],name)
        assert not dependency.get('path') or Path(dependency['path']).resolve().is_relative_to(root), 'implicit sibling path dependency'
print('HEADLESS_DEPENDENCY_BOUNDARIES_OK')
