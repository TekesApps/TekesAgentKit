"""Strict validators for the frozen Pi Run cancel/Unknown acceptance slice."""
from __future__ import annotations

from typing import Any


class LifecycleEvidenceError(ValueError):
    """Raised when public Run evidence does not bind the original submission."""


def validate_unknown_reconciliation(before: Any, after: Any, *, ticket_id: str,
                                    expected_prompt: str, expected_draft: str, provider: Any,
                                    gate_events: Any) -> dict[str, Any]:
    first = _object(before, "Unknown capture")
    last = _object(after, "reconciled capture")
    if first.get("scope") != last.get("scope") or first["scope"].get("endpointId") != "pi":
        raise LifecycleEvidenceError("Unknown reconciliation changed its Pi Run scope")
    if first.get("connectionEpoch") != last.get("connectionEpoch"):
        raise LifecycleEvidenceError("original Pi connection epoch changed before reconciliation")
    if not _positive_int(first.get("publication")) or not _positive_int(last.get("publication")) \
            or last["publication"] <= first["publication"]:
        raise LifecycleEvidenceError("reconciliation publication does not follow Unknown")
    initial, final = _ticket(first, ticket_id), _ticket(last, ticket_id)
    if initial.get("phase") != "unknown" or initial.get("reconciled") is not False:
        raise LifecycleEvidenceError("original ticket did not enter unreconciled Unknown")
    if final.get("id") != ticket_id or final.get("reconciled") is not True:
        raise LifecycleEvidenceError("the exact original ticket was not automatically reconciled")
    if final.get("phase") not in ("unknown", "failed", "admitted"):
        raise LifecycleEvidenceError("reconciled ticket has an unrecognized terminal submission phase")
    if first.get("draft", {}).get("text") != expected_draft \
            or last.get("draft", {}).get("text") != expected_draft or not expected_draft:
        raise LifecycleEvidenceError("Unknown reconciliation changed the parent Composer draft")
    if first.get("sameMemoryAuthority") is not True or last.get("sameMemoryAuthority") is not True \
            or first.get("projectionSucceeded") is not True or last.get("projectionSucceeded") is not True:
        raise LifecycleEvidenceError("captures lack successful same-memory public source witnesses")
    _require_authority_scope(first)
    _require_authority_scope(last)
    if first.get("serverTransactionalFreshnessProven") is not False \
            or last.get("serverTransactionalFreshnessProven") is not False:
        raise LifecycleEvidenceError("server-side freshness must remain unclaimed")
    raw_after = _pi_receipts(last)
    receipt = raw_after.get(ticket_id)
    if receipt is None:
        raise LifecycleEvidenceError("post-release Native Pi source lost the original durable rpcId receipt")
    if receipt.get("hash") != _sha256(expected_prompt) or receipt.get("mode") not in ("followUp", "started"):
        raise LifecycleEvidenceError("raw native input receipt does not bind the exact submitted prompt")
    if len(_matching_user_receipt(last, ticket_id, expected_prompt)) != 1:
        raise LifecycleEvidenceError("public user Transcript row does not join exactly to the original rpcId receipt")
    if not _raw_receipt_matches_user(last, receipt, expected_prompt):
        raise LifecycleEvidenceError("raw native input receipt timestamp/hash does not bind its user message")
    receipt = _object(provider, "provider observation")
    if receipt.get("requestCount") != 1 or receipt.get("businessRequests") != 1 \
            or receipt.get("matchingBusinessRequests") != 1 \
            or receipt.get("requestBodiesPersisted") is not False or receipt.get("providerErrorType") is not None:
        raise LifecycleEvidenceError("provider did not observe exactly one redacted original business mutation")
    events = gate_events if isinstance(gate_events, list) else []
    names = [event.get("event") for event in events if isinstance(event, dict)]
    if names.count("gate_ready") != 1 or names.count("mutation_forwarded") != 1 \
            or names.count("released") != 1 or names.index("gate_ready") > names.index("mutation_forwarded") \
            or names.index("mutation_forwarded") > names.index("released"):
        raise LifecycleEvidenceError("Pi response gate is missing its one-way send/release ordering")
    if last.get("secondDispatch") is not False or last.get("manualAcknowledgement") is not False:
        raise LifecycleEvidenceError("Unknown was retried or manually acknowledged")
    return {"accepted": True, "endpoint": "pi", "ticketId": ticket_id,
            "sameRunScope": True, "sameConnectionEpoch": True, "autoReconciled": True,
            "providerBusinessMutations": 1, "noRetry": True,
            "serverTransactionalFreshnessProven": False}


def validate_cancel(before: Any, after: Any, *, ticket_id: str, expected_prompt: str,
                    expected_draft: str, provider: Any) -> dict[str, Any]:
    first, last = _object(before, "cancellable capture"), _object(after, "cancelled capture")
    scope = _object(first.get("scope"), "original Run scope")
    if scope.get("endpointId") != "pi" or last.get("scope") != scope:
        raise LifecycleEvidenceError("cancel changed the original Pi Run scope")
    if not _positive_int(first.get("publication")) or not _positive_int(last.get("publication")) \
            or last["publication"] <= first["publication"]:
        raise LifecycleEvidenceError("cancelled publication does not follow the cancellable Run")
    if first.get("connectionEpoch") != last.get("connectionEpoch"):
        raise LifecycleEvidenceError("connection epoch changed during cancellation")
    if _ticket(first, ticket_id).get("phase") not in ("dispatched", "admitted"):
        raise LifecycleEvidenceError("original submission was not dispatched before cancel")
    if last.get("ticketId") != ticket_id or last.get("cancelCallCompleted") is not True:
        raise LifecycleEvidenceError("cancel callback did not target and complete for the original ticket")
    transcript = _object(last.get("transcript"), "settled public Transcript")
    turns = transcript.get("turns")
    matching = [t for t in turns if isinstance(t, dict) and t.get("input") == expected_prompt] \
        if isinstance(turns, list) else []
    if len(matching) != 1 or matching[0].get("status") != "aborted":
        raise LifecycleEvidenceError("same original prompt lacks a public aborted settlement")
    _require_authority_scope(first)
    _require_authority_scope(last)
    raw = _pi_receipts(last)
    if ticket_id not in raw or raw[ticket_id].get("hash") != _sha256(expected_prompt):
        raise LifecycleEvidenceError("cancelled Run lacks the raw receipt for its original prompt")
    if not _raw_receipt_matches_user(last, raw[ticket_id], expected_prompt):
        raise LifecycleEvidenceError("cancel input receipt does not bind the exact native user message")
    native = last.get("session", {}).get("native_conversation", {})
    entries = native.get("entries", []) if isinstance(native, dict) else []
    raw_aborted = _raw_aborted_for_turn(entries, matching[0].get("id"), scope)
    if not raw_aborted:
        raise LifecycleEvidenceError("raw Pi source does not prove an aborted settlement")
    if last.get("run", {}).get("canCancel") is not False or last.get("session", {}).get("run_open") is not False:
        raise LifecycleEvidenceError("original Run remains active after the cancel acknowledgement")
    if last.get("draft", {}).get("text") != expected_draft or not expected_draft:
        raise LifecycleEvidenceError("post-prepare Composer draft was not preserved")
    proof = _object(provider, "held provider observation")
    if proof.get("requestCount") != 1 or proof.get("matchingBusinessRequests") != 1 \
            or proof.get("firstStreamFrameSent") is not True \
            or proof.get("connectionClosedAfterCancel") is not True \
            or proof.get("requestBodiesPersisted") is not False or proof.get("providerErrorType") is not None:
        raise LifecycleEvidenceError("provider did not prove one held original request closed after cancel")
    return {"accepted": True, "endpoint": "pi", "ticketId": ticket_id,
            "sameRunScope": True, "samePromptAborted": True,
            "postPrepareDraftPreserved": True, "heldProviderClosed": True}


def _ticket(capture: dict[str, Any], ticket_id: str) -> dict[str, Any]:
    rows = capture.get("submissions")
    if not isinstance(rows, list):
        # The Rust consumer carries the selected ticket separately for convenient
        # serialization; all transition properties still come from public state.
        row = capture.get("submission")
        if isinstance(row, dict) and row.get("id") == ticket_id:
            return row
        raise LifecycleEvidenceError("Run capture has no submissions for the original ticket")
    matches = [row for row in rows if isinstance(row, dict) and row.get("id") == ticket_id]
    if len(matches) != 1:
        raise LifecycleEvidenceError("original ticket is missing or duplicated in public submission state")
    return matches[0]


def _pi_receipts(capture: dict[str, Any]) -> dict[str, dict[str, Any]]:
    session = _object(capture.get("session"), "same-memory Pi session source")
    native = _object(session.get("native_conversation"), "public Pi native conversation")
    entries = native.get("entries")
    if not isinstance(entries, list):
        raise LifecycleEvidenceError("Pi public native entries are unavailable")
    found: dict[str, dict[str, Any]] = {}
    for entry in entries:
        event = entry.get("event") if isinstance(entry, dict) and isinstance(entry.get("event"), dict) else entry
        if not isinstance(event, dict):
            continue
        if event.get("customType") in (
                "agentkit.inputReceipt.v1", "tekes.inputReceipt.v1"):
            data = event.get("data")
            if isinstance(data, dict) and isinstance(data.get("rpcId"), str):
                if data["rpcId"] in found:
                    raise LifecycleEvidenceError("duplicate raw Native Pi input receipt for one rpcId")
                found[data["rpcId"]] = data
    return found


def _matching_user_receipt(capture: dict[str, Any], ticket_id: str,
                           expected_prompt: str) -> list[dict[str, Any]]:
    transcript = _object(capture.get("transcript"), "public Transcript")
    items = transcript.get("items")
    if not isinstance(items, list):
        raise LifecycleEvidenceError("public Transcript items are missing")
    return [item for item in items if isinstance(item, dict)
            and item.get("kind") == "user" and item.get("reconciliationId") == ticket_id
            and item.get("text") == expected_prompt and item.get("isProvisional") is False]


def _require_authority_scope(capture: dict[str, Any]) -> None:
    authority = _object(capture.get("authority"), "public source authority")
    scope = _object(capture.get("scope"), "public Run scope")
    epoch = capture.get("connectionEpoch")
    sessions = authority.get("sessions")
    if authority.get("endpoint_id") != scope.get("endpointId") \
            or authority.get("connection_epoch") != epoch or authority.get("connection") != "Ready" \
            or not isinstance(sessions, dict) or sessions.get(scope.get("sessionId")) != capture.get("session"):
        raise LifecycleEvidenceError("source authority/session differs from the selected Run scope or epoch")


def _raw_receipt_matches_user(capture: dict[str, Any], receipt: dict[str, Any],
                              expected_prompt: str) -> bool:
    session = capture.get("session")
    native = session.get("native_conversation") if isinstance(session, dict) else None
    entries = native.get("entries") if isinstance(native, dict) else None
    if not isinstance(entries, list) or not isinstance(receipt.get("timestamp"), (int, float)):
        return False
    matches = []
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict) or entry.get("type") != "message":
            continue
        message = entry.get("message")
        if not isinstance(message, dict) or message.get("role") != "user":
            continue
        content = message.get("content")
        if isinstance(content, str):
            text = content
        elif isinstance(content, list):
            text = "\n".join(block.get("text", "") for block in content
                              if isinstance(block, dict) and block.get("type") == "text"
                              and isinstance(block.get("text"), str))
        else:
            continue
        if text == expected_prompt and message.get("timestamp") == receipt.get("timestamp") \
                and _sha256(text) == receipt.get("hash"):
            matches.append((index, entry))
    if len(matches) != 1:
        return False
    index, entry = matches[0]
    rows = _matching_user_receipt(capture, receipt.get("rpcId"), expected_prompt)
    if len(rows) != 1 or rows[0].get("recordId") != entry.get("id"):
        return False
    position = rows[0].get("sourcePosition")
    return (isinstance(position, dict) and position.get("kind") == "native"
            and position.get("entryId") == entry.get("id")
            and type(position.get("entryOrdinal")) is int and position["entryOrdinal"] == index)


def _raw_aborted_for_turn(entries: Any, turn_id: Any, scope: dict[str, Any]) -> bool:
    if not isinstance(entries, list) or not isinstance(turn_id, str):
        return False
    try:
        decoded = __import__("json").loads(turn_id)
    except (ValueError, TypeError):
        return False
    if not isinstance(decoded, list) or len(decoded) != 3 \
            or decoded[:2] != [scope.get("endpointId"), scope.get("sessionId")] \
            or not isinstance(decoded[2], str) or not decoded[2].startswith("native-run:"):
        return False
    run_id = decoded[2][len("native-run:"):]
    started_at = next((index for index, entry in enumerate(entries) if isinstance(entry, dict)
        and entry.get("customType") in ("agentkit.run.v1", "tekes.run.v1")
        and isinstance(entry.get("data"), dict)
        and entry["data"].get("phase") == "started"
        and entry["data"].get("id") == run_id), None)
    settled_at = next((index for index, entry in enumerate(entries) if isinstance(entry, dict)
        and entry.get("customType") in ("agentkit.run.v1", "tekes.run.v1")
        and isinstance(entry.get("data"), dict)
        and entry["data"].get("phase") == "settled"
        and entry["data"].get("id") == run_id
        and entry["data"].get("aborted") is True), None)
    return started_at is not None and settled_at is not None and started_at < settled_at


def _sha256(value: str) -> str:
    import hashlib
    return hashlib.sha256(value.encode()).hexdigest()


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise LifecycleEvidenceError(f"{label} must be an object")
    return value


def _positive_int(value: Any) -> bool:
    return type(value) is int and value > 0
