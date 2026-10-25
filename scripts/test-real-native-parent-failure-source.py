#!/usr/bin/env python3
"""Run the bounded Native approval denial through the public Rust consumer."""
from __future__ import annotations
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import threading
import uuid

from native_owned_fixture import owned_native
from native_parent_failure_fixture import validate_parent_only_catalog, validate_running_to_failed_parent
from runtime_provenance import kernel_programs

ROOT = Path(__file__).resolve().parents[1]
SSD_PROJECT = Path('/Volumes/HP P900/dev/TekesAgentKit')
FROZEN_HEAD = '5b7c84361206b45ac381b10437097d3b76c322c1'
CALL_ID = 'agentkit-parent-approval-task-call'
PROBE_MARKER = 'AGENTKIT_NATIVE_PARENT_APPROVAL'
EXPECTED_DRAFT = 'Keep this parent draft unchanged across approval rejection.'


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def write_json(path: Path, value: object) -> None:
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2)
        stream.write('\n')


def task_tool(value: object) -> dict | None:
    if not isinstance(value, dict):
        return None
    if value.get('type') == 'function' and value.get('name') == 'task':
        return value
    nested = value.get('function')
    return nested if isinstance(nested, dict) and nested.get('name') == 'task' else None


def task_arguments() -> dict:
    return {'task_name': 'Approval rejection parent Task proof',
            'goal': 'Verify the original parent Task fails when its per-call approval is rejected.',
            'instructions': 'Do not create a child. Use the valid inline output contract.',
            'input_sources': [],
            'output': {'mode': 'inline', 'format': 'text', 'name': 'result', 'path': '',
                       'contract': 'A short inline result.'}}


def provider_server():
    observations: list[dict] = []
    lock = threading.Lock()

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            with lock:
                ordinal = len(observations) + 1
                item = {'ordinal': ordinal, 'status': 'entered'}
                observations.append(item)
            try:
                length = int(self.headers.get('Content-Length', '0'))
                if not 0 < length <= 16 * 1024 * 1024 or not self.path.endswith('/responses'):
                    raise ValueError('unexpected provider request')
                body = json.loads(self.rfile.read(length))
                inputs = body.get('input')
                if not isinstance(inputs, list):
                    raise ValueError('Responses input is not an item array')
                marker = any(PROBE_MARKER in json.dumps(x, ensure_ascii=False) for x in inputs)
                outputs = [x for x in inputs if isinstance(x, dict) and x.get('type') == 'function_call_output']
                matching = [x for x in outputs if x.get('call_id') == CALL_ID]
                item.update(markerPresent=marker, matchingOutputCount=len(matching),
                    outputCount=len(outputs), continuationMatches=body.get('previous_response_id') == 'agentkit-parent-approval-response',
                    inputTypes=[x.get('type') for x in inputs if isinstance(x, dict)])
                if ordinal == 1:
                    if not marker or outputs:
                        raise ValueError('initial provider turn did not match parent probe')
                    tools = body.get('tools')
                    task = next((task_tool(x) for x in tools if task_tool(x)), None) if isinstance(tools, list) else None
                    if task is None:
                        raise ValueError('Kernel omitted public task schema')
                    frames = [
                        {'type': 'response.output_item.added', 'output_index': 0,
                         'item': {'type': 'function_call', 'call_id': CALL_ID, 'name': 'task', 'arguments': ''}},
                        {'type': 'response.function_call_arguments.delta', 'output_index': 0,
                         'delta': json.dumps(task_arguments())},
                        {'type': 'response.function_call_arguments.done', 'output_index': 0,
                         'arguments': json.dumps(task_arguments())},
                        {'type': 'response.completed', 'response': {'id': 'agentkit-parent-approval-response',
                         'status': 'completed', 'output': [{'type': 'function_call', 'call_id': CALL_ID,
                         'name': 'task', 'arguments': json.dumps(task_arguments())}],
                         'usage': {'input_tokens': 20, 'output_tokens': 30, 'total_tokens': 50}}},
                    ]
                    item.update(status='completed', markerPresent=True, returnedCallId=CALL_ID,
                                validInlineContract=True, advertisedTaskSchema=True)
                else:
                    if not matching or not (marker or item['continuationMatches']):
                        raise ValueError('follow-up did not carry original denied parent tool result')
                    if any(x.get('call_id') != CALL_ID for x in outputs):
                        raise ValueError('unexpected child or unrelated tool output')
                    frames = [{'type': 'response.completed', 'response': {'id': f'agentkit-denial-followup-{ordinal}',
                        'status': 'completed', 'output': [{'type': 'message', 'role': 'assistant',
                        'phase': 'final_answer', 'status': 'completed', 'content': [{'type': 'output_text',
                        'text': 'The per-call approval was rejected.'}]}],
                        'usage': {'input_tokens': 25, 'output_tokens': 8, 'total_tokens': 33}}}]
                    item.update(status='completed', matchingParentFunctionCallOutput=True,
                                returnedAnotherTaskCall=False)
                payload = ''.join('data: ' + json.dumps(frame) + '\n\n' for frame in frames).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                self.wfile.flush()
            except (OSError, ValueError, TypeError, json.JSONDecodeError) as error:
                item.update(status='failed', error=type(error).__name__)
                payload = json.dumps({'error': {'type': 'invalid_request_error',
                    'message': 'owned approval fixture rejected provider request',
                    'code': 'invalid_request_error'}}).encode()
                self.send_response(400)
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

    return Provider, observations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe-program', required=True, type=Path,
                        help='explicit consumer example executable built from the frozen source overlay')
    parser.add_argument('--kernel-root', required=True, type=Path)
    parser.add_argument('--kernel-supervisor', required=True, type=Path)
    parser.add_argument('--kernel-worker', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    probe = args.probe_program.resolve(strict=True)
    if not probe.is_file() or not os.access(probe, os.X_OK):
        parser.error('--probe-program must be an existing executable')
    output = args.output.expanduser().absolute()
    if output.exists() or output.is_symlink() or not output.is_relative_to(SSD_PROJECT) or not output.parent.is_dir():
        parser.error('output must be new beneath the existing AgentKit SSD fixture root')
    kernel_root = args.kernel_root.resolve(strict=True)
    supervisor, worker, kernel = kernel_programs(kernel_root, supervisor=args.kernel_supervisor.resolve(strict=True),
        worker=args.kernel_worker.resolve(strict=True), expected_version='0.9.3')
    manifest_path = SSD_PROJECT / '.runtime-fixtures/stage-close-20261025/frozen-source.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('head') != FROZEN_HEAD:
        parser.error('frozen overlay manifest head differs from required 5b source')
    providers_path = kernel_root / 'fixtures/config/providers.canonical.json'
    providers = json.loads(providers_path.read_text())
    provider = providers['providers'][0]

    output.mkdir(parents=True, mode=0o700)
    owned = output / 'owned'
    owned.mkdir(mode=0o700)
    workspace = owned / 'workspace'
    workspace.mkdir(mode=0o700)
    write_json(output / 'source-programs.json', {
        'sourceRevision': FROZEN_HEAD,
        'probeProgram': {'path': str(probe), 'sha256': digest(probe)},
        'kernel': kernel,
        'providerSnapshot': {'path': str(providers_path), 'sha256': digest(providers_path)},
        'frozenSourceManifest': {'path': str(manifest_path), 'sha256': digest(manifest_path)},
        'pythonInputs': {str(p.resolve()): digest(p.resolve()) for p in (Path(__file__),
            Path(__file__).with_name('native_parent_failure_fixture.py'),
            Path(__file__).with_name('native_summary_transition_fixture.py'),
            Path(__file__).with_name('native_owned_fixture.py'),
            Path(__file__).with_name('runtime_provenance.py'))},
    })
    Provider, observations = provider_server()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    server.daemon_threads = False
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    provider_url = f'http://127.0.0.1:{server.server_port}/v1'
    context = None
    context_entered = False
    result = None
    failure = None
    native = None
    try:
        context = owned_native(supervisor, worker, providers, provider_url, owned,
            {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'C.UTF-8'})
        native = context.__enter__()
        context_entered = True
        home = owned / 'probe-home'
        home.mkdir(mode=0o700)
        config = {'url': native['url'], 'bearer': native['bearer'], 'cwd': str(workspace),
                  'provider': provider['id'], 'model': provider['models'][0]['id']}
        env = {'HOME': str(home), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin',
               'TMPDIR': str(home), 'LANG': 'C.UTF-8'}
        completed = subprocess.run([str(probe)], input=json.dumps(config), text=True,
            capture_output=True, timeout=180, env=env, cwd=workspace)
        write_json(output / 'consumer-process.json', {'exitStatus': completed.returncode,
            'stdout': completed.stdout, 'stderr': completed.stderr})
        if completed.returncode != 0:
            raise RuntimeError(f'public consumer exited {completed.returncode}: {completed.stderr[-2000:]}')
        rows = [line for line in completed.stdout.splitlines() if line.strip()]
        if len(rows) != 1:
            raise RuntimeError('public consumer must emit exactly one JSON evidence row')
        result = json.loads(rows[0])
        if not observations or observations[0].get('returnedCallId') != CALL_ID \
                or any(item.get('status') != 'completed' for item in observations) \
                or any(not item.get('matchingParentFunctionCallOutput') for item in observations[1:]):
            raise RuntimeError('provider evidence includes a missing/failed call or an unrelated follow-up')
        write_json(output / 'parent-approval-transition.json', result)
        catalog = validate_parent_only_catalog(result.get('catalog'), result['running']['scope']['sessionId'])
        transition = validate_running_to_failed_parent(result['running'], result['failed'],
            approval_response=result['response'], expected_draft=EXPECTED_DRAFT)
        transition['publicCatalog'] = catalog
        write_json(output / 'parent-failure-validation.json', transition)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyError, TypeError) as error:
        failure = f'{type(error).__name__}: {error}'
        write_json(output / 'failure.json', {'error': failure, 'acceptance': False})
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        write_json(output / 'provider-observation.json', {
            'provider': 'synthetic-loopback', 'requestBodiesPersisted': False,
            'requestCount': len(observations), 'requests': observations,
            'everyFollowupMatchedOriginalCall': all(x.get('matchingParentFunctionCallOutput')
                for x in observations[1:]) if len(observations) > 1 else False,
        })
        if context is not None and context_entered:
            try:
                context.__exit__(None, None, None)
            except BaseException as error:
                failure = failure or f'Kernel shutdown: {type(error).__name__}'
        if native is not None:
            exit_status = native['process'].poll()
            write_json(output / 'owned-exit.json', {'pid': native['process'].pid, 'exitStatus': exit_status})
            if exit_status != 0:
                failure = failure or f'Owned Kernel exit status: {exit_status}'
        if failure is None:
            write_json(output / 'acceptance.json', {'accepted': True, 'transition': 'doing->failed',
                'probeProgramSha256': digest(probe), 'kernelVersion': kernel['build']['version'],
                'providerRequestCount': len(observations)})
    return 1 if failure else 0


if __name__ == '__main__':
    raise SystemExit(main())
