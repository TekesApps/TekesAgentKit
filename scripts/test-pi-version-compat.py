#!/usr/bin/env python3
"""Pi version upgrade probes through the public headless consumer and local SSE provider.

Uses owned state only. Checks provider-bound tool pairing, persisted recovery edits,
extension loadout after restart, and the model limit used for real compaction.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pi-root', type=Path, required=True)
    parser.add_argument('--headless-program', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expected-version', required=True, choices=['1.3.0', '1.4.0'])
    parser.add_argument('--recovery-only', action='store_true',
                        help='Isolate recovery when an older baseline fails late-extension activation')
    parser.add_argument('--baseline-pi-root', type=Path,
                        help='Create the warmup with 1.3.0, then restore its pointer with the candidate')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    root = args.pi_root.resolve()
    program = args.headless_program.resolve()
    requests = []
    failures = []

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            requests.append(body)
            messages = body.get('messages', [])
            # A strict provider rejects orphan results even when Pi can render the history.
            calls = set()
            for message in messages:
                calls.update(call['id'] for call in message.get('tool_calls', []))
                if message.get('role') == 'tool' and message['tool_call_id'] not in calls:
                    failures.append('orphan tool result: ' + message['tool_call_id'])
            if failures:
                self.send_response(400)
                self.end_headers()
                self.wfile.write(json.dumps({'error': {'message': failures[-1]}}).encode())
                return
            user = next((m.get('content') for m in reversed(messages) if m['role'] == 'user'), '')
            common = {'id': 'compat', 'object': 'chat.completion.chunk', 'created': 1, 'model': 'compat'}
            result = 'COMPAT_REPLY_OK'
            delta = {'role': 'assistant', 'content': result}
            reason = 'stop'
            # The initial warmup is essential: Pi must have a persisted native pointer.
            # Emit a real cut-off tool call and let Pi synthesize its failed result.
            if body.get('tools') and 'TRUNCATED_TOOL_PROBE' in str(user) and not any('cut-call' in json.dumps(m) for m in messages):
                delta = {'role': 'assistant', 'tool_calls': [
                    {'index': 0, 'id': 'cut-call', 'type': 'function',
                     'function': {'name': 'compat_echo', 'arguments': '{"value":"unfinished'}}]}
                reason = 'length'
            elif body.get('tools') and 'EXTENSION_RESTART_PROBE' in str(user) and not any(
                    m.get('role') == 'tool' and m.get('tool_call_id') == 'restart-call' for m in messages):
                names = [tool['function']['name'] for tool in body.get('tools', [])]
                if 'compat_echo' not in names or 'compat_late' not in names:
                    failures.append('extension tool inactive after restore')
                delta = {'role': 'assistant', 'tool_calls': [
                    {'index': 0, 'id': 'restart-call', 'type': 'function',
                     'function': {'name': 'compat_late', 'arguments': '{"value":"EXTENSION_TOOL_OK"}'}}]}
                reason = 'tool_calls'
            # Pi's actual summary request uses a separate system prompt.
            if not body.get('tools'):
                assert str(messages[0].get('content', '')).startswith('You are a context summarization assistant.'), 'unexpected non-agent request'
                delta = {'role': 'assistant', 'content': 'COMPACTION_SUMMARY_OK: preserve compatibility probe.'}
                reason = 'stop'
            frames = [dict(common, choices=[{'index': 0, 'delta': delta, 'finish_reason': None}]),
                      dict(common, choices=[{'index': 0, 'delta': {}, 'finish_reason': reason}],
                           usage={'prompt_tokens': 100, 'completion_tokens': 8, 'total_tokens': 108})]
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            try:
                for frame in frames:
                    self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    data = output / 'data'
    (data / 'agent').mkdir(parents=True)
    (output / 'home').mkdir()
    (output / 'project').mkdir()
    (data / 'agent/models.json').write_text(json.dumps({'providers': {'compat': {
        'baseUrl': f'http://127.0.0.1:{server.server_port}/v1', 'api': 'openai-completions',
        'apiKey': 'synthetic-key', 'models': [{'id': 'compat', 'name': 'Compat',
            'contextWindow': 131072, 'maxTokens': 24576}]}}}))
    # Small keep window forces an actual model summary during recovery.
    (data / 'agent/settings.json').write_text(json.dumps({'compaction': {
        'enabled': True, 'reserveTokens': 16384, 'keepRecentTokens': 1},
        'retry': {'enabled': False}}))
    extension = output / 'extension.mjs'
    source = '''import { Type } from '@earendil-works/pi-ai';
export default function(pi) {
  for (const name of NAMES) pi.registerTool({name, label:name, description:'Owned compatibility echo',
    parameters:Type.Object({value:Type.String()}),
    async execute(id,params) {return {content:[{type:'text',text:params.value}],details:{value:params.value}};}
  });
}
'''
    extension.write_text(source.replace('NAMES', "['compat_echo']"))
    config = {'kind': 'pi', 'executable': str(root / 'node/bin/node'),
              'arguments': [str(root / 'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')],
              'dataRoot': str(data), 'environment': {'HOME': str(output / 'home'),
                  'PATH': os.environ.get('PATH', '/usr/bin:/bin')}, 'extensions': [str(extension)]}

    def run(name, commands, expected_version=None):
        result = subprocess.run([str(program)], input=json.dumps(dict(config, commands=commands)),
                                text=True, capture_output=True, timeout=90)
        (output / (name + '.stdout.jsonl')).write_text(result.stdout)
        (output / (name + '.stderr.log')).write_text(result.stderr)
        (output / 'provider-requests.json').write_text(json.dumps(requests, indent=2))
        assert not failures, failures
        assert result.returncode == 0, result.stderr + result.stdout[-3000:]
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        assert rows[0]['version'] == (expected_version or args.expected_version), rows[0]
        return rows

    try:
        if args.baseline_pi_root:
            baseline = args.baseline_pi_root.resolve()
            config['executable'] = str(baseline / 'node/bin/node')
            config['arguments'] = [str(baseline / 'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')]
        rows = run('warmup', [
            {'op': 'create', 'cwd': str(output / 'project'), 'saveAs': 's'},
            {'op': 'call', 'sessionId': 's', 'method': 'session.selectModel',
             'args': {'provider': 'compat', 'model': 'compat'}},
            {'op': 'open', 'sessionId': 's'},
            {'op': 'compose', 'sessionId': 's', 'text': 'WARMUP'},
            {'op': 'waitSettled', 'sessionId': 's'},
            {'op': 'call', 'sessionId': 's', 'method': 'session.locator'}],
            expected_version='1.3.0' if args.baseline_pi_root else args.expected_version)
        config['executable'] = str(root / 'node/bin/node')
        config['arguments'] = [str(root / 'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')]
        sid = next(row['result']['sessionId'] for row in rows if row.get('op') == 'create')
        locator = rows[-1]['result']
        if not args.recovery_only:
            # Extension installed after the branch first recorded its loadout.
            extension.write_text(source.replace('NAMES', "['compat_echo','compat_late']"))
            restored = run('extension-restart', [
                {'op': 'open', 'sessionId': sid},
                {'op': 'compose', 'sessionId': sid, 'text': 'EXTENSION_RESTART_PROBE'},
                {'op': 'waitSettled', 'sessionId': sid, 'inputText': 'EXTENSION_RESTART_PROBE'},
                {'op': 'snapshot'}, {'op': 'presentation', 'sessionId': sid},
                {'op': 'call', 'sessionId': sid, 'method': 'session.locator'}])
            assert restored[-1]['result']['nativeSessionFile'] == locator['nativeSessionFile']
            assert 'EXTENSION_TOOL_OK' in json.dumps(restored)
        run('truncated-live', [
            {'op': 'open', 'sessionId': sid},
            {'op': 'compose', 'sessionId': sid, 'text': 'TRUNCATED_TOOL_PROBE'},
            {'op': 'waitSettled', 'sessionId': sid, 'inputText': 'TRUNCATED_TOOL_PROBE'},
            {'op': 'snapshot'}, {'op': 'presentation', 'sessionId': sid}])
        native = Path(locator['nativeSessionFile'])
        entries = [json.loads(line) for line in native.read_text().splitlines()]
        cut = next(e for e in entries if e.get('message', {}).get('stopReason') == 'length')
        tool = next(e for e in entries if e.get('message', {}).get('toolCallId') == 'cut-call')
        # A live length/tool stop follows Pi's ordinary tool scheduler. Reproduce
        # upstream #11227 separately: restore the public JSONL at that interrupted
        # boundary (before its next assistant), with the real synthetic result.
        fixture = output / 'interrupted'
        fixture.mkdir()
        stopped = fixture / 'cut.jsonl'
        stopped.write_text(''.join(json.dumps(e) + '\n' for e in entries[:entries.index(tool) + 1]))
        resumed_data = output / 'resumed-data'
        shutil.copytree(data / 'agent', resumed_data / 'agent')
        config['dataRoot'] = str(resumed_data)
        config['sessionRoots'] = [str(fixture)]
        sid = str(uuid.uuid4())
        run('truncated-resume', [
            {'op': 'call', 'method': 'session.adopt', 'args': {
                'sessionId': sid, 'cwd': str(output / 'project'), 'sessionFile': str(stopped)}},
            {'op': 'open', 'sessionId': sid},
            {'op': 'compose', 'sessionId': sid, 'text': 'RESUME_TRUNCATED_BOUNDARY'},
            {'op': 'waitSettled', 'sessionId': sid, 'inputText': 'RESUME_TRUNCATED_BOUNDARY'},
            {'op': 'snapshot'}, {'op': 'presentation', 'sessionId': sid}])
        entries = [json.loads(line) for line in stopped.read_text().splitlines()]
        edits = {e.get('targetId') for e in entries if e.get('type') == 'context_edit' and e.get('replacement') is None}
        assert cut['id'] in edits and tool['id'] in edits, 'recovery did not persist paired omission'
        assert any(e.get('type') == 'compaction' and 'COMPACTION_SUMMARY_OK' in e.get('summary', '') for e in entries)
        summary_requests = [r for r in requests if not r.get('tools')]
        assert summary_requests, 'no real model summary request'
        if args.expected_version == '1.4.0':
            assert all(r.get('max_tokens', r.get('max_completion_tokens')) == 24576 for r in summary_requests), 'summary output limit mismatch'
        restarted = run('recovery-restart', [
            {'op': 'open', 'sessionId': sid},
            {'op': 'compose', 'sessionId': sid, 'text': 'AFTER_RECOVERY_RESTART'},
            {'op': 'waitSettled', 'sessionId': sid, 'inputText': 'AFTER_RECOVERY_RESTART'},
            {'op': 'reconnect'}, {'op': 'open', 'sessionId': sid},
            {'op': 'presentation', 'sessionId': sid},
            {'op': 'call', 'sessionId': sid, 'method': 'session.locator'}])
        assert restarted[-1]['result']['nativeSessionFile'] == str(stopped)
        assert 'COMPAT_REPLY_OK' in json.dumps(restarted)
        receipt = {'version': args.expected_version, 'passed': True,
            'extensionAfterLoadoutRestart': None if args.recovery_only else True,
            'liveTruncatedToolRecovery': True,
            'resumedTruncatedToolRecovery': True,
            'pairedPersistentOmission': True, 'realModelCompaction': True,
            'summaryOutputLimits': [r.get('max_tokens', r.get('max_completion_tokens')) for r in summary_requests],
            'recoveryRestartAndReconnect': True, 'stableNativePointer': True,
            'baselineNativePointerUpgraded': bool(args.baseline_pi_root),
            'providerRequests': len(requests), 'orphanToolResults': failures,
            'headlessSha256': hashlib.sha256(program.read_bytes()).hexdigest(),
            'desktopAcceptance': False}
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(receipt, indent=2))
    except Exception as error:
        (output / 'receipt.json').write_text(json.dumps({
            'version': args.expected_version, 'passed': False,
            'recoveryOnly': args.recovery_only, 'failure': str(error),
            'providerFailures': failures, 'providerRequests': len(requests),
            'desktopAcceptance': False}, indent=2) + '\n')
        raise
    finally:
        (output / 'provider-requests.json').write_text(json.dumps(requests, indent=2))
        server.shutdown()


if __name__ == '__main__':
    main()
