"""Owned Native child request routing and strict head-only pane proof checks."""
from __future__ import annotations

import io
import json

from owned_endpoint_provider import provider_handler
from summary_transition_provider import provider_with_failure_gates

PREFIX = 'AGENTKIT_CHILD_PANE_SOURCE '
MARKER = 'CHILD_NATIVE_PROOF'


def is_native_child_request(body: object) -> bool:
    """Select the child user input, never a parent function-call argument."""
    if not isinstance(body, dict) or not isinstance(body.get('input'), list):
        return False
    return any(isinstance(item, dict) and item.get('role') == 'user'
               and MARKER in json.dumps(item)
               and 'ENDPOINT_NATIVE_CHILD' not in json.dumps(item)
               for item in body['input'])


def native_child_provider(*, workspace, requests, gate):
    if gate.marker != MARKER:
        raise ValueError('Native fixture requires the fixed child marker')
    original = provider_handler(workspace=workspace, requests=requests)
    gated = provider_with_failure_gates(workspace=workspace, requests=requests,
                                       gates={'native': gate})

    class NativeProvider(original):
        def do_POST(self):
            stream = self.rfile
            raw = stream.read(int(self.headers.get('Content-Length', '0')))
            try:
                body = json.loads(raw)
            except (ValueError, TypeError):
                body = None
            self.rfile = io.BytesIO(raw)
            try:
                (gated if is_native_child_request(body) else original).do_POST(self)
            finally:
                self.rfile.close()
                self.rfile = stream

    return NativeProvider


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_native_frame(frame, *, parent, child_id, expected_phase, previous=None):
    _require(isinstance(frame, dict) and frame.get('phase') == expected_phase
             and frame.get('parent') == parent and frame.get('childId') == child_id
             and frame.get('marker') == MARKER, 'Native pane frame identity mismatch')
    _require(parent.get('endpointId') == 'native' and parent.get('threadId')
             == parent.get('sessionId') and bool(parent.get('sessionId'))
             and bool(child_id) and child_id != parent['sessionId'], 'invalid Native parent/child')
    witness = frame.get('witness', {})
    snapshot, source = witness.get('snapshot', {}), witness.get('source', {})
    scope = dict(parent, sessionId=child_id)
    _require(snapshot.get('scope') == scope and source.get('parentScope') == parent
             and source.get('childId') == child_id, 'Native witness scope mismatch')
    _require(source.get('evidenceScope') == 'built-in-bounded-head-only-same-local-read'
             and source.get('serverTransactionalFreshnessProven') is False,
             'bounded head scope/freshness mismatch')
    request, page = source.get('headRequest', {}), source.get('rawHead', {})
    _require(request.get('parentSessionId') == parent['sessionId']
             and request.get('childSessionId') == child_id
             and type(source.get('pageSize')) is int
             and 0 < source['pageSize'] <= 200
             and request.get('maxMessages') == source['pageSize']
             and all(request.get(key) is None for key in (
                 'throughSequence', 'beforeSequence', 'historyRevision')), 'not an original head request')
    _require(page.get('parentSessionId') == parent['sessionId']
             and page.get('childSessionId') == child_id
             and type(page.get('throughSequence')) is int and page['throughSequence'] >= 0
             and isinstance(page.get('historyRevision'), str) and bool(page['historyRevision'])
             and isinstance(page.get('entries'), list) and bool(page['entries'])
             and len(page['entries']) <= source['pageSize'], 'invalid original bounded head')
    seqs = [entry.get('event', {}).get('seq') for entry in page['entries']]
    _require(all(type(seq) is int and 0 <= seq <= page['throughSequence'] for seq in seqs)
             and seqs == sorted(set(seqs)), 'head sequence order mismatch')
    call = page.get('sourceCall', {})
    phase = 'running' if expected_phase == 'running' else 'error'
    _require(expected_phase in ('running', 'failed') and call.get('phase') == phase
             and call.get('name') == 'task' and bool(call.get('callId'))
             and MARKER in json.dumps(call.get('arguments')), 'source Task phase or marker mismatch')
    _require(page.get('runOpen') is (expected_phase == 'running'), 'raw child run-open mismatch')
    transcript = snapshot.get('transcript', {})
    _require(transcript.get('isStale') is False
             and transcript.get('hasMoreBefore') is page.get('hasMoreBefore')
             and type(page.get('hasMoreBefore')) is bool, 'head-only transcript cache flags mismatch')
    task_ids = ['native', child_id, 'task-source', call['callId']]
    turns = [turn for turn in transcript.get('turns', [])
             if _decode_id(turn.get('id')) == task_ids]
    _require(len(turns) == 1 and turns[0].get('status') == phase, 'original source Task turn mismatch')
    task_turn = turns[0]['id']
    tools = [tool for tool in transcript.get('tools', []) if tool.get('turnId') == task_turn]
    _require(len(tools) == 1 and tools[0].get('id') == task_turn + ':call'
             and tools[0].get('phase') == phase and tools[0].get('name') == call['name']
             and tools[0].get('arguments') == call['arguments'], 'original Task tool projection mismatch')
    _require(snapshot.get('connection', {}).get('phase') == 'connected'
             and snapshot['connection'].get('historyIsStale') is False, 'original pane connection is not fresh')
    epoch, publication = snapshot.get('connectionEpoch'), snapshot.get('publication')
    _require(type(epoch) is int and epoch > 0 and type(publication) is int and publication > 0,
             'invalid original pane publication')
    _require(frame.get('mountedSnapshot') == snapshot, 'mounted frame differs from captured original pane')
    starts = [entry['event'].get('data', {}).get('turn') for entry in page['entries']
              if entry['event'].get('type') == 'turn/start']
    _require(len(starts) == 1 and type(starts[0]) is int and starts[0] > 0,
             'owned child fixture requires one unambiguous original child turn')
    child_turn_number = starts[0]
    child_turn_id = ['native', child_id, 'turn:' + str(child_turn_number)]
    if expected_phase == 'failed':
        _require(any(entry['event'].get('type') == 'turn/end'
                     and entry['event'].get('data', {}).get('reason') == 'error'
                     and entry['event'].get('data', {}).get('turn') == child_turn_number
                     and (previous is None or entry['event']['seq'] > previous['throughSequence'])
                     for entry in page['entries']), 'failed raw head lacks this child turn terminal error')
        _require(any(entry['event'].get('type') == 'response/error'
                     and entry['event'].get('data', {}).get('turn') == child_turn_number
                     and (previous is None or entry['event']['seq'] > previous['throughSequence'])
                     for entry in page['entries']), 'failed raw head lacks this child turn response error')
        _require(any(_decode_id(item.get('turnId')) == child_turn_id and item.get('kind') == 'error'
                     for item in transcript.get('items', [])), 'failed child transcript lacks same-turn error item')
        _require(any(_decode_id(turn.get('id')) == child_turn_id and turn.get('status') == 'error'
                     for turn in transcript.get('turns', [])), 'failed child transcript lacks same-turn error status')
    proof = {'parent': parent, 'childId': child_id, 'scope': scope,
             'connectionEpoch': epoch, 'publication': publication,
             'throughSequence': page['throughSequence'], 'historyRevision': page['historyRevision'],
             'sourceCall': call, 'entries': page['entries'], 'taskTurnId': task_turn,
             'childTurnNumber': child_turn_number,
             'headOnly': True, 'readOnly': True, 'serverTransactionalFreshnessProven': False}
    if previous is not None:
        _require(expected_phase == 'failed' and previous['parent'] == parent
                 and previous['scope'] == scope and previous['childId'] == child_id
                 and previous['connectionEpoch'] == epoch and previous['publication'] < publication
                 and previous['throughSequence'] < page['throughSequence']
                 and previous['historyRevision'] != page['historyRevision']
                 and previous['taskTurnId'] == task_turn
                 and previous['childTurnNumber'] == child_turn_number
                 and all(previous['sourceCall'][key] == call[key]
                         for key in ('callId', 'name', 'arguments'))
                 and page['entries'][:len(previous['entries'])] == previous['entries'],
                 'Native transition changed source identity, epoch, or original prefix')
    return proof


def _decode_id(value):
    try:
        return json.loads(value)
    except (TypeError, ValueError):
        return None


def validate_native_receipt(receipt, running, failed):
    _require(isinstance(receipt, dict) and receipt.get('accepted') is True
             and receipt.get('parent') == running['parent']
             and receipt.get('childId') == running['childId']
             and receipt.get('childScope') == running['scope']
             and receipt.get('connectionEpoch') == running['connectionEpoch']
             and receipt.get('runningPublication') == running['publication']
             and receipt.get('failedPublication') == failed['publication'], 'Native receipt pairing mismatch')
    for key in ('readOnly', 'headOnly', 'openedViaOriginalControl', 'closedViaOriginalControl',
                'parentDraftPreserved', 'parentSelectionPreserved', 'retiredWitnessRejected'):
        _require(receipt.get(key) is True, 'Native host lifecycle incomplete: ' + key)
    for key in ('sentViaOriginalComposer', 'nativeInteraction', 'physicalIme',
                'serverTransactionalFreshnessProven'):
        _require(receipt.get(key) is False, 'Native host scope changed: ' + key)
    return {'accepted': True, 'evidenceScope': 'real-native-readonly-child-pane-original-head-source',
            'parent': running['parent'], 'childScope': running['scope'],
            'connectionEpoch': running['connectionEpoch'],
            'runningPublication': running['publication'], 'failedPublication': failed['publication'],
            'headOnly': True, 'readOnly': True, 'serverTransactionalFreshnessProven': False}
