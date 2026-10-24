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
from transcript_source_fixture import validate_transcript_pair, validate_transcript_source
from task_family_fixture import dsh_subagent_seed_commands

ROOT = Path(__file__).resolve().parents[1]
SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
FRESH_CHILD_POINTER = '/snapshot/summary/subagents/0/sessionId'


def write_json(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def commands_for(kind, workspace, marker, child_transcript_source=False, child_transcript_reconnect=False):
    if child_transcript_reconnect and not child_transcript_source:
        raise ValueError('--child-transcript-reconnect requires --child-transcript-source')
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
    commands += [
        {'op': 'open', 'sessionId': 'parent'},
        {'op': 'presentationSource', 'sessionId': 'parent'},
        {'op': 'open', 'sessionId': 'child'},
        {'op': 'childPrompt', 'sessionId': 'parent', 'childSessionId': 'child',
         'content': [{'type': 'text', 'text': marker}]},
        {'op': 'waitSourceRunOpen', 'sessionId': 'child', 'inputText': marker},
        {'op': 'presentationSource', 'sessionId': 'parent'},
    ]
    if child_transcript_source:
        commands.append({'op': 'presentationSource', 'sessionId': 'child'})
    commands += [
        {'op': 'waitSettled', 'sessionId': 'child', 'minimumTurn': minimum_turn},
        *([{'op': 'childRead', 'sessionId': 'parent', 'childSessionId': 'child'}] if kind == 'pi' else []),
        {'op': 'presentationSource', 'sessionId': 'parent'},
    ]
    if child_transcript_source:
        commands.append({'op': 'presentationSource', 'sessionId': 'child'})
    commands.append({'op': 'childHistory', 'sessionId': 'parent', 'childSessionId': 'child'})
    if child_transcript_reconnect:
        commands += [
            {'op': 'reconnect'},
            {'op': 'open', 'sessionId': 'parent'},
            {'op': 'presentationSource', 'sessionId': 'parent', 'saveAs': 'freshChild',
             'savePointer': FRESH_CHILD_POINTER},
            {'op': 'open', 'sessionId': 'freshChild'},
            {'op': 'presentationSource', 'sessionId': 'parent'},
            {'op': 'presentationSource', 'sessionId': 'freshChild'},
            {'op': 'childHistory', 'sessionId': 'parent', 'childSessionId': 'freshChild'},
        ]
    return commands


def parse_arguments(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'pi-root', 'dsh-root', 'output'):
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--program-archive', type=Path, help='Existing exact-SHA archive reference; never executed or modified')
    parser.add_argument('--child-transcript-source', action='store_true',
                        help='Also capture and validate distinct child-scoped Transcript running/error publications')
    parser.add_argument('--child-transcript-reconnect', action='store_true',
                        help='Reopen parent and primary child Runs after reconnect and validate failed Transcript recovery')
    args = parser.parse_args(argv)
    if args.child_transcript_reconnect and not args.child_transcript_source:
        parser.error('--child-transcript-reconnect requires --child-transcript-source')
    return args


def json_pointer(value, pointer):
    current = value
    for part in pointer.strip('/').split('/'):
        part = part.replace('~1', '/').replace('~0', '~')
        current = current[int(part)] if isinstance(current, list) else current[part]
    return current


def run_profile(kind, config, commands, program, workspace, output, env, gate, child_transcript_source=False,
                child_transcript_reconnect=False):
    lines, rows, sources, receipts, child_sources, child_receipts = [], [], [], [], [], []
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
                if child_transcript_reconnect and row.get('op') == 'childHistory' and commands[index - 1].get('childSessionId') == 'freshChild':
                    from transcript_reconnect_fixture import validate_recovered_history
                    recovery = json.loads((output / 'transcript-reconnect-receipt.json').read_text(encoding='utf-8'))
                    history_receipt = validate_recovered_history(row['result'], child_sources[2], recovery['turnId'])
                    write_json(output / 'recovered-child-history-receipt.json', history_receipt)
                if row.get('op') != 'presentationSource':
                    continue
                witness = row['result']
                command = commands[index - 1]
                if child_transcript_source and command.get('sessionId') in ('child', 'freshChild'):
                    if child_transcript_reconnect and command.get('sessionId') == 'freshChild':
                        if len(child_sources) != 2 or len(sources) != 5:
                            raise ValueError('reconnected child Transcript arrived before new parent publication')
                        from transcript_reconnect_fixture import validate_child_transcript_reconnect
                        recovery = validate_child_transcript_reconnect(endpoint=kind, marker=gate.marker,
                            old_running=child_sources[0], old_failed=child_sources[1],
                            old_parent=sources[2], reconnected_parent=sources[4], fresh_child=witness,
                            fresh_pointer=json_pointer(sources[3], FRESH_CHILD_POINTER))
                        child_sources.append(witness)
                        write_json(output / 'transcript-reconnected.json', witness)
                        write_json(output / 'transcript-reconnect-receipt.json', recovery)
                        continue
                    phase = 'running' if not child_sources else 'failed'
                    if len(child_sources) >= 2 or len(receipts) != (1 if phase == 'running' else 2):
                        raise ValueError('child Transcript source arrived outside its parent publication pair')
                    parent_receipt = receipts[0] if phase == 'running' else receipts[1]
                    proof = validate_transcript_source(
                        witness, endpoint=kind, child_id=parent_receipt['childSessionId'],
                        phase=phase, marker=gate.marker,
                        expected_epoch=sources[1 if phase == 'running' else 2]['snapshot']['connectionEpoch'],
                        expected_scope={'endpointId': kind, 'threadId': parent_receipt['childSessionId'],
                                        'sessionId': parent_receipt['childSessionId']})
                    if proof['childSessionId'] != parent_receipt['childSessionId']:
                        raise ValueError('child Transcript proof does not use the parent publication child pointer')
                    child_sources.append(witness)
                    child_receipts.append(proof)
                    write_json(output / ('transcript-' + phase + '.json'), witness)
                    write_json(output / ('transcript-' + phase + '-receipt.json'), proof)
                    if phase == 'running':
                        gate.release_after_capture()
                    if phase == 'failed':
                        child_pair = validate_transcript_pair(child_receipts[0], proof)
                        write_json(output / 'transcript-pair-receipt.json', child_pair)
                        if (proof['connectionEpoch'] != child_receipts[0]['connectionEpoch']
                                or proof['authoredInput'] != child_receipts[0]['authoredInput']
                                or proof['turnId'] != child_receipts[0]['turnId']):
                            raise ValueError('child Transcript transition changed its public source identity')
                    continue
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
                        if not child_transcript_source:
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
                elif child_transcript_reconnect and len(sources) == 4:
                    # Discovery supplies the fresh alias before opening it. A
                    # retained child reader may still be restoring at this point;
                    # this frame is not accepted as failed-child source evidence.
                    previous = receipts[1]
                    scope = witness['snapshot']['summary']['scope']
                    if (witness.get('projectionSucceeded') is not True
                            or witness.get('evidenceScope') != 'original-publication-and-same-memory-input'
                            or witness['authority'].get('endpoint_id') != kind
                            or witness['authority'].get('connection') != 'Ready'
                            or scope != sources[2]['snapshot']['summary']['scope']
                            or witness['snapshot']['connectionEpoch'] != witness['authority']['connection_epoch']
                            or witness['snapshot']['connectionEpoch'] <= sources[2]['snapshot']['connectionEpoch']
                            or json_pointer(witness, FRESH_CHILD_POINTER) != previous['childSessionId']):
                        raise ValueError('new parent discovery did not preserve its scope, pointer and epoch')
                elif child_transcript_reconnect and len(sources) == 5:
                    receipt = validate_witness(witness, endpoint=kind, transition_phase='failed', authored_marker=gate.marker)
                    previous = receipts[1]
                    if (receipt['parentSessionId'] != previous['parentSessionId']
                            or receipt['childSessionId'] != previous['childSessionId']
                            or receipt['sourceIdentity'] != previous['sourceIdentity']
                            or witness['snapshot']['connectionEpoch'] != sources[3]['snapshot']['connectionEpoch']
                            or receipt['childSessionId'] != json_pointer(sources[3], FRESH_CHILD_POINTER)):
                        raise ValueError('reconnected parent changed its original child pointer or did not advance epoch')
                    write_json(output / 'reconnected-parent-receipt.json', receipt)
            exit_status = process.wait(timeout=10)
            thread.join(timeout=5)
            expected_sources = 5 if child_transcript_reconnect else 3
            expected_child_sources = 3 if child_transcript_reconnect else 2
            if (exit_status != 0 or len(rows) != len(commands) + 1 or len(sources) != expected_sources or len(receipts) != 2
                    or (child_transcript_source and (len(child_sources) != expected_child_sources or len(child_receipts) != 2))):
                raise ValueError(kind + ' incomplete source exchange')
            state = gate.receipt()
            if (state['requestCount'] < 1 or not state['explicitlyReleased'] or state['aborted'] or state['timedOut']):
                raise ValueError('controlled failure was not explicitly released after capture')
            return {'endpoint': kind, 'version': config['expectedVersion'], 'commandCount': len(commands),
                    'processExit': exit_status, 'running': receipts[0], 'failed': receipts[1], 'gate': state,
                    **({'childTranscript': {'running': child_receipts[0], 'failed': child_receipts[1],
                                           'pair': validate_transcript_pair(child_receipts[0], child_receipts[1])}}
                       if child_transcript_source else {}),
                    **({'childTranscriptReconnect': {
                        'receipt': json.loads((output / 'transcript-reconnect-receipt.json').read_text(encoding='utf-8')),
                        'historyReceipt': json.loads((output / 'recovered-child-history-receipt.json').read_text(encoding='utf-8')),
                        'sourceFiles': ['witness-4.json', 'witness-5.json', 'transcript-reconnected.json',
                                        'recovered-child-history-receipt.json']}}
                       if child_transcript_reconnect else {})}
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
    args = parse_arguments()
    output = args.output.absolute()
    if output.exists() or not output.resolve().is_relative_to(SSD.resolve(strict=True)):
        raise ValueError('output must be a new directory under the existing SSD project')
    program = args.headless_program.resolve(strict=True)
    if not program.is_file() or not os.access(program, os.X_OK):
        raise ValueError('headless program must already exist and be executable')
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
             ROOT / 'crates/presentation/src/transcript.rs',
             ROOT / 'scripts/summary_transition_provider.py', ROOT / 'scripts/summary_transition_fixture.py',
             ROOT / 'scripts/transcript_source_fixture.py',
             ROOT / 'scripts/transcript_reconnect_fixture.py',
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
            commands = commands_for(kind, workspace, gates[kind].marker,
                                    child_transcript_source=args.child_transcript_source,
                                    child_transcript_reconnect=args.child_transcript_reconnect)
            try:
                if kind == 'pi':
                    config = prepare_pi(args.pi_root, case / 'data', case / 'home', provider_url, strict_environment=True)
                    results[kind] = run_profile(kind, dict(config, expectedVersion='1.3.0'), commands, program, workspace, case, env, gates[kind], child_transcript_source=args.child_transcript_source, child_transcript_reconnect=args.child_transcript_reconnect)
                else:
                    dsh_home = case / 'home'
                    dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home), DSH_AGENTS_HOME=str(dsh_home / 'agents'),
                                   DSH_TELEMETRY_DISABLED='1', AGENTKIT_SYNTHETIC_KEY='synthetic-key')
                    with owned_dsh(args.dsh_root, case / 'data', dsh_home, workspace, provider_url, dsh_env, query_sqlite=False) as dsh:
                        dsh_process = dsh.process
                        if dsh.version != '0.2.1-rc.2':
                            raise ValueError('unexpected DSH version')
                        results[kind] = run_profile(kind, dict(dsh.connector_config([]), expectedVersion=dsh.version), commands, program, workspace, case, env, gates[kind], child_transcript_source=args.child_transcript_source, child_transcript_reconnect=args.child_transcript_reconnect)
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
    suffix = '; child Transcript source paired' if args.child_transcript_source else ''
    if args.child_transcript_reconnect:
        suffix += '; reconnect -> same-pointer primary-child failed Transcript recovery'
    print('SUMMARY_TRANSITIONS_ACCEPTED pi+dsh; running-source -> controlled-failure-source' + suffix + '; desktop=false')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
