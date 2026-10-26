"""Strict materializer and aggregator for the frozen Native Family performance matrix."""
from __future__ import annotations

import hashlib
import json
import copy
from pathlib import Path
from typing import Any

from performance_sample_report import _stable_machine_identity, nearest_rank

MODES = ("cold", "warm", "reconnectLifecycle")
ROLES = ("baseline", "candidate")
PATHS = ("common50", "product20")
CASE_COUNTS = {"cold": 5, "warm": 1, "reconnectLifecycle": 1}
ROUND_COUNTS = {"cold": 1, "warm": 11, "reconnectLifecycle": 11}
MEASURED_ROUNDS = {"cold": (0,), "warm": tuple(range(1, 11)),
                   "reconnectLifecycle": tuple(range(1, 11))}
SIDECARS = (
    "inputs.json", "seed-source-history.json", "seed-proof.json",
    "seed-exit.json", "seed-history-exit.json", "seed-earlier-exit.json",
    "consumer.stdout.jsonl", "consumer-exit.json",
    "consumer-process-boundaries.jsonl", "measurement-rss.jsonl",
    "last-rss-summary.json", "machine-after.json", "fixture-lifecycle.json",
    "owned-process-retirement.json", "seed-provider-before-measurement.json",
    "provider-observation.json",
    "provider-end-receipt.json", "cold-kernel-restart.json",
    "official-execution-receipt.json",
)


def _need(ok: bool, message: str) -> None:
    if not ok:
        raise ValueError(message)


def _canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode("utf-8")


def _sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _read_json(path: Path, name: str) -> Any:
    _need(not path.is_symlink() and path.is_file(), f"missing regular sidecar {name}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"invalid JSON sidecar {name}: {error}") from error


def _read_jsonl(path: Path, name: str) -> list[dict[str, Any]]:
    _need(not path.is_symlink() and path.is_file(), f"missing regular sidecar {name}")
    data = path.read_bytes()
    _need(not data or data.endswith(b"\n"), f"{name} has a truncated final line")
    rows = []
    for index, line in enumerate(data.splitlines(), 1):
        try:
            row = json.loads(line)
        except ValueError as error:
            raise ValueError(f"invalid JSONL {name}:{index}") from error
        _need(isinstance(row, dict), f"{name}:{index} is not an object")
        rows.append(row)
    return rows


def _file_sha(path: Path) -> str:
    return _sha(path.read_bytes())


def _check_path_digest(path_value: Any, expected: Any, label: str,
                       errors: list[str]) -> None:
    if not isinstance(path_value, str) or not path_value or not isinstance(expected, str):
        errors.append(f"{label} path/hash identity is missing")
        return
    path = Path(path_value)
    try:
        if path.is_symlink() or not path.is_file() or _file_sha(path) != expected:
            errors.append(f"{label} on-disk bytes differ from the registered exact SHA")
    except OSError as error:
        errors.append(f"{label} exact input is unavailable: {error}")


def _check_digest_bindings(registration: dict[str, Any], family_profile: dict[str, Any],
                           inputs: dict[str, Any], receipt: dict[str, Any],
                           errors: list[str], *, pilot: bool = False) -> None:
    def same(label: str, expected: Any, *values: Any) -> None:
        if not isinstance(expected, str) or not expected:
            errors.append(f"{label} is absent from the frozen registration/profile")
        elif any(value != expected for value in values):
            errors.append(f"{label} differs across registration, input, and execution receipt")

    reg_sha = (registration.get("registrationSha256") or registration.get("registrationFileSha256")
               or registration.get("officialRegistrationSha256"))
    same("registrationSha256", reg_sha,
         inputs.get("registrationSha256") or inputs.get("officialRegistrationSha256"),
         receipt.get("registrationSha256"))
    base_sha = registration.get("baseProfileSha256") or family_profile.get("baseProfileSha256")
    same("baseProfileSha256", base_sha, inputs.get("baseProfileSha256"),
         receipt.get("baseProfileSha256"), family_profile.get("baseProfileSha256"))
    family_sha = registration.get("familyProfileSha256") or registration.get("profileSha256")
    same("familyProfileSha256", family_sha,
         inputs.get("familyProfileSha256") or inputs.get("profileSha256"),
         receipt.get("familyProfileSha256") or receipt.get("profileSha256"))
    role = registration.get("role")
    expected_source = family_profile.get("sourceProducts", {}).get(role)
    for name, expected in (
        ("sourceCommit", expected_source),
        ("programSha256", registration.get("programSha256")),
        ("consumerSha256", registration.get("consumerSha256")),
        ("runnerSha256", registration.get("runnerSha256")),
        ("runtimeSha256", registration.get("runtimeSha256")),
    ):
        same(name, expected, receipt.get(name), inputs.get(name))

    runtime_before = receipt.get("runtimeBefore") or receipt.get("runtimeBeforeMeasurement")
    runtime_after = receipt.get("runtimeAfter") or receipt.get("runtimeAfterMeasurement")
    expected_runtime = family_profile.get("runtime", {})
    if not isinstance(runtime_before, dict) or not isinstance(runtime_after, dict):
        errors.append("raw runtime-before/after identities are missing")
    else:
        for runtime in (runtime_before, runtime_after):
            expected_source = expected_runtime.get("source")
            if (runtime.get("kind") != "native"
                    or runtime.get("version") != expected_runtime.get("kernelVersion")
                    or runtime.get("source") != expected_source):
                errors.append("runtime version/source differs from the frozen Native runtime")
        critical_before = runtime_before.get("criticalFiles")
        critical_after = runtime_after.get("criticalFiles")
        if not isinstance(critical_before, dict) or not critical_before or critical_before != critical_after:
            errors.append("fixed Native runtime critical-file identities changed or are missing")
        runtime_files = registration.get("runtimeFiles")
        if isinstance(runtime_files, list):
            expected_files = {row.get("path"): row.get("sha256") for row in runtime_files
                              if isinstance(row, dict)}
            if expected_files != critical_before:
                errors.append("runtime critical files differ from the original registered runtime closure")
            if _sha(_canonical(runtime_files)) != registration.get("runtimeSha256"):
                errors.append("runtimeSha256 does not match the exact registered critical-file closure")
    if receipt.get("identityRechecked") is not True:
        errors.append("runner did not recheck registered identities after execution")
    if pilot:
        if not (registration.get("purpose") == "toolchain-pilot"
                and registration.get("diagnosticOnly") is True
                and receipt.get("purpose") == "toolchain-pilot"
                and receipt.get("official") is False and receipt.get("diagnosticOnly") is True):
            errors.append("pilot requires explicit diagnostic registration and execution receipt")
    else:
        if receipt.get("official") is not True or registration.get("official") is not True:
            errors.append("original registration/receipt is not explicitly official")
        if (receipt.get("diagnosticOnly") is not False or registration.get("diagnosticOnly") is True
                or registration.get("purpose") == "toolchain-pilot"):
            errors.append("diagnostic evidence cannot be promoted to an official case")
    original = registration.get("registrationBytes") or inputs.get("registrationBytes")
    if isinstance(original, str):
        original_sha = _sha(original.encode("utf-8"))
        if reg_sha != original_sha:
            errors.append("registration SHA does not bind the exact original registration bytes")
    inputs_reg = inputs.get("officialRegistration")
    if isinstance(inputs_reg, dict):
        exact_bytes = inputs_reg.get("registrationBytes")
        if isinstance(exact_bytes, str) and _sha(exact_bytes.encode("utf-8")) != reg_sha:
            errors.append("inputs sidecar registration bytes differ from the registered SHA")


def _raw_entry_sequence(row: Any) -> int:
    event = row.get("event") if isinstance(row, dict) else None
    seq = event.get("seq") if isinstance(event, dict) else None
    _need(type(seq) is int, "raw source entry is missing its original integer sequence")
    return seq


def _message_text(message: Any) -> str:
    _need(isinstance(message, dict), "Native message entry has no message object")
    content = message.get("content")
    if isinstance(content, str):
        return content
    _need(isinstance(content, list), "Native message content is not a string or block array")
    _need(all(isinstance(block, dict) and block.get("type") == "text"
              and isinstance(block.get("text"), str) for block in content),
          "Native seed includes a non-text content block")
    return "".join(block["text"] for block in content)


def _event_data(row: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    event = row.get("event")
    _need(isinstance(event, dict), "raw history entry lacks event object")
    kind, data = event.get("type"), event.get("data")
    _need(isinstance(kind, str) and isinstance(data, dict), "raw history event type/data malformed")
    return kind, data


def _call_id(data: dict[str, Any]) -> Any:
    value = data.get("callId")
    if value is not None:
        return value
    message = data.get("message")
    source = message.get("source") if isinstance(message, dict) else None
    return source.get("callId") if isinstance(source, dict) else None


def _read_call_arguments(data: dict[str, Any]) -> dict[str, Any]:
    args = data.get("arguments")
    if isinstance(args, str):
        try:
            args = json.loads(args)
        except ValueError:
            args = None
    _need(isinstance(args, dict), "Native read call arguments are not an object")
    return args


def _validate_source_history(source: Any, registration: dict[str, Any],
                             family_profile: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    _need(isinstance(source, dict), "seed source history must be a public full200 response")
    parent, child = registration.get("parentSessionId"), registration.get("childSessionId")
    _need(isinstance(parent, str) and parent and isinstance(child, str) and child and parent != child,
          "registration must bind distinct original parent/child session pointers")
    _need(source.get("parentSessionId") == parent and source.get("childSessionId") == child,
          "full200 source owner differs from the registered parent/child pointers")
    _need(source.get("hasMoreBefore") is False and source.get("runOpen") is False,
          "full200 groundtruth is not exhausted and closed")
    entries = source.get("entries")
    _need(isinstance(entries, list) and entries, "full200 source has no entries")
    seqs = [_raw_entry_sequence(row) for row in entries]
    _need(seqs == sorted(set(seqs)), "full200 source entries are duplicated or out of source order")
    source_call = source.get("sourceCall")
    _need(isinstance(source_call, dict) and source_call,
          "full200 source lacks raw Native task sourceCall metadata")

    expected = family_profile["seed"]
    commentaries, calls, results, finals = [], [], [], []
    for row in entries:
        kind, data = _event_data(row)
        message = data.get("message")
        appended = data.get("surfaceOp", "append") == "append"
        if kind == "assistant/message" and appended and isinstance(message, dict) and message.get("role") == "assistant":
            body = _message_text(message)
            if body.startswith("AKFAMILY|native|"):
                commentaries.append(body)
            if data.get("sessionFinal") is True:
                finals.append(body)
        elif kind == "tool/call" and data.get("name") == "read":
            calls.append(data)
        elif kind == "tool/result":
            results.append(data)
    _need(len(commentaries) == expected.get("commentaryBodies", 61),
          "full200 source does not have the exact expected commentary count")
    for ordinal, body in enumerate(commentaries, 1):
        marker = f"AKFAMILY|native|{ordinal:02}|commentary|"
        expected_body = marker + "x" * (1024 - len(marker.encode("utf-8")))
        _need(body == expected_body and len(body.encode("utf-8")) == 1024,
              f"commentary {ordinal} content/order/UTF-8 byte count differs")
    group_count = expected.get("actualReadGroups", 61)
    _need(len(calls) == group_count and len(results) == group_count,
          "full200 source does not have 61 original read call/result records")
    normalized_reads = []
    proof_text = expected["proofText"]
    for ordinal, (call, result) in enumerate(zip(calls, results), 1):
        call_id, result_id = _call_id(call), _call_id(result)
        _need(isinstance(call_id, str) and call_id and result_id == call_id,
              f"read result {ordinal} does not pair through its original call ID")
        args = _read_call_arguments(call)
        _need(args.get("path", args.get("file_path")) == "proof.txt",
              f"read call {ordinal} did not request the owned proof file")
        message = result.get("message")
        _need(isinstance(message, dict) and message.get("role") == "user",
              f"read result {ordinal} is not an original tool result message")
        try:
            payload = json.loads(_message_text(message))
        except ValueError as error:
            raise ValueError(f"read result {ordinal} is not JSON") from error
        proof_sha = _sha(proof_text.encode("utf-8"))
        payload_sha = payload.get("sha256") if isinstance(payload, dict) else None
        _need(isinstance(payload, dict) and payload.get("path") == "proof.txt"
              and payload.get("bytes") == len(proof_text.encode("utf-8"))
              and isinstance(payload_sha, str)
              and payload_sha.removeprefix("sha256-") == proof_sha
              and payload.get("truncated") is False
              and payload.get("lines") == [{"line": 1, "text": proof_text.rstrip("\n"),
                                             "truncated": False}],
              f"read result {ordinal} does not match the source proof bytes/hash")
        normalized_reads.append({"ordinal": ordinal, "name": "read", "path": "proof.txt",
                                 "bytes": payload["bytes"],
                                 "sha256": payload_sha.removeprefix("sha256-")})
    _need(bool(finals) and finals[-1] == expected.get("finalText"),
          "last final assistant message differs from the frozen Native family seed")
    normalized = {"commentaryBodies": commentaries, "readGroups": normalized_reads,
                  "finalText": finals[-1]}
    semantic_sha = _sha(_canonical(normalized))
    _need(semantic_sha == expected.get("semanticCorpusSha256"),
          "raw public full200 corpus differs from the frozen semantic corpus hash")
    return normalized, entries


def _page_value(response: Any) -> dict[str, Any] | None:
    if not isinstance(response, dict):
        return None
    value = response.get("value")
    return value if isinstance(value, dict) else None


def _validate_page(page: Any, *, parent: str, child: str, source: dict[str, Any],
                   expected_size: int, before: int | None = None) -> list[dict[str, Any]]:
    _need(isinstance(page, dict), "bounded_call did not retain a raw successful history page")
    _need(page.get("parentSessionId") == parent and page.get("childSessionId") == child,
          "raw bounded history page owner differs from the public child pointer")
    _need(page.get("throughSequence") == source.get("throughSequence")
          and page.get("historyRevision") == source.get("historyRevision")
          and page.get("sourceCall") == source.get("sourceCall"),
          "raw history page cut/revision/sourceCall changed from full200 source")
    _need(page.get("runOpen") is False, "Native bounded child became writable/open")
    rows = page.get("entries")
    _need(isinstance(rows, list) and rows, "actual bounded history page is empty")
    seqs = [_raw_entry_sequence(row) for row in rows]
    _need(seqs == sorted(set(seqs)), "raw page entries are duplicated or unordered")
    if before is None:
        _need(page.get("hasMoreBefore") is True, "initial raw page does not expose real Earlier history")
    else:
        _need(max(seqs) < before, "Earlier response overlaps the visible head or crosses its exclusive frontier")
    return rows


def _request_payload(call: dict[str, Any], *, parent: str, child: str, expected_size: int,
                     before: int | None = None, through: Any = None, revision: Any = None) -> None:
    request = call.get("request")
    _need(isinstance(request, dict) and request.get("method") == "subagents.history",
          "bounded_call is not the original subagents.history request")
    payload = request.get("payload")
    _need(isinstance(payload, dict) and payload.get("parentSessionId") == parent
          and payload.get("childSessionId") == child and payload.get("maxMessages") == expected_size,
          "actual history request has wrong parent/child pointer or maxMessages")
    if before is None:
        _need(payload.get("beforeSequence") is None,
              "initial page was rendered from a cursor/prefix-validation request")
    else:
        _need(payload.get("beforeSequence") == before,
              "Earlier request cursor differs from the prior page source frontier")
        _need(payload.get("throughSequence") == through and payload.get("historyRevision") == revision,
              "Earlier request changed frozen cut or history revision")
    _need(call.get("passthroughUnmodified") is True,
          "transparent endpoint wrapper did not affirm original request forwarding")


def _calls_from(rows: Any) -> list[dict[str, Any]]:
    _need(isinstance(rows, list) and rows, "actual transparent bounded_call records are missing")
    calls = [row for row in rows if isinstance(row, dict) and row.get("event") == "bounded_call"]
    _need(len(calls) == len(rows), "raw call list contains a non-bounded_call substitute")
    for call in calls:
        _need(isinstance(call.get("request"), dict)
              and isinstance(call.get("request", {}).get("payload"), dict)
              and isinstance(call.get("response"), dict),
              "bounded_call lacks its original request or response/error object")
        _need(type(call.get("connectionOrdinal")) is int and call["connectionOrdinal"] > 0,
              "bounded_call has no original endpoint connection identity")
        _need(isinstance(call.get("phase"), str) and call.get("phase"),
              "bounded_call has no product-operation phase")
        intent, returned = call.get("intentMonotonicNs"), call.get("returnMonotonicNs")
        _need(type(intent) is int and type(returned) is int and returned >= intent
              and call.get("latencyNs") == returned - intent,
              "bounded_call does not retain consistent original monotonic intent/return times")
        _need(call.get("response", {}).get("value") is not None,
              "bounded_call is an original error/missing response, not successful history evidence")
    return calls


def _dedupe_calls(calls: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Deduplicate event copies by source ordinal, rejecting any altered copy."""
    by_ordinal: dict[int, dict[str, Any]] = {}
    for call in calls:
        ordinal = call.get("sourceReadOrdinal")
        _need(type(ordinal) is int and ordinal > 0,
              "bounded_call has no unique original sourceReadOrdinal")
        old = by_ordinal.get(ordinal)
        _need(old is None or _canonical(old) == _canonical(call),
              "duplicated bounded_call sourceReadOrdinal changed its raw value")
        by_ordinal[ordinal] = call
    return [by_ordinal[key] for key in sorted(by_ordinal)]


def _validate_all_recorded_calls(calls: list[dict[str, Any]], *, parent: str, child: str,
                                source: dict[str, Any], source_entries: list[dict[str, Any]],
                                path: str, role: str, case_index: int, round_index: int,
                                measured: bool, through_sequence: int | None = None,
                                history_revision: Any = None) -> None:
    for call in calls:
        scope = call.get("parentScope")
        if (call.get("path") != path or call.get("role") != role
                or call.get("caseIndex") != case_index or call.get("round") != round_index
                or call.get("measured") is not measured
                or not isinstance(scope, dict) or scope.get("sessionId") != parent
                or call.get("childId") != child):
            raise ValueError("bounded_call raw context differs from its actual case/iteration/owner")
        request = call["request"]["payload"]
        if request.get("parentSessionId") != parent or request.get("childSessionId") != child:
            raise ValueError("bounded_call actual request has another Native parent/child pointer")
        size = request.get("maxMessages")
        if size not in (20, 50, 200):
            raise ValueError("bounded_call changed maxMessages outside the frozen native family calls")
        phase = call.get("phase")
        expected_product_size = 50 if path == "common50" else 20
        if phase in ("sep20-head", "sep20-earlier"):
            _need(path == "common50" and size == 20,
                  "SEP20 raw call escaped its common50 public probe or changed page size")
        elif phase == "childrun-open-and-first-snapshot":
            _need(size == expected_product_size and request.get("beforeSequence") is None,
                  "initial ChildRun source is not the registered page size/head")
        elif phase == "product-childrun-earlier20":
            _need(path == "product20" and size == 20 and request.get("beforeSequence") is not None,
                  "product20 render Earlier is not its real bounded page20 cursor call")
        elif phase == "product-childrun-earlier50":
            _need(path == "common50" and size == 50 and request.get("beforeSequence") is not None,
                  "common50 render Earlier is not its real bounded page50 cursor call")
        elif phase in ("product-childrun-refresh-after-earlier20",
                       "product-childrun-refresh-after-earlier50"):
            expected_phase_path = "product20" if phase.endswith("20") else "common50"
            expected_refresh_size = 20 if expected_phase_path == "product20" else 50
            _need(path == expected_phase_path and size in (expected_refresh_size, 200),
                  "post-Earlier prefix refresh escaped its product operation")
        elif phase == "reconnect-and-same-child-refresh":
            _need(size in (expected_product_size, 200),
                  "reconnect refresh changed the registered ChildRun page size")
        else:
            raise ValueError("bounded_call came from an unregistered Native family operation phase")
        page = _page_value(call.get("response"))
        if page is None:
            raise ValueError("bounded_call failed; an error cannot be hidden by successful sibling pages")
        _need(page.get("parentSessionId") == parent and page.get("childSessionId") == child
              and page.get("throughSequence") == (through_sequence if through_sequence is not None
                                                    else source.get("throughSequence"))
              and page.get("historyRevision") == (history_revision if history_revision is not None
                                                    else source.get("historyRevision"))
              and page.get("sourceCall") == source.get("sourceCall")
              and page.get("runOpen") is False,
              "bounded_call raw source response changed pointer/cut/revision/sourceCall/closed state")
        _union_pages([page], source_entries)
        if request.get("beforeSequence") is not None:
            _need(request.get("throughSequence") == source.get("throughSequence")
                  and request.get("historyRevision") == source.get("historyRevision"),
                  "bounded_call pagination request changed the original cut/revision")
        elif request.get("throughSequence") is not None or request.get("historyRevision") is not None:
            raise ValueError("bounded_call initial page unexpectedly uses a cursor")
        if size == 200 and request.get("beforeSequence") is None:
            _need(page.get("hasMoreBefore") is False
                  and page.get("entries") == source_entries,
                  "an independent full200 bounded call is not EOF/full-content groundtruth")


def _validate_page_pair(calls: list[dict[str, Any]], *, parent: str, child: str,
                        source: dict[str, Any], page_size: int,
                        head_phase: str | None = None,
                        earlier_phase: str | None = None) -> list[dict[str, Any]]:
    heads, earlier = [], []
    for call in calls:
        request = call.get("request", {}).get("payload", {})
        if request.get("maxMessages") != page_size:
            continue
        expected_phase = head_phase if request.get("beforeSequence") is None else earlier_phase
        if expected_phase is not None and call.get("phase") != expected_phase:
            continue
        page = _page_value(call.get("response"))
        _need(page is not None, "actual history request failed or has no raw response value")
        if request.get("beforeSequence") is None:
            _request_payload(call, parent=parent, child=child, expected_size=page_size)
            heads.append((call, page))
        else:
            earlier.append((call, page))
    _need(len(heads) == 1 and len(earlier) == 1,
          f"actual page{page_size} path must have exactly one head and one Earlier request")
    head_call, head = heads[0]
    head_entries = _validate_page(head, parent=parent, child=child, source=source,
                                  expected_size=page_size)
    _need(head_call["request"]["payload"].get("throughSequence") is None
          and head_call["request"]["payload"].get("historyRevision") is None,
          "initial history head unexpectedly used a prefix/cursor cache")
    frontier = min(_raw_entry_sequence(row) for row in head_entries)
    for call, page in earlier:
        _request_payload(call, parent=parent, child=child, expected_size=page_size,
                         before=frontier, through=head.get("throughSequence"),
                         revision=head.get("historyRevision"))
        _validate_page(page, parent=parent, child=child, source=source,
                       expected_size=page_size, before=frontier)
    return [head] + [page for _, page in earlier]


def _validate_initial_head(calls: list[dict[str, Any]], *, parent: str, child: str,
                           source: dict[str, Any], page_size: int,
                           expected_phase: str | None = None) -> dict[str, Any]:
    heads = []
    for call in calls:
        payload = call.get("request", {}).get("payload", {})
        if payload.get("maxMessages") != page_size or payload.get("beforeSequence") is not None:
            continue
        if expected_phase is not None and call.get("phase") != expected_phase:
            continue
        page = _page_value(call.get("response"))
        _need(page is not None, "actual initial history request failed")
        _request_payload(call, parent=parent, child=child, expected_size=page_size)
        _validate_page(page, parent=parent, child=child, source=source, expected_size=page_size)
        heads.append(page)
    _need(bool(heads), f"actual ChildRun initial snapshot lacks a raw page{page_size} request")
    _need(all(page == heads[0] for page in heads),
          "repeated initial reads changed the closed child source")
    return heads[0]


def _validate_actual_earlier(calls: list[dict[str, Any]], *, parent: str, child: str,
                             source: dict[str, Any], page_size: int,
                             head: dict[str, Any],
                             expected_phase: str | None = None) -> list[dict[str, Any]]:
    head_entries = head.get("entries")
    _need(isinstance(head_entries, list) and head_entries, "saved initial head has no source frontier")
    before = min(_raw_entry_sequence(row) for row in head_entries)
    matches = []
    for call in calls:
        payload = call.get("request", {}).get("payload", {})
        if payload.get("maxMessages") != page_size or payload.get("beforeSequence") is None:
            continue
        if expected_phase is not None and call.get("phase") != expected_phase:
            continue
        response = _page_value(call.get("response"))
        _need(response is not None, "actual Earlier request failed or lacks its raw response")
        _request_payload(call, parent=parent, child=child, expected_size=page_size,
                         before=before, through=head.get("throughSequence"),
                         revision=head.get("historyRevision"))
        _validate_page(response, parent=parent, child=child, source=source,
                       expected_size=page_size, before=before)
        matches.append(response)
    _need(len(matches) == 1, f"product must have exactly one true Earlier page{page_size} request")
    return matches


def _union_pages(pages: list[dict[str, Any]], source_entries: list[dict[str, Any]],
                 *, must_exhaust: bool = False) -> list[dict[str, Any]]:
    union: dict[int, dict[str, Any]] = {}
    for page in pages:
        for entry in page.get("entries", []):
            seq = _raw_entry_sequence(entry)
            if seq in union:
                _need(_canonical(union[seq]) == _canonical(entry),
                      f"same source identity {seq} changed content across pages")
            union[seq] = entry
    expected = {_raw_entry_sequence(row): row for row in source_entries}
    _need(all(seq in expected and _canonical(entry) == _canonical(expected[seq])
              for seq, entry in union.items()),
          "actual page union contains changed or non-groundtruth source content")
    if must_exhaust:
        _need(set(union) == set(expected),
              "the frozen initial/Earlier pages do not form the complete full200 source union")
    _need(bool(union), "actual source page union is empty")
    return [union[key] for key in sorted(union)]


def _source_message_map(entries: list[dict[str, Any]]) -> dict[int, tuple[str, str]]:
    expected: dict[int, tuple[str, str]] = {}
    for entry in entries:
        kind, data = _event_data(entry)
        message = data.get("message")
        if (kind == "assistant/message" and data.get("surfaceOp", "append") == "append"
                and isinstance(message, dict)):
            expected[_raw_entry_sequence(entry)] = (message.get("role"), _message_text(message))
    return expected


def _snapshot_items(snapshot: Any, source_entries: list[dict[str, Any]],
                    required_entries: list[dict[str, Any]] | None = None) -> list[dict[str, Any]]:
    _need(isinstance(snapshot, dict) and snapshot.get("isStale") is False,
          "rendered TranscriptSnapshot is missing or stale")
    items = snapshot.get("items")
    _need(isinstance(items, list), "TranscriptSnapshot has no items array")
    full_source = _source_message_map(source_entries)
    expected = _source_message_map(required_entries if required_entries is not None else source_entries)
    rendered = []
    for item in items:
        if not isinstance(item, dict):
            continue
        position = item.get("sourcePosition")
        seq = position.get("sequence") if isinstance(position, dict) else item.get("sourceSequence")
        if type(seq) is int and isinstance(item.get("text"), str):
            rendered.append((seq, item.get("kind"), item["text"]))
    _need(rendered, "TranscriptSnapshot lacks source sequence and content identities")
    _need(len({row[0] for row in rendered}) == len(rendered),
          "TranscriptSnapshot repeats a source identity")
    _need(all(seq in full_source for seq, _, _ in rendered),
          "TranscriptSnapshot contains a message absent from public source history")
    rendered_seq = [row[0] for row in rendered]
    _need(set(rendered_seq) == set(expected),
          "TranscriptSnapshot does not render exactly the message identities from its actual raw page union")
    for seq, kind, text in rendered:
        role, body = expected[seq]
        expected_kind = "assistant" if role == "assistant" else "user" if role == "user" else None
        _need(kind == expected_kind and text == body,
              f"TranscriptSnapshot changed source content/identity at sequence {seq}")
    _need([row[0] for row in rendered] == sorted(row[0] for row in rendered),
          "TranscriptSnapshot messages are not in original source order")
    return items


def _validate_sep_projection(event: dict[str, Any], pages: list[dict[str, Any]],
                             source_entries: list[dict[str, Any]]) -> None:
    """Validate SEP20's public projection directly, without inventing item mapping."""
    projection = event.get("projection")
    _need(isinstance(projection, dict), "SEP20 event has no public SessionProjection")
    union = _union_pages(pages, source_entries, must_exhaust=True)
    expected = [_canonical(row) for row in union]
    for label, rows in (("event unionEntries", event.get("unionEntries")),
                        ("SessionProjection.entries", projection.get("entries"))):
        if isinstance(rows, dict):
            values = list(rows.values())
        else:
            values = rows
        _need(isinstance(values, list), f"SEP20 {label} is not a raw entry collection")
        normalized = sorted(values, key=_raw_entry_sequence)
        _need([_canonical(row) for row in normalized] == expected,
              f"SEP20 {label} differs from the exact raw head/Earlier page union")
    snapshot = event.get("transcript")
    _need(isinstance(snapshot, dict) and snapshot.get("isStale") is False,
          "SEP20 TranscriptSnapshot is missing or stale")
    expected_more = pages[-1].get("hasMoreBefore")
    _need(type(expected_more) is bool and snapshot.get("hasMoreBefore") is expected_more,
          "SEP20 transcript Earlier affordance differs from the actual final page")
    if "hasMoreBefore" in projection:
        _need(projection.get("hasMoreBefore") is expected_more,
              "SEP20 projection Earlier affordance differs from the actual final page")
    items = snapshot.get("items")
    _need(isinstance(items, list), "SEP20 TranscriptSnapshot has no public items")
    item_ids = [item.get("id") for item in items if isinstance(item, dict)]
    _need(len(item_ids) == len(items) and all(isinstance(item_id, str) and item_id for item_id in item_ids)
          and len(item_ids) == len(set(item_ids)),
          "SEP20 transcript item identities are absent or duplicated")
    final_messages = [message for role, message in _source_message_map(union).values()
                      if role == "assistant" and message == "NATIVE_FAMILY_SEED_COMPLETE"]
    _need(len(final_messages) == 1 and final_messages[0] in _canonical(snapshot).decode("utf-8"),
          "SEP20 public transcript does not retain the original final assistant content")


def _validate_product_projection(event: dict[str, Any], actual: Any,
                                 source: dict[str, Any], parent: str, child: str) -> None:
    """Compare the exact public TranscriptSnapshot core with raw page projection.

    Native ChildRun adds exactly one public source Task turn/tool around the
    child transcript. Validate that decoration against the original sourceCall,
    remove only those exact records, then compare every remaining field with the
    independent SessionProjection::read snapshot supplied by the observer.
    """
    expected = event.get("rawProjectionTranscript")
    decoration = event.get("productSourceDecoration")
    _need(isinstance(actual, dict) and isinstance(expected, dict)
          and isinstance(decoration, dict),
          "product snapshot lacks raw projection or source-decoration evidence")
    source_call = source.get("sourceCall")
    _need(decoration.get("sourceCall") == source_call,
          "product source decoration differs from independent public sourceCall")
    call_id = source_call.get("callId") if isinstance(source_call, dict) else None
    _need(isinstance(call_id, str) and call_id,
          "public sourceCall does not provide its original Task call ID")
    task_id = json.dumps(["native", child, "task-source", call_id], separators=(",", ":"))
    turn = decoration.get("turn")
    tool = decoration.get("tool")
    _need(decoration.get("turnId") == task_id
          and decoration.get("toolId") == task_id + ":call"
          and isinstance(turn, dict) and isinstance(tool, dict),
          "source Task decoration has fabricated identities")
    _need(turn.get("id") == task_id and turn.get("input") == ""
          and turn.get("status") == "completed",
          "source Task turn differs from the documented Native sourceTask projection")
    _need(tool.get("id") == task_id + ":call" and tool.get("turnId") == task_id
          and tool.get("name") == source_call.get("name")
          and tool.get("arguments") == source_call.get("arguments")
          and tool.get("phase") == source_call.get("phase"),
          "source Task tool differs from the original Native sourceCall")
    actual_core = copy.deepcopy(actual)
    turns, tools = actual_core.get("turns"), actual_core.get("tools")
    _need(isinstance(turns, list) and turns and turns[0] == turn
          and isinstance(tools, list) and tools and tools[0] == tool,
          "actual ChildRun snapshot does not contain exactly the witnessed source Task decoration")
    _need(not any(row.get("id") == task_id for row in turns[1:] if isinstance(row, dict))
          and not any(row.get("id") == task_id + ":call" for row in tools[1:] if isinstance(row, dict)),
          "ChildRun snapshot duplicates its source Task decoration")
    turns.pop(0)
    tools.pop(0)
    _need(_canonical(actual_core) == _canonical(expected),
          "actual ChildRun TranscriptSnapshot core differs from SessionProjection::read over raw pages")
    _need("AKFAMILY_PARENT_DRAFT_KEEP" not in _canonical(actual).decode("utf-8"),
          "parent composer draft leaked into the child product transcript")


def _validate_latency(event: dict[str, Any], intent_key: str, publication_key: str,
                      latency_key: str) -> int:
    intent, publication, reported = event.get(intent_key), event.get(publication_key), event.get(latency_key)
    _need(type(intent) is int and type(publication) is int and publication >= intent
          and type(reported) is int and reported == publication - intent,
          f"{event.get('event')} has inconsistent original monotonic intent/publication timing")
    return reported


def _validate_event_identity(event: dict[str, Any], registration: dict[str, Any],
                             path: str, parent_field: str = "parentScope") -> None:
    _need(event.get("path") == path and event.get("role") == registration.get("role")
          and event.get("caseIndex") == registration.get("caseIndex"),
          "raw family event path/role/caseIndex differs from its official registration")
    scope = event.get(parent_field)
    parent = registration.get("parentSessionId")
    child = registration.get("childSessionId")
    _need(isinstance(scope, dict) and scope.get("sessionId") == parent
          and event.get("childId") == child,
          "raw family event uses a different public parent/child session pointer")


def _validate_reconnect_identity(event: dict[str, Any], parent: str, child: str) -> None:
    parent_scope = event.get("parentScope")
    _need(event.get("childId") == child
          and isinstance(parent_scope, dict) and parent_scope.get("sessionId") == parent,
          "reconnect changed the original parent/child session pointers")
    _need(event.get("childRunInstanceRetained") is True and event.get("converged") is True,
          "reconnect did not retain the original ChildRun and converge history")
    _need(type(event.get("endpointOrdinalBeforeReconnect")) is int
          and type(event.get("endpointOrdinalAfterReconnect")) is int
          and event["endpointOrdinalAfterReconnect"] > event["endpointOrdinalBeforeReconnect"]
          and type(event.get("connectionEpochBeforeReconnect")) is int
          and type(event.get("connectionEpochAfterReconnect")) is int
          and event["connectionEpochAfterReconnect"] > event["connectionEpochBeforeReconnect"],
          "reconnect did not publish a new endpoint connection and epoch")
    _need(event.get("parentDraft") == "AKFAMILY_PARENT_DRAFT_KEEP",
          "parent composer draft changed or leaked during child reconnect")
    _validate_latency(event, "intentMonotonicNs", "publicationMonotonicNs", "latencyNs")


def _protocol(events: list[dict[str, Any]], registration: dict[str, Any],
              path: str, source: dict[str, Any], source_entries: list[dict[str, Any]],
              mode: str, errors: list[str]) -> dict[str, Any]:
    parent, child = registration.get("parentSessionId"), registration.get("childSessionId")
    count = ROUND_COUNTS[mode]
    for name in ("consumer_started", "family_consumer_started"):
        rows = [row for row in events if row.get("event") == name]
        if len(rows) != 1:
            errors.append(f"missing or duplicate {name}")
        elif rows[0].get("path") != path or rows[0].get("role") != registration.get("role"):
            errors.append(f"{name} path/role differs from registration")
        elif rows[0].get("caseIndex") != registration.get("caseIndex"):
            errors.append(f"{name} caseIndex differs from registration")
    ready = [row for row in events if row.get("event") == "runtime_ready"]
    consumer = [row for row in events if row.get("event") == "consumer_started"]
    family_consumer = [row for row in events if row.get("event") == "family_consumer_started"]
    if len(ready) != 1 or len(consumer) != 1 or len(family_consumer) != 1:
        errors.append("Native consumer/runtime readiness boundary is missing or duplicated")
    elif (consumer[0].get("endpointId") != "native"
          or family_consumer[0].get("host", {}).get("product", {}).get("version") != "0.9.3"
          or family_consumer[0].get("expectedVersion") != "0.9.3"):
        errors.append("observed endpoint/Kernel handshake differs from Native 0.9.3")
    standard = [row for row in events if row.get("event") == "iteration_started"]
    family = [row for row in events if row.get("event") == "family_iteration_started"]
    completed_rows = [row for row in events if row.get("event") == "family_iteration_completed"]
    completed = {row.get("round") for row in completed_rows}
    failures = [row for row in events if row.get("event") in ("family_iteration_failed", "consumer_error")]
    if len(standard) != count or len(family) != count:
        errors.append("raw iteration-start count differs from the frozen case profile")
    for round_index in range(count):
        expected_measured = round_index in MEASURED_ROUNDS[mode]
        pair = ([row for row in standard if row.get("round", row.get("iteration")) == round_index]
                + [row for row in family if row.get("round") == round_index])
        if len(pair) != 2:
            errors.append(f"round {round_index} lacks both original iteration boundaries")
        for row in pair:
            if (row.get("path") != path or row.get("role") != registration.get("role")
                    or row.get("caseIndex") != registration.get("caseIndex")
                    or row.get("measured") is not expected_measured):
                errors.append(f"round {round_index} differs from registered path/role/case/init contract")
    if len(completed_rows) != count or completed != set(range(count)) or failures:
        errors.append("not all original protocol iterations completed without a failure")

    snapshot_rows = [row for row in events if row.get("event") == "family_first_snapshot"]
    page_rows = [row for row in events if row.get("event") == "family_history_page"]
    product_rows = [row for row in events if row.get("event") == "family_product_earlier50"]
    catalog_rows = [row for row in events if row.get("event") == "family_catalog"]
    snapshots = {row.get("round"): row for row in snapshot_rows}
    pages = {row.get("round"): row for row in page_rows}
    products = {row.get("round"): row for row in product_rows}
    catalogs = {row.get("round"): row for row in catalog_rows}
    reconnects = [row for row in events if row.get("event") == "family_reconnect"]
    groundtruths = [row for row in events if row.get("event") == "family_groundtruth"]
    expected_rounds = set(range(count))
    if (len(snapshot_rows) != count or len(page_rows) != count
            or set(snapshots) != expected_rounds or set(pages) != expected_rounds):
        errors.append("first snapshot or actual page20 evidence is incomplete/duplicated")
    if len(catalog_rows) != count or set(catalogs) != expected_rounds:
        errors.append("original public child catalog evidence is incomplete/duplicated")
    if path == "common50" and (len(product_rows) != count or set(products) != expected_rounds):
        errors.append("common50 did not perform actual ChildRun Earlier50 on every round")
    if path == "product20" and products:
        errors.append("product20 path mixed in a common50 product operation")
    if mode == "reconnectLifecycle":
        if (len(reconnects) != count
                or {row.get("round") for row in reconnects} != expected_rounds):
            errors.append("lifecycle lacks original-ChildRun reconnects for init plus all ten measured rounds")
    elif reconnects:
        errors.append("unexpected reconnect in a non-lifecycle case")
    case_call_copies: list[dict[str, Any]] = []
    for owner, field in ([(row, "boundedCalls") for row in snapshot_rows]
                         + [(row, "rawCalls") for row in page_rows]
                         + [(row, "boundedCalls") for row in product_rows]
                         + [(row, "boundedCalls") for row in reconnects]):
        try:
            case_call_copies.extend(_calls_from(owner.get(field)))
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"case call ledger is incomplete: {error}")
    if case_call_copies:
        try:
            raw_call_events = [row for row in events if row.get("event") == "bounded_call"]
            nested_calls = _dedupe_calls(case_call_copies)
            raw_calls = _dedupe_calls(raw_call_events)
            if (len(raw_call_events) != len(raw_calls)
                    or {row["sourceReadOrdinal"]: _canonical(row) for row in nested_calls}
                    != {row["sourceReadOrdinal"]: _canonical(row) for row in raw_calls}):
                errors.append("raw stdout bounded_call stream is missing or differs from embedded evidence")
            ordinals = [row["sourceReadOrdinal"] for row in nested_calls]
            if ordinals != list(range(1, len(ordinals) + 1)):
                errors.append("case-wide sourceReadOrdinal sequence has an omitted or substituted original call")
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"case-wide original call ordinals are inconsistent: {error}")
    if len(groundtruths) != 1 or groundtruths[0].get("sourceHistory") != source:
        errors.append("observer post-measurement full200 groundtruth differs from independent public source")
    elif groundtruths[0].get("rawPageUnion") != source_entries:
        errors.append("observer raw page union differs from every ordered full200 source entry")
    elif (groundtruths[0].get("parentSessionId") != parent
          or groundtruths[0].get("childSessionId") != child
          or groundtruths[0].get("eof") is not True):
        errors.append("observer groundtruth lost Native parent/child ownership or EOF proof")

    latencies = {"snapshot": {}, "page20": {}, "reconnect": {}}
    for round_index in range(count):
        measured = round_index in MEASURED_ROUNDS[mode]
        first, page = snapshots.get(round_index), pages.get(round_index)
        round_call_rows: list[dict[str, Any]] = []
        for owner_event, field in ((first, "boundedCalls"), (page, "rawCalls"),
                                   (products.get(round_index), "boundedCalls")):
            if owner_event:
                try:
                    round_call_rows.extend(_calls_from(owner_event.get(field)))
                except (ValueError, TypeError, KeyError) as error:
                    errors.append(f"round {round_index} missing raw call rows: {error}")
        round_call_rows.extend(call for row in reconnects if row.get("round") == round_index
                               for call in row.get("boundedCalls", [])
                               if isinstance(call, dict) and call.get("event") == "bounded_call")
        if round_call_rows:
            try:
                round_call_rows = _dedupe_calls(round_call_rows)
                _validate_all_recorded_calls(
                    round_call_rows, parent=parent, child=child, source=source,
                    source_entries=source_entries, path=path, role=registration.get("role"),
                    case_index=registration.get("caseIndex"), round_index=round_index,
                    measured=measured)
            except (ValueError, TypeError, KeyError) as error:
                errors.append(f"round {round_index} raw bounded calls failed independent validation: {error}")
        catalog_event = catalogs.get(round_index)
        if catalog_event:
            catalog = catalog_event.get("publicCatalog")
            child_entry = catalog_event.get("childEntry")
            availability = catalog_event.get("childAvailability")
            if (catalog_event.get("parentSessionId") != parent or catalog_event.get("childId") != child
                    or catalog_event.get("path") != path or catalog_event.get("role") != registration.get("role")
                    or catalog_event.get("caseIndex") != registration.get("caseIndex")
                    or catalog_event.get("measured") is not measured):
                errors.append(f"public catalog round {round_index} has wrong owner/path/role/init status")
            catalog_rows = catalog.get("entries", []) if isinstance(catalog, dict) else []
            matches = [item for item in catalog_rows
                       if isinstance(item, dict) and item.get("id") == child]
            if len(matches) != 1 or not isinstance(child_entry, dict) or matches[0] != child_entry or matches[0].get("mode") != "bounded":
                errors.append(f"public catalog round {round_index} does not identify the one original bounded child")
            if (not isinstance(availability, dict) or availability.get("catalogMode") != "bounded"
                    or availability.get("history") != "readOnlyBounded"
                    or availability.get("send") != "unavailableByBoundedChildContract"
                    or availability.get("prompt") != "unavailableByBoundedChildContract"
                    or availability.get("cancel") != "unavailableByBoundedChildContract"
                    or availability.get("parentAvailable") is not True
                    or availability.get("childId") != child):
                errors.append(f"public catalog round {round_index} violates Native bounded-child read-only contract")
        if first:
            try:
                _validate_event_identity(first, registration, path)
            except ValueError as error:
                errors.append(f"first snapshot round {round_index} identity failed: {error}")
            if (first.get("parentDraft") != "AKFAMILY_PARENT_DRAFT_KEEP"
                    or first.get("measured") is not measured):
                errors.append(f"first snapshot round {round_index} changed owner, draft, or init status")
            if first.get("sourceCall") != source.get("sourceCall"):
                errors.append(f"first snapshot round {round_index} lost its original task sourceCall identity")
            try:
                latency = _validate_latency(first, "intentMonotonicNs", "publicationMonotonicNs", "latencyNs")
            except ValueError as error:
                errors.append(f"first snapshot round {round_index} timing invalid: {error}")
                latency = None
            if type(latency) is int and measured:
                latencies["snapshot"][round_index] = latency
            else:
                errors.append(f"first snapshot round {round_index} lacks a valid publication latency")
            # Raw catalog, not a sourceChecks flag, proves the actual bounded child.
            catalog = first.get("publicCatalog")
            child_entry = first.get("childEntry")
            if child_entry is None and isinstance(catalog, dict):
                owned_rows = [row for row in catalog.get("entries", [])
                              if isinstance(row, dict) and row.get("id") == child]
                child_entry = owned_rows[0] if len(owned_rows) == 1 else None
            availability = first.get("childAvailability")
            if not isinstance(catalog, dict) or not isinstance(child_entry, dict) or not isinstance(availability, dict):
                errors.append(f"first snapshot round {round_index} lacks raw public bounded-child catalog/capability")
            else:
                catalog_rows = catalog.get("entries", [])
                matches = [row for row in catalog_rows if isinstance(row, dict) and row.get("id") == child]
                if len(matches) != 1 or matches[0].get("mode") != "bounded" or matches[0] != child_entry:
                    errors.append(f"first snapshot round {round_index} raw catalog does not own its bounded child")
                if (availability.get("catalogMode") != "bounded"
                        or availability.get("history") != "readOnlyBounded"
                        or availability.get("send") != "unavailableByBoundedChildContract"
                        or availability.get("prompt") != "unavailableByBoundedChildContract"
                        or availability.get("cancel") != "unavailableByBoundedChildContract"
                        or availability.get("parentAvailable") is not True
                        or availability.get("childId") != child):
                    errors.append(f"first snapshot round {round_index} raw bounded-child contract is invalid")
            try:
                calls = _calls_from(first.get("boundedCalls"))
                page_size = 50 if path == "common50" else 20
                head = _validate_initial_head(calls, parent=parent, child=child,
                                              source=source, page_size=page_size,
                                              expected_phase="childrun-open-and-first-snapshot")
                _union_pages([head], source_entries)
                _validate_product_projection(first, first.get("transcript"), source, parent, child)
                if first.get("transcript", {}).get("hasMoreBefore") is not True:
                    raise ValueError("initial ChildRun page does not expose Earlier history")
            except (ValueError, TypeError, KeyError) as error:
                errors.append(f"first snapshot round {round_index} raw source validation failed: {error}")
        if page:
            try:
                _validate_event_identity(page, registration, path)
            except ValueError as error:
                errors.append(f"page round {round_index} identity failed: {error}")
            if (page.get("requestedMessages") != 20 or page.get("measured") is not measured):
                errors.append(f"page round {round_index} is not the original page20 operation")
            try:
                if page.get("mechanism") == "sep20-projection":
                    phase_head = "sep20-head"
                    phase_earlier = "sep20-earlier"
                else:
                    phase_head = "childrun-open-and-first-snapshot"
                    phase_earlier = "product-childrun-earlier20"
                if page.get("mechanism") == "sep20-projection":
                    latency = _validate_latency(page, "earlierIntentMonotonicNs",
                                                "earlierPublicationMonotonicNs", "earlierLatencyNs")
                else:
                    latency = _validate_latency(page, "intentMonotonicNs",
                                                "publicationMonotonicNs", "latencyNs")
            except ValueError as error:
                errors.append(f"page round {round_index} timing invalid: {error}")
                latency = None
            if type(latency) is int and measured:
                latencies["page20"][round_index] = latency
            else:
                errors.append(f"page round {round_index} has no real intent-to-publication latency")
            expected_mechanism = "sep20-projection" if path == "common50" else "childrun20"
            if page.get("mechanism") != expected_mechanism:
                errors.append(f"page round {round_index} did not use its registered product path")
            try:
                page_calls = _calls_from(page.get("rawCalls"))
                if path == "common50":
                    raw_pages = _validate_page_pair(page_calls, parent=parent, child=child,
                                                    source=source, page_size=20,
                                                    head_phase=phase_head, earlier_phase=phase_earlier)
                else:
                    first_calls = _calls_from(first.get("boundedCalls")) if first else []
                    head = _validate_initial_head(first_calls, parent=parent, child=child,
                                                  source=source, page_size=20,
                                                  expected_phase=phase_head)
                    earlier = _validate_actual_earlier(
                        _calls_from(page.get("renderEarlierCalls")), parent=parent, child=child,
                        source=source, page_size=20, head=head, expected_phase=phase_earlier)
                    raw_pages = [head] + earlier
                page_union = _union_pages(raw_pages, source_entries, must_exhaust=True)
                if path == "common50":
                    _validate_sep_projection(page, raw_pages, source_entries)
                else:
                    _need(page.get("rawUnion") == page_union,
                          "product20 rawUnion differs from its actual head plus render Earlier pages")
                    _validate_product_projection(page, page.get("transcript"), source, parent, child)
            except (ValueError, TypeError, KeyError) as error:
                errors.append(f"page round {round_index} raw request/projection validation failed: {error}")
        product = products.get(round_index)
        if product:
            if product.get("requestedMessages") != 50 or "ChildRun::open" not in str(product.get("mechanism", "")):
                errors.append(f"common50 round {round_index} lacks real default50 Earlier path")
            try:
                actual50_ns = _validate_latency(product, "intentMonotonicNs",
                                                "publicationMonotonicNs", "latencyNs")
                if product.get("loaded") is not True:
                    raise ValueError("ChildRun::load_earlier did not report loaded history")
                if measured:
                    latencies.setdefault("actual50Earlier", {})[round_index] = actual50_ns
                first_calls = _calls_from(first.get("boundedCalls")) if first else []
                head = _validate_initial_head(first_calls, parent=parent, child=child,
                                              source=source, page_size=50,
                                              expected_phase="childrun-open-and-first-snapshot")
                earlier = _validate_actual_earlier(
                    _calls_from(product.get("renderEarlierCalls")), parent=parent, child=child,
                    source=source, page_size=50, head=head,
                    expected_phase="product-childrun-earlier50")
                product_union = _union_pages([head] + earlier, source_entries, must_exhaust=True)
                _validate_product_projection(product, product.get("transcript"), source, parent, child)
            except (ValueError, TypeError, KeyError) as error:
                errors.append(f"common50 Earlier50 raw proof failed: {error}")
    for row in reconnects:
        round_index = row.get("round")
        if round_index not in range(count):
            errors.append("reconnect occurred outside the frozen lifecycle trajectory")
            continue
        measured = round_index in MEASURED_ROUNDS[mode]
        try:
            latency = _validate_latency(row, "intentMonotonicNs",
                                        "publicationMonotonicNs", "latencyNs")
        except ValueError as error:
            errors.append(f"reconnect round {round_index} timing invalid: {error}")
            latency = None
        if type(latency) is int and measured:
            latencies["reconnect"][round_index] = latency
        elif type(latency) is not int:
            errors.append(f"reconnect round {round_index} lacks original intent-to-convergence latency")
        try:
            _validate_event_identity(row, registration, path)
            if row.get("measured") is not measured:
                raise ValueError("reconnect init/measured status differs from its frozen round")
            _validate_reconnect_identity(row, parent, child)
        except ValueError as error:
            errors.append(f"reconnect round {round_index} identity/epoch/draft proof failed: {error}")
        try:
            if row.get("sourceCall") != source.get("sourceCall"):
                raise ValueError("sourceCall changed across the new endpoint epoch")
            loaded = products.get(round_index)
            if loaded:
                calls = _calls_from(first.get("boundedCalls")) if first else []
                head = _validate_initial_head(calls, parent=parent, child=child,
                                              source=source, page_size=50,
                                              expected_phase="childrun-open-and-first-snapshot")
                earlier = _validate_actual_earlier(_calls_from(loaded.get("renderEarlierCalls")),
                                                   parent=parent, child=child,
                                                   source=source, page_size=50, head=head,
                                                   expected_phase="product-childrun-earlier50")
                required = _union_pages([head] + earlier, source_entries, must_exhaust=True)
            else:
                raise ValueError("reconnect has no prior actual ChildRun Earlier50 source pages")
            _validate_product_projection(row, row.get("beforeTranscript"), source, parent, child)
            _validate_product_projection(row, row.get("afterTranscript"), source, parent, child)
            if row.get("beforeTranscript") != row.get("afterTranscript"):
                raise ValueError("same ChildRun transcript changed across endpoint reconnect")
            reconnect_union = []
            for page_name in ("sourceBefore", "sourceAfter"):
                raw_pages = row.get(page_name)
                if not isinstance(raw_pages, list) or not raw_pages:
                    raise ValueError(f"{page_name} raw Native source witness is missing")
                for raw_page in raw_pages:
                    _need(isinstance(raw_page, dict)
                          and raw_page.get("parentSessionId") == parent
                          and raw_page.get("childSessionId") == child
                          and raw_page.get("throughSequence") == source.get("throughSequence")
                          and raw_page.get("historyRevision") == source.get("historyRevision")
                          and raw_page.get("sourceCall") == source.get("sourceCall")
                          and raw_page.get("runOpen") is False,
                          f"{page_name} changed bounded source owner/cut/revision/sourceCall")
                    _union_pages([raw_page], source_entries)
                    reconnect_union.append(raw_page)
            _union_pages(reconnect_union, source_entries, must_exhaust=True)
            catalog = row.get("publicCatalog")
            if not isinstance(catalog, dict) or not any(
                    isinstance(item, dict) and item.get("id") == child and item.get("mode") == "bounded"
                    for item in catalog.get("entries", [])):
                raise ValueError("new endpoint catalog no longer identifies the original bounded child")
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"reconnect round {round_index} raw continuity validation failed: {error}")
    return {"latenciesNs": latencies, "successfulRounds": sorted(completed),
            "iterationCount": count, "reconnectCount": len(reconnects)}


def _validate_lifecycle(case: Path, mode: str, registration: dict[str, Any],
                        receipt: dict[str, Any],
                        errors: list[str], *, cleanup_deadline_ms: int = 10000) -> dict[str, Any]:
    boundaries = _read_jsonl(case / "consumer-process-boundaries.jsonl", "consumer-process-boundaries.jsonl")
    rss_rows = _read_jsonl(case / "measurement-rss.jsonl", "measurement-rss.jsonl")
    rss = _read_json(case / "last-rss-summary.json", "last-rss-summary.json")
    starts = [row for row in boundaries if row.get("event") == "iteration_started"]
    expected = ROUND_COUNTS[mode]
    if len(starts) != expected or [row.get("iteration", row.get("round")) for row in starts] != list(range(expected)):
        errors.append("OS sidecar lacks exact process boundaries for every iteration")
    if any(row.get("error") is not None or not isinstance(row.get("osProcesses"), list) for row in starts):
        errors.append("one or more OS process-boundary observations failed")
    for row in starts:
        if type(row.get("monotonicNs")) is not int:
            errors.append("OS iteration boundary lacks its original observed monotonic time")
    raw_events = _read_jsonl(case / "consumer.stdout.jsonl", "consumer.stdout.jsonl")
    for row in starts:
        raw_index = row.get("rawEventIndex")
        if type(raw_index) is not int or not 0 <= raw_index < len(raw_events):
            errors.append("OS iteration boundary does not address an original raw consumer event")
            continue
        original = raw_events[raw_index]
        if (original.get("event") != "iteration_started"
                or original.get("round", original.get("iteration")) != row.get("iteration")
                or original.get("mode") != mode or original.get("path") != registration.get("path")
                or original.get("role") != registration.get("role")
                or original.get("caseIndex") != registration.get("caseIndex")
                or original.get("topology") != "native-bounded-family"
                or row.get("originalIntentMonotonicNs") != original.get("intentMonotonicNs")):
            errors.append("OS process boundary differs from its original path/role/mode/round event")
    fixture = _read_json(case / "fixture-lifecycle.json", "fixture-lifecycle.json")
    operations = {row.get("operation"): row for row in fixture if isinstance(row, dict)} if isinstance(fixture, list) else {}
    measurement_row = operations.get("resume") or operations.get("start") or {}
    measurement_process = measurement_row.get("process") if isinstance(measurement_row, dict) else None
    identities, harness_identities = [], []
    root = receipt.get("consumerRoot") or receipt.get("consumerExit")
    for row in starts:
        process_rows = row.get("osProcesses", [])
        owned = [proc for proc in process_rows if proc.get("role") == "agentkit"
                 and root and proc.get("pid") == root.get("pid")
                 and proc.get("startIdentity") == root.get("startIdentity")]
        if len(owned) != 1:
            errors.append("iteration OS boundary does not observe the registered Native consumer PID/start identity")
        else:
            identities.append((owned[0].get("pid"), owned[0].get("startIdentity")))
        if not isinstance(root, dict):
            errors.append("official receipt omits the Native consumer root identity")
        elif not owned:
            errors.append("Kernel/consumer identity differs between OS iteration boundaries")
        if not isinstance(measurement_process, dict):
            errors.append("fixture lifecycle lacks its measured Native Kernel root identity")
        else:
            harness = [proc for proc in process_rows if proc.get("role") == "harness"
                       and proc.get("pid") == measurement_process.get("pid")
                       and proc.get("startIdentity") == measurement_process.get("startIdentity")]
            if len(harness) != 1:
                errors.append("iteration OS boundary does not observe the registered Native Kernel root")
            else:
                harness_identities.append((harness[0].get("pid"), harness[0].get("startIdentity")))
    if len(identities) != expected or len(set(identities)) != 1:
        errors.append("same Rust consumer PID/start identity was not retained across the case")
    if len(harness_identities) != expected or len(set(harness_identities)) != 1:
        errors.append("same Native Kernel/harness PID/start identity was not retained across the case")
    roots = rss.get("roots")
    root_by_role = {}
    if isinstance(roots, dict):
        root_by_role = {value.get("role"): (int(pid), value.get("startIdentity"))
                        for pid, value in roots.items()
                        if isinstance(value, dict) and str(pid).isdigit()}
    if (not isinstance(root, dict) or not isinstance(measurement_process, dict)
            or root_by_role.get("agentkit") != (root.get("pid"), root.get("startIdentity"))
            or root_by_role.get("harness") != (measurement_process.get("pid"),
                                                measurement_process.get("startIdentity"))):
        errors.append("RSS sampler roots differ from the original consumer and measured Kernel identities")
    max_gap = rss.get("maxObservedIntervalNs")
    if type(max_gap) is not int or not 0 < max_gap <= 250_000_000:
        errors.append("50ms RSS sample gap is missing or exceeds the frozen 250ms limit")
    if rss.get("sampleCount") != len(rss_rows) or not rss_rows:
        errors.append("RSS summary does not match the raw sample count")
    if rss.get("requestedIntervalMs") != 50:
        errors.append("RSS sampler receipt does not attest the frozen 50ms requested interval")
    clocks = [row.get("monotonicNs") for row in rss_rows]
    if any(type(value) is not int for value in clocks) or clocks != sorted(clocks):
        errors.append("RSS sample clocks are missing or unordered")
    elif len(clocks) > 1:
        raw_gap = max(b - a for a, b in zip(clocks, clocks[1:]))
        if raw_gap > 250_000_000:
            errors.append("raw RSS sample gap exceeds 250ms")
        if raw_gap != max_gap:
            errors.append("RSS max-gap summary differs from the raw monotonic sample sequence")
    peaks = {"agentkit": 0, "harness": 0, "provider": 0}
    known_processes = rss.get("knownProcesses")
    known_ids = {(row.get("pid"), row.get("startIdentity")): row.get("role")
                 for row in known_processes if isinstance(row, dict)} \
        if isinstance(known_processes, list) else {}
    if not known_ids:
        errors.append("RSS summary lacks the original OS-observed owned process identities")
    for row in rss_rows:
        processes = row.get("processes")
        if not isinstance(processes, list):
            errors.append("RSS sample lacks raw per-process OS identities")
            continue
        if any(not isinstance(proc, dict)
               or known_ids.get((proc.get("pid"), proc.get("startIdentity"))) != proc.get("role")
               for proc in processes):
            errors.append("RSS sample contains a process outside the observed PID/start-identity census")
        for role in peaks:
            total = sum(proc.get("rssKiB", 0) for proc in processes
                        if isinstance(proc, dict) and proc.get("role") == role
                        and type(proc.get("rssKiB")) is int)
            peaks[role] = max(peaks[role], total)
        if (not isinstance(row.get("treeRssKiB"), dict)
                or any(row["treeRssKiB"].get(role) != sum(
                    proc.get("rssKiB", 0) for proc in processes
                    if isinstance(proc, dict) and proc.get("role") == role
                    and type(proc.get("rssKiB")) is int) for role in peaks)):
            errors.append("raw RSS simultaneous tree totals differ from their process ownership rows")
    claimed = rss.get("sampledTreePeakRssKiB")
    if not isinstance(claimed, dict) or any(claimed.get(role) != value for role, value in peaks.items()):
        errors.append("claimed RSS peaks differ from simultaneous sums of raw owned process samples")
    retirement = _read_json(case / "owned-process-retirement.json", "owned-process-retirement.json")
    consumer_exit = _read_json(case / "consumer-exit.json", "consumer-exit.json")
    if consumer_exit.get("exitStatus") != 0 or consumer_exit.get("ownedGroupAbsent") is not True:
        errors.append("Native consumer did not exit together with its owned process group")
    if receipt.get("consumerExitSha256") and receipt["consumerExitSha256"] != _file_sha(case / "consumer-exit.json"):
        errors.append("consumer-exit raw receipt hash mismatch")
    if (retirement.get("status") != "verified"
            or retirement.get("ownedAgentKitOrHarnessProcessesRemaining")):
        errors.append("OS retirement census did not prove all owned roots exited")
    root_pid = retirement.get("newRootPID") or retirement.get("consumerRoot") or retirement.get("root")
    if not isinstance(root_pid, dict) or (root_pid.get("pid"), root_pid.get("startIdentity")) != (
            consumer_exit.get("pid"), consumer_exit.get("startIdentity")):
        errors.append("OS retirement receipt does not bind the exact original Native root PID/start identity")
    lower = retirement.get("cleanupIntentMonotonicNsLowerBound")
    exited = retirement.get("allOwnedRootsExitedMonotonicNs")
    intent_observed = retirement.get("cleanupIntentObservedMonotonicNs")
    runtime_cleanup = retirement.get("runtimeCleanupCompletedMonotonicNs")
    cleanup_limit = cleanup_deadline_ms
    if retirement.get("cleanupDeadlineMs") != cleanup_limit:
        errors.append("OS cleanup receipt changed the preregistered absolute cleanup budget")
    elapsed = exited - lower if type(exited) is int and type(lower) is int else None
    if elapsed is None or elapsed < 0 or type(cleanup_limit) is not int or elapsed > cleanup_limit * 1_000_000:
        errors.append("conservative lower-bound to OS-root-exit cleanup exceeds its frozen absolute budget")
    if (type(intent_observed) is not int or type(runtime_cleanup) is not int
            or runtime_cleanup < intent_observed or type(exited) is not int or exited < runtime_cleanup
            or retirement.get("cleanupElapsedNs") != elapsed):
        errors.append("cleanup sidecar does not preserve the observed intent/runtime-close/OS-exit sequence")
    if not isinstance(fixture, list):
        errors.append("fixture lifecycle must retain its original process/timing rows")
    else:
        operations = {row.get("operation"): row for row in fixture if isinstance(row, dict)}
        required_lifecycle = (("seed_retire", "measurement_retire") if mode == "cold"
                             else ("measurement_retire",))
        for name in required_lifecycle:
            row = operations.get(name)
            if not isinstance(row, dict) or row.get("exitStatus") != 0:
                errors.append(f"owned Native fixture {name} did not exit successfully")
        for name, row in operations.items():
            if not isinstance(row, dict):
                continue
            times = [row.get(key) for key in ("intentMonotonicNs", "readyMonotonicNs", "completedMonotonicNs")
                     if row.get(key) is not None]
            if any(type(value) is not int for value in times) or times != sorted(times):
                errors.append(f"fixture lifecycle operation {name} has invalid monotonic boundaries")
        measurement = operations.get("resume") or operations.get("start")
        process = measurement.get("process") if isinstance(measurement, dict) else None
        if not isinstance(process, dict) or (process.get("pid"), process.get("startIdentity")) != (
                harness_identities[0] if harness_identities else (None, None)):
            errors.append("measured Native Kernel root differs from fixture lifecycle process identity")
    if mode == "cold":
        restart = _read_json(case / "cold-kernel-restart.json", "cold-kernel-restart.json")
        if (restart.get("seedClosed") is not True or restart.get("seedStateReused") is not True
                or restart.get("seedExitStatus") != 0
                or restart.get("privateDatabaseRewrite") is not False):
            errors.append("cold case lacks a closed, publicly seeded read-only state before measurement")
        old = (restart.get("oldPid"), restart.get("oldStartIdentity"))
        new = (restart.get("newPid"), restart.get("newStartIdentity"))
        measured_kernel = harness_identities[0] if harness_identities else None
        measured_consumer = (consumer_exit.get("pid"), consumer_exit.get("startIdentity"))
        if old == new or new != measured_kernel:
            errors.append("cold measurement did not start a fresh Kernel root over the same closed seed")
        lifecycle_rows = {row.get("operation"): row for row in fixture if isinstance(row, dict)} if isinstance(fixture, list) else {}
        seed_process = lifecycle_rows.get("seed_retire", {}).get("process")
        measurement_row = lifecycle_rows.get("resume") or lifecycle_rows.get("start") or {}
        measured_process = measurement_row.get("process")
        if not isinstance(seed_process, dict) or (
                seed_process.get("pid"), seed_process.get("startIdentity")) != old:
            errors.append("cold old Kernel identity differs from the OS-observed seed-retirement root")
        if not isinstance(measured_process, dict) or (
                measured_process.get("pid"), measured_process.get("startIdentity")) != new:
            errors.append("cold new Kernel identity differs from the OS-observed measured fixture root")
        if (restart.get("parentSessionId") != registration.get("parentSessionId")
                or restart.get("childSessionId") != registration.get("childSessionId")):
            errors.append("cold restart changed the original public parent/child pointer")
        if identities and identities[0] != measured_consumer:
            errors.append("cold iteration boundaries differ from the captured Rust consumer root")
        if (restart.get("consumerPid"), restart.get("consumerStartIdentity")) != measured_consumer:
            errors.append("cold restart receipt does not bind the measured Rust consumer separately")
        if receipt.get("seedExitStatus") not in (None, 0):
            errors.append("official receipt reports a failed cold seed process exit")
        if (restart.get("seedReceiptSha256") is None
                or receipt.get("seedReceiptSha256") != restart.get("seedReceiptSha256")):
            errors.append("cold closed-seed proof is not bound to its exact raw receipt SHA")
        closed_seed_path = case / ".agentkit-performance-closed-seed.json"
        try:
            closed_seed = _read_json(closed_seed_path, "closed-seed receipt")
            closed_sha = _file_sha(closed_seed_path)
            if (closed_sha != restart.get("seedReceiptSha256")
                    or closed_sha != receipt.get("seedReceiptSha256")
                    or closed_sha != receipt.get("seed", {}).get("closedSeedReceiptSha256")
                    or closed_seed.get("exitCode") != 0
                    or not isinstance(closed_seed.get("closedSeedProcesses"), list)
                    or not closed_seed.get("closedSeedProcesses")):
                errors.append("cold closed-seed raw receipt is not the exact successful retired-seed proof")
            elif not any((row.get("pid"), row.get("startIdentity")) == old
                         for row in closed_seed["closedSeedProcesses"] if isinstance(row, dict)):
                errors.append("cold closed-seed process census omits the original retired Kernel root")
        except (OSError, ValueError, TypeError) as error:
            errors.append(f"cold closed-seed raw receipt is missing or invalid: {error}")
    return {"peaksKiB": peaks, "maxGapNs": max_gap, "cleanupElapsedNs": elapsed,
            "retirementVerified": retirement.get("status") == "verified"}


def materialize_sample(case: Path, registration: dict[str, Any],
                       base_profile: dict[str, Any], family_profile: dict[str, Any], *,
                       pilot: bool = False) -> dict[str, Any]:
    """Materialize one registered case while retaining every failure and sidecar."""
    case = Path(case)
    errors: list[str] = []
    evidence: dict[str, Any] = {}
    for name in SIDECARS:
        try:
            evidence[name] = (_read_jsonl(case / name, name) if name.endswith(".jsonl")
                              else _read_json(case / name, name))
        except (OSError, ValueError) as error:
            errors.append(str(error))
    inputs = evidence.get("inputs.json", {})
    receipt = evidence.get("official-execution-receipt.json", {})
    if not isinstance(inputs, dict):
        errors.append("inputs.json must be a raw official identity object")
        inputs = {}
    if not isinstance(receipt, dict):
        errors.append("official execution receipt must be an object")
        receipt = {}
    mode, path, role = registration.get("mode"), registration.get("path"), registration.get("role")
    official = (registration.get("official") is True and receipt.get("official") is True
                and receipt.get("diagnosticOnly") is False)
    sample = {"sampleId": registration.get("sampleId"), "caseName": registration.get("caseName"),
              "caseIndex": registration.get("caseIndex"), "mode": mode, "path": path,
              "role": role, "endpoint": "native", "sourceCommit": registration.get("sourceCommit"),
              "programSha256": registration.get("programSha256"),
              "consumerSha256": registration.get("consumerSha256"),
              "runnerSha256": registration.get("runnerSha256"),
              "runtimeSha256": registration.get("runtimeSha256"),
              "observerSha256": registration.get("observerSha256") or receipt.get("observerSha256"),
              "official": official, "diagnosticOnly": registration.get("diagnosticOnly") is True
              or receipt.get("diagnosticOnly") is True, "status": "failed", "failureReason": None,
              "registration": registration, "receipt": receipt, "inputs": inputs, "evidence": evidence}
    if family_profile.get("kind") != "agentkit-native-family-profile" or family_profile.get("format") != 1:
        errors.append("unsupported Native Family profile")
    if base_profile.get("format") != 1:
        errors.append("unsupported original stage performance profile")
    base_thresholds = base_profile.get("thresholds", {})
    family_thresholds = family_profile.get("thresholds", {})
    for field in ("latencyRegressionRatioMax", "peakRssRegressionRatioMax",
                  "firstUsableSnapshotP95Ms", "historyPageToProjectionP95Ms",
                  "sendToFirstPublicFrameP95Ms",
                  "explicitReconnectToConvergedP95Ms", "agentkitPeakRssMiBMax",
                  "ownedHarnessPeakRssMiBMax", "ownedCleanupDeadlineMs"):
        if family_thresholds.get(field) != base_thresholds.get(field):
            errors.append(f"Native Family changed original frozen budget {field}")
    if path not in PATHS or role not in ROLES or mode not in MODES:
        errors.append("registration path/role/mode is outside the frozen case matrix")
    if family_profile.get("issue") != 56 or registration.get("sourceCommit") != family_profile.get(
            "sourceProducts", {}).get(role):
        errors.append("registration Issue/source revision differs from the frozen role profile")
    if role not in family_profile.get("paths", {}).get(path, {}).get("roles", []):
        errors.append("role is not registered for this exact product path")
    expected_count = CASE_COUNTS.get(mode, 0)
    if type(registration.get("caseIndex")) is not int or not 0 <= registration["caseIndex"] < expected_count:
        errors.append("caseIndex is outside the preregistered path/mode group")
    if registration.get("iterations") != ROUND_COUNTS.get(mode):
        errors.append("iteration count differs from the frozen trajectory")
    if path == "product20" and registration.get("cargoFeatures") != ["acceptance-native-family-page20"]:
        errors.append("product20 lacks the exact preregistered consumer feature")
    for field in ("sampleId", "caseName", "sourceCommit", "role", "path", "mode", "caseIndex"):
        if receipt.get(field) != registration.get(field):
            errors.append(f"official receipt {field} differs from original registration")
    if receipt.get("familyPath") != path or inputs.get("familyPath") != path:
        errors.append("official case familyPath differs from its exact common50/product20 registration")
    if receipt.get("status") != "success":
        errors.append(receipt.get("failureReason") or "official execution receipt is not successful")
    if receipt.get("failureReason") not in (None, ""):
        errors.append(f"runner retained failure: {receipt['failureReason']}")
    _check_digest_bindings(registration, family_profile, inputs, receipt, errors, pilot=pilot)
    _check_path_digest(inputs.get("baseProfilePath"), registration.get("baseProfileSha256"),
                       "base profile", errors)
    _check_path_digest(inputs.get("familyProfilePath"), registration.get("familyProfileSha256"),
                       "family profile", errors)
    _check_path_digest(inputs.get("programPath"), registration.get("programSha256"),
                       "consumer executable", errors)
    _check_path_digest(inputs.get("consumerSourcePath"), registration.get("consumerSha256"),
                       "consumer source", errors)
    _check_path_digest(inputs.get("runnerPath"), registration.get("officialRunnerSha256"),
                       "official case runner", errors)
    _check_path_digest(receipt.get("seeder", {}).get("path"), inputs.get("seederSha256"),
                       "seed executable", errors)
    for row in registration.get("runtimeFiles", []):
        if isinstance(row, dict):
            _check_path_digest(row.get("path"), row.get("sha256"), "fixed runtime file", errors)
    for path_key, value, label in (("baseProfilePath", base_profile, "base profile"),
                                   ("familyProfilePath", family_profile, "family profile")):
        profile_path = inputs.get(path_key)
        try:
            if not isinstance(profile_path, str) or _read_json(Path(profile_path), label) != value:
                errors.append(f"loaded {label} object differs from its exact registered file")
        except (OSError, ValueError, TypeError) as error:
            errors.append(f"exact {label} file could not be independently checked: {error}")
    expected_source = family_profile.get("sourceProducts", {}).get(role)
    program = receipt.get("program", {})
    if (program.get("sha256") != receipt.get("programSha256")
            or program.get("sourceCommit") != expected_source
            or program.get("consumerSourceSha256") != receipt.get("consumerSha256")
            or program.get("cargoFeatures") != registration.get("cargoFeatures")):
        errors.append("raw program receipt/source/features differ from the frozen registered consumer")
    seeder = receipt.get("seeder", {})
    if seeder.get("sha256") != inputs.get("seederSha256"):
        errors.append("raw seeder executable identity differs from the recorded fixed input")
    if (receipt.get("source", {}).get("role") != role
            or receipt.get("source", {}).get("commit") != expected_source
            or receipt.get("source", {}).get("familyPath") != path):
        errors.append("raw source receipt differs from the registered role/path/revision")
    if (receipt.get("parentSessionId") != registration.get("parentSessionId")
            or receipt.get("childSessionId") != registration.get("childSessionId")
            or receipt.get("seed", {}).get("parentSessionId") != registration.get("parentSessionId")
            or receipt.get("seed", {}).get("childSessionId") != registration.get("childSessionId")):
        errors.append("official seed/receipt changed its original Native parent/child pointers")
    # Verify raw sidecar digests copied into the execution receipt. Missing
    # hash evidence fails closed; a summary success bit never substitutes for it.
    seed_receipt = receipt.get("seed", {})
    hash_bindings = (
        (seed_receipt.get("proofSha256"), "seed-proof.json", "seed-proof.json"),
        (seed_receipt.get("sourceHistorySha256"), "seed-source-history.json", "seed-source-history.json"),
        (seed_receipt.get("seedCliExitSha256"), "seed-exit.json", "seed-exit.json"),
        (receipt.get("consumerEventsSha256"), "consumer.stdout.jsonl", "consumer.stdout.jsonl"),
    )
    for expected_hash, filename, label in hash_bindings:
        if filename not in evidence:
            continue
        if not isinstance(expected_hash, str) or expected_hash != _file_sha(case / filename):
            errors.append(f"{label} raw SHA differs from its official execution receipt")
    raw_event_count = receipt.get("consumerEventCount")
    if raw_event_count is not None and raw_event_count != len(evidence.get("consumer.stdout.jsonl", [])):
        errors.append("consumer event count differs from the official execution receipt")
    for sidecar, receipt_field in (("consumer-exit.json", "consumerExit"),
                                   ("last-rss-summary.json", "rssSummary"),
                                   ("fixture-lifecycle.json", "fixtureLifecycle"),
                                   ("owned-process-retirement.json", "ownedProcessRetirement")):
        if sidecar in evidence and receipt.get(receipt_field) != evidence[sidecar]:
            errors.append(f"{sidecar} differs from its full raw official-receipt copy")
    if (receipt.get("machineBefore") != inputs.get("machineBefore")
            or receipt.get("machineAfter") != evidence.get("machine-after.json")):
        errors.append("raw before/after machine evidence differs from the full official-receipt copy")
    if receipt.get("providerObservation") != evidence.get("provider-end-receipt.json"):
        errors.append("provider end observation differs from its full raw official-receipt copy")
    if evidence.get("provider-observation.json") != evidence.get("provider-end-receipt.json"):
        errors.append("provider observation and end receipt differ")
    seed_process_receipt = receipt.get("seedConsumerExit")
    if seed_process_receipt != evidence.get("seed-exit.json"):
        errors.append("seed consumer exit differs from its full raw official-receipt copy")
    for label in ("seed-exit.json", "seed-history-exit.json", "seed-earlier-exit.json"):
        seed_exit = evidence.get(label)
        if not isinstance(seed_exit, dict) or seed_exit.get("exitStatus") != 0 \
                or seed_exit.get("ownedGroupAbsent") is not True:
            errors.append(f"original public seed process did not exit cleanly: {label}")

    source, source_entries = None, []
    if "seed-source-history.json" in evidence:
        try:
            source = evidence["seed-source-history.json"]
            if isinstance(source, dict) and isinstance(source.get("sourceHistory"), dict):
                source = source["sourceHistory"]
            normalized, source_entries = _validate_source_history(source, registration, family_profile)
            proof = evidence.get("seed-proof.json")
            if not isinstance(proof, dict):
                raise ValueError("seed-proof.json must retain normalized source content")
            supplied = proof.get("normalizedContent")
            if supplied != normalized:
                raise ValueError("normalized seed proof differs from independent full200 source reconstruction")
            semantic_sha = _sha(_canonical(normalized))
            if proof.get("semanticCorpusSha256") != semantic_sha:
                raise ValueError("semantic corpus digest differs from reconstructed source data")
            if semantic_sha != family_profile["seed"].get("semanticCorpusSha256"):
                raise ValueError("seed semantic corpus differs from the frozen cross-role profile")
            sample.update({"sourceHistory": source, "normalizedContent": normalized,
                           "semanticCorpusSha256": semantic_sha})
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"public full200 source verification failed: {error}")
    if source is not None and "consumer.stdout.jsonl" in evidence:
        try:
            sample["protocol"] = _protocol(evidence["consumer.stdout.jsonl"], registration,
                                           path, source, source_entries, mode, errors)
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"raw protocol verification failed: {error}")
    try:
        sample["lifecycle"] = _validate_lifecycle(
            case, mode, registration, receipt, errors,
            cleanup_deadline_ms=family_thresholds.get("ownedCleanupDeadlineMs", 10000))
    except (OSError, ValueError, TypeError, KeyError) as error:
        errors.append(f"process/RSS/cleanup evidence failed: {error}")
    before_provider = evidence.get("seed-provider-before-measurement.json")
    if not isinstance(before_provider, dict) or not isinstance(before_provider.get("requestCounts"), dict):
        errors.append("seed-time provider counters are missing")
    after_provider = evidence.get("provider-end-receipt.json")
    if not isinstance(before_provider, dict) or not isinstance(after_provider, dict):
        errors.append("provider start/end receipts are missing")
    elif before_provider.get("requestCounts") is None or before_provider.get("requestCounts") != after_provider.get("requestCounts"):
        errors.append("provider counters changed during the read-only Native family measurement")
    machine_before, machine_after = inputs.get("machineBefore"), evidence.get("machine-after.json")
    if not isinstance(machine_before, dict) or not isinstance(machine_after, dict):
        errors.append("raw before/after machine identity is missing")
    else:
        try:
            before_stable = _stable_machine_identity(machine_before)
            after_stable = _stable_machine_identity(machine_after)
            if machine_before.get("stableIdentity") != before_stable:
                errors.append("machine-before stable identity does not match its raw OS/hardware/power observations")
            if machine_after.get("stableIdentity") != after_stable:
                errors.append("machine-after stable identity does not match its raw OS/hardware/power observations")
            if before_stable != after_stable:
                errors.append("stable OS/hardware/power identity changed during the case")
        except (ValueError, TypeError, KeyError) as error:
            errors.append(f"raw machine identity normalization failed: {error}")
    sample["machineBefore"], sample["machineAfter"] = machine_before, machine_after
    sample["validationErrors"] = list(dict.fromkeys(errors))
    sample["pilot"] = pilot
    sample["performanceAccepted"] = False
    sample["status"] = "success" if (official or pilot) and receipt.get("status") == "success" and not errors else "failed"
    sample["failureReason"] = None if sample["status"] == "success" else (
        "; ".join(dict.fromkeys(errors)) or "original case is not a successful official sample")
    return sample


def _metric(values: list[float], *, limit: float, statistic: str = "p95") -> dict[str, Any]:
    if not values:
        return {"count": 0, "p50": None, "p95": None, "max": None,
                "absoluteLimit": limit, "absolutePass": None}
    p50, p95, maximum = nearest_rank(values, .5), nearest_rank(values, .95), max(values)
    tested = maximum if statistic == "max" else p95
    return {"count": len(values), "p50": p50, "p95": p95, "max": maximum,
            "absoluteLimit": limit, "absoluteStatistic": statistic,
            "absoluteValue": tested, "absolutePass": tested <= limit}


def _values(rows: list[dict[str, Any]], metric: str, mode: str) -> list[float]:
    rounds = MEASURED_ROUNDS[mode]
    return [value / 1_000_000 for row in rows
            for index in rounds
            for value in [row.get("protocol", {}).get("latenciesNs", {}).get(metric, {}).get(index)]
            if type(value) is int and value >= 0]


def _summary(rows: list[dict[str, Any]], path: str, mode: str,
             family_profile: dict[str, Any]) -> dict[str, Any]:
    thresholds = family_profile["thresholds"]
    cleanup = [row["lifecycle"]["cleanupElapsedNs"] / 1_000_000 for row in rows
               if type(row.get("lifecycle", {}).get("cleanupElapsedNs")) is int]
    agent_rss = [row["lifecycle"]["peaksKiB"]["agentkit"] / 1024 for row in rows
                 if type(row.get("lifecycle", {}).get("peaksKiB", {}).get("agentkit")) is int]
    harness_rss = [row["lifecycle"]["peaksKiB"]["harness"] / 1024 for row in rows
                   if type(row.get("lifecycle", {}).get("peaksKiB", {}).get("harness")) is int]
    actual50 = [value / 1_000_000 for row in rows for value in
                row.get("protocol", {}).get("latenciesNs", {}).get("actual50Earlier", {}).values()
                if type(value) is int and value >= 0]
    return {"sampleCount": len(rows), "sampleIds": [row.get("sampleId") for row in rows],
            "hasComparableBaseline": path == "common50",
            "noComparableBaselineReason": None if path == "common50" else family_profile["paths"]["product20"]["noSameEntryBaselineReason"],
            "firstUsableSnapshotMs": _metric(_values(rows, "snapshot", mode), limit=thresholds["firstUsableSnapshotP95Ms"]),
            "historyPage20ToProjectionMs": _metric(_values(rows, "page20", mode), limit=thresholds["historyPageToProjectionP95Ms"]),
            "actualChildRun50EarlierMs": {"count": len(actual50),
                "p50": nearest_rank(actual50, .5) if actual50 else None,
                "p95": nearest_rank(actual50, .95) if actual50 else None,
                "max": max(actual50) if actual50 else None,
                "budget": None},
            "explicitReconnectToConvergedMs": _metric(_values(rows, "reconnect", mode), limit=thresholds["explicitReconnectToConvergedP95Ms"]),
            "cleanupMs": _metric(cleanup, limit=thresholds["ownedCleanupDeadlineMs"], statistic="max"),
            "agentkitSampledPeakMiB": _metric(agent_rss, limit=thresholds["agentkitPeakRssMiBMax"], statistic="max"),
            "ownedHarnessSampledPeakMiB": _metric(harness_rss, limit=thresholds["ownedHarnessPeakRssMiBMax"], statistic="max")}


def aggregate_batch(batch: dict[str, Any]) -> dict[str, Any]:
    """Aggregate the exact 21-case matrix; keep all groups and failures visible."""
    family = batch.get("familyProfile") or batch.get("profile")
    base = batch.get("baseProfile")
    samples = batch.get("samples")
    _need(isinstance(family, dict) and isinstance(base, dict) and isinstance(samples, list),
          "batch requires the frozen family/base profiles and retained sample rows")
    expected = set()
    for mode, count in CASE_COUNTS.items():
        for index in range(count):
            expected.update(("common50", role, mode, index) for role in ROLES)
            expected.add(("product20", "candidate", mode, index))
    seen, failed, grouped = set(), [], {}
    for row in samples:
        _need(isinstance(row, dict), "sample row must be an object")
        key = (row.get("path"), row.get("role"), row.get("mode"), row.get("caseIndex"))
        _need(key not in seen, "duplicate sample cannot replace a preregistered case")
        seen.add(key)
        if key not in expected:
            failed.append({"sampleId": row.get("sampleId"), "reason": "sample is outside the exact frozen matrix"})
        elif row.get("status") != "success" or row.get("official") is not True or row.get("diagnosticOnly") is True:
            failed.append({"sampleId": row.get("sampleId"), "path": key[0], "role": key[1],
                           "mode": key[2], "caseIndex": key[3], "status": row.get("status"),
                           "failureReason": row.get("failureReason")})
        else:
            grouped.setdefault(key[:3], []).append(row)
    missing = []
    reports = {}
    for path, roles in (("common50", ROLES), ("product20", ("candidate",))):
        for role in roles:
            for mode, count in CASE_COUNTS.items():
                rows = sorted(grouped.get((path, role, mode), []), key=lambda row: row.get("caseIndex", -1))
                indices = list(range(count)) if mode == "cold" else [0]
                actual = [row.get("caseIndex") for row in rows]
                if actual != indices:
                    missing.append({"path": path, "role": role, "mode": mode,
                                    "expectedCaseIndices": indices, "actualCaseIndices": actual,
                                    "missingCaseIndices": [value for value in indices if value not in actual]})
                if rows:
                    signatures = {(row.get("semanticCorpusSha256"), row.get("runtimeSha256"),
                                   _canonical(row.get("machineBefore", {}).get("stableIdentity"))) for row in rows}
                    if len(signatures) != 1:
                        failed.append({"path": path, "role": role, "mode": mode,
                                       "reason": "cases do not share exact seed/runtime/machine identity"})
                    reports[f"{path}/{role}:{mode}"] = _summary(rows, path, mode, family)
    # Paired common50 roles must share the exact workload and observation
    # implementation on one stable machine/runtime; source product revisions
    # differ intentionally.
    for mode, count in CASE_COUNTS.items():
        base_rows = {row.get("caseIndex"): row for row in grouped.get(("common50", "baseline", mode), [])}
        candidate_rows = {row.get("caseIndex"): row for row in grouped.get(("common50", "candidate", mode), [])}
        for index in sorted(set(base_rows) & set(candidate_rows)):
            left, right = base_rows[index], candidate_rows[index]
            same_inputs = (
                left.get("semanticCorpusSha256") == right.get("semanticCorpusSha256")
                and left.get("runtimeSha256") == right.get("runtimeSha256")
                and isinstance(left.get("observerSha256"), str)
                and left.get("observerSha256") == right.get("observerSha256")
                and _canonical(left.get("machineBefore", {}).get("stableIdentity"))
                == _canonical(right.get("machineBefore", {}).get("stableIdentity"))
            )
            if not same_inputs:
                failed.append({"path": "common50", "mode": mode, "caseIndex": index,
                               "reason": "paired roles differ in observer, corpus, runtime, or stable machine identity"})
    ratios, budget_failures = [], []
    relative_limit = family["thresholds"]["latencyRegressionRatioMax"]
    rss_limit = family["thresholds"]["peakRssRegressionRatioMax"]
    for mode in MODES:
        baseline = {row.get("caseIndex"): row for row in grouped.get(("common50", "baseline", mode), [])}
        candidate = {row.get("caseIndex"): row for row in grouped.get(("common50", "candidate", mode), [])}
        common = sorted(set(baseline) & set(candidate))
        relative = {}
        for metric in ("snapshot", "page20", "reconnect"):
            bvals = _values([baseline[i] for i in common], metric, mode)
            cvals = _values([candidate[i] for i in common], metric, mode)
            if bvals and cvals:
                bstat, cstat = nearest_rank(bvals, .95), nearest_rank(cvals, .95)
                ratio = cstat / bstat if bstat > 0 else None
                passed = ratio is not None and ratio <= relative_limit
                relative[metric] = {"baselineP95": bstat, "candidateP95": cstat,
                                    "ratio": ratio, "limit": relative_limit, "pass": passed}
                ratios.append(passed)
        for metric, limit in (("cleanupMs", relative_limit), ("agentkitSampledPeakMiB", rss_limit),
                              ("ownedHarnessSampledPeakMiB", rss_limit)):
            bvals, cvals = [], []
            for index in common:
                b, c = baseline[index], candidate[index]
                if metric == "cleanupMs":
                    bv, cv = b["lifecycle"]["cleanupElapsedNs"] / 1_000_000, c["lifecycle"]["cleanupElapsedNs"] / 1_000_000
                else:
                    resource = "agentkit" if metric.startswith("agentkit") else "harness"
                    bv = b["lifecycle"]["peaksKiB"][resource] / 1024
                    cv = c["lifecycle"]["peaksKiB"][resource] / 1024
                bvals.append(bv)
                cvals.append(cv)
            if bvals and cvals:
                bstat, cstat = max(bvals), max(cvals)
                ratio = cstat / bstat if bstat > 0 else None
                passed = ratio is not None and ratio <= limit
                relative[metric] = {"baselineMax": bstat, "candidateMax": cstat,
                                    "ratio": ratio, "limit": limit, "pass": passed}
                ratios.append(passed)
        if relative:
            reports.get(f"common50/candidate:{mode}", {}).update({"relativeComparisons": relative})
    for group, report in reports.items():
        role = group.split("/", 1)[1].split(":", 1)[0]
        path_mode = group.rsplit(":", 1)[1]
        expected_cases = CASE_COUNTS[path_mode]
        expected_round_metrics = expected_cases * len(MEASURED_ROUNDS[path_mode])
        for metric, expected_count in (
            ("firstUsableSnapshotMs", expected_round_metrics),
            ("historyPage20ToProjectionMs", expected_round_metrics),
            ("actualChildRun50EarlierMs",
             expected_round_metrics if group.startswith("common50/") else 0),
            ("explicitReconnectToConvergedMs",
             expected_round_metrics if path_mode == "reconnectLifecycle" else 0),
            ("cleanupMs", expected_cases),
            ("agentkitSampledPeakMiB", expected_cases),
            ("ownedHarnessSampledPeakMiB", expected_cases),
        ):
            actual = report[metric]["count"]
            if actual != expected_count:
                budget_failures.append({"group": group, "metric": metric,
                                        "reason": "metric evidence count is incomplete",
                                        "actualCount": actual, "expectedCount": expected_count})
        for metric in ("firstUsableSnapshotMs", "historyPage20ToProjectionMs",
                       "explicitReconnectToConvergedMs", "cleanupMs",
                       "agentkitSampledPeakMiB", "ownedHarnessSampledPeakMiB"):
            if role == "candidate" and report[metric]["absolutePass"] is False:
                budget_failures.append({"group": group, "metric": metric,
                                        "value": report[metric].get("absoluteValue"),
                                        "limit": report[metric].get("absoluteLimit")})
        for metric, comparison in report.get("relativeComparisons", {}).items():
            if comparison.get("pass") is False:
                budget_failures.append({"group": group, "metric": metric, "reason": "relative budget exceeded"})
    all_present = seen == expected
    accepted = all_present and not missing and not failed and not budget_failures and bool(ratios) and all(ratios)
    return {"format": 1, "issue": 56, "endpoint": "native", "plannedCaseCount": 21,
            "retainedCaseCount": len(samples),
            "officialCaseCount": sum(row.get("official") is True for row in samples),
            "allPreregisteredCasesPresent": all_present, "missingCaseGroups": missing,
            "failedCases": failed, "trajectories": reports,
            "absoluteBudgetFailures": budget_failures,
            "relativeComparisonsPass": all(ratios) if ratios else None,
            "nativeFamilyPerformanceAccepted": accepted,
            "performanceAccepted": False, "issue56Accepted": False,
            "scope": {"pathsAggregatedSeparately": True,
                      "common50HasComparableBaseline": True,
                      "product20SameEntryBaseline": False,
                      "product20NoComparableBaselineReason": family["paths"]["product20"]["noSameEntryBaselineReason"]}}
