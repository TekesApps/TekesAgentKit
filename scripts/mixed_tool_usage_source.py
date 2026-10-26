#!/usr/bin/env python3
"""Validate opt-in mixed-tool usage receipts against their captured sources."""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def _fail(message: str) -> None:
    raise ValueError(message)


def _count(value, *keys):
    if not isinstance(value, dict):
        return None
    for key in keys:
        count = value.get(key)
        if isinstance(count, int) and not isinstance(count, bool) and count >= 0:
            return count
    return None


def _identity(frame):
    return tuple(frame.get(key) for key in ("endpointId", "threadId", "sessionId", "turnId"))


def _scoped_turn(frame):
    try:
        encoded = json.loads(frame["turnId"])
    except (KeyError, TypeError, json.JSONDecodeError):
        _fail("canonical turn id is not a scoped identity")
    if (not isinstance(encoded, list) or len(encoded) != 3
            or encoded[0] != frame.get("endpointId") or encoded[1] != frame.get("sessionId")
            or not isinstance(encoded[2], str)):
        _fail("canonical turn scope prefix differs from endpoint/session")
    return encoded[2]


def _turn_number(frame):
    suffix = _scoped_turn(frame)
    if not suffix.startswith("turn:"):
        _fail("journal source requires numeric turn identity")
    try:
        return int(suffix[5:])
    except ValueError:
        _fail("journal turn number is invalid")


def _source_usage(data):
    if isinstance(data.get("usage"), dict):
        return data["usage"]
    stream = data.get("stream")
    if isinstance(stream, list):
        for record in reversed(stream):
            if record.get("type") == "chunk" and record.get("chunk", {}).get("type") == "usage":
                return record["chunk"].get("usage", {})
    return None


def _raw_events(session):
    entries = session.get("entries")
    if not isinstance(entries, dict):
        _fail("captured raw journal entries missing")
    events = []
    for key, entry in entries.items():
        event = entry.get("event") if isinstance(entry, dict) else None
        if not isinstance(event, dict) or not isinstance(event.get("data"), dict):
            _fail("captured journal entry lacks original event")
        if str(event.get("seq")) != str(key):
            _fail("captured journal sequence key differs from original event")
        events.append(event)
    return sorted(events, key=lambda event: event.get("seq", -1))


def _validate_frame(frame, *, live=False):
    if not isinstance(frame, dict) or frame.get("projectionSucceeded") is not True:
        _fail("source frame is missing a successful same-publication projection witness")
    if not all(isinstance(part, str) and part for part in _identity(frame)):
        _fail("source frame identity is incomplete")
    suffix = _scoped_turn(frame)
    snapshot = frame.get("snapshot")
    authority = frame.get("authority")
    session = frame.get("rawSession")
    canonical = frame.get("canonical")
    if not all(isinstance(value, dict) for value in (snapshot, authority, session, canonical)):
        _fail("original snapshot, authority, raw session, or canonical projection missing")
    if (authority.get("endpointId") != frame["endpointId"]
            or authority.get("connectionEpoch") != frame.get("connectionEpoch")
            or authority.get("sessionKey") != frame["sessionId"]):
        _fail("captured authority endpoint/epoch/session differs from receipt")
    if snapshot.get("connectionEpoch") != frame.get("connectionEpoch"):
        _fail("captured snapshot epoch differs from receipt")
    if (type(frame.get("revision")) is not int or frame["revision"] < 1
            or snapshot.get("revision") != frame["revision"]):
        _fail("captured snapshot publication differs from receipt")
    destination = (snapshot.get("selection") or {}).get("destination", {})
    if (destination.get("kind") != "run" or destination.get("scope") != {
            'endpointId':frame['endpointId'], 'threadId':frame['threadId'], 'sessionId':frame['sessionId']}):
        _fail("captured original Composer destination differs from endpoint/thread/session")
    transcript = snapshot.get("transcript", {})
    turns = [turn for turn in transcript.get("turns", []) if turn.get("id") == frame["turnId"]]
    if len(turns) != 1 or turns[0] != canonical:
        _fail("canonical summary differs from the original captured transcript turn")
    source_kind = frame.get("sourceKind")
    if source_kind not in ("journal", "nativeConversation"):
        _fail("unknown raw usage source")
    sources = frame.get("liveSources" if live else "reportedSources")
    if not isinstance(sources, list):
        _fail("raw source list missing")
    usage = canonical.get("liveUsage" if live else "reportedUsage")
    if live:
        # A running turn's cumulative provider report includes already durable
        # steps. It can legitimately remain visible between steps with no
        # transient preview. Validate that durable part against the same raw
        # events before using it; never infer it from provider-fixture numbers.
        durable_proof = _validate_frame(frame, live=False)
        previews = session.get("previews")
        if not isinstance(previews, dict):
            _fail("captured raw previews missing")
        keys = [source.get("previewKey") for source in sources]
        if len(keys) != len(set(keys)):
            _fail("duplicate raw preview key")
        matched = []
        for key, preview in previews.items():
            parts = key.split(":", 1)
            if len(parts) != 2 or not parts[0].isdigit() or not parts[1].isdigit():
                continue
            if parts[0] == suffix.removeprefix("turn:") and _count(preview, "output_tokens") is not None:
                matched.append((key, preview))
        if keys != [key for key, _ in matched]:
            _fail("live source keys differ from same-turn raw previews")
        for source, (key, preview) in zip(sources, matched):
            if source.get("previewKey") != key or source.get("preview") != preview:
                _fail("live source preview differs from captured raw session")
            if source.get("outputTokens") != preview.get("output_tokens"):
                _fail("live preview usage summary differs from raw preview")
        if usage is None:
            if sources or (canonical.get("status") == "running"
                           and session.get("run_open") is True
                           and durable_proof.get("reportedObserved") is True):
                _fail("raw live source exists but canonical live usage is absent")
            return {"liveObserved": False, "status": "not_observed", "sourceKind": source_kind}
        expected = _count(usage, "outputTokens")
        if (expected is None or canonical.get("status") != "running"
                or session.get("run_open") is not True
                or (not sources and durable_proof.get("reportedObserved") is not True)):
            _fail("canonical live usage has no raw source witness")
        durable = _count(canonical.get("usage"), "outputTokens")
        raw = sum(_count(source.get("preview"), "output_tokens") or 0 for source in sources)
        if durable is None or durable + raw != expected:
            _fail("live usage does not match same-turn durable plus raw transient usage")
        return {"liveObserved": True, "outputTokens": expected, "sourceKind": source_kind,
                "transientPreviewObserved": bool(sources),
                "durableUsagePaired": durable_proof.get("reportedObserved") is True}

    canonical_usage = canonical.get("usage")
    reported = canonical.get("reportedUsage")
    if not isinstance(canonical_usage, dict):
        _fail("canonical durable usage missing")
    raw_outputs, raw_inputs = [], []
    settlement_output = settlement_input = None
    if source_kind == "journal":
        number = _turn_number(frame)
        events = _raw_events(session)
        turn_starts = [event for event in events if event.get("type") == "turn/start"
                       and event["data"].get("turn") == number]
        if frame.get("turnStarted") is not bool(turn_starts):
            _fail("turn-start summary differs from captured raw events")
        expected_events = [event for event in events if event.get("type") in ("assistant/message", "assistant/attempt")
                           and event["data"].get("turn") == number]
        source_events = [source.get("event") for source in sources]
        if source_events != expected_events:
            _fail("journal source list differs from captured assistant turn events")
        for source, event in zip(sources, expected_events):
            data = event["data"]
            value = _source_usage(data)
            if (source.get("sequence") != event.get("seq") or source.get("kind") != event.get("type")
                    or source.get("turn") != data.get("turn") or source.get("step") != data.get("step")
                    or source.get("recordId") != data.get("message", {}).get("id")
                    or source.get("usage") != value):
                _fail("journal usage summary differs from its original event")
            output = _count(value, "outputTokens", "output")
            input_count = _count(value, "inputTokens", "input")
            if source.get("outputKnown") is not (output is not None):
                _fail("journal output coverage flag does not match original event")
            if output is not None:
                raw_outputs.append(output)
            raw_inputs.append(input_count or 0)
        raw_turn = frame.get("rawTurn")
        settlement_events = [event for event in events if event.get("type") == "turn/end"
                             and event["data"].get("turn") == number]
        if raw_turn is None:
            if settlement_events:
                _fail("raw turn settlement omitted despite captured turn/end event")
        else:
            event = raw_turn.get("event")
            if (not isinstance(event, dict) or event.get("type") != "turn/end"
                    or event.get("data", {}).get("turn") != number
                    or raw_turn.get("sequence") != event.get("seq")
                    or raw_turn.get("kind") != event.get("type")
                    or raw_turn.get("turn") != event.get("data", {}).get("turn")
                    or raw_turn.get("usage") != event.get("data", {}).get("usage")
                    or settlement_events != [event]):
                _fail("raw settlement identity/kind/usage differs from original turn event")
            settlement_output = _count(event["data"].get("usage"), "outputTokens", "output")
            settlement_input = _count(event["data"].get("usage"), "inputTokens", "input")
        if settlement_output is not None and settlement_input is not None:
            expected_output, expected_input = settlement_output, settlement_input
        else:
            expected_output, expected_input = sum(raw_outputs), sum(raw_inputs)
    else:
        if (frame.get("absenceContract") != "docs/session-endpoints.md:37-38"
                or frame.get("canonicalAbsence", {}).get("reportedUsage") is not True
                or frame.get("canonicalAbsence", {}).get("liveUsage") is not True
                or reported is not None or canonical.get("liveUsage") is not None
                or frame.get("liveSources")):
            _fail("Pi absence no longer matches the frozen presentation contract")
        native = session.get("native_conversation")
        if not isinstance(native, dict) or not isinstance(native.get("entries"), list):
            _fail("captured native conversation missing")
        item_records = {item.get("recordId") for item in frame.get("canonicalItems", [])
                        if item.get("turnId") == frame["turnId"] and isinstance(item.get("recordId"), str)}
        snapshot_records = {item.get("recordId") for item in transcript.get("items", [])
                            if item.get("turnId") == frame["turnId"] and isinstance(item.get("recordId"), str)}
        if item_records != snapshot_records:
            _fail("canonical turn record identities differ from captured snapshot")
        entries = {entry.get("id"): entry for entry in native["entries"]
                   if entry.get("type") == "message" and entry.get("message", {}).get("role") == "assistant"
                   and isinstance(entry.get("message", {}).get("usage"), dict)}
        expected_ids = [entry.get("id") for entry in native["entries"]
                        if entry.get("id") in item_records and entry.get("id") in entries]
        source_ids = [source.get("recordId") for source in sources]
        if source_ids != expected_ids:
            _fail("Pi sources are not the exact canonical item to native entry pairing")
        for source in sources:
            record_id = source.get("recordId")
            entry = entries[record_id]
            value = entry["message"]["usage"]
            if (source.get("kind") != "message" or source.get("entry") != entry
                    or source.get("usage") != value):
                _fail("Pi usage summary differs from exact original message entry")
            raw_outputs.append(_count(value, "outputTokens", "output") or 0)
            raw_inputs.append(_count(value, "inputTokens", "input") or 0)
        if not sources:
            _fail("Pi turn usage has no record-paired source")
        expected_output, expected_input = sum(raw_outputs), sum(raw_inputs)
        if (_count(canonical_usage, "outputTokens") != expected_output
                or _count(canonical_usage, "inputTokens") != expected_input):
            _fail("canonical usage differs from raw native conversation entries")
        return {"reportedObserved": False, "durableUsageObserved": True,
                "sourceKind": source_kind, "outputTokens": expected_output}

    if (_count(canonical_usage, "outputTokens") != expected_output
            or _count(canonical_usage, "inputTokens") != expected_input):
        _fail("canonical usage differs from raw endpoint settlement/source")
    if reported is not None:
        value = _count(reported, "outputTokens")
        if value != (settlement_output if source_kind == "journal" and settlement_output is not None
                     else expected_output) or not isinstance(reported.get("isComplete"), bool):
            _fail("reported usage differs from raw endpoint settlement/source")
        if source_kind == "journal":
            known = bool(sources) and all(source.get("outputKnown") is True for source in sources)
            complete = settlement_output is not None or (frame.get("turnStarted") is True and known)
            if reported["isComplete"] != complete:
                _fail("reported usage completeness differs from source coverage/settlement")
        return {"reportedObserved": True, "complete": reported["isComplete"],
                "sourceKind": source_kind, "outputTokens": value}
    if source_kind == "journal" and settlement_output is None and raw_outputs:
        _fail("durable raw usage exists but canonical reported usage is absent")
    return {"reportedObserved": False, "sourceKind": source_kind}


def validate(receipt):
    if not isinstance(receipt, dict) or "providerRequests" in receipt:
        _fail("receipt must be projection-source evidence, not a provider request echo")
    final = receipt.get("final")
    result = _validate_frame(final)
    live = receipt.get("liveObserved")
    live_result = {"liveObserved": False, "status": "not_observed"}
    if live is not None:
        if (_identity(live) != _identity(final)
                or live.get("connectionEpoch") != final.get("connectionEpoch")):
            _fail("live and final source witnesses crossed endpoint/session/turn identity or epoch")
        live_result = _validate_frame(live, live=True)
    return {"identity": dict(zip(("endpointId", "threadId", "sessionId", "turnId"), _identity(final))),
            "final": result, "live": live_result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    args = parser.parse_args()
    result = validate(json.loads(args.receipt.read_text()))
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
