"""Narrow public-source predicates for the controlled running/error profile.

These validate one authored marker and its public current-run evidence. They do
not implement the general activity policy or change any captured source fields.
"""
from __future__ import annotations


def _text(content):
    if not isinstance(content, list):
        return None
    return ''.join(item.get('text', '') for item in content
                   if isinstance(item, dict) and item.get('type') == 'text'
                   and isinstance(item.get('text'), str))


def validate_transition_child(child, *, endpoint, child_id, phase, marker, actionables):
    if endpoint not in ('pi', 'dsh') or phase not in ('running', 'failed') or not isinstance(marker, str) or not marker:
        raise ValueError('unsupported transition profile')
    summary = child['summary']
    if (child.get('journal_ready') is not True or child.get('journal_error') is not None
            or summary.get('runState') == 'parked'
            or summary.get('status') in ('waitingForInput', 'waiting_for_input')
            or any(isinstance(value, dict) and value.get('sessionId') == child_id for value in actionables.values())):
        raise ValueError('child source is unreadable, queued or waiting')
    queue = child.get('queue')
    if not isinstance(queue, list) or (phase == 'failed' and queue):
        raise ValueError('terminal child queue is not empty')
    for item in queue:
        payload = item.get('payload', {}) if isinstance(item, dict) else {}
        if (phase != 'running' or item.get('placement') != 'dispatching'
                or payload.get('sessionId') != child_id or payload.get('mode') != 'queue'
                or _text(payload.get('content')) != marker
                or _text(item.get('message', {}).get('content')) != marker):
            raise ValueError('queued work is not the captured dispatching input')
    if child.get('run_open') is not (phase == 'running'):
        raise ValueError('child run_open disagrees with the required phase')
    if phase == 'failed' and summary.get('running') is not False:
        raise ValueError('child inventory has not left running')
    proof = _native(child, child_id, phase, marker) if endpoint == 'pi' else _journal(child, phase, marker)
    proof['capturedDispatchingQueueCount'] = len(queue)
    return proof


def _native(child, child_id, phase, marker):
    native = child.get('native_conversation')
    if (not isinstance(native, dict) or native.get('sessionId') != child_id
            or not isinstance(native.get('generation'), str) or not native.get('generation') or native.get('isStreaming') is not (phase == 'running')):
        raise ValueError('wrong or stale native child frame')
    entries = native.get('entries')
    if not isinstance(entries, list) or not all(isinstance(entry, dict) for entry in entries):
        raise ValueError('native entries absent')
    users = [(i, entry) for i, entry in enumerate(entries) if isinstance(entry, dict)
             and entry.get('type') == 'message' and entry.get('message', {}).get('role') == 'user']
    if not users:
        raise ValueError('authored native input absent')
    user_index, user = users[-1]
    if _text(user['message'].get('content')) != marker or not user.get('id'):
        raise ValueError('latest native user is not the controlled input')
    source = {'kind': 'native', 'entryId': user['id'], 'entryOrdinal': user_index}
    if phase == 'running':
        return {'basis': 'public-native-run-open-and-authored-input', 'authoredInput': source}
    assistants = [(i, entry) for i, entry in enumerate(entries) if i > user_index
                  and entry.get('type') == 'message' and entry.get('message', {}).get('role') == 'assistant']
    if not assistants:
        raise ValueError('current native failure assistant absent')
    assistant_index, assistant = assistants[-1]
    message = assistant['message']
    if message.get('stopReason') != 'error' or marker not in str(message.get('errorMessage', '')):
        raise ValueError('latest native assistant does not prove the controlled error')
    markers = [(i, entry) for i, entry in enumerate(entries)
               if entry.get('type') == 'custom' and entry.get('customType') in ('agentkit.run.v1', 'tekes.run.v1')]
    if len(markers) < 2:
        raise ValueError('current native run boundary absent')
    (start_index, start), (end_index, end) = markers[-2:]
    begin, finish = start.get('data', {}), end.get('data', {})
    if (begin.get('phase') != 'started' or finish.get('phase') != 'settled'
            or not begin.get('id') or begin.get('id') != finish.get('id')
            or finish.get('aborted') is not False
            or not start_index < user_index < assistant_index < end_index
            or any(entry.get('type') == 'message' and entry.get('message', {}).get('role') in ('user', 'assistant')
                   for entry in entries[end_index + 1:])):
        raise ValueError('native failure belongs to an older, aborted or unmatched run')
    return {'basis': 'native-current-run-error', 'authoredInput': source,
            'terminal': {'kind': 'native', 'entryId': assistant['id'], 'entryOrdinal': assistant_index,
                         'stopReason': 'error', 'runId': begin['id'],
                         'startedEntryId': start['id'], 'settledEntryId': end['id']}}


def _journal(child, phase, marker):
    entries = child.get('entries')
    if not isinstance(entries, dict):
        raise ValueError('journal entries absent')
    events = []
    for key, entry in entries.items():
        event = entry.get('event', {}) if isinstance(entry, dict) else {}
        if not isinstance(key, str) or not key.isdecimal() or type(event.get('seq')) is not int or event['seq'] != int(key):
            raise ValueError('invalid journal source sequence')
        events.append(event)
    events.sort(key=lambda event: event['seq'])
    users = [event for event in events if event.get('type') == 'user/message']
    starts = [event for event in events if event.get('type') == 'turn/start']
    boundaries = [event for event in events if event.get('type') in ('turn/start', 'turn/end')]
    if not users or not starts or not boundaries:
        raise ValueError('current authored turn source absent')
    user, start, boundary = users[-1], starts[-1], boundaries[-1]
    if (_text(user.get('data', {}).get('content')) != marker
            or user.get('data', {}).get('source', {}).get('kind') != 'user'
            or not start['seq'] < user['seq']
            or type(start.get('data', {}).get('turn')) is not int or start['data']['turn'] <= 0):
        raise ValueError('latest journal input is outside the current controlled turn')
    source = {'kind': 'journal', 'sequence': user['seq']}
    if phase == 'running':
        if boundary != start:
            raise ValueError('current journal turn already ended')
        return {'basis': 'public-journal-turn-start-and-authored-input', 'authoredInput': source}
    reason = boundary.get('data', {}).get('reason', {})
    if not isinstance(reason, dict) or not isinstance(reason.get('error'), dict):
        raise ValueError('journal error reason is not a structured public error')
    if (boundary.get('type') != 'turn/end' or not user['seq'] < boundary['seq']
            or type(boundary.get('data', {}).get('turn')) is not int
            or boundary.get('data', {}).get('turn') != start['data']['turn']
            or reason.get('kind') != 'error' or marker not in str(reason.get('error', {}).get('message', ''))):
        raise ValueError('latest journal boundary does not prove the controlled error')
    return {'basis': 'latest-child-journal-error', 'authoredInput': source,
            'terminal': {'kind': 'journal', 'sequence': boundary['seq'], 'reasonKind': 'error',
                         'turn': start['data']['turn']}}
