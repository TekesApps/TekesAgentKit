"""Owned provider and strict evidence checks for real Native child pagination."""
from __future__ import annotations

import io
import json
from pathlib import Path
from typing import Any

from native_child_pane_source_fixture import MARKER, is_native_child_request
from owned_endpoint_provider import provider_handler

PREFIX = 'AGENTKIT_NATIVE_PAGED_SOURCE '
CHILD_MARKER = MARKER
STEP_TEXT = tuple(f'NATIVE_PAGED_STEP_{n}' for n in range(1, 5))
def phases_for_pages(earlier_reads: int) -> tuple[str, ...]:
    _require(type(earlier_reads) is int and 1 <= earlier_reads <= 16,
             'public page plan requires one to sixteen Earlier reads')
    return ('running_head', 'reconnected_refresh',
            *(f'reconnected_page_{index}' for index in range(1, earlier_reads + 1)),
            'settled_refresh')


EVIDENCE_SCOPE = 'actual-native-paged-child-original-intents'


def _require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def _tool_specs(body: object) -> list[dict[str, Any]]:
    """Return the public tool declarations supplied to this model request."""
    _require(isinstance(body, dict), 'provider request is not an object')
    tools = body.get('tools')
    _require(isinstance(tools, list), 'provider request omitted public tools schema')
    return [tool for tool in tools if isinstance(tool, dict)]


def _strings(value: object) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, list):
        return [text for item in value for text in _strings(item)]
    if isinstance(value, dict):
        return [text for item in value.values() for text in _strings(item)]
    return []


def _validate_prior_read_result(body: object, expected_call_id: str, proof_text: str) -> bool:
    _require(isinstance(body, dict) and isinstance(body.get('input'), list),
             'Native read continuation omitted public Responses input history')
    outputs = [item for item in body['input'] if isinstance(item, dict)
               and item.get('type') == 'function_call_output'
               and item.get('call_id') == expected_call_id]
    _require(len(outputs) == 1 and any(proof_text in text
                                      for text in _strings(outputs[0].get('output'))),
             'public child history did not contain the previous real read(proof.txt) result')
    return True


def _read_arguments(body: object, proof_path: Path) -> tuple[str, dict[str, Any]]:
    """Derive read arguments from the request's declared JSON schema.

    This intentionally refuses undocumented/ambiguous schemas. The path is
    accepted only when the public schema declares one required string path.
    """
    matches = [tool for tool in _tool_specs(body)
               if tool.get('name') == 'read'
               or (tool.get('function') or {}).get('name') == 'read']
    _require(len(matches) == 1, 'request must declare exactly one public read tool')
    tool = matches[0]
    function = tool.get('function') if isinstance(tool.get('function'), dict) else tool
    schema = function.get('parameters')
    _require(isinstance(schema, dict) and isinstance(schema.get('properties'), dict),
             'public read tool has no parameter object schema')
    props = schema['properties']
    required = schema.get('required', [])
    _require(isinstance(required, list), 'public read required list is malformed')
    candidates = [name for name, spec in props.items()
                  if name in required and isinstance(spec, dict)
                  and spec.get('type') == 'string'
                  and name.lower() in ('path', 'file_path', 'filepath', 'file')]
    _require(len(candidates) == 1,
             'public read schema must identify one required string path property')
    name = candidates[0]
    arguments = {name: proof_path.name}
    for key in required:
        if key == name:
            continue
        _require(key in ('offset', 'limit') and isinstance(props.get(key), dict),
                 'unsupported required public read argument: ' + str(key))
        arguments[key] = 1 if key == 'offset' else 2
    return name, arguments


def is_paged_child_request(body: object) -> bool:
    """Route this owned Responses continuation by its explicit public identity."""
    if is_native_child_request(body):
        return True
    if not isinstance(body, dict):
        return False
    previous = body.get('previous_response_id')
    if isinstance(previous, str) and previous in {
            f'native-paged-step-{index}' for index in range(1, 5)}:
        return True
    inputs = body.get('input')
    return isinstance(inputs, list) and any(isinstance(item, dict)
        and item.get('type') == 'function_call_output'
        and item.get('call_id') in {f'native-paged-read-{index}' for index in range(1, 5)}
        for item in inputs)


def paged_native_provider(*, workspace: Path, requests: list[str], gate: Any):
    """Serve four real read-tool child steps, then hold the fifth request.

    Bodies are inspected in memory only. The ordinary endpoint provider owns
    all unrelated calls and records only request paths.
    """
    original = provider_handler(workspace=lambda: workspace, requests=requests)
    state = {'childRequests': 0, 'stepReceipts': [], 'gateResponseStatus': None,
             'gateErrorCode': None}

    class Provider(original):
        def do_POST(self):
            stream = self.rfile
            raw = stream.read(int(self.headers.get('Content-Length', '0')))
            try:
                body = json.loads(raw)
            except (ValueError, TypeError):
                body = None
            self.rfile = io.BytesIO(raw)
            try:
                if not is_paged_child_request(body):
                    return original.do_POST(self)
                state['childRequests'] += 1
                index = state['childRequests']
                if index <= 4:
                    _require(isinstance(body, dict) and self.path.endswith('/responses'),
                             'Native child must use the public Responses endpoint')
                    name, args = _read_arguments(body, workspace / 'proof.txt')
                    call_id = f'native-paged-read-{index}'
                    prior_read = False
                    if index > 1:
                        prior_read = _validate_prior_read_result(
                            body, f'native-paged-read-{index - 1}',
                            (workspace / 'proof.txt').read_text(encoding='utf-8').strip())
                    state['stepReceipts'].append({'step': index, 'path': self.path,
                        'tool': 'read', 'argumentName': name, 'requestedPath': args[name],
                        'proofPath': str(workspace / 'proof.txt'),
                        'priorReadResultObserved': prior_read,
                        'requestBodyPersisted': False})
                    text = STEP_TEXT[index - 1]
                    message = {'id': f'native-paged-message-{index}', 'type': 'message',
                        'role': 'assistant', 'phase': 'commentary', 'status': 'completed',
                        'content': [{'type': 'output_text', 'text': text}]}
                    function = {'type': 'function_call', 'call_id': call_id, 'name': 'read',
                                'arguments': json.dumps(args)}
                    events = [
                        {'type': 'response.output_item.added', 'output_index': 0, 'item': dict(message, status='in_progress', content=[])},
                        {'type': 'response.output_text.delta', 'output_index': 0, 'content_index': 0, 'delta': text},
                        {'type': 'response.output_text.done', 'output_index': 0, 'content_index': 0, 'text': text},
                        {'type': 'response.output_item.done', 'output_index': 0, 'item': message},
                        {'type': 'response.output_item.added', 'output_index': 1, 'item': dict(function, status='in_progress', arguments='')},
                        {'type': 'response.function_call_arguments.delta', 'output_index': 1, 'delta': json.dumps(args)},
                        {'type': 'response.function_call_arguments.done', 'output_index': 1, 'arguments': json.dumps(args)},
                        {'type': 'response.output_item.done', 'output_index': 1, 'item': function},
                        {'type': 'response.completed', 'response': {
                            'id': f'native-paged-step-{index}', 'status': 'completed',
                            'output': [message, function],
                            'usage': {'input_tokens': 1, 'output_tokens': 1, 'total_tokens': 2}}},
                    ]
                    encoded = ''.join('data: ' + json.dumps(event) + '\n\n'
                                      for event in events).encode()
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/event-stream')
                    self.send_header('Content-Length', str(len(encoded)))
                    self.end_headers()
                    self.wfile.write(encoded)
                    return
                if index == 5:
                    _require(len(state['stepReceipts']) == 4,
                             'fifth request arrived before all four public read steps')
                    _validate_prior_read_result(
                        body, 'native-paged-read-4',
                        (workspace / 'proof.txt').read_text(encoding='utf-8').strip())
                    requests.append(self.path)
                    result = gate._wait_for_release()
                    if result == 'explicitly_released':
                        payload, status = {'error': {'message': 'NATIVE_PAGED_GATE: controlled invalid request',
                            'type': 'invalid_request_error', 'code': 'invalid_request_error'}}, 400
                    elif result == 'timed_out':
                        payload, status = {'error': {'message': 'failure gate timed out',
                            'type': 'timeout_error'}}, 504
                    else:
                        payload, status = {'error': {'message': 'failure gate aborted',
                            'type': 'aborted_error'}}, 503
                    state['gateResponseStatus'] = status
                    state['gateErrorCode'] = payload.get('error', {}).get('code')
                    encoded = json.dumps(payload).encode()
                    self.send_response(status)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(encoded)))
                    self.end_headers()
                    self.wfile.write(encoded)
                    return
                raise ValueError('unexpected extra Native child provider request')
            finally:
                self.rfile.close()
                self.rfile = stream

    return Provider, state


def normalized_entry(entry: object) -> object:
    """Normalize only serde's omitted optional None fields described by contract."""
    if not isinstance(entry, dict):
        return entry
    result = {key: _deep_copy(value) for key, value in entry.items()}
    result.pop('view', None) if result.get('view') is None else None
    result.pop('isModelOnly', None) if result.get('isModelOnly') is None else None
    event = result.get('event')
    if isinstance(event, dict) and event.get('ignorable') is None:
        event.pop('ignorable', None)
    return result


def _deep_copy(value: object) -> object:
    if isinstance(value, dict):
        return {key: _deep_copy(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_deep_copy(item) for item in value]
    return value


def _scope(parent: object, child_id: object) -> dict[str, Any]:
    _require(isinstance(parent, dict) and parent.get('endpointId') == 'native'
             and bool(parent.get('threadId')) and parent.get('threadId') == parent.get('sessionId'),
             'invalid native parent scope')
    _require(isinstance(child_id, str) and child_id and child_id != parent.get('sessionId'),
             'invalid bounded child ID')
    return dict(parent, sessionId=child_id)


def _validate_intent_binding(intent: object, snapshot: object, expected_read: str,
                             scope: dict[str, Any], phase: str) -> None:
    _require(isinstance(intent, dict) and isinstance(snapshot, dict),
             phase + ': read intent/snapshot malformed')
    read_epoch, read_publication = intent.get('connectionEpoch'), intent.get('publication')
    snapshot_epoch, snapshot_publication = snapshot.get('connectionEpoch'), snapshot.get('publication')
    _require(intent.get('read') == expected_read and intent.get('scope') == scope
             and type(read_publication) is int and type(snapshot_publication) is int
             and 0 < read_publication < snapshot_publication
             and type(read_epoch) is int and type(snapshot_epoch) is int
             and (read_epoch == snapshot_epoch if expected_read == 'earlier'
                  else read_epoch <= snapshot_epoch),
             phase + ': original intent does not precede/bind its returned publication and epoch')


def _entry_sequence(entry: object) -> int:
    _require(isinstance(entry, dict) and isinstance(entry.get('event'), dict),
             'history entry missing public event')
    seq = entry['event'].get('seq')
    _require(type(seq) is int and seq >= 0, 'invalid public history sequence')
    return seq


def _append_origin(entry: object) -> bool:
    if not isinstance(entry, dict) or not isinstance(entry.get('event'), dict):
        return False
    event = entry['event']
    return event.get('type') in ('user/message', 'assistant/message') and event.get('surfaceOp') == 'append'


def validate_public_history(head: object, one_page: object, *, parent: dict, child_id: str,
                            marker: str = CHILD_MARKER) -> dict[str, Any]:
    """Validate independent public subagents.history groundtruth before opening the pane."""
    _require(isinstance(head, dict) and isinstance(one_page, dict),
             'public subagents.history results must be objects')
    for page in (head, one_page):
        _require(page.get('parentSessionId') == parent['sessionId']
                 and page.get('childSessionId') == child_id
                 and type(page.get('throughSequence')) is int
                 and isinstance(page.get('historyRevision'), str)
                 and isinstance(page.get('entries'), list)
                 and page.get('runOpen') is True,
                 'public child history identity/metadata mismatch')
    _require(head['throughSequence'] == one_page['throughSequence']
             and head['historyRevision'] == one_page['historyRevision'],
             'full and one-message public heads have different frozen cuts')
    full_entries = head['entries']
    _require(bool(full_entries) and head.get('hasMoreBefore') is False,
             'public full child head lacks the four completed steps')
    append_messages = [entry for entry in full_entries if _append_origin(entry)]
    _require(len(append_messages) >= 4,
             'public full head lacks four append-origin messages')
    _require(one_page.get('hasMoreBefore') is True and bool(one_page['entries']),
             'public maxMessages=1 head did not prove more history')
    seqs = [_entry_sequence(entry) for entry in full_entries]
    _require(seqs == sorted(set(seqs)), 'public complete head sequences are not unique and ordered')
    append_text = json.dumps([entry for entry in append_messages
                              if entry['event']['type'] == 'assistant/message'], ensure_ascii=False)
    _require(isinstance(head.get('sourceCall'), dict)
             and marker in json.dumps(head['sourceCall'].get('arguments'), ensure_ascii=False)
             and head['sourceCall'].get('phase') == 'running',
             'public complete head omitted original child task source marker')
    for text in STEP_TEXT:
        _require(text in append_text, 'public complete head omitted append-origin assistant ' + text)
    one_entries = one_page['entries']
    _require(len(one_entries) <= len(full_entries)
             and [normalized_entry(item) for item in one_entries]
             == [normalized_entry(item) for item in full_entries[-len(one_entries):]],
             'maxMessages=1 head is not the exact causal-event suffix of complete public history')
    _require(any(_append_origin(entry) for entry in one_entries),
             'maxMessages=1 page contains no append-origin message')
    return {'parentSessionId': parent['sessionId'], 'childSessionId': child_id,
            'throughSequence': head['throughSequence'], 'historyRevision': head['historyRevision'],
            'entries': full_entries, 'entryCount': len(full_entries), 'hasMoreBefore': False,
            'onePage': one_page}


def validate_public_page_plan(groundtruth: dict[str, Any], pages: list[dict[str, Any]],
                              requests: list[dict[str, Any]]) -> dict[str, Any]:
    _require(2 <= len(pages) <= 17 and len(requests) == len(pages), 'invalid bounded public page plan')
    _compare_entries(pages[0]['entries'],groundtruth['onePage']['entries'])
    union: dict[int, object] = {}
    before = None
    for index,(request,page) in enumerate(zip(requests,pages)):
        _require(request.get('parentSessionId') == groundtruth['parentSessionId']
                 and request.get('childSessionId') == groundtruth['childSessionId']
                 and request.get('maxMessages') == 1
                 and page.get('parentSessionId') == groundtruth['parentSessionId']
                 and page.get('childSessionId') == groundtruth['childSessionId']
                 and page.get('throughSequence') == groundtruth['throughSequence']
                 and page.get('historyRevision') == groundtruth['historyRevision']
                 and page.get('hasMoreBefore') is (index < len(pages)-1),
                 'public page plan scope/cut/revision/EOF mismatch')
        if index:
            _require(request.get('throughSequence') == groundtruth['throughSequence']
                     and request.get('historyRevision') == groundtruth['historyRevision']
                     and request.get('beforeSequence') == before,
                     'public Earlier request does not use the exact frozen frontier')
        else:
            _require(all(request.get(key) is None for key in
                         ('throughSequence','historyRevision','beforeSequence')),
                     'public initial head unexpectedly has a frozen frontier')
        entries=page.get('entries')
        _require(isinstance(entries,list) and bool(entries),'public page made no progress')
        seqs=[_entry_sequence(entry) for entry in entries]
        _require(seqs == sorted(set(seqs)) and (before is None or max(seqs) < before),
                 'public page plan overlaps or regresses its frontier')
        union.update(zip(seqs,entries));before=min(seqs)
    _compare_entries([union[seq] for seq in sorted(union)],groundtruth['entries'])
    return dict(groundtruth,pagePlan=pages,pagePlanRequests=requests)


def _validate_projected_content(source: dict[str, Any], snapshot: dict[str, Any]) -> None:
    transcript=snapshot.get('transcript',{})
    items,tools=transcript.get('items'),transcript.get('tools')
    _require(isinstance(items,list) and isinstance(tools,list),'projected content arrays missing')
    for entry in source['projectedEntries']:
        event=entry['event']; data=event.get('data',{});seq=event['seq']
        if event['type'] == 'assistant/message' and event.get('surfaceOp') == 'append':
            message=data.get('message',{})
            text=''.join(part.get('text','') for part in message.get('content',[])
                         if part.get('type') == 'text')
            matches=[item for item in items if item.get('kind') == 'assistant'
                     and item.get('sourcePosition',{}).get('sequence') == seq]
            _require(len(matches) == 1 and matches[0].get('text') == text
                     and matches[0].get('isVisible') is True
                     and matches[0].get('isProvisional') is False,
                     'projected assistant content differs from its public journal message')
        if event['type'] == 'tool/call':
            matches=[tool for tool in tools if isinstance(tool.get('sourcePosition'),dict)
                     and tool['sourcePosition'].get('sequence') == seq]
            _require(len(matches) == 1 and matches[0].get('name') == data.get('name')
                     and matches[0].get('arguments') == data.get('arguments'),
                     'projected tool call differs from public history')
        if event['type'] == 'tool/result':
            matches=[tool for tool in tools if isinstance(tool.get('resultPosition'),dict)
                     and tool['resultPosition'].get('sequence') == seq]
            _require(len(matches) == 1 and matches[0].get('result') == data.get('message')
                     and matches[0].get('resultMeta') == data.get('meta'),
                     'projected tool result differs from public history')
    call=source['sourceCall'];scope=snapshot['scope']
    turn_id=json.dumps([scope['endpointId'],scope['sessionId'],'task-source',call['callId']],
                       separators=(',',':'),ensure_ascii=False)
    _require(any(turn.get('id') == turn_id and turn.get('status') == call['phase']
                 for turn in transcript.get('turns',[]))
             and any(tool.get('id') == turn_id+':call' and tool.get('phase') == call['phase']
                     and tool.get('arguments') == call.get('arguments') for tool in tools),
             'synthetic task details do not match the public source task identity/status')
    if call['phase'] == 'error':
        _require(any(item.get('kind') == 'error' and item.get('isVisible') is True
                     and item.get('sourcePosition',{}).get('sequence') in {
                         entry['event']['seq'] for entry in source['projectedEntries']
                         if entry['event']['type'] == 'response/error'} for item in items),
                 'public error was not projected visibly')


def _merged_projection(source: dict[str, Any]) -> list[object]:
    merged: dict[int, object] = {}
    for item in source['receipts']:
        if item.get('reset') is True:
            merged.clear()
        if item.get('merged') is True:
            _require(item['response']['kind'] == 'page', 'non-page response was merged')
            for entry in item['response']['value']['entries']:
                seq = _entry_sequence(entry)
                _require(seq not in merged or normalized_entry(merged[seq]) == normalized_entry(entry),
                         'source rewrote an overlapping retained entry')
                merged[seq] = entry
    derived = [merged[seq] for seq in sorted(merged)]
    _compare_entries(source.get('projectedEntries'), derived)
    return derived


def validate_prefix_frames(frames: list[dict[str, Any]], *, groundtruth: dict[str, Any],
                           parent: dict[str, Any], child_id: str) -> dict[str, Any]:
    return _validate_frames(frames, groundtruth, parent, child_id, terminal=False)


def _validate_frames(frames: list[dict[str, Any]], groundtruth: dict[str, Any],
                     parent: dict[str, Any], child_id: str, *, terminal: bool) -> dict[str, Any]:
    pages = groundtruth['pagePlan']
    earlier_reads = len(pages) - 1
    phases = phases_for_pages(earlier_reads)
    expected = phases if terminal else phases[:-1]
    _require([frame.get('phase') for frame in frames] == list(expected),
             'Native source frames do not match the independently frozen public page plan')
    scope = _scope(parent, child_id)
    source_id = frames[0].get('sourceInstance')
    _require(isinstance(source_id, str) and bool(source_id), 'missing original source identity')
    prior_publication, prior_epoch = 0, 0
    root = None
    rows_by_key: dict[str, int] = {}
    prior_receipts: list[dict[str, Any]] = []
    for index, frame in enumerate(frames):
        phase = frame['phase']
        _require(frame.get('parent') == parent and frame.get('childId') == child_id
                 and frame.get('marker') == MARKER and frame.get('sourceInstance') == source_id,
                 phase + ': original source/parent changed')
        witness = frame.get('witness')
        _require(isinstance(witness, dict), phase + ': missing source witness')
        source, snap = witness.get('source'), witness.get('snapshot')
        _require(isinstance(source, dict) and isinstance(snap, dict)
                 and source.get('parentScope') == parent and source.get('childId') == child_id
                 and source.get('pageSize') == 1
                 and source.get('evidenceScope') == 'built-in-bounded-paged-same-local-read'
                 and source.get('serverTransactionalFreshnessProven') is False,
                 phase + ': invalid bounded source identity')
        publication, epoch = snap.get('publication'), snap.get('connectionEpoch')
        _require(snap.get('scope') == scope and type(publication) is int
                 and publication > prior_publication and type(epoch) is int and epoch >= prior_epoch
                 and source.get('connectionEpoch') == epoch
                 and frame.get('mountedSnapshot') == snap,
                 phase + ': stale or mismatched mounted publication')
        expected_read = 'earlier' if phase.startswith('reconnected_page_') else 'refresh'
        _validate_intent_binding(frame.get('intent'), snap, expected_read, scope, phase)
        mounted = frame.get('mounted')
        _require(isinstance(mounted, dict) and mounted.get('control') == 'original-ReadOnlyTranscript'
                 and type(mounted.get('transcriptId')) is int and mounted['transcriptId'] > 0
                 and mounted.get('sourceIdentity') == source_id
                 and mounted.get('parentBindingPreserved') is True
                 and mounted.get('transcript') == snap.get('transcript'),
                 phase + ': wrong original mounted component')
        if root is None:
            root = mounted['transcriptId']
        _require(mounted['transcriptId'] == root, phase + ': original pane root changed')
        keys, ids = mounted.get('rowKeys'), mounted.get('rowIds')
        _require(isinstance(keys, list) and bool(keys) and isinstance(ids, list)
                 and len(keys) == len(ids) and all(isinstance(key, str) and key for key in keys)
                 and len(set(keys)) == len(keys) and all(type(row) is int and row >= 0 for row in ids)
                 and len(set(ids)) == len(ids), phase + ': invalid mounted row mapping')
        for key, row in zip(keys, ids):
            _require(key not in rows_by_key or rows_by_key[key] == row,
                     phase + ': retained row identity changed')
            rows_by_key[key] = row
        _require(isinstance(mounted.get('parentDraft'), str) and bool(mounted['parentDraft'])
                 and mounted['parentDraft'] == frames[0]['mounted']['parentDraft']
                 and mounted.get('parentSelection') == frames[0]['mounted'].get('parentSelection'),
                 phase + ': parent Composer state changed')
        _require(isinstance(snap.get('connection'), dict)
                 and snap['connection'].get('phase') == 'connected'
                 and snap['connection'].get('historyIsStale') is False
                 and snap.get('transcript', {}).get('isStale') is False,
                 phase + ': pane is stale or disconnected')
        receipts = source.get('receipts')
        _require(isinstance(receipts, list) and receipts
                 and receipts[:len(prior_receipts)] == prior_receipts,
                 phase + ': source receipt chain was lost or rewritten')
        _validate_receipt_chain(receipts, parent, child_id)
        _require(receipts[0].get('operation') == 'initialHead'
                 and receipts[0].get('reset') is True and receipts[0].get('merged') is True,
                 phase + ': source lost its original head')
        heads = [item for item in receipts if item['operation'] in ('initialHead','refreshHead')
                 and item['response']['kind'] == 'page']
        latest = heads[-1]['response']['value']
        _require(all(source.get(key) == latest.get(key) for key in
                     ('throughSequence','historyRevision','runOpen','sourceCall'))
                 and receipts[-1]['readEpoch'] == epoch
                 and all(0 < item['readEpoch'] <= epoch for item in receipts)
                 and receipts[-1]['effectiveHasMoreBefore'] == source.get('hasMoreBefore')
                 and all(item.get('reset') is False for item in receipts[1:]),
                 phase + ': source metadata/epoch/reset differs from the actual read chain')
        derived = _merged_projection(source)
        _validate_projected_content(source,snap)
        earlier = [item for item in receipts if item.get('operation') == 'earlier']
        count = earlier_reads if phase == 'settled_refresh' else max(0, index - 1)
        _require(len(earlier) == count, phase + ': unexpected Earlier reads (including stale intent)')
        for position, item in enumerate(earlier, start=1):
            _require(item['request'] == groundtruth['pagePlanRequests'][position],
                     phase + ': original Earlier request differs from frozen public plan')
            _compare_entries(item['response']['value']['entries'], pages[position]['entries'])
        if phase != 'settled_refresh':
            _require(source.get('throughSequence') == groundtruth['throughSequence']
                     and source.get('historyRevision') == groundtruth['historyRevision']
                     and source.get('runOpen') is True,
                     phase + ': pre-gate frozen cut/revision changed')
            _require(source.get('sourceCall') == groundtruth['onePage'].get('sourceCall'),
                     phase + ': source task differs from independent public head')
            covered = {_entry_sequence(entry): entry
                       for page in pages[:count + 1] for entry in page['entries']}
            _compare_entries(derived, [covered[seq] for seq in sorted(covered)])
            _require(source.get('hasMoreBefore') is (count < earlier_reads),
                     phase + ': pagination EOF differs from public page plan')
        else:
            _validate_settled_witness(source, groundtruth)
            _require(source.get('hasMoreBefore') is False and source.get('runOpen') is False
                     and isinstance(source.get('sourceCall'), dict)
                     and source['sourceCall'].get('phase') == 'error',
                     'controlled error did not settle the complete original child')
            previous_call = frames[-2]['witness']['source'].get('sourceCall')
            _require(isinstance(previous_call, dict)
                     and all(previous_call.get(key) == source['sourceCall'].get(key)
                             for key in ('callId', 'name', 'arguments')),
                     'post-gate error changed original task identity')
        if phase == 'reconnected_refresh':
            old = frame.get('oldEarlier')
            _require(isinstance(old, dict) and old.get('rejected') is True
                     and old.get('capturedEpoch') == frames[0]['witness']['snapshot']['connectionEpoch']
                     and old.get('currentEpoch') == epoch and epoch > prior_epoch
                     and isinstance(old.get('error'), str) and 'epoch expired' in old['error']
                     and any(item['readEpoch'] == old['capturedEpoch'] for item in receipts),
                     'reconnect did not reject original old Earlier and retain its actual cached read epochs')
        prior_publication, prior_epoch, prior_receipts = publication, epoch, receipts
    if not terminal:
        _compare_entries(frames[-1]['witness']['source']['projectedEntries'],groundtruth['entries'])
    return {'accepted': True, 'evidenceScope': EVIDENCE_SCOPE,
            'parent':parent, 'childId':child_id, 'sourceInstance':source_id,
            'phaseCount':len(frames), 'earlierReadCount':earlier_reads,
            'publicPagePlanMatched':True, 'groundtruthEntries':len(groundtruth['entries']),
            'desktopInteraction':False,'physicalIme':False,'readingAnchorAcceptance':False}


def validate_receipt(receipt: object, frames: list[dict[str, Any]], *, groundtruth: dict[str, Any],
                     gate: dict[str, Any]) -> dict[str, Any]:
    _require(isinstance(receipt, dict) and receipt.get('accepted') is True
             and receipt.get('evidenceScope') == EVIDENCE_SCOPE, 'host receipt scope mismatch')
    for key in ('openedViaOriginalControl','closedViaOriginalControl','sameSource',
                'oldEarlierRejected','publicPagePlanMatched','parentDraftPreserved',
                'parentSelectionPreserved','retiredWitnessRejected'):
        _require(receipt.get(key) is True,'incomplete original host lifecycle: '+key)
    for key in ('desktopInteraction','physicalIme','readingAnchorAcceptance'):
        _require(receipt.get(key) is False,'profile exceeded evidence scope: '+key)
    _require(gate.get('requestCount') == 1 and gate.get('explicitlyReleased') is True
             and gate.get('aborted') is False and gate.get('timedOut') is False,
             'fifth provider request gate did not complete by explicit release')
    _require(receipt.get('earlierReadCount') == len(groundtruth['pagePlan']) - 1,
             'host Earlier count differs from public page plan')
    parent, child = receipt.get('parent'),receipt.get('childId')
    _require(receipt.get('childScope') == _scope(parent,child),'host child scope mismatch')
    return _validate_frames(frames,groundtruth,parent,child,terminal=True)


def _validate_receipt_chain(receipts: object, parent: dict, child_id: str) -> None:
    previous_epoch = 0
    frontier_entries: dict[int, object] = {}
    for receipt in receipts:
        _require(isinstance(receipt, dict), 'malformed typed page receipt')
        request = receipt.get('request')
        _require(isinstance(request, dict) and request.get('parentSessionId') == parent['sessionId']
                 and request.get('childSessionId') == child_id and request.get('maxMessages') == 1,
                 'typed page request scope/page size mismatch')
        epoch = receipt.get('readEpoch')
        _require(type(epoch) is int and epoch >= previous_epoch,
                 'typed read epoch order regressed')
        previous_epoch = epoch
        response = receipt.get('response')
        _require(isinstance(response, dict) and response.get('kind') in ('page', 'remoteError'),
                 'typed response kind is not the public serialized enum')
        operation = receipt.get('operation')
        if operation == 'initialHead':
            _require(not frontier_entries and receipt.get('reset') is True
                     and request.get('throughSequence') is None
                     and request.get('beforeSequence') is None
                     and request.get('historyRevision') is None,
                     'initial head did not establish the source cut')
        elif operation == 'refreshHead':
            _require(request.get('throughSequence') is None
                     and request.get('beforeSequence') is None
                     and request.get('historyRevision') is None,
                     'head refresh unexpectedly reused a page frontier')
        elif operation == 'earlier':
            _require(frontier_entries and request.get('beforeSequence') == min(frontier_entries),
                     'Earlier did not use the current exclusive page frontier')
        elif operation == 'prefixValidation':
            _require(frontier_entries and request.get('throughSequence') is not None
                     and request.get('historyRevision') is not None
                     and request.get('beforeSequence') is None,
                     'prefix validation did not use the frozen prior cut')
        else:
            raise ValueError('unknown bounded source operation')
        if response['kind'] == 'page':
            value = response.get('value')
            _require(isinstance(value, dict) and value.get('parentSessionId') == parent['sessionId']
                     and value.get('childSessionId') == child_id
                     and isinstance(value.get('entries'), list),
                     'typed page response malformed')
            _require((request.get('throughSequence') is None
                      or value.get('throughSequence') == request.get('throughSequence'))
                     and (request.get('historyRevision') is None
                          or value.get('historyRevision') == request.get('historyRevision')),
                     'typed response changed frozen cut/revision')
            seqs = [_entry_sequence(entry) for entry in value['entries']]
            before = request.get('beforeSequence')
            _require(seqs == sorted(set(seqs)) and (before is None or all(seq < before for seq in seqs)),
                     'typed page violated exclusive frontier or sequence ordering')
            if receipt.get('operation') == 'earlier':
                _require(before is not None and receipt.get('merged') is True
                         and receipt.get('reset') is False and bool(seqs),
                         'Earlier receipt did not make a real non-reset merge')
            elif receipt.get('operation') == 'prefixValidation':
                _require(before is None and receipt.get('merged') is False
                         and receipt.get('reset') is False,
                         'prefix validation was merged or reset the source')
            if receipt.get('reset') is True:
                frontier_entries.clear()
            if receipt.get('merged') is True:
                frontier_entries.update((_entry_sequence(entry), entry) for entry in value['entries'])
        else:
            _require(isinstance(response.get('value'), dict), 'typed remote error value malformed')
            _require(receipt.get('operation') == 'prefixValidation'
                     and receipt.get('merged') is False and receipt.get('reset') is False,
                     'remote error was not a non-merging prefix validation')


def _validate_settled_witness(source: dict[str, Any], groundtruth: dict[str, Any]) -> None:
    old_cut, old_revision = groundtruth['throughSequence'], groundtruth['historyRevision']
    _require(type(source.get('throughSequence')) is int and source['throughSequence'] > old_cut
             and isinstance(source.get('historyRevision'), str)
             and source['historyRevision'] != old_revision,
             'post-gate error did not advance the public history cut and revision')
    receipts = source.get('receipts')
    _require(isinstance(receipts, list) and receipts, 'settled source omitted typed receipts')
    previous_head = None
    pending_prefix = None
    checked_original = False
    task_phase_rank = 0
    for item in receipts:
        response=item['response']
        if response.get('kind') != 'page':
            continue
        value=response['value']
        if item['operation'] in ('initialHead','refreshHead'):
            if previous_head is not None and value['throughSequence'] > previous_head['throughSequence']:
                _require(pending_prefix is None,'an advancing head lost its prior prefix validation')
                pending_prefix=previous_head
            previous_head=value
        elif item['operation'] == 'prefixValidation':
            request=item['request']
            _require(pending_prefix is not None
                     and request.get('throughSequence') == pending_prefix['throughSequence']
                     and request.get('historyRevision') == pending_prefix['historyRevision']
                     and request.get('beforeSequence') is None
                     and item.get('reset') is False and item.get('merged') is False,
                     'prefix validation is not bound to the immediately prior frozen cut')
            _compare_entries(value.get('entries'),pending_prefix['entries'])
            _require(value.get('hasMoreBefore') == pending_prefix.get('hasMoreBefore')
                     and value.get('runOpen') == pending_prefix.get('runOpen')
                     and isinstance(value.get('sourceCall'),dict)
                     and all(value['sourceCall'].get(key) == pending_prefix['sourceCall'].get(key)
                             for key in ('callId','name','arguments')),
                     'frozen prefix paging/run/task metadata changed unexpectedly')
            if request['throughSequence'] == old_cut:
                _require(request['historyRevision'] == old_revision,
                         'original prefix revision differs from independent public head')
                _compare_entries(value['entries'],groundtruth['onePage']['entries'])
                checked_original=True
            pending_prefix=None
        call=value.get('sourceCall')
        _require(isinstance(call,dict) and call.get('phase') in ('running','error'),
                 'prefix task phase is not part of the controlled running/error transition')
        rank=1 if call['phase'] == 'error' else 0
        _require(rank >= task_phase_rank,'prefix task phase metadata regressed')
        task_phase_rank=rank
    _require(checked_original and pending_prefix is None,
             'settled refresh omitted the independent original prefix or an intermediate prefix')
    refreshed = [item for item in receipts if item.get('operation') == 'refreshHead'
                 and item.get('merged') is True]
    _require(refreshed, 'settled source has no merged refresh head')
    latest_page = refreshed[-1]['response']['value']
    _require(latest_page.get('throughSequence') == source['throughSequence']
             and latest_page.get('historyRevision') == source['historyRevision']
             and latest_page.get('runOpen') is False
             and isinstance(latest_page.get('sourceCall'), dict)
             and latest_page['sourceCall'].get('phase') == 'error',
             'final raw refresh page does not carry the actual post-gate error')
    prior_by_seq = {_entry_sequence(item): normalized_entry(item) for item in groundtruth['entries']}
    final = source.get('projectedEntries')
    _require(isinstance(final, list), 'settled source omitted its final cumulative projection')
    final_by_seq = {_entry_sequence(item): normalized_entry(item) for item in final}
    _require(all(final_by_seq.get(seq) == value for seq, value in prior_by_seq.items())
             and set(final_by_seq).difference(prior_by_seq),
             'settled cumulative history did not retain the old prefix and append new error history')
    for entry in final:
        _require(_entry_sequence(entry) <= source['throughSequence'],
                 'settled cumulative history exceeds its new frozen cut')
    # Reconstruct exactly what merged page receipts publish; prefix-validation pages are evidence only.
    merged: dict[int, object] = {}
    for item in receipts:
        if item.get('reset') is True:
            merged.clear()
        response = item.get('response', {})
        if item.get('merged') is True and response.get('kind') == 'page':
            for entry in response['value'].get('entries', []):
                merged[_entry_sequence(entry)] = entry
    reconstructed = [merged[seq] for seq in sorted(merged)]
    _require([normalized_entry(item) for item in reconstructed]
             == [normalized_entry(item) for item in final],
             'settled projection differs from exact merged typed receipt union')
    error_events = [item['event'] for item in latest_page.get('entries', [])
                    if isinstance(item.get('event'), dict)]
    endings = [event for event in error_events if event.get('type') == 'turn/end'
               and event.get('data', {}).get('reason') == 'error']
    errors = [event for event in error_events if event.get('type') == 'response/error']
    _require(endings and errors and any(end.get('data', {}).get('turn') == error.get('data', {}).get('turn')
                                       for end in endings for error in errors),
             'final real Native page omitted matching response/error and error turn/end events')


def _compare_entries(actual: object, expected: object) -> None:
    _require(isinstance(actual, list) and isinstance(expected, list), 'entry sets must be arrays')
    normalized_actual = [normalized_entry(entry) for entry in actual]
    normalized_expected = [normalized_entry(entry) for entry in expected]
    _require(normalized_actual == normalized_expected,
             'typed projected entries differ from independent public raw history')

