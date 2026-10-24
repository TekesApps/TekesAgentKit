"""Narrow validator for a reconnected primary-child Run Transcript source."""
from __future__ import annotations

from transcript_source_fixture import validate_transcript_pair, validate_transcript_source
from summary_source_fixture import validate_witness


def _obj(value, label):
    if not isinstance(value, dict):
        raise ValueError(label + ' must be an object')
    return value


def _parent_pointer(witness, endpoint, child_id, marker):
    receipt = validate_witness(witness, endpoint=endpoint, transition_phase='failed', authored_marker=marker)
    if receipt.get('childSessionId') != child_id:
        raise ValueError('original public parent validator changed the child pointer')
    epoch = witness.get('snapshot', {}).get('connectionEpoch')
    if type(epoch) is not int:
        raise ValueError('parent epoch is invalid')
    return {'parentSessionId':receipt['parentSessionId'], 'childSessionId':receipt['childSessionId'],
            'rowId':receipt['summaryRow']['id'], 'sourceIdentity':receipt['sourceIdentity'],
            'epoch':epoch, 'receipt':receipt}


def _transcript_content(witness, turn_id):
    transcript = _obj(_obj(witness.get('snapshot'), 'child snapshot').get('transcript'), 'child Transcript')
    items, turns = transcript.get('items'), transcript.get('turns')
    if not isinstance(items, list) or not isinstance(turns, list):
        raise ValueError('child Transcript items or turns are absent')
    matching_turns = [t for t in turns if isinstance(t, dict) and t.get('id') == turn_id]
    if len(matching_turns) != 1:
        raise ValueError('original opaque turn ID is not unique after reconnect')
    turn = matching_turns[0]
    users = [i for i in items if isinstance(i, dict) and i.get('kind') == 'user' and i.get('turnId') == turn_id]
    errors = [i for i in items if isinstance(i, dict) and i.get('turnId') == turn_id
              and (i.get('kind') == 'error' or (i.get('kind') == 'assistant' and isinstance(i.get('metadata'), dict)
                   and i['metadata'].get('stopReason') == 'error'))]
    if len(users) != 1 or len(errors) != 1:
        raise ValueError('reconnected failed Transcript lost its original User or error item')
    user, error = users[0], errors[0]
    # nativeStreamId is live subscription metadata, absent after history reopen.
    # Compare the error's durable semantics, not that ephemeral delivery marker.
    error_fields = {k:error.get(k) for k in ('kind','text','recordId','sourcePosition','sourceSequence','time')}
    metadata = error.get('metadata')
    error_fields['metadata'] = ({k:v for k,v in metadata.items() if k != 'nativeStreamId'}
                                if isinstance(metadata, dict) else metadata)
    return {'turnId': turn_id, 'turn': {k:turn.get(k) for k in ('input','startedAt','endedAt','status','forkBoundary')},
            'user': {k:user.get(k) for k in ('text','recordId','sourcePosition','sourceSequence')},
            'error': error_fields}


def validate_child_transcript_reconnect(*, endpoint, marker, old_running, old_failed,
                                        old_parent, reconnected_parent, fresh_child, fresh_pointer):
    """Validate failure recovery through a new parent pointer and child primary Run."""
    old_epoch = old_failed.get('snapshot', {}).get('connectionEpoch') if isinstance(old_failed, dict) else None
    old_child_id = old_failed.get('snapshot', {}).get('summary', {}).get('scope', {}).get('sessionId') if isinstance(old_failed, dict) else None
    if not isinstance(old_child_id, str) or not old_child_id or fresh_pointer != old_child_id:
        raise ValueError('fresh pointer differs from the original parent publication child ID')
    old_scope = {'endpointId':endpoint,'threadId':old_child_id,'sessionId':old_child_id}
    old_run = validate_transcript_source(old_running, endpoint=endpoint, child_id=old_child_id, phase='running', marker=marker,
        expected_epoch=old_running.get('snapshot',{}).get('connectionEpoch'), expected_scope=old_scope)
    old_fail = validate_transcript_source(old_failed, endpoint=endpoint, child_id=old_child_id, phase='failed', marker=marker,
        expected_epoch=old_epoch, expected_scope=old_scope)
    pair = validate_transcript_pair(old_run, old_fail)
    before = _parent_pointer(old_parent, endpoint, old_child_id, marker)
    after = _parent_pointer(reconnected_parent, endpoint, old_child_id, marker)
    if (before['parentSessionId'] != after['parentSessionId'] or before['childSessionId'] != after['childSessionId']
            or before['rowId'] != after['rowId'] or before['sourceIdentity'] != after['sourceIdentity']):
        raise ValueError('reconnected parent publication changed the original Summary pointer source')
    if after['epoch'] <= before['epoch']:
        raise ValueError('parent connection epoch did not strictly advance after reconnect')
    if before['epoch'] != old_epoch:
        raise ValueError('original parent and child publications did not share their original epoch')
    fresh_epoch = fresh_child.get('snapshot', {}).get('connectionEpoch') if isinstance(fresh_child, dict) else None
    if type(fresh_epoch) is not int or fresh_epoch <= old_epoch or fresh_epoch != after['epoch']:
        raise ValueError('fresh child Run does not use the new parent connection epoch')
    fresh_scope = {'endpointId':endpoint,'threadId':old_child_id,'sessionId':old_child_id}
    fresh = validate_transcript_source(fresh_child, endpoint=endpoint, child_id=old_child_id, phase='failed', marker=marker,
        expected_epoch=fresh_epoch, expected_scope=fresh_scope)
    recovered = _transcript_content(fresh_child, fresh['turnId'])
    original = _transcript_content(old_failed, old_fail['turnId'])
    if recovered != original:
        raise ValueError('reconnected child Transcript changed the original failed User/error/fork content')
    return {'endpoint':endpoint, 'scope':'primary-child-run-only', 'sidePanelChildPaneNotRun':True,
            'parentSessionId':before['parentSessionId'], 'freshPointer':fresh_pointer,
            'sameParentPointerSource':True, 'oldConnectionEpoch':old_epoch, 'newConnectionEpoch':fresh_epoch,
            'connectionEpochStrictlyAdvanced':True, 'sameOpaqueTurnId':True, 'sameFailedTranscript':True,
            'newParentConnectionEpoch':after['epoch'], 'turnId':fresh['turnId'],
            'sameParentToolSourceIdentity':True, 'serverTransactionalFreshnessProven':False,
            'runningRevision':old_run['revision'], 'failedRevision':old_fail['revision'],
            'recoveredRevision':fresh['revision'], 'originalPair':pair}


def validate_recovered_history(history, fresh_child, turn_id):
    """Compare source content across scopes without equating their opaque IDs.

    ChildRun keeps the parent thread; the primary child Run does not. Find the
    history User through its public source position, then follow its own turn ID.
    This later read is not part of the fresh publication's atomic authority.
    """
    history = _obj(history, 'recovered ChildRun history')
    original = _transcript_content(fresh_child, turn_id)
    if history.get('isStale') is not False or not isinstance(history.get('items'), list):
        raise ValueError('recovered ChildRun history is stale or incomplete')
    users = [item for item in history['items'] if isinstance(item, dict)
             and item.get('kind') == 'user'
             and all(item.get(key) == original['user'][key]
                     for key in ('recordId', 'sourcePosition', 'sourceSequence'))]
    if len(users) != 1 or users[0].get('isVisible') is not True or users[0].get('isProvisional') is not False:
        raise ValueError('recovered ChildRun history lost its unique authored User source')
    history_turn_id = users[0].get('turnId')
    if not isinstance(history_turn_id, str) or not history_turn_id:
        raise ValueError('recovered ChildRun history lacks an opaque turn identity')
    recovered = _transcript_content({'snapshot': {'transcript': history}}, history_turn_id)
    original.pop('turnId')
    recovered.pop('turnId')
    if recovered != original:
        raise ValueError('ChildRun history did not restore the original failed Transcript')
    return {'sameSourceContent': True, 'historyOpaqueTurnId': history_turn_id,
            'primaryRunOpaqueTurnId': turn_id, 'samePublication': False,
            'scope': 'parent-bound-child-history'}
