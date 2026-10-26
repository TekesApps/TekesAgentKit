"""Fail-closed normalizer for public performance transcript readbacks.

The input is a headless ``RunSnapshot`` and the matching session projection
returned by the public ``MemoryProjectionStore`` API. This module never opens
storage files. It resolves transcript positions against the supplied raw
projection so opaque presentation tool identifiers are never treated as
protocol call identifiers.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
import hashlib
import json
from typing import Any

from performance_provider_fixture import (
    BODY_BYTES,
    SEED_TOOL_TURNS,
    body_text,
    expected_tool_call_id,
    read_source_bytes,
    validate_history,
)


_BODY_KINDS = {"user", "assistant"}
_IGNORED_KINDS = {
    "system", "extrasystem", "extra_system", "requestprompt", "request_prompt", "request-prompt",
}
_TEXT_BLOCKS = {"text", "input_text", "output_text"}


def _mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise ValueError(f"{label} must be an object")
    return value


def _position(value: Any, label: str) -> Mapping[str, Any]:
    result = _mapping(value, label)
    if result.get("kind") not in ("journal", "native"):
        raise ValueError(f"{label} has an unsupported source kind")
    return result


def _source_anchor(position: Mapping[str, Any]) -> dict[str, Any]:
    """Keep stable public source coordinates, omitting presentation IDs."""
    anchor: dict[str, Any] = {"kind": position["kind"]}
    for key in ("sequence", "entryId", "entryOrdinal", "contentIndex"):
        value = position.get(key)
        if value is not None:
            anchor[key] = value
    if "sequence" not in anchor and "entryId" not in anchor:
        raise ValueError("source position lacks a public journal sequence or native entry ID")
    return anchor


def _raw_session(projection: Any) -> Mapping[str, Any]:
    projection = _mapping(projection, "raw projection")
    nested = projection.get("rawSession")
    if nested is None:
        nested = projection.get("raw_session")
    return _mapping(nested if nested is not None else projection, "raw session projection")


def _journal_event(raw: Mapping[str, Any], sequence: Any) -> Mapping[str, Any]:
    if type(sequence) is not int:
        raise ValueError("journal source position must have an integer sequence")
    entries = _mapping(raw.get("entries"), "journal entries")
    record = entries.get(str(sequence), entries.get(sequence))
    record = _mapping(record, f"journal entry {sequence}")
    event = record.get("event", record)
    event = _mapping(event, f"journal event {sequence}")
    if event.get("seq") != sequence:
        raise ValueError("journal source sequence does not match its raw event")
    return event


def _native_entry(raw: Mapping[str, Any], position: Mapping[str, Any]) -> Mapping[str, Any]:
    conversations = raw.get("native_conversation")
    conversations = _mapping(conversations, "Pi native conversation")
    entries = conversations.get("entries")
    if not isinstance(entries, Sequence) or isinstance(entries, (str, bytes)):
        raise ValueError("Pi native conversation entries are missing")
    entry_id = position.get("entryId")
    matches = [entry for entry in entries if isinstance(entry, Mapping) and entry.get("id") == entry_id]
    if len(matches) != 1:
        raise ValueError("native entry ID does not resolve to exactly one raw conversation entry")
    ordinal = position.get("entryOrdinal")
    if type(ordinal) is int and not (0 <= ordinal < len(entries) and entries[ordinal] is matches[0]):
        raise ValueError("native entry ordinal and ID disagree")
    return matches[0]


def _record_at(raw: Mapping[str, Any], position: Mapping[str, Any]) -> tuple[Mapping[str, Any], Mapping[str, Any]]:
    kind = position.get("kind")
    if kind == "journal":
        event = _journal_event(raw, position.get("sequence"))
        return event, _mapping(event.get("data"), "journal event data")
    entry = _native_entry(raw, position)
    message = entry.get("message")
    if not isinstance(message, Mapping):
        raise ValueError("native conversation entry lacks its original message")
    return entry, message


def _content_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if not isinstance(content, Sequence) or isinstance(content, (str, bytes)):
        raise ValueError("source content is not text or a content-block list")
    parts = [block.get("text") for block in content
             if isinstance(block, Mapping) and block.get("type") in _TEXT_BLOCKS
             and isinstance(block.get("text"), str)]
    return "".join(parts)


def _body_from_source(raw: Mapping[str, Any], position: Mapping[str, Any], role: str) -> str:
    record, data = _record_at(raw, position)
    if position.get("kind") == "journal":
        event_type = record.get("type")
        if role == "user":
            if event_type not in ("user/message", "user/message.updated"):
                raise ValueError("user transcript row does not point to a raw user journal event")
            message = data
        else:
            if event_type not in ("assistant/message", "assistant/message.updated"):
                raise ValueError("assistant transcript row does not point to a raw assistant journal event")
            message = _mapping(data.get("message"), "raw assistant message")
    else:
        message = data
        expected_role = "user" if role == "user" else "assistant"
        if message.get("role") != expected_role:
            raise ValueError("transcript row role disagrees with its original native entry")
    raw_role = message.get("role")
    if raw_role is not None and raw_role != role:
        raise ValueError("transcript row role disagrees with its original source")
    return _content_text(message.get("content"))


def _turn_number(row: Mapping[str, Any], text: str, phase: str) -> int:
    prefix = f"AKPERF|{phase}|"
    if not text.startswith(prefix):
        raise ValueError("ordinary transcript body does not have the expected fixture identity")
    try:
        parts = text.split("|", 5)
        if parts[0] != "AKPERF" or parts[1] != phase or not parts[2].startswith("s"):
            raise ValueError
        return int(parts[3][1:])
    except (IndexError, ValueError) as error:
        raise ValueError("ordinary transcript body has an invalid session coordinate") from error


def _ordinary_rows(transcript: Mapping[str, Any], history_rows: Any,
                   raw: Mapping[str, Any]) -> tuple[list[Mapping[str, Any]], list[dict[str, Any]]]:
    rows: Any
    if history_rows is None:
        if transcript.get("hasMoreBefore") is not False:
            raise ValueError("transcript readback is incomplete: hasMoreBefore must be false")
        rows = transcript.get("items")
    else:
        history_result = _mapping(history_rows, "paged history result")
        if history_result.get("hasMoreBefore") is not False:
            raise ValueError("paged history readback must prove hasMoreBefore is false")
        rows = history_result.get("items", history_result.get("rows"))
    if not isinstance(rows, Sequence) or isinstance(rows, (str, bytes)):
        raise ValueError("public history rows must be an array")

    ordinary: list[Mapping[str, Any]] = []
    ignored_anchors: list[dict[str, Any]] = []
    for index, raw_row in enumerate(rows):
        row = _mapping(raw_row, f"transcript row {index}")
        kind = row.get("kind", row.get("role"))
        normalized_kind = str(kind).replace("-", "_").lower() if kind is not None else ""
        if normalized_kind in _IGNORED_KINDS:
            position = _position(row.get("sourcePosition"), "context source position")
            _record_at(raw, position)
            ignored_anchors.append(_source_anchor(position))
            continue
        if normalized_kind in _BODY_KINDS:
            if normalized_kind == 'user':
                position = _position(row.get('sourcePosition'), 'user source position')
                record, data = _record_at(raw, position)
                source = data.get('source')
                if (record.get('type') == 'user/message' and isinstance(source, Mapping)
                        and source.get('kind') in ('runtime-context', 'time-context')
                        and source.get('form') == 'snapshot'):
                    metadata = row.get('metadata', {})
                    if (not isinstance(metadata, Mapping) or metadata.get('hidden') is not True
                            or metadata.get('source') != source
                            or row.get('recordId') != data.get('id')
                            or row.get('text') != _content_text(data.get('content'))):
                        raise ValueError('public context row does not match its hidden typed/raw source')
                    ignored_anchors.append(_source_anchor(position))
                    continue
            # A tool-only model step can publish an empty, non-final
            # assistant carrier in addition to its dedicated tool events.
            # It is not one of the frozen nonempty final body messages.
            if (normalized_kind == 'assistant' and row.get('text') == ''
                    and row.get('isFinal') is False and row.get('isProvisional') is False):
                position = _position(row.get('sourcePosition'), 'empty tool-step carrier')
                record, data = _record_at(raw, position)
                message = data.get('message', data)
                tools = transcript.get('tools', [])
                same_turn_tools = [tool for tool in tools if tool.get('turnId') == row.get('turnId')]
                content = message.get('content') if isinstance(message, Mapping) else None
                tool_only_content = content == []
                if isinstance(content, list) and content:
                    tool_only_content = all(
                        isinstance(block, Mapping) and block.get('type') in ('toolCall', 'tool-call')
                        and any(tool.get('sourcePosition', {}).get('contentIndex') == block_index
                                and all(tool.get('sourcePosition', {}).get(key) == position.get(key)
                                        for key in ('kind', 'entryId', 'sequence'))
                                for tool in same_turn_tools)
                        for block_index, block in enumerate(content))
                if (not isinstance(message, Mapping) or message.get('role') != 'assistant'
                        or not tool_only_content
                        or not row.get('turnId')
                        or not same_turn_tools):
                    raise ValueError('empty assistant is not an original tool-only carrier')
                ignored_anchors.append(_source_anchor(position))
                continue
            ordinary.append(row)
        elif normalized_kind in ("tool", "toolresult", "tool_result", "reasoning", "footnote"):
            # These are represented by dedicated public snapshot collections;
            # they are not user/final-assistant body messages.
            continue
        else:
            raise ValueError(f"transcript row {index} has an unknown semantic kind")
    return ordinary, ignored_anchors


def _content_at_index(content: Any, index: Any, label: str) -> Mapping[str, Any]:
    if type(index) is not int or not isinstance(content, Sequence) or isinstance(content, (str, bytes)):
        raise ValueError(f"{label} source position must identify a content index")
    if index < 0 or index >= len(content):
        raise ValueError(f"{label} content index is outside the original message")
    return _mapping(content[index], label)


def _raw_tool_call(raw: Mapping[str, Any], position: Mapping[str, Any]) -> tuple[str, str, str]:
    record, data = _record_at(raw, position)
    if position.get("kind") == "journal" and record.get("type") == "tool/call":
        call_id, name = data.get("callId"), data.get("name")
    else:
        if position.get("kind") == "journal":
            if record.get("type") not in ("assistant/message", "assistant/message.updated"):
                raise ValueError("tool source position is not an original assistant message")
            message = _mapping(data.get("message"), "raw tool-call assistant message")
        else:
            message = data
            if message.get("role") != "assistant":
                raise ValueError("tool source position is not an original Pi assistant entry")
        call = _content_at_index(message.get("content"), position.get("contentIndex"), "raw tool call")
        if call.get("type") not in ("tool-call", "toolCall"):
            raise ValueError("canonical tool position does not resolve to a raw tool call")
        call_id, name = call.get("id", call.get("callId")), call.get("name")
    turn = data.get("turn")
    turn_id = ""
    if isinstance(turn, (str, int)):
        turn_id = str(turn)
    elif position.get("kind") == "native":
        turn_id = str(record.get("id", ""))
    return str(call_id or ""), str(name or ""), turn_id


def _raw_tool_result(raw: Mapping[str, Any], position: Mapping[str, Any]) -> tuple[str, str]:
    record, data = _record_at(raw, position)
    if position.get("kind") == "journal":
        if record.get("type") != "tool/result":
            raise ValueError("tool result position is not an original tool/result journal event")
        message = _mapping(data.get("message"), "raw tool result message")
    else:
        message = data
        if message.get("role") != "toolResult":
            raise ValueError("native result position is not an original Pi toolResult entry")
    source = message.get('source', {})
    native_call_id = source.get('callId') if isinstance(source, Mapping) and source.get('kind') == 'tool' else None
    call_id = message.get("toolCallId", data.get("callId", native_call_id))
    result_text = _content_text(message.get("content"))
    if not isinstance(call_id, str) or not call_id or not result_text:
        raise ValueError("original tool result is missing its call ID or text")
    return call_id, result_text


def _validate_tool_group(raw: Mapping[str, Any], tool: Mapping[str, Any], expected_turn: int,
                         expected_call_id: str, expected_turn_id: Any) -> dict[str, Any]:
    call_position = _position(tool.get("sourcePosition"), "tool source position")
    result_position = _position(tool.get("resultPosition"), "tool result position")
    call_id, tool_name, _ = _raw_tool_call(raw, call_position)
    result_call_id, result_text = _raw_tool_result(raw, result_position)
    if tool_name not in ("read", "str_replace_editor"):
        raise ValueError("original public tool name is not an allowed read tool")
    if call_id != expected_call_id or result_call_id != call_id:
        raise ValueError("raw tool call and result IDs do not match the fixture turn")
    if tool.get("turnId") != expected_turn_id:
        raise ValueError("tool group belongs to a different canonical transcript turn")
    source_bytes = read_source_bytes()
    try:
        structured = json.loads(result_text)
    except (ValueError, TypeError):
        structured = None
    if isinstance(structured, Mapping) and 'lines' in structured:
        lines = structured['lines']
        expected_lines = source_bytes.decode('utf-8').splitlines()
        if (structured.get('bytes') != len(source_bytes)
                or structured.get('sha256') != 'sha256-'+hashlib.sha256(source_bytes).hexdigest()
                or structured.get('truncated') is not False
                or structured.get('total_lines') != len(expected_lines)
                or not isinstance(lines, list)
                or len(lines) != len(expected_lines)
                or any(line.get('line') != i+1 or line.get('text') != expected
                       or line.get('truncated') is not False
                       for i, (line, expected) in enumerate(zip(lines, expected_lines)))):
            raise ValueError('structured read result differs from complete original fixture bytes')
    missing = [line.decode("utf-8") for line in source_bytes.splitlines()
               if line.strip() and line.decode("utf-8") not in result_text]
    if missing:
        raise ValueError("original read result omits deterministic source evidence lines")
    # If a typed tool result is present, bind it to the original source result.
    typed_result = tool.get("result")
    if isinstance(typed_result, Mapping):
        typed_call_id = typed_result.get("toolCallId")
        typed_text = _content_text(typed_result.get("content")) if "content" in typed_result else None
        if typed_call_id is not None and typed_call_id != call_id:
            raise ValueError("typed tool result call ID disagrees with its raw source")
        if typed_text is not None and typed_text != result_text:
            raise ValueError("typed tool result text disagrees with its raw source")
    if tool.get("name") not in (None, tool_name):
        raise ValueError("canonical tool label disagrees with the original source name")
    if tool.get("phase") not in (None, "done", "completed"):
        raise ValueError("public tool group is not completed")
    return {
        "turn": expected_turn,
        "toolName": tool_name,
        "callId": call_id,
        "resultCallId": result_call_id,
        "resultText": result_text,
        "sourcePosition": _source_anchor(call_position),
        "resultPosition": _source_anchor(result_position),
    }


def normalize_public_session(
    snapshot: Mapping[str, Any],
    raw_projection: Mapping[str, Any],
    *,
    run_scope: Mapping[str, Any],
    session_index: int,
    phase: str = "seed",
    body_count: int = 100,
    history_rows: Any = None,
) -> dict[str, Any]:
    """Normalize and validate one complete public session readback.

    ``body_count`` counts user and final-assistant body rows. This version
    accepts the frozen 100-row seed corpus. ``history_rows`` is optional; when
    supplied it must be a complete public paging result with
    ``hasMoreBefore=False``. Every supplied row still resolves through the
    same canonical source-position and raw-source checks.
    """
    snapshot = _mapping(snapshot, "RunSnapshot")
    run_scope = _mapping(run_scope, "RunScope")
    if (not isinstance(run_scope.get("endpointId"), str) or not run_scope.get("endpointId")
            or not isinstance(run_scope.get("sessionId"), str) or not run_scope.get("sessionId")
            or not isinstance(run_scope.get("threadId"), str) or not run_scope.get("threadId")):
        raise ValueError("RunScope must declare endpointId, threadId and sessionId")
    run = _mapping(snapshot.get("run"), "RunSnapshot.run")
    actual_scope = _mapping(run.get("scope"), "RunSnapshot.run.scope")
    for key in ("endpointId", "threadId", "sessionId"):
        if actual_scope.get(key) != run_scope.get(key):
            raise ValueError(f"RunSnapshot scope {key} does not match the requested RunScope")
    if type(session_index) is not int or session_index not in (0, 1):
        raise ValueError("session_index must be 0 or 1")
    if phase not in ("seed", "measure"):
        raise ValueError("phase must be seed or measure")
    if type(body_count) is not int or body_count < 2 or body_count % 2:
        raise ValueError("body_count must be a positive even number of user/final-assistant rows")
    rounds = body_count // 2
    if phase == "seed" and body_count != 100:
        raise ValueError("seed readback requires the complete frozen 100-body corpus")
    if phase == "measure":
        raise ValueError("post-seed measure readback is not supported by this normalizer yet")
    if rounds > 50:
        raise ValueError("body_count exceeds the frozen profile for this phase")

    raw = _raw_session(raw_projection)
    native_conversation = raw.get("native_conversation")
    if isinstance(native_conversation, Mapping):
        source_session_id = native_conversation.get("sessionId")
        if source_session_id is not None and source_session_id != run_scope["sessionId"]:
            raise ValueError("raw native conversation belongs to a different session")
    for key in ("sessionId", "session_id"):
        source_session_id = raw.get(key)
        if source_session_id is not None and source_session_id != run_scope["sessionId"]:
            raise ValueError("raw projection belongs to a different session")
    transcript = _mapping(snapshot.get("transcript"), "RunSnapshot.transcript")
    ordinary, context_anchors = _ordinary_rows(transcript, history_rows, raw)
    if len(ordinary) != body_count:
        raise ValueError("public history does not contain exactly the requested body_count")

    normalized_rows: list[dict[str, Any]] = []
    body_anchors: list[dict[str, Any]] = []
    positions_seen: set[tuple[Any, ...]] = set()
    for index, row in enumerate(ordinary):
        role = row.get("kind", row.get("role"))
        if role not in _BODY_KINDS:
            raise ValueError("ordinary history row has an invalid role")
        if row.get("isProvisional") is not False:
            raise ValueError("provisional transcript rows cannot satisfy historical readback")
        if role == "assistant" and row.get("isFinal") is not True:
            raise ValueError("assistant transcript row is not final")
        position = _position(row.get("sourcePosition"), f"body row {index} source position")
        anchor = _source_anchor(position)
        position_key = tuple(anchor.items())
        if position_key in positions_seen:
            raise ValueError("duplicate source position in public body history")
        positions_seen.add(position_key)
        text = row.get("text")
        if not isinstance(text, str):
            raise ValueError("public transcript body is missing text")
        turn = _turn_number(row, text, phase)
        expected_text = body_text(phase, session_index, turn, role)
        if text != expected_text or len(text.encode("utf-8")) != BODY_BYTES:
            raise ValueError("public transcript body is not the complete expected 1024-byte fixture")
        source_text = _body_from_source(raw, position, role)
        if source_text != text:
            raise ValueError("public transcript body disagrees with its original raw source")
        turn_id = row.get("turnId")
        if not isinstance(turn_id, str) or run_scope["sessionId"] not in turn_id:
            raise ValueError("canonical transcript turn does not identify the requested session")
        normalized_rows.append({"role": role, "text": text, "isFinal": row.get("isFinal") is True})
        body_anchors.append(anchor)

    expected_turns = list(range(1, rounds + 1)) if phase == "seed" else list(range(0, rounds))
    found_turns: list[int] = []
    for index in range(0, len(normalized_rows), 2):
        user_row, assistant_row = normalized_rows[index:index + 2]
        user_turn = _turn_number(ordinary[index], user_row["text"], phase)
        assistant_turn = _turn_number(ordinary[index + 1], assistant_row["text"], phase)
        if user_row["role"] != "user" or assistant_row["role"] != "assistant" or user_turn != assistant_turn:
            raise ValueError("public body rows are not paired user/final-assistant turns")
        found_turns.append(user_turn)
    if found_turns != expected_turns:
        raise ValueError("public body rows contain missing, duplicated, or old turns")

    tools = transcript.get("tools", [])
    if not isinstance(tools, Sequence) or isinstance(tools, (str, bytes)):
        raise ValueError("RunSnapshot transcript tools must be an array")
    expected_tool_count = SEED_TOOL_TURNS if phase == "seed" else 0
    if len(tools) != expected_tool_count:
        raise ValueError("public transcript tool groups do not match the frozen profile")
    turn_ids: dict[int, Any] = {}
    for row, text in zip(ordinary, [row["text"] for row in normalized_rows]):
        if row.get("turnId") is not None:
            if run_scope["sessionId"] not in str(row["turnId"]):
                raise ValueError("canonical transcript turn does not identify the requested session")
            turn_ids[_turn_number(row, text, phase)] = row["turnId"]
    validated_groups: list[dict[str, Any]] = []
    for index, value in enumerate(tools):
        tool = _mapping(value, f"tool group {index}")
        turn = index + 1
        turn_id = tool.get("turnId")
        if turn_id is None or turn_id not in turn_ids.values():
            raise ValueError("tool group has no source-matched canonical turn")
        matching_turns = [number for number, identity in turn_ids.items() if identity == turn_id]
        if matching_turns != [turn]:
            raise ValueError("tool group is attached to an unexpected or old turn")
        validated_groups.append(_validate_tool_group(
            raw, tool, turn, expected_tool_call_id(session_index, turn), turn_id,
        ))

    validation = validate_history(
        normalized_rows,
        [{key: group[key] for key in ("turn", "toolName", "callId", "resultCallId", "resultText")}
         for group in validated_groups],
        session_index,
        phase=phase,
        rounds=rounds,
    )
    return {
        "validation": validation,
        "sourceIdentity": {
            "endpointId": run_scope["endpointId"],
            "sessionId": run_scope["sessionId"],
            "sessionIndex": session_index,
            "phase": phase,
            "bodyCount": body_count,
            "bodyRows": body_anchors,
            "contextRows": context_anchors,
            "toolGroups": [{"call": group["sourcePosition"], "result": group["resultPosition"]}
                           for group in validated_groups],
        },
    }
