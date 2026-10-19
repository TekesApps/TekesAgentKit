#!/usr/bin/env python3
"""Compare a full published GitHub commit against reviewed bytes. No dependency writes."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.request
from acceptance_tools import require, sha256, snapshot


def git_tree_diff(repository, commit, reviewed):
    """Read committed blobs only: dirty working files and mutable refs never enter a pin."""
    require(re.fullmatch('[0-9a-f]{40}', commit), 'a full 40-character commit SHA is required')
    tree = subprocess.check_output(['git', '-C', str(repository), 'ls-tree', '-r', '-z', commit])
    entries = {}
    for item in tree.split(b'\0'):
        if item:
            head, name = item.split(b'\t', 1)
            mode, kind, blob = head.decode().split()
            entries[name.decode()] = (mode, kind, blob)
    changed, missing, unsupported = [], [], []
    for name, expected in reviewed['files'].items():
        if name not in entries:
            missing.append(name)
            continue
        mode, kind, blob = entries[name]
        if kind != 'blob' or mode not in {'100644', '100755'}:
            unsupported.append(name)
            continue
        value = subprocess.check_output(['git', '-C', str(repository), 'cat-file', 'blob', blob])
        actual = hashlib.sha256(value).hexdigest()
        if actual != expected:
            changed.append({'path': name, 'reviewed': expected, 'published': actual})
    # New inputs inside a consumed crate require review too, even if old files match.
    prefixes = {'/'.join(name.split('/')[:2]) + '/' for name in reviewed['files']
                if name.startswith('crates/') and len(name.split('/')) > 2}
    added = sorted(name for name in entries if name not in reviewed['files']
                   and any(name.startswith(prefix) for prefix in prefixes))
    return {'changed': changed, 'missing': missing, 'unsupported': unsupported,
            'added_in_consumed_crates': added,
            'matches': not (changed or missing or unsupported or added)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True, help='Previously reviewed snapshot manifest')
    parser.add_argument('--commit', required=True, help='Full immutable GitHub commit, not branch/tag name')
    parser.add_argument('--repository', default='https://github.com/TekesApps/TekesUI.git')
    parser.add_argument('--output', type=Path, required=True, help='New JSON receipt; never overwritten')
    args = parser.parse_args()
    require(re.fullmatch('[0-9a-f]{40}', args.commit), 'use a full 40-character commit SHA')
    match = re.fullmatch(r'https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?', args.repository)
    require(match, 'repository must be an explicit HTTPS github.com owner/repo URL')
    owner, repo = match.groups()
    reviewed = snapshot(args.manifest)
    receipt = {'kind': 'reviewed-snapshot-public-pin-comparison', 'status': 'running',
               'repository': args.repository, 'commit': args.commit,
               'reviewed_snapshot': reviewed['sha256'], 'manifest_sha256': sha256(args.manifest),
               'checked_at': datetime.now(timezone.utc).isoformat(), 'promotion': False,
               'release_authorized': False}
    # Reserve the receipt before any network operation; failures are reviewable too.
    with args.output.open('x') as stream:
        json.dump(receipt, stream, indent=2)
    try:
        # No token/header is provided: a local ref or authenticated private fetch is
        # insufficient evidence that a consumer can obtain this public commit.
        request = urllib.request.Request(
            f'https://api.github.com/repos/{owner}/{repo}/commits/{args.commit}',
            headers={'Accept': 'application/vnd.github+json', 'User-Agent': 'TekesAgentKit-pin-check'})
        with urllib.request.urlopen(request, timeout=30) as response:
            public = json.load(response)
        require(public.get('sha') == args.commit, 'public commit identity mismatch')
        receipt['anonymous_public_commit_verified'] = True
        with tempfile.TemporaryDirectory(prefix='agentkit-pin-') as directory:
            subprocess.run(['git', 'init', '--bare', directory], check=True, capture_output=True)
            subprocess.run(['git', '-C', directory, 'fetch', '--no-tags', '--depth=1',
                            args.repository, args.commit], check=True, capture_output=True, timeout=120)
            receipt['diff'] = git_tree_diff(directory, args.commit, reviewed)
        receipt['status'] = 'matching-inputs' if receipt['diff']['matches'] else 'review-required'
    except (Exception, KeyboardInterrupt) as error:
        receipt.update(status='failed', error=f'{type(error).__name__}: {error}')
    args.output.write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'{receipt["status"]}: {args.output}')
    return 0 if receipt['status'] == 'matching-inputs' else 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
