#!/usr/bin/env python3
"""Own Pi/DSH, then exercise original demo child panes without a native window."""
from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import threading
import time

from child_pane_source_fixture import PREFIX, validate_pane_frame, validate_pane_receipt
from dsh_owned_fixture import owned_dsh
from pi_owned_fixture import prepare_pi
from summary_transition_provider import HeldFailureGate, provider_with_failure_gates
from task_family_fixture import pi_subagent_seed_commands, dsh_subagent_seed_commands, parse_family_exchange

ROOT = Path(__file__).resolve().parents[1]
SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
DSH_AUTH = 'AGENTKIT_CHILD_PANE_OWNED_COOKIE'


def write_json(path, value):
    with path.open('x', encoding='utf-8') as out:
        json.dump(value, out, ensure_ascii=False, indent=2)
        out.write('\n')


def seed(kind, config, headless, workspace, case, env):
    builder = pi_subagent_seed_commands if kind == 'pi' else dsh_subagent_seed_commands
    commands = builder('parent', workspace, 'endpoint-test', 'endpoint-test')
    # The existing demo host accepts exactly two ordinary session slots. Obtain
    # its second slot through the public API; never reuse the child as a main Run.
    commands.append({'op': 'create', 'cwd': str(workspace), 'saveAs': 'host-alternate'})
    write_json(case / 'seed-commands.json', commands)
    result = subprocess.run([str(headless)], input=json.dumps(dict(config, commands=commands)),
                            text=True, capture_output=True, env=env, cwd=workspace, timeout=120)
    (case / 'seed.stdout').write_text(result.stdout, encoding='utf-8')
    (case / 'seed.stderr').write_text(result.stderr, encoding='utf-8')
    write_json(case / 'seed-exit.json', {'exitStatus': result.returncode})
    if result.returncode != 0:
        raise ValueError('owned headless seed failed')
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if not rows or rows[0].get('version') != config['expectedVersion']:
        raise ValueError('actual harness version differs from the fixed seed runtime')
    parent = next(row['result']['sessionId'] for row in rows if row.get('op') == 'create')
    manifest = parse_family_exchange(kind, commands, rows, parent, workspace)
    manifest['hostAuxiliarySessionId'] = next(row['result']['sessionId'] for row in reversed(rows)
                                            if row.get('op') == 'create')
    write_json(case / 'seed-source.json', manifest)
    return manifest, len(commands)


def capture_demo(kind, demo, command, env, workspace, case, gate, parent, child):
    events = queue.Queue()
    process = None
    lines, proofs = [], []
    write_json(case / 'demo-command.json', command)
    try:
        with (case / 'demo.stderr').open('w') as stderr:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr,
                                       text=True, env=env, cwd=workspace)

            def read():
                for line in process.stdout: events.put(line)
                events.put(None)

            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            deadline = time.monotonic() + 150
            while True:
                try: line = events.get(timeout=max(0.01, deadline - time.monotonic()))
                except queue.Empty: raise RuntimeError('owned child pane source deadline') from None
                if line is None: break
                lines.append(line)
                (case / 'demo.stdout').write_text(''.join(lines), encoding='utf-8')
                if not line.startswith(PREFIX): continue
                if len(proofs) >= 2: raise ValueError('unexpected extra original pane source frame')
                frame = json.loads(line[len(PREFIX):])
                phase = 'running' if not proofs else 'failed'
                write_json(case / ('source-' + phase + '.json'), frame)
                proof = validate_pane_frame(frame, endpoint=kind, parent=parent, child_id=child,
                    marker=gate.marker, expected_phase=phase,
                    expected_epoch=proofs[0]['connectionEpoch'] if proofs else None)
                write_json(case / ('source-' + phase + '-receipt.json'), proof)
                proofs.append(proof)
                if phase == 'running':
                    if not gate.entered.wait(5):
                        raise ValueError('pane running source preceded any held provider request')
                    gate.release_after_capture()
            status = process.wait(timeout=10)
            reader.join(timeout=5)
            if status != 0 or len(proofs) != 2:
                raise ValueError('original child pane host did not complete both source publications')
            state = gate.receipt()
            if not state['explicitlyReleased'] or state['timedOut'] or state['aborted']:
                raise ValueError('pane failure gate was not released by verified running source')
            receipt = json.loads((case / 'host-receipt.json').read_text())
            accepted = validate_pane_receipt(receipt, *proofs)
            accepted.update(endpoint=kind, gate=state, demoExit=status)
            write_json(case / 'accepted.json', accepted)
            return accepted
    finally:
        gate.abort()
        if process is not None:
            if process.poll() is None:
                try: process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=10)
            write_json(case / 'owned-demo-exit.json', {'pid': process.pid, 'exitStatus': process.poll()})


def profile(kind, config, headless, demo, workspace, case, env, gate, cookie=None):
    manifest, command_count = seed(kind, config, headless, workspace, case, env)
    parent = manifest['parent']['scope']
    child = manifest['child']['address']['sessionId']
    write_json(case / 'pane-manifest.json', {'parent': parent, 'childId': child, 'marker': gate.marker})
    endpoint = {key: value for key, value in config.items()
                if key not in ('cookie', 'bearer', 'expectedVersion', 'commands')}
    endpoint.update(endpointId=kind, sessions=[parent['sessionId'], manifest['hostAuxiliarySessionId']])
    demo_env = dict(env)
    if kind == 'dsh':
        endpoint['cookieEnv'] = DSH_AUTH
        demo_env[DSH_AUTH] = cookie
    write_json(case / 'endpoints.json', {'endpoints': [endpoint]})
    command = [str(demo), '--components', '--transcript-content', '--endpoint-config', str(case / 'endpoints.json'),
               '--child-pane-source-proof', str(case / 'pane-manifest.json'),
               '--child-pane-source-receipt', str(case / 'host-receipt.json'),
               '--offscreen', str(case / 'pane.png')]
    accepted = capture_demo(kind, demo, command, demo_env, workspace, case, gate, parent, child)
    accepted.update(seedCommandCount=command_count, harnessVersion=config['expectedVersion'])
    return accepted


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'ui-demo', 'pi-root', 'dsh-root', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.absolute()
    if output.exists() or not output.resolve().is_relative_to(SSD.resolve(strict=True)):
        parser.error('output must be a new directory under the existing SSD project')
    programs = {key: path.resolve(strict=True) for key, path in
                (('headless', args.headless_program), ('demo', args.ui_demo))}
    if any(not p.is_file() or not os.access(p, os.X_OK) for p in programs.values()):
        parser.error('explicit existing executable programs are required')
    output.mkdir(mode=0o700)
    archive = output / 'programs'; archive.mkdir()
    identities = {}
    for key, path in programs.items():
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        saved = archive / path.name; shutil.copy2(path, saved)
        if hashlib.sha256(saved.read_bytes()).hexdigest() != digest:
            raise RuntimeError('program changed during archival')
        identities[key] = {'path': str(path), 'sha256': digest, 'archiveOnly': str(saved)}
    write_json(output / 'programs.json', identities)
    source_paths = [Path(__file__), ROOT / 'scripts/child_pane_source_fixture.py',
                    ROOT / 'scripts/transcript_source_fixture.py', ROOT / 'scripts/summary_transition_fixture.py',
                    ROOT / 'scripts/summary_transition_provider.py', ROOT / 'scripts/owned_endpoint_provider.py',
                    ROOT / 'scripts/task_family_fixture.py', ROOT / 'scripts/pi_owned_fixture.py',
                    ROOT / 'scripts/dsh_owned_fixture.py']
    write_json(output / 'source-hashes.json', {str(p.resolve().relative_to(ROOT)):
        hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths})
    workspace = output / 'workspace'; workspace.mkdir()
    (workspace / 'proof.txt').write_text('owned independent child pane source\n')
    host_home = output / 'host-home'; host_home.mkdir(mode=0o700)
    tmp = output / 'tmp'; tmp.mkdir()
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(host_home), 'TMPDIR': str(tmp), 'LANG': 'en_US.UTF-8'}
    gates = {kind: HeldFailureGate('AGENTKIT_PANE_' + kind.upper() + '_HELD_FAILURE') for kind in ('pi', 'dsh')}
    requests = []
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
        provider_with_failure_gates(workspace=lambda: workspace, requests=requests, gates=gates))
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    provider_url = 'http://127.0.0.1:' + str(server.server_port) + '/v1'
    results, failures = {}, {}
    dsh_process = None
    try:
        for kind in ('pi', 'dsh'):
            case = output / kind; case.mkdir()
            try:
                if kind == 'pi':
                    config = prepare_pi(args.pi_root, case / 'data', case / 'home', provider_url, strict_environment=True)
                    results[kind] = profile(kind, dict(config, expectedVersion='1.3.0'), programs['headless'],
                        programs['demo'], workspace, case, env, gates[kind])
                else:
                    home = case / 'home'
                    dsh_env = dict(env, HOME=str(home), DSH_HOME=str(home), DSH_AGENTS_HOME=str(home / 'agents'),
                                   DSH_TELEMETRY_DISABLED='1', AGENTKIT_SYNTHETIC_KEY='synthetic-key')
                    with owned_dsh(args.dsh_root, case / 'data', home, workspace, provider_url, dsh_env,
                                   query_sqlite=False) as dsh:
                        dsh_process = dsh.process
                        if dsh.version != '0.2.1-rc.2': raise ValueError('unexpected DSH version')
                        results[kind] = profile(kind, dict(dsh.connector_config([]), expectedVersion=dsh.version),
                            programs['headless'], programs['demo'], workspace, case, env, gates[kind], cookie=dsh.cookie)
                    write_json(case / 'owned-dsh-exit.json', {'pid': dsh_process.pid, 'exitStatus': dsh_process.poll()})
            except (OSError, ValueError, RuntimeError, KeyError, StopIteration, subprocess.SubprocessError) as error:
                failures[kind] = type(error).__name__ + ': ' + str(error)
                write_json(case / 'failure.json', {'error': failures[kind]})
            finally:
                exit_receipt = case / 'owned-dsh-exit.json'
                if kind == 'dsh' and dsh_process is not None and not exit_receipt.exists():
                    write_json(exit_receipt, {'pid': dsh_process.pid, 'exitStatus': dsh_process.poll()})
    finally:
        for gate in gates.values(): gate.abort()
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        write_json(output / 'gates.json', {kind: gate.receipt() for kind, gate in gates.items()})
        write_json(output / 'provider-requests.json', {'paths': requests, 'requestBodiesPersisted': False})
    for key, path in programs.items():
        if hashlib.sha256(path.read_bytes()).hexdigest() != identities[key]['sha256']:
            failures[key] = 'runtime program changed during source capture'
    if failures or set(results) != {'pi', 'dsh'} or dsh_process is None or dsh_process.poll() != 0:
        write_json(output / 'incomplete.json', {'accepted': False, 'failures': failures, 'completed': list(results)})
        print('LIVE_CHILD_PANE_SOURCE_FAILED; inspect retained owned evidence')
        return 1
    write_json(output / 'run-receipt.json', {'accepted': True, 'endpoints': results,
        'programs': identities, 'providerRequestCount': len(requests), 'provider': 'synthetic-loopback',
        'nativeInteraction': False, 'physicalIme': False, 'privateHarnessHistoryRead': False,
        'paidModel': False, 'ownedDshExitStatus': dsh_process.poll()})
    print('LIVE_CHILD_PANE_SOURCE_ACCEPTED pi+dsh; original child panes; native_window=false')
    return 0


if __name__ == '__main__': raise SystemExit(main())
