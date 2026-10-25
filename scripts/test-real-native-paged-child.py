#!/usr/bin/env python3
"""Exercise Native bounded child pagination through original headless/public APIs.

This runner consumes explicit prebuilt binaries and a fixed installed Kernel.
It never compiles, creates a window, reads harness-private history, or stores
provider request bodies or bearer credentials.
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
import signal
import subprocess
import threading
import time
import uuid

from native_child_pane_source_fixture import MARKER
from native_owned_fixture import owned_native
from native_paged_child_fixture import (CHILD_MARKER, phases_for_pages, PREFIX, paged_native_provider,
    validate_prefix_frames, validate_public_history, validate_public_page_plan, validate_receipt)
from runtime_provenance import kernel_programs
from summary_transition_provider import HeldFailureGate
from task_family_fixture import native_task_seed_commands
from native_summary_transition_fixture import validate_parent_summary_sample

SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
AUTH_ENV = 'AGENTKIT_NATIVE_PAGED_OWNED_BEARER'


def write_json(path: Path, value: object) -> None:
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_headless(program: Path, native: dict[str, object], commands: list[dict[str, object]],
                 label: str, env: dict[str, str], workspace: Path, output: Path) -> list[dict[str, object]]:
    # Commands contain only public method/argument data; endpoint bearer is supplied separately.
    write_json(output / (label + '-commands.json'), commands)
    payload = {'kind': 'native', 'url': native['url'], 'bearer': native['bearer'],
               'expectedVersion': '0.9.3', 'commands': commands}
    result = subprocess.run([str(program)], input=json.dumps(payload), text=True, capture_output=True,
                            env=env, cwd=workspace, timeout=25)
    (output / (label + '.stdout')).write_text(result.stdout, encoding='utf-8')
    (output / (label + '.stderr')).write_text(result.stderr, encoding='utf-8')
    write_json(output / (label + '-exit.json'), {'exitStatus': result.returncode})
    if result.returncode != 0:
        raise ValueError(label + ' Native headless client failed')
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if len(rows) != len(commands) + 1 or rows[0] != {
            'kind': 'native', 'ready': True, 'version': '0.9.3'}:
        raise ValueError(label + ' Native handshake/command count mismatch')
    if any(row.get('op') != command['op'] or 'result' not in row
           for command, row in zip(commands, rows[1:])):
        raise ValueError(label + ' command/result pairing mismatch')
    return rows


def _group_alive(process: subprocess.Popen[str]) -> bool:
    try:
        os.killpg(process.pid, 0)
        return True
    except ProcessLookupError:
        return False


def _stop_group(process: subprocess.Popen[str]) -> bool:
    if _group_alive(process):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + 4
    while _group_alive(process) and time.monotonic() < deadline:
        time.sleep(.05)
    if _group_alive(process):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process.poll() is None:
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
        process.wait(timeout=5)
    return not _group_alive(process)


def _prefix_check(frames: list[dict[str, object]], groundtruth: dict[str, object],
                  parent: dict[str, object], child_id: str) -> None:
    validate_prefix_frames(frames, groundtruth=groundtruth, parent=parent, child_id=child_id)


def validate_summary_transition(frames: list[dict[str, object]]) -> dict[str, object]:
    """Validate the distinct, parent-owned passive Task Summary probe.

    A missing parent publication is retained as an explicit negative result;
    this supplemental probe never upgrades or waives bounded-history acceptance.
    """
    by_phase = {frame.get('phase'): frame for frame in frames}
    running = by_phase.get('running_head', {}).get('summaryRunning')
    terminal = by_phase.get('settled_refresh', {}).get('summarySettled')
    transition = by_phase.get('settled_refresh', {}).get('summaryTransition')
    if not isinstance(running, dict) or not isinstance(terminal, dict) or not isinstance(transition, dict):
        raise ValueError('Native demo omitted the parent Summary transition receipt')
    if transition.get('profile') != 'native-passive-spawn-tool' or transition.get('formalTaskLifecycleAcceptance') is not False or transition.get('parentToolFailureAcceptance') is not False:
        raise ValueError('spawn-tool evidence cannot claim formal Task or parent failure acceptance')
    if transition.get('parentProjectionOnly') is not True or transition.get('statusComesFromChildWitness') is not False:
        raise ValueError('Summary receipt improperly substitutes child source metadata')
    if type(transition.get('accepted')) is not bool:
        raise ValueError('Summary transition acceptance flag must be a boolean')
    accepted = transition['accepted'] is True
    if accepted:
        if transition.get('running') != running or transition.get('settled') != terminal:
            raise ValueError('summary transition subreceipt differs from its phase frames')
        running_source = validate_parent_summary_sample(running)
        terminal_source = validate_parent_summary_sample(terminal)
        for sample, phase, raw_status, status, source in (
                (running, 'running', 'doing', 'in_progress', running_source),
                (terminal, 'settled', 'done', 'completed', terminal_source)):
            if sample.get('available') is not True or sample.get('phase') != phase \
                    or sample.get('rawStatus') != raw_status \
                    or sample.get('canonicalStatus') != status or sample.get('mounted') is not True \
                    or sample.get('passive') is not True or sample.get('actionable') is not False \
                    or sample.get('independentRun') is not False:
                raise ValueError('parent Summary transition is not an installed passive Task row')
            if type(sample.get('publication')) is not int or type(sample.get('connectionEpoch')) is not int:
                raise ValueError('Summary sample omitted its Run publication/epoch')
            if type(sample.get('rowId')) is not int or not sample.get('viewInstance') \
                    or not sample.get('titleInstance') or source['rowId'] != sample['rowId'] \
                    or source['viewInstance'] != sample['viewInstance'] \
                    or source['titleInstance'] != sample['titleInstance']:
                raise ValueError('Summary mounted row/view/title identity is missing or changed')
        if running.get('taskId') != terminal.get('taskId') \
                or running.get('title') != terminal.get('title') \
                or running.get('scope') != terminal.get('scope') \
                or running.get('rowId') != terminal.get('rowId') \
                or running.get('viewInstance') != terminal.get('viewInstance') \
                or running.get('titleInstance') != terminal.get('titleInstance') \
                or type(terminal['publication']) is not int \
                or type(running['publication']) is not int \
                or terminal['publication'] <= running['publication'] \
                or type(terminal['connectionEpoch']) is not int \
                or type(running['connectionEpoch']) is not int \
                or terminal['connectionEpoch'] < running['connectionEpoch'] \
                or running['tool']['sourcePosition'] != terminal['tool']['sourcePosition'] \
                or running_source['sourceCallId'] != terminal_source['sourceCallId'] \
                or running_source['sourcePosition'] != terminal_source['sourcePosition']:
            raise ValueError('parent Summary identity/publication did not advance coherently')
        if transition.get('unavailableReason') is not None:
            raise ValueError('accepted Summary transition has an unavailable reason')
    else:
        if not isinstance(transition.get('unavailableReason'), str) \
                or not transition['unavailableReason']:
            raise ValueError('negative Summary probe must retain its concrete limitation')
    return {'accepted': accepted, 'profile': 'native-passive-spawn-tool',
            'formalTaskLifecycleAcceptance': False, 'parentToolFailureAcceptance': False,
            'running': running, 'settled': terminal,
            'unavailableReason': transition.get('unavailableReason'),
            'parentProjectionOnly': True}


def capture_demo(command: list[str], env: dict[str, str], workspace: Path, output: Path,
                 gate: HeldFailureGate, parent: dict[str, object], child_id: str,
                 groundtruth: dict[str, object]) -> tuple[list[dict[str, object]], dict[str, object], dict[str, object]]:
    phases = phases_for_pages(len(groundtruth['pagePlan']) - 1)
    process = None
    frames: list[dict[str, object]] = []
    events: list[str] = []
    released = False
    group_exited = False
    output_lines: queue.Queue[str | None] = queue.Queue()
    write_json(output / 'demo-command.json', command)
    try:
        with (output / 'demo.stderr').open('x', encoding='utf-8') as stderr:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr, text=True,
                env=env, cwd=workspace, start_new_session=True)
            deadline = time.monotonic() + 150
            assert process.stdout is not None
            def forward_stdout() -> None:
                for output_line in process.stdout:
                    output_lines.put(output_line)
                output_lines.put(None)
            reader = threading.Thread(target=forward_stdout, name='native-paged-demo-stdout', daemon=True)
            reader.start()
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('Native paged pane proof deadline')
                try:
                    line = output_lines.get(timeout=min(1, remaining))
                except queue.Empty:
                    continue
                if line is None:
                    break
                events.append(line)
                (output / 'demo.stdout').write_text(''.join(events), encoding='utf-8')
                if not line.startswith(PREFIX):
                    continue
                frame = json.loads(line[len(PREFIX):])
                index = len(frames)
                if index >= len(phases) or frame.get('phase') != phases[index]:
                    raise ValueError('unexpected or out-of-order Native paged source frame')
                write_json(output / f'frame-{index + 1}-{phases[index]}.json', frame)
                frames.append(frame)
                if len(frames) == len(phases)-1:
                    _prefix_check(frames, groundtruth, parent, child_id)
                    state = gate.receipt()
                    if state['requestCount'] != 1 or not state['entered']:
                        raise ValueError('fifth child request was not held after all pre-gate source frames')
                    gate.release_after_capture()
                    released = True
            status = process.wait(timeout=10)
            reader.join(timeout=5)
            if reader.is_alive():
                raise RuntimeError('Native paged stdout reader did not finish')
            group_exited = _stop_group(process)
            if status != 0 or len(frames) != len(phases) or not released:
                raise ValueError('Native paged demo did not complete every public-plan phase')
            if not group_exited:
                raise ValueError('Native paged demo process group did not exit cleanly')
            host_receipt = json.loads((output / 'host-receipt.json').read_text(encoding='utf-8'))
            gate_receipt = gate.receipt()
            accepted = validate_receipt(host_receipt, frames, groundtruth=groundtruth, gate=gate_receipt)
            accepted['summaryTransition'] = validate_summary_transition(frames)
            write_json(output / 'demo-process.json', {'pid': process.pid, 'exitStatus': status,
                'processGroup': process.pid, 'startNewSession': True,
                'processGroupExited': group_exited})
            return frames, host_receipt, accepted
    finally:
        gate.abort()
        if process is not None:
            group_exited = _stop_group(process)
            write_json(output / 'owned-demo-process.json', {'pid': process.pid,
                'exitStatus': process.poll(), 'processGroup': process.pid,
                'processGroupExited': group_exited})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'ui-demo', 'kernel-root', 'kernel-supervisor',
                 'kernel-worker', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    program, demo = (path.resolve(strict=True) for path in (args.headless_program, args.ui_demo))
    if any(not path.is_file() or not os.access(path, os.X_OK) for path in (program, demo)):
        parser.error('explicit prebuilt executable regular files required')
    output = args.output.expanduser().absolute()
    if (output.is_symlink() or not output.resolve().is_relative_to(SSD)
            or (output.exists() and (not output.is_dir() or any(output.iterdir())))):
        parser.error('output must be absent or empty under the existing AgentKit SSD project')
    output.mkdir(parents=True, exist_ok=True)
    output = output.resolve(strict=True)
    kernel_root = args.kernel_root.resolve(strict=True)
    supervisor, worker, kernel_identity = kernel_programs(
        kernel_root, supervisor=args.kernel_supervisor.resolve(strict=True),
        worker=args.kernel_worker.resolve(strict=True), expected_version='0.9.3')
    provider_snapshot = kernel_root / 'fixtures/config/providers.canonical.json'
    providers = json.loads(provider_snapshot.read_text(encoding='utf-8'))

    archive = output / 'programs'
    archive.mkdir()
    shutil.copy2(program, archive / 'tekes-agent-headless')
    shutil.copy2(demo, archive / 'tekes-agent-ui-demo-feature')
    script_paths = [Path(__file__).resolve(), Path(__file__).with_name('native_paged_child_fixture.py'),
        Path(__file__).with_name('native_child_pane_source_fixture.py'),
        Path(__file__).with_name('native_summary_transition_fixture.py'),
        Path(__file__).with_name('native_owned_fixture.py'),
        Path(__file__).with_name('summary_transition_provider.py'),
        Path(__file__).with_name('owned_endpoint_provider.py'),
        Path(__file__).with_name('task_family_fixture.py'),
        Path(__file__).with_name('runtime_provenance.py')]
    write_json(output / 'source-receipt.json', {'kernel': kernel_identity,
        'headless': {'path': str(program), 'sha256': digest(program)},
        'demo': {'path': str(demo), 'sha256': digest(demo)},
        'archivedPrograms': [{'path': str(archive / name), 'sha256': digest(archive / name)}
                             for name in ('tekes-agent-headless', 'tekes-agent-ui-demo-feature')],
        'providerSnapshot': {'path': str(provider_snapshot), 'sha256': digest(provider_snapshot)},
        'scripts': [{'path': str(path), 'sha256': digest(path)} for path in script_paths]})

    base = output / 'owned'
    base.mkdir(mode=0o700)
    workspace = base / 'workspace'
    workspace.mkdir()
    proof = workspace / 'proof.txt'
    proof.write_text('owned Native paged child proof\n', encoding='utf-8')
    home = base / 'client-home'
    home.mkdir(mode=0o700)
    env = {'HOME': str(home), 'TMPDIR': str(home),
           'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'LANG': 'C.UTF-8'}
    requests: list[str] = []
    gate = HeldFailureGate(MARKER)
    provider, provider_state = paged_native_provider(workspace=workspace, requests=requests, gate=gate)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), provider)
    server_thread = threading.Thread(target=server.serve_forever, name='native-paged-provider', daemon=True)
    server_thread.start()
    native = None
    failure: BaseException | None = None
    accepted: dict[str, object] | None = None
    service_exits: dict[str, object] = {}
    try:
        with owned_native(supervisor, worker, providers,
                f'http://127.0.0.1:{server.server_port}/v1', base, env) as native:
            parent_id, auxiliary_id = str(uuid.uuid4()), str(uuid.uuid4())
            model_provider = providers['providers'][0]
            commands = native_task_seed_commands(parent_id, workspace,
                model_provider['id'], model_provider['models'][0]['id'])[:5]
            execution = commands[0]['args']['execution']
            execution['allowed_tools'] = ['task', 'read']
            execution['read_roots'] = [str(workspace)]
            commands.append(dict(commands[0], args=dict(commands[0]['args'],
                sessionId=auxiliary_id), saveAs='auxiliary'))
            rows = run_headless(program, native, commands, 'seed', env, workspace, output)
            if rows[1]['result'].get('sessionId') != parent_id or rows[-1]['result'].get('sessionId') != auxiliary_id:
                raise ValueError('seed did not create the original host two distinct main slots')
            if not gate.entered.wait(30):
                # Diagnose only through the same public APIs, while this owned
                # supervisor is still alive. Never inspect its private journal.
                rows = run_headless(program, native,
                    [{'op': 'childCatalog', 'sessionId': parent_id}],
                    'failed-public-catalog', env, workspace, output)
                for index, entry in enumerate(rows[-1]['result'].get('entries', [])):
                    if entry.get('kind') == 'child':
                        run_headless(program, native, [{'op':'call','method':'subagents.history',
                            'args':{'parentSessionId':parent_id,'childSessionId':entry['id'],
                                    'maxMessages':200}}], f'failed-public-child-{index}',
                            env, workspace, output)
                raise ValueError('fifth Native child model request did not enter held gate')
            parent = {'endpointId': 'native', 'threadId': parent_id, 'sessionId': parent_id}
            catalog_rows = run_headless(program, native,
                [{'op': 'childCatalog', 'sessionId': parent_id}], 'catalog', env, workspace, output)
            catalog = catalog_rows[-1]['result']
            children = [entry for entry in catalog.get('entries', [])
                        if isinstance(entry, dict) and entry.get('kind') == 'child']
            if len(children) != 1 or children[0].get('mode') != 'bounded':
                raise ValueError('public Native parent catalog did not expose one bounded child')
            child_id = children[0]['id']
            if child_id in (parent_id, auxiliary_id):
                raise ValueError('bounded child must not reuse a main slot')
            history_commands = [
                {'op': 'call', 'method': 'subagents.history', 'args': {
                    'parentSessionId': parent_id, 'childSessionId': child_id, 'maxMessages': 200}},
                {'op': 'call', 'method': 'subagents.history', 'args': {
                    'parentSessionId': parent_id, 'childSessionId': child_id, 'maxMessages': 1}},
            ]
            history_rows = run_headless(program, native, history_commands,
                'public-groundtruth', env, workspace, output)
            groundtruth = validate_public_history(history_rows[1]['result'], history_rows[2]['result'],
                parent=parent, child_id=child_id)
            pages = [history_rows[2]['result']]
            page_requests = [history_commands[1]['args']]
            while pages[-1].get('hasMoreBefore') is True:
                if len(pages) >= 17:
                    raise ValueError('public page plan exceeded bounded validation limit')
                request = {'parentSessionId':parent_id,'childSessionId':child_id,'maxMessages':1,
                    'throughSequence':groundtruth['throughSequence'],
                    'historyRevision':groundtruth['historyRevision'],
                    'beforeSequence':min(entry['event']['seq'] for entry in pages[-1]['entries'])}
                rows = run_headless(program,native,[{'op':'call','method':'subagents.history','args':request}],
                    f'public-earlier-{len(pages)}',env,workspace,output)
                pages.append(rows[-1]['result']);page_requests.append(request)
            groundtruth = validate_public_page_plan(groundtruth,pages,page_requests)
            write_json(output / 'public-history-groundtruth.json', groundtruth)
            write_json(output / 'seed-receipt.json', {'parent': parent, 'childId': child_id,
                'auxiliarySessionId':auxiliary_id, 'seedExitedBeforeDemo':True,
                'gateBeforePane': gate.receipt(), 'providerStepCount': len(provider_state['stepReceipts']),
                'providerSteps': provider_state['stepReceipts'], 'privateHistoryRead': False,
                'requestBodiesPersisted': False})
            write_json(output / 'pane-manifest.json', {'parent': parent, 'childId': child_id,
                'marker': MARKER, 'profile': 'native-paged', 'expectedEarlierReads':len(pages)-1})
            write_json(output / 'endpoints.json', {'endpoints': [{
                'kind': 'native', 'endpointId': 'native', 'url': native['url'],
                'bearerEnv': AUTH_ENV, 'sessions': [parent_id, auxiliary_id]}]})
            demo_env = dict(env, **{AUTH_ENV: native['bearer']})
            command = [str(demo), '--components', '--transcript-content', '--endpoint-config',
                str(output / 'endpoints.json'), '--child-pane-source-proof', str(output / 'pane-manifest.json'),
                '--child-pane-source-receipt', str(output / 'host-receipt.json'),
                '--child-pane-paged-source', '--offscreen', str(output / 'native-paged-pane.png')]
            frames, host_receipt, accepted = capture_demo(command, demo_env, workspace, output,
                gate, parent, child_id, groundtruth)
            if (provider_state.get('childRequests') != 5
                    or provider_state.get('gateResponseStatus') != 400
                    or provider_state.get('gateErrorCode') != 'invalid_request_error'):
                raise ValueError('fifth Native child request did not return the controlled HTTP 400')
            accepted.update(kernelVersion='0.9.3', providerStepCount=len(provider_state['stepReceipts']),
                privateHistoryRead=False, requestBodiesPersisted=False)
    except BaseException as error:
        failure = error
        write_json(output / 'failure.json', {'type': type(error).__name__, 'message': str(error)})
    finally:
        gate.abort()
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=5)
        if native is not None:
            service_exits['supervisor'] = {'pid': native['process'].pid,
                'exitStatus': native['process'].poll()}
            (output / 'kernel-diagnostics.log').write_text(native['diagnostic_tail'](), encoding='utf-8')
        write_json(output / 'provider-receipt.json', {'paths': requests,
            'childRequestCount': provider_state['childRequests'],
            'gateResponseStatus': provider_state['gateResponseStatus'],
            'gateErrorCode': provider_state['gateErrorCode'],
            'stepReceipts': provider_state['stepReceipts'], 'requestBodiesPersisted': False})
        write_json(output / 'gate-final.json', gate.receipt())
        write_json(output / 'owned-services-exit.json', {'providerThreadStopped': not server_thread.is_alive(),
            'providerServerClosed': True, **service_exits})
    if failure is not None:
        raise failure
    if accepted is None:
        raise RuntimeError('Native paged run ended without accepted evidence')
    if server_thread.is_alive() or (native is not None and native['process'].poll() is None):
        raise RuntimeError('owned services remained alive; final acceptance withheld')
    accepted['ownedServicesExited'] = True
    # The final positive receipt is created only after the demo process and all owned services exit.
    write_json(output / 'accepted.json', accepted)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
