#!/usr/bin/env python3
"""Run one bounded frozen Pi 1.3.0 cancel or Unknown public consumer flow.

The runner never builds. It uses an explicitly supplied executable, the fixed
Pi installation, the public Rust consumer, and a synthetic loopback provider.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import threading
import time

from pi_owned_fixture import prepare_pi
from pi_run_lifecycle_validation import validate_cancel, validate_unknown_reconciliation

SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
FROZEN_HEAD = '5b7c84361206b45ac381b10437097d3b76c322c1'
UNKNOWN_MARKER = 'AGENTKIT_PI_UNKNOWN_PROOF_20261025'
CANCEL_MARKER = 'ENDPOINT_CANCEL_NATIVE_PROOF_20261025'
POST_DRAFT = 'Preserve this draft while the Run is uncertain or cancelled.'
MAX_PROVIDER_BODY = 4 * 1024 * 1024
MAX_OUTPUT_LINE = 8 * 1024 * 1024


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def write_new(path: Path, value: object) -> None:
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2)
        stream.write('\n')


def write_signal(path: Path, value: bytes) -> None:
    with path.open('xb') as stream:
        stream.write(value)


def sha_json(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def provider_handler(action: str, state: dict, lock: threading.Lock,
                     cancel_wakeup: threading.Event):
    marker = UNKNOWN_MARKER if action == 'unknown' else CANCEL_MARKER

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def setup(self):
            super().setup()
            with lock:
                state['activeHandlers'] += 1

        def finish(self):
            try:
                super().finish()
            finally:
                with lock:
                    state['activeHandlers'] -= 1

        def do_POST(self):
            try:
                length = int(self.headers.get('Content-Length', '0'))
                if not 0 < length <= MAX_PROVIDER_BODY or not self.path.endswith('/chat/completions'):
                    raise ValueError('unexpected bounded Pi provider request')
                body = json.loads(self.rfile.read(length))
                messages = body.get('messages')
                if not isinstance(messages, list):
                    raise ValueError('Pi chat request lacks messages')
                marker_present = marker in json.dumps(messages, ensure_ascii=False)
                selected = next((m for m in reversed(messages)
                                 if isinstance(m, dict) and m.get('role') == 'user'
                                 and marker in json.dumps(m, ensure_ascii=False)), None)
                request = {'path': self.path, 'markerPresent': marker_present,
                           'matchingUserMessage': selected is not None,
                           'bodySha256': digest_bytes(json.dumps(body, sort_keys=True).encode()),
                           'requestId': self.headers.get('X-Request-Id') is not None}
                with lock:
                    state['requests'].append(request)
                    if selected is not None:
                        state['matchingBusinessRequests'] += 1
                        ordinal = state['matchingBusinessRequests']
                    else:
                        ordinal = 0
                if selected is not None and ordinal != 1:
                    raise ValueError('provider saw a duplicate marked business mutation')
                if action == 'cancel' and selected is not None:
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/event-stream')
                    self.send_header('Cache-Control', 'no-cache')
                    self.end_headers()
                    chunk = {'id': 'pi-cancel-held', 'object': 'chat.completion.chunk',
                             'created': 1, 'model': 'endpoint-test',
                             'choices': [{'index': 0, 'delta': {'role': 'assistant',
                                 'content': 'CANCEL_PENDING'}, 'finish_reason': None}]}
                    self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
                    self.wfile.flush()
                    with lock:
                        state['firstStreamFrameSent'] = True
                    state['entered'].set()
                    if not wait_for_disconnect(self.connection, cancel_wakeup, 100):
                        with lock:
                            state['handlerTimedOut'] = True
                    with lock:
                        state['connectionClosedAfterCancel'] = client_closed(self.connection)
                    return

                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.send_header('Cache-Control', 'no-cache')
                self.end_headers()
                common = {'id': 'pi-lifecycle-reply', 'object': 'chat.completion.chunk',
                          'created': 1, 'model': 'endpoint-test'}
                text = ('Unknown response was applied once.' if action == 'unknown'
                        and selected is not None else 'Fixture setup response.')
                frames = [
                    dict(common, choices=[{'index': 0,
                        'delta': {'role': 'assistant', 'content': text}, 'finish_reason': None}]),
                    dict(common, choices=[{'index': 0, 'delta': {}, 'finish_reason': 'stop'}],
                         usage={'prompt_tokens': 16, 'completion_tokens': 8, 'total_tokens': 24}),
                ]
                for frame in frames:
                    self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
                    self.wfile.flush()
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
                if selected is not None:
                    state['entered'].set()
            except (BrokenPipeError, ConnectionResetError, OSError):
                with lock:
                    if action == 'cancel' and state['firstStreamFrameSent']:
                        state['connectionClosedAfterCancel'] = True
            except (ValueError, TypeError, json.JSONDecodeError) as error:
                with lock:
                    state['providerError'] = type(error).__name__
                try:
                    self.send_error(400)
                except OSError:
                    pass

    return Provider


def digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def client_closed(connection: socket.socket) -> bool:
    try:
        ready, _, _ = select.select([connection], [], [], 0)
        return bool(ready and connection.recv(1, socket.MSG_PEEK | socket.MSG_DONTWAIT) == b'')
    except (BlockingIOError, ConnectionResetError, OSError):
        return True


def wait_for_disconnect(connection: socket.socket, wakeup: threading.Event, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and not wakeup.is_set():
        if client_closed(connection):
            return True
        wakeup.wait(0.025)
    return client_closed(connection)


class ProcessCapture:
    def __init__(self, process: subprocess.Popen, output: Path):
        self.process = process
        self.output = output
        self.lines: list[dict] = []
        self.line_errors: list[str] = []
        self.stderr_chunks: list[bytes] = []
        self.lock = threading.Lock()
        self.condition = threading.Condition(self.lock)
        self.stdout_thread = threading.Thread(target=self._read_stdout, daemon=True)
        self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
        self.stdout_thread.start()
        self.stderr_thread.start()

    def _read_stdout(self):
        assert self.process.stdout is not None
        for line in iter(self.process.stdout.readline, b''):
            if len(line) > MAX_OUTPUT_LINE:
                with self.condition:
                    self.line_errors.append('consumer output line exceeded bound')
                    self.condition.notify_all()
                break
            try:
                value = json.loads(line)
                if not isinstance(value, dict):
                    raise ValueError
            except (json.JSONDecodeError, ValueError):
                with self.condition:
                    self.line_errors.append('consumer emitted a non-object JSONL row')
                    self.condition.notify_all()
                continue
            with self.condition:
                self.lines.append(value)
                self.condition.notify_all()

    def _read_stderr(self):
        assert self.process.stderr is not None
        while chunk := self.process.stderr.read(65536):
            self.stderr_chunks.append(chunk)

    def wait_event(self, event: str, timeout: float) -> dict:
        deadline = time.monotonic() + timeout
        with self.condition:
            while True:
                found = [row for row in self.lines if row.get('event') == event]
                if found:
                    return found[-1]
                if self.line_errors:
                    raise RuntimeError(self.line_errors[-1])
                if self.process.poll() is not None:
                    raise RuntimeError(f'consumer exited before {event}; status={self.process.returncode}')
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(f'timed out waiting for consumer event {event}')
                self.condition.wait(min(remaining, 0.1))

    def wait_exit(self, timeout: float) -> int:
        status = self.process.wait(timeout=timeout)
        self.stdout_thread.join(timeout=2)
        self.stderr_thread.join(timeout=2)
        safe_stderr = b''.join(self.stderr_chunks).decode('utf-8', 'replace')[-8192:]
        (self.output / 'consumer.stderr.log').write_text(safe_stderr, encoding='utf-8')
        write_new(self.output / 'consumer.stdout.jsonl', self.lines)
        if self.line_errors:
            raise RuntimeError(self.line_errors[-1])
        return status


def process_group_snapshot(pgid: int) -> list[dict]:
    result = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid=,pgid=,state=,comm='],
                            capture_output=True, text=True, timeout=5, check=True)
    rows = []
    for line in result.stdout.splitlines():
        columns = line.strip().split(None, 4)
        if len(columns) != 5:
            continue
        try:
            pid, parent, group = map(int, columns[:3])
        except ValueError:
            continue
        if group == pgid:
            rows.append({'pid': pid, 'ppid': parent, 'pgid': group,
                         'state': columns[3], 'comm': columns[4]})
    return rows


def stop_owned_group(process: subprocess.Popen, pgid: int) -> dict:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            pass
    remaining = process_group_snapshot(pgid)
    cleanup_signal = None
    if remaining:
        os.killpg(pgid, signal.SIGTERM)
        cleanup_signal = 'SIGTERM-own-session-group'
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            remaining = process_group_snapshot(pgid)
            if not remaining:
                break
            time.sleep(0.05)
    if remaining:
        os.killpg(pgid, signal.SIGKILL)
        cleanup_signal = 'SIGKILL-own-session-group'
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            remaining = process_group_snapshot(pgid)
            if not remaining:
                break
            time.sleep(0.05)
    return {'cleanupSignal': cleanup_signal, 'remainingAfterCleanup': remaining,
            'groupAbsent': not remaining}


def wait_receipt_event(template: Path, event: str, timeout: float) -> tuple[Path, list[dict]]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for path in template.parent.glob(template.name.replace('{pid}', '*')):
            try:
                rows = [json.loads(line) for line in path.read_text().splitlines() if line]
            except (OSError, json.JSONDecodeError):
                continue
            if any(row.get('event') == event for row in rows):
                return path, rows
        time.sleep(0.025)
    raise TimeoutError(f'timed out waiting for Pi wrapper receipt {event}')


def provider_receipt(state: dict, lock: threading.Lock, action: str) -> dict:
    with lock:
        requests = list(state['requests'])
        matches = state['matchingBusinessRequests']
        error = state['providerError']
        first = state['firstStreamFrameSent']
        closed = state['connectionClosedAfterCancel']
        timed_out = state['handlerTimedOut']
        active = state['activeHandlers']
    return {'provider': 'synthetic-loopback-openai-chat-completions',
            'requestBodiesPersisted': False, 'requests': requests,
            'requestCount': len(requests), 'matchingBusinessRequests': matches,
            'businessRequests': matches, 'firstStreamFrameSent': first,
            'connectionClosedAfterCancel': closed if action == 'cancel' else None,
            'heldHandlerTimedOut': timed_out, 'providerErrorType': error,
            'activeHandlers': active, 'handlerThreadsStopped': active == 0}


def validate_runtime(row: dict) -> None:
    capture = row.get('capture') or row.get('afterRelease') or row
    authority = capture.get('authority') if isinstance(capture, dict) else None
    product = authority.get('host', {}).get('product', {}) if isinstance(authority, dict) else {}
    if product.get('version') != '1.3.0':
        raise ValueError('captured public Pi host version is not 1.3.0')


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe-program', required=True, type=Path)
    parser.add_argument('--pi-root', required=True, type=Path)
    parser.add_argument('--action', choices=('cancel', 'unknown'), required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args(argv)
    args.probe_program = args.probe_program.resolve(strict=True)
    args.pi_root = args.pi_root.resolve(strict=True)
    args.output = args.output.expanduser().absolute()
    if not args.probe_program.is_file() or not os.access(args.probe_program, os.X_OK):
        parser.error('--probe-program must be an existing executable')
    if args.pi_root == Path('/') or not args.pi_root.is_dir():
        parser.error('--pi-root must be the existing fixed runtime directory')
    if args.output.exists() or args.output.is_symlink() or not args.output.is_relative_to(SSD) \
            or not args.output.parent.is_dir():
        parser.error('--output must be a new directory beneath the existing SSD AgentKit root')
    return args


def run(args) -> int:
    manifest_path = SSD / '.runtime-fixtures/stage-close-20261025/frozen-source.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('head') != FROZEN_HEAD:
        raise ValueError('frozen product source manifest does not identify 5b7c843')
    source_files = (Path(__file__), Path(__file__).with_name('pi_run_lifecycle_validation.py'),
                    Path(__file__).with_name('pi_owned_fixture.py'))
    source_hashes = {str(path.resolve()): digest(path.resolve()) for path in source_files}
    probe_sha = digest(args.probe_program)
    pi_node = args.pi_root / 'node/bin/node'
    pi_cli = args.pi_root / 'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js'
    if not pi_node.is_file() or not pi_cli.is_file():
        raise ValueError('fixed Pi root is missing its Node or CLI executable')
    node_version = subprocess.run([str(pi_node), '--version'], capture_output=True,
                                  text=True, timeout=5, check=True).stdout.strip()
    if node_version != 'v24.20.0':
        raise ValueError(f'Pi Node runtime does not match fixed 24.20.0: {node_version}')
    args.output.mkdir(mode=0o700)
    owned = args.output / 'owned'
    owned.mkdir(mode=0o700)
    workspace = owned / 'workspace'
    workspace.mkdir(mode=0o700)
    home, data_root = owned / 'home', owned / 'pi-data'
    provider_state = {'requests': [], 'matchingBusinessRequests': 0,
        'firstStreamFrameSent': False, 'connectionClosedAfterCancel': False,
        'handlerTimedOut': False, 'providerError': None, 'activeHandlers': 0,
        'entered': threading.Event()}
    provider_lock = threading.Lock()
    cancel_wakeup = threading.Event()
    handler = provider_handler(args.action, provider_state, provider_lock, cancel_wakeup)
    class QuietProvider(handler):
        pass
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), QuietProvider)
    server.daemon_threads = True
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    base_config = prepare_pi(args.pi_root, data_root, home,
        f'http://127.0.0.1:{server.server_port}/v1', strict_environment=True)
    marker = UNKNOWN_MARKER if args.action == 'unknown' else CANCEL_MARKER
    config = {**base_config, 'kind': 'pi', 'identity': 'pi', 'cwd': str(workspace),
              'provider': 'endpoint-test', 'model': 'endpoint-test', 'action': args.action,
              'prompt': marker + ' submit exactly once and preserve the captured lifecycle.',
              'postPrepareDraft': POST_DRAFT}
    gate_receipt_template = None
    release_signal = None
    if args.action == 'unknown':
        gate_receipt_template = owned / 'pi-return-{pid}.jsonl'
        gate_release_template = owned / 'pi-release-{pid}'
        wrapper = Path(__file__).with_name('pi_return_gate_fixture.py').resolve(strict=True)
        config['executable'] = sys.executable
        config['arguments'] = [str(wrapper), '--marker', marker,
            '--receipt', str(gate_receipt_template), '--release-file', str(gate_release_template),
            '--', str(pi_node), str(pi_cli)]
        release_signal = owned / 'consumer-release.signal'
        config['releaseSignal'] = str(release_signal)
    else:
        cancel_signal = owned / 'consumer-cancel.signal'
        config['cancelSignal'] = str(cancel_signal)
    write_new(args.output / 'source-programs.json', {
        'sourceCommit': FROZEN_HEAD,
        'frozenSourceManifest': {'path': str(manifest_path), 'sha256': digest(manifest_path)},
        'probe': {'path': str(args.probe_program), 'sha256': probe_sha},
        'runtime': {'versionExpected': '1.3.0', 'capturedVersionRequired': '1.3.0',
                    'nodePath': str(pi_node), 'nodeSha256': digest(pi_node),
                    'nodeVersion': node_version, 'cliPath': str(pi_cli), 'cliSha256': digest(pi_cli)},
        'runnerInputs': source_hashes, 'action': args.action,
        'providerUrl': 'loopback-only', 'credentialInput': 'owned Pi config with synthetic key',
        'configurationPersisted': False,
    })
    write_new(args.output / 'provider-port.json', {'host': '127.0.0.1', 'port': server.server_port})
    process = subprocess.Popen([str(args.probe_program)], cwd=workspace,
        env=base_config['environment'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, start_new_session=True, bufsize=0)
    pgid = process.pid
    process_snapshots = [{'phase': 'launched', 'processes': process_group_snapshot(pgid)}]
    capture = ProcessCapture(process, args.output)
    status = None
    result = None
    accepted_candidate = None
    error = None
    gate_path, gate_events = None, []
    try:
        assert process.stdin is not None
        process.stdin.write(json.dumps(config).encode() + b'\n')
        process.stdin.close()
        if args.action == 'cancel':
            event = capture.wait_event('run_cancellable', 120)
            process_snapshots.append({'phase': 'cancellable-and-provider-held',
                                      'processes': process_group_snapshot(pgid)})
            provider_state['entered'].wait(40)
            with provider_lock:
                entered = provider_state['entered'].is_set()
                matches = provider_state['matchingBusinessRequests']
                first = provider_state['firstStreamFrameSent']
            if not entered or matches != 1 or not first:
                raise RuntimeError('cancel fixture lacked one held original provider request and first frame')
            write_signal(cancel_signal, b'cancel\n')
            result = capture.wait_event('cancel_settled', 50)
            provider_state['entered'].wait(2)
            status = capture.wait_exit(10)
            receipt = provider_receipt(provider_state, provider_lock, 'cancel')
            validated_after = {**result.get('after', {}), 'ticketId': result.get('ticketId'),
                                'cancelCallCompleted': result.get('cancelCallCompleted')}
            validation = validate_cancel(result.get('before'), validated_after,
                ticket_id=result.get('ticketId'), expected_prompt=config['prompt'],
                expected_draft=POST_DRAFT, provider=receipt)
            if status != 0:
                raise RuntimeError(f'public consumer exited {status} after cancel')
            validate_runtime(result.get('before', {}))
        else:
            event = capture.wait_event('submission_unknown', 90)
            process_snapshots.append({'phase': 'unknown-before-release',
                                      'processes': process_group_snapshot(pgid)})
            gate_path, gate_events = wait_receipt_event(gate_receipt_template, 'mutation_forwarded', 30)
            if sum(row.get('event') == 'mutation_forwarded' for row in gate_events) != 1:
                raise RuntimeError('Pi wrapper did not forward exactly one marked business prompt')
            receipt_pid = gate_path.stem.removeprefix('pi-return-')
            gate_release = Path(str(gate_release_template).replace('{pid}', receipt_pid))
            with provider_lock:
                matches = provider_state['matchingBusinessRequests']
            if matches != 1:
                raise RuntimeError(f'unknown fixture expected exactly one business request, saw {matches}')
            write_signal(gate_release, b'')
            write_signal(release_signal, b'release\n')
            result = capture.wait_event('unknown_reconciled', 45)
            status = capture.wait_exit(10)
            # The wrapper may be terminated by its owned RPC task immediately after
            # forwarding bytes. Its release record proves the gate; process-group
            # observation below is the separate cleanup evidence.
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                try:
                    gate_events = [json.loads(line) for line in gate_path.read_text().splitlines() if line]
                except (OSError, json.JSONDecodeError):
                    pass
                if any(row.get('event') == 'released' for row in gate_events):
                    break
                time.sleep(0.025)
            receipt = provider_receipt(provider_state, provider_lock, 'unknown')
            after_release = {**result.get('afterRelease', {}),
                'secondDispatch': result.get('secondDispatch'),
                'manualAcknowledgement': result.get('manualAcknowledgement')}
            validation = validate_unknown_reconciliation(
                event.get('capture'), after_release, ticket_id=result.get('ticketId'),
                expected_prompt=config['prompt'], expected_draft=POST_DRAFT,
                provider=receipt, gate_events=gate_events)
            if status != 0:
                raise RuntimeError(f'public consumer exited {status} after automatic reconciliation')
            validate_runtime(result.get('afterRelease', {}))
        write_new(args.output / 'provider-observation.json', receipt)
        write_new(args.output / 'lifecycle-validation.json', validation)
        write_new(args.output / 'consumer-summary.json', {
            'exitStatus': status, 'events': [row.get('event') for row in capture.lines],
            'rowCount': len(capture.lines), 'stdoutParseErrors': capture.line_errors})
        accepted_candidate = {
            'accepted': True, 'action': args.action, 'sourceCommit': FROZEN_HEAD,
            'probeProgramSha256': probe_sha, 'piVersion': '1.3.0',
            'nodeVersion': node_version, 'validation': validation,
            'serverTransactionalFreshnessProven': False,
            'desktop': False, 'physicalIME': False, 'accessibility': False,
            }
    except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError,
            KeyError, TypeError) as exc:
        error = f'{type(exc).__name__}: {str(exc)[:1000]}'
    finally:
        cancel_wakeup.set()
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=3)
        handler_deadline = time.monotonic() + 3
        while time.monotonic() < handler_deadline:
            with provider_lock:
                if provider_state['activeHandlers'] == 0:
                    break
            time.sleep(0.025)
        if status is None:
            try:
                status = capture.wait_exit(5)
            except (subprocess.TimeoutExpired, RuntimeError):
                pass
        cleanup = stop_owned_group(process, pgid)
        process_snapshots.append({'phase': 'after-owned-stop',
                                  'processes': process_group_snapshot(pgid)})
        if capture.stdout_thread.is_alive() or capture.stderr_thread.is_alive():
            try:
                status = capture.wait_exit(3)
            except (subprocess.TimeoutExpired, RuntimeError):
                pass
        receipt = provider_receipt(provider_state, provider_lock, args.action)
        try:
            if not (args.output / 'provider-observation.json').exists():
                write_new(args.output / 'provider-observation.json', receipt)
            write_new(args.output / 'owned-cleanup.json', {
                'consumerPid': process.pid, 'ownedProcessGroup': pgid,
                'consumerExitStatus': process.poll(), 'groupSnapshotAfterStop': process_group_snapshot(pgid),
                **cleanup,
                'wrapperReceipt': str(gate_path) if gate_path else None,
                'wrapperReceiptEvents': gate_events,
                'wrapperReceiptMayEndBeforeOwnRpcTaskCleanup': args.action == 'unknown',
                'providerServerStopped': not server_thread.is_alive(),
                'providerHandlersStopped': receipt['handlerThreadsStopped'],
            })
            write_new(args.output / 'owned-process-snapshots.json', process_snapshots)
            if error is None and accepted_candidate is not None:
                if (not cleanup['groupAbsent'] or server_thread.is_alive() or status != 0
                        or not receipt['handlerThreadsStopped']):
                    error = 'owned cleanup or consumer exit did not satisfy acceptance'
                else:
                    write_new(args.output / 'acceptance.json', accepted_candidate)
            if error is not None and not (args.output / 'failure.json').exists():
                write_new(args.output / 'failure.json', {'accepted': False, 'error': error})
        except OSError:
            error = error or 'required evidence could not be persisted'
    return 1 if error else 0


def main(argv=None) -> int:
    args = parse_args(argv)
    try:
        return run(args)
    except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError,
            KeyError, TypeError) as error:
        print(f'{type(error).__name__}: {str(error)[:1000]}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
