"""Conservative checks for a Native Task in an original Summary publication."""

from __future__ import annotations

from typing import Any


class NativeSummarySourceError(ValueError):
    """Raised when a Native Summary row cannot be tied to its source call."""


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise NativeSummarySourceError(f"{label} must be an object")
    return value


def _same_scope(value: Any, expected: dict[str, str], label: str) -> None:
    if _object(value, label) != expected:
        raise NativeSummarySourceError(f"{label} does not match the parent Run")


def validate_witness(witness: Any, *, endpoint: str = "native") -> dict[str, Any]:
    """Match one passive Summary Task to its exact same-frame Native tool call.

    The validator only checks the supplied public witness. It cannot establish
    that a producer did not fabricate the entire witness or that remote server
    streams were transactionally consistent.
    """
    root = _object(witness, "witness")
    if root.get("evidenceScope") != "original-publication-and-same-memory-input":
        raise NativeSummarySourceError("unsupported evidence scope")
    if root.get("serverTransactionalFreshnessProven") is not False:
        raise NativeSummarySourceError("server freshness must remain explicitly unproven")
    if root.get("projectionSucceeded") is not True:
        raise NativeSummarySourceError("degraded or failed projection is not fresh evidence")

    snapshot = _object(root.get("snapshot"), "snapshot")
    authority = _object(root.get("authority"), "authority")
    summary = _object(snapshot.get("summary"), "summary")
    scope = _object(summary.get("scope"), "Summary scope")
    parent = scope.get("sessionId")
    expected_scope = {"endpointId": endpoint, "threadId": parent, "sessionId": parent}
    if not isinstance(parent, str) or not parent:
        raise NativeSummarySourceError("invalid parent Run address")
    _same_scope(scope, expected_scope, "Summary scope")
    if authority.get("endpoint_id") != endpoint:
        raise NativeSummarySourceError("authority belongs to another endpoint")
    epoch = snapshot.get("connectionEpoch")
    if type(epoch) is not int or epoch <= 0 or epoch != authority.get("connection_epoch"):
        raise NativeSummarySourceError("snapshot and authority epochs differ")

    selection = _object(snapshot.get("selection"), "selection")
    destination = _object(selection.get("destination"), "selection destination")
    if destination.get("kind") != "run":
        raise NativeSummarySourceError("published selection is not a Run")
    _same_scope(destination.get("scope"), expected_scope, "selected Run scope")
    if authority.get("connection") != "Ready" or not all(
        authority.get(key) is True
        for key in ("inventory_ready", "control_ready", "actionables_ready")
    ):
        raise NativeSummarySourceError("authority is not ready")

    sessions = _object(authority.get("sessions"), "authority sessions")
    parent_projection = _object(sessions.get(parent), "parent authority projection")
    if parent_projection.get("journal_ready") is not True:
        raise NativeSummarySourceError("parent Native source is not ready")

    transcript = _object(snapshot.get("transcript"), "published transcript")
    if transcript.get("isStale") is not False:
        raise NativeSummarySourceError("retained stale transcript is not fresh evidence")
    tools = transcript.get("tools")
    if not isinstance(tools, list):
        raise NativeSummarySourceError("published tool source is missing")
    tasks = summary.get("tasks")
    if not isinstance(tasks, list):
        raise NativeSummarySourceError("Summary task rows are missing")

    matched: list[tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]] = []
    for row_value in tasks:
        row = _object(row_value, "Summary task row")
        if row.get("kind") != "task":
            continue
        _same_scope(row.get("scope"), expected_scope, "Summary Task scope")
        # AgentTask has no child-session pointer. A Native task remains passive
        # in its parent Run; a dynamically injected sessionId is rejected.
        if "sessionId" in row or "independentRun" in row:
            raise NativeSummarySourceError("Native parent Task must remain passive")
        row_id = row.get("id")
        if not isinstance(row_id, str) or not row_id:
            raise NativeSummarySourceError("Summary Task identity is missing")
        row_tools = [tool for tool in tools if isinstance(tool, dict) and tool.get("id") == row_id]
        if len(row_tools) != 1:
            raise NativeSummarySourceError("Summary Task must match exactly one original tool row")
        tool = row_tools[0]
        if tool.get("name") != "task":
            raise NativeSummarySourceError("Summary Task is not sourced by the Native task tool")
        arguments = _object(tool.get("arguments"), "tool arguments")
        task_name = arguments.get("task_name")
        if not isinstance(task_name, str) or not task_name.strip() or row.get("name") != task_name:
            raise NativeSummarySourceError("Summary Task name differs from original tool arguments")
        phase = tool.get("phase")
        if not isinstance(phase, str) or not phase or row.get("status") != phase:
            raise NativeSummarySourceError("Summary Task status differs from same-frame tool phase")

        position = _object(tool.get("sourcePosition"), "tool source position")
        if position.get("kind") == "journal":
            entries = _object(parent_projection.get("entries"), "original journal")
            entry = _object(entries.get(str(position.get("sequence"))), "original call entry")
            event = _object(entry.get("event"), "original call event")
            call = _object(event.get("data"), "original call data")
            if (event.get("type") != "tool/call" or event.get("seq") != position.get("sequence")
                    or call.get("name") != "task" or call.get("arguments") != arguments):
                raise NativeSummarySourceError("Task arguments differ from original journal call")
            result_position = _object(tool.get("resultPosition"), "original result position")
            result_entry = _object(entries.get(str(result_position.get("sequence"))), "original result entry")
            result_event = _object(result_entry.get("event"), "original result event")
            message = result_event.get("data", {}).get("message", {})
            if (result_position.get("kind") != "journal" or result_event.get("type") != "tool/result"
                    or result_event.get("seq") != result_position.get("sequence")
                    or message != tool.get("result")
                    or message.get("source", {}).get("callId") != call.get("callId")):
                raise NativeSummarySourceError("Task result differs from original journal result")
            matched.append((row, tool, position, call))
            continue
        if position.get("kind") != "native":
            raise NativeSummarySourceError("Task call has no recognized public source position")
        native = _object(parent_projection.get("native_conversation"), "Native conversation")
        native_entries = native.get("entries")
        if not isinstance(native_entries, list):
            raise NativeSummarySourceError("Native conversation entries are missing")
        ordinal = position.get("entryOrdinal")
        content_index = position.get("contentIndex")
        entry_id = position.get("entryId")
        if type(ordinal) is not int or not 0 <= ordinal < len(native_entries):
            raise NativeSummarySourceError("Native source ordinal is out of range")
        if type(content_index) is not int or content_index < 0 or not isinstance(entry_id, str):
            raise NativeSummarySourceError("Native source position is incomplete")
        entry = _object(native_entries[ordinal], "Native source entry")
        if entry.get("id") != entry_id:
            raise NativeSummarySourceError("Native source entry identity differs from its position")
        message = _object(entry.get("message"), "Native source message")
        if message.get("role") != "assistant":
            raise NativeSummarySourceError("Native task source is not an assistant tool call")
        content = message.get("content")
        if not isinstance(content, list) or content_index >= len(content):
            raise NativeSummarySourceError("Native source content index is out of range")
        call = _object(content[content_index], "Native source tool call")
        if call.get("type") != "toolCall" or call.get("name") != "task":
            raise NativeSummarySourceError("original Native content is not the task call")
        if call.get("arguments") != arguments:
            raise NativeSummarySourceError("tool arguments differ from the original Native call")
        # The opaque tool ID and native call ID are intentionally not decoded
        # or compared by splitting their strings.
        matched.append((row, tool, position, call))

    if len(matched) != 1:
        raise NativeSummarySourceError("expected exactly one source-backed passive Native Task")
    row, tool, position, _call = matched[0]
    return {
        "endpoint": endpoint,
        "parentSessionId": parent,
        "summaryRow": row,
        "toolPhase": tool["phase"],
        "sourceIdentity": position,
        "passiveParentTask": True,
        "projectionSucceeded": True,
        "serverTransactionalFreshnessProven": False,
        "evidenceScope": "single-publication-same-memory-authority",
        "fabricationResistance": "not-proven-by-json-validation",
    }
