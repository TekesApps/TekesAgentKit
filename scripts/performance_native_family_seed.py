"""Finite Native task-family performance candidate producer and public validators.

This module is deliberately not a runner. The stage orchestrator owns the
Kernel process and invokes the public command, provider, and validation helpers.
All receipts contain digests and identities only; request and response bodies
are never persisted.
"""
from __future__ import annotations

import hashlib
import io
import json
from pathlib import Path
from typing import Any

from native_child_pane_source_fixture import MARKER
from native_paged_child_fixture import (
    _append_origin, _entry_sequence, _read_arguments, _strings, normalized_entry,
)
from owned_endpoint_provider import provider_handler
from task_family_fixture import native_task_seed_commands

PREFIX = "AGENTKIT_NATIVE_FAMILY_SEED "
COMMENTARY_COUNT = 61
COMMENTARY_BYTES = 1024
PAGE_SIZE = 20
EVIDENCE_SCOPE = "diagnostic-native-family-seed-candidate"
_PROOF_CONTENT = "owned Native paged child proof\n"


def _require(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def _digest(value: bytes | str) -> str:
    if isinstance(value, str):
        value = value.encode("utf-8")
    return hashlib.sha256(value).hexdigest()


def commentary_body(sequence: int) -> str:
    """Return a stable, uniquely marked commentary body of exactly 1024 UTF-8 bytes."""
    _require(type(sequence) is int and 1 <= sequence <= COMMENTARY_COUNT,
             "family commentary sequence is outside the frozen candidate")
    marker = f"AKFAMILY|native|{sequence:02d}|commentary|"
    body = marker + ("x" * (COMMENTARY_BYTES - len(marker.encode("utf-8"))))
    _require(len(body.encode("utf-8")) == COMMENTARY_BYTES,
             "family commentary body does not have its frozen byte length")
    return body


def native_family_seed_commands(parent_session_id: str, cwd: str | Path,
                                model_provider: str, model_id: str) -> list[dict[str, Any]]:
    """Return the public Native parent creation/open/compose commands.

    The original task marker and pointer-producing Native Task flow are reused.
    Only the task/read tools and the caller-owned workspace read root are
    granted; no additional writable/toolchain roots or network are introduced.
    The orchestrator should execute these commands and then use public
    ``childCatalog``/``subagents.history`` commands for the child.
    """
    commands = native_task_seed_commands(parent_session_id, cwd, model_provider, model_id)[:5]
    execution = commands[0]["args"]["execution"]
    workspace = str(Path(cwd).resolve())
    execution["allowed_tools"] = ["task", "read"]
    execution["read_roots"] = [workspace]
    execution["writable_roots"] = []
    execution["toolchain_roots"] = []
    execution["network"] = False
    return commands


def family_history_commands(parent_session_id: str, child_session_id: str) -> list[dict[str, Any]]:
    """Build public full-groundtruth and initial-page requests for the frozen two-page plan."""
    scope = {"parentSessionId": parent_session_id, "childSessionId": child_session_id}
    return [
        {"op": "call", "method": "subagents.history", "args": dict(scope, maxMessages=200)},
        {"op": "call", "method": "subagents.history", "args": dict(scope, maxMessages=PAGE_SIZE)},
    ]


def earlier_history_request(parent_session_id: str, child_session_id: str,
                            through_sequence: int, history_revision: str,
                            before_sequence: int) -> dict[str, Any]:
    """Build one actual public Earlier request from the previous page frontier."""
    _require(type(through_sequence) is int and through_sequence >= 0
             and isinstance(history_revision, str) and history_revision
             and type(before_sequence) is int and before_sequence > 0,
             "invalid frozen public history frontier")
    return {"parentSessionId": parent_session_id, "childSessionId": child_session_id,
            "maxMessages": PAGE_SIZE, "throughSequence": through_sequence,
            "historyRevision": history_revision, "beforeSequence": before_sequence}


def _message_text(message: object) -> str:
    """Read only documented text blocks from a stored public message."""
    _require(isinstance(message, dict) and isinstance(message.get("content"), list),
             "public message content is malformed")
    blocks = message["content"]
    _require(all(isinstance(block, dict) and block.get("type") == "text"
                 and isinstance(block.get("text"), str) for block in blocks),
             "public message contains an unsupported content block")
    return "".join(block["text"] for block in blocks)


def _event_text(entry: dict[str, Any]) -> str:
    event = entry.get("event", {})
    data = event.get("data", {}) if isinstance(event, dict) else {}
    message = data.get("message") if isinstance(data, dict) else None
    return _message_text(message)


def _original_call_id(data: object) -> object:
    if not isinstance(data, dict):
        return None
    message = data.get("message")
    source = message.get("source") if isinstance(message, dict) else None
    if isinstance(source, dict) and source.get("kind") == "tool":
        return source.get("callId")
    return data.get("callId", data.get("call_id", data.get("id")))


def _validate_read_result(result: dict[str, Any], *, call_id: str,
                          expected_proof_path: str) -> None:
    event = result.get("event", {})
    data = event.get("data", {}) if isinstance(event, dict) else {}
    message = data.get("message") if isinstance(data, dict) else None
    _require(isinstance(message, dict) and message.get("role") == "user"
             and isinstance(message.get("source"), dict)
             and message["source"].get("kind") == "tool"
             and message["source"].get("callId") == call_id,
             "public read result is not paired through its tool source call ID")
    text = _message_text(message)
    try:
        payload = json.loads(text)
    except (ValueError, TypeError):
        payload = None
    expected_bytes = len(_PROOF_CONTENT.encode("utf-8"))
    expected_hash = "sha256-" + _digest(_PROOF_CONTENT)
    _require(isinstance(payload, dict)
             and payload.get("path") == expected_proof_path
             and payload.get("bytes") == expected_bytes
             and payload.get("sha256") == expected_hash
             and payload.get("truncated") is False
             and payload.get("total_lines") == 1
             and payload.get("lines") == [{"line": 1, "text": _PROOF_CONTENT.rstrip("\n"),
                                            "truncated": False}],
             "public read result does not match the owned proof bytes, hash, and lines")


def validate_public_child_catalog(catalog: object, parent_session_id: str) -> str:
    """Resolve exactly one bounded child through the public parent catalog."""
    _require(isinstance(catalog, dict) and isinstance(catalog.get("entries"), list),
             "public child catalog is malformed")
    children = [item for item in catalog["entries"] if isinstance(item, dict)
                and item.get("kind") == "child"]
    _require(len(children) == 1 and children[0].get("mode") == "bounded",
             "public Native catalog must expose exactly one bounded child")
    child_id = children[0].get("id")
    _require(isinstance(child_id, str) and child_id and child_id != parent_session_id,
             "bounded Native child must be a real child pointer, not a session slot")
    return child_id


def validate_public_family_groundtruth(history: object, *, parent: dict[str, Any],
                                       child_id: str,
                                       expected_proof_path: str = "proof.txt") -> dict[str, Any]:
    """Validate one public maxMessages=200 child history as diagnostic groundtruth."""
    _require(isinstance(history, dict) and history.get("parentSessionId") == parent.get("sessionId")
             and history.get("childSessionId") == child_id
             and type(history.get("throughSequence")) is int
             and isinstance(history.get("historyRevision"), str)
             and isinstance(history.get("entries"), list),
             "public Native family history identity/metadata mismatch")
    _require(history.get("hasMoreBefore") is False and history.get("runOpen") is False,
             "public maxMessages=200 history is incomplete or the child is still running")
    entries = history["entries"]
    seqs = [_entry_sequence(item) for item in entries]
    _require(bool(entries) and seqs == sorted(set(seqs)),
             "public Native family groundtruth is empty, duplicated, or unordered")
    append_assistant = [item for item in entries if _append_origin(item)
                        and item.get("event", {}).get("type") == "assistant/message"
                        and item.get("event", {}).get("data", {}).get("message", {}).get("role") == "assistant"]
    commentary = [_event_text(item) for item in append_assistant
                  if _event_text(item).startswith("AKFAMILY|native|")]
    expected = [commentary_body(index) for index in range(1, COMMENTARY_COUNT + 1)]
    _require(commentary == expected,
             "public groundtruth does not contain the exact ordered 61 commentary bodies")
    calls = [item for item in entries if item.get("event", {}).get("type") == "tool/call"
             and item.get("event", {}).get("data", {}).get("name") == "read"]
    results = [item for item in entries if item.get("event", {}).get("type") == "tool/result"]
    _require(len(calls) == COMMENTARY_COUNT and len(results) == COMMENTARY_COUNT,
             "public groundtruth lacks one real read call/result group per commentary")
    for call, result in zip(calls, results):
        data = call["event"].get("data", {})
        args = data.get("arguments")
        if isinstance(args, str):
            try:
                args = json.loads(args)
            except ValueError:
                args = None
        _require(isinstance(args, dict)
                 and args.get("path", args.get("file_path")) == expected_proof_path,
                 "public read call did not target the owned proof file")
        call_id = _original_call_id(data)
        result_data = result["event"].get("data", {})
        result_id = _original_call_id(result_data)
        _require(call_id is not None and result_id == call_id,
                 "public read result is not paired with its original read call")
        _validate_read_result(result, call_id=call_id,
                              expected_proof_path=expected_proof_path)
    final_messages = [item for item in append_assistant
                      if item.get("event", {}).get("data", {}).get("sessionFinal") is True]
    _require(bool(final_messages), "public groundtruth has no completed assistant final")
    _require(_event_text(final_messages[-1]) == "NATIVE_FAMILY_SEED_COMPLETE",
             "public groundtruth final assistant message does not match the completed candidate")
    _require(isinstance(history.get("sourceCall"), dict),
             "public child history omitted its original Native task source")
    return {"parentSessionId": parent["sessionId"], "childSessionId": child_id,
            "throughSequence": history["throughSequence"],
            "historyRevision": history["historyRevision"], "entries": entries,
            "entryCount": len(entries), "commentaryCount": COMMENTARY_COUNT,
            "commentaryBytes": COMMENTARY_BYTES, "readGroupCount": COMMENTARY_COUNT,
            "diagnosticOnly": True, "officialSamples": 0,
            "performanceAccepted": False, "history": history}


def validate_public_family_pages(groundtruth: dict[str, Any], pages: list[dict[str, Any]],
                                 requests: list[dict[str, Any]]) -> dict[str, Any]:
    """Check the frozen initial maxMessages=20 page and one actual Earlier page."""
    _require(len(pages) == 2 and len(requests) == 2,
             "family candidate requires one initial page and exactly one actual Earlier page")
    expected_scope = (groundtruth["parentSessionId"], groundtruth["childSessionId"])
    previous_floor: int | None = None
    union: list[int] = []
    for index, (page, request) in enumerate(zip(pages, requests)):
        _require(isinstance(page, dict) and isinstance(request, dict)
                 and (request.get("parentSessionId"), request.get("childSessionId")) == expected_scope
                 and (page.get("parentSessionId"), page.get("childSessionId")) == expected_scope
                 and page.get("throughSequence") == groundtruth["throughSequence"]
                 and page.get("historyRevision") == groundtruth["historyRevision"]
                 and page.get("runOpen") is False
                 and page.get("sourceCall") == groundtruth.get("history", {}).get("sourceCall",
                                                                                groundtruth.get("sourceCall"))
                 and request.get("maxMessages") == PAGE_SIZE,
                 "public family page changed child scope, history cut, revision, or page size")
        entries = page.get("entries")
        _require(isinstance(entries, list) and bool(entries),
                 "public family page is empty")
        seqs = [_entry_sequence(item) for item in entries]
        _require(seqs == sorted(set(seqs)) and (previous_floor is None or max(seqs) < previous_floor),
                 "public family Earlier pages overlap or are not source ordered")
        if index == 0:
            _require(all(request.get(key) is None for key in
                         ("beforeSequence", "throughSequence", "historyRevision")),
                     "initial public family head unexpectedly carries a cursor")
            _require(page.get("hasMoreBefore") is True,
                     "initial maxMessages=20 page does not expose older history")
        else:
            _require(request.get("throughSequence") == groundtruth["throughSequence"]
                     and request.get("historyRevision") == groundtruth["historyRevision"]
                     and request.get("beforeSequence") == previous_floor,
                     "public Earlier request did not use the exact preceding exclusive cursor")
            _require(page.get("hasMoreBefore") in (True, False),
                     "public Earlier page omitted its pagination state")
        previous_floor = min(seqs)
        union.extend(seqs)
    _require(len(union) == len(set(union)), "public family page union contains duplicate source entries")
    gt_by_seq = {_entry_sequence(item): normalized_entry(item) for item in groundtruth["entries"]}
    page_entries = [item for page in pages for item in page["entries"]]
    _require(all(_entry_sequence(item) in gt_by_seq
                 and normalized_entry(item) == gt_by_seq[_entry_sequence(item)]
                 for item in page_entries),
             "public family page entry content differs from independent groundtruth")
    _require(any(_entry_sequence(item) not in {_entry_sequence(value) for value in pages[0]["entries"]}
                 for item in pages[1]["entries"]),
             "public Earlier page did not add a genuinely new source record")
    return dict(groundtruth, pagePlan=pages, pagePlanRequests=requests,
                pagePlanEntryCount=len(union), publicPagePlanMatched=True)


def family_native_provider(*, workspace: Path, requests: list[str]):
    """Return a finite owned provider for 61 read continuations and a valid final.

    The existing provider handles the original parent Task request. This
    handler intercepts only the public Native child Responses path. Each next
    commentary/read pair requires the previous original read output and exact
    server-side previous_response_id. There is no sleep or held-failure gate.
    """
    workspace = Path(workspace).resolve(strict=True)
    proof_path = workspace / "proof.txt"
    _require(proof_path.is_file() and proof_path.read_text(encoding="utf-8") == _PROOF_CONTENT,
             "family provider requires the fixed owned proof file")
    original = provider_handler(workspace=lambda: workspace, requests=requests)
    state: dict[str, Any] = {"childRequests": 0, "stepReceipts": [],
                             "finalResponseId": None, "diagnosticOnly": True,
                             "officialSamples": 0}

    class Provider(original):
        def do_POST(self):
            stream = self.rfile
            raw = stream.read(int(self.headers.get("Content-Length", "0")))
            try:
                body = json.loads(raw)
            except (ValueError, TypeError):
                body = None
            self.rfile = io.BytesIO(raw)
            try:
                if not _is_family_child_request(body):
                    return original.do_POST(self)
                _require(isinstance(body, dict) and self.path.endswith("/responses"),
                         "Native family child must use the public Responses endpoint")
                state["childRequests"] += 1
                request_index = state["childRequests"]
                prior_response = state["stepReceipts"][-1]["responseId"] if state["stepReceipts"] else None
                if request_index > 1:
                    _require(body.get("previous_response_id") == prior_response,
                             "Native child continuation broke the previous_response_id chain")
                    _validate_prior_read_result(body, state["stepReceipts"][-1]["callId"],
                                                _PROOF_CONTENT.strip())
                if request_index <= COMMENTARY_COUNT:
                    name, args = _read_arguments(body, proof_path)
                    commentary = commentary_body(request_index)
                    call_id = f"native-family-read-{request_index}"
                    response_id = f"native-family-step-{request_index}"
                    message = {"id": f"native-family-message-{request_index}", "type": "message",
                        "role": "assistant", "phase": "commentary", "status": "completed",
                        "content": [{"type": "output_text", "text": commentary}]}
                    function = {"type": "function_call", "call_id": call_id,
                                "name": "read", "arguments": json.dumps(args)}
                    events = [
                        {"type": "response.output_item.added", "output_index": 0,
                         "item": dict(message, status="in_progress", content=[])},
                        {"type": "response.output_text.delta", "output_index": 0,
                         "content_index": 0, "delta": commentary},
                        {"type": "response.output_text.done", "output_index": 0,
                         "content_index": 0, "text": commentary},
                        {"type": "response.output_item.done", "output_index": 0, "item": message},
                        {"type": "response.output_item.added", "output_index": 1,
                         "item": dict(function, status="in_progress", arguments="")},
                        {"type": "response.function_call_arguments.delta", "output_index": 1,
                         "delta": json.dumps(args)},
                        {"type": "response.function_call_arguments.done", "output_index": 1,
                         "arguments": json.dumps(args)},
                        {"type": "response.output_item.done", "output_index": 1, "item": function},
                        {"type": "response.completed", "response": {"id": response_id,
                         "status": "completed", "output": [message, function],
                         "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}},
                    ]
                    state["stepReceipts"].append({"step": request_index, "responseId": response_id,
                        "callId": call_id, "tool": "read", "pathDigest": _digest(args[name]),
                        "commentaryDigest": _digest(commentary), "commentaryBytes": COMMENTARY_BYTES,
                        "priorReadResultObserved": request_index > 1})
                    return self._send_sse(events)
                _require(request_index == COMMENTARY_COUNT + 1
                         and len(state["stepReceipts"]) == COMMENTARY_COUNT,
                         "Native family child made an unexpected extra request")
                final_text = "NATIVE_FAMILY_SEED_COMPLETE"
                response_id = "native-family-final"
                message = {"id": "native-family-final-message", "type": "message",
                    "role": "assistant", "phase": "final_answer", "status": "completed",
                    "content": [{"type": "output_text", "text": final_text}]}
                events = [
                    {"type": "response.output_item.added", "output_index": 0,
                     "item": dict(message, status="in_progress", content=[])},
                    {"type": "response.output_text.delta", "output_index": 0,
                     "content_index": 0, "delta": final_text},
                    {"type": "response.output_text.done", "output_index": 0,
                     "content_index": 0, "text": final_text},
                    {"type": "response.output_item.done", "output_index": 0, "item": message},
                    {"type": "response.completed", "response": {"id": response_id,
                     "status": "completed", "output": [message],
                     "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}},
                ]
                state["finalResponseId"] = response_id
                self._send_sse(events)
            finally:
                self.rfile.close()
                self.rfile = stream

        def _send_sse(self, events: list[dict[str, Any]]) -> None:
            encoded = "".join("data: " + json.dumps(event) + "\n\n"
                              for event in events).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    return Provider, state


def _is_family_child_request(body: object) -> bool:
    if not isinstance(body, dict) or not isinstance(body.get("input"), list):
        return False
    if any(isinstance(item, dict) and item.get("role") == "user"
           and MARKER in json.dumps(item, ensure_ascii=False)
           and "ENDPOINT_NATIVE_CHILD" not in json.dumps(item, ensure_ascii=False)
           for item in body["input"]):
        return True
    previous = body.get("previous_response_id")
    if isinstance(previous, str) and previous.startswith("native-family-step-"):
        return True
    return any(isinstance(item, dict) and item.get("type") == "function_call_output"
               and isinstance(item.get("call_id"), str)
               and item["call_id"].startswith("native-family-read-")
               for item in body["input"])


def _validate_prior_read_result(body: object, expected_call_id: str, proof_text: str) -> bool:
    _require(isinstance(body, dict) and isinstance(body.get("input"), list),
             "Native read continuation omitted public Responses input history")
    outputs = [item for item in body["input"] if isinstance(item, dict)
               and item.get("type") == "function_call_output"
               and item.get("call_id") == expected_call_id]
    _require(len(outputs) == 1 and any(proof_text in text for text in _strings(outputs[0].get("output"))),
             "Native family continuation omitted the previous original read result")
    return True


def diagnostic_candidate_receipt(*, parent_session_id: str, child_session_id: str,
                                 provider_state: dict[str, Any], history: dict[str, Any],
                                 pages: dict[str, Any]) -> dict[str, Any]:
    """Create a secret/body-free receipt that cannot be mistaken for acceptance."""
    _require(len(provider_state.get("stepReceipts", [])) == COMMENTARY_COUNT
             and provider_state.get("finalResponseId") == "native-family-final",
             "finite Native family provider did not complete its 61-step candidate")
    return {"evidenceScope": EVIDENCE_SCOPE, "parentSessionId": parent_session_id,
            "childSessionId": child_session_id,
            "sourceHistoryDigest": _digest(json.dumps(history["entries"], sort_keys=True,
                                                        ensure_ascii=False)),
            "pagePlanDigest": _digest(json.dumps(pages["pagePlan"], sort_keys=True,
                                                  ensure_ascii=False)),
            "providerSteps": COMMENTARY_COUNT, "commentaryBytes": COMMENTARY_BYTES,
            "readGroupCount": COMMENTARY_COUNT,
            "providerReceiptDigest": _digest(json.dumps(provider_state["stepReceipts"], sort_keys=True)),
            "diagnosticOnly": True, "officialSamples": 0,
            "performanceAccepted": False, "desktopInteraction": False,
            "physicalIme": False, "nativePaneAccepted": False}

