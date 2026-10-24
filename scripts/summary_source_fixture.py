"""Conservative validator for a single Pi presentation-source witness."""

from __future__ import annotations

from typing import Any


class SummarySourceEvidenceError(ValueError):
    """Raised when the witness cannot prove one same-authority Summary row."""


def _dict(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SummarySourceEvidenceError(f"{label} must be an object")
    return value


def validate_witness(witness: Any, *, endpoint: str = "pi") -> dict[str, Any]:
    """Match the published parent Summary to its same-frame memory authority.

    This checks consistency and provenance fields in the captured JSON. It does
    not claim protection against a producer that fabricates the entire witness.
    """
    root = _dict(witness, "witness")
    if root.get("evidenceScope") != "original-publication-and-same-memory-input":
        raise SummarySourceEvidenceError("unsupported evidence scope")
    if root.get("serverTransactionalFreshnessProven") is not False:
        raise SummarySourceEvidenceError("server freshness must remain explicitly unproven")
    if root.get("projectionSucceeded") is not True:
        raise SummarySourceEvidenceError("degraded or failed projection is not fresh evidence")
    snapshot = _dict(root.get("snapshot"), "snapshot")
    authority = _dict(root.get("authority"), "authority")
    summary = _dict(snapshot.get("summary"), "summary")
    scope = _dict(summary.get("scope"), "summary scope")
    if scope != {"endpointId": endpoint, "threadId": scope.get("threadId"),
                 "sessionId": scope.get("sessionId")}:
        raise SummarySourceEvidenceError("Summary scope is not the requested endpoint")
    parent = scope.get("sessionId")
    if not isinstance(parent, str) or not parent or scope.get("threadId") != parent:
        raise SummarySourceEvidenceError("invalid parent Run address")
    if authority.get("endpoint_id") != endpoint:
        raise SummarySourceEvidenceError("authority belongs to another endpoint")
    if (not isinstance(snapshot.get("connectionEpoch"), int)
            or snapshot["connectionEpoch"] <= 0
            or snapshot["connectionEpoch"] != authority.get("connection_epoch")):
        raise SummarySourceEvidenceError("snapshot and authority epochs differ")
    selection = _dict(snapshot.get("selection"), "selection")
    destination = _dict(selection.get("destination"), "selection destination")
    if destination.get("kind") != "run" or destination.get("scope") != scope:
        raise SummarySourceEvidenceError("published selection points at another endpoint or Run")
    if authority.get("connection") != "Ready" or not all(authority.get(k) is True for k in
            ("inventory_ready", "control_ready", "actionables_ready")):
        raise SummarySourceEvidenceError("authority is not ready")
    sessions = _dict(authority.get("sessions"), "authority sessions")
    parent_projection = _dict(sessions.get(parent), "parent authority projection")
    if parent_projection.get("journal_ready") is not True:
        raise SummarySourceEvidenceError("parent source journal is not ready")
    entries = parent_projection.get("entries")
    if not isinstance(entries, dict):
        raise SummarySourceEvidenceError("parent source entries are missing")
    transcript = _dict(snapshot.get("transcript"), "published transcript")
    if transcript.get("isStale") is not False:
        raise SummarySourceEvidenceError("retained stale transcript is not fresh evidence")
    tools = transcript.get("tools")
    if not isinstance(tools, list):
        raise SummarySourceEvidenceError("published tool source is missing")
    rows = summary.get("subagents")
    if not isinstance(rows, list):
        raise SummarySourceEvidenceError("Summary subagent rows are missing")

    matches = []
    for row_value in rows:
        row = _dict(row_value, "Summary subagent row")
        child_id = row.get("sessionId")
        row_id = row.get("id")
        if not isinstance(child_id, str) or not child_id or not isinstance(row_id, str):
            continue
        child = _dict(sessions.get(child_id), "child authority projection")
        child_summary = _dict(child.get("summary"), "child endpoint summary")
        if child_summary.get("sessionId") != child_id or child_id == parent:
            raise SummarySourceEvidenceError("child inventory identity mismatch")
        # The public child pointer must be present in a source tool result in
        # the exact parent authority captured with this Snapshot.
        for tool in tools:
            if not isinstance(tool, dict) or tool.get("id") != row_id:
                continue
            # Use public positions, never split the opaque scoped row ID.
            position = tool.get("resultPosition", {})
            if position.get("kind") == "journal":
                source_entry = entries.get(str(position.get("sequence")))
                if not isinstance(source_entry, dict):
                    continue
                event = source_entry.get("event", {})
                if (event.get("type") != "tool/result"
                        or event.get("seq") != position.get("sequence")):
                    continue
                raw_message = event.get("data", {}).get("message")
                source_identity = {"kind":"journal", "sequence":event["seq"]}
            elif position.get("kind") == "native":
                native = parent_projection.get("native_conversation") or {}
                native_entries = native.get("entries", [])
                ordinal = position.get("entryOrdinal")
                if type(ordinal) is not int or not 0 <= ordinal < len(native_entries):
                    continue
                source_entry = native_entries[ordinal]
                if source_entry.get("id") != position.get("entryId"):
                    continue
                raw_message = source_entry.get("message")
                source_identity = {"kind":"native", "entryId":source_entry["id"], "entryOrdinal":ordinal}
            else:
                continue
            if raw_message != tool.get("result"):
                continue
            raw_result = raw_message.get("content")
            if child_id not in str(raw_result):
                continue
            raw_status = child_summary.get("status")
            status_basis = "explicit-child-status"
            if raw_status in (None, "", "active", "archived"):
                # This profile admits only the narrow public idle fallback,
                # not a second implementation of the general activity policy.
                idle_inputs = (child_summary.get("running") is False
                    and child_summary.get("blank") is False
                    and child.get("run_open") is False
                    and child.get("queue") == [] and child.get("entries") == {}
                    and child_summary.get("runState") not in ("running", "parked")
                    and not any(a.get("sessionId") == child_id
                                for a in authority.get("actionables", {}).values()))
                if not idle_inputs or row.get("status") != "idle":
                    raise SummarySourceEvidenceError("unsupported or inconsistent activity fallback")
                status_basis = "public-idle-inputs-no-explicit-child-status"
            elif raw_status != row.get("status"):
                raise SummarySourceEvidenceError("Summary status differs from same-frame child authority")
            matches.append((row, source_identity, child, status_basis))
    if len(matches) != 1:
        raise SummarySourceEvidenceError("expected exactly one Summary row matched to an owned source child")
    row, source_identity, child, status_basis = matches[0]
    expected_indicator = "running" if row.get("status") in ("running", "in_progress", "working") else (
        "failed" if row.get("status") in ("failed", "interrupted") else None)
    return {
        "endpoint": endpoint,
        "parentSessionId": parent,
        "childSessionId": row["sessionId"],
        "summaryRow": row,
        "sourceIdentity": source_identity,
        "rawChildStatus": child["summary"].get("status"),
        "childStatus": row["status"],
        "statusBasis": status_basis,
        "indicator": expected_indicator,
        "projectionSucceeded": True,
        "serverTransactionalFreshnessProven": False,
        "evidenceScope": "single-publication-same-memory-authority",
        "fabricationResistance": "not-proven-by-json-validation",
    }
