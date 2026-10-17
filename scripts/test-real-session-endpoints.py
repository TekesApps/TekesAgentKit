#!/usr/bin/env python3
"""Exercise the three separate endpoints with owned temporary state and synthetic local providers."""
import argparse, base64, shutil, uuid, http.server, json, os, re, select, signal, struct, subprocess, tempfile, threading, time, urllib.request, urllib.parse
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--pi-root',type=Path)
parser.add_argument('--dsh-root',type=Path)
parser.add_argument('--kernel-root',type=Path)
args=parser.parse_args()
if not any((args.pi_root,args.dsh_root,args.kernel_root)):parser.error("supply at least one runtime root")
subprocess.run(['cargo','build','--locked','-p','tekes-agent-headless'],cwd=ROOT,check=True)
requests=[]
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(self.path)
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        if self.path.endswith('/responses'):
            frames=[{'type':'response.completed','response':{'id':'synthetic-response','status':'completed','output':[{'type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':'ENDPOINT_REPLY_OK'}]}],'usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}}]
        else:
            common={'id':'synthetic-chat','object':'chat.completion.chunk','created':1,'model':'endpoint-test'}
            frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','content':'ENDPOINT_'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{'content':'REPLY_OK'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'stop'}],usage={'prompt_tokens':10,'completion_tokens':4,'total_tokens':14})]
            transcript=json.dumps(body.get('messages',[]))
            change=re.search(r'ENDPOINT_CHANGE_DIRECTORY=([^\s"\\]+)',transcript)
            if change and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'directory-call','type':'function','function':{'name':'working_directory','arguments':json.dumps({'cd':change.group(1)})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
        try:
            for frame in frames:self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush()
            if not self.path.endswith('/responses'):self.wfile.write(b'data: [DONE]\n\n')
        except (BrokenPipeError,ConnectionResetError):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
threading.Thread(target=server.serve_forever,daemon=True).start()
base_url=f'http://127.0.0.1:{server.server_port}/v1'
def commands(cwd,provider,model):
    return [
        {'op':'list'}, {'op':'create','cwd':str(cwd),'saveAs':'s'},
        {'op':'call','sessionId':'s','method':'session.selectModel','args':{'provider':provider,'model':model}},
        {'op':'open','sessionId':'s'},
        {'op':'call','sessionId':'s','method':'session.prompt','rpcId':'endpoint-prompt-1','args':{'mode':'queue','content':[{'type':'text','text':'Reply to the endpoint acceptance test.'}]}},
        {'op':'waitSettled','sessionId':'s'},
        {'op':'call','sessionId':'s','method':'session.prompt','rpcId':'endpoint-prompt-1','args':{'mode':'queue','content':[{'type':'text','text':'Reply to the endpoint acceptance test.'}]}},
        {'op':'call','sessionId':'s','method':'session.locator'},
        {'op':'call','sessionId':'s','method':'session.files.stat','args':{'path':'proof.txt'}},
        {'op':'call','sessionId':'s','method':'session.files.read','args':{'path':'proof.txt','offset':1,'limit':2}},
        {'op':'call','sessionId':'s','method':'session.files.readBytes','args':{'path':'proof.txt','offset':0,'limit':5}},
        {'op':'reconnect'},{'op':'open','sessionId':'s'},{'op':'snapshot'},
    ]
def execute(config):
    result=subprocess.run([str(ROOT/'target/debug/tekes-agent-headless')],input=json.dumps(config),text=True,capture_output=True,timeout=120)
    if result.returncode:
        print(result.stdout[-3000:]);print(result.stderr[-3000:]);raise AssertionError(config['kind']+' endpoint failed')
    outputs=[json.loads(line) for line in result.stdout.splitlines()]
    if 'expectedVersion' in config:assert outputs[0]['version']==config['expectedVersion'],outputs[0]
    return outputs
def run(config):
    outputs=execute(config)
    snapshot=next(row['result'] for row in reversed(outputs) if row.get('op')=='snapshot')
    serialized=json.dumps(snapshot)
    assert 'ENDPOINT_REPLY_OK' in serialized, config['kind']+' lost assistant result'
    stat=[row['result'] for row in outputs if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'absolutePath' in row['result']]
    assert len(stat)==3, 'missing session file responses'
    assert stat[1]['text'].startswith('proof line one'),stat[1]
    assert base64.b64decode(stat[2]['data'])==b'proof',stat[2]
    for state in snapshot['sessions'].values():
        if state.get('native_conversation'):
            users=[e for e in state['native_conversation']['entries'] if e.get('message',{}).get('role')=='user']
        else:users=[e for e in state['entries'].values() if e['event']['type']=='user/message' and (e['event']['data'].get('source') or {}).get('kind') in (None,'user')]
        if users:assert len(users)==1,'duplicate accepted input: '+json.dumps(users)[-3500:]
    print(config['kind'].upper()+'_SESSION_ENDPOINT_LIVE_OK history+prompt+idempotency+locator+files+reconnect')
    return outputs
try:
    with tempfile.TemporaryDirectory(prefix='agentkit-three-endpoints-') as tmp:
        base=Path(tmp);cwd=base/'project';cwd.mkdir();(cwd/'proof.txt').write_text('proof line one\nline two\n')
        if args.pi_root:
            pi=args.pi_root.resolve();data=base/'pi';(data/'agent').mkdir(parents=True)
            (data/'agent/models.json').write_text(json.dumps({'providers':{'endpoint-test':{'baseUrl':base_url,'api':'openai-completions','apiKey':'synthetic-key','models':[{'id':'endpoint-test','name':'Endpoint test','contextWindow':32768,'maxTokens':1024}]}}}))
            config={'kind':'pi','executable':str(pi/'node/bin/node'),'arguments':[str(pi/'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')],'dataRoot':str(data),'environment':{'HOME':str(base/'pi-home'),'PATH':os.environ.get('PATH','/usr/bin:/bin')},'commands':commands(cwd,'endpoint-test','endpoint-test')}
            Path(config['environment']['HOME']).mkdir()
            extension=base/'question.mjs'
            extension.write_text("export default function(pi){pi.registerCommand('endpoint-question',{handler:async(args,ctx)=>{const value=await ctx.ui.select('Endpoint question',['yes','no']);pi.appendEntry('endpoint-answer',{value});}});}")
            config['extensions']=[str(extension)]
            config['commands'].insert(-3,{'op':'promptAndAnswer','sessionId':'s','text':'/endpoint-question','outcome':{'value':'yes'}})
            outputs=run(config)
            assert 'endpoint-answer' in json.dumps(outputs) and 'yes' in json.dumps(outputs),'native question result missing'
            print('PI_NATIVE_ACTIONABLE_LIVE_OK pending prompt + answer + persisted native entry')
            # Resume a native session from an explicitly authorized host root, with a new pointer registry.
            locator=next(row['result'] for row in outputs if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'nativeSessionFile' in row['result'])
            adopted=base/'adopted';adopted.mkdir();source=adopted/'existing.jsonl';shutil.copyfile(locator['nativeSessionFile'],source)
            imported_id=str(uuid.uuid4());resumed=dict(config,dataRoot=str(base/'resumed-pi'),sessionRoots=[str(adopted)])
            resumed['commands']=[{'op':'call','method':'session.adopt','args':{'sessionId':imported_id,'cwd':str(cwd),'sessionFile':str(source)}},{'op':'open','sessionId':imported_id},{'op':'snapshot'}]
            result=subprocess.run([str(ROOT/'target/debug/tekes-agent-headless')],input=json.dumps(resumed),text=True,capture_output=True,timeout=40)
            assert result.returncode==0,result.stderr
            assert 'ENDPOINT_REPLY_OK' in result.stdout and 'endpoint-answer' in result.stdout,'resumed native history missing'
            print('PI_NATIVE_POINTER_RESUME_LIVE_OK explicit root + native RPC history')
        if args.dsh_root:
            dsh=args.dsh_root.resolve();node=dsh/'node/bin/node';entry=dsh/'node_modules/@deepseek-ai/dsh/lib/bin.js';home=base/'dsh';home.mkdir()
            # This read-only host plugin exposes official runtime version evidence, like the ClientKit host adapter.
            plugin=base/'version-probe.mjs'
            package=dsh/'node_modules/@deepseek-ai/dsh-app-boot/lib/index.js'
            plugin.write_text("import { getDshRuntimeVersion } from "+json.dumps(package.as_uri())+";\nexport const name='agentkit-version-probe';export const inject=['webServer'];export function apply(ctx){ctx.effect(()=>ctx.webServer.register({kind:'exact',path:'/api/tekes/harness-version',handler(req,res){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({product:'@deepseek-ai/dsh',version:getDshRuntimeVersion(),source:'@deepseek-ai/dsh-app-boot#getDshRuntimeVersion'}));}}));}\n")
            patch=base/'dsh.patch.yml';patch.write_text(json.dumps([
                {'id':'llm-deepseek','disabled':True},
                {'id':'agent-default-model','config':{'provider':'endpoint-test','model':'endpoint-test'}},
                {'id':'llm-pi-ai','config':{'providers':{'endpoint-test':{'api':'openai-completions','apiKeyEnv':'AGENTKIT_SYNTHETIC_KEY','baseURL':base_url,'models':[{'id':'endpoint-test','contextWindow':32768,'maxTokens':1024}]}}}},
                {'id':'session-persistence-jsonl','config':{'root':str(base/'dsh-sessions'),'compression':'none'}},
                {'insert':[{'name':plugin.as_uri()}]},
            ]))
            env=dict(os.environ,HOME=str(home),DSH_HOME=str(home),DSH_AGENTS_HOME=str(home/'agents'),DSH_TELEMETRY_DISABLED='1',AGENTKIT_SYNTHETIC_KEY='synthetic-key')
            process=subprocess.Popen([str(node),str(entry),'--profile','web','--patch',str(patch),'--no-open','--host','127.0.0.1','--port','0'],cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,start_new_session=True)
            try:
                lines=[];launch=None;deadline=time.monotonic()+60
                while time.monotonic()<deadline:
                    if not select.select([process.stdout],[],[],1)[0]:
                        if process.poll() is not None:break
                        continue
                    line=process.stdout.readline();lines.append(line)
                    match=re.search(r'dsh web: (http[^\s]+)',line)
                    if match:launch=match.group(1);break
                if not launch:raise AssertionError('DSH did not become ready: '+''.join(lines)[-5000:])
                # Do not follow redirects when exchanging the per-launch token for an authenticated cookie.
                class NoRedirect(urllib.request.HTTPRedirectHandler):
                    def redirect_request(self,*_):return None
                opener=urllib.request.build_opener(NoRedirect)
                try:response=opener.open(launch)
                except urllib.error.HTTPError as error:response=error
                cookie=response.headers.get('Set-Cookie','').split(';',1)[0];assert cookie,'DSH launch cookie missing'
                parsed=urllib.parse.urlsplit(launch);url=urllib.parse.urlunsplit((parsed.scheme,parsed.netloc,'/','',''))
                version=json.loads((dsh/'node_modules/@deepseek-ai/dsh/package.json').read_text())['version']
                config={'kind':'dsh','url':url,'cookie':cookie,'expectedVersion':version,'commands':commands(cwd,'endpoint-test','endpoint-test')}
                outputs=run(config)
                session_id=next(row['result']['sessionId'] for row in outputs if row.get('op')=='create')
                # Public lifecycle calls, with the original session and its fork independently addressable.
                lifecycle=[{'op':'call','sessionId':session_id,'method':'session.rename','args':{'title':'Endpoint lifecycle proof'}},
                    {'op':'call','sessionId':session_id,'method':'session.fork','args':{'allowMigration':False},'saveAs':'fork'},
                    {'op':'open','sessionId':'fork'},{'op':'snapshot'},
                    {'op':'close','sessionId':'fork'},
                    {'op':'call','sessionId':'fork','method':'session.cancel'},
                    {'op':'call','sessionId':'fork','method':'session.archive'},
                    {'op':'call','sessionId':'fork','method':'session.unarchive'},
                    {'op':'open','sessionId':'fork'},{'op':'snapshot'}]
                result=execute(dict(config,commands=lifecycle))
                assert not any('queueCleanupWarning' in json.dumps(row) for row in result),'fork inbox cleanup failed'
                fork_id=next(row['result']['sessionId'] for row in result if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'sessionId' in row['result'] and row['result']['sessionId']!=session_id)
                assert 'ENDPOINT_REPLY_OK' in json.dumps(result[-1]['result']['sessions'][fork_id]),'fork history missing'
                print('DSH_LIFECYCLE_LIVE_OK version='+version+' rename+fork+cancel+archive+unarchive')
                # Build enough history to force an initial clipped window, then load its frozen previous page.
                history=[{'op':'create','cwd':str(cwd),'saveAs':'h'},{'op':'call','sessionId':'h','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},{'op':'open','sessionId':'h'}]
                for turn in range(1,29):
                    history += [{'op':'call','sessionId':'h','method':'session.prompt','rpcId':'page-'+str(turn),'args':{'mode':'queue','content':[{'type':'text','text':'History proof '+str(turn)}]}},{'op':'waitSettled','sessionId':'h','minimumTurn':turn}]
                history += [{'op':'snapshot'}]
                result=execute(dict(config,commands=history))
                history_id=next(row['result']['sessionId'] for row in result if row.get('op')=='create')
                # A fresh client has no retained early coverage from the live stream.
                result=execute(dict(config,commands=[{'op':'open','sessionId':history_id},{'op':'snapshot'},{'op':'earlier','sessionId':history_id},{'op':'snapshot'}]))
                snapshots=[row['result'] for row in result if row.get('op')=='snapshot']
                before,after=[snapshot['sessions'][history_id] for snapshot in snapshots]
                assert before['has_more_before'] and len(after['entries'])>len(before['entries']),'history page did not extend window'
                assert after['history_through_sequence']==before['history_through_sequence'],'history cut moved during paging'
                print('DSH_FROZEN_HISTORY_LIVE_OK version='+version)
                if version=='0.2.1-rc.2':
                    child=cwd/'child';child.mkdir()
                    directory=[{'op':'create','cwd':str(cwd),'saveAs':'directory'},{'op':'call','sessionId':'directory','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},{'op':'open','sessionId':'directory'},
                        {'op':'call','sessionId':'directory','method':'session.prompt','rpcId':'directory-1','args':{'mode':'queue','content':[{'type':'text','text':'ENDPOINT_CHANGE_DIRECTORY='+str(child)}]}},
                        {'op':'waitSettled','sessionId':'directory'},{'op':'call','sessionId':'directory','method':'session.locator'},{'op':'reconnect'},{'op':'snapshot'}]
                    result=execute(dict(config,commands=directory))
                    locator=next(row['result'] for row in result if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'historySource' in row['result'])
                    assert Path(locator['cwd']).resolve()==cwd.resolve() and Path(locator['workingDirectory']).resolve()==child.resolve(),locator
                    assert 'working-directory/change' in json.dumps(result[-1]),'new required event lost'
                    print('DSH_WORKING_DIRECTORY_LIVE_OK original root + current directory + reconnect')
            finally:
                if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=15)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
        if args.kernel_root:
            kernel=args.kernel_root.resolve();home=base/'kernel-home';home.mkdir()
            providers=json.loads((kernel/'fixtures/config/providers.canonical.json').read_text());providers['providers'][0]['endpoint']=base_url
            launch=base/'kernel-launch.json';launch.write_text(json.dumps({'format':1,'bootstrap_version':1,'root':str(base/'kernel'),'worker':str(kernel/'target/debug/tekes-worker'),'listen':'127.0.0.1:0','providers':providers,'credential_bindings':{'openai-main':'AGENTKIT_SYNTHETIC_KEY'}}))
            env=dict(os.environ,HOME=str(home));env.pop('TEKES_KERNEL_ENDPOINT_TOKEN',None);env.pop('AGENTKIT_SYNTHETIC_KEY',None)
            process=subprocess.Popen([str(kernel/'target/debug/tekes-supervisor'),'--built-in',str(launch)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
            try:
                token=os.urandom(32);payload=json.dumps({'endpointToken':token.hex(),'credentials':{'openai-main':'synthetic-key'}}).encode();process.stdin.write(b'TEKES-KERNEL-BOOTSTRAP/1\n'+struct.pack('!I',len(payload))+payload);process.stdin.flush()
                assert select.select([process.stdout],[],[],15)[0],'Kernel readiness timeout';ready=json.loads(process.stdout.readline())
                run({'kind':'native','url':ready['url'],'bearer':base64.urlsafe_b64encode(token).decode().rstrip('='),'commands':commands(cwd,providers['providers'][0]['id'],providers['providers'][0]['models'][0]['id'])})
            finally:
                process.stdin.close()
                try:process.wait(timeout=15)
                except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=10)
finally:
    server.shutdown();server.server_close()
