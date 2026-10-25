"""Strict, source-bound validators for the frozen Native/DSH cancel slice."""
from __future__ import annotations

import json
from typing import Any


class LifecycleEvidenceError(ValueError):
    """Raised when a cancellation receipt does not prove the original run."""


def validate_cancel(endpoint: str, before: Any, after: Any, *, ticket_id: str,
                    expected_prompt: str, expected_draft: str, provider: Any) -> dict[str, Any]:
    if endpoint not in ("native", "dsh"):
        raise LifecycleEvidenceError("endpoint is outside the frozen Native/DSH slice")
    first, last = _object(before, "cancellable capture"), _object(after, "cancelled capture")
    scope = _object(first.get("scope"), "original Run scope")
    if scope.get("endpointId") != endpoint or last.get("scope") != scope:
        raise LifecycleEvidenceError("cancel changed the original Run scope")
    if not _positive(first.get("publication")) or not _positive(last.get("publication")) \
            or last["publication"] <= first["publication"]:
        raise LifecycleEvidenceError("cancelled publication does not follow the cancellable Run")
    if first.get("connectionEpoch") != last.get("connectionEpoch"):
        raise LifecycleEvidenceError("connection epoch changed during cancellation")
    if first.get("projectionSucceeded") is not True or last.get("projectionSucceeded") is not True \
            or first.get("sameMemoryAuthority") is not True or last.get("sameMemoryAuthority") is not True:
        raise LifecycleEvidenceError("captures lack successful same-memory source witnesses")
    _authority_scope(first, scope)
    _authority_scope(last, scope)
    initial_ticket = _ticket(first, ticket_id)
    if initial_ticket.get("phase") not in ("dispatched", "admitted"):
        raise LifecycleEvidenceError("original submission was not dispatched before cancel")
    if last.get("ticketId") != ticket_id or last.get("cancelCallCompleted") is not True:
        raise LifecycleEvidenceError("cancel callback did not complete for the original ticket")
    final_ticket = _ticket(last, ticket_id)
    if initial_ticket.get("operation") != final_ticket.get("operation"):
        raise LifecycleEvidenceError("original submission operation changed during cancel")
    for row in (initial_ticket, final_ticket):
        resolved = row.get("resolvedScope")
        if resolved is not None and resolved != scope:
            raise LifecycleEvidenceError("original submission resolved to a different Run scope")

    before_turns = _turns(first, expected_prompt)
    after_turns = _turns(last, expected_prompt)
    if len(before_turns) != 1 or len(after_turns) != 1 \
            or before_turns[0].get("id") != after_turns[0].get("id") \
            or before_turns[0].get("status") != "running" \
            or after_turns[0].get("status") != "aborted":
        raise LifecycleEvidenceError("same original Transcript turn lacks a running-to-aborted transition")
    turn_id = after_turns[0].get("id")
    if last.get("run", {}).get("canCancel") is not False \
            or last.get("session", {}).get("run_open") is not False:
        raise LifecycleEvidenceError("original Run remains active after cancellation")
    if last.get("draft", {}).get("text") != expected_draft or not expected_draft:
        raise LifecycleEvidenceError("post-prepare Composer draft was not preserved")

    session = _object(last.get("session"), "original session source")
    if endpoint == "native":
        _native_terminal(session, scope, ticket_id, expected_prompt, turn_id)
    else:
        _dsh_terminal(session, scope, ticket_id, expected_prompt, turn_id)

    proof = _object(provider, "held loopback provider observation")
    title_count = proof.get("titleRequests", 0)
    request_rows = proof.get("requests")
    business_rows = [row for row in request_rows if isinstance(row, dict)
                     and row.get("kind") == "prompt" and row.get("markerPresent") is True] \
        if isinstance(request_rows, list) else []
    title_rows = [row for row in request_rows if isinstance(row, dict)
                  and row.get("kind") == "title"] if isinstance(request_rows, list) else []
    accounting_invalid = (
        type(title_count) is not int or title_count < 0
        or proof.get("requestCount") != title_count + proof.get("matchingBusinessRequests", 0)
            + proof.get("unexpectedRequestCount", 0)
        or not isinstance(request_rows, list)
        or len(request_rows) != proof.get("requestCount")
        or len(business_rows) != proof.get("matchingBusinessRequests")
        or len(title_rows) != title_count
    )
    if accounting_invalid:
        raise LifecycleEvidenceError("provider request accounting does not reconcile title and business requests")
    if proof.get("matchingBusinessRequests") != 1 or proof.get("firstStreamFrameSent") is not True \
            or proof.get("connectionClosedAfterCancel") is not True \
            or proof.get("heldHandlerTimedOut") is not False \
            or proof.get("requestBodiesPersisted") is not False \
            or proof.get("providerErrorType") is not None or proof.get("unexpectedRequestCount") != 0:
        raise LifecycleEvidenceError("provider did not prove one held original request closed after cancel")
    if proof.get("handlerThreadsStopped") is not True or proof.get("activeHandlers") != 0:
        raise LifecycleEvidenceError("owned provider handlers were not stopped before acceptance")
    return {"accepted": True, "endpoint": endpoint, "ticketId": ticket_id,
            "sameRunScope": True, "sameConnectionEpoch": True,
            "sameOriginalTurnAborted": True, "postPrepareDraftPreserved": True,
            "heldProviderClosed": True, "providerBusinessRequests": 1,
            "dshTitleRequests": proof.get("titleRequests", 0) if endpoint == "dsh" else 0}


def _native_terminal(session: dict[str, Any], scope: dict[str, Any], ticket_id: str,
                     prompt: str, turn_id: Any) -> None:
    entries = _journal_events(session, "Native journal")
    decoded = _decode_turn(turn_id)
    if decoded[:2] != [scope.get("endpointId"), scope.get("sessionId")] \
            or not isinstance(decoded[2], str) or not decoded[2].startswith("turn:"):
        raise LifecycleEvidenceError("public Transcript turn is not this Native journal turn")
    _require_journal_cancel(entries, decoded[2][len("turn:"):], ticket_id, prompt,
                            ("aborted",), "Native")


def _dsh_terminal(session: dict[str, Any], scope: dict[str, Any], ticket_id: str,
                  prompt: str, turn_id: Any) -> None:
    decoded = _decode_turn(turn_id)
    if decoded[:2] != [scope.get("endpointId"), scope.get("sessionId")] \
            or not isinstance(decoded[2], str) or not decoded[2].startswith("turn:"):
        raise LifecycleEvidenceError("public Transcript turn is not this DSH journal turn")
    number = decoded[2][len("turn:"):]
    ordered = _journal_events(session, "DSH public journal")
    _require_journal_cancel(ordered, number, ticket_id, prompt,
                            ("aborted", "cancelled"), "DSH")


def _journal_events(session: dict[str, Any], label: str) -> list[tuple[int, dict[str, Any]]]:
    entries = session.get("entries")
    if not isinstance(entries, dict):
        raise LifecycleEvidenceError(f"{label} entries are absent")
    ordered = []
    for entry in entries.values():
        if not isinstance(entry, dict) or not isinstance(entry.get("event"), dict):
            continue
        event = entry["event"]
        seq = event.get("seq")
        if type(seq) is int:
            ordered.append((seq, event))
    ordered.sort(key=lambda pair: pair[0])
    return ordered


def _require_journal_cancel(ordered: list[tuple[int, dict[str, Any]]], number: str,
                            ticket_id: str, prompt: str, accepted_reasons: tuple[str, ...],
                            label: str) -> None:
    user = []
    active_turn = None
    for seq, event in ordered:
        data = event.get("data", {})
        if event.get("type") == "turn/start":
            active_turn = str(data.get("turn"))
        elif event.get("type") == "user/message" and active_turn == number:
            # Native user/message inherits the turn from journal ordering;
            # it does not require a duplicated data.turn field.
            if data.get("turn") is not None and str(data["turn"]) != number:
                raise LifecycleEvidenceError(f"{label} user message has conflicting turn identity")
            if (_message_text(data.get("text", data.get("content"))) == prompt
                    and (data.get("source", {}).get("rpcId") == ticket_id
                         or data.get("submission", {}).get("rpcId") == ticket_id)):
                user.append((seq, event))
        elif event.get("type") == "turn/end" and str(data.get("turn")) == active_turn:
            active_turn = None
    if len(user) != 1:
        raise LifecycleEvidenceError(f"{label} journal user message does not uniquely bind original ticket and turn")
    starts = [(seq, event) for seq, event in ordered if event.get("type") == "turn/start"
              and str(event.get("data", {}).get("turn")) == number]
    ends = [(seq, event) for seq, event in ordered if event.get("type") == "turn/end"
            and str(event.get("data", {}).get("turn")) == number
            and _cancel_reason(event.get("data", {}).get("reason"), accepted_reasons)]
    if len(starts) != 1 or len(ends) != 1 \
            or not (starts[0][0] < user[0][0] < ends[0][0]):
        raise LifecycleEvidenceError(f"{label} journal lacks the original turn's ordered cancelled/aborted terminal")


def _cancel_reason(reason: Any, accepted: tuple[str, ...]) -> bool:
    if isinstance(reason, str):
        return reason in accepted
    return isinstance(reason, dict) and reason.get("kind") in accepted


def _decode_turn(value: Any) -> list[Any]:
    if not isinstance(value, str):
        raise LifecycleEvidenceError("public Transcript turn identity is absent")
    try:
        decoded = json.loads(value)
    except (json.JSONDecodeError, TypeError):
        raise LifecycleEvidenceError("public Transcript turn identity is malformed") from None
    if not isinstance(decoded, list) or len(decoded) != 3:
        raise LifecycleEvidenceError("public Transcript turn identity has an unexpected shape")
    return decoded


def _message_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(block.get("text", "") for block in content
                       if isinstance(block, dict) and block.get("type") == "text")
    return ""


def _turns(capture: dict[str, Any], prompt: str) -> list[dict[str, Any]]:
    transcript = _object(capture.get("transcript"), "public Transcript")
    rows = transcript.get("turns")
    return [row for row in rows if isinstance(row, dict) and row.get("input") == prompt] \
        if isinstance(rows, list) else []


def _ticket(capture: dict[str, Any], ticket_id: str) -> dict[str, Any]:
    row = capture.get("submission")
    if not isinstance(row, dict) or row.get("id") != ticket_id:
        raise LifecycleEvidenceError("Run capture has no exact original submission")
    return row


def _authority_scope(capture: dict[str, Any], scope: dict[str, Any]) -> None:
    authority = _object(capture.get("authority"), "same-memory authority")
    sessions = authority.get("sessions")
    if (authority.get("endpoint_id") != scope.get("endpointId")
            or authority.get("connection_epoch") != capture.get("connectionEpoch")
            or authority.get("connection") != "Ready"
            or not isinstance(sessions, dict)
            or sessions.get(scope.get("sessionId")) != capture.get("session")):
        raise LifecycleEvidenceError("same-memory authority differs from this session/endpoint/epoch")



def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise LifecycleEvidenceError(f"{label} is absent or malformed")
    return value


def _positive(value: Any) -> bool:
    return type(value) is int and value > 0
