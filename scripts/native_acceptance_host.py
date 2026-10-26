"""Fixed native acceptance identity. Inspection never launches or captures.

Installation is explicit and signs a private copy, never a Cargo hard link.
Historical applications are evidence, not launch candidates.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HOST_ROOT = ROOT / '.runtime-fixtures/native-host'
BUNDLE = HOST_ROOT / 'AgentKitAcceptance.app'
BUNDLE_ID = 'org.tekesapps.agentkit.acceptance'
EXECUTABLE = 'tekes-agent-ui-demo'
# A currently valid local Apple Development certificate; never fall back to '-'.
SIGNING_IDENTITY = 'F091EF63D82913C1B9E9371BDF6533F4B727FF20'
TEAM_ID = '9HFX228644'
RECEIPT = HOST_ROOT / 'identity.json'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inspect_signature(bundle):
    subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(bundle)],
                   check=True, capture_output=True)
    result = subprocess.run(['/usr/bin/codesign', '-dvv', '-r-', str(bundle)],
                            check=True, capture_output=True, text=True)
    lines = result.stderr.splitlines()
    if ('Signature=adhoc' in lines or f'Identifier={BUNDLE_ID}' not in lines
            or f'TeamIdentifier={TEAM_ID}' not in lines
            or 'Info.plist=not bound' in lines):
        raise ValueError('native host needs the fixed certificate-backed bundle identity')
    requirement = next((s for s in lines if s.startswith('designated => ')), None)
    if not requirement:
        raise ValueError('native host has no stable designated requirement')
    return requirement


def validate_fixed_host(program, bundle):
    if Path(bundle).resolve() != BUNDLE.resolve() or Path(program).resolve() != BUNDLE.resolve() / 'Contents/MacOS' / EXECUTABLE:
        raise ValueError('native acceptance must use the fixed signed host; historical binaries are archive-only')
    info = plistlib.loads((BUNDLE / 'Contents/Info.plist').read_bytes())
    if info.get('CFBundleIdentifier') != BUNDLE_ID or info.get('CFBundleExecutable') != EXECUTABLE:
        raise ValueError('fixed native bundle identity mismatch')
    receipt = json.loads(RECEIPT.read_text())
    if (receipt.get('signingIdentity') != SIGNING_IDENTITY
            or receipt.get('binarySha256') != digest(program)
            or receipt.get('designatedRequirement') != inspect_signature(BUNDLE)):
        raise ValueError('fixed native host differs from its signed installation receipt')
    return receipt


@contextmanager
def host_lease(program, bundle):
    import fcntl
    # Existing lock only: launching cannot implicitly provision a new identity.
    if Path(bundle).resolve() != BUNDLE.resolve():
        raise ValueError('historical native apps are archive-only; use the fixed host')
    if not (HOST_ROOT / '.host-lock').is_file():
        raise RuntimeError('fixed signed host is not installed; no launch performed')
    with (HOST_ROOT / '.host-lock').open('rb') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        validate_fixed_host(program, bundle)
        yield


def install(program):
    program = Path(program).resolve(strict=True)
    if program == BUNDLE / 'Contents/MacOS' / EXECUTABLE:
        raise ValueError('install requires a separate verified build input')
    HOST_ROOT.mkdir(parents=True, exist_ok=True)
    # Serialize installation and launch checks. Never replace a running host.
    import fcntl
    with (HOST_ROOT / '.host-lock').open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        handles = subprocess.run(['/usr/sbin/lsof', '-nP', '-Fpn'], capture_output=True, text=True)
        if handles.returncode != 0:
            raise RuntimeError('cannot establish host file occupancy')
        if any(line.startswith('n' + str(BUNDLE) + '/') for line in handles.stdout.splitlines()):
            raise RuntimeError('fixed native host is in use; no replacement performed')
        previous = None
        if BUNDLE.exists():
            previous = validate_fixed_host(BUNDLE / 'Contents/MacOS' / EXECUTABLE, BUNDLE)
            if previous['inputSha256'] == digest(program):
                return previous
        with tempfile.TemporaryDirectory(prefix='.prepare-', dir=HOST_ROOT) as stage:
            candidate = Path(stage) / BUNDLE.name
            binary = candidate / 'Contents/MacOS' / EXECUTABLE
            binary.parent.mkdir(parents=True)
            input_sha = digest(program)
            shutil.copy2(program, binary)
            if digest(binary) != input_sha or digest(program) != input_sha:
                raise RuntimeError('build input changed while preparing host')
            (candidate / 'Contents/Info.plist').write_bytes(plistlib.dumps({
                'CFBundleIdentifier': BUNDLE_ID, 'CFBundleExecutable': EXECUTABLE,
                'CFBundleName': 'AgentKit Acceptance', 'CFBundlePackageType': 'APPL',
                'NSHighResolutionCapable': True,
            }))
            subprocess.run(['/usr/bin/codesign', '--force', '--sign', SIGNING_IDENTITY,
                            '--identifier', BUNDLE_ID, '--timestamp=none', str(candidate)],
                           check=True, timeout=30, stdin=subprocess.DEVNULL)
            requirement = inspect_signature(candidate)
            if previous and previous['designatedRequirement'] != requirement:
                raise ValueError('host update would change its designated requirement')
            # Old signed programs remain archived with their own receipt.
            if previous:
                archive_root = HOST_ROOT / 'archive'
                archive_root.mkdir(parents=True, exist_ok=True)
                # A rollback can return to identical bytes. Each retired install
                # still keeps its receipt without colliding with or replacing
                # an earlier archive. Archived bundles are never launched.
                archive = Path(tempfile.mkdtemp(
                    prefix=previous['binarySha256'] + '-', dir=archive_root))
                shutil.move(str(BUNDLE), archive / BUNDLE.name)
                shutil.copy2(RECEIPT, archive / RECEIPT.name)
            os.replace(candidate, BUNDLE)
            receipt = {'bundlePath': str(BUNDLE), 'bundleId': BUNDLE_ID,
                       'signingIdentity': SIGNING_IDENTITY, 'teamId': TEAM_ID,
                       'designatedRequirement': requirement, 'inputSha256': input_sha,
                       'binarySha256': digest(BUNDLE / 'Contents/MacOS' / EXECUTABLE),
                       'launched': False, 'screenCapturePerformed': False}
            temporary = HOST_ROOT / '.identity.next.json'
            temporary.write_text(json.dumps(receipt, indent=2) + '\n')
            os.replace(temporary, RECEIPT)
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', type=Path, help='explicitly install and sign a verified build; does not launch')
    args = parser.parse_args()
    result = install(args.install) if args.install else validate_fixed_host(BUNDLE / 'Contents/MacOS' / EXECUTABLE, BUNDLE)
    print(json.dumps(result, indent=2))
