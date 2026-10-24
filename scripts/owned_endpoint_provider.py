"""Shared synthetic loopback provider for owned acceptance profiles.

This is a fixture protocol, not an AgentKit state machine. Explicit inputs avoid
importing an executable test script or reading a user's provider credentials.
"""
import http.server
import json
import time
from native_probe_provider import native_probe_intent, directory_probe_path
from mixed_tool_provider import frames as mixed_tool_frames


def provider_handler(*, workspace, requests, child_run=False, mixed_tools=False,
                     native_readonly=False, native_child_reply='ENDPOINT_REPLY_OK'):
    """Return the original handler with profile inputs bound once.

    workspace is a getter because the legacy runner starts its server before
    allocating the owned work directory. Requests records paths only, not bodies.
    """
    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def do_POST(self):
            body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));requests.append(self.path)
            intent=native_probe_intent(body)
            native_cancel=intent=='ENDPOINT_CANCEL_NATIVE_PROOF'
            if child_run and native_cancel:print('CHILD_CANCEL_PROVIDER_REQUEST '+self.path,flush=True)
            if intent=='ENDPOINT_ERROR_NATIVE_PROOF':
                error=json.dumps({'error':{'message':'ENDPOINT_ERROR_NATIVE_PROOF: controlled invalid request','type':'invalid_request_error','code':'invalid_request_error'}}).encode()
                self.send_response(400);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(error)));self.end_headers();self.wfile.write(error);return
            if 'ENDPOINT_QUEUE_HOLD' in json.dumps(body):time.sleep(0.6)
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
            mixed=mixed_tool_frames(body,(workspace()/'proof.txt').resolve(),self.path.endswith('/responses')) if mixed_tools else None
            if mixed is not None:
                history=body.get('input',body.get('messages',[]))
                print('MIXED_PROVIDER_REQUEST_RECEIPT '+json.dumps({'path':self.path,'inputItems':[
                    {key:item[key] for key in ('type','role','call_id','tool_call_id') if key in item}
                    for item in history if isinstance(item,dict)],'frames':len(mixed)}),flush=True)
                try:
                    for frame in mixed:
                        self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush();time.sleep(0.12)
                    if not self.path.endswith('/responses'):self.wfile.write(b'data: [DONE]\n\n')
                    self.wfile.flush()
                except (BrokenPipeError,ConnectionResetError):pass
                return
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
                if native_readonly and any(message.get('role')=='user' and 'CHILD_NATIVE_PROOF' in json.dumps(message) and 'ENDPOINT_NATIVE_CHILD' not in json.dumps(message) for message in body.get('input',[]) if isinstance(message,dict)):
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
                change=directory_probe_path(body)
                if change and not any(message.get('role')=='tool' for message in body.get('messages',[])):
                    frames=[dict(common,choices=[{'index':0,'delta':{'role':'assistant','tool_calls':[{'index':0,'id':'directory-call','type':'function','function':{'name':'working_directory','arguments':json.dumps({'cd':change})}}]},'finish_reason':None}]),dict(common,choices=[{'index':0,'delta':{},'finish_reason':'tool_calls'}])]
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
            reading_stream='ENDPOINT_READING_STREAM_PROOF' in json.dumps(body)
            if reading_stream:
                chunks=['READING_STREAM_BEGIN\n','流式第一段\n'*60,'流式第二段\n'*60,'READING_STREAM_END']
                if self.path.endswith('/responses'):
                    frames=[{'type':'response.output_item.added','output_index':0,'item':{'id':'reading-native-output','type':'message','role':'assistant','phase':'final_answer','content':[]}},
                        *[{'type':'response.output_text.delta','output_index':0,'content_index':0,'delta':chunk} for chunk in chunks],
                        {'type':'response.completed','response':{'id':'reading-native-response','status':'completed','output':[{'id':'reading-native-output','type':'message','role':'assistant','phase':'final_answer','status':'completed','content':[{'type':'output_text','text':''.join(chunks)}]}],'usage':{'input_tokens':10,'output_tokens':600,'total_tokens':610}}}]
                else:
                    frames=[dict(common,choices=[{'index':0,'delta':dict(content=chunk,**({'role':'assistant'} if i==0 else {})),'finish_reason':None}]) for i,chunk in enumerate(chunks)]
                    frames.append(dict(common,choices=[{'index':0,'delta':{},'finish_reason':'stop'}],usage={'prompt_tokens':10,'completion_tokens':600,'total_tokens':610}))
            try:
                for frame in frames:
                    self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush()
                    if reading_stream:time.sleep(1.5)
                    # Keep this owned cancellation stream open beyond the UI's
                    # bounded Stop observation deadline; the real harness must
                    # terminate it through its public cancel route.
                    if native_cancel:time.sleep(20 if child_run else 1)
                    if 'ENDPOINT_STREAM_IDENTITY_PROOF' in json.dumps(body):time.sleep(0.5)
                    if 'ENDPOINT_STREAM_NATIVE_PROOF' in json.dumps(body):time.sleep(1)
                if not self.path.endswith('/responses'):self.wfile.write(b'data: [DONE]\n\n')
            except (BrokenPipeError,ConnectionResetError):pass
    return Provider
