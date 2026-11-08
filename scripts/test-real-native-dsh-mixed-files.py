#!/usr/bin/env python3
"""Verify Native and DSH public image/file/image sends through owned headless fixtures.

The runner consumes existing executables only. It does not invoke Cargo, launch a
desktop, inspect harness history, or contact a real model/provider.
"""
from __future__ import annotations

import argparse
from contextlib import ExitStack, suppress
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import threading
import time
import uuid

from dsh_owned_fixture import owned_dsh
from inline_image_live_fixture import stream_response
from native_dsh_mixed_fixture import (
    TEXT_PROMPT,
    BUSY_HOLD_TEXT,
    busy_hold_stream_frames,
    classify_dsh_request,
    strip_dsh_context_suffixes,
    strip_provider_hold_request,
    inspect_provider_request,
    record_imageio_evidence,
    validate_probe_receipt,
    validate_busy_queue_ready,
)
from native_owned_fixture import owned_native
from runtime_provenance import kernel_programs


SSD_PROJECT = Path("/Volumes/HP P900/dev/TekesAgentKit")
NATIVE_AUTH = "AGENTKIT_NATIVE_DSH_MIXED_NATIVE_BEARER"
DSH_AUTH = "AGENTKIT_NATIVE_DSH_MIXED_DSH_COOKIE"
EXPECTED_DSH_VERSION = "0.2.1-rc.2"
HEADLESS_TIMEOUT = 90
PROBE_TIMEOUT = 300
MAX_REQUEST_BYTES = 64 * 1024 * 1024
BUSY_READY_PREFIX = "AGENTKIT_NATIVE_DSH_BUSY_QUEUE_READY "
BUSY_READY_TIMEOUT = 90


def write_json(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2)
        stream.write("\n")


def write_text(path: Path, value: str) -> None:
    with path.open("x", encoding="utf-8") as stream:
        stream.write(value)


def digest_file(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def executable(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve(strict=True)
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise ValueError(label + " must be an existing executable regular file")
    return resolved


def terminate_owned_group(process: subprocess.Popen) -> None:
    """Terminate only a subprocess session this runner created."""
    signalled = False
    with suppress(ProcessLookupError):
        os.killpg(process.pid, signal.SIGTERM)
        signalled = True
    if process.poll() is None:
        with suppress(subprocess.TimeoutExpired):
            process.wait(timeout=4)
    if signalled:
        with suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)
    if process.poll() is None:
        with suppress(subprocess.TimeoutExpired):
            process.wait(timeout=4)


def group_exited(pid: int, timeout: float = 3.0) -> bool:
    import time
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        try:
            os.killpg(pid, 0)
        except ProcessLookupError:
            return True
        except PermissionError:
            # Ambiguous during process teardown: retain the bounded grace
            # period and fail closed if it remains unresolved at the deadline.
            pass
        time.sleep(0.05)
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return True
    return False


def run_headless(program: Path, config: dict, commands: list[dict], output: Path,
                 environment: dict[str, str], workspace: Path, label: str) -> list[dict]:
    command_path = output / (label + "-commands.json")
    write_json(command_path, commands)
    child = subprocess.Popen(
        [str(program)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace",
        env=environment, cwd=workspace, close_fds=True, start_new_session=True)
    try:
        stdout, stderr = child.communicate(
            input=json.dumps(dict(config, commands=commands), ensure_ascii=False),
            timeout=HEADLESS_TIMEOUT)
    except subprocess.TimeoutExpired:
        terminate_owned_group(child)
        stdout, stderr = child.communicate()
        write_text(output / (label + ".stdout.log"), stdout)
        write_text(output / (label + ".stderr.log"), stderr)
        write_json(output / (label + ".exit.json"), {"timedOut": True, "pid": child.pid})
        raise RuntimeError(label + " public session creation timed out; logs retained") from None
    write_text(output / (label + ".stdout.log"), stdout)
    write_text(output / (label + ".stderr.log"), stderr)
    leaked = not group_exited(child.pid)
    if leaked:
        terminate_owned_group(child)
    write_json(output / (label + ".exit.json"), {
        "exitStatus": child.returncode, "pid": child.pid,
        "processGroupExited": not leaked,
    })
    if leaked:
        raise RuntimeError(label + " left an owned process group; it was terminated")
    if child.returncode:
        raise RuntimeError(label + " public session creation failed; logs retained")
    try:
        rows = [json.loads(line) for line in stdout.splitlines() if line.strip()]
    except json.JSONDecodeError:
        raise ValueError(label + " output contains a non-JSON line") from None
    if len(rows) != len(commands) + 1 or rows[0].get("ready") is not True:
        raise ValueError(label + " public handshake or command count differs")
    version = config.get("expectedVersion")
    if version is not None and rows[0].get("version") != version:
        raise ValueError(label + " public runtime version differs")
    if any(row.get("op") != command.get("op") or "result" not in row
           for command, row in zip(commands, rows[1:])):
        raise ValueError(label + " public command/result pairing differs")
    return rows


def seed_endpoint(kind: str, config: dict, provider_id: str, model_id: str,
                  program: Path, env: dict[str, str], workspace: Path, case: Path) -> tuple[dict, list[str]]:
    provided_ids = [str(uuid.uuid4()), str(uuid.uuid4())] if kind == "native" else None
    commands = []
    for index in range(2):
        alias = "mixed-" + str(index)
        if kind == "native":
            commands.extend([
                {"op": "call", "method": "session.create", "args": {
                    "sessionId": provided_ids[index],
                    "execution": {"cwd": str(workspace), "read_roots": [],
                        "writable_roots": [], "toolchain_roots": [], "network": False,
                        "allowed_tools": [], "mcp": []}}, "saveAs": alias},
                {"op": "call", "sessionId": alias, "method": "session.selectModel",
                 "args": {"provider": provider_id, "model": model_id}},
                {"op": "open", "sessionId": alias},
            ])
        else:
            commands.extend([
                {"op": "create", "cwd": str(workspace), "saveAs": alias},
                {"op": "call", "sessionId": alias, "method": "session.selectModel",
                 "args": {"provider": provider_id, "model": model_id}},
                {"op": "open", "sessionId": alias},
            ])
    rows = run_headless(program, config, commands, case, env, workspace, "seed")
    ids = []
    for command, row in zip(commands, rows[1:]):
        if command.get("method") == "session.create" or command.get("op") == "create":
            session_id = row["result"].get("sessionId")
            if not isinstance(session_id, str) or not session_id:
                raise ValueError(kind + " public create result omitted its session ID")
            ids.append(session_id)
    if len(ids) != 2 or len(set(ids)) != 2:
        raise ValueError(kind + " must create two distinct public sessions")
    if kind == "native" and set(ids) != set(provided_ids):
        raise ValueError("Native did not preserve the caller-supplied public session IDs")
    endpoint = {key: value for key, value in config.items()
                if key not in ("bearer", "cookie", "expectedVersion", "commands")}
    endpoint.update(endpointId=kind, sessions=ids)
    return endpoint, ids


class MixedProvider:
    """Capture strict synthetic provider attempts without persisting request bodies."""

    def __init__(self, kind: str, busy_queue: bool = False):
        self.kind = kind
        self.busy_queue = busy_queue
        self.lock = threading.Lock()
        self.hold_entered = threading.Event()
        self.release_hold = threading.Event()
        self.attempts = 0
        self.errors: list[str] = []
        self.prompt_bodies: list[dict] = []
        self.prompt_texts: list[str] = []
        self.titles: list[str] = []
        self.request_log: list[dict] = []
        self.validated: list[dict] = []
        self.hold_body: dict | None = None

    def observe(self, path: str, body: object) -> None:
        with self.lock:
            self.attempts += 1
            attempt = self.attempts
        try:
            expected_path = "/native/v1/responses" if self.kind == "native" else "/dsh/v1/chat/completions"
            if path.rstrip("/") != expected_path:
                raise ValueError("unexpected provider route")
            if self.kind == "dsh":
                request_kind, title_text = classify_dsh_request(body)
                if request_kind == "title":
                    if title_text not in (TEXT_PROMPT, BUSY_HOLD_TEXT, ""):
                        raise ValueError("DSH title request includes unknown human text")
                    with self.lock:
                        if title_text in self.titles:
                            raise ValueError("duplicate DSH first-prompt title request")
                        if len(self.titles) >= (2 if not self.busy_queue else 3):
                            raise ValueError("unexpected extra DSH title request")
                        self.titles.append(title_text)
                        self.request_log.append({"attempt": attempt, "kind": "title", "text": title_text})
                    return
            text = _identify_prompt_text(self.kind, body, busy_queue=self.busy_queue, held_request=self.hold_body)
            expected_texts = (BUSY_HOLD_TEXT, TEXT_PROMPT, "") if self.busy_queue else (TEXT_PROMPT, "")
            if text not in expected_texts:
                raise ValueError("provider request includes unknown or repeated human input")
            with self.lock:
                if text in self.prompt_texts:
                    raise ValueError("duplicate provider request or automatic retry")
                if len(self.prompt_bodies) >= (3 if self.busy_queue else 2):
                    raise ValueError("unexpected extra provider prompt")
                self.prompt_texts.append(text)
                self.prompt_bodies.append(body)
                self.request_log.append({"attempt": attempt, "kind": "prompt", "text": text})
                if self.busy_queue and text == BUSY_HOLD_TEXT:
                    if self.hold_body is not None:
                        raise ValueError("duplicate busy hold request")
                    self.hold_body = body
        except Exception as error:
            with self.lock:
                self.errors.append(type(error).__name__ + ": " + str(error))
                self.request_log.append({"attempt": attempt, "kind": "rejected", "error": self.errors[-1],
                    "userShapes": [{"contentKind": type(item.get("content")).__name__,
                        "stringPrefix": item["content"][:120] if isinstance(item.get("content"), str) else None,
                        "blockTypes": [part.get("type") for part in item["content"] if isinstance(part, dict)]
                            if isinstance(item.get("content"), list) else None}
                        for item in (body.get("messages", []) if isinstance(body, dict) else [])
                        if isinstance(item, dict) and item.get("role") == "user"]})

    def validate(self, probe: dict, session_ids: list[str], dsh_home: str,
                 image_directory: Path) -> dict:
        try:
            validated = validate_probe_receipt(probe, self.kind, session_ids,
                                               busy_queue=self.busy_queue)
            by_text = dict(zip(self.prompt_texts, self.prompt_bodies))
            expected_texts = {BUSY_HOLD_TEXT, TEXT_PROMPT, ""} if self.busy_queue else {TEXT_PROMPT, ""}
            if len(self.prompt_bodies) != len(expected_texts) or set(self.prompt_texts) != expected_texts:
                raise ValueError(self.kind + " provider did not observe exactly the unretried business scenarios")
            busy = probe.get("busyQueue", {}) if self.busy_queue else {}
            hold_input = busy.get("holdInput")
            hold_assistant = busy.get("holdAssistant")
            if self.busy_queue:
                hold_users = _provider_users(self.kind, by_text[BUSY_HOLD_TEXT])
                if len(hold_users) != 1 or _provider_user_text(hold_users[0]) != BUSY_HOLD_TEXT:
                    raise ValueError(self.kind + " held provider request differs from its committed hold input")
            for index, text in enumerate((TEXT_PROMPT, "")):
                row = probe["roundTrips"][index]
                facts = inspect_provider_request(self.kind, by_text[text], expected_text=text,
                    images=validated["images"][index][0],
                    image_bytes=validated["images"][index][1],
                    upload_receipt=row["uploadReceipt"], input_item=row["input"], dsh_home=dsh_home,
                    preceding_turn=(hold_input, hold_assistant) if self.busy_queue and index == 0 else None,
                    preceding_request=by_text[BUSY_HOLD_TEXT] if self.busy_queue and index == 0 else None,
                    preceding_response_id=stream_response("/responses", {})[0]["response"]["id"]
                        if self.kind == "native" and self.busy_queue and index == 0 else None)
                self.validated.append(facts)
            if self.kind == "native" and self.titles:
                raise ValueError("Native request unexpectedly triggered a DSH title request")
            if self.kind == "dsh" and len(self.titles) > 2:
                raise ValueError("DSH title generator issued more than one first-prompt call per session")
            observations = [dict(facts) for facts in self.validated]
            image_data = [image for _, images in validated["images"] for image in images]
            image_rows = record_imageio_evidence(observations, image_data, image_directory)
            with self.lock:
                if self.errors:
                    raise ValueError(self.kind + " provider attempts include rejected, unknown, duplicate, or retry requests")
                expected_business = 3 if self.busy_queue else 2
                if len(self.prompt_bodies) != expected_business or self.attempts != expected_business + len(self.titles):
                    raise ValueError(self.kind + " provider wire request count differs or contains an automatic retry")
            return {
                "attemptCount": self.attempts,
                "requestCount": 3 if self.busy_queue else 2,
                "retryCount": 0,
                "busyQueue": self.busy_queue,
                "auxiliaryTitleRequests": list(self.titles),
                "requests": image_rows,
                "requestLog": list(self.request_log),
                "requestBodiesPersisted": False,
            }
        except Exception as error:
            with self.lock:
                message = type(error).__name__ + ": " + str(error)
                self.errors.append(message)
                self.request_log.append({"attempt": self.attempts, "kind": "validation-rejected",
                                         "error": message})
            raise

    def diagnostic(self) -> dict:
        with self.lock:
            return {"attemptCount": self.attempts, "requestLog": list(self.request_log),
                    "errors": list(self.errors), "requestBodiesPersisted": False}


def _provider_users(kind: str, body: object) -> list[dict]:
    request = body if isinstance(body, dict) else {}
    field = "input" if kind == "native" else "messages"
    history = request.get(field)
    if not isinstance(history, list):
        return []
    users = [item for item in history if isinstance(item, dict) and item.get("role") == "user"]
    if kind == "dsh":
        users, _ = strip_dsh_context_suffixes(users)
    return users


def _provider_user_text(user: dict) -> str:
    content = user.get("content")
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return "<unknown>"
    texts = [part.get("text") for part in content if isinstance(part, dict)
             and part.get("type") in ("input_text", "text") and isinstance(part.get("text"), str)]
    if len(texts) == 1 and len(content) == 1:
        return texts[0]
    markers = [text for text in texts if text in (TEXT_PROMPT, BUSY_HOLD_TEXT)]
    return markers[0] if len(markers) == 1 else ("<unknown>" if markers else "")


def _identify_prompt_text(kind: str, body: object, *, busy_queue: bool = False, held_request: dict | None = None) -> str:
    users = _provider_users(kind, body)
    if not users:
        return "<unknown>"
    if len(users) == 1:
        text = _provider_user_text(users[0])
        return text if text in ((BUSY_HOLD_TEXT, TEXT_PROMPT, "") if busy_queue else (TEXT_PROMPT, "")) else "<unknown>"
    if busy_queue and held_request is not None and isinstance(body, dict):
        stripped = strip_provider_hold_request(kind, body, held_request)
        remaining = _provider_users(kind, stripped)
        if len(remaining) == 1 and _provider_user_text(remaining[0]) == TEXT_PROMPT:
            return TEXT_PROMPT
    return "<unknown>"


class ProviderHandler(http.server.BaseHTTPRequestHandler):
    providers: dict[str, MixedProvider] = {}

    def log_message(self, *_args) -> None:
        pass

    def setup(self) -> None:
        super().setup()
        self.connection.settimeout(20)

    def do_POST(self) -> None:
        kind = self.path.split("/", 2)[1] if "/" in self.path else ""
        provider = self.providers.get(kind)
        if provider is None:
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("Content-Length", "-1"))
            if length < 0 or length > MAX_REQUEST_BYTES:
                raise ValueError("provider request size is outside the bounded fixture limit")
            body = json.loads(self.rfile.read(length))
            provider.observe(self.path, body)
            frames = stream_response(self.path, body)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            def emit(frame):
                self.wfile.write(("data: " + json.dumps(frame, ensure_ascii=False) + "\n\n").encode())
                self.wfile.flush()
            if provider.busy_queue and provider.hold_body is body:
                prefix, frames = busy_hold_stream_frames(kind)
                for frame in prefix:
                    emit(frame)
                provider.hold_entered.set()
                if not provider.release_hold.wait(timeout=PROBE_TIMEOUT):
                    raise TimeoutError("owned provider hold exceeded its bounded release deadline")
            for frame in frames:
                emit(frame)
            if not self.path.endswith("/responses"):
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        except Exception as error:
            with provider.lock:
                provider.errors.append(type(error).__name__ + ": " + str(error))
                provider.request_log.append({"attempt": provider.attempts, "kind": "handler-error",
                                             "error": provider.errors[-1]})
            payload = json.dumps({"error": {"message": "synthetic provider rejected malformed input",
                                               "type": "invalid_request_error"}}).encode()
            with suppress(BrokenPipeError, ConnectionResetError):
                self.send_response(400)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)


def run_probe(kind: str, endpoint: dict, session_ids: list[str], program: Path,
              env: dict[str, str], workspace: Path, case: Path,
              *, busy_queue: bool = False) -> dict:
    config = {key: value for key, value in endpoint.items() if not key.startswith("_")}
    if busy_queue:
        config["busyQueue"] = True
    if kind == "native":
        config["bearerEnv"] = NATIVE_AUTH
    else:
        config["cookieEnv"] = DSH_AUTH
    config_path = case / "endpoint.json"
    write_json(config_path, config)
    receipt_path = case / "native-dsh-mixed-probe-receipt.json"
    probe_env = dict(env)
    probe_env[NATIVE_AUTH if kind == "native" else DSH_AUTH] = endpoint["_auth"]
    argv = [str(program), "--endpoint-config", str(config_path), "--receipt", str(receipt_path)]
    write_json(case / "probe-command.json", {"argv": argv, "sessionIds": session_ids,
        "credentialsPassedViaEnvironment": True})
    process = subprocess.Popen(argv, text=True, encoding="utf-8", errors="replace",
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=probe_env, cwd=workspace,
        close_fds=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=PROBE_TIMEOUT)
    except subprocess.TimeoutExpired:
        terminate_owned_group(process)
        stdout, stderr = process.communicate()
        write_text(case / "probe.stdout.log", stdout)
        write_text(case / "probe.stderr.log", stderr)
        write_json(case / "probe.exit.json", {"timedOut": True, "pid": process.pid,
                                                "processGroupExited": group_exited(process.pid)})
        raise RuntimeError(kind + " mixed attachment probe timed out; owned process group was stopped") from None
    leaked = not group_exited(process.pid)
    if leaked:
        terminate_owned_group(process)
    write_text(case / "probe.stdout.log", stdout)
    write_text(case / "probe.stderr.log", stderr)
    write_json(case / "probe.exit.json", {"exitStatus": process.returncode, "pid": process.pid,
                                            "processGroupExited": not leaked})
    if leaked:
        raise RuntimeError(kind + " mixed attachment probe left an owned process group")
    if process.returncode:
        raise RuntimeError(kind + " public RunBinding mixed attachment probe failed; logs retained")
    if not receipt_path.is_file() or receipt_path.is_symlink():
        raise ValueError(kind + " probe did not write a regular receipt")
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        raise ValueError(kind + " probe receipt is unreadable JSON") from None
    validate_probe_receipt(receipt, kind, session_ids, busy_queue=busy_queue)
    return receipt


class ProbeStdout:
    """Capture line-oriented probe stdout without blocking provider gate coordination."""

    def __init__(self, stream, path: Path):
        self.stream = stream
        self.path = path
        self.lines: list[str] = []
        self.events: queue.Queue[str | None] = queue.Queue()
        self.lock = threading.Lock()
        self.thread = threading.Thread(target=self._read, daemon=True)
        self.thread.start()

    def _read(self) -> None:
        try:
            for line in self.stream:
                with self.lock:
                    self.lines.append(line)
                self.events.put(line)
        finally:
            self.events.put(None)

    def snapshot(self) -> list[str]:
        with self.lock:
            return list(self.lines)

    def save(self) -> None:
        write_text(self.path, "".join(self.snapshot()))


def run_busy_probe(kind: str, endpoint: dict, session_ids: list[str], program: Path,
                   env: dict[str, str], workspace: Path, case: Path,
                   provider: MixedProvider) -> dict:
    config = {key: value for key, value in endpoint.items() if not key.startswith("_")}
    config["busyQueue"] = True
    if kind == "native":
        config["bearerEnv"] = NATIVE_AUTH
    else:
        config["cookieEnv"] = DSH_AUTH
    config_path = case / "endpoint.json"
    write_json(config_path, config)
    receipt_path = case / "native-dsh-mixed-probe-receipt.json"
    probe_env = dict(env)
    probe_env[NATIVE_AUTH if kind == "native" else DSH_AUTH] = endpoint["_auth"]
    argv = [str(program), "--endpoint-config", str(config_path), "--receipt", str(receipt_path)]
    write_json(case / "probe-command.json", {"argv": argv, "sessionIds": session_ids,
        "credentialsPassedViaEnvironment": True, "busyQueue": True})
    stderr_stream = (case / "probe.stderr.log").open("x", encoding="utf-8")
    process = subprocess.Popen(argv, text=True, encoding="utf-8", errors="replace",
        stdout=subprocess.PIPE, stderr=stderr_stream, env=probe_env, cwd=workspace,
        close_fds=True, start_new_session=True, bufsize=1)
    assert process.stdout is not None
    captured = ProbeStdout(process.stdout, case / "probe.stdout.log")
    released = False
    try:
        if not provider.hold_entered.wait(timeout=HEADLESS_TIMEOUT):
            raise TimeoutError("provider did not enter the independent busy hold request")
        deadline = time.monotonic() + BUSY_READY_TIMEOUT
        marker_lines = []
        while time.monotonic() < deadline:
            remaining = max(0.01, deadline - time.monotonic())
            try:
                line = captured.events.get(timeout=min(0.25, remaining))
            except queue.Empty:
                if process.poll() is not None:
                    raise RuntimeError(kind + " probe exited before its public queue-ready event")
                continue
            if line is None:
                raise RuntimeError(kind + " probe stdout closed before its public queue-ready event")
            if line.startswith(BUSY_READY_PREFIX):
                marker_lines.append(line)
                break
        if len(marker_lines) != 1:
            raise TimeoutError(kind + " probe did not emit one bounded public busy queue-ready line")
        line = marker_lines[0]
        if not line.endswith("\n"):
            raise ValueError("busy queue-ready marker was not a complete stdout line")
        try:
            ready = json.loads(line[len(BUSY_READY_PREFIX):].strip())
        except json.JSONDecodeError:
            raise ValueError("busy queue-ready marker is not JSON") from None
        busy = validate_busy_queue_ready(ready, kind, session_ids[0],
                                         ready.get("holdRpcId") if isinstance(ready, dict) else "")
        with provider.lock:
            if (not provider.hold_entered.is_set() or provider.release_hold.is_set()
                    or provider.prompt_texts != [BUSY_HOLD_TEXT]
                    or provider.attempts != len(provider.titles) + 1
                    or provider.errors):
                raise ValueError("queue-ready arrived without an active isolated provider hold")
        if (ready.get("holdInput") != busy["ready"].get("holdInput")
                or ready.get("holdRpcId") != ready["holdInput"].get("reconciliationId")):
            raise ValueError("queue-ready hold identity differs from the committed original hold user")
        write_json(case / "queue-ready-validated.json", {
            "status": "validated-before-release", "scope": busy["scope"],
            "rpcId": busy["rpcId"], "ticketId": ready["ticketId"],
            "snapshotRevision": ready["snapshotRevision"],
            "connectionEpoch": ready["connectionEpoch"],
            "queueFact": ready["queueFact"], "queueWitness": ready["queueWitness"],
            "providerHoldEntered": True, "queuedBusinessRequestReachedProvider": False,
            "providerHoldReleased": False,
        })
        provider.release_hold.set()
        released = True
        try:
            process.wait(timeout=PROBE_TIMEOUT)
        except subprocess.TimeoutExpired:
            terminate_owned_group(process)
            captured.save()
            write_json(case / "probe.exit.json", {"timedOut": True, "pid": process.pid,
                "processGroupExited": group_exited(process.pid)})
            raise RuntimeError(kind + " busy probe timed out after the bounded provider release") from None
        captured.thread.join(timeout=5)
        captured.save()
        leaked = not group_exited(process.pid)
        if leaked:
            terminate_owned_group(process)
        write_json(case / "probe.exit.json", {"exitStatus": process.returncode, "pid": process.pid,
            "processGroupExited": not leaked})
        if leaked:
            raise RuntimeError(kind + " busy probe left an owned process group")
        if process.returncode:
            raise RuntimeError(kind + " busy public probe failed after queue release; logs retained")
        markers = [row for row in captured.snapshot() if row.startswith(BUSY_READY_PREFIX)]
        if len(markers) != 1 or markers[0] != line:
            raise ValueError(kind + " probe emitted duplicate or changed its public queue-ready event")
        if not receipt_path.is_file() or receipt_path.is_symlink():
            raise ValueError(kind + " busy probe did not write a regular receipt")
        try:
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            raise ValueError(kind + " busy probe receipt is unreadable JSON") from None
        if receipt.get("busyQueue", {}).get("queueReady") != ready:
            raise ValueError(kind + " final receipt differs from the pre-release ready event")
        validate_probe_receipt(receipt, kind, session_ids, busy_queue=True)
        return receipt
    except BaseException:
        provider.release_hold.set()
        if process.poll() is None:
            with suppress(subprocess.TimeoutExpired):
                process.wait(timeout=2)
        if process.poll() is None or not group_exited(process.pid, timeout=0.2):
            terminate_owned_group(process)
        with suppress(OSError):
            captured.save()
            write_json(case / "probe.exit.json", {"exitStatus": process.poll(), "pid": process.pid,
                "providerHoldReleasedForCleanup": True, "releasedAfterQueueValidation": released,
                "processGroupExited": group_exited(process.pid, timeout=0.2)})
        raise
    finally:
        provider.release_hold.set()
        with suppress(OSError):
            stderr_stream.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("headless-program", "image-program", "dsh-root", "kernel-root",
                 "kernel-supervisor", "kernel-worker", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--busy-queue", action="store_true",
                         help="hold the first provider request until a strict public queue witness is validated")
    parser.add_argument("--endpoint", action="append", choices=("native", "dsh"), help="select an explicit endpoint subset; default is both")
    args = parser.parse_args()
    selected = args.endpoint or ["native", "dsh"]
    if len(selected) != len(set(selected)):
        parser.error("duplicate endpoint selection")
    output = args.output.expanduser().absolute()
    try:
        ssd_root = SSD_PROJECT.resolve(strict=True)
    except OSError:
        parser.error("the existing AgentKit SSD project directory is required")
    if output.is_symlink() or not output.resolve().is_relative_to(ssd_root) or output.exists():
        parser.error("--output must be a new directory under the existing AgentKit SSD project")
    programs = {
        "headless": executable(args.headless_program, "--headless-program"),
        "image": executable(args.image_program, "--image-program"),
    }
    dsh_root = args.dsh_root.resolve(strict=True)
    supervisor, worker, kernel_identity = kernel_programs(args.kernel_root,
        supervisor=args.kernel_supervisor, worker=args.kernel_worker, expected_version="0.9.3")
    provider_path = args.kernel_root.resolve(strict=True) / "fixtures/config/providers.canonical.json"
    provider_snapshot = json.loads(provider_path.read_text(encoding="utf-8"))
    if (not isinstance(provider_snapshot.get("providers"), list) or not provider_snapshot["providers"]
            or not provider_snapshot["providers"][0].get("id")
            or not provider_snapshot["providers"][0].get("models")):
        parser.error("canonical Native provider snapshot is incomplete")

    output.mkdir(mode=0o700, parents=True)
    programs_dir = output / "programs"
    programs_dir.mkdir()
    identities = {}
    for key, source in programs.items():
        source_digest = digest_file(source)
        archived = programs_dir / ("owned-" + key + "-" + source.name)
        shutil.copy2(source, archived)
        if digest_file(source) != source_digest or digest_file(archived) != source_digest:
            raise RuntimeError("an existing executable changed while its identity was recorded")
        identities[key] = {"path": str(source), "sha256": source_digest, "archiveOnly": str(archived)}
    write_json(output / "programs.json", identities)
    source_files = [Path(__file__).resolve(), Path(__file__).with_name("native_dsh_mixed_fixture.py"),
        Path(__file__).with_name("native_owned_fixture.py"), Path(__file__).with_name("dsh_owned_fixture.py"),
        Path(__file__).with_name("runtime_provenance.py"),
        Path(__file__).with_name("inline_image_live_fixture.py")]
    write_json(output / "source-hashes.json", {path.name: digest_file(path) for path in source_files})
    write_json(output / "runtime-inputs.json", {"dshRoot": str(dsh_root), "kernel": kernel_identity,
        "providerSnapshot": {"path": str(provider_path), "sha256": digest_file(provider_path)},
        "privateHistoryRead": False, "credentialsRecorded": False})

    workspace = output / "workspace"
    workspace.mkdir()
    host_home = output / "host-home"
    host_home.mkdir(mode=0o700)
    temp_root = output / "tmp"
    temp_root.mkdir(mode=0o700)
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(host_home),
        "TMPDIR": str(temp_root), "LANG": "en_US.UTF-8", "LC_ALL": "en_US.UTF-8"}
    providers = {kind: MixedProvider(kind, busy_queue=args.busy_queue) for kind in ("native", "dsh")}
    ProviderHandler.providers = providers
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), ProviderHandler)
    server.daemon_threads = False
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    provider_url = "http://127.0.0.1:" + str(server.server_port)
    results: dict[str, dict] = {}
    sessions: dict[str, list[str]] = {}
    processes: dict[str, object] = {}
    failure: str | None = None
    try:
        with ExitStack() as stack:
            native = stack.enter_context(owned_native(supervisor, worker, provider_snapshot,
                provider_url + "/native/v1", output, env))
            processes["native"] = native["process"]
            dsh_home = output / "dsh-home"
            dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home),
                DSH_AGENTS_HOME=str(dsh_home / "agents"), DSH_TELEMETRY_DISABLED="1",
                AGENTKIT_SYNTHETIC_KEY="synthetic-key")
            dsh = stack.enter_context(owned_dsh(dsh_root, output / "dsh-data", dsh_home,
                workspace, provider_url + "/dsh/v1", dsh_env, query_sqlite=False, image_input=True))
            processes["dsh"] = dsh.process
            if dsh.version != EXPECTED_DSH_VERSION:
                raise ValueError("unexpected pinned DSH runtime version")
            base = native["providers"]["providers"][0]
            endpoints = [
                ("native", {"kind": "native", "url": native["url"], "bearer": native["bearer"],
                    "expectedVersion": "0.9.3"}, base["id"], base["models"][0]["id"]),
                ("dsh", dict(dsh.connector_config([]), expectedVersion=dsh.version),
                    "endpoint-test", "endpoint-test"),
            ]
            for kind, config, provider_id, model_id in endpoints:
                if kind not in selected:
                    continue
                case = output / kind
                case.mkdir(mode=0o700)
                endpoint, ids = seed_endpoint(kind, config, provider_id, model_id,
                    programs["headless"], env, workspace, case)
                sessions[kind] = ids
                endpoint["_auth"] = native["bearer"] if kind == "native" else dsh.cookie
                receipt = (run_busy_probe(kind, endpoint, ids, programs["image"], env,
                    workspace, case, providers[kind]) if args.busy_queue else
                    run_probe(kind, endpoint, ids, programs["image"], env, workspace, case))
                results[kind] = providers[kind].validate(receipt, ids, str(dsh_home),
                    case / "provider-images")
                write_json(case / "probe-receipt-validated.json", receipt)
                write_json(case / "provider-observation.json", results[kind])
    except BaseException as error:
        failure = type(error).__name__ + ": " + str(error)
        with suppress(OSError):
            write_json(output / "failure.json", {"error": failure})
    finally:
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=5)
        exits = {name: process.poll() for name, process in processes.items()}
        with suppress(OSError):
            write_json(output / "owned-process-exits.json", exits)
        for kind, provider in providers.items():
            with suppress(OSError):
                write_json(output / kind / "provider-attempts.json", provider.diagnostic())

    if server_thread.is_alive():
        failure = failure or "owned provider server did not stop"
    if failure is None:
        for kind in selected:
            provider = providers[kind]
            expected_count = (3 if args.busy_queue else 2) + len(provider.titles)
            if provider.errors or provider.attempts != expected_count:
                failure = kind + " provider count changed after owned services stopped or contains errors"
                break
        if any(status != 0 for status in exits.values()) or set(exits) != {"native", "dsh"}:
            failure = failure or "an owned Native/DSH process did not exit cleanly"
        if set(results) != set(selected):
            failure = failure or "the selected endpoint probes were not accepted"
    if failure is not None:
        with suppress(OSError):
            write_json(output / "failure.json", {"error": failure})
        print("NATIVE_DSH_MIXED_LIVE_FAILED; inspect retained owned evidence")
        return 1

    provider_count = sum(result["requestCount"] for result in results.values())
    write_json(output / "run-receipt.json", {
        "accepted": True,
        "endpoints": results,
        "endpointsNotRun": [kind for kind in ("native", "dsh") if kind not in selected],
        "providerPromptCount": provider_count,
        "busyQueue": args.busy_queue,
        "provider": "synthetic-loopback",
        "desktopInteraction": False,
        "privateHarnessHistoryRead": False,
        "paidModel": False,
        "pi": "not_run",
    })
    print("NATIVE_DSH_MIXED_LIVE_ACCEPTED " + "+".join(selected) + "; desktop=false")
    return 0


def main_entry() -> None:
    raise SystemExit(main())


def _main() -> None:
    main_entry()


if __name__ == "__main__":
    _main()
