#!/usr/bin/env python3
"""Replay an explicit portable TekesUI candidate in an isolated AgentKit snapshot.

Never edits the default manifests/lockfiles or the candidate source. This proves
receiver integration only, not native input, visual acceptance or a release pin.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(source, manifest):
    for name, expected in manifest['files'].items():
        path = source / name
        if not path.resolve().is_relative_to(source) or any(
            parent.is_symlink() for parent in [path, *path.parents] if parent != source.parent
        ):
            raise ValueError(f'unsafe candidate input: {name}')
        if not path.is_file() or sha(path) != expected:
            raise ValueError(f'candidate hash mismatch: {name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--workdir', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    source = args.source.resolve()
    manifest = json.loads(args.manifest.read_text())
    verify(source, manifest)
    work = args.workdir.resolve()
    work.mkdir(parents=True, exist_ok=False)
    snapshot = work / 'agentkit'
    snapshot.mkdir()
    names = subprocess.check_output(
        ['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=repo
    ).decode().split('\0')
    inputs = {}
    for name in sorted(set(names)):
        if not name or not (name in ('Cargo.toml', 'Cargo.lock', 'AGENTS.md', 'LICENSE')
                           or name.startswith(('crates/', 'ui/'))):
            continue
        original = repo / name
        if original.is_symlink() or not original.is_file():
            raise ValueError(f'unsafe AgentKit snapshot input: {name}')
        if 'target' in Path(name).parts or '.deps' in Path(name).parts:
            continue
        dest = snapshot / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, dest)
        inputs[name] = sha(original)
    probe = repo / 'scripts/probes/agentkit-reading-candidate.rs'
    shutil.copyfile(probe, snapshot / 'ui/demo/tests/reading_candidate.rs')
    cargo = snapshot / 'ui/Cargo.toml'
    cargo.write_text(re.sub(
        r'^(tekes-ui-[\w-]+) = \{ git = "https://github.com/TekesApps/TekesUI.git", rev = "[a-f0-9]+" \}$',
        lambda m: f'{m[1]} = {{ path = {json.dumps(str(source / "crates" / m[1]))} }}',
        cargo.read_text(), flags=re.M))
    lock = snapshot / 'ui/Cargo.lock'
    lock.write_text(re.sub(r'^source = "git\+https://github.com/TekesApps/TekesUI.git[^\n]+\n',
                           '', lock.read_text(), flags=re.M))
    # Candidate-only additive API adaptation. Keep the default public pin source
    # unchanged and retain the exact transformed host for receiver review.
    native = snapshot / 'ui/demo/src/native.rs'
    old = '                invalid: n.semantics.invalid,'
    assert native.read_text().count(old) == 1
    native.write_text(native.read_text().replace(old, old + '\n                required: n.semantics.required,'))
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()),
               CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0',
               CARGO_PROFILE_DEV_INCREMENTAL='false', CARGO_PROFILE_TEST_INCREMENTAL='false')
    base = ['cargo', '+1.95.0']
    common = ['--manifest-path', str(cargo), '--locked', '--offline']
    receipt = dict(candidate_identity=manifest['identity'], candidate_files=len(manifest['files']),
                   portable_manifest_sha256=sha(args.manifest), agentkit_inputs=inputs,
                   probe_sha256=sha(probe), source=str(source), snapshot=str(snapshot),
                   candidate_only_host_adaptations=['AX required forwarded from public semantics'],
                   candidate_overrides={name: sha(snapshot / name) for name in
                                        ('ui/Cargo.toml', 'ui/Cargo.lock', 'ui/demo/src/native.rs')},
                   default_pin_promoted=False, native_acceptance=False, visual_acceptance=False,
                   full_runview_acceptance=False, checks=[])
    try:
        metadata = subprocess.check_output(base + ['metadata', *common, '--format-version', '1'], env=env)
        (work / 'metadata.json').write_bytes(metadata)
        packages = [p for p in json.loads(metadata)['packages'] if p['name'].startswith('tekes-ui-')]
        assert len(packages) == 8, [p['name'] for p in packages]
        assert all(Path(p['manifest_path']).resolve().is_relative_to(source) for p in packages)
        for label, cmd in [
            ('tests', ['test', *common, '-p', 'tekes-agent-ui-demo', '--test', 'reading_candidate']),
            ('clippy', ['clippy', *common, '-p', 'tekes-agent-ui-demo', '--test', 'reading_candidate', '--', '-D', 'warnings']),
        ]:
            with (work / f'{label}.log').open('w') as log:
                result = subprocess.run(base + cmd, env=env, stdout=log, stderr=subprocess.STDOUT)
            receipt['checks'].append(dict(check=label, exit_code=result.returncode))
            if result.returncode:
                raise RuntimeError(f'{label} failed; see {work / (label + ".log")}')
    finally:
        verify(source, manifest)
        assert all(sha(repo / name) == digest for name, digest in inputs.items()), 'AgentKit input changed during replay'
        (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(dict(workdir=str(work), checks=receipt['checks'])))


if __name__ == '__main__':
    main()
