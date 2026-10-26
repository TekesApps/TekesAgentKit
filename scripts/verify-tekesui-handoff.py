#!/usr/bin/env python3
"""Replay an explicitly hashed handoff outside the checkout; never promote a snapshot."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import tomllib
from acceptance_tools import extract_source, local_packages, require, sha256, verify_payload

TOOLS = Path(__file__).resolve().parent


class Replay:
    def __init__(self, output, env, timeout):
        self.output, self.env, self.timeout = output, env, timeout
        self.receipt = {'kind': 'development-handoff-replay', 'status': 'running',
                        'started_at': datetime.now(timezone.utc).isoformat(),
                        'steps': [], 'native_interaction': False, 'promotion': False}
        self.save()

    def save(self):
        temporary = self.output / 'receipt.tmp'
        temporary.write_text(json.dumps(self.receipt, indent=2) + '\n')
        temporary.replace(self.output / 'receipt.json')

    def step(self, name, command, cwd, metadata=False):
        log = self.output / f'{name}.log'
        row = {'name': name, 'command': list(map(str, command)), 'cwd': str(cwd),
               'log': str(log), 'status': 'running'}
        self.receipt['steps'].append(row)
        self.save()
        started = time.monotonic()
        print(f'RUN {name}', flush=True)
        with log.open('wb') as stream:
            try:
                process = subprocess.Popen(row['command'], cwd=cwd, env=self.env,
                                           stdout=stream, stderr=subprocess.PIPE if metadata else subprocess.STDOUT,
                                           start_new_session=os.name == 'posix')
            except OSError as error:
                row.update(status='failed', error=str(error))
                self.save()
                raise
            try:
                _, error = process.communicate(timeout=self.timeout)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                if os.name == 'posix':
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
                process.communicate()
                row.update(status='interrupted-or-timeout', seconds=time.monotonic() - started)
                self.save()
                raise
        row.update(exit_code=process.returncode, seconds=time.monotonic() - started,
                   status='passed' if process.returncode == 0 else 'failed', sha256=sha256(log))
        if metadata:
            (self.output / f'{name}.stderr.log').write_bytes(error or b'')
        counts = re.findall(rb'test result: ok\. (\d+) passed', log.read_bytes())
        if counts:
            row['tests_passed'] = sum(map(int, counts))
        self.save()
        require(process.returncode == 0, f'{name} failed; see {log}')
        return log


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--snapshot-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True, help='New directory outside this checkout')
    parser.add_argument('--target-dir', type=Path, required=True, help='Reusable target cache outside the payload')
    parser.add_argument('--offscreen', action='store_true', help='macOS GPU only; never opens a window')
    parser.add_argument('--timeout-seconds', type=int, default=1200, help='Per-command timeout')
    args = parser.parse_args()
    require(args.timeout_seconds > 0, 'timeout must be positive')
    archive = args.archive.resolve(strict=True)
    output, target = args.output.resolve(), args.target_dir.resolve()
    require(not output.is_relative_to(TOOLS.parent), 'output must be outside this checkout')
    require(not target.is_relative_to(output), 'target must be outside the replay directory')
    output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_TERM_COLOR='never')
    env.update(PYTHONDONTWRITEBYTECODE='1', PYTHONOPTIMIZE='0')
    # This runner builds the host-native demo. Never run a cached host binary
    # after Cargo silently built a different target inherited from the caller.
    env.pop('CARGO_BUILD_TARGET', None)
    # Audit/native fixture environment from another task must not alter this replay.
    for key in list(env):
        if key.startswith(('TEKESUI_', 'TEKES_SELFTEST')):
            del env[key]
    replay = Replay(output, env, args.timeout_seconds)
    replay.receipt.update(archive=str(archive), archive_sha256=args.sha256,
                          expected_snapshot=args.snapshot_sha256, target_dir=str(target),
                          tools={p.name: sha256(p) for p in [Path(__file__), TOOLS / 'acceptance_tools.py',
                                 TOOLS / 'check-headless-boundaries.py', TOOLS / 'check-ui-boundaries.py']})
    try:
        extract_source(archive, args.sha256, output / 'source')
        handoff = output / 'source/handoff'
        replay.receipt['inputs'] = verify_payload(handoff, args.snapshot_sha256)
        toolchain = tomllib.loads((handoff / 'rust-toolchain.toml').read_text())['toolchain']['channel']
        require(re.fullmatch(r'\d+\.\d+\.\d+', toolchain), 'handoff must pin an exact Rust version')
        cargo = ['cargo', f'+{toolchain}']
        consumer = handoff / 'consumer'
        manifests = {'ui': consumer / 'ui/Cargo.toml',
                     'public-consumer': consumer / 'ui/.deps/tekesui/examples/composition-consumer/Cargo.toml',
                     'headless': consumer / 'Cargo.toml'}
        replay.receipt['local_packages'] = {}
        for name, manifest in manifests.items():
            path = replay.step(f'metadata-{name}', cargo + ['metadata', '--manifest-path', manifest,
                              '--locked', '--format-version', '1'], handoff, metadata=True)
            replay.receipt['local_packages'][name] = local_packages(json.loads(path.read_text()), handoff)
        replay.step('headless-boundary', [sys.executable, TOOLS / 'check-headless-boundaries.py',
                    '--root', consumer, '--metadata', output / 'metadata-headless.log'], handoff)
        replay.step('ui-boundary', [sys.executable, TOOLS / 'check-ui-boundaries.py', '--root', consumer], handoff)
        for name in ('headless', 'ui'):
            replay.step(f'fmt-{name}', cargo + ['fmt', '--manifest-path', manifests[name], '--all', '--check'], handoff)
        for name, manifest in manifests.items():
            selection = ['--all-targets'] if name == 'public-consumer' else ['--workspace']
            replay.step(f'tests-{name}', cargo + ['test', '--manifest-path', manifest, *selection,
                        '--locked', '--', '--nocapture'], handoff)
            replay.step(f'clippy-{name}', cargo + ['clippy', '--manifest-path', manifest,
                        *selection, *([] if name == 'public-consumer' else ['--all-targets']),
                        '--locked', '--', '-D', 'warnings'], handoff)
        replay.step('demo-build', cargo + ['build', '--manifest-path', manifests['ui'], '-p',
                    'tekes-agent-ui-demo', '--locked'], handoff)
        if args.offscreen:
            require(sys.platform == 'darwin', '--offscreen requires macOS/Metal')
            picture = output / 'offscreen.png'
            replay.step('gpu-offscreen', [target / 'debug/tekes-agent-ui-demo', '--offscreen', picture], handoff)
            require(picture.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'), 'GPU output is not PNG')
            replay.receipt['offscreen_png_sha256'] = sha256(picture)
        replay.receipt['inputs_after'] = verify_payload(handoff, args.snapshot_sha256)
        replay.receipt['status'] = 'passed'
    except (Exception, KeyboardInterrupt) as error:
        replay.receipt.update(status='failed', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        replay.receipt['finished_at'] = datetime.now(timezone.utc).isoformat()
        replay.save()
    print(f'REPLAY_PASSED {output / "receipt.json"}')


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
