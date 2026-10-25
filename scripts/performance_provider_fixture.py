"""Deterministic provider and public-command helpers for the frozen perf profile.

This module never reads or writes AgentKit storage. It emits real advertised
tool calls and waits for the harness to return the matching tool output before
finishing the turn. Request statistics deliberately retain digests and public
identifiers only, never provider bodies or tool output text.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import hashlib
import http.server
import json
from pathlib import Path
import re
import sys
import threading
from typing import Any, Iterable


_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from native_dsh_mixed_fixture import classify_dsh_request, strip_dsh_context_suffixes


BODY_BYTES = 1024
SEED_TURNS = 50
SEED_TOOL_TURNS = 10
SESSION_COUNT = 2
_HEADER_RE = re.compile(r"^AKPERF\|(seed|measure)\|s([01])\|t([0-9]{2})\|(user|assistant)\|")
_CALL_ID_RE = re.compile(r"^akperf-seed-s([01])-t([0-9]{2})-read$")
_USAGE = {"input_tokens": 2048, "output_tokens": 256, "total_tokens": 2304}
_READ_SOURCE = (b"AKPERF_PUBLIC_READ_SOURCE|fixture-v1\n"
                b"line-01|public read result source\n"
                b"line-02|deterministic fixture bytes\n"
                b"line-03|tool output must come from the harness\n")


def _validate_coordinates(phase: str, session_index: int, turn_index: int, role: str) -> None:
    if phase not in ("seed", "measure"):
        raise ValueError("phase must be seed or measure")
    if type(session_index) is not int or session_index not in (0, 1):
        raise ValueError("session_index must be 0 or 1")
    minimum_turn = 1 if phase == "seed" else 0
    if type(turn_index) is not int or not minimum_turn <= turn_index <= (SEED_TURNS if phase == "seed" else 10):
        raise ValueError("turn_index is outside the frozen profile")
    if role not in ("user", "assistant"):
        raise ValueError("role must be user or assistant")


def body_bytes(phase: str, session_index: int, turn_index: int, role: str) -> bytes:
    """Return a stable UTF-8 body whose complete encoded form is exactly 1024 bytes."""
    _validate_coordinates(phase, session_index, turn_index, role)
    header = f"AKPERF|{phase}|s{session_index}|t{turn_index:02d}|{role}|"
    body = (header + "会话性能固定正文；保留公开历史内容与 UTF-8 字节计数。|").encode("utf-8")
    if len(body) > BODY_BYTES:
        raise AssertionError("fixed performance body prefix exceeds byte budget")
    return body + (b"x" * (BODY_BYTES - len(body)))


def body_text(phase: str, session_index: int, turn_index: int, role: str) -> str:
    """Text form of :func:`body_bytes`, for public composer commands."""
    return body_bytes(phase, session_index, turn_index, role).decode("utf-8")


def seed_commands(session_id: str, provider: str, model: str, session_index: int) -> list[dict[str, Any]]:
    """Build the public select/open plus 50 compose/settle pairs for one existing session."""
    if not isinstance(session_id, str) or not session_id:
        raise ValueError("session_id must be non-empty")
    if not isinstance(provider, str) or not provider or not isinstance(model, str) or not model:
        raise ValueError("provider and model must be non-empty")
    if type(session_index) is not int or session_index not in (0, 1):
        raise ValueError("session_index must be 0 or 1")
    commands: list[dict[str, Any]] = [
        {"op": "call", "sessionId": session_id, "method": "session.selectModel",
         "args": {"provider": provider, "model": model}},
        {"op": "open", "sessionId": session_id},
    ]
    for turn in range(1, SEED_TURNS + 1):
        text = body_text("seed", session_index, turn, "user")
        commands.extend((
            {"op": "compose", "sessionId": session_id, "text": text},
            {"op": "waitSettled", "sessionId": session_id, "inputText": text},
        ))
    return commands


def measure_commands(session_id: str, session_index: int, round_index: int) -> list[dict[str, Any]]:
    """Build one warm send; round 0 is excluded initialization, rounds 1..10 measured."""
    if not isinstance(session_id, str) or not session_id:
        raise ValueError("session_id must be non-empty")
    text = body_text("measure", session_index, round_index, "user")
    return [
        {"op": "compose", "sessionId": session_id, "text": text},
        {"op": "waitSettled", "sessionId": session_id, "inputText": text},
    ]


def read_source_path(workspace: str | Path) -> Path:
    """Return the stable public workspace path the actual read tool should access."""
    return Path(workspace).resolve() / "performance-read-proof.txt"


def read_source_bytes() -> bytes:
    """Return bytes for the wrapper to write before starting the public seeder."""
    return _READ_SOURCE


def _body_digest(value: str | bytes) -> str:
    raw = value if isinstance(value, bytes) else value.encode("utf-8")
    return hashlib.sha256(raw).hexdigest()


def content_digest_manifest(
    session_index: int,
    phase: str = "seed",
    rounds: int | None = None,
    *,
    first_turn: int = 1,
) -> list[dict[str, Any]]:
    """Produce a digest-only expected manifest for a normalized public readback."""
    count = SEED_TURNS if rounds is None and phase == "seed" else rounds
    if count is None:
        raise ValueError("rounds is required for measure phase")
    if (type(count) is not int or type(first_turn) is not int or count < 1
            or first_turn < (1 if phase == "seed" else 0)
            or first_turn + count - 1 > (SEED_TURNS if phase == "seed" else 10)):
        raise ValueError("manifest round count is outside the frozen profile")
    rows: list[dict[str, Any]] = []
    for turn in range(first_turn, first_turn + count):
        for role in ("user", "assistant"):
            value = body_bytes(phase, session_index, turn, role)
            rows.append({"role": role, "turn": turn, "byteCount": len(value), "sha256": _body_digest(value),
                         "isFinal": role == "assistant"})
    return rows


def expected_tool_call_id(session_index: int, turn_index: int) -> str:
    _validate_coordinates("seed", session_index, turn_index, "user")
    if turn_index > SEED_TOOL_TURNS:
        raise ValueError("only the first ten seed turns have tool calls")
    return f"akperf-seed-s{session_index}-t{turn_index:02d}-read"


def validate_history(
    rows: Iterable[dict[str, Any]],
    tool_groups: Iterable[dict[str, Any]],
    session_index: int,
    *,
    phase: str = "seed",
    rounds: int | None = None,
) -> dict[str, Any]:
    """Validate normalized *publicly read* transcript rows and separate tool groups.

    `rows` must already have been collected through the public history paging API
    (including all earlier pages), then normalized to `{role, text, isFinal}`.
    Tool records are separately normalized to `{turn, toolName, callId,
    resultCallId, resultText}`. The returned receipt contains no body or tool
    output text, only digests and public call identifiers.
    """
    manifest = content_digest_manifest(session_index, phase, rounds)
    normalized_rows = list(rows)
    if len(normalized_rows) != len(manifest):
        raise ValueError("public history row count differs from the complete expected manifest")
    digests: list[dict[str, Any]] = []
    for index, (row, expected) in enumerate(zip(normalized_rows, manifest)):
        if not isinstance(row, dict) or not {"role", "text"} <= set(row):
            raise ValueError("public history row lacks normalized role/text")
        text = row["text"]
        if not isinstance(text, str):
            raise ValueError("public history body must be text")
        digest = _body_digest(text)
        if (row.get("role") != expected["role"] or len(text.encode("utf-8")) != BODY_BYTES
                or digest != expected["sha256"]):
            raise ValueError(f"public history content differs at row {index}")
        if expected["role"] == "assistant" and row.get("isFinal") is not True:
            raise ValueError(f"assistant body is not final at row {index}")
        digests.append({"role": expected["role"], "turn": expected["turn"],
                        "byteCount": BODY_BYTES, "sha256": digest,
                        "isFinal": expected["isFinal"]})

    groups = list(tool_groups)
    expected_group_count = SEED_TOOL_TURNS if phase == "seed" else 0
    if len(groups) != expected_group_count:
        raise ValueError("public tool group count differs from the frozen profile")
    validated_groups: list[dict[str, Any]] = []
    for turn, group in enumerate(groups, 1):
        if not isinstance(group, dict):
            raise ValueError("public tool group must be an object")
        expected_id = expected_tool_call_id(session_index, turn)
        tool_name = group.get("toolName")
        call_id = group.get("callId")
        result_call_id = group.get("resultCallId")
        result_text = group.get("resultText")
        if (group.get("turn") not in (None, turn)
                or tool_name not in ("read", "str_replace_editor") or call_id != expected_id
                or result_call_id != expected_id or not isinstance(result_text, str) or not result_text):
            raise ValueError("public tool call/result pair is missing, reordered, or not an original read")
        validated_groups.append({"turn": turn, "toolName": tool_name, "callId": call_id,
                                 "resultCallId": result_call_id, "resultSha256": _body_digest(result_text)})
    return {"phase": phase, "sessionIndex": session_index, "bodyMessageCount": len(digests),
            "toolGroupCount": len(validated_groups), "bodyRows": digests, "toolGroups": validated_groups}


@dataclass
class RequestStats:
    """Thread-safe privacy-preserving provider counters and request receipts."""

    lock: threading.Lock = field(default_factory=threading.Lock, repr=False)
    business_requests: int = 0
    title_requests: int = 0
    tool_calls: int = 0
    tool_results: int = 0
    requests: list[dict[str, Any]] = field(default_factory=list)
    error_responses: list[dict[str, Any]] = field(default_factory=list)

    def record(self, receipt: dict[str, Any], *, title: bool = False) -> None:
        with self.lock:
            if title:
                self.title_requests += 1
            else:
                self.business_requests += 1
                self.tool_calls += len(receipt.get("callIds", []))
                self.tool_results += len(receipt.get("resultCallIds", []))
            self.requests.append(dict(receipt, title=title))

    def record_error(self, status: int, route: str) -> None:
        with self.lock:
            self.error_responses.append({'status':status,
                'route':route if route in ('/v1/responses','/v1/chat/completions') else '<unknown>'})

    def snapshot(self) -> dict[str, Any]:
        with self.lock:
            return {"businessRequests": self.business_requests, "titleRequests": self.title_requests,
                    "toolCalls": self.tool_calls, "toolResults": self.tool_results,
                    "usageIsFixedFixtureValue": True, "usageIsNotModelMeasurement": True,
                    "errorResponses": list(self.error_responses),
                    "requests": [dict(row) for row in self.requests]}


def _content_text(content: Any) -> str | None:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        blocks = [block.get("text") for block in content if isinstance(block, dict)
                  and block.get("type") in ("text", "input_text", "output_text")
                  and isinstance(block.get("text"), str)]
        return "".join(blocks) if blocks else None
    return None


def _history(body: dict[str, Any], responses: bool) -> list[dict[str, Any]]:
    key = "input" if responses else "messages"
    value = body.get(key)
    if not isinstance(value, list) or not value:
        raise ValueError("provider history is missing")
    if any(not isinstance(item, dict) for item in value):
        raise ValueError("provider history contains a non-object item")
    return value


def _latest_prompt(body: dict[str, Any], responses: bool, endpoint: str) -> tuple[str, dict[str, int | str]]:
    history = _history(body, responses)
    users = [item for item in history if item.get("role") == "user"]
    if endpoint == "dsh":
        users, _ = strip_dsh_context_suffixes(users)
    if not users:
        raise ValueError("provider history has no human input")
    prompt = _content_text(users[-1].get("content"))
    if prompt is None:
        raise ValueError("latest human input is not text")
    match = _HEADER_RE.match(prompt)
    if not match:
        raise ValueError("unknown performance fixture input")
    phase, session_index, turn_index, role = match.groups()
    session_index, turn_index = int(session_index), int(turn_index)
    expected = body_text(phase, session_index, turn_index, role)
    if prompt != expected or role != "user":
        raise ValueError("performance marker does not match a canonical user body")
    return prompt, {"phase": phase, "sessionIndex": session_index, "turn": turn_index}


def _tool_results(history: list[dict[str, Any]], responses: bool) -> list[tuple[str, str]]:
    found = []
    for item in history:
        if responses and item.get("type") == "function_call_output":
            call_id, output = item.get("call_id"), _content_text(item.get("output"))
        elif not responses and item.get("role") == "tool":
            call_id, output = item.get("tool_call_id"), _content_text(item.get("content"))
        else:
            continue
        if not isinstance(call_id, str) or output is None or not output:
            raise ValueError("tool result is missing its original id or nonempty output")
        found.append((call_id, output))
    return found


def _tool_call(history: list[dict[str, Any]], responses: bool, call_id: str) -> bool:
    for item in history:
        if responses and item.get("type") == "function_call" and item.get("call_id") == call_id:
            return True
        if not responses and item.get("role") == "assistant":
            for call in item.get("tool_calls", []) if isinstance(item.get("tool_calls"), list) else []:
                if isinstance(call, dict) and call.get("id") == call_id:
                    return True
    return False


def _advertised_read_tool(body: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    tools = body.get("tools")
    if not isinstance(tools, list):
        raise ValueError("first ten seed turns require an advertised public read tool")
    candidates: list[tuple[str, dict[str, Any]]] = []
    for tool in tools:
        if not isinstance(tool, dict):
            continue
        value = tool.get("function", tool)
        if isinstance(value, dict) and value.get("name") in ("read", "str_replace_editor"):
            candidates.append((value["name"], value))
    if not candidates:
        raise ValueError("first ten seed turns require an advertised public read tool")
    name, schema = next((row for row in candidates if row[0] == "read"), candidates[0])
    parameters = schema.get("parameters")
    properties = parameters.get("properties") if isinstance(parameters, dict) else None
    if not isinstance(properties, dict):
        raise ValueError("advertised read tool must define parameter properties")
    path_key = "file_path" if "file_path" in properties else "path" if "path" in properties else None
    if path_key is None:
        raise ValueError("advertised read tool must define file_path or path")
    return name, schema


def _read_arguments(name: str, schema: dict[str, Any], path: Path) -> dict[str, Any]:
    parameters = schema["parameters"]
    properties = parameters["properties"]
    path_key = "file_path" if "file_path" in properties else "path"
    arguments: dict[str, Any] = {path_key: str(path)}
    if name == "str_replace_editor":
        arguments["command"] = "view"
    required = parameters.get("required", [])
    if not isinstance(required, list):
        raise ValueError("advertised read tool required parameters must be an array")
    for key in required:
        if key in arguments:
            continue
        if key == "offset":
            arguments[key] = 1
        elif key == "limit":
            arguments[key] = len(_READ_SOURCE.splitlines())
        else:
            raise ValueError("advertised read tool has an unsupported required argument")
    return arguments


def _frames(body: dict[str, Any], *, responses: bool, endpoint: str, workspace: str | Path) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    prompt, marker = _latest_prompt(body, responses, endpoint)
    history = _history(body, responses)
    phase, session_index, turn = marker["phase"], marker["sessionIndex"], marker["turn"]
    call_id = expected_tool_call_id(session_index, turn) if phase == "seed" and turn <= SEED_TOOL_TURNS else None
    result_rows = _tool_results(history, responses)
    matching_results = [(cid, text) for cid, text in result_rows if cid == call_id] if call_id else []
    calls: list[dict[str, Any]] = []
    if call_id and not matching_results:
        if _tool_call(history, responses, call_id):
            raise ValueError("original read call has no returned public tool result")
        name, schema = _advertised_read_tool(body)
        arguments = _read_arguments(name, schema, read_source_path(workspace))
        calls.append({"id": call_id, "name": name, "arguments": arguments})
    elif call_id:
        if len(matching_results) != 1 or not _tool_call(history, responses, call_id):
            raise ValueError("public read result does not pair with exactly one original tool call")
    assistant_text = body_text(phase, session_index, turn, "assistant")
    request_receipt = {
        "pathKind": "responses" if responses else "chat",
        "phase": phase,
        "sessionIndex": session_index,
        "turn": turn,
        "roles": [item.get("role", item.get("type")) for item in history if item.get("role") or item.get("type")],
        "bodyMarkers": [marker],
        "bodySha256": [_body_digest(prompt), _body_digest(assistant_text)],
        "callIds": [row["id"] for row in calls],
        "resultCallIds": [row[0] for row in matching_results],
        "resultSha256": [_body_digest(row[1]) for row in matching_results],
    }
    if responses:
        return _responses_frames(calls, assistant_text, marker), request_receipt
    return _chat_frames(calls, assistant_text, marker), request_receipt


def _responses_frames(calls: list[dict[str, Any]], text: str, marker: dict[str, Any]) -> list[dict[str, Any]]:
    response_id = f"akperf-{marker['phase']}-s{marker['sessionIndex']}-t{marker['turn']:02d}"
    frames: list[dict[str, Any]] = [{"type": "response.created", "response": {
        "id": response_id, "status": "in_progress", "output": []}}]
    completed: list[dict[str, Any]] = []
    if calls:
        for index, call in enumerate(calls):
            arguments = json.dumps(call["arguments"], ensure_ascii=False, separators=(",", ":"))
            frames.extend((
                {"type": "response.output_item.added", "output_index": index, "item": {
                    "id": call["id"], "type": "function_call", "call_id": call["id"],
                    "name": call["name"], "arguments": ""}},
                {"type": "response.function_call_arguments.delta", "output_index": index,
                 "delta": arguments},
                {"type": "response.function_call_arguments.done", "output_index": index,
                 "arguments": arguments},
            ))
            completed.append({"id": call["id"], "type": "function_call", "call_id": call["id"],
                              "name": call["name"], "arguments": arguments, "status": "completed"})
    else:
        item_id = response_id + "-message"
        frames.extend((
            {"type": "response.output_item.added", "output_index": 0, "item": {
                "id": item_id, "type": "message", "role": "assistant", "phase": "final_answer",
                "status": "in_progress", "content": []}},
            {"type": "response.output_text.delta", "output_index": 0, "content_index": 0, "delta": text},
            {"type": "response.output_text.done", "output_index": 0, "content_index": 0, "text": text},
        ))
        completed.append({"id": item_id, "type": "message", "role": "assistant", "phase": "final_answer",
                          "status": "completed", "content": [{"type": "output_text", "text": text}]})
    frames.append({"type": "response.completed", "response": {
        "id": response_id, "status": "completed", "output": completed, "usage": dict(_USAGE)}})
    return frames


def _chat_frames(calls: list[dict[str, Any]], text: str, marker: dict[str, Any]) -> list[dict[str, Any]]:
    response_id = f"akperf-{marker['phase']}-s{marker['sessionIndex']}-t{marker['turn']:02d}"
    common = {"id": response_id, "object": "chat.completion.chunk", "created": 1,
              "model": "agentkit-performance-fixture"}
    frames: list[dict[str, Any]] = []
    if calls:
        for index, call in enumerate(calls):
            frames.append(dict(common, choices=[{"index": 0, "delta": {
                "role": "assistant" if index == 0 else None,
                "tool_calls": [{"index": index, "id": call["id"], "type": "function",
                                "function": {"name": call["name"], "arguments": json.dumps(
                                    call["arguments"], ensure_ascii=False, separators=(",", ":"))}}]},
                "finish_reason": None}]))
        finish = "tool_calls"
    else:
        frames.append(dict(common, choices=[{"index": 0, "delta": {"role": "assistant", "content": text},
                                              "finish_reason": None}]))
        finish = "stop"
    frames.append(dict(common, choices=[{"index": 0, "delta": {}, "finish_reason": finish}],
                       usage={"prompt_tokens": _USAGE["input_tokens"],
                              "completion_tokens": _USAGE["output_tokens"],
                              "total_tokens": _USAGE["total_tokens"]}))
    return frames


def _sse_payload(frames: list[dict[str, Any]], *, responses: bool) -> bytes:
    body = b"".join(("data: " + json.dumps(frame, ensure_ascii=False, separators=(",", ":")) + "\n\n").encode("utf-8")
                    for frame in frames)
    if not responses:
        body += b"data: [DONE]\n\n"
    return body


def provider_frames(
    body: dict[str, Any],
    *,
    path_kind: str,
    workspace: str | Path,
    endpoint: str = "native",
) -> tuple[list[dict[str, Any]], dict[str, Any], bool]:
    """Pure provider decision function; returns frames, digest receipt, title flag."""
    if not isinstance(body, dict):
        raise ValueError("provider request must be an object")
    if path_kind not in ("responses", "chat"):
        raise ValueError("path_kind must be responses or chat")
    if endpoint not in ("native", "pi", "dsh"):
        raise ValueError("endpoint must be native, pi, or dsh")
    if endpoint == "dsh":
        request_kind, title_text = classify_dsh_request(body)
        if request_kind == "title":
            if path_kind != "chat" or not isinstance(title_text, str):
                raise ValueError("DSH title route must be a classified chat request")
            receipt = {"pathKind": "chat", "titleInputSha256": _body_digest(title_text),
                       "roles": [item.get("role") for item in body.get("messages", [])
                                 if isinstance(item, dict)]}
            return _chat_title_frames(), receipt, True
    frames, receipt = _frames(body, responses=path_kind == "responses", endpoint=endpoint,
                              workspace=workspace)
    return frames, receipt, False


def create_provider_handler(workspace: str | Path, stats: RequestStats | None = None, *, endpoint: str = "native"):
    """Create a stdlib HTTP handler for `/v1/responses` and `/v1/chat/completions`.

    `endpoint` may be native, pi, or dsh. DSH title routing uses the existing
    strict classifier before any ordinary prompt parsing; title calls are
    counted separately and excluded from business request counters.
    """
    if endpoint not in ("native", "pi", "dsh"):
        raise ValueError("endpoint must be native, pi, or dsh")
    request_stats = stats if stats is not None else RequestStats()
    issued = {}
    issued_lock = threading.Lock()

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args: Any) -> None:
            return

        def _respond(self, status: int, content_type: str, payload: bytes) -> None:
            if status >= 400:
                request_stats.record_error(status,self.path)
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:
            if self.path not in ("/v1/responses", "/v1/chat/completions"):
                self._respond(404, "application/json", b'{"error":{"message":"unknown fixture route"}}')
                return
            try:
                size = int(self.headers.get("Content-Length", "0"))
                if not 0 < size <= 8 * 1024 * 1024:
                    raise ValueError("request size outside fixture bound")
                raw = self.rfile.read(size)
                body = json.loads(raw)
                if not isinstance(body, dict):
                    raise ValueError("provider request must be an object")
                responses = self.path.endswith("/responses")
                original_body = body
                # Responses server-chain continuation can contain only the
                # actual tool result. Restore only our own issued context,
                # never infer an arbitrary prompt from an untrusted call ID.
                if (endpoint == 'native' and responses and body.get('previous_response_id')
                        and not any(item.get('role') == 'user' for item in _history(body, True))):
                    with issued_lock:
                        prior = issued.get(body['previous_response_id'])
                    if prior is None:
                        raise ValueError('unknown fixture previous_response_id')
                    actual = _history(body, True)
                    if (len(actual) != 1 or actual[0].get('type') != 'function_call_output'
                            or actual[0].get('call_id') != prior['call']['call_id']):
                        shape = [{'type': item.get('type'), 'role': item.get('role'),
                                  'call_id': item.get('call_id')} for item in actual]
                        raise ValueError('server-chain continuation differs from its issued read call: '
                                         + json.dumps(shape, separators=(',', ':')))
                    body = dict(body, input=[prior['user'], prior['call'], *actual])
                frames, receipt, title = provider_frames(
                    body, path_kind="responses" if responses else "chat", workspace=workspace, endpoint=endpoint)
                receipt['serverChainContextRestored'] = original_body is not body
                if responses:
                    completed = frames[-1].get('response', {})
                    calls = [item for item in completed.get('output', [])
                             if item.get('type') == 'function_call']
                    if calls:
                        users = [item for item in _history(body, True) if item.get('role') == 'user']
                        with issued_lock:
                            issued[completed['id']] = {'user': users[-1], 'call': calls[0]}
                # Do not retain the original provider request or its output text.
                request_stats.record(receipt, title=title)
                self._respond(200, "text/event-stream", _sse_payload(frames, responses=responses))
            except (ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
                message = str(error).replace('"', "'")[:200]
                # Fixture errors are static parser diagnostics; retain their
                # exact message so a failed pilot can be diagnosed offline.
                print('performance provider rejected: '+message, file=sys.stderr, flush=True)
                payload = json.dumps({"error": {"type": "invalid_request_error", "message": message}},
                                     separators=(",", ":")).encode("utf-8")
                self._respond(400, "application/json", payload)

    return Provider


def _chat_title_frames() -> list[dict[str, Any]]:
    common = {"id": "akperf-dsh-title", "object": "chat.completion.chunk", "created": 1,
              "model": "agentkit-performance-fixture"}
    return [
        dict(common, choices=[{"index": 0, "delta": {"role": "assistant", "content": "Performance"},
                               "finish_reason": None}]),
        dict(common, choices=[{"index": 0, "delta": {"content": " workload"}, "finish_reason": None}]),
        dict(common, choices=[{"index": 0, "delta": {}, "finish_reason": "stop"}]),
    ]
