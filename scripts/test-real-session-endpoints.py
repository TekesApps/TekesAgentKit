#!/usr/bin/env python3
"""Exercise the three separate endpoints with owned temporary state and synthetic local providers."""
import argparse, base64, zlib, shutil, uuid, http.server, json, os, re, select, signal, struct, subprocess, tempfile, threading, time, urllib.request, urllib.parse
from pathlib import Path
from native_probe_provider import native_probe_intent
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--pi-root',type=Path)
parser.add_argument('--dsh-root',type=Path)
parser.add_argument('--kernel-root',type=Path)
parser.add_argument('--kernel-target-dir',type=Path)
parser.add_argument('--ui-demo',type=Path,help='Optional already-built AgentKit UI consumer for each supplied harness history-to-GPU acceptance')
parser.add_argument('--ui-output',type=Path)
parser.add_argument('--ui-components',action='store_true',help='Mount formal Composer/Indicators with RunBinding; structured Transcript remains unavailable')
parser.add_argument('--ui-model-choices',action='store_true',help='Exercise public model choices and explicit Send preparation in isolated offscreen component sessions')
parser.add_argument('--ui-context',action='store_true',help='Exercise draft context cards, explicit removal and annotation/reference return in isolated component sessions')
parser.add_argument('--ui-initial-presets',action='store_true',help='Exercise public initial preset selection through shared create and prompt in isolated component sessions')
parser.add_argument('--ui-annotation-editor',action='store_true',help='Exercise public annotation editing and shared context compilation; implies the context proof')
parser.add_argument('--ui-attachments',action='store_true',help='Exercise public upload entry, original draft, Send, history and capability-specific file reads')
parser.add_argument('--ui-bounded-child',action='store_true',help='Verify actual read-only Native child messages through the public component; requires components and Kernel')
parser.add_argument('--ui-feedback-read',action='store_true',help='Read typed committed feedback and verify proven final response IDs; no mutations or rating controls')
parser.add_argument('--ui-native-run-controls',action='store_true',help='Native Send/Cancel/provider failure/explicit recovery over fresh owned sessions; no physical IME or reading-anchor acceptance')
parser.add_argument('--ui-native-readonly',action='store_true',help='Real Native bounded pane: explicit one-message pages, native anchor/refresh and draft isolation')
parser.add_argument('--ui-native-reading',action='store_true',help='Native public history page, retained A/B reading and reconnect on seeded owned harness sessions; synthetic wheel, no physical IME')
parser.add_argument('--ui-native-stream',action='store_true',help='Use fresh owned sessions for native Send, active preview, selection, settlement, draft isolation and reconnect; excludes reading anchors')
parser.add_argument('--ui-native-feedback',action='store_true',help='Open each owned-session native window sequentially and exercise feedback clicks, text services and session switching')
parser.add_argument('--ui-feedback-transcript',action='store_true',help='Exercise feedback through actual Transcript/RunView and captured host callbacks; offscreen only')
parser.add_argument('--ui-feedback-controls',action='store_true',help='Exercise public feedback controls and note editing over isolated real transports; offscreen only')
parser.add_argument('--ui-feedback-write',action='store_true',help='Mutating CAS/note proof on isolated fixture targets; no rating UI acceptance')
parser.add_argument('--ui-turn-footnote',action='store_true',help='Opt in to shared completed-turn fork in isolated UI sessions; requires components')
parser.add_argument('--ui-transcript-content',action='store_true',help='Mount the canonical mixed content tree with a generic TekesUI viewport; no keyed reading acceptance')
parser.add_argument('--ui-message-rows',action='store_true',help='Mount canonical plain user and machine rows through public components; not a full Transcript')
parser.add_argument('--ui-history-pages',action='store_true',help='Seed long isolated sessions and verify public page receipts, original scope and fresh reconnect history')
parser.add_argument('--ui-actionables',action='store_true',help='Exercise public actionable cards on configured Native/Pi/DSH question sources')
parser.add_argument('--ui-sidebar-query',action='store_true',help='Read typed public search and explicitly refresh public inventory cursors on owned conversation-history targets; offscreen protocol proof only')
args=parser.parse_args()
if args.ui_native_reading and not (args.ui_demo and args.ui_components and args.ui_transcript_content and args.ui_history_pages):
    parser.error('--ui-native-reading requires --ui-demo, --ui-components, --ui-transcript-content and --ui-history-pages')
if args.ui_sidebar_query and not (args.ui_demo and args.ui_components):
    parser.error('--ui-sidebar-query requires --ui-demo and --ui-components')
if args.ui_native_run_controls and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-native-run-controls requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_native_stream and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-native-stream requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_native_feedback and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-native-feedback requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_feedback_transcript and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-feedback-transcript requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_feedback_controls and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-feedback-controls requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_feedback_write and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-feedback-write requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_feedback_read and not (args.ui_demo and args.ui_components and args.ui_transcript_content):
    parser.error('--ui-feedback-read requires --ui-demo, --ui-components and --ui-transcript-content')
if args.ui_native_readonly and not (args.ui_bounded_child and args.ui_demo and args.ui_transcript_content):
    parser.error('--ui-native-readonly requires bounded child, UI demo and Transcript content')
if args.ui_bounded_child and (not args.ui_components or not args.kernel_root):
    parser.error('--ui-bounded-child requires --ui-components and --kernel-root')
if args.ui_turn_footnote and not args.ui_components:
    parser.error('--ui-turn-footnote requires --ui-components')
if args.ui_message_rows and not args.ui_components:
    parser.error('--ui-message-rows requires --ui-components')
if args.ui_transcript_content and not args.ui_components:
    parser.error('--ui-transcript-content requires --ui-components')
if args.ui_transcript_content and (args.ui_message_rows or args.ui_turn_footnote):
    parser.error('--ui-transcript-content cannot combine independent row probes')
if args.ui_history_pages and not args.ui_components:
    parser.error('--ui-history-pages requires --ui-components')
if args.ui_actionables and not args.ui_components:
    parser.error('--ui-actionables requires --ui-components')
if args.ui_model_choices and not args.ui_components:
    parser.error('--ui-model-choices requires --ui-components')
if args.ui_context and not args.ui_components:
    parser.error('--ui-context requires --ui-components')
if args.ui_annotation_editor and not args.ui_components:
    parser.error('--ui-annotation-editor requires --ui-components')
if args.ui_attachments and not args.ui_components:
    parser.error('--ui-attachments requires --ui-components')
if args.ui_initial_presets and not args.ui_components:
    parser.error('--ui-initial-presets requires --ui-components')
if args.ui_output and not args.ui_demo:
    parser.error("--ui-output requires --ui-demo")
if args.ui_components and not args.ui_demo:
    parser.error("--ui-components requires --ui-demo")
if not any((args.pi_root,args.dsh_root,args.kernel_root)):parser.error("supply at least one runtime root")
subprocess.run(['cargo','build','--locked','-p','tekes-agent-headless'],cwd=ROOT,check=True)
native_child_reply='ENDPOINT_REPLY_OK'+(('\n原生有界子历史长回答，用于真实阅读锚点验证。'*120) if args.ui_native_readonly else '')
requests=[]
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(self.path)
        intent=native_probe_intent(body)
        native_cancel=intent=='ENDPOINT_CANCEL_NATIVE_PROOF'
        if intent=='ENDPOINT_ERROR_NATIVE_PROOF':
            error=json.dumps({'error':{'message':'ENDPOINT_ERROR_NATIVE_PROOF: controlled invalid request','type':'invalid_request_error','code':'invalid_request_error'}}).encode()
            self.send_response(400);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(error)));self.end_headers();self.wfile.write(error);return
        if 'ENDPOINT_QUEUE_HOLD' in json.dumps(body):time.sleep(0.6)
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        if self.path.endswith('/responses'):
            frames=[{'type':'response.completed','response':{'id':'synthetic-response','status':'completed','output':[{'type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':'ENDPOINT_REPLY_OK'}]}],'usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}}]
            if native_cancel:
                frames=[
                    {'type':'response.output_item.added','output_index':0,'item':{'id':'cancel-native-output','type':'message','role':'assistant','phase':'final_answer','content':[]}},
                    {'type':'response.output_text.delta','output_index':0,'content_index':0,'delta':'CANCEL_PENDING'},
                    *frames,
                ]
            if 'ENDPOINT_STREAM_NATIVE_PROOF' in json.dumps(body):
                frames=[
                    {'type':'response.output_item.added','output_index':0,'item':{'id':'stream-native-output','type':'message','role':'assistant','phase':'final_answer','content':[]}},
                    {'type':'response.output_text.delta','output_index':0,'content_index':0,'delta':'ENDPOINT_'},
                    {'type':'response.output_text.delta','output_index':0,'content_index':0,'delta':'REPLY_OK'},
                    *frames,
                ]
            if args.ui_native_readonly and any(message.get('role')=='user' and 'CHILD_NATIVE_PROOF' in json.dumps(message) and 'ENDPOINT_NATIVE_CHILD' not in json.dumps(message) for message in body.get('input',[]) if isinstance(message,dict)):
                frames=[{'type':'response.completed','response':{'id':'bounded-native-result','status':'completed','output':[{'id':'bounded-native-output','type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':native_child_reply}]}],'usage':{'input_tokens':10,'output_tokens':400,'total_tokens':410}}}]
            if 'ENDPOINT_NATIVE_CHILD' in json.dumps(body) and not any(item.get('type')=='function_call_output' for item in body.get('input',[]) if isinstance(item,dict)):
                task={'task_name':'Bounded child proof','goal':'Return a short proof','instructions':'CHILD_NATIVE_PROOF. Reply ENDPOINT_REPLY_OK.','input_sources':[],'output':{'mode':'inline','format':'text','name':'result','path':'','contract':'short result'}}
                frames=[
                    {'type':'response.output_item.added','output_index':0,'item':{'type':'function_call','call_id':'native-child-proof','name':'task','arguments':''}},
                    {'type':'response.function_call_arguments.delta','output_index':0,'delta':''},
                    {'type':'response.function_call_arguments.done','output_index':0,'arguments':json.dumps(task)},
                    {'type':'response.completed','response':{'id':'native-child-call','status':'completed','usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}},
                ]
            if 'NATIVE_ACTIONABLE_UI_PROOF' in json.dumps(body) and not any(item.get('type')=='function_call_output' and item.get('call_id')=='native-ui-question' for item in body.get('input',[]) if isinstance(item,dict)):
                question={'question':'Native UI actionable round trip?','options':['continue-proof']}
                frames=[
                    {'type':'response.output_item.added','output_index':0,'item':{'type':'function_call','call_id':'native-ui-question','name':'ask_user_questions','arguments':''}},
                    {'type':'response.function_call_arguments.delta','output_index':0,'delta':''},
                    {'type':'response.function_call_arguments.done','output_index':0,'arguments':json.dumps(question)},
                    {'type':'response.completed','response':{'id':'native-ui-question-call','status':'completed','usage':{'input_tokens':10,'output_tokens':4,'total_tokens':14}}},
                ]
        else:
            common={'id':'synthetic-chat','object':'chat.completion.chunk','created':1,'model':'endpoint-test'}
            frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','content':'ENDPOINT_'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{'content':'REPLY_OK'},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'stop'}],usage={'prompt_tokens':10,'completion_tokens':4,'total_tokens':14})]
            if native_cancel:
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','content':'CANCEL_PENDING'},'finish_reason':None}]),*frames[1:]]
            transcript=json.dumps(body.get('messages',[]))
            change=re.search(r'ENDPOINT_CHANGE_DIRECTORY=([^\s"\\]+)',transcript)
            if change and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'directory-call','type':'function','function':{'name':'working_directory','arguments':json.dumps({'cd':change.group(1)})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
            if 'DSH_ACTIONABLE_UI_PROOF' in transcript and not any(message.get('role')=='tool' and message.get('tool_call_id')=='dsh-ui-question-call' for message in body.get('messages',[])):
                question={'questions':[{'id':'dsh-ui-question','question':'DSH UI actionable round trip?','options':[{'label':'continue-proof'}],'multi_select':False}]}
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'dsh-ui-question-call','type':'function','function':{'name':'ask_user_question','arguments':json.dumps(question)}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
            if 'DSH_ACTIONABLE_PROOF' in transcript and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                question={'questions':[{'id':'dsh-question-proof','question':'DSH actionable round trip?','options':[{'label':'continue-proof'}],'multi_select':False}]}
                frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'dsh-question-call','type':'function','function':{'name':'ask_user_question','arguments':json.dumps(question)}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
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
            for frame in frames:
                self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush()
                if native_cancel:time.sleep(1)
                if 'ENDPOINT_STREAM_IDENTITY_PROOF' in json.dumps(body):time.sleep(0.5)
                if 'ENDPOINT_STREAM_NATIVE_PROOF' in json.dumps(body):time.sleep(1)
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
        {'op':'open','sessionId':'presented'},{'op':'compose','sessionId':'presented','text':'ENDPOINT_STREAM_IDENTITY_PROOF' if config['kind']=='pi' else 'Presentation layer proof','captureNativePreview':config['kind']=='pi'},
        {'op':'waitSettled','sessionId':'presented'},
        *([{'op':'waitNativeEvent','sessionId':'presented','eventType':'message_end','role':'assistant'}] if config['kind']=='pi' else []),
        {'op':'snapshot'},{'op':'presentation','sessionId':'presented'},
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
    if config['kind']=='pi':
        presented=next(row['result']['sessionId'] for row in proof if row.get('op')=='create')
        settled=next(row['result'] for row in proof if row.get('op')=='snapshot')['sessions'][presented]
        kinds=[(event.get('type'),event.get('message',{}).get('role') if isinstance(event.get('message'),dict) else None) for event in settled['native_events']]
        assert any(kind=='message_end' and role=='assistant' for kind,role in kinds), 'Pi dropped the assistant lifecycle cause: '+json.dumps(kinds)
        assert any(entry.get('message',{}).get('role')=='assistant' for entry in settled['native_conversation']['entries']), 'Pi lifecycle cause missing its canonical snapshot'
        live=next(row['result']['nativePreview'] for row in proof if row.get('op')=='compose' and 'nativePreview' in row['result'])
        assert live['native_conversation']['isStreaming'], 'Live probe did not observe an active source'
        live_ids={event.get('agentkitStreamId') for event in live['native_events'] if event.get('type')=='message_update' and event.get('message',{}).get('role')=='assistant'}
        assert live_ids and None not in live_ids, 'Actual active Pi preview has no stream identity'
        assert not any(entry.get('message',{}).get('role')=='assistant' for entry in live['native_conversation']['entries']), 'Live probe did not precede persistence'
        receipts=settled.get('native_reading_identities',[])
        assert receipts and live_ids <= {receipt['streamId'] for receipt in receipts}, 'Pi active preview did not reconcile to a loaded entry receipt'
        assistants={entry['id'] for entry in settled['native_conversation']['entries'] if entry.get('message',{}).get('role')=='assistant'}
        ended={event.get('agentkitStreamId') for event in settled['native_events'] if event.get('type')=='message_end' and event.get('message',{}).get('role')=='assistant'}
        assert all(receipt['entryId'] in assistants and receipt['streamId'] in ended for receipt in receipts), receipts
        assert len({receipt['streamId'] for receipt in receipts})==len(receipts),receipts
        assert len({receipt['entryId'] for receipt in receipts})==len(receipts),receipts
        print('PI_READING_IDENTITY_LIVE_OK active-preview+public-final-object+loaded-entry+authenticated-stream-event')
        print('PI_NATIVE_ASSISTANT_BOUNDARY_LIVE_OK canonical-snapshot+original-message-end')
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
def ui_smoke(config, first, bounded_child=None):
    history_pages=args.ui_history_pages and bounded_child is None
    if not args.ui_demo:
        return
    kind=config['kind']
    if kind=='native' and (args.ui_actionables and bounded_child is None):
        first=str(uuid.uuid4())
        execute(dict(config,commands=[{'op':'call','method':'session.create','args':{'sessionId':first,'execution':{'cwd':str(cwd),'read_roots':[],'writable_roots':[],'toolchain_roots':[],'network':False,'allowed_tools':['ask_user_questions'],'mcp':[]}}},
            {'op':'call','sessionId':first,'method':'session.selectModel','args':{'provider':providers['providers'][0]['id'],'model':providers['providers'][0]['models'][0]['id']}}]))
    if history_pages:
        model=next(command['args'] for command in config['commands'] if command.get('method')=='session.selectModel')
        if not (kind=='native' and (args.ui_actionables and bounded_child is None)):
            first=next(row['result']['sessionId'] for row in execute(dict(config,commands=[{'op':'create','cwd':str(cwd)}])) if row.get('op')=='create')
        history=[{'op':'call','sessionId':first,'method':'session.selectModel','args':model},{'op':'open','sessionId':first}]
        for turn in range(1,29):
            text='UI_HISTORY_PAGE_TURN_'+str(turn)
            history += [{'op':'compose','sessionId':first,'text':text},{'op':'waitSettled','sessionId':first,'inputText':text}]
        execute(dict(config,commands=history))
        print(kind.upper()+'_UI_HISTORY_SEEDED turns=28 exact-input+completed-reply',flush=True)
    search_query=None
    if args.ui_sidebar_query and bounded_child is None:
        # A content-search contract does not promise that rename metadata is an
        # indexed document. Seed a unique word through the ordinary owned
        # conversation and local synthetic provider, then search that history.
        search_query='AgentKitOwnedQuery'+uuid.uuid4().hex
        model=next(command['args'] for command in config['commands'] if command.get('method')=='session.selectModel')
        execute(dict(config,commands=[{'op':'call','sessionId':first,'method':'session.selectModel','args':model},
            {'op':'open','sessionId':first},{'op':'compose','sessionId':first,'text':search_query},
            {'op':'waitSettled','sessionId':first,'inputText':search_query}]))
    second=next(row['result']['sessionId'] for row in execute(dict(config,commands=[{'op':'create','cwd':str(cwd)}])) if row.get('op')=='create')
    if args.ui_native_reading and bounded_child is None:
        history=[{'op':'call','sessionId':second,'method':'session.selectModel','args':model},{'op':'open','sessionId':second}]
        for turn in range(1,9):
            text='UI_READING_B_TURN_'+str(turn)
            history += [{'op':'compose','sessionId':second,'text':text},{'op':'waitSettled','sessionId':second,'inputText':text}]
        execute(dict(config,commands=history))
        print(kind.upper()+'_UI_READING_B_SEEDED turns=8 exact-input+completed-reply',flush=True)
    explicit={'kind':kind,'endpointId':kind+'-ui','sessions':[first,second]}
    ui_env=dict(os.environ)
    if kind=='pi':
        for field in ('executable','arguments','dataRoot','environment','extensions','sessionRoots'):
            if field in config: explicit[field]=config[field]
    else:
        explicit['url']=config['url']
        field='cookie' if 'cookie' in config else 'bearer'
        explicit[field+'Env']='AGENTKIT_UI_FIXTURE_CREDENTIAL'
        ui_env['AGENTKIT_UI_FIXTURE_CREDENTIAL']=config[field]
    path=base/('ui-'+kind+'.json')
    path.write_text(json.dumps(explicit))
    # Preserve --ui-output's original Native path when multiple harnesses are requested.
    output=args.ui_output.resolve() if args.ui_output else base/'ui-native.png'
    if bounded_child:output=output.with_name(output.stem+'-bounded'+output.suffix)
    if kind!='native':output=output.with_name(output.stem+'-'+kind+output.suffix)
    mode=['--components'] if args.ui_components else []
    if search_query:mode+=['--sidebar-query',search_query,'--sidebar-query-proof']
    if bounded_child:mode+=['--bounded-child-proof','--bounded-child-id',bounded_child]
    if args.ui_feedback_read:mode+=['--feedback-read-proof']
    if args.ui_feedback_write:mode+=['--feedback-write-proof']
    if args.ui_feedback_controls:mode+=['--feedback-controls-proof']
    if args.ui_feedback_transcript:mode+=['--feedback-transcript-proof']
    if (args.ui_model_choices and bounded_child is None):mode+=['--model-choice-proof']
    if (args.ui_context and bounded_child is None) or (args.ui_annotation_editor and bounded_child is None):mode+=['--context-proof']
    if (args.ui_annotation_editor and bounded_child is None):mode+=['--annotation-proof']
    if (args.ui_attachments and bounded_child is None):mode+=['--upload-proof']
    if (args.ui_actionables and bounded_child is None):mode+=['--actionable-proof']
    if history_pages:mode+=['--history-page-proof']
    if (args.ui_transcript_content or bounded_child is not None):mode+=['--transcript-content']
    if (args.ui_message_rows and bounded_child is None):mode+=['--input-message-proof','--output-message-proof']
    if (args.ui_turn_footnote and bounded_child is None):mode+=['--turn-footnote-proof','--turn-fork-proof']
    if (args.ui_initial_presets and bounded_child is None):mode+=['--initial-preset-proof','--initial-preset-cwd',str(cwd)]
    ui=subprocess.run([str(args.ui_demo.resolve()),*mode,'--endpoint-config',str(path),'--offscreen',str(output)],env=ui_env,text=True,capture_output=True,timeout=60)
    assert ui.returncode==0,ui.stderr[-3000:]
    receipt=next(json.loads(line) for line in ui.stdout.splitlines() if line.startswith('{') and json.loads(line).get('offscreen'))
    if search_query:
        search=receipt['sidebarSearchProof']
        assert search['publicTypedSearch'] and search['sharedController'] and search['expectedTargetFound'] and search['noSubmission'],search
        assert isinstance(search['hasMore'],bool),search
        refresh=search['inventoryRefresh']
        assert all(refresh[key] for key in ('publicCursors','sameConnectionEpoch','historyUnchanged','searchResultRetained')),refresh
        print(kind.upper()+'_UI_INVENTORY_REFRESH_OK public-cursors+same-epoch+retained-history+search-result; offscreen-only',flush=True)
        print(kind.upper()+'_UI_SIDEBAR_QUERY_OK public-typed-search+shared-controller+owned-target+original-endpoint; offscreen-only',flush=True)
    if args.ui_native_readonly and bounded_child is not None:
        raw=execute(dict(config,commands=[{'op':'call','method':'subagents.history','args':{'parentSessionId':first,'childSessionId':bounded_child,'maxMessages':1}}]))
        first_page=next(row['result'] for row in raw if row.get('op')=='call')
        older=execute(dict(config,commands=[{'op':'call','method':'subagents.history','args':{'parentSessionId':first,'childSessionId':bounded_child,'maxMessages':1,'throughSequence':first_page['throughSequence'],'beforeSequence':first_page['entries'][0]['event']['seq'],'historyRevision':first_page['historyRevision']}}]))
        older_page=next(row['result'] for row in older if row.get('op')=='call')
        assert first_page['hasMoreBefore'] and not older_page['hasMoreBefore'] and older_page['entries'],older_page
        assert all(entry['event']['type']=='turn/start' for entry in older_page['entries']),older_page
        output.with_suffix('.raw-bounded.json').write_text(json.dumps({'initial':raw,'older':older},ensure_ascii=False,indent=2)+'\n')
        print('NATIVE_BOUND_RAW_PAGE '+json.dumps([{'kind':entry['event']['type'],'seq':entry['event']['seq']} for entry in next(row['result'] for row in raw if row.get('op')=='call')['entries']]),flush=True)
        native_log=output.with_suffix('.native-readonly.log')
        with native_log.open('w') as capture:
            native=subprocess.run([str(args.ui_demo.resolve()),'-ApplePersistenceIgnoreState','YES','--components','--transcript-content','--endpoint-config',str(path),'--history-readonly-native-selftest','--bounded-child-id',bounded_child],env=ui_env,stdout=capture,stderr=subprocess.STDOUT,timeout=55)
        text=native_log.read_text(); assert native.returncode==0,str(native_log)+'\n'+text[-5000:]
        marker='NATIVE_AGENT_READONLY_READING_RECEIPT '
        proof=next(json.loads(line[len(marker):]) for line in text.splitlines() if line.startswith(marker))
        assert all(proof[key] for key in ('nativeWindow','gpu','boundedPublicSep','originalPageAndRefreshControls','prependAnchorPreserved','refreshAnchorPreserved','mainDraftPreserved','readOnlyNoComposer','noSubmission','realHarness')),proof
        assert proof['rowCountAfter']>=proof['rowCountBefore']>0 and proof['pageSize']==1 and proof['historyExhausted'],proof
        print('NATIVE_UI_BOUNDED_READING_RECEIPT '+json.dumps(proof,ensure_ascii=False,sort_keys=True),flush=True)
    if args.ui_native_reading and bounded_child is None:
        native_log=output.with_suffix('.native-reading.log')
        with native_log.open('w') as capture:
            native=subprocess.run([str(args.ui_demo.resolve()),'-ApplePersistenceIgnoreState','YES','--components','--transcript-content','--endpoint-config',str(path),'--history-reading-native-selftest'],env=ui_env,stdout=capture,stderr=subprocess.STDOUT,timeout=55)
        native_text=native_log.read_text()
        assert native.returncode==0,str(native_log)+'\n'+native_text[-5000:]
        marker='NATIVE_AGENT_READING_SELFTEST_RECEIPT '
        proof=next(json.loads(line[len(marker):]) for line in native_text.splitlines() if line.startswith(marker))
        assert all(proof[key] for key in ('nativeWindow','publicHistoryPage','publicPageRequest','prependAnchorPreserved','upwardReadingPreserved','focusAndTextStatePreserved','samePublicationRefresh','retainedSessionReadingRestored','independentBReadingRestored','reconnectReadingRestored','draftIsolation','sameEditorOwner','noSubmission','realHarness','readingAnchorAcceptance')),proof
        assert proof['loadedItemsAfter']>proof['loadedItemsBefore']>0,proof
        assert not proof['syntheticPreedit'] and not proof['physicalIme'] and not proof['voiceOver'],proof
        assert not proof['nativePagingControl'] and not proof['markedTextPreserved'] and not proof['opaqueBaselinePreserved'],proof
        print(kind.upper()+'_UI_NATIVE_READING_RECEIPT '+json.dumps(proof,ensure_ascii=False,sort_keys=True),flush=True)
    if args.ui_native_feedback and bounded_child is None:
        native_log=output.with_suffix('.native-feedback.log')
        with native_log.open('w') as capture:
            native=subprocess.run([str(args.ui_demo.resolve()),'--components','--transcript-content','--endpoint-config',str(path),'--feedback-native-selftest'],env=ui_env,stdout=capture,stderr=subprocess.STDOUT,timeout=55)
        native_text=native_log.read_text()
        assert native.returncode==0, str(native_log)+'\n'+native_text[-3000:]
        marker='NATIVE_AGENT_FEEDBACK_SELFTEST_RECEIPT '
        proof=next(json.loads(line[len(marker):]) for line in native_text.splitlines() if line.startswith(marker))
        assert all(proof[key] for key in ('nativeInteraction','nativePointer','nativeTextServices','syntheticPreedit','imeSendAndNavigationBlocked','savedChineseNote','noteReopened','retainedAcrossSessionSwitch','sameRatingDeletes','freshReadAfterDelete','draftsPreserved','nonemptyComposerDrafts','sendEnabledBeforeComposition','noPromptSubmitted','stateDrivenCallbacks')),proof
        assert not proof['physicalIme'] and not proof['voiceOver'] and not proof['readingAnchorAcceptance'],proof
        print(kind.upper()+'_UI_NATIVE_FEEDBACK_RECEIPT '+json.dumps(proof,ensure_ascii=False,sort_keys=True),flush=True)

    native_modes=[]
    if args.ui_native_stream:
        native_modes.append(('stream','--stream-native-selftest','NATIVE_AGENT_STREAM_SELFTEST_RECEIPT ',('nativeWindow','nativePointer','nativeTextServices','nativeAxSelection','activePreview','sameOutputRow','sameTextOwner','selectionRetained','focusRetainedDuringSettlement','oneSubmission','sessionDraftIsolation','reconnectNoReplay','stateDrivenCallbacks')))
    if args.ui_native_run_controls:
        native_modes.append(('run-controls','--run-controls-native-selftest','NATIVE_AGENT_RUN_CONTROLS_SELFTEST_RECEIPT ',('nativeWindow','nativePointer','nativeTextServices','activePreviewBeforeCancel','normalCancelCallback','authoritativeAbortedTurn','cancelReconnectNoReplay','providerFailureInIndicators','providerFailureInTranscript','nativeFailureTextQuery','explicitRecovery','priorFailureCleared','oneSubmissionPerSend','draftIsolation','recoveryReconnectNoReplay','stateDrivenCallbacks')))
    if bounded_child is None:
        for probe_name,flag,marker,required in native_modes:
            model=next(command['args'] for command in config['commands'] if command.get('method')=='session.selectModel')
            commands=[{'op':'create','cwd':str(cwd),'saveAs':'native-probe'},
                      {'op':'call','sessionId':'native-probe','method':'session.selectModel','args':model},
                      {'op':'create','cwd':str(cwd)}]
            fresh=[row['result']['sessionId'] for row in execute(dict(config,commands=commands)) if row.get('op')=='create']
            probe_path=base/('ui-native-'+probe_name+'-'+kind+'.json')
            probe_path.write_text(json.dumps(dict(explicit,sessions=fresh)))
            native_log=output.with_suffix('.native-'+probe_name+'.log')
            with native_log.open('w') as capture:
                native=subprocess.run([str(args.ui_demo.resolve()),'--components','--transcript-content','--endpoint-config',str(probe_path),flag],env=ui_env,stdout=capture,stderr=subprocess.STDOUT,timeout=55)
            native_text=native_log.read_text()
            assert native.returncode==0,str(native_log)+'\n'+native_text[-4000:]
            proof=next(json.loads(line[len(marker):]) for line in native_text.splitlines() if line.startswith(marker))
            assert all(proof[key] for key in required),proof
            if probe_name=='run-controls':assert isinstance(proof.get('reportedError'),str) and proof['reportedError'].strip(),proof
            assert not proof['physicalIme'] and not proof['voiceOver'] and not proof['readingAnchorAcceptance'],proof
            print(kind.upper()+'_UI_NATIVE_'+probe_name.upper().replace('-','_')+'_RECEIPT '+json.dumps(proof,ensure_ascii=False,sort_keys=True),flush=True)

    assert not receipt['isStale'] and receipt['finalAssistantItems']>=1,receipt
    assert output.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'),kind+' UI image missing'
    if bounded_child:
        bounded=receipt['boundedChildProof']
        assert all(bounded[key] for key in ['readOnlyComponent','childScope','originalReadIntent','sourceCallReadonlyDetails','retainedWidgets','reconnect','noComposer','sendRejected','parentDraftPreserved']),bounded
        assert bounded['messages']>=1 and not bounded['readingAnchorAcceptance'],bounded
        print(kind.upper()+'_UI_BOUNDED_CHILD_OK real-child+source-call+readonly-details+explicit-read+retained-widgets+reconnect+no-composer; reading-anchor=false; native-interaction=false',flush=True)
    if (args.ui_turn_footnote and bounded_child is None):
        footer=receipt['turnFootnoteProof'];fork=receipt['turnForkProof']
        assert footer['leadingAside'],footer
        assert all(fork[key] for key in ['sharedCatalog','originalScope','explicitBoundary','singleAdmission','forkHistoryRestored','lateNavigationIgnored','originalDraftPreserved','otherDraftPreserved']),fork
        print(kind.upper()+'_UI_TURN_FOOTNOTE_OK shared-catalog+durable-boundary+single-admission+fork-history+late-navigation-fence+draft-isolation; native-interaction=false',flush=True)
    if (args.ui_transcript_content or bounded_child is not None):
        content=receipt['transcriptContentProof']
        assert content['renderer'] and content['messages']>=1 and content['userTrailing'] and content['machineLeading'] and content['genericViewport'],content
        assert content['outputCopyControls'] >= 1, 'restored completed reply must expose a final-output copy control'
        if args.ui_feedback_read:
            feedback=receipt['feedbackReadProof']
            assert feedback['readCommitted'] and feedback['originalScope'] and feedback['publication']>0 and feedback['finalTargets']>0,feedback
            assert not feedback['mutated'] and not feedback['ratingControls'],feedback
        if args.ui_feedback_write:
            feedback=receipt['feedbackWriteProof']
            assert feedback['mutated'] and all(feedback[key] for key in ('originalScope','duplicateFrameRejected','casConflictCurrent','noteSave','setRatingPreservesNote','newJudgmentClearsNote','sameRatingDeletes','sameValueSet','serverTimestamp','opaqueVersions','freshReadAfterDelete','draftsPreserved')),feedback
            assert not feedback['ratingControls'] and not feedback['nativeInteraction'],feedback
        if args.ui_feedback_transcript:
            feedback=receipt['feedbackTranscriptProof']
            assert all(feedback[key] for key in ('embeddedTranscript','runViewRouting','originalTarget','coldRead','savedChineseNote','noteReopened','sameRatingDeletes','freshReadAfterDelete','draftsPreserved')),feedback
            assert not feedback['nativeInteraction'] and not feedback['readingAnchorAcceptance'],feedback
            assert content['feedbackRows']>=1,content
        if args.ui_feedback_controls:
            feedback=receipt['feedbackControlsProof']
            assert all(feedback[key] for key in ('publicControls','semanticActions','committedNoteReopened','trimmedChineseNote','blankClearsNote','sameRatingDeletes','freshReadAfterDelete','draftPreserved','lightDarkRendered')),feedback
            assert not feedback['nativeInteraction'] and not feedback['physicalIme'] and not feedback['embeddedTranscript'],feedback
        assert not content['keyedReadingAcceptance'] and not content['richContent'] and not content['completeTranscript'],content
        print(kind.upper()+'_UI_TRANSCRIPT_CONTENT_OK canonical-mixed-tree+stable-identities+role-layout+public-generic-viewport; keyed-reading=false; native-interaction=false',flush=True)
    if (args.ui_message_rows and bounded_child is None):
        user=receipt['inputMessageProof'];machine=receipt['outputMessageProof']
        assert user['rightBubble'] and user['externalRightFootnote'] and user['plainReadonlyContent'] and not user['completeTranscript'],user
        assert machine['leadingContent'] and machine['parts']>=1 and not machine['completeTranscript'],machine
        print(kind.upper()+'_UI_MESSAGE_ROWS_OK canonical-user+external-caption+machine-content+public-controls; full-transcript=false; native-interaction=false',flush=True)
    if args.ui_components:
        assert receipt['components'] and not receipt['transcriptView'],receipt
        run=receipt['runViewComposition']
        assert run['mounted'] and run['sharedIntents'] and not run['ownsRuntime'] and not run['completeRunView'],run
        if (args.ui_transcript_content or bounded_child is not None):assert run['transcriptMounted'],run
        assert receipt['inventoryRows']>=1 and receipt['sharedRuntime'] and receipt['sidebarMounted'],receipt
        if bounded_child:
            assert not receipt['summaryMounted'] and receipt['boundedChildProof']['readOnlyComponent'],receipt
        else:
            assert receipt['summaryMounted'],receipt
        document=receipt['documentProjection']
        assert document['sections']>=1 and document['visibleMessages']>=1 and not document['isStale'] and not document['renderer'],document
        assert document['positionedEvents']>=1,document
        assert document['structuredMessages']>=1 and document['structuredMessages']+document['aggregateMessages']==document['visibleMessages'],document
        assert document['unresolvedToolBlocks']==0,document
        unplaced=document['unplacedFacts']
        assert document['unplacedEvents']==len(unplaced),document
        # Pi extension commands can retain a local echo with no native user entry.
        # Unknown positions are explicit, never promoted to a fake source sequence.
        assert all(fact['kind']=='message' and fact['provisional'] and fact['recordId'] is None and fact['sourceSequence'] is None for fact in unplaced),document
        if history_pages:
            proof=receipt['historyPagingProof']
            assert all(proof[key] for key in ('originalScope','extended','frozenCut','otherDraftPreserved','sourceStatusPublished','newEpochHistoryRetained','noDuplicateMessages','oldReceiptInvalidated')),proof
            assert proof['retainedMessages']>=1 and not proof['nativeInteraction'] and not proof['readingAnchorAcceptance'],proof
        if (args.ui_actionables and bounded_child is None):
            proof=receipt['actionableProof']
            assert all(proof[key] for key in ('probeConfigured','publicComposer','publicCard','originalScope','oneAttempt','sharedResolved','canonicalAnswerRecord','otherDraftPreserved','freshHistoryAfterReconnect','reconnectNoDuplicateInput')),proof
            assert proof['followupInputCount']==1 and not proof['nativeInteraction'] and not proof['physicalIME'],proof
        if (args.ui_model_choices and bounded_child is None):
            proof=receipt['modelChoiceProof']
            assert proof['catalogChoices']>=1 and proof['explicitSelection'] and proof['configurationReceipt'] and proof['promptSettled'] and not proof['nativeInteraction'],proof
        if (args.ui_context and bounded_child is None) or (args.ui_annotation_editor and bounded_child is None):
            proof=receipt['contextProof']
            assert proof['mountedCards'] and proof['explicitRemoval'] and proof['referenceReturned'] and proof['annotationReturned'] and proof['promptSettled'] and proof['clearedDraft'],proof
            assert not proof['fileUpload'] and not proof['nativeInteraction'] and not proof['structuredTranscript'],proof
        if (args.ui_annotation_editor and bounded_child is None):
            proof=receipt['annotationProof']
            assert proof['componentEditor'] and proof['committedComment'] and proof['sourceRetained'] and proof['sharedCompilation'] and proof['promptSettled'],proof
            assert not proof['nativeInteraction'] and not proof['physicalIME'] and not proof['sourceHighlight'],proof
        if (args.ui_initial_presets and bounded_child is None):
            proof=receipt['initialPresetProof']
            assert proof['catalogChoices']>=1 and proof['explicitSelection'] and proof['createdSession'] and proof['singleInput'] and proof['promptSettled'] and proof['originalDraftPreserved'],proof
            if kind!='pi': assert proof['presetIdentityReturned'],proof
            assert not proof['nativeInteraction'] and proof['componentCreationEntry'],proof
        if (args.ui_attachments and bounded_child is None):
            proof=receipt['uploadProof']
            assert all(proof[key] for key in ('publicEntry','hostBoundedFileRead','receiptValidated','originalDraft','otherDraftPreserved','oneShotUpload','componentSend','promptSettled','reconnectNoDuplicate')),proof
            assert proof['durableInputCount']==1 and (proof['historyFileMetadata'] or proof['historyFileReference']),proof
            assert proof['attachmentOnlySend'] and proof['attachmentOnlyVisible'] and proof['attachmentOnlyInputCount']==1 and proof['freshJournalAfterReconnect'],proof
            assert not proof['nativeInteraction'] and not proof['inlineImage'],proof
            if kind in ('native','pi'): assert proof['publicByteRead'] and proof['foreignSessionReadRejected'],proof
            else: assert proof['readUnavailable'] and not proof['publicByteRead'],proof
        print(kind.upper()+('_UI_BOUNDED_COMPONENTS_RECEIPT ' if bounded_child else '_UI_COMPONENTS_RECEIPT ')+json.dumps(receipt,sort_keys=True))
        print(kind.upper()+'_UI_COMPONENTS_LIVE_OK explicit-SEP+shared-runtime+restored-data+Composer+Indicators+Sidebar+Summary+GPU; transcript-view=false; native-interaction=false')
    else:
        print(kind.upper()+'_UI_CONSUMER_LIVE_OK explicit-SEP+RunController+restored-final-answer+TekesUI-offscreen; native-interaction=false')

try:
    with tempfile.TemporaryDirectory(prefix='agentkit-three-endpoints-') as tmp:
        base=Path(tmp);cwd=base/'project';cwd.mkdir();(cwd/'proof.txt').write_text('proof line one\nline two\n')
        if args.pi_root:
            pi=args.pi_root.resolve();data=base/'pi';(data/'agent').mkdir(parents=True)
            (data/'agent/models.json').write_text(json.dumps({'providers':{'endpoint-test':{'baseUrl':base_url,'api':'openai-completions','apiKey':'synthetic-key','models':[{'id':'endpoint-test','name':'Endpoint test','contextWindow':32768,'maxTokens':1024,'input':['text','image'],**({'reasoning':True} if args.ui_model_choices else {})}]}}}))
            config={'kind':'pi','executable':str(pi/'node/bin/node'),'arguments':[str(pi/'node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js')],'dataRoot':str(data),'environment':{'HOME':str(base/'pi-home'),'PATH':os.environ.get('PATH','/usr/bin:/bin')},'commands':commands(cwd,'endpoint-test','endpoint-test')}
            Path(config['environment']['HOME']).mkdir()
            extension=base/'question.mjs'
            extension.write_text("export default function(pi){pi.registerCommand('endpoint-question',{handler:async(args,ctx)=>{const value=await ctx.ui.select('Endpoint question',['yes','no']);pi.appendEntry('endpoint-answer',{value});}});pi.registerCommand('endpoint-ui-question',{handler:async(args,ctx)=>{const value=await ctx.ui.select('AgentKit UI question',['yes','no']);pi.appendEntry('endpoint-ui-answer',{value});}});pi.registerCommand('endpoint-settled',{handler:async()=>{pi.appendEntry('tekes.run.v1',{id:'endpoint-settled-turn',phase:'settled'});}});}")
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
            ui_smoke(config,next(row['result']['sessionId'] for row in outputs if row.get('op')=='create'))

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
                # Explicit opt-in for this isolated protocol probe only. DSH's
                # documented Web default is openAt=never; leave it untouched otherwise.
                *([{'id':'session-query-sqlite','config':{'path':':memory:','openAt':'first-search'}}] if args.ui_sidebar_query else []),
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
                question_result=execute(dict(config,commands=[{'op':'open','sessionId':session_id},
                    {'op':'promptAndAnswer','sessionId':session_id,'text':'DSH_ACTIONABLE_PROOF','outcome':{'sessionId':session_id,'answer':{'answers':[{'id':'dsh-question-proof','selected':['continue-proof']}]}}},
                    {'op':'waitSettled','sessionId':session_id},{'op':'snapshot'}]))
                assert not question_result[-1]['result']['actionables'],'DSH question remained pending after shared answer'

                question_state=question_result[-1]['result']['sessions'][session_id]
                question_tools=[entry['event']['data'] for entry in question_state['entries'].values() if entry['event']['type']=='tool/result' and 'continue-proof' in json.dumps(entry['event']['data'].get('message',{}))]
                assert len(question_tools)==1 and not question_tools[0].get('error') and question_tools[0].get('message',{}).get('isError') is not True,'DSH did not return the accepted structured answer to the model'

                print('DSH_ACTIONABLE_ENVELOPE_LIVE_OK public-question+shared-session-envelope+waterfall-result+settlement')

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
                ui_smoke(config,session_id)
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
                if args.ui_bounded_child:assert 'subagents.history' in caps['methods'],'Native endpoint does not publish bounded child history'
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
                    assert len(histories)==2 and all(any(item['text']==native_child_reply and item['isFinal'] for item in h['items']) for h in histories),histories
                    assert all(h['tools'][0]['arguments']['task_name']=='Bounded child proof' for h in histories),'missing parent source-call projection'
                    print('NATIVE_BOUNDED_CHILD_LIVE_OK real-task+source-call+read-only-history+reconnect+ownership')
                    if args.ui_bounded_child:
                        child_id=next(row['result']['entries'][1]['id'] for row in child_result if row.get('op')=='childCatalog')
                        ui_smoke(config,parent,child_id)
                print('NATIVE_COMPLETE_LIFECYCLE_LIVE_OK models+commands+skills+settings+presets+upload+scratch-discard+rename+archive')
                ui_smoke(config,native_id)

            finally:
                process.stdin.close()
                try:process.wait(timeout=15)
                except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=10)
finally:
    server.shutdown();server.server_close()
