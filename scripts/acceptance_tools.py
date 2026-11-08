"""Shared, standard-library-only checks for explicit consumer inputs (Python 3.11+)."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def relative_path(name):
    require(isinstance(name, str) and bool(name) and '\\' not in name,
            f'invalid relative path: {name!r}')
    path = PurePosixPath(name)
    require(not path.is_absolute() and '..' not in path.parts and path.parts
            and path.as_posix() == name and ':' not in path.parts[0],
            f'unsafe relative path: {name!r}')
    return Path(*path.parts)


def read_hashes(path):
    hashes = json.loads(Path(path).read_text())
    require(isinstance(hashes, dict) and hashes, f'empty/non-object manifest: {path}')
    for name, digest in hashes.items():
        relative_path(name)
        require(isinstance(digest, str) and re.fullmatch('[0-9a-f]{64}', digest),
                f'invalid SHA256: {name}')
    return hashes


def snapshot(path):
    data = json.loads(Path(path).read_text())
    hashes = data.get('files')
    require(isinstance(hashes, dict) and hashes, 'missing snapshot files')
    for name, digest in hashes.items():
        relative_path(name)
        require(isinstance(digest, str) and re.fullmatch('[0-9a-f]{64}', digest),
                f'invalid snapshot SHA256: {name}')
    actual = hashlib.sha256(json.dumps(hashes, sort_keys=True,
                                      separators=(',', ':')).encode()).hexdigest()
    require(actual == data.get('sha256'), 'snapshot manifest digest mismatch')
    return data


def verify_files(root, hashes):
    root = Path(root).resolve()
    for name, digest in hashes.items():
        relative = relative_path(name)
        path = root / relative
        require(not any((root / Path(*relative.parts[:i])).is_symlink()
                        for i in range(1, len(relative.parts) + 1)),
                f'manifest path uses symlink: {name}')
        require(path.is_file() and path.resolve().is_relative_to(root),
                f'missing/outside manifest file: {name}')
        require(sha256(path) == digest, f'content hash mismatch: {name}')


def extract_source(archive, expected_sha, destination):
    require(re.fullmatch('[0-9a-f]{64}', expected_sha), 'invalid archive SHA256')
    require(sha256(archive) == expected_sha, 'archive SHA256 mismatch')
    destination = Path(destination)
    destination.mkdir(exist_ok=False)
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        seen = set()
        for member in members:
            path = relative_path(member.name.rstrip('/') if member.isdir() else member.name)
            require(path not in seen, f'duplicate archive entry: {path}')
            seen.add(path)
            require(member.isdir() or member.isfile(),
                    f'archive link/device is not allowed: {path}')
        # Validate the entire directory before writing any archive member.
        for member in members:
            target = destination / relative_path(member.name.rstrip('/') if member.isdir() else member.name)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as source, target.open('xb') as output:
                    shutil.copyfileobj(source, output)
                target.chmod(member.mode & 0o777)
    require(sha256(archive) == expected_sha, 'archive changed during extraction')


def verify_payload(handoff, expected_snapshot):
    handoff = Path(handoff)
    payload = read_hashes(handoff / 'payload-manifest.json')
    actual = {str(p.relative_to(handoff)) for p in handoff.rglob('*') if p.is_file()}
    require(actual == set(payload) | {'payload-manifest.json'},
            'unlisted/missing payload files')
    require(not any(p.is_symlink() for p in handoff.rglob('*')), 'payload symlink')
    verify_files(handoff, payload)
    public = snapshot(handoff / 'consumer/ui/tekesui-source.json')
    require(public['sha256'] == expected_snapshot, 'unexpected public snapshot')
    verify_files(handoff / 'consumer/ui/.deps/tekesui', public['files'])
    return {'payload_files': len(payload), 'public_files': len(public['files']),
            'snapshot': public['sha256']}


def local_packages(metadata, root):
    root = Path(root).resolve()
    local = [Path(p['manifest_path']).resolve() for p in metadata['packages']
             if p.get('source') is None]
    require(local and all(p.is_relative_to(root) for p in local),
            'Cargo local dependency escaped the explicit payload root')
    return len(local)


def source_files(root):
    """Use Git only for this exact checkout; archives never borrow parent Git state."""
    root = Path(root).resolve()
    git = subprocess.run(['git', 'rev-parse', '--show-toplevel'], cwd=root,
                         capture_output=True, text=True)
    if git.returncode == 0 and Path(git.stdout.strip()).resolve() == root:
        return subprocess.check_output(
            ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
            cwd=root).decode().split('\0'), 'git'
    files = []
    for directory, names, leaves in os.walk(root, followlinks=False):
        names[:] = [n for n in names if n not in {'.git', 'target', '.deps', 'node_modules'}]
        files.extend(str((Path(directory) / name).relative_to(root)) for name in leaves)
    return files, 'archive-walk'


def headless_dependencies(metadata, root):
    root = Path(root).resolve()
    names = {'tekes-agent-contract', 'tekes-agent-session', 'tekes-agent-endpoint-kernel',
             'tekes-agent-endpoint-pi', 'tekes-agent-endpoint-dsh',
             'tekes-agent-headless', 'tekes-agent-presentation'}
    for package in metadata['packages']:
        if package['name'] not in names:
            continue
        for dependency in package['dependencies']:
            if dependency['kind'] == 'dev':
                continue
            name = dependency['name']
            require(not any(part in name for part in ('tekes-ui', 'rusqlite', 'sqlite', 'sqlx'))
                    and name not in {'schema', 'endpoint', 'engine', 'store', 'tekes-agent-components'},
                    f'headless dependency violation: {package["name"]} -> {name}')
            require(package['name'] in {'tekes-agent-headless', 'tekes-agent-presentation'}
                    or name != 'tekes-agent-presentation', 'presentation below authority layer')
            require(not dependency.get('path') or Path(dependency['path']).resolve().is_relative_to(root),
                    'implicit sibling path dependency')
