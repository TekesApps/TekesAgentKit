#!/usr/bin/env python3
"""Verify the actual Cargo-resolved public UI sources against the immutable reviewed manifest."""
import argparse
import json
from pathlib import Path
import re
from acceptance_tools import local_packages, require, snapshot, verify_files


def check(root, metadata, reviewed):
    root = Path(root).resolve()
    require(Path(metadata['workspace_root']).resolve() == root / 'ui', 'UI metadata workspace mismatch')
    require(reviewed['kind'] == 'published-git-revision', 'CI needs a published immutable UI pin')
    require(reviewed['repository'] == 'https://github.com/TekesApps/TekesUI.git', 'unexpected UI repository')
    require(re.fullmatch('[0-9a-f]{40}', reviewed['rev']), 'UI revision must be a full commit')
    expected_source = f"git+{reviewed['repository']}?rev={reviewed['rev']}#{reviewed['rev']}"
    manifests = {name for name in reviewed['files'] if name.startswith('crates/') and name.endswith('/Cargo.toml')}
    expected_names = {Path(name).parts[1] for name in manifests}
    packages = [package for package in metadata['packages'] if package['name'].startswith('tekes-ui-')]
    require({package['name'] for package in packages} == expected_names, 'resolved UI package closure mismatch')
    roots = set()
    for package in packages:
        require(package.get('source') == expected_source, 'resolved UI package source mismatch')
        path = Path(package['manifest_path']).resolve(strict=True)
        require(path.name == 'Cargo.toml' and path.parent.name == package['name']
                and path.parent.parent.name == 'crates', 'unexpected UI source layout')
        roots.add(path.parents[2])
    require(len(roots) == 1, 'UI packages do not share one immutable source root')
    source = roots.pop()
    verify_files(source, reviewed['files'])
    paths = [path for name in expected_names for path in (source / 'crates' / name).rglob('*')]
    require(not any(path.is_symlink() for path in paths), 'unexpected symlink in consumed UI crates')
    actual = {path.relative_to(source).as_posix() for path in paths if path.is_file()}
    expected = {name for name in reviewed['files'] if name.startswith('crates/')}
    require(actual == expected, 'unexpected files in consumed UI crates require review')
    return {'commit': reviewed['rev'], 'sha256': reviewed['sha256'],
            'ui_packages': len(packages), 'reviewed_files': len(reviewed['files']),
            'local_packages': local_packages(metadata, root), 'native_interaction': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--metadata', type=Path, required=True)
    args = parser.parse_args()
    reviewed = snapshot(args.root / 'ui/tekesui-source.json')
    receipt = check(args.root, json.loads(args.metadata.read_text()), reviewed)
    print(json.dumps(receipt, sort_keys=True))


if __name__ == '__main__':
    main()
