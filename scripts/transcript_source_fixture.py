"""Validate a child-scoped Transcript publication against public source facts.

This deliberately composes the narrow transition predicate with Transcript
source positions. It does not parse opaque identifiers or infer source from
matching text alone.
"""
from __future__ import annotations

import math

from summary_transition_fixture import validate_transition_child


SCOPE = "original-publication-and-same-memory-input"


def _object(value, label):
    if not isinstance(value, dict):
        raise ValueError(label + " must be an object")
    return value


def validate_transcript_source(witness, *, endpoint, child_id, phase, marker,
                               expected_epoch, expected_scope):
    """Validate one running/failed Transcript frame using its same authority."""
    if endpoint not in ('pi', 'dsh') or phase not in ('running', 'failed'):
        raise ValueError('unsupported transcript source profile')
    root = _object(witness, 'witness')
    if (root.get('evidenceScope') != SCOPE or root.get('projectionSucceeded') is not True
            or root.get('serverTransactionalFreshnessProven') is not False):
        raise ValueError('Transcript publication failed its original-source gate')
    snapshot = _object(root.get('snapshot'), 'snapshot')
    authority = _object(root.get('authority'), 'authority')
    if (authority.get('endpoint_id') != endpoint or authority.get('connection') != 'Ready'
            or any(authority.get(key) is not True for key in
                   ('inventory_ready', 'control_ready', 'actionables_ready'))):
        raise ValueError('Transcript authority is not Ready for the requested endpoint')
    connection = _object(snapshot.get('connection'), 'snapshot connection')
    if connection.get('phase') != 'connected' or connection.get('canSubmit') is not True:
        raise ValueError('published child Run is not connected and submit-ready')
    actionables = _object(authority.get('actionables'), 'authority actionables')
    if (type(snapshot.get('connectionEpoch')) is not int
            or type(authority.get('connection_epoch')) is not int
            or snapshot.get('connectionEpoch') != expected_epoch
            or authority.get('connection_epoch') != expected_epoch):
        raise ValueError('Transcript source connection epoch changed')
    if type(snapshot.get('revision')) is not int or snapshot['revision'] < 0:
        raise ValueError('Transcript publication revision is invalid')
    summary = _object(snapshot.get('summary'), 'summary')
    scope = _object(summary.get('scope'), 'summary scope')
    if scope != expected_scope or scope.get('endpointId') != endpoint or scope.get('sessionId') != child_id:
        raise ValueError('Transcript summary scope is not the selected child')
    run = _object(snapshot.get('run'), 'run')
    if run.get('scope') != expected_scope:
        raise ValueError('Transcript Run points at another scope')
    selection = _object(snapshot.get('selection'), 'selection')
    destination = _object(selection.get('destination'), 'selection destination')
    if destination.get('kind') != 'run' or destination.get('scope') != expected_scope:
        raise ValueError('selected Run destination points at another scope')
    transcript = _object(snapshot.get('transcript'), 'transcript')
    if transcript.get('isStale') is not False:
        raise ValueError('stale Transcript is not source evidence')

    sessions = _object(authority.get('sessions'), 'authority sessions')
    child = _object(sessions.get(child_id), 'same-authority child projection')
    try:
        raw = validate_transition_child(child, endpoint=endpoint, child_id=child_id,
                                        phase=phase, marker=marker,
                                        actionables=_object(authority.get('actionables'), 'same-authority actionables'),
                                        allow_absent_dsh_inventory=(endpoint == 'dsh'))
    except (AttributeError, IndexError, KeyError, TypeError, ValueError) as error:
        raise ValueError('child Native/Journal source rejected: ' + str(error)) from error
    authored = raw['authoredInput']

    items = transcript.get('items')
    turns = transcript.get('turns')
    if not isinstance(items, list) or not isinstance(turns, list):
        raise ValueError('Transcript items or turns are missing')
    users = [item for item in items if isinstance(item, dict) and item.get('kind') == 'user'
             and item.get('text') == marker]
    if len(users) != 1:
        raise ValueError('controlled authored input is not uniquely published')
    user = users[0]
    if sum(1 for item in items if isinstance(item, dict) and item.get('id') == user.get('id')) != 1:
        raise ValueError('authored Transcript item ID is not unique')
    _visible(user)
    _input_link(user)
    source_position = _object(user.get('sourcePosition'), 'authored source position')
    if source_position.get('contentIndex') is not None:
        raise ValueError('authored input must identify the whole source message')
    input_turn_id = _linked_turn_id(user)
    if authored.get('kind') == 'native':
        if (source_position.get('kind') != 'native'
                or source_position.get('entryId') != authored.get('entryId')
                or source_position.get('entryOrdinal') != authored.get('entryOrdinal')
                or user.get('recordId') != authored.get('entryId')):
            raise ValueError('Pi authored input source position or recordId differs from Native source')
    else:
        seq = authored.get('sequence')
        if (source_position.get('kind') != 'journal' or source_position.get('sequence') != seq
                or user.get('sourceSequence') != seq):
            raise ValueError('DSH authored input source position differs from Journal source')
        event = _journal_event(child, seq)
        data = _object(event.get('data'), 'authored Journal event data')
        if user.get('recordId') != data.get('id'):
            raise ValueError('DSH authored input recordId differs from Journal source')
        metadata = _object(user.get('metadata'), 'authored metadata')
        source = _object(metadata.get('source'), 'authored metadata source')
        if source != data.get('source'):
            raise ValueError('DSH authored metadata source differs from Journal source')

    turns_for_input = [turn for turn in turns if isinstance(turn, dict) and turn.get('id') == input_turn_id]
    if len(turns_for_input) != 1:
        raise ValueError('authored input does not link to exactly one Transcript turn')
    turn = turns_for_input[0]
    if turn.get('input') != marker:
        raise ValueError('Transcript turn input differs from the authored source')
    if authored.get('kind') == 'journal' and turn.get('startedAt') != _latest_start_event(child).get('time'):
        raise ValueError('Transcript turn start time differs from current Journal source')
    if phase == 'running':
        if (turn.get('status') != 'running' or turn.get('endedAt') is not None
                or turn.get('forkBoundary') is not None):
            raise ValueError('Transcript turn is not genuinely running')
        if authored.get('kind') == 'native':
            _validate_native_running_boundary(child, authored)
        if authored.get('kind') == 'journal':
            event = _latest_start_event(child)
            if turn.get('startedAt') != event.get('time'):
                raise ValueError('running Transcript turn differs from current Journal turn')
    else:
        if (turn.get('status') != 'error' or type(turn.get('endedAt')) not in (int, float)
                or not math.isfinite(turn['endedAt'])):
            raise ValueError('Transcript turn is not a settled error')
        _validate_error_item(items, child, endpoint, raw, turn, input_turn_id)
    return {'endpoint': endpoint, 'phase': phase, 'childSessionId': child_id,
            'scope': dict(expected_scope),
            'inventorySummaryPresent': child.get('summary') is not None,
            'connectionEpoch': expected_epoch, 'authoredInput': authored,
            'turnId': input_turn_id, 'revision': snapshot.get('revision'),
            'turnStatus': turn.get('status'), 'basis': raw.get('basis')}


def validate_transcript_pair(running, failed):
    """Prove the two separate child publications represent one source turn."""
    if (running.get('endpoint') not in ('pi', 'dsh')
            or not isinstance(running.get('childSessionId'), str) or not running['childSessionId']
            or type(running.get('connectionEpoch')) is not int
            or not isinstance(running.get('scope'), dict)
            or not isinstance(running.get('authoredInput'), dict)
            or not isinstance(running.get('turnId'), str) or not running['turnId']
            or running.get('turnStatus') != 'running' or failed.get('turnStatus') != 'error'):
        raise ValueError('child Transcript pair lacks validated source identities or statuses')
    keys = ('endpoint', 'childSessionId', 'scope', 'connectionEpoch', 'authoredInput', 'turnId')
    if any(running.get(key) != failed.get(key) for key in keys):
        raise ValueError('child Transcript frames changed source identity or turn')
    if running.get('phase') != 'running' or failed.get('phase') != 'failed':
        raise ValueError('child Transcript frame phases are out of order')
    if (type(running.get('revision')) is not int or type(failed.get('revision')) is not int
            or failed['revision'] <= running['revision']):
        raise ValueError('failed Transcript publication revision did not advance')
    return {'sameChild': True, 'sameEpoch': True, 'sameAuthoredInput': True,
            'sameOpaqueTurnId': True, 'runningRevision': running['revision'],
            'failedRevision': failed['revision'],
            'serverTransactionalFreshnessProven': False}


def _visible(item):
    if (not isinstance(item.get('id'), str) or not item['id']
            or item.get('isVisible') is not True or item.get('isProvisional') is not False):
        raise ValueError('Transcript item is hidden or provisional')


def _input_link(item):
    metadata = _object(item.get('metadata'), 'Transcript item metadata')
    link = _object(metadata.get('inputPresentation'), 'input presentation link')
    if (link.get('itemId') != item.get('id') or link.get('turnId') != item.get('turnId')
            or link.get('role') not in ('opening', 'inner')):
        raise ValueError('input presentation does not link this item and turn')


def _linked_turn_id(item):
    turn_id = item.get('turnId')
    if not isinstance(turn_id, str) or not turn_id:
        raise ValueError('authored input has no opaque turn link')
    return turn_id


def _journal_event(child, seq):
    entries = _object(child.get('entries'), 'child Journal entries')
    entry = _object(entries.get(str(seq)), 'Journal source entry')
    event = _object(entry.get('event'), 'Journal source event')
    if type(event.get('seq')) is not int or event['seq'] != seq:
        raise ValueError('Journal source sequence does not match its key')
    return event


def _latest_start_event(child):
    entries = _object(child.get('entries'), 'child Journal entries')
    starts = []
    for key, entry in entries.items():
        event = entry.get('event') if isinstance(entry, dict) else None
        if (isinstance(key, str) and key.isdecimal() and isinstance(event, dict)
                and event.get('type') == 'turn/start' and type(event.get('seq')) is int
                and event['seq'] == int(key)):
            starts.append(event)
    if not starts:
        raise ValueError('current Journal turn start is absent')
    return max(starts, key=lambda event: event['seq'])


def _validate_native_running_boundary(child, authored):
    native = _object(child.get('native_conversation'), 'Native conversation')
    entries = native.get('entries')
    if not isinstance(entries, list):
        raise ValueError('Native run source entries are absent')
    ordinal = authored.get('entryOrdinal')
    if type(ordinal) is not int or not 0 <= ordinal < len(entries):
        raise ValueError('current authored Native entry position is absent')
    markers = [(index, entry['data'].get('phase')) for index, entry in enumerate(entries)
              if isinstance(entry, dict) and entry.get('type') == 'custom'
              and entry.get('customType') in ('agentkit.run.v1', 'tekes.run.v1')
              and isinstance(entry.get('data'), dict)
              and entry['data'].get('phase') in ('started', 'settled')]
    if not markers or markers[-1][1] != 'started' or markers[-1][0] >= ordinal:
        raise ValueError('running Native frame has no latest started boundary before its authored input')


def _validate_error_item(items, child, endpoint, raw, turn, turn_id):
    kind = 'assistant' if endpoint == 'pi' else 'error'
    errors = [item for item in items if isinstance(item, dict) and item.get('kind') == kind
              and item.get('turnId') == turn_id
              and (endpoint != 'pi' or (isinstance(item.get('metadata'), dict)
                                        and item['metadata'].get('stopReason') == 'error'))]
    if len(errors) != 1:
        raise ValueError('settled Transcript error row is not unique for the current turn')
    item = errors[0]
    _visible(item)
    if sum(1 for other in items if isinstance(other, dict) and other.get('id') == item['id']) != 1:
        raise ValueError('Transcript error item identity is ambiguous')
    position = _object(item.get('sourcePosition'), 'error source position')
    if position.get('contentIndex') is not None:
        raise ValueError('Transcript error must identify its whole source message')
    terminal = raw.get('terminal')
    if not isinstance(terminal, dict):
        raise ValueError('public current-run error source is absent')
    if endpoint == 'pi':
        if (position.get('kind') != 'native' or position.get('entryId') != terminal.get('entryId')
                or position.get('entryOrdinal') != terminal.get('entryOrdinal')
                or item.get('recordId') != terminal.get('entryId')):
            raise ValueError('Pi Transcript error row does not link the current Native error entry')
        native = _object(child.get('native_conversation'), 'Native conversation')
        entries = native.get('entries')
        ordinal = terminal.get('entryOrdinal')
        if not isinstance(entries, list) or type(ordinal) is not int or not 0 <= ordinal < len(entries):
            raise ValueError('Native error source position is absent')
        entry = _object(entries[ordinal], 'Native error entry')
        message = _object(entry.get('message'), 'Native error message')
        metadata = _object(item.get('metadata'), 'Transcript error metadata')
        if (entry.get('id') != terminal.get('entryId') or message.get('stopReason') != 'error'
                or metadata.get('stopReason') != message.get('stopReason')
                or metadata.get('errorMessage') != message.get('errorMessage')):
            raise ValueError('Pi Transcript error metadata differs from Native error source')
        boundaries = [entry for entry in entries if isinstance(entry, dict)
                      and entry.get('id') == terminal.get('settledEntryId')]
        if len(boundaries) != 1:
            raise ValueError('Native settlement source is ambiguous')
        data = _object(boundaries[0].get('data'), 'Native settlement data')
        if (type(data.get('timestamp')) not in (int, float) or not math.isfinite(data['timestamp'])
                or turn.get('endedAt') != data['timestamp']
                or turn.get('forkBoundary') != {'entryId': terminal['settledEntryId']}):
            raise ValueError('Native end time or fork boundary differs from its settlement')
    else:
        seq = terminal.get('sequence')
        if position.get('kind') != 'journal' or position.get('sequence') != seq or item.get('sourceSequence') != seq:
            raise ValueError('DSH Transcript error row does not link the current Journal boundary')
        event = _journal_event(child, seq)
        data = _object(event.get('data'), 'Journal error boundary data')
        reason = _object(data.get('reason'), 'Journal error reason')
        error = _object(reason.get('error'), 'Journal error')
        if (event.get('type') != 'turn/end' or reason.get('kind') != 'error'
                or item.get('text') != error.get('message')
                or turn.get('endedAt') != event.get('time')
                or turn.get('forkBoundary') != {'atSeq': seq}):
            raise ValueError('DSH Transcript error row differs from Journal error source')
