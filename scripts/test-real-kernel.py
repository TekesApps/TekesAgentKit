#!/usr/bin/env python3
"""Real Kernel, isolated HOME/state, synthetic local provider. No account credentials or UI."""
import argparse
import base64
import http.server
import json
import os
from pathlib import Path
import secrets
import select
import struct
import subprocess
import tempfile
import threading
from cargo_program import cargo_program

parser = argparse.ArgumentParser()
parser.add_argument('--kernel-root', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
kernel = args.kernel_root.resolve()
binaries = kernel / 'target/debug'
for name in ('tekes-supervisor', 'tekes-worker', 'tekes-helper', 'tekes-workspace-service'):
    if not (binaries/name).is_file():
        raise SystemExit(f'Missing built Kernel binary: {name}')
client_program, client_identity = cargo_program(root, 'tekes-agent-endpoint-kernel', 'kernel_headless', kind='example')
print('HEADLESS_PROGRAM_RECEIPT ' + json.dumps(client_identity,sort_keys=True),flush=True)
requests = []
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append(body)
        if 'AgentKit cancellation acceptance' in json.dumps(body):
            # Keep a real worker active until the client cancels; do not call a real provider.
            threading.Event().wait(3)
        event = {'type':'response.completed','response':{'id':'agentkit-response','status':'completed',
            'output':[{'type':'message','role':'assistant','phase':'final_answer','status':'completed',
            'content':[{'type':'output_text','text':'AGENTKIT_REAL_KERNEL_OK'}]}],
            'usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}}
        try:
            self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
            self.wfile.write(('data: '+json.dumps(event)+'\n\n').encode())
        except (BrokenPipeError, ConnectionResetError): pass

server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with tempfile.TemporaryDirectory(prefix='agentkit-kernel-') as tmp:
        base = Path(tmp)
        cwd = base/'project'; cwd.mkdir()
        config = json.loads((kernel/'fixtures/config/providers.canonical.json').read_text())
        config['providers'][0]['endpoint'] = f'http://127.0.0.1:{server.server_port}/v1'
        launch = base/'launch.json'
        launch.write_text(json.dumps({'format':1,'bootstrap_version':1,'root':str(base/'state'),
            'worker':str(binaries/'tekes-worker'),'listen':'127.0.0.1:0','providers':config,
            'credential_bindings':{'openai-main':'AGENTKIT_SYNTHETIC_PROVIDER'}}))
        launch.chmod(0o600)
        env = dict(os.environ, HOME=tmp)
        env.pop('TEKES_KERNEL_ENDPOINT_TOKEN',None); env.pop('AGENTKIT_SYNTHETIC_PROVIDER',None)
        process = subprocess.Popen([str(binaries/'tekes-supervisor'),'--built-in',str(launch)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        try:
            token = secrets.token_bytes(32)
            payload = json.dumps({'endpointToken':token.hex(),'credentials':{'openai-main':'synthetic-agentkit-key'}}).encode()
            process.stdin.write(b'TEKES-KERNEL-BOOTSTRAP/1\n'+struct.pack('!I',len(payload))+payload)
            process.stdin.flush()
            if not select.select([process.stdout],[],[],15)[0]: raise AssertionError('Kernel startup timeout')
            line = process.stdout.readline()
            if not line: raise AssertionError('Kernel exited before readiness: '+process.stderr.read(2000).decode())
            ready = json.loads(line)
            assert ready['type']=='ready' and ready['protocolVersion']==4, ready
            connection = {'url':ready['url'],'bearer':base64.urlsafe_b64encode(token).decode().rstrip('='),
                'cwd':str(cwd),'provider':config['providers'][0]['id'],'model':config['providers'][0]['models'][0]['id']}
            run = subprocess.run([str(client_program)],input=json.dumps(connection),
                text=True,capture_output=True,timeout=60)
            print(run.stdout,end='')
            if run.returncode:
                print(run.stderr)
                raise AssertionError('headless client failed')
            assert requests, 'real Kernel did not reach isolated provider'
            assert not list(base.glob('**/*mirror*')), 'client unexpectedly wrote a mirror'
            print('REAL_KERNEL_SYNTHETIC_PROVIDER_OK requests='+str(len(requests)))
        finally:
            process.stdin.close()
            try: process.wait(timeout=15)
            except subprocess.TimeoutExpired: process.terminate(); process.wait(timeout=10)
finally:
    server.shutdown(); server.server_close()
