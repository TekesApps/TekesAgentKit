"""Owned read-only model fixture; actual tools still run inside each harness."""
import json

MARKER = 'ENDPOINT_MIXED_TOOL_RUN_PROOF'
REASONING = 'MIXED_REASONING_OWNED'
FINAL = 'ENDPOINT_REPLY_OK MIXED_FINAL_OWNED'


def frames(body, path, responses=False):
    history = body.get('input', body.get('messages', []))
    results = {item.get('call_id', item.get('tool_call_id')) for item in history
               if isinstance(item, dict) and (item.get('type') == 'function_call_output' or item.get('role') == 'tool')}
    if MARKER not in json.dumps(body) and not any(str(call).startswith('mixed-') for call in results):
        return None
    phase = 2 if 'mixed-c' in results else 1 if {'mixed-a', 'mixed-b'} <= results else 0
    tools = [tool.get('function', tool) for tool in body.get('tools', [])]
    names = {tool.get('name') for tool in tools}
    name = 'read' if 'read' in names else 'str_replace_editor' if 'str_replace_editor' in names else None
    if phase < 2 and name is None:
        raise ValueError('owned mixed fixture requires a public read tool')
    selected = next((tool for tool in tools if tool.get('name') == name), {})
    properties = selected.get('parameters', {}).get('properties', {})
    path_key = 'file_path' if 'file_path' in properties else 'path' if 'path' in properties else None
    if phase < 2 and path_key is None:
        raise ValueError('owned read tool must advertise its path parameter')
    arguments = {path_key: str(path)} if path_key else {}
    if name == 'str_replace_editor':
        arguments['command'] = 'view'
    for key in selected.get('parameters', {}).get('required', []):
        if key not in arguments:
            if key == 'offset':arguments[key] = 1
            elif key == 'limit':arguments[key] = 2
            else:raise ValueError('owned read tool has unsupported required argument: '+key)
    calls = ['mixed-a', 'mixed-b'] if phase == 0 else ['mixed-c'] if phase == 1 else []
    if responses:
        out = []
        if phase == 0:
            out += [
                {'type': 'response.output_item.added', 'output_index': 0,
                 'item': {'id': 'mixed-thought', 'type': 'reasoning', 'summary': []}},
                {'type': 'response.reasoning_summary_text.delta', 'output_index': 0,
                 'summary_index': 0, 'delta': REASONING},
            ]
        completed = ([{'id': 'mixed-thought', 'type': 'reasoning', 'status': 'completed',
                       'summary': [{'type': 'summary_text', 'text': REASONING}]}] if phase == 0 else [])
        for index, call in enumerate(calls, start=len(completed)):
            out += [
                {'type': 'response.output_item.added', 'output_index': index,
                 'item': {'type': 'function_call', 'call_id': call, 'name': name, 'arguments': ''}},
                {'type': 'response.function_call_arguments.delta', 'output_index': index,
                 'delta': json.dumps(arguments)},
                {'type': 'response.function_call_arguments.done', 'output_index': index,
                 'arguments': json.dumps(arguments)},
            ]
            completed.append({'type': 'function_call', 'call_id': call, 'name': name,
                              'arguments': json.dumps(arguments), 'status': 'completed'})
        output = completed if calls else [{'type': 'message', 'role': 'assistant', 'phase': 'final_answer',
                                    'status': 'completed', 'content': [{'type': 'output_text', 'text': FINAL}]}]
        out.append({'type': 'response.completed', 'response': {'id': f'mixed-response-{phase}',
                    'status': 'completed', 'output': output,
                    'usage': {'input_tokens': 10, 'output_tokens': 20, 'total_tokens': 30}}})
        return out
    common = {'id': f'mixed-response-{phase}', 'object': 'chat.completion.chunk', 'created': 1, 'model': 'endpoint-test'}
    out = []
    if phase == 0:
        out.append(dict(common, choices=[{'index': 0, 'delta': {'role': 'assistant', 'reasoning_content': REASONING}, 'finish_reason': None}]))
    for index, call in enumerate(calls):
        out.append(dict(common, choices=[{'index': 0, 'delta': {'role': 'assistant', 'tool_calls': [
            {'index': index, 'id': call, 'type': 'function', 'function': {'name': name, 'arguments': json.dumps(arguments)}}]}, 'finish_reason': None}]))
    if not calls:
        out.append(dict(common, choices=[{'index': 0, 'delta': {'role': 'assistant', 'content': FINAL}, 'finish_reason': None}]))
    out.append(dict(common, choices=[{'index': 0, 'delta': {}, 'finish_reason': 'tool_calls' if calls else 'stop'}],
                    usage={'prompt_tokens': 10, 'completion_tokens': 20, 'total_tokens': 30}))
    return out
