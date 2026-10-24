#!/usr/bin/env python3
"""Exercise public image prompts and reconnect against three owned SEPs.

Consumes explicit existing programs only. It does not build, open a desktop,
read private harness history, or use a real model/provider.
"""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import uuid

from dsh_owned_fixture import owned_dsh
from inline_image_live_fixture import (InlineImageProvider, decode_image_with_imageio,
    inspect_user_request, stream_response, validate_public_image_read)
from native_owned_fixture import owned_native
from pi_owned_fixture import prepare_pi
from runtime_provenance import kernel_programs

ROOT = Path(__file__).resolve().parents[1]
SSD = Path('/Volumes/HP P900/dev/TekesAgentKit')
NATIVE_AUTH = 'AGENTKIT_INLINE_NATIVE_BEARER'
DSH_AUTH = 'AGENTKIT_INLINE_DSH_COOKIE'


def write_json(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


class Handler(http.server.BaseHTTPRequestHandler):
    providers = {}

    def log_message(self, *_):
        pass

    def do_POST(self):
        kind = self.path.split('/', 2)[1]
        provider = self.providers.get(kind)
        if provider is None:
            self.send_error(404)
            return
        try:
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            try:
                provider.observe(body)
            except ValueError as error:
                history = body.get('input', body.get('messages', []))
                shapes = [{'role': item.get('role'), 'textPrefix': item.get('content', '')[:100] if isinstance(item.get('content'), str) else None, 'parts': [
                    {'type': part.get('type'), 'keys': sorted(part),
                     'textLength': len(part.get('text', '')) if isinstance(part.get('text'), str) else None}
                    for part in item.get('content', []) if isinstance(part, dict)]}
                    for item in history if isinstance(item, dict) and item.get('role') == 'user']
                with provider.lock:
                    provider.rejections.append({'error': str(error), 'userPartShapes': shapes})
                # Complete the synthetic response so invalid observation cannot
                # provoke harness retry/backoff. Acceptance still fails strictly.
            frames = stream_response(self.path, body)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            for frame in frames:
                self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
                self.wfile.flush()
            if not self.path.endswith('/responses'):
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
        except (ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
            payload = json.dumps({'error': {'message': str(error), 'type': 'invalid_request_error'}}).encode()
            self.send_response(400)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass


def run_headless(program, config, commands, output, env, workspace, label):
    source = dict(config, commands=commands)
    write_json(output / (label + '-commands.json'), commands)
    result = subprocess.run([str(program)], input=json.dumps(source), text=True,
        capture_output=True, env=env, cwd=workspace, timeout=120)
    (output / (label + '.stdout')).write_text(result.stdout, encoding='utf-8')
    (output / (label + '.stderr')).write_text(result.stderr, encoding='utf-8')
    write_json(output / (label + '-exit.json'), {'exitStatus': result.returncode})
    if result.returncode:
        raise RuntimeError(label + ' public headless commands failed; inspect retained logs')
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    version = config['expectedVersion']
    if len(rows) != len(commands) + 1 or rows[0].get('ready') is not True or rows[0].get('version') != version:
        raise ValueError(label + ' runtime version or command count mismatch')
    if any(row.get('op') != command['op'] or 'result' not in row
           for command, row in zip(commands, rows[1:])):
        raise ValueError(label + ' public command/result pairing mismatch')
    return rows


def seed_endpoint(kind, config, provider_id, model_id, program, env, workspace, case):
    ids = [str(uuid.uuid4()), str(uuid.uuid4())] if kind == 'native' else None
    commands = []
    for index in range(2):
        session_id = ids[index] if ids else None
        if kind == 'native':
            commands.extend([
                {'op': 'call', 'method': 'session.create', 'args': {'sessionId': session_id,
                    'execution': {'cwd': str(workspace), 'read_roots': [], 'writable_roots': [],
                    'toolchain_roots': [], 'network': False, 'allowed_tools': [], 'mcp': []}},
                    'saveAs': 'image-' + str(index)},
                {'op': 'call', 'sessionId': 'image-' + str(index), 'method': 'session.selectModel',
                    'args': {'provider': provider_id, 'model': model_id}},
                {'op': 'open', 'sessionId': 'image-' + str(index)},
            ])
        else:
            commands.extend([
                {'op': 'create', 'cwd': str(workspace), 'saveAs': 'image-' + str(index)},
                {'op': 'call', 'sessionId': 'image-' + str(index), 'method': 'session.selectModel',
                    'args': {'provider': provider_id, 'model': model_id}},
                {'op': 'open', 'sessionId': 'image-' + str(index)},
            ])
    rows = run_headless(program, config, commands, case, env, workspace, 'seed')
    actual_ids = []
    for command, row in zip(commands, rows[1:]):
        if command.get('method') == 'session.create' or command.get('op') == 'create':
            session_id = row['result'].get('sessionId')
            if not isinstance(session_id, str) or not session_id:
                raise ValueError(kind + ' public create result omitted its session ID')
            actual_ids.append(session_id)
    if len(actual_ids) != 2 or len(set(actual_ids)) != 2:
        raise ValueError(kind + ' public create results did not return two distinct session IDs')
    if kind == 'native' and set(actual_ids) != set(ids):
        raise ValueError('Native did not preserve caller-supplied public session IDs')
    endpoint = {key: value for key, value in config.items()
                if key not in ('bearer', 'cookie', 'expectedVersion', 'commands')}
    endpoint.update(endpointId=kind, sessions=actual_ids)
    return endpoint, actual_ids


def run_probe(kind, endpoint, ids, image_program, env, workspace, case, provider):
    config = {key: value for key, value in endpoint.items() if not key.startswith('_')}
    if kind == 'native':
        config['bearerEnv'] = NATIVE_AUTH
    if kind == 'dsh':
        config['cookieEnv'] = DSH_AUTH
    config_path = case / 'endpoint.json'
    write_json(config_path, config)
    receipt_path = case / 'image-probe-receipt.json'
    if receipt_path.exists():
        raise ValueError('probe receipt path must be new')
    demo_env = dict(env)
    if kind == 'native':
        demo_env[NATIVE_AUTH] = endpoint['_bearer']
    if kind == 'dsh':
        demo_env[DSH_AUTH] = endpoint['_cookie']
    command = [str(image_program), '--endpoint-config', str(config_path), '--receipt', str(receipt_path)]
    write_json(case / 'probe-command.json', {'argv': command, 'sessionIds': ids,
        'credentialsPassedViaEnvironment': True})
    result = subprocess.run(command, text=True, capture_output=True, env=demo_env,
        cwd=workspace, timeout=180)
    (case / 'probe.stdout').write_text(result.stdout, encoding='utf-8')
    (case / 'probe.stderr').write_text(result.stderr, encoding='utf-8')
    write_json(case / 'probe-exit.json', {'exitStatus': result.returncode})
    if result.returncode:
        raise RuntimeError(kind + ' public RunBinding inline image probe failed; inspect logs')
    if not receipt_path.is_file() or receipt_path.is_symlink():
        raise ValueError(kind + ' image probe did not write its receipt')
    receipt = json.loads(receipt_path.read_text(encoding='utf-8'))
    if receipt.get('status') != 'completed' or receipt.get('endpointKind') != kind:
        raise ValueError(kind + ' probe receipt does not confirm the expected endpoint')
    reconnect = receipt.get('reconnect')
    if (not isinstance(reconnect, dict) or type(reconnect.get('epochBefore')) is not int
            or type(reconnect.get('epochAfter')) is not int
            or reconnect['epochAfter'] <= reconnect['epochBefore']
            or not isinstance(reconnect.get('recovered'), list) or len(reconnect['recovered']) != 2):
        raise ValueError(kind + ' receipt does not prove both original sessions reopened after reconnect')
    round_trips = receipt.get('roundTrips')
    if not isinstance(round_trips, list) or len(round_trips) != 2:
        raise ValueError(kind + ' receipt is missing its two original RunBinding sends')
    for index, row in enumerate(round_trips):
        expected_text = '请描述这张图片。' if index == 0 else ''
        if row.get('input', {}).get('text') != expected_text:
            raise ValueError(kind + ' round trip input text does not match the scenario')
        attachments = row.get('input', {}).get('attachments')
        evidence = row.get('imageEvidence', {})
        if (not isinstance(attachments, list) or len(attachments) != 1
                or attachments[0].get('type') != 'image'
                or evidence.get('rawTranscriptAttachment') != attachments[0]):
            raise ValueError(kind + ' public image source evidence is missing')
        validate_public_image_read(kind, evidence, attachments[0], ONE_PIXEL_PNG, '中文图片.png')
        if row.get('image', {}).get('byteCount') != len(ONE_PIXEL_PNG):
            raise ValueError(kind + ' receipt lost original image byte count')
        scope = row.get('scope', {})
        recovered = reconnect['recovered'][index]
        if (scope.get('sessionId') != ids[index] or scope.get('endpointId') != kind
                or recovered.get('scope') != scope
                or recovered.get('input', {}).get('id') != row['input'].get('id')
                or recovered.get('assistant', {}).get('id') != row['assistant'].get('id')
                or recovered.get('input', {}).get('attachments') != attachments
                or recovered.get('imageEvidence', {}).get('rawTranscriptAttachment') != attachments[0]):
            raise ValueError(kind + ' reconnect did not recover the original public source identities')
        validate_public_image_read(kind, recovered.get('imageEvidence'), attachments[0], ONE_PIXEL_PNG, '中文图片.png')
    observed = provider.receipt()
    for index, request in enumerate(observed['requests']):
        image = provider.images[index]
        decoded = decode_image_with_imageio(image['data'], image['mime'],
            case / 'provider-images', 'prompt-' + str(index + 1))
        request['images'][0].update(decoded)
    normalized_from_original = [
        request['images'][0]['sha256'] != provider.original_sha
        for request in observed['requests']]
    if observed['requestCount'] != 2 or [r['hasText'] for r in observed['requests']] != [True, False]:
        raise ValueError(kind + ' provider did not observe text+image then image-only requests')
    if kind != 'dsh' and any(r['images'][0]['sha256'] != provider.original_sha for r in observed['requests']):
        raise ValueError(kind + ' provider image bytes differ from original input')
    observed['normalizedFromOriginal'] = normalized_from_original
    write_json(case / 'provider-observation.json', observed)
    write_json(case / 'probe-receipt-validated.json', receipt)
    return {'endpoint': kind, 'requestCount': observed['requestCount'],
        'wireRequestCount': observed['wireRequestCount'], 'auxiliaryRequests': observed['auxiliaryRequests'],
        'imageMimes': [r['images'][0]['mime'] for r in observed['requests']],
        'inputSha256': provider.original_sha, 'providerSha256': [r['images'][0]['sha256'] for r in observed['requests']],
        'normalizedFromOriginal': normalized_from_original,
        'reconnectReceipt': receipt}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('headless-program', 'image-program', 'pi-root', 'dsh-root',
                 'kernel-root', 'kernel-supervisor', 'kernel-worker', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.expanduser().absolute()
    if (output.is_symlink() or not output.resolve().is_relative_to(SSD.resolve(strict=True))
            or output.exists()):
        parser.error('--output must be a new directory under the existing AgentKit SSD project')
    program_paths = {key: path.resolve(strict=True) for key, path in (
        ('headless', args.headless_program), ('image', args.image_program))}
    if any(not path.is_file() or not os.access(path, os.X_OK) for path in program_paths.values()):
        parser.error('existing executable regular programs are required')
    pi_root, dsh_root = args.pi_root.resolve(strict=True), args.dsh_root.resolve(strict=True)
    supervisor, worker, kernel_identity = kernel_programs(args.kernel_root,
        supervisor=args.kernel_supervisor, worker=args.kernel_worker, expected_version='0.9.3')
    provider_path = args.kernel_root.resolve(strict=True) / 'fixtures/config/providers.canonical.json'
    providers = json.loads(provider_path.read_text(encoding='utf-8'))
    if not providers.get('providers') or not providers['providers'][0].get('id'):
        parser.error('canonical Native provider snapshot is incomplete')
    output.mkdir(mode=0o700, parents=True)
    programs_dir = output / 'programs'; programs_dir.mkdir()
    identities = {}
    for key, source in program_paths.items():
        source_hash = digest(source)
        archived = programs_dir / ('owned-' + key + '-' + source.name)
        shutil.copy2(source, archived)
        if digest(source) != source_hash or digest(archived) != source_hash:
            raise RuntimeError('executable changed while its identity was recorded')
        identities[key] = {'path': str(source), 'sha256': source_hash,
                           'archiveOnly': str(archived)}
    write_json(output / 'programs.json', identities)
    script_paths = [Path(__file__).resolve(), Path(__file__).with_name('inline_image_live_fixture.py'),
        Path(__file__).with_name('native_owned_fixture.py'), Path(__file__).with_name('pi_owned_fixture.py'),
        Path(__file__).with_name('dsh_owned_fixture.py'), Path(__file__).with_name('runtime_provenance.py')]
    write_json(output / 'source-hashes.json', {p.name: digest(p) for p in script_paths})
    write_json(output / 'runtime-inputs.json', {'piRoot': str(pi_root), 'dshRoot': str(dsh_root),
        'kernel': kernel_identity, 'providerSnapshot': {'path': str(provider_path), 'sha256': digest(provider_path)},
        'privateHistoryRead': False, 'credentialsRecorded': False})
    workspace = output / 'workspace'; workspace.mkdir()
    home = output / 'host-home'; home.mkdir(mode=0o700)
    tmp = output / 'tmp'; tmp.mkdir(mode=0o700)
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'TMPDIR': str(tmp),
           'LANG': 'en_US.UTF-8', 'LC_ALL': 'en_US.UTF-8'}
    providers_by_kind = {kind: InlineImageProvider(dsh=kind == 'dsh')
                         for kind in ('native', 'pi', 'dsh')}
    Handler.providers = providers_by_kind
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    url = 'http://127.0.0.1:' + str(server.server_port)
    results, processes, failure, case_failures = {}, {}, None, {}
    try:
        with ExitStack() as stack:
            native = stack.enter_context(owned_native(supervisor, worker, providers, url + '/native/v1', output, env))
            processes['native'] = native['process']
            dsh_home = output / 'dsh-home'
            dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home),
                DSH_AGENTS_HOME=str(dsh_home / 'agents'), DSH_TELEMETRY_DISABLED='1',
                AGENTKIT_SYNTHETIC_KEY='synthetic-key')
            dsh = stack.enter_context(owned_dsh(dsh_root, output / 'dsh-data', dsh_home,
                workspace, url + '/dsh/v1', dsh_env, query_sqlite=False, image_input=True))
            processes['dsh'] = dsh.process
            if dsh.version != '0.2.1-rc.2':
                raise ValueError('unexpected pinned DSH runtime version')
            pi = prepare_pi(pi_root, output / 'pi-data', output / 'pi-home', url + '/pi/v1', strict_environment=True)
            base_providers = native['providers']['providers'][0]
            cases = [
                ('native', {'kind': 'native', 'url': native['url'], 'bearer': native['bearer'],
                    'expectedVersion': '0.9.3'}, base_providers['id'], base_providers['models'][0]['id']),
                ('pi', dict(pi, expectedVersion='1.3.0'), 'endpoint-test', 'endpoint-test'),
                ('dsh', dict(dsh.connector_config([]), expectedVersion=dsh.version), 'endpoint-test', 'endpoint-test')]
            for kind, config, provider_id, model_id in cases:
                case = output / kind; case.mkdir(mode=0o700)
                local_provider = None
                try:
                    endpoint, ids = seed_endpoint(kind, config, provider_id, model_id,
                        program_paths['headless'], env, workspace, case)
                    if kind == 'native': endpoint['_bearer'] = native['bearer']
                    if kind == 'dsh': endpoint['_cookie'] = dsh.cookie
                    local_provider = providers_by_kind[kind]
                    local_provider.original_sha = hashlib.sha256(ONE_PIXEL_PNG).hexdigest()
                    local_provider.images = []
                    # Wrap observation without persisting request bodies.
                    original_observe = local_provider.observe
                    def observe(body, *, target=local_provider, observe_original=original_observe):
                        index = observe_original(body)
                        if index is None:
                            return None
                        facts = inspect_user_request(body, dsh=target.dsh)
                        target.images.extend({'data': image['_data'], 'mime': image['mime']}
                                             for image in facts['images'])
                        return index
                    local_provider.observe = observe
                    results[kind] = run_probe(kind, endpoint, ids, program_paths['image'], env,
                                              workspace, case, local_provider)
                except (OSError, ValueError, RuntimeError, KeyError, subprocess.SubprocessError) as error:
                    case_failures[kind] = type(error).__name__ + ': ' + str(error)
                    write_json(case / 'failure.json', {'error': case_failures[kind]})
                finally:
                    # Include failed/excess requests in the evidence, even when
                    # no success receipt can be written by the original probe.
                    observed = locals().get('local_provider')
                    if observed is not None:
                        with observed.lock:
                            write_json(case / 'provider-attempts.json', {
                                'attemptCount': observed.attempts,
                                'acceptedRequests': list(observed.requests), 'rejections': list(observed.rejections), 'auxiliaryRequests': list(observed.auxiliary)})
            if case_failures:
                raise RuntimeError('endpoint failures: ' + ', '.join(sorted(case_failures)))
    except BaseException as error:
        failure = type(error).__name__ + ': ' + str(error)
        try:
            write_json(output / 'failure.json', {'error': failure})
        except OSError:
            pass
    finally:
        server.shutdown(); server.server_close(); thread.join(timeout=5)
        exits = {name: process.poll() for name, process in processes.items()}
        write_json(output / 'owned-process-exits.json', exits)
        if 'native' in processes:
            # owned_native has already performed bounded child cleanup on stack exit.
            write_json(output / 'native-supervisor-exit.json', {'exitStatus': exits['native']})
    if failure is not None:
        print('INLINE_IMAGE_LIVE_FAILED; inspect retained owned evidence')
        return 1
    # Services and the HTTP server have stopped. Late work must not be hidden
    # by switching the active endpoint or by an earlier success receipt.
    for kind, provider in providers_by_kind.items():
        terminal = provider.receipt()
        if terminal['wireRequestCount'] != results[kind]['wireRequestCount']:
            raise ValueError(kind + ' produced a late provider request after its probe')
    if set(results) != {'native', 'pi', 'dsh'}:
        raise RuntimeError('not all endpoint probes completed')
    write_json(output / 'run-receipt.json', {'accepted': True, 'endpoints': results,
        'providerPromptCount': sum(value['requestCount'] for value in results.values()),
        'image': {'mime': 'image/png', 'bytes': len(ONE_PIXEL_PNG),
                  'sha256': hashlib.sha256(ONE_PIXEL_PNG).hexdigest()},
        'provider': 'synthetic-loopback', 'desktopInteraction': False,
        'privateHarnessHistoryRead': False, 'paidModel': False,
        'mixedFileImage': 'not_run'})
    print('INLINE_IMAGE_LIVE_ACCEPTED native+pi+dsh; desktop=false')
    return 0


ONE_PIXEL_PNG = bytes([
    137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,2,0,0,0,144,119,83,222,
    0,0,0,12,73,68,65,84,120,156,99,96,96,96,0,0,0,4,0,1,246,23,56,85,0,0,0,0,73,69,78,68,174,66,96,130])
ONE_PIXEL_DATA = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC'


if __name__ == '__main__':
    raise SystemExit(main())
