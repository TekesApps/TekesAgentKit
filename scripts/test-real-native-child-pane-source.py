#!/usr/bin/env python3
"""Run original readonly Native child pane against owned Kernel and loopback provider.

Consumes explicit prebuilt executables; never compiles, opens a window, or reads
private harness history. The provider gate is released only after validating the
original mounted pane's running publication and same-local-read typed head.
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
import uuid

from native_child_pane_source_fixture import (MARKER, PREFIX, native_child_provider,
    validate_native_frame, validate_native_receipt)
from native_owned_fixture import owned_native
from runtime_provenance import kernel_programs
from summary_transition_provider import HeldFailureGate
from task_family_fixture import native_task_seed_commands

SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
AUTH_ENV = 'AGENTKIT_NATIVE_CHILD_OWNED_BEARER'


def write_json(path, value):
    with path.open('x', encoding='utf-8') as out:
        json.dump(value, out, ensure_ascii=False, indent=2)
        out.write('\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_headless(program, native, commands, case, label, env, workspace):
    write_json(case / (label + '-commands.json'), commands)
    result = subprocess.run([str(program)], input=json.dumps({
        'kind': 'native', 'url': native['url'], 'bearer': native['bearer'],
        'expectedVersion': '0.9.3', 'commands': commands}),
        text=True, capture_output=True, env=env, cwd=workspace, timeout=15)
    (case / (label + '.stdout')).write_text(result.stdout, encoding='utf-8')
    (case / (label + '.stderr')).write_text(result.stderr, encoding='utf-8')
    write_json(case / (label + '-exit.json'), {'exitStatus': result.returncode})
    if result.returncode != 0:
        raise ValueError(label + ' headless process failed')
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if (len(rows) != len(commands) + 1 or rows[0] != {
            'kind': 'native', 'ready': True, 'version': '0.9.3'}):
        raise ValueError(label + ' Native handshake or command count mismatch')
    if any(row.get('op') != command['op'] or 'result' not in row
           for command, row in zip(commands, rows[1:])):
        raise ValueError(label + ' command/result pairing mismatch')
    return rows


def capture_demo(command, env, workspace, case, gate, parent, child):
    events = queue.Queue()
    process = None
    lines, proofs = [], []
    write_json(case / 'demo-command.json', command)
    try:
        with (case / 'demo.stderr').open('x') as stderr:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr,
                text=True, env=env, cwd=workspace)
            def read():
                for line in process.stdout:
                    events.put(line)
                events.put(None)
            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            deadline = time.monotonic() + 75
            while True:
                try:
                    line = events.get(timeout=max(.01, deadline - time.monotonic()))
                except queue.Empty:
                    raise RuntimeError('Native child pane proof deadline') from None
                if line is None:
                    break
                lines.append(line)
                (case / 'demo.stdout').write_text(''.join(lines), encoding='utf-8')
                if not line.startswith(PREFIX):
                    continue
                if len(proofs) >= 2:
                    raise ValueError('unexpected extra Native source frame')
                phase = 'running' if not proofs else 'failed'
                frame = json.loads(line[len(PREFIX):])
                write_json(case / ('source-' + phase + '.json'), frame)
                proof = validate_native_frame(frame, parent=parent, child_id=child,
                    expected_phase=phase, previous=proofs[0] if proofs else None)
                write_json(case / ('source-' + phase + '-receipt.json'), proof)
                proofs.append(proof)
                if phase == 'running':
                    state = gate.receipt()
                    if state['requestCount'] != 1 or not state['entered']:
                        raise ValueError('running publication is not paired with one held child request')
                    gate.release_after_capture()
            status = process.wait(timeout=10)
            reader.join(timeout=5)
            if status != 0 or len(proofs) != 2:
                raise ValueError('Native original pane did not complete both source publications')
            gate_state = gate.receipt()
            if (gate_state['requestCount'] != 1 or not gate_state['explicitlyReleased']
                    or gate_state['timedOut'] or gate_state['aborted']):
                raise ValueError('Native gate failed strict lifecycle validation')
            receipt = json.loads((case / 'host-receipt.json').read_text())
            accepted = validate_native_receipt(receipt, *proofs)
            accepted.update(gate=gate_state, demoExit=status)
            return accepted
    finally:
        gate.abort()
        if process is not None:
            if process.poll() is None:
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=10)
            write_json(case / 'owned-demo-exit.json', {'pid': process.pid,
                'exitStatus': process.poll()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'ui-demo', 'kernel-root', 'kernel-supervisor', 'kernel-worker', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    program, demo = (p.resolve(strict=True) for p in (args.headless_program, args.ui_demo))
    for executable in (program, demo):
        if not executable.is_file() or not os.access(executable, os.X_OK):
            parser.error('explicit executable regular files required')
    output = args.output.expanduser().absolute()
    if (output.is_symlink() or not output.resolve().is_relative_to(SSD)
            or (output.exists() and (not output.is_dir() or any(output.iterdir())))):
        parser.error('output must be absent or empty under existing AgentKit SSD project')
    kernel_root = args.kernel_root.resolve(strict=True)
    supervisor, worker, identity = kernel_programs(kernel_root,
        supervisor=args.kernel_supervisor.resolve(strict=True), worker=args.kernel_worker.resolve(strict=True),
        expected_version='0.9.3')
    provider_path = kernel_root / 'fixtures/config/providers.canonical.json'
    providers = json.loads(provider_path.read_text())
    output.mkdir(parents=True, exist_ok=True)
    programs = output / 'programs'
    programs.mkdir()
    for source, name in ((program, 'tekes-agent-headless'), (demo, 'tekes-agent-ui-demo-feature')):
        shutil.copy2(source, programs / name)
    write_json(output / 'source-receipt.json', {'kernel': identity,
        'headless': {'path': str(program), 'sha256': digest(program)},
        'demo': {'path': str(demo), 'sha256': digest(demo)},
        'providerSnapshot': {'path': str(provider_path), 'sha256': digest(provider_path)},
        'scripts': [{'path': str(p), 'sha256': digest(p)} for p in (
            Path(__file__).resolve(), Path(__file__).with_name('native_child_pane_source_fixture.py'),
            Path(__file__).with_name('native_owned_fixture.py'),
            Path(__file__).with_name('summary_transition_provider.py'),
            Path(__file__).with_name('owned_endpoint_provider.py'),
            Path(__file__).with_name('task_family_fixture.py'),
            Path(__file__).with_name('runtime_provenance.py'))]})
    base = output / 'owned'
    base.mkdir(mode=0o700)
    workspace = base / 'workspace'
    workspace.mkdir()
    (workspace / 'proof.txt').write_text('owned Native child pane source proof\n')
    home = base / 'client-home'
    home.mkdir(mode=0o700)
    env = {'HOME': str(home), 'TMPDIR': str(home), 'PATH': os.environ.get('PATH', '/usr/bin:/bin'),
           'LANG': 'C.UTF-8'}
    requests = []
    gate = HeldFailureGate(MARKER)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
        native_child_provider(workspace=lambda: workspace, requests=requests, gate=gate))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    native = None
    failure = None
    try:
        with owned_native(supervisor, worker, providers,
                f'http://127.0.0.1:{server.server_port}/v1', base, env) as native:
            parent_id, aux_id = str(uuid.uuid4()), str(uuid.uuid4())
            first = providers['providers'][0]
            commands = native_task_seed_commands(parent_id, workspace, first['id'], first['models'][0]['id'])[:5]
            auxiliary = dict(commands[0], args=dict(commands[0]['args'], sessionId=aux_id), saveAs='auxiliary')
            commands.append(auxiliary)
            rows = run_headless(program, native, commands, output, 'seed', env, workspace)
            if rows[1]['result']['sessionId'] != parent_id or rows[-1]['result']['sessionId'] != aux_id:
                raise ValueError('seed did not create two original ordinary main sessions')
            if not gate.entered.wait(8):
                raise ValueError('child request did not survive seed client exit')
            rows = run_headless(program, native, [{'op': 'childCatalog', 'sessionId': parent_id}],
                                output, 'catalog', env, workspace)
            catalog = rows[-1]['result']
            children = [item for item in catalog['entries'] if item.get('kind') == 'child']
            if (catalog.get('parentAvailable') is not True or len(children) != 1
                    or children[0].get('mode') != 'bounded'):
                raise ValueError('expected one public bounded child in original parent catalog')
            child = children[0]['id']
            if child in (parent_id, aux_id):
                raise ValueError('child incorrectly reused as main slot')
            parent = {'endpointId': 'native', 'threadId': parent_id, 'sessionId': parent_id}
            write_json(output / 'seed-receipt.json', {'seedExitedBeforeDemo': True,
                'supervisorAlive': native['process'].poll() is None, 'gate': gate.receipt(),
                'parent': parent, 'auxiliarySessionId': aux_id, 'childId': child,
                'catalog': catalog, 'privateHistoryRead': False})
            write_json(output / 'pane-manifest.json', {'parent': parent, 'childId': child, 'marker': MARKER})
            write_json(output / 'endpoints.json', {'endpoints': [{
                'kind': 'native', 'endpointId': 'native', 'url': native['url'],
                'bearerEnv': AUTH_ENV, 'sessions': [parent_id, aux_id]}]})
            demo_env = dict(env, **{AUTH_ENV: native['bearer']})
            command = [str(demo), '--components', '--transcript-content', '--endpoint-config',
                str(output / 'endpoints.json'), '--child-pane-source-proof', str(output / 'pane-manifest.json'),
                '--child-pane-source-receipt', str(output / 'host-receipt.json'),
                '--offscreen', str(output / 'pane.png')]
            accepted = capture_demo(command, demo_env, workspace, output, gate, parent, child)
            accepted.update(kernelVersion='0.9.3', seedExitedBeforeDemo=True,
                providerRequestCount=len(requests), privateHistoryRead=False,
                desktopInteraction=False, physicalIme=False, paidModel=False)
            write_json(output / 'accepted.json', accepted)
    except BaseException as error:
        failure = error
        write_json(output / 'failure.json', {'type': type(error).__name__, 'message': str(error)})
    finally:
        gate.abort()
        server.shutdown()
        server.server_close()
        write_json(output / 'gate-final.json', gate.receipt())
        write_json(output / 'provider-requests.json', {'paths': requests, 'requestBodiesPersisted': False})
        if native is not None:
            write_json(output / 'owned-supervisor-exit.json', {
                'pid': native['process'].pid, 'exitStatus': native['process'].poll()})
            (output / 'kernel-diagnostics.log').write_text(native['diagnostic_tail']())
    if failure is not None:
        raise failure
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
