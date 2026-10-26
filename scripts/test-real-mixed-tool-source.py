#!/usr/bin/env python3
"""Run one fixed endpoint's original mixed-tool UI proof, strictly offscreen.

Existing programs only; no Cargo, native window, screen capture or private DB.
This deliberately excludes the legacy all-endpoint runner's unrelated lifecycle
and goal mutation profiles. The original consumer assertions are unchanged.
"""
from __future__ import annotations

import argparse
from contextlib import ExitStack, suppress
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import time
import traceback
import uuid

from dsh_owned_fixture import owned_dsh
from native_owned_fixture import owned_native
from native_dsh_mixed_fixture import classify_dsh_request
from owned_endpoint_provider import provider_handler
from pi_owned_fixture import prepare_pi
from mixed_tool_usage_source import validate as validate_usage_source

SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
AUTH = 'AGENTKIT_MIXED_TOOL_OWNED_CREDENTIAL'
PROOF_KEYS = ('realHarness', 'realReadResults', 'reasoningSource', 'oneContinuousGroup',
              'liveControlsObserved', 'originalControlsRetained', 'syntheticPreeditPreserved',
              'focusPreserved', 'sendBlockedDuringPreedit', 'otherDraftPreserved',
              'reconnectNoDuplicateInput', 'reconnectNoDuplicateTools')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, ensure_ascii=False, indent=2)
        output.write('\n')


def validate_proof(receipt):
    proof = receipt.get('mixedToolProof', {})
    if (receipt.get('offscreen') is not True or proof.get('canonicalTools') != 3
            or any(proof.get(key) is not True for key in PROOF_KEYS)
            or any(proof.get(key) is not False for key in
                   ('nativeInteraction', 'physicalIme', 'readingAnchorAcceptance'))):
        raise ValueError('original offscreen consumer mixed-tool proof is incomplete')
    return proof


def verify_source(manifest_path):
    manifest = json.loads(manifest_path.read_text())
    source = Path(manifest['source'])
    for name, expected in manifest['files'].items():
        if digest(source / name) != expected:
            raise ValueError('frozen source input changed: ' + name)
    return {'head': manifest['head'], 'manifestSha256': digest(manifest_path),
            'verifiedFiles': len(manifest['files'])}


def verify_program(path, receipt_path, source):
    receipt = json.loads(receipt_path.read_text())
    if (receipt['sourceCommit'] != source['head']
            or receipt['sourceManifestSha256'] != source['manifestSha256']
            or Path(receipt['path']).resolve(strict=True) != path
            or receipt['sha256'] != digest(path)):
        raise ValueError('program does not match its frozen-source build receipt')
    return {'receiptSha256': digest(receipt_path), 'path': str(path), 'sha256': digest(path)}


def group_snapshot(pid):
    # macOS may return EPERM from killpg(..., 0) for a just-retired Rust
    # process. Read the authoritative process table rather than interpreting
    # that denial as either presence or absence. A failed ps fails closed.
    table = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid=,pgid='], check=True,
                           capture_output=True, text=True, timeout=5)
    rows = [tuple(map(int, line.split())) for line in table.stdout.splitlines() if line.strip()]
    return [{'pid': row[0], 'ppid': row[1], 'pgid': row[2]} for row in rows if row[2] == pid]


def group_exists(pid):
    return bool(group_snapshot(pid))


def run_owned(command, *, out, label, env, cwd, input_text=None):
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, env=env, cwd=cwd, start_new_session=True)
    stdout, stderr, failure = '', '', None
    try:
        stdout, stderr = process.communicate(input=input_text, timeout=90)
        (out / (label + '.stdout')).write_text(stdout)
        (out / (label + '.stderr')).write_text(stderr)
    except subprocess.TimeoutExpired:
        failure = 'owned ' + label + ' deadline'
    finally:
        deadline = time.monotonic() + (10 if failure is None else 0)
        while group_exists(process.pid) and time.monotonic() < deadline:
            time.sleep(0.05)
        if group_exists(process.pid):
            failure = failure or 'owned ' + label + ' process group did not retire normally'
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            deadline = time.monotonic() + 5
            while group_exists(process.pid) and time.monotonic() < deadline:
                time.sleep(0.05)
            if group_exists(process.pid):
                with suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
            if process.poll() is None:
                stdout, stderr = process.communicate(timeout=5)
        (out / (label + '.stdout')).write_text(stdout)
        (out / (label + '.stderr')).write_text(stderr)
        write(out / (label + '-exit.json'), {'pid': process.pid, 'exitStatus': process.returncode,
            'ownedGroupAbsent': not group_exists(process.pid), 'error': failure,
            'processGroupSnapshot': group_snapshot(process.pid), 'inspection': 'ps pid/ppid/pgid'})
    if failure or process.returncode != 0 or group_exists(process.pid):
        raise ValueError(failure or 'owned ' + label + ' failed')
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', choices=('native', 'pi', 'dsh'), required=True)
    for name in ('headless-program', 'ui-demo', 'source-manifest', 'output',
                 'headless-build-receipt', 'ui-build-receipt'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--headless-source-manifest', type=Path,
                        help='explicit separate immutable source for the already-built headless seeder')
    parser.add_argument('--usage-source-proof', action='store_true',
                        help='validate opt-in original usage witness; absent live samples remain not_observed')
    for name in ('pi-root', 'dsh-root', 'kernel-supervisor', 'kernel-worker', 'providers-config'):
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    out = args.output.absolute()
    if out.exists() or not out.resolve().is_relative_to(SSD.resolve(strict=True)):
        parser.error('output must be a new directory under the existing SSD project')
    required = {'native': ('kernel_supervisor', 'kernel_worker', 'providers_config'),
                'pi': ('pi_root',), 'dsh': ('dsh_root',)}[args.endpoint]
    if any(getattr(args, name) is None for name in required):
        parser.error('missing explicit fixed runtime input for ' + args.endpoint)
    programs = {key: path.resolve(strict=True) for key, path in
                (('headless', args.headless_program), ('demo', args.ui_demo))}
    if any(not p.is_file() or not os.access(p, os.X_OK) for p in programs.values()):
        parser.error('explicit existing executable programs required')
    source_before = verify_source(args.source_manifest)
    headless_source_before = verify_source(args.headless_source_manifest or args.source_manifest)
    program_bindings = {
        'headless': verify_program(programs['headless'], args.headless_build_receipt, headless_source_before),
        'demo': verify_program(programs['demo'], args.ui_build_receipt, source_before)}
    paths = list(programs.values())
    if args.endpoint == 'native':
        paths += [args.kernel_supervisor, args.kernel_worker, args.providers_config]
    else:
        root = args.pi_root if args.endpoint == 'pi' else args.dsh_root
        package = ('node_modules/@earendil-works/pi-coding-agent' if args.endpoint == 'pi'
                   else 'node_modules/@deepseek-ai/dsh')
        paths += [root / 'node/bin/node', root / package / 'package.json']
        paths += [root / package / ('dist/bundle/cli.js' if args.endpoint == 'pi' else 'lib/bin.js')]
    before = {str(p.resolve(strict=True)): digest(p) for p in paths}
    out.mkdir(mode=0o700)
    write(out / 'inputs-before.json', {'source': source_before, 'programsAndRuntime': before,
                                     'headlessSource': headless_source_before,
                                     'programBindings': program_bindings})
    tools = Path(__file__).resolve().parent
    write(out / 'runner-inputs.json', {p.name: digest(p) for p in
        [Path(__file__), *(tools / name for name in ('owned_endpoint_provider.py',
         'mixed_tool_provider.py', 'native_probe_provider.py', 'native_owned_fixture.py',
         'pi_owned_fixture.py', 'dsh_owned_fixture.py', 'mixed_tool_usage_source.py'))]})
    workspace = out / 'workspace'; workspace.mkdir()
    (workspace / 'proof.txt').write_text('proof line one\nline two\n')
    home = out / 'host-home'; home.mkdir(mode=0o700)
    tmp = out / 'tmp'; tmp.mkdir()
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home),
           'TMPDIR': str(tmp), 'LANG': 'en_US.UTF-8'}
    requests, errors, title_requests = [], [], []
    active = 0
    lock = threading.Lock()
    base_handler = provider_handler(workspace=lambda: workspace, requests=requests, mixed_tools=True)

    class Provider(base_handler):
        def setup(self):
            nonlocal active
            super().setup()
            with lock:
                active += 1

        def finish(self):
            nonlocal active
            try:
                super().finish()
            finally:
                with lock:
                    active -= 1

        def do_POST(self):
            try:
                if args.endpoint != 'dsh':
                    return super().do_POST()
                size = int(self.headers.get('Content-Length', '0'))
                if not 0 < size < 8 * 1024 * 1024:
                    raise ValueError('owned DSH request outside byte bound')
                raw = self.rfile.read(size)
                kind, title_text = classify_dsh_request(json.loads(raw))
                if kind == 'title':
                    if title_text != 'ENDPOINT_MIXED_TOOL_RUN_PROOF':
                        raise ValueError('title request does not refer to the original input')
                    requests.append(self.path)
                    title_requests.append({'path': self.path, 'bodySha256': hashlib.sha256(raw).hexdigest()})
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/event-stream')
                    self.end_headers()
                    for delta, finish in (({'role': 'assistant', 'content': 'Owned tool proof'}, None),
                                          ({}, 'stop')):
                        frame = {'id': 'owned-title', 'object': 'chat.completion.chunk', 'created': 1,
                            'model': 'endpoint-test', 'choices': [{'index': 0, 'delta': delta,
                                                                 'finish_reason': finish}]}
                        self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
                    self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
                    return
                original = self.rfile
                try:
                    self.rfile = io.BytesIO(raw)
                    return super().do_POST()
                finally:
                    self.rfile = original
            except Exception as error:
                errors.append(type(error).__name__)
                raise

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    server.daemon_threads = False
    server.block_on_close = True
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    url = f'http://127.0.0.1:{server.server_port}/v1'
    owned_process = None
    result = None
    failure = None
    try:
        with ExitStack() as stack:
            kind = args.endpoint
            provider, model = 'endpoint-test', 'endpoint-test'
            expected_version = {'native': '0.9.3', 'pi': '1.3.0', 'dsh': '0.2.1-rc.2'}[kind]
            if kind == 'native':
                providers = json.loads(args.providers_config.read_text())
                fixture = stack.enter_context(owned_native(args.kernel_supervisor, args.kernel_worker,
                    providers, url, out, env))
                owned_process = fixture['process']
                config = {'kind': kind, 'endpointId': kind, 'url': fixture['url'],
                          'bearer': fixture['bearer']}
                provider = providers['providers'][0]['id']
                model = providers['providers'][0]['models'][0]['id']
            elif kind == 'pi':
                config = prepare_pi(args.pi_root, out / 'data', out / 'pi-home', url,
                                    reasoning=True, strict_environment=True)
                config.update(endpointId=kind)
            else:
                dsh_home = out / 'dsh-home'
                fixture = stack.enter_context(owned_dsh(args.dsh_root, out / 'data', dsh_home,
                    workspace, url, dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home),
                        DSH_AGENTS_HOME=str(dsh_home / 'agents'), DSH_TELEMETRY_DISABLED='1',
                        AGENTKIT_SYNTHETIC_KEY='synthetic-key'), query_sqlite=False))
                owned_process = fixture.process
                if fixture.version != expected_version:
                    raise ValueError('unexpected fixed DSH runtime version')
                config = dict(fixture.connector_config([]), endpointId=kind)
            first = str(uuid.uuid4())
            create = ({'op': 'call', 'method': 'session.create', 'args': {'sessionId': first,
                       'execution': {'cwd': str(workspace), 'read_roots': [str(workspace)],
                        'writable_roots': [], 'toolchain_roots': [], 'network': False,
                        'allowed_tools': ['read'], 'mcp': []}}} if kind == 'native'
                      else {'op': 'create', 'cwd': str(workspace), 'saveAs': 'first'})
            commands = [create, {'op': 'create', 'cwd': str(workspace), 'saveAs': 'other'}]
            if kind != 'native': first = 'first'
            commands.append({'op': 'call', 'sessionId': first, 'method': 'session.selectModel',
                             'args': {'provider': provider, 'model': model}})
            seed = run_owned([str(programs['headless'])],
                input_text=json.dumps(dict(config, commands=commands)), out=out, label='seed',
                env=env, cwd=workspace)
            rows = [json.loads(line) for line in seed.stdout.splitlines() if line.strip()]
            if rows[0].get('version') != expected_version:
                raise ValueError('unexpected fixed runtime handshake version')
            created = [row['result']['sessionId'] for row in rows if row.get('op') == 'create']
            sessions = [first, created[0]] if kind == 'native' else created
            endpoint = {key: value for key, value in config.items() if key not in
                        ('bearer', 'cookie', 'commands', 'expectedVersion')}
            for key in ('bearer', 'cookie'):
                if key in config:
                    env[AUTH] = config[key]
                    endpoint[key + 'Env'] = AUTH
            endpoint['sessions'] = sessions
            write(out / 'endpoints.json', {'endpoints': [endpoint]})
            command = [str(programs['demo']), '--components', '--transcript-content',
                '--mixed-tool-proof', '--endpoint-config', str(out / 'endpoints.json'),
                '--offscreen', str(out / 'pane.png')]
            write(out / 'demo-command.json', command)
            demo = run_owned(command, out=out, label='demo', env=env, cwd=workspace)
            receipt = next(json.loads(line) for line in demo.stdout.splitlines()
                           if line.startswith('{') and json.loads(line).get('offscreen'))
            proof = validate_proof(receipt)
            write(out / 'host-receipt.json', receipt)
            usage = None
            if args.usage_source_proof:
                usage = validate_usage_source(proof.get('usageSourceWitness'))
                write(out / 'usage-validation.json', usage)
            result = {'accepted': True, 'endpoint': kind, 'runtimeVersion': expected_version,
                      'proof': proof, 'seedExit': seed.returncode, 'demoExit': demo.returncode,
                      'source': source_before, 'headlessSource': headless_source_before,
                      'usageSourceValidation': usage, 'programsAndRuntime': before}
    except Exception as error:
        failure = type(error).__name__ + ': ' + str(error)
        (out / 'failure-traceback.log').write_text(traceback.format_exc())
    finally:
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        write(out / 'provider-observation.json', {'paths': requests, 'errors': errors,
            'requestBodiesPersisted': False, 'providerServerThreadStopped': not thread.is_alive(),
            'activeHandlers': active, 'handlerThreadsJoined': True, 'titleRequests': title_requests})
        status = owned_process.poll() if owned_process is not None else None
        write(out / 'owned-exit.json', {'pid': owned_process.pid if owned_process else None,
                                       'exitStatus': status})
    if errors or active or thread.is_alive() or (owned_process is not None and status != 0):
        failure = failure or 'owned provider/harness cleanup failed'
    if len(requests) - len(title_requests) != 3 or len(title_requests) != (1 if args.endpoint == 'dsh' else 0):
        failure = failure or 'unexpected business/title provider request count'
    after = {path: digest(Path(path)) for path in before}
    if (after != before or verify_source(args.source_manifest) != source_before
            or verify_source(args.headless_source_manifest or args.source_manifest) != headless_source_before):
        failure = failure or 'frozen source/program/runtime changed during run'
    write(out / 'inputs-after.json', after)
    if failure:
        write(out / 'incomplete.json', {'accepted': False, 'error': failure})
        print('MIXED_TOOL_SOURCE_FAILED ' + args.endpoint)
        return 1
    result.update(providerRequests=len(requests), ownedHarnessExit=status,
        paidModel=False, privateHarnessHistoryRead=False, nativeWindow=False,
        physicalIme=False, performanceAccepted=False, usageAccepted=False)
    write(out / 'run-receipt.json', result)
    print('MIXED_TOOL_SOURCE_ACCEPTED ' + args.endpoint)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
