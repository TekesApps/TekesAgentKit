#!/usr/bin/env python3
"""Run the original demo attachment host operation against three owned SEPs.

Existing programs only. No Cargo, desktop, system picker, paid model, or private
session history access. Rendering is offscreen; a PNG is not a screen capture.
"""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import threading

from cargo_program import cargo_program
from dsh_owned_fixture import owned_dsh
from native_owned_fixture import owned_native
from owned_endpoint_provider import provider_handler
from pi_owned_fixture import prepare_pi
from runtime_provenance import kernel_programs

ROOT = Path(__file__).resolve().parents[1]
SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
NATIVE_AUTH = 'AGENTKIT_UPLOAD_NATIVE_BEARER'
DSH_AUTH = 'AGENTKIT_UPLOAD_DSH_COOKIE'


def write_json(path, value):
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def execute(command, directory, prefix, env, *, input_text=None, timeout=120):
    try:
        result = subprocess.run(command, input=input_text, text=True, capture_output=True,
                                env=env, cwd=directory, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        for suffix, value in [('stdout', error.stdout), ('stderr', error.stderr)]:
            if isinstance(value, bytes):
                value = value.decode('utf-8', errors='replace')
            prefix.with_suffix('.' + suffix).write_text(value or '', encoding='utf-8')
        raise RuntimeError(prefix.name + ' timed out; partial logs retained') from None
    prefix.with_suffix('.stdout').write_text(result.stdout, encoding='utf-8')
    prefix.with_suffix('.stderr').write_text(result.stderr, encoding='utf-8')
    if result.returncode != 0:
        raise RuntimeError(prefix.name + ' exited ' + str(result.returncode) + '; inspect owned logs')
    return result.stdout


def validate_upload_receipt(value, endpoint):
    if endpoint not in ('native', 'pi', 'dsh'):
        raise ValueError('unknown endpoint profile')
    if value.get('offscreen') is not True or value.get('nativeInteraction') is not False:
        raise ValueError('expected an offscreen-only component receipt')
    proof = value.get('uploadProof', {})
    required = ('publicEntry', 'sharedHostSelectionOperation', 'hostCompletionApplied',
                'hostBoundedFileRead', 'receiptValidated', 'originalDraft', 'otherDraftPreserved',
                'oneShotUpload', 'componentSend', 'promptSettled', 'attachmentOnlySend',
                'attachmentOnlyVisible', 'freshJournalAfterReconnect', 'reconnectNoDuplicate')
    if any(proof.get(key) is not True for key in required):
        raise ValueError('missing original attachment operation assertion')
    if (type(proof.get('durableInputCount')) is not int or type(proof.get('attachmentOnlyInputCount')) is not int
            or proof.get('durableInputCount') != 1 or proof.get('attachmentOnlyInputCount') != 1
            or proof.get('systemPickerExercised') is not False
            or proof.get('nativeInteraction') is not False or proof.get('inlineImage') is not False):
        raise ValueError('wrong counts or unsupported acceptance claim')
    readable = endpoint in ('native', 'pi')
    if (proof.get('publicByteRead') is not readable or proof.get('readUnavailable') is not (not readable)
            or proof.get('foreignSessionReadRejected') != (True if readable else None)):
        raise ValueError('capability-specific byte read was overstated')
    return proof


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'ui-demo', 'pi-root', 'dsh-root', 'kernel-root',
                 'kernel-supervisor', 'kernel-worker', 'output'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args()
    output = args.output.absolute()
    if output.exists() or not output.resolve().is_relative_to(SSD.resolve(strict=True)):
        parser.error('output must be a new directory under the existing SSD project')
    output.mkdir(mode=0o700)
    workspace = output / 'workspace'; workspace.mkdir()
    (workspace / 'proof.txt').write_text('owned upload host proof\n')
    home = output / 'host-home'; home.mkdir(mode=0o700)
    tmp = output / 'tmp'; tmp.mkdir(mode=0o700)
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'TMPDIR': str(tmp),
           'LANG': 'en_US.UTF-8', 'LC_ALL': 'en_US.UTF-8'}
    headless, headless_identity = cargo_program(ROOT, 'tekes-agent-headless', 'tekes-agent-headless', existing=args.headless_program)
    demo, demo_identity = cargo_program(ROOT / 'ui', 'tekes-agent-ui-demo', 'tekes-agent-ui-demo', existing=args.ui_demo)
    supervisor, worker, kernel_identity = kernel_programs(args.kernel_root, supervisor=args.kernel_supervisor,
                                                        worker=args.kernel_worker, expected_version='0.9.3')
    write_json(output / 'programs.json', {'headless': headless_identity, 'demo': demo_identity, 'kernel': kernel_identity})
    write_json(output / 'source-hashes.json', {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in (Path(__file__).resolve(), ROOT / 'ui/demo/src/attachment_files.rs',
                     ROOT / 'ui/demo/src/components_native.rs', ROOT / 'ui/demo/src/upload_proof.rs')})
    requests = []
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), provider_handler(workspace=lambda: workspace, requests=requests))
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    processes = {}; receipts = {}; failure = None
    try:
        with ExitStack() as stack:
            url = 'http://127.0.0.1:' + str(server.server_port) + '/v1'
            providers = json.loads((args.kernel_root / 'fixtures/config/providers.canonical.json').read_text())
            native = stack.enter_context(owned_native(supervisor, worker, providers, url, output, env))
            processes['native'] = native['process']
            dsh_home = output / 'dsh-home'
            dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home), DSH_AGENTS_HOME=str(dsh_home / 'agents'),
                           DSH_TELEMETRY_DISABLED='1', AGENTKIT_SYNTHETIC_KEY='synthetic-key')
            dsh = stack.enter_context(owned_dsh(args.dsh_root, output / 'dsh-data', dsh_home,
                                              workspace, url, dsh_env, query_sqlite=False))
            processes['dsh'] = dsh.process
            if dsh.version != '0.2.1-rc.2':
                raise ValueError('unexpected DSH version')
            pi = prepare_pi(args.pi_root, output / 'pi-data', output / 'pi-home', url, strict_environment=True)
            pi['environment'].update({NATIVE_AUTH: '', DSH_AUTH: ''})
            configs = [
                ('native', {'kind': 'native', 'url': native['url'], 'bearer': native['bearer'], 'expectedVersion': '0.9.3'},
                 native['providers']['providers'][0]['id'], native['providers']['providers'][0]['models'][0]['id']),
                ('pi', dict(pi, expectedVersion='1.3.0'), 'endpoint-test', 'endpoint-test'),
                ('dsh', dict(dsh.connector_config([]), expectedVersion='0.2.1-rc.2'), 'endpoint-test', 'endpoint-test')]
            for kind, config, provider, model in configs:
                commands = []
                for index in range(2):
                    alias = 'seed-' + str(index)
                    commands.extend([
                        {'op': 'create', 'cwd': str(workspace), 'saveAs': alias},
                        {'op': 'call', 'sessionId': alias, 'method': 'session.selectModel', 'args': {'provider': provider, 'model': model}},
                        {'op': 'open', 'sessionId': alias},
                        {'op': 'compose', 'sessionId': alias, 'text': 'Owned attachment host seed ' + str(index)},
                        {'op': 'waitSettled', 'sessionId': alias}])
                write_json(output / (kind + '-commands.json'), commands)
                text = execute([str(headless)], workspace, output / (kind + '-seed'), env,
                               input_text=json.dumps(dict(config, commands=commands)))
                rows = [json.loads(line) for line in text.splitlines() if line.strip()]
                if (len(rows) != len(commands) + 1 or rows[0].get('ready') is not True
                        or rows[0].get('version') != config['expectedVersion']):
                    raise ValueError(kind + ' actual runtime handshake or row count mismatch')
                if any(row.get('op') != command['op'] or 'result' not in row for command, row in zip(commands, rows[1:])):
                    raise ValueError(kind + ' public command/result pairing mismatch')
                ids = [row['result']['sessionId'] for row in rows if row.get('op') == 'create']
                if len(ids) != 2 or len(set(ids)) != 2:
                    raise ValueError(kind + ' distinct public session IDs missing')
                endpoint = {key: value for key, value in config.items() if key not in ('bearer', 'cookie', 'expectedVersion', 'commands')}
                endpoint.update(endpointId=kind, sessions=ids)
                ui_env = dict(env)
                if kind == 'native': endpoint['bearerEnv'] = NATIVE_AUTH; ui_env[NATIVE_AUTH] = native['bearer']
                if kind == 'dsh': endpoint['cookieEnv'] = DSH_AUTH; ui_env[DSH_AUTH] = dsh.cookie
                config_path = output / (kind + '-endpoint.json'); write_json(config_path, {'endpoints': [endpoint]})
                text = execute([str(demo), '--components', '--upload-proof', '--endpoint-config', str(config_path),
                                '--offscreen', str(output / (kind + '.png'))], workspace, output / (kind + '-demo'), ui_env)
                values = [json.loads(line) for line in text.splitlines() if line.startswith('{')]
                value = next(value for value in values if value.get('offscreen') is True)
                proof = validate_upload_receipt(value, kind)
                write_json(output / (kind + '-receipt.json'), value)
                receipts[kind] = proof
    except (OSError, ValueError, RuntimeError, KeyError, StopIteration, subprocess.SubprocessError) as error:
        failure = type(error).__name__ + ': ' + str(error)
        (output / 'failure.txt').write_text(failure + '\n')
    finally:
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        exits = {kind: process.poll() for kind, process in processes.items()}
        write_json(output / 'owned-process-exits.json', exits)
        write_json(output / 'provider-requests.json', {'paths': requests, 'requestBodiesPersisted': False})
    if failure is not None or set(receipts) != {'native', 'pi', 'dsh'} or exits != {'native': 0, 'dsh': 0}:
        print('ATTACHMENT_HOST_OPERATION_FAILED; inspect retained owned evidence')
        return 1
    write_json(output / 'run-receipt.json', {'accepted': True, 'endpoints': list(receipts), 'provider': 'synthetic-loopback',
        'providerRequestCount': len(requests), 'sharedHostOperation': True, 'systemPickerExercised': False,
        'desktopInteraction': False, 'privateSessionHistoryRead': False, 'paidModel': False,
        'ownedProcessExitStatus': exits, 'receiptWrittenAfterOwnedProcessesExited': True})
    print('ATTACHMENT_HOST_OPERATION_ACCEPTED native+pi+dsh; system-picker=false; desktop=false')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
