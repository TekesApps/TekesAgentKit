#!/usr/bin/env python3
"""Capture real Pi/DSH child running and failure from one public publication.

Existing programs only; owned loopback model fixtures, no desktop or private
harness history reads. The provider releases an error only after running proof.
"""
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

from dsh_owned_fixture import owned_dsh
from pi_owned_fixture import prepare_pi
from summary_source_fixture import validate_witness
from summary_transition_provider import HeldFailureGate, provider_with_failure_gates
from task_family_fixture import dsh_subagent_seed_commands

ROOT = Path(__file__).resolve().parents[1]
SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')


def write_json(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def commands_for(kind, workspace, marker):
    if kind == 'pi':
        commands = [
            {'op': 'create', 'cwd': str(workspace), 'saveAs': 'parent'},
            {'op': 'call', 'sessionId': 'parent', 'method': 'session.selectModel',
             'args': {'provider': 'endpoint-test', 'model': 'endpoint-test'}},
            {'op': 'open', 'sessionId': 'parent'},
            {'op': 'compose', 'sessionId': 'parent', 'text': 'ENDPOINT_PI_CHILD'},
            {'op': 'waitSettled', 'sessionId': 'parent'},
            {'op': 'presentation', 'sessionId': 'parent', 'saveAs': 'child',
             'savePointer': '/summary/subagents/0/sessionId'},
            {'op': 'open', 'sessionId': 'child'},
            {'op': 'waitSettled', 'sessionId': 'child', 'inputText': 'CHILD_PI_TOOL_PROOF'}]
        minimum_turn = 2
    elif kind == 'dsh':
        commands = dsh_subagent_seed_commands('parent', workspace, 'endpoint-test', 'endpoint-test')
        minimum_turn = 3
    else:
        raise ValueError('unsupported endpoint')
    return commands + [
        {'op': 'open', 'sessionId': 'parent'},
        {'op': 'presentationSource', 'sessionId': 'parent'},
        {'op': 'open', 'sessionId': 'child'},
        {'op': 'childPrompt', 'sessionId': 'parent', 'childSessionId': 'child',
         'content': [{'type': 'text', 'text': marker}]},
        {'op': 'waitSourceRunOpen', 'sessionId': 'child', 'inputText': marker},
        {'op': 'presentationSource', 'sessionId': 'parent'},
        {'op': 'waitSettled', 'sessionId': 'child', 'minimumTurn': minimum_turn},
        *([{'op': 'childRead', 'sessionId': 'parent', 'childSessionId': 'child'}] if kind == 'pi' else []),
        {'op': 'presentationSource', 'sessionId': 'parent'},
        {'op': 'childHistory', 'sessionId': 'parent', 'childSessionId': 'child'}]


def run_profile(kind, config, commands, program, workspace, output, env, gate):
    lines, rows, sources, receipts = [], [], [], []
    events = queue.Queue()
    process = None
    write_json(output / 'commands.json', commands)
    try:
        with (output / 'stderr.log').open('w') as stderr:
            process = subprocess.Popen([str(program)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=stderr, text=True, env=env, cwd=workspace)

            def reader():
                for line in process.stdout:
                    events.put(line)
                events.put(None)

            thread = threading.Thread(target=reader, daemon=True)
            thread.start()
            process.stdin.write(json.dumps(dict(config, commands=commands)))
            process.stdin.close()
            deadline = time.monotonic() + 150
            while True:
                try:
                    line = events.get(timeout=max(0.01, deadline - time.monotonic()))
                except queue.Empty:
                    raise RuntimeError(kind + ' source capture timed out') from None
                if line is None:
                    break
                lines.append(line)
                (output / 'stdout.log').write_text(''.join(lines), encoding='utf-8')
                row = json.loads(line)
                index = len(rows)
                if index == 0:
                    if row.get('kind') != kind or row.get('ready') is not True or row.get('version') != config['expectedVersion']:
                        raise ValueError(kind + ' actual handshake mismatch')
                elif index > len(commands) or row.get('op') != commands[index - 1]['op'] or 'result' not in row:
                    raise ValueError(kind + ' command/result mismatch')
                rows.append(row)
                if row.get('op') == 'childRead':
                    state = row['result']
                    if (type(state.get('pendingInputs')) is not int or state['pendingInputs'] != 0
                            or state.get('running') is not False or state.get('settled') is not True):
                        raise ValueError('original public childRead has not retired the input dispatch')
                if row.get('op') != 'presentationSource':
                    continue
                witness = row['result']
                sources.append(witness)
                write_json(output / ('witness-' + str(len(sources)) + '.json'), witness)
                if len(sources) in (2, 3):
                    phase = 'running' if len(sources) == 2 else 'failed'
                    receipt = validate_witness(witness, endpoint=kind, transition_phase=phase, authored_marker=gate.marker)
                    if len(sources) == 2:
                        if not gate.entered.wait(5):
                            raise ValueError('controlled child request did not enter provider')
                        baseline = sources[0]['snapshot']['summary']['subagents']
                        if not any(item.get('sessionId') == receipt['childSessionId'] and item.get('id') == receipt['summaryRow']['id'] for item in baseline):
                            raise ValueError('running pointer differs from original baseline')
                        # The provider remains held until this original source is
                        # validated; no fixed delay is used as proof of running.
                        write_json(output / 'running-receipt.json', receipt)
                        gate.release_after_capture()
                    else:
                        before = receipts[0]
                        if any(receipt[key] != before[key] for key in ('parentSessionId', 'childSessionId', 'sourceIdentity')):
                            raise ValueError('failure switched parent, child or original tool result')
                        if (witness['snapshot']['connectionEpoch'] != sources[1]['snapshot']['connectionEpoch']
                                or witness['snapshot']['revision'] <= sources[1]['snapshot']['revision']
                                or receipt['summaryRow']['id'] != before['summaryRow']['id']
                                or receipt['summaryRow']['title'] != before['summaryRow']['title']):
                            raise ValueError('transition changed ownership, title or source epoch')
                        write_json(output / 'failed-receipt.json', receipt)
                    receipts.append(receipt)
            exit_status = process.wait(timeout=10)
            thread.join(timeout=5)
            if exit_status != 0 or len(rows) != len(commands) + 1 or len(sources) != 3 or len(receipts) != 2:
                raise ValueError(kind + ' incomplete source exchange')
            state = gate.receipt()
            if (state['requestCount'] < 1 or not state['explicitlyReleased'] or state['aborted'] or state['timedOut']):
                raise ValueError('controlled failure was not explicitly released after capture')
            return {'endpoint': kind, 'version': config['expectedVersion'], 'commandCount': len(commands),
                    'processExit': exit_status, 'running': receipts[0], 'failed': receipts[1], 'gate': state}
    finally:
        gate.abort()
        if process is not None:
            if process.poll() is None:
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=10)
            write_json(output / 'owned-headless-exit.json', {'pid': process.pid, 'exitStatus': process.poll()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'pi-root', 'dsh-root', 'output'):
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--program-archive', type=Path, help='Existing exact-SHA archive reference; never executed or modified')
    args = parser.parse_args()
    output = args.output.absolute()
    if output.exists() or not output.resolve().is_relative_to(SSD.resolve(strict=True)):
        parser.error('output must be a new directory under the existing SSD project')
    program = args.headless_program.resolve(strict=True)
    if not program.is_file() or not os.access(program, os.X_OK):
        parser.error('headless program must already exist and be executable')
    output.mkdir(mode=0o700)
    workspace = output / 'workspace'; workspace.mkdir()
    (workspace / 'proof.txt').write_text('owned Summary transition proof\n')
    home = output / 'host-home'; home.mkdir(mode=0o700)
    tmp = output / 'tmp'; tmp.mkdir(mode=0o700)
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'TMPDIR': str(tmp), 'LANG': 'en_US.UTF-8'}
    digest = hashlib.sha256(program.read_bytes()).hexdigest()
    if args.program_archive is not None:
        archived = args.program_archive.resolve(strict=True)
        if not archived.is_file() or hashlib.sha256(archived.read_bytes()).hexdigest() != digest:
            raise ValueError('existing program archive does not match the runtime')
    else:
        archive = output / 'programs'; archive.mkdir()
        archived = archive / program.name
        shutil.copy2(program, archived)
        if hashlib.sha256(archived.read_bytes()).hexdigest() != digest:
            raise RuntimeError('program changed during archival')
    write_json(output / 'program.json', {'path': str(program), 'sha256': digest, 'archiveOnly': True,
                                       'archiveReference': str(archived)})
    paths = [Path(__file__).resolve(), ROOT / 'crates/headless/src/main.rs',
             ROOT / 'crates/presentation/src/thread_activity.rs', ROOT / 'crates/presentation/src/summary.rs',
             ROOT / 'scripts/summary_transition_provider.py', ROOT / 'scripts/summary_transition_fixture.py',
             ROOT / 'scripts/summary_source_fixture.py', ROOT / 'scripts/owned_endpoint_provider.py',
             ROOT / 'scripts/pi_owned_fixture.py', ROOT / 'scripts/dsh_owned_fixture.py', ROOT / 'scripts/task_family_fixture.py']
    write_json(output / 'source-hashes.json', {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths})
    gates = {kind: HeldFailureGate('AGENTKIT_SOURCE_' + kind.upper() + '_HELD_FAILURE') for kind in ('pi', 'dsh')}
    requests = []
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), provider_with_failure_gates(workspace=lambda: workspace, requests=requests, gates=gates))
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    provider_url = 'http://127.0.0.1:' + str(server.server_port) + '/v1'
    results, failures = {}, {}
    dsh_process = None
    try:
        for kind in ('pi', 'dsh'):
            case = output / kind; case.mkdir()
            commands = commands_for(kind, workspace, gates[kind].marker)
            try:
                if kind == 'pi':
                    config = prepare_pi(args.pi_root, case / 'data', case / 'home', provider_url, strict_environment=True)
                    results[kind] = run_profile(kind, dict(config, expectedVersion='1.3.0'), commands, program, workspace, case, env, gates[kind])
                else:
                    dsh_home = case / 'home'
                    dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home), DSH_AGENTS_HOME=str(dsh_home / 'agents'),
                                   DSH_TELEMETRY_DISABLED='1', AGENTKIT_SYNTHETIC_KEY='synthetic-key')
                    with owned_dsh(args.dsh_root, case / 'data', dsh_home, workspace, provider_url, dsh_env, query_sqlite=False) as dsh:
                        dsh_process = dsh.process
                        if dsh.version != '0.2.1-rc.2':
                            raise ValueError('unexpected DSH version')
                        results[kind] = run_profile(kind, dict(dsh.connector_config([]), expectedVersion=dsh.version), commands, program, workspace, case, env, gates[kind])
                    write_json(case / 'owned-dsh-exit.json', {'pid': dsh_process.pid, 'exitStatus': dsh_process.poll()})
            except (OSError, ValueError, RuntimeError, KeyError, StopIteration, subprocess.SubprocessError) as error:
                failures[kind] = type(error).__name__ + ': ' + str(error)
                write_json(case / 'failure.json', {'error': failures[kind]})
    finally:
        for gate in gates.values(): gate.abort()
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        write_json(output / 'provider-requests.json', {'paths': requests, 'requestBodiesPersisted': False})
        write_json(output / 'gates.json', {kind: gate.receipt() for kind, gate in gates.items()})
    if hashlib.sha256(program.read_bytes()).hexdigest() != digest:
        failures['program'] = 'runtime executable changed during capture'
    if failures or set(results) != {'pi', 'dsh'} or dsh_process is None or dsh_process.poll() != 0:
        write_json(output / 'incomplete.json', {'failures': failures, 'completedEndpoints': list(results), 'accepted': False})
        print('SUMMARY_TRANSITIONS_FAILED; inspect retained owned sources')
        return 1
    write_json(output / 'run-receipt.json', {'accepted': True, 'endpoints': results, 'provider': 'synthetic-loopback',
        'providerRequestCount': len(requests), 'ownedDshExitStatus': dsh_process.poll(),
        'programSha256': digest, 'desktopInteraction': False, 'paidModel': False,
        'privateHarnessHistoryRead': False, 'serverTransactionalFreshnessProven': False})
    print('SUMMARY_TRANSITIONS_ACCEPTED pi+dsh; running-source -> controlled-failure-source; desktop=false')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
