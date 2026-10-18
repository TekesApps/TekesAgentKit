#!/usr/bin/env python3
"""Exercise the three separate endpoints with owned temporary state and synthetic local providers."""
import argparse, base64, zlib, shutil, uuid, http.server, json, os, re, select, signal, struct, subprocess, tempfile, threading, time, urllib.request, urllib.parse
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--pi-root',type=Path)
parser.add_argument('--dsh-root',type=Path)
parser.add_argument('--kernel-root',type=Path)
parser.add_argument('--kernel-target-dir',type=Path)
parser.add_argument('--ui-demo',type=Path,help='Optional already-built AgentKit UI consumer for Native history-to-GPU acceptance')
parser.add_argument('--ui-output',type=Path)
args=parser.parse_args()
if args.ui_demo and not args.kernel_root:
    parser.error("--ui-demo requires --kernel-root")
if args.ui_output and not args.ui_demo:
    parser.error("--ui-output requires --ui-demo")
if not any((args.pi_root,args.dsh_root,args.kernel_root)):parser.error("supply at least one runtime root")
subprocess.run(['cargo','build','--locked','-p','tekes-agent-headless'],cwd=ROOT,check=True)
requests=[]
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(self.path)
        if 'ENDPOINT_QUEUE_HOLD' in json.dumps(body):time.sleep(0.6)
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        if self.path.endswith('/responses'):
            frames=[{'type':'response.completed','response':{'id':'synthetic-response','status':'completed','output':[{'type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':'ENDPOINT_REPLY_OK'}]}],'usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}}]
            if 'ENDPOINT_NATIVE_CHILD' in json.dumps(body) and not any(item.get('type')=='function_call_output' for item in body.get('input',[]) if isinstance(item,dict)):
                task={'task_name':'Bounded child proof','goal':'Return a short proof','instructions':'CHILD_NATIVE_PROOF. Reply ENDPOINT_REPLY_OK.','input_sources':[],'output':{'mode':'inline','format':'text','name':'result','path':'','contract':'short result'}}
                frames=[
                    {'type':'response.output_item.added','output_index':0,'item':{'type':'function_call','call_id':'native-child-proof','name':'task','arguments':''}},
                    {'type':'response.function_call_arguments.delta','output_index':0,'delta':''},
                    {'type':'response.function_call_arguments.done','output_index':0,'arguments':json.dumps(task)},
                    {'type':'response.completed','response':{'id':'native-child-call','status':'completed','usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}},
                ]
        else:
            common={'id':'synthetic-chat','object':'chat.completion.chunk','created':1,'model':'endpoint-test'}
            frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','content':'ENDPOINT_'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{'content':'REPLY_OK'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'stop'}],usage={'prompt_tokens':10,'completion_tokens':4,'total_tokens':14})]
            transcript=json.dumps(body.get('messages',[]))
            change=re.search(r'ENDPOINT_CHANGE_DIRECTORY=([^\s"\\]+)',transcript)
            if change and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'directory-call','type':'function','function':{'name':'working_directory','arguments':json.dumps({'cd':change.group(1)})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
            if 'ENDPOINT_SUBAGENT_CATALOG' in transcript and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'child-call','type':'function','function':{'name':'subagent','arguments':json.dumps({'description':'Catalog proof child','prompt':'CHILD_INITIAL_PROOF'})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
            if 'ENDPOINT_PI_CHILD' in transcript:
                values=[]
                for message in body.get('messages',[]):
                    if message.get('role')=='tool':
                        try: values.append(json.loads(message.get('content','')))
                        except (ValueError,TypeError): pass
                child=next((value.get('childSessionId') for value in values if isinstance(value,dict) and value.get('childSessionId')),None)
                action=None
                if child is None: action={'action':'create','label':'Model child'}
                elif not any(value.get('accepted') for value in values if isinstance(value,dict)): action={'action':'prompt','childSessionId':child,'text':'CHILD_PI_TOOL_PROOF'}
                elif not any(value.get('settled') and 'entries' in value for value in values if isinstance(value,dict)):
                    action={'action':'read','childSessionId':child};time.sleep(0.05)
                if action:
                    frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'pi-child-'+str(len(values)),'type':'function','function':{'name':'tekes_subagent','arguments':json.dumps(action)}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
            if 'ENDPOINT_GOAL_COMPLETE' in transcript and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'goal-call','type':'function','function':{'name':'agentkit_goal','arguments':json.dumps({'phase':'complete'})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
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
        print(result.stdout[-12000:]);print(result.stderr[-3000:])
        if config['kind']=='native':
            for path in (base/'kernel').rglob('*.jsonl'):
                print('KERNEL_FIXTURE_DIAGNOSTIC',path.name,path.read_text()[-20000:])
            print('KERNEL_STDERR',b''.join(kernel_errors).decode(errors='replace')[-16000:])
        raise AssertionError(config['kind']+' endpoint failed')
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
    # Independent no-UI presentation consumer: compile draft, immediate echo, dispatch, settle, reconcile.
    selected_model=next(command['args'] for command in config['commands'] if command.get('method')=='session.selectModel')
    proof_commands=[{'op':'create','cwd':str(Path(next(iter(stat))['absolutePath']).parent),'saveAs':'presented'},
        {'op':'call','sessionId':'presented','method':'session.selectModel','args':selected_model},
        {'op':'open','sessionId':'presented'},{'op':'compose','sessionId':'presented','text':'Presentation layer proof'},
        {'op':'waitSettled','sessionId':'presented'},{'op':'presentation','sessionId':'presented'},
        {'op':'reconnect'},{'op':'open','sessionId':'presented'},{'op':'presentation','sessionId':'presented'}]
    if config['kind'] in ('native','pi'):
        proof_commands += [
            {'op':'sideChatCreate','sessionId':'presented','title':'Proof parent','saveAs':'side'},
            {'op':'open','sessionId':'side'},{'op':'compose','sessionId':'side','text':'SideChat isolated turn'},
            {'op':'waitSettled','sessionId':'side','minimumTurn':2},{'op':'presentation','sessionId':'side'},
            {'op':'sideChatClose','sessionId':'side'},{'op':'sideChats'}]
    else:
        proof_commands += [{'op':'sideChatCreateError','sessionId':'presented'},{'op':'childCatalog','sessionId':'presented'}]
    proof=execute(dict(config,commands=proof_commands))
    for row in (row for row in proof if row.get('op')=='presentation'):
        view=row['result']
        assert view['connection']['phase']=='connected' and view['canSubmit'],view
        assert view['summary']['scope']['sessionId']==view['run']['scope']['sessionId'],view
        assert any(item['text']=='ENDPOINT_REPLY_OK' and item['isFinal'] for item in view['transcript']['items']),view
        assert not any(item['id'].startswith('echo-item:') for item in view['transcript']['items']),view
    if config['kind'] in ('native','pi'):
        assert next(row['result'] for row in proof if row.get('op')=='sideChats')['chats']=={}, 'SideChat survived confirmed discard'
        chat=next(row['result'] for row in proof if row.get('op')=='sideChatCreate')
        assert chat['parent']['sessionId']!=chat['scope']['sessionId'] and chat['title']=='SideChat · Proof parent',chat
    else:
        catalog=next(row['result'] for row in proof if row.get('op')=='childCatalog')
        assert isinstance(catalog['parentAvailable'],bool) and isinstance(catalog['entries'],list),catalog
    print(config['kind'].upper()+'_PANELS_LIVE_OK summary-scope+sidechat-capability+lifecycle-or-catalog')
    print(config['kind'].upper()+'_PRESENTATION_LIVE_OK composer+immediate-echo+identity-reconciliation+final-answer+reconnect')
    return outputs
try:
    with tempfile.TemporaryDirectory(prefix='agentkit-three-endpoints-') as tmp:
        base=Path(tmp);cwd=base/'project';cwd.mkdir();(cwd/'proof.txt').write_text('proof line one\nline two\n')
        if args.pi_root:
            pi=args.pi_root.resolve();data=base/'pi';(data/'agent').mkdir(parents=True)
            (data/'agent/models.json').write_text(json.dumps({'providers':{'endpoint-test':{'baseUrl':base_url,'api':'openai-completions','apiKey':'synthetic-key','models':[{'id':'endpoint-test','name':'Endpoint test','contextWindow':32768,'maxTokens':1024,'input':['text','image']}]}}}))
            config={'kind':'pi','executable':str(pi/'node/bin/node'),'arguments':[str(pi/'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')],'dataRoot':str(data),'environment':{'HOME':str(base/'pi-home'),'PATH':os.environ.get('PATH','/usr/bin:/bin')},'commands':commands(cwd,'endpoint-test','endpoint-test')}
            Path(config['environment']['HOME']).mkdir()
            extension=base/'question.mjs'
            extension.write_text("export default function(pi){pi.registerCommand('endpoint-question',{handler:async(args,ctx)=>{const value=await ctx.ui.select('Endpoint question',['yes','no']);pi.appendEntry('endpoint-answer',{value});}});pi.registerCommand('endpoint-settled',{handler:async()=>{pi.appendEntry('tekes.run.v1',{id:'endpoint-settled-turn',phase:'settled'});}});}")
            config['extensions']=[str(extension)]
            config['commands'].insert(-3,{'op':'promptAndAnswer','sessionId':'s','text':'/endpoint-question','outcome':{'value':'yes'}})
            outputs=run(config)
            assert 'endpoint-answer' in json.dumps(outputs) and 'yes' in json.dumps(outputs),'native question result missing'
            print('PI_NATIVE_ACTIONABLE_LIVE_OK pending prompt + answer + persisted native entry')
            children=[{'op':'create','cwd':str(cwd),'saveAs':'parent'},
                {'op':'call','sessionId':'parent','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},
                {'op':'childCreate','sessionId':'parent','label':'SDK child','rpcId':'stable-create','saveAs':'sdk-child'},
                {'op':'childCreate','sessionId':'parent','label':'SDK child','rpcId':'stable-create'},
                {'op':'childPrompt','sessionId':'parent','childSessionId':'sdk-child','rpcId':'child-input','content':[{'type':'text','text':'CHILD_PI_SDK_PROOF'}]},
                {'op':'open','sessionId':'sdk-child'},{'op':'waitSettled','sessionId':'sdk-child'},
                {'op':'childRead','sessionId':'parent','childSessionId':'sdk-child'},
                {'op':'childPrompt','sessionId':'parent','childSessionId':'sdk-child','rpcId':'child-steer','mode':'steer','content':[{'type':'text','text':'CHILD_PI_STEER_PROOF'}]},
                {'op':'waitSettled','sessionId':'sdk-child','minimumTurn':2},
                {'op':'open','sessionId':'parent'},{'op':'compose','sessionId':'parent','text':'ENDPOINT_PI_CHILD'},
                {'op':'waitSettled','sessionId':'parent'},{'op':'presentation','sessionId':'parent','saveAs':'model-child','savePointer':'/summary/subagents/0/sessionId'},
                {'op':'childCatalog','sessionId':'parent'},
                {'op':'childHistory','sessionId':'parent','childSessionId':'model-child'},
                {'op':'reconnect'},{'op':'childRead','sessionId':'parent','childSessionId':'sdk-child'},
                {'op':'childCancel','sessionId':'parent','childSessionId':'sdk-child'},
                {'op':'create','cwd':str(cwd),'saveAs':'other'},
                {'op':'callError','method':'subagents.cancel','args':{'parentSessionId':'@other','childSessionId':'@sdk-child'}}]
            child_outputs=execute(dict(config,commands=children))
            creates=[row['result']['sessionId'] for row in child_outputs if row.get('op')=='childCreate'];assert creates[0]==creates[1],'child create replay duplicated pointer'
            parent_view=next(row['result'] for row in child_outputs if row.get('op')=='presentation')
            assert len(parent_view['summary']['subagents'])==1 and parent_view['summary']['subagents'][0]['title']=='Model child',parent_view['summary']
            child_view=next(row['result'] for row in child_outputs if row.get('op')=='childHistory')
            assert 'CHILD_PI_TOOL_PROOF' in json.dumps(child_view),'model child input missing'
            assert 'CHILD_PI_SDK_PROOF' not in json.dumps(child_view) and 'ENDPOINT_PI_CHILD' not in json.dumps(child_view),'parent/sibling history leaked into child'
            assert not any('agentkit-host/v1:' in json.dumps(row['result']) for row in child_outputs if row.get('op') in ('presentation','childHistory')),'host control token leaked to projection'
            parent_id=next(row['result']['sessionId'] for row in child_outputs if row.get('op')=='create')
            replay=execute(dict(config,commands=[{'op':'childCatalog','sessionId':parent_id},{'op':'childCreate','sessionId':parent_id,'label':'SDK child','rpcId':'stable-create'}]))
            assert replay[-1]['result']['sessionId']==creates[0],'child pointer did not survive process restart'
            print('PI_SUBAGENTS_LIVE_OK API+model-tool+ownership+queue+steer+summary+history+reconnect+restart')
            # Resume a native session from an explicitly authorized host root, with a new pointer registry.
            locator=next(row['result'] for row in outputs if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'nativeSessionFile' in row['result'])
            adopted=base/'adopted';adopted.mkdir();source=adopted/'existing.jsonl';shutil.copyfile(locator['nativeSessionFile'],source)
            imported_id=str(uuid.uuid4());resumed=dict(config,dataRoot=str(base/'resumed-pi'),sessionRoots=[str(adopted)])
            resumed['commands']=[{'op':'call','method':'session.adopt','args':{'sessionId':imported_id,'cwd':str(cwd),'sessionFile':str(source)}},{'op':'open','sessionId':imported_id},{'op':'snapshot'}]
            result=subprocess.run([str(ROOT/'target/debug/tekes-agent-headless')],input=json.dumps(resumed),text=True,capture_output=True,timeout=40)
            assert result.returncode==0,result.stderr
            assert 'ENDPOINT_REPLY_OK' in result.stdout and 'endpoint-answer' in result.stdout,'resumed native history missing'
            print('PI_NATIVE_POINTER_RESUME_LIVE_OK explicit root + native RPC history')
            source_id=next(row['result']['sessionId'] for row in outputs if row.get('op')=='create')
            state=next(row['result'] for row in reversed(outputs) if row.get('op')=='snapshot')['sessions'][source_id]
            boundary=next(entry['id'] for entry in state['native_conversation']['entries'] if entry.get('message',{}).get('role')=='assistant')
            lifecycle=[
                {'op':'call','method':'endpoint.capabilities'},
                {'op':'call','method':'workspace.create','args':{'path':str(cwd)},'saveAs':'workspace','savePointer':'/workspace/workspaceId'},
                {'op':'call','method':'workspace.rename','args':{'workspaceId':'@workspace','title':'Rust workspace'}},
                {'op':'call','method':'session.create','args':{'workspaceId':'@workspace'},'saveAs':'workspace-session'},
                {'op':'call','sessionId':source_id,'method':'session.fork','args':{'entryId':boundary},'saveAs':'fork'},
                {'op':'open','sessionId':'fork'}, {'op':'snapshot'},
                {'op':'call','sessionId':source_id,'method':'session.fork','args':{'ephemeral':True},'saveAs':'scratch'},
                {'op':'open','sessionId':'scratch'},
                {'op':'call','sessionId':'scratch','method':'session.locator'},
                {'op':'call','sessionId':'scratch','method':'session.discard'},
                {'op':'call','sessionId':source_id,'method':'session.rename','args':{'title':'Renamed source'}},
                {'op':'call','sessionId':source_id,'method':'session.archive'},
                {'op':'call','sessionId':source_id,'method':'session.unarchive'},
                {'op':'open','sessionId':source_id},
                {'op':'call','sessionId':source_id,'method':'commands/list'},
                {'op':'call','sessionId':source_id,'method':'session.models'},
                {'op':'call','method':'models.list'},
                {'op':'observeReady','sessionId':source_id},
                {'op':'call','sessionId':source_id,'method':'settings.describe'},
                {'op':'call','sessionId':source_id,'method':'skills/list'},
                {'op':'call','sessionId':source_id,'method':'goals.create','args':{'objective':'Test goal','maxGoalRounds':2},'saveAs':'goal','savePointer':''},
                {'op':'call','sessionId':source_id,'method':'goals.edit','args':{'ref':'@goal','objective':'ENDPOINT_GOAL_COMPLETE'},'saveAs':'edited-goal','savePointer':''},
                {'op':'callError','sessionId':source_id,'method':'goals.pause','args':{'ref':'@goal'}},
                {'op':'call','sessionId':source_id,'method':'goals.pause','args':{'ref':'@edited-goal'},'saveAs':'paused-goal','savePointer':''},
                {'op':'call','sessionId':source_id,'method':'goals.resume','args':{'ref':'@paused-goal'},'saveAs':'resumed-goal','savePointer':''},
                {'op':'waitSettled','sessionId':source_id,'minimumTurn':2},
                {'op':'waitGoal','sessionId':source_id,'phase':'complete'},
                {'op':'call','sessionId':source_id,'method':'goals.get','saveAs':'settled-goal','savePointer':''},
                {'op':'call','sessionId':source_id,'method':'goals.clear','args':{'ref':'@settled-goal'}},
                {'op':'call','sessionId':source_id,'method':'feedback.put','args':{'messageId':boundary,'rating':'positive','ifVersion':None},'saveAs':'feedback-version','savePointer':'/version'},
                {'op':'callError','sessionId':source_id,'method':'feedback.delete','args':{'messageId':boundary,'ifVersion':'stale'}},
                {'op':'call','sessionId':source_id,'method':'feedback.delete','args':{'messageId':boundary,'ifVersion':'@feedback-version'}},
                {'op':'reconnect'},{'op':'snapshot'},
            ]
            result=execute(dict(config,commands=lifecycle))
            scratch_locator=next(row['result'] for row in result if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'nativeSessionFile' in row['result'])
            assert not Path(scratch_locator['nativeSessionFile']).exists(),'discard left scratch native file'
            fork_states=next(row['result'] for row in result if row.get('op')=='snapshot')['sessions']
            fork_id=next(key for key,state in fork_states.items() if state.get('native_conversation'))
            fork_snapshot=fork_states[fork_id]
            assert 'ENDPOINT_REPLY_OK' in json.dumps(fork_snapshot) and 'endpoint-answer' not in json.dumps(fork_snapshot),'exact native fork boundary lost'
            assert result[-1]['result']['workspaces'][0]['title']=='Rust workspace','workspace sync lost'
            print('PI_COMPLETE_LIFECYCLE_LIVE_OK workspace+exact-fork+scratch-discard+rename+archive+commands+settings+skills+goal-CAS+feedback-CAS')
            # A previously adopted pointer must not accept a different native identity at the same path.
            fork_file=fork_snapshot['native_conversation']['state']['sessionFile']
            shutil.copyfile(fork_file,source)
            guard=execute(dict(resumed,commands=[{'op':'callError','sessionId':imported_id,'method':'pi.getState'}]))
            assert 'identity changed' in json.dumps(guard),'replacement native identity was accepted'
            source.unlink()
            guard=execute(dict(resumed,commands=[{'op':'callError','sessionId':imported_id,'method':'pi.getState'}]))
            assert 'missing' in json.dumps(guard) and not source.exists(),'missing pointer silently recreated history'
            print('PI_POINTER_GUARDS_LIVE_OK native-identity-replacement+missing-file-no-recreation')
            marked=execute(dict(config,commands=[{'op':'call','sessionId':fork_id,'method':'commands.execute','args':{'name':'endpoint-settled'}},
                {'op':'call','sessionId':fork_id,'method':'session.history.export'}]))
            mark_entries=marked[-1]['result']['entries']
            cut=next(index+1 for index,entry in enumerate(mark_entries) if entry.get('customType')=='tekes.run.v1' and entry['data']['id']=='endpoint-settled-turn')
            precise=execute(dict(config,commands=[{'op':'open','sessionId':fork_id},
                {'op':'call','sessionId':fork_id,'method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},
                {'op':'call','sessionId':fork_id,'method':'session.prompt','args':{'mode':'queue','content':[{'type':'text','text':'AFTER_COMPLETED_BOUNDARY'}]}},
                {'op':'waitSettled','sessionId':fork_id,'minimumTurn':2},
                {'op':'call','sessionId':fork_id,'method':'session.fork','args':{'atSeq':cut},'saveAs':'precise'},
                {'op':'open','sessionId':'precise'},{'op':'snapshot'}]))
            precise_state=next(state for key,state in precise[-1]['result']['sessions'].items() if key!=fork_id and state.get('native_conversation'))
            assert 'endpoint-settled-turn' in json.dumps(precise_state) and 'AFTER_COMPLETED_BOUNDARY' not in json.dumps(precise_state),'completed-turn fork included later history'
            print('PI_COMPLETED_FORK_LIVE_OK ClientKit-sequence+exact-settled-boundary')
            assert any(row.get('op')=='waitGoal' and row['result']['phase']=='complete' and row['result']['activation']=='disarmed' for row in result),'goal completion did not disarm'
            bounded=[{'op':'create','cwd':str(cwd),'saveAs':'bounded'},{'op':'call','sessionId':'bounded','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},{'op':'open','sessionId':'bounded'},
                {'op':'call','sessionId':'bounded','method':'goals.create','args':{'objective':'A goal the synthetic provider will not complete','maxGoalRounds':1},'saveAs':'bounded-goal','savePointer':''},
                {'op':'call','sessionId':'bounded','method':'goals.resume','args':{'ref':'@bounded-goal'}},
                {'op':'waitGoal','sessionId':'bounded','phase':'blocked'}, {'op':'reconnect'},{'op':'call','sessionId':'bounded','method':'goals.get'}]
            bounded_result=execute(dict(config,commands=bounded))
            assert bounded_result[-1]['result']['roundsStarted']==1 and bounded_result[-1]['result']['activation']=='disarmed','goal bound/reconnect fence failed'
            print('PI_GOAL_DRIVER_LIVE_OK model-completion+round-budget+disarm-on-reconnect')
            def png_chunk(kind,data):return struct.pack('!I',len(data))+kind+data+struct.pack('!I',zlib.crc32(kind+data))
            png=base64.b64encode(b'\x89PNG\r\n\x1a\n'+png_chunk(b'IHDR',struct.pack('!IIBBBBB',1,1,8,6,0,0,0))+png_chunk(b'IDAT',zlib.compress(b'\x00\xff\x00\x00\xff'))+png_chunk(b'IEND',b'')).decode()
            attachments=[{'op':'create','cwd':str(cwd),'saveAs':'image'}, {'op':'call','sessionId':'image','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}}, {'op':'open','sessionId':'image'},
                {'op':'call','sessionId':'image','method':'session.uploadFile','args':{'name':'a+b.txt','mediaType':'text/plain','data':base64.b64encode(b'attachment proof').decode()},'saveAs':'upload','savePointer':'/receiptId'},
                {'op':'call','sessionId':'image','method':'session.fileAttachment','args':{'attachmentId':'@upload'}},
                {'op':'callError','sessionId':source_id,'method':'session.fileAttachment','args':{'attachmentId':'@upload'}},
                {'op':'call','sessionId':'image','method':'session.prompt','args':{'mode':'queue','content':[{'type':'text','text':'Image and file proof'},{'type':'image','mediaType':'image/png','data':png},{'type':'file','receiptId':'@upload'}]}},
                {'op':'waitSettled','sessionId':'image'},{'op':'snapshot'}]
            result=execute(dict(config,commands=attachments))
            assert 'attachment proof'==base64.b64decode(next(row['result']['data'] for row in result if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'data' in row['result'])).decode()
            assert any(part.get('type')=='image' for session in result[-1]['result']['sessions'].values() if session.get('native_conversation') for entry in session['native_conversation']['entries'] for part in (entry.get('message',{}).get('content',[]) if isinstance(entry.get('message',{}).get('content',[]),list) else [])),'native image missing: '+json.dumps([session['native_conversation'] for session in result[-1]['result']['sessions'].values() if session.get('native_conversation')])[-3000:]
            print('PI_ATTACHMENTS_LIVE_OK image+file+historical-read+session-isolation')
            queue=[{'op':'create','cwd':str(cwd),'saveAs':'queue'}, {'op':'call','sessionId':'queue','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}}, {'op':'open','sessionId':'queue'},
                {'op':'call','sessionId':'queue','method':'session.prompt','rpcId':'queue-first','args':{'mode':'queue','content':[{'type':'text','text':'ENDPOINT_QUEUE_HOLD'}]}},
                {'op':'call','sessionId':'queue','method':'session.prompt','rpcId':'queue-second','args':{'mode':'queue','content':[{'type':'text','text':'QUEUED_BEFORE_EDIT'}]},'saveAs':'queued','savePointer':'/itemId'},
                {'op':'call','sessionId':'queue','method':'session.updateQueue','args':{'itemId':'@queued','action':{'kind':'edit','content':[{'type':'text','text':'QUEUED_EDIT_OK'}]}}},
                {'op':'call','sessionId':'queue','method':'session.prompt','rpcId':'queue-third','args':{'mode':'queue','content':[{'type':'text','text':'QUEUED_TO_REMOVE'}]},'saveAs':'removed','savePointer':'/itemId'},
                {'op':'call','sessionId':'queue','method':'session.updateQueue','args':{'itemId':'@removed','action':{'kind':'remove'}}},
                {'op':'waitSettled','sessionId':'queue','minimumTurn':2},{'op':'snapshot'}]
            result=execute(dict(config,commands=queue));queue_id=next(row['result']['sessionId'] for row in result if row.get('op')=='create')
            conversation=json.dumps(result[-1]['result']['sessions'][queue_id]['native_conversation'])
            assert 'QUEUED_EDIT_OK' in conversation and 'QUEUED_BEFORE_EDIT' not in conversation and 'QUEUED_TO_REMOVE' not in conversation,'queue edit/remove/drain lost'
            assert not result[-1]['result']['sessions'][queue_id]['queue'],'queue did not settle'
            print('PI_QUEUE_LIVE_OK durable-admission+edit+remove+automatic-drain')

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
                extensions=[{'op':'call','method':'endpoint.capabilities'},
                    {'op':'call','sessionId':session_id,'method':'session.models'},
                    {'op':'call','sessionId':session_id,'method':'commands/list'},
                    {'op':'call','sessionId':session_id,'method':'skills/list'},
                    {'op':'call','method':'settings.describe'},
                    {'op':'call','method':'providers.list'},
                    {'op':'call','sessionId':session_id,'method':'attachments.policy'},
                    {'op':'observeReady','sessionId':session_id},
                    {'op':'call','sessionId':session_id,'method':'session.uploadFile','args':{'name':'a+b.txt','mediaType':'text/plain','data':base64.b64encode(b'DSH upload proof').decode()}},
                    {'op':'call','method':'workspace.create','args':{'path':str(cwd)},'saveAs':'dsh-workspace','savePointer':'/workspace/workspaceId'},
                    {'op':'call','method':'workspace.rename','args':{'workspaceId':'@dsh-workspace','title':'DSH workspace proof'}},
                    {'op':'reconnect'},{'op':'snapshot'}]
                extended=execute(dict(config,commands=extensions))
                model_catalog=next(row['result'] for row in extended if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'groups' in row['result'])
                assert model_catalog['current']['model']=='endpoint-test' and model_catalog['groups'],'model catalog normalization lost'
                upload=next(row['result'] for row in extended if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'receiptId' in row['result'])
                assert upload['file']['name']=='a+b.txt','upload query encoding lost plus'
                assert any(workspace['title']=='DSH workspace proof' for workspace in extended[-1]['result']['workspaces']),'workspace sync lost'
                print('DSH_EXTENSIONS_LIVE_OK models+commands+skills+settings+providers+policy+upload+workspace-sync')
                child_proof=[{'op':'create','cwd':str(cwd),'saveAs':'parent'},
                    {'op':'call','sessionId':'parent','method':'session.selectModel','args':{'provider':'endpoint-test','model':'endpoint-test'}},
                    {'op':'open','sessionId':'parent'},{'op':'compose','sessionId':'parent','text':'ENDPOINT_SUBAGENT_CATALOG'},
                    {'op':'waitSettled','sessionId':'parent'},
                    {'op':'childCatalog','sessionId':'parent','saveAs':'child'},
                    {'op':'open','sessionId':'child'},
                    {'op':'childPrompt','sessionId':'parent','childSessionId':'child','content':[{'type':'text','text':'CHILD_CONTINUATION_PROOF'}]},
                    {'op':'waitSettled','sessionId':'child','minimumTurn':2},
                    {'op':'reconnect'},{'op':'open','sessionId':'child'},
                    {'op':'presentation','sessionId':'child'},
                    {'op':'childCancel','sessionId':'parent','childSessionId':'child'}]
                child_result=execute(dict(config,commands=child_proof))
                child_view=next(row['result'] for row in child_result if row.get('op')=='presentation')
                assert any(item['text']=='CHILD_CONTINUATION_PROOF' for item in child_view['transcript']['items']), 'child input missing'
                assert any(item['text']=='ENDPOINT_REPLY_OK' and item['isFinal'] for item in child_view['transcript']['items']), 'child output missing'
                print('DSH_CHILD_CONTROL_LIVE_OK catalog+owned-prompt+independent-history+receipt+cancel')
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
            kernel=args.kernel_root.resolve();kernel_target=args.kernel_target_dir.resolve() if args.kernel_target_dir else kernel/'target';home=base/'kernel-home';home.mkdir()
            providers=json.loads((kernel/'fixtures/config/providers.canonical.json').read_text());providers['providers'][0]['endpoint']=base_url
            (base/'kernel').mkdir(mode=0o700)
            (base/'kernel/config').mkdir(mode=0o700)
            (base/'kernel/config/settings.json').write_text(json.dumps({'format':1,'revision':1,'limits':{'max_workers':8,'max_provider_leases':8}},sort_keys=True,separators=(',',':'))+'\n')
            (base/'kernel/config/settings.json').chmod(0o600)
            launch=base/'kernel-launch.json';launch.write_text(json.dumps({'format':1,'bootstrap_version':1,'root':str(base/'kernel'),'worker':str(kernel_target/'debug/tekes-worker'),'listen':'127.0.0.1:0','providers':providers,'credential_bindings':{'openai-main':'AGENTKIT_SYNTHETIC_KEY'}}))
            env=dict(os.environ,HOME=str(home));env.pop('TEKES_KERNEL_ENDPOINT_TOKEN',None);env.pop('AGENTKIT_SYNTHETIC_KEY',None)
            process=subprocess.Popen([str(kernel_target/'debug/tekes-supervisor'),'--built-in',str(launch)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
            kernel_errors=[]
            threading.Thread(target=lambda:kernel_errors.extend(iter(process.stderr.readline,b'')),daemon=True).start()
            try:
                token=os.urandom(32);payload=json.dumps({'endpointToken':token.hex(),'credentials':{'openai-main':'synthetic-key'}}).encode();process.stdin.write(b'TEKES-KERNEL-BOOTSTRAP/1\n'+struct.pack('!I',len(payload))+payload);process.stdin.flush()
                assert select.select([process.stdout],[],[],15)[0],'Kernel readiness timeout';readiness=process.stdout.readline();assert readiness,b''.join(kernel_errors).decode(errors='replace');ready=json.loads(readiness)
                config={'kind':'native','url':ready['url'],'bearer':base64.urlsafe_b64encode(token).decode().rstrip('='),'commands':commands(cwd,providers['providers'][0]['id'],providers['providers'][0]['models'][0]['id'])}
                outputs=run(config);native_id=next(row['result']['sessionId'] for row in outputs if row.get('op')=='create')
                native=[{'op':'call','method':'endpoint.capabilities'},
                    {'op':'call','sessionId':native_id,'method':'session.models'},
                    {'op':'call','sessionId':native_id,'method':'commands/list'},
                    {'op':'call','sessionId':native_id,'method':'skills/list'},
                    {'op':'call','sessionId':native_id,'method':'attachments.policy'},
                    {'op':'observeReady','sessionId':native_id},
                    {'op':'call','method':'settings.describe'},
                    {'op':'call','method':'session.initialPresets'},
                    {'op':'call','sessionId':native_id,'method':'session.uploadFile','args':{'name':'native-proof.txt','mediaType':'text/plain','data':base64.b64encode(b'Native upload proof').decode()},'saveAs':'native-upload','savePointer':'/file/attachmentId'},
                    {'op':'call','sessionId':native_id,'method':'session.fileAttachment','args':{'attachmentId':'@native-upload'}},
                    {'op':'call','sessionId':native_id,'method':'session.fork','args':{'ephemeral':True},'saveAs':'scratch'},
                    {'op':'open','sessionId':'scratch'}, {'op':'call','sessionId':'scratch','method':'session.discard'},
                    {'op':'call','sessionId':native_id,'method':'session.rename','args':{'title':'Native lifecycle proof'}},
                    {'op':'call','sessionId':native_id,'method':'session.archive'},
                    {'op':'call','sessionId':native_id,'method':'session.unarchive'},
                    {'op':'open','sessionId':native_id},{'op':'snapshot'}]
                result=execute(dict(config,commands=native))
                assert any(row.get('op')=='call' and isinstance(row.get('result'),dict) and row['result'].get('data')==base64.b64encode(b'Native upload proof').decode() for row in result),'native upload round trip lost'
                caps=next(row['result'] for row in result if row.get('op')=='call' and isinstance(row.get('result'),dict) and 'methods' in row['result'])
                if 'subagents.history' in caps['methods']:
                    parent=str(uuid.uuid4())
                    native_children=[{'op':'call','method':'session.create','args':{'sessionId':parent,'execution':{'cwd':str(cwd),'read_roots':[],'writable_roots':[],'toolchain_roots':[],'network':False,'allowed_tools':['task'],'mcp':[]}},'saveAs':'parent'},
                        {'op':'call','sessionId':'parent','method':'session.selectModel','args':{'provider':providers['providers'][0]['id'],'model':providers['providers'][0]['models'][0]['id']}},
                        {'op':'call','sessionId':'parent','method':'approvals.select','args':{'mode':'danger-full-access'}},
                        {'op':'open','sessionId':'parent'},{'op':'compose','sessionId':'parent','text':'ENDPOINT_NATIVE_CHILD'},
                        {'op':'waitSettled','sessionId':'parent'},{'op':'presentation','sessionId':'parent'},
                        {'op':'childCatalog','sessionId':'parent','saveAs':'bounded','savePointer':'/entries/1/id'},
                        {'op':'childHistory','sessionId':'parent','childSessionId':'bounded'},
                        {'op':'childEarlier','sessionId':'parent','childSessionId':'bounded'},
                        {'op':'reconnect'},{'op':'childHistory','sessionId':'parent','childSessionId':'bounded'},
                        {'op':'callError','method':'subagents.history','args':{'parentSessionId':native_id,'childSessionId':'@bounded','maxMessages':50}}]
                    child_result=execute(dict(config,commands=native_children))
                    histories=[row['result'] for row in child_result if row.get('op')=='childHistory']
                    assert len(histories)==2 and all(any(item['text']=='ENDPOINT_REPLY_OK' and item['isFinal'] for item in h['items']) for h in histories),histories
                    assert all(h['tools'][0]['arguments']['task_name']=='Bounded child proof' for h in histories),'missing parent source-call projection'
                    print('NATIVE_BOUNDED_CHILD_LIVE_OK real-task+source-call+read-only-history+reconnect+ownership')
                print('NATIVE_COMPLETE_LIFECYCLE_LIVE_OK models+commands+skills+settings+presets+upload+scratch-discard+rename+archive')
                if args.ui_demo:
                    second=next(row['result']['sessionId'] for row in execute(dict(config,commands=[{'op':'create','cwd':str(cwd)}])) if row.get('op')=='create')
                    ui_config=base/'ui-native.json'
                    ui_config.write_text(json.dumps({'endpointId':'native-ui','url':ready['url'],'bearerEnv':'AGENTKIT_UI_FIXTURE_BEARER','sessions':[native_id,second]}))
                    output=args.ui_output.resolve() if args.ui_output else base/'ui-native.png'
                    ui_env=dict(os.environ,AGENTKIT_UI_FIXTURE_BEARER=config['bearer'])
                    ui=subprocess.run([str(args.ui_demo.resolve()),'--endpoint-config',str(ui_config),'--offscreen',str(output)],env=ui_env,text=True,capture_output=True,timeout=30)
                    assert ui.returncode==0,ui.stderr[-3000:]
                    assert output.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'),'Native UI image missing'
                    print('NATIVE_UI_CONSUMER_LIVE_OK authenticated-SEP+RunController+TekesUI-offscreen; native-interaction=false')

            finally:
                process.stdin.close()
                try:process.wait(timeout=15)
                except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=10)
finally:
    server.shutdown();server.server_close()
