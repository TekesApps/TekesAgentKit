"""Independent same-authority validator for running Native Summary samples."""

from __future__ import annotations

from typing import Any
import json
import math


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f'{label} must be an object')
    return value


def _position(value: Any, label: str, *, allow_message: bool = False) -> dict[str, Any]:
    position = _object(value, label)
    if position.get('kind') == 'journal':
        if type(position.get('sequence')) is not int or position['sequence'] < 0 \
                or position.get('contentIndex') is not None:
            raise ValueError(f'{label} is invalid')
    elif position.get('kind') == 'native':
        if not isinstance(position.get('entryId'), str) or not position['entryId'] \
                or type(position.get('entryOrdinal')) is not int or position['entryOrdinal'] < 0 \
                or (position.get('contentIndex') is not None and (type(position['contentIndex']) is not int or position['contentIndex'] < 0)) \
                or (position.get('contentIndex') is None and not allow_message):
            raise ValueError(f'{label} is invalid')
    else:
        raise ValueError(f'{label} has an unsupported kind')
    return position


def _arguments(value: Any) -> dict[str, Any]:
    if isinstance(value, str):
        try:
            value = json.loads(value)
        except (ValueError, TypeError) as error:
            raise ValueError('original Task arguments are malformed') from error
    if not isinstance(value, dict):
        raise ValueError('original Task arguments are malformed')
    return value


def _journal_event(parent: dict[str, Any], position: dict[str, Any], label: str) -> dict[str, Any]:
    entries = parent.get('entries')
    if not isinstance(entries, dict):
        raise ValueError('parent journal projection is missing')
    entry = entries.get(str(position['sequence']))
    event = entry.get('event') if isinstance(entry, dict) else None
    if not isinstance(event, dict) or type(event.get('seq')) is not int \
            or event['seq'] != position['sequence']:
        raise ValueError(f'{label} position is absent from parent journal')
    return event


def _failed_result(data: dict[str, Any], entry: dict[str, Any]) -> bool:
    # Public settlement fields only; output prose and a claimed phase are not
    # error evidence. Interrupted remains a different canonical outcome.
    error = data.get('error')
    if isinstance(error, dict) and error.get('code') == 'interrupted':
        return False
    message = data.get('message')
    message = message if isinstance(message, dict) else {}
    parts = message.get('content')
    if isinstance(error, dict) or message.get('isError') is True or (
            isinstance(parts, list) and any(isinstance(p, dict) and p.get('isError') is True for p in parts)):
        return True
    def exited(value: Any) -> bool:
        if not isinstance(value, dict):
            return False
        code = value.get('exitCode')
        return ((type(code) in (int, float) and math.isfinite(code) and code != 0
                 and int(code) == code and -(2**63) <= code < 2**63)
                or isinstance(value.get('signal'), str))
    meta = data.get('meta')
    meta = meta if isinstance(meta, dict) else {}
    status = meta.get('sessionStatus')
    view = entry.get('view')
    return (exited(meta) or meta.get('timedOut') is True
            or (isinstance(status, dict) and status.get('kind') == 'exited' and exited(status))
            or (isinstance(view, dict) and view.get('for') == 'result' and exited(view.get('view'))))


def validate_parent_summary_sample(sample: Any) -> dict[str, Any]:
    if not isinstance(sample, dict) or sample.get('available') is not True:
        raise ValueError('parent Summary sample is unavailable')
    witness = _object(sample.get('witness'), 'Summary witness')
    if witness.get('evidenceScope') != 'original-publication-and-same-memory-input' \
            or witness.get('serverTransactionalFreshnessProven') is not False \
            or witness.get('projectionSucceeded') is not True:
        raise ValueError('Summary sample lacks its successful same-authority source witness')
    snapshot = _object(witness.get('snapshot'), 'snapshot')
    authority = _object(witness.get('authority'), 'authority')
    publication = sample.get('publication')
    epoch = sample.get('connectionEpoch')
    if type(publication) is not int or type(snapshot.get('revision')) is not int \
            or snapshot['revision'] != publication:
        raise ValueError('sample publication differs from same-frame source snapshot')
    if type(epoch) is not int or epoch <= 0 or type(snapshot.get('connectionEpoch')) is not int \
            or snapshot['connectionEpoch'] != epoch or type(authority.get('connection_epoch')) is not int \
            or authority['connection_epoch'] != epoch:
        raise ValueError('sample epoch differs from same-frame source authority')
    scope = _object(sample.get('scope'), 'sample scope')
    endpoint, parent_id = scope.get('endpointId'), scope.get('sessionId')
    if endpoint != 'native' or not isinstance(parent_id, str) or not parent_id \
            or scope.get('threadId') != parent_id:
        raise ValueError('invalid parent Run scope')
    summary = _object(snapshot.get('summary'), 'parent Summary')
    transcript = _object(snapshot.get('transcript'), 'parent transcript')
    if summary.get('scope') != scope or authority.get('endpoint_id') != endpoint \
            or authority.get('connection_epoch') != epoch:
        raise ValueError('parent Summary scope/epoch differs from its authority witness')
    sessions = authority.get('sessions')
    parent = sessions.get(parent_id) if isinstance(sessions, dict) else None
    if authority.get('connection') != 'Ready' or not all(authority.get(k) is True for k in ('inventory_ready', 'control_ready', 'actionables_ready')) or transcript.get('isStale') is not False:
        raise ValueError('parent authority or transcript is not fresh and ready')
    if not isinstance(parent, dict) or parent.get('journal_ready') is not True:
        raise ValueError('parent source projection is not ready')
    rows, tools = summary.get('tasks'), transcript.get('tools')
    if not isinstance(rows, list) or not isinstance(tools, list):
        raise ValueError('parent Task facts or tools missing')
    row_id = sample.get('taskId')
    if not isinstance(row_id, str) or not row_id:
        raise ValueError('Summary Task identity is missing')
    matches = [row for row in rows if isinstance(row, dict) and row.get('id') == row_id]
    tool_matches = [tool for tool in tools if isinstance(tool, dict) and tool.get('id') == row_id]
    if len(matches) != 1 or len(tool_matches) != 1:
        raise ValueError('Summary row does not map to exactly one original parent tool')
    row, tool = matches[0], tool_matches[0]
    if row.get('kind') != 'task' or row.get('scope') != scope or row.get('name') != sample.get('title') \
            or 'sessionId' in row or 'independentRun' in row or tool.get('name') != 'task':
        raise ValueError('Summary Task identity differs from original parent tool')
    raw_status = tool.get('phase')
    canonical = {'doing': 'in_progress', 'running': 'in_progress', 'error': 'failed', 'done': 'completed'}.get(raw_status)
    adapted_status = 'in_progress' if raw_status == 'doing' else raw_status
    if sample.get('adaptedStatus', row.get('status')) != row.get('status'):
        raise ValueError('reported adapted status differs from same-frame Task row')
    if canonical is None or sample.get('rawStatus') != raw_status or row.get('status') != adapted_status \
            or sample.get('canonicalStatus') != canonical:
        raise ValueError('Summary Task status is not derived from its original tool phase')
    arguments = _arguments(tool.get('arguments'))
    if arguments.get('task_name') != row.get('name'):
        raise ValueError('Summary title differs from original Task arguments')
    position = _position(tool.get('sourcePosition'), 'original Task source position')
    if sample.get('sourcePosition') != position:
        raise ValueError('original Task source position differs from sample')
    embedded_tool = _object(sample.get('tool'), 'embedded Transcript tool')
    if embedded_tool != tool:
        raise ValueError('embedded Transcript tool differs from same-frame public projection')

    if position['kind'] == 'journal':
        event = _journal_event(parent, position, 'Task call')
        original_call = event.get('data')
        if event.get('type') != 'tool/call' or not isinstance(original_call, dict) \
                or original_call.get('name') != 'task' \
                or _arguments(original_call.get('arguments')) != arguments:
            raise ValueError('Task arguments differ from original parent journal call')
        source_entry = sample.get('sourceEntry')
        if not isinstance(source_entry, dict) or source_entry.get('event') != event:
            raise ValueError('captured source entry differs from original parent journal')
        source_call_id = original_call.get('callId')
    else:
        native = parent.get('native_conversation')
        entries = native.get('entries') if isinstance(native, dict) else None
        ordinal, content_index = position['entryOrdinal'], position['contentIndex']
        if not isinstance(entries, list) or ordinal >= len(entries):
            raise ValueError('Native Task source position is out of range')
        entry = entries[ordinal]
        message = entry.get('message') if isinstance(entry, dict) else None
        content = message.get('content') if isinstance(message, dict) else None
        call = content[content_index] if isinstance(content, list) and content_index < len(content) else None
        if not isinstance(call, dict) or entry.get('id') != position['entryId'] \
                or message.get('role') != 'assistant' or call.get('type') != 'toolCall' \
                or call.get('name') != 'task' or call.get('arguments') != arguments:
            raise ValueError('Task arguments differ from original Native tool call')
        if sample.get('sourceEntry') != call:
            raise ValueError('captured source call differs from original Native conversation')
        source_call_id = call.get('id')

    if not isinstance(source_call_id, str) or not source_call_id \
            or source_call_id != sample.get('sourceCallId'):
        raise ValueError('opaque source call ID differs from original call')
    if canonical in ('failed', 'completed'):
        result_position = _position(tool.get('resultPosition'), 'original Task result position', allow_message=True)
        if result_position['kind'] != position['kind']:
            raise ValueError('Task result source dialect differs from original call')
        if result_position['kind'] == 'journal':
            result_event = _journal_event(parent, result_position, 'Task result')
            data = result_event.get('data')
            if result_event.get('type') == 'tool/result':
                message = data.get('message') if isinstance(data, dict) else None
                message_source = message.get('source') if isinstance(message, dict) else None
                if message != tool.get('result') or not isinstance(message_source, dict) \
                        or message_source.get('callId') != source_call_id:
                    raise ValueError('failed Task result differs from original public result/callId')
                result_entry = parent['entries'][str(result_position['sequence'])]
                if isinstance(data.get('error'), dict) and data['error'].get('code') == 'interrupted':
                    raise ValueError('interrupted parent outcome cannot be completed or failed')
                if tool.get('resultError') != data.get('error') or tool.get('resultMeta') != data.get('meta') or _failed_result(data, result_entry) != (canonical == 'failed'):
                    raise ValueError('claimed failed Task has no matching public failure facts')
            elif result_event.get('type') == 'response/error' and canonical == 'failed':
                error_source = data.get('source') if isinstance(data, dict) else None
                error_call_id = (data.get('callId') if isinstance(data, dict) else None) \
                    or (error_source.get('callId') if isinstance(error_source, dict) else None)
                if not isinstance(data, dict) or data.get('turn') != event.get('data', {}).get('turn') \
                        or error_call_id != source_call_id \
                        or not isinstance(tool.get('resultError'), dict) or data != tool.get('resultError'):
                    raise ValueError('failed Task error differs from original public response/error')
            else:
                raise ValueError('failed Task result position is not a public error/result event')
        else:
            native = parent.get('native_conversation')
            entries = native.get('entries') if isinstance(native, dict) else None
            ordinal, content_index = result_position['entryOrdinal'], result_position['contentIndex']
            if not isinstance(entries, list) or ordinal >= len(entries):
                raise ValueError('Native Task result position is out of range')
            result_entry = entries[ordinal]
            result_message = result_entry.get('message') if isinstance(result_entry, dict) else None
            if not isinstance(result_message, dict) or result_entry.get('id') != result_position['entryId'] \
                    or result_position['contentIndex'] is not None \
                    or result_message.get('role') != 'toolResult' \
                    or result_message.get('toolCallId') != source_call_id \
                    or (result_message.get('isError') is True) != (canonical == 'failed') \
                    or result_message != tool.get('result'):
                raise ValueError('failed Task result differs from original Native result/callId')

    for field, expected_type in (('rowId', int), ('viewInstance', (str, int)),
                                 ('titleInstance', (str, int))):
        value = sample.get(field)
        if (type(value) is not int if field == 'rowId' else
                (not isinstance(value, expected_type) or (isinstance(value, str) and not value.strip())
                 or type(value) is bool)):
            raise ValueError(f'mounted original {field} identity is missing or malformed')
    if sample.get('mounted') is not True:
        raise ValueError('original Summary row is not mounted')
    return {'sourceCallId': source_call_id, 'sourcePosition': position,
            'taskId': row_id, 'title': row['name'], 'status': row['status'],
            'publication': publication, 'rowId': sample['rowId'],
            'viewInstance': sample['viewInstance'], 'titleInstance': sample['titleInstance'],
            'serverTransactionalFreshnessProven': False}
