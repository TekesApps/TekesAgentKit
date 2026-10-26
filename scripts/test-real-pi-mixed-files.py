#!/usr/bin/env python3
"""Verify Pi direct and genuinely queued image/file/image sends using owned fixtures.

This runner consumes existing headless and probe executables. It never invokes Cargo,
uses Pi's public session APIs, and persists request summaries rather than provider bodies.
"""
from __future__ import annotations

import argparse
import base64
from contextlib import suppress
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import threading
import time
import uuid

from inline_image_live_fixture import decode_image_with_imageio, stream_response
from pi_mixed_file_fixture import (
    DIRECT_TEXT,
    EXPECTED_REPLY,
    FILE_BYTES,
    FILE_NAME,
    HOLD_TEXT,
    QUEUED_TEXT,
    READY_PREFIX,
    has_transcript_file_prompt,
    inspect_pi_user_message,
    validate_file_read,
    validate_queue_ready_lines,
    write_json_exclusive,
)
from pi_owned_fixture import prepare_pi


ROOT = Path(__file__).resolve().parents[1]
SSD_PROJECT = Path("/Volumes/HP P900/dev/TekesAgentKit")
EXPECTED_PI_VERSION = "1.3.0"
READY_TIMEOUT_SECONDS = 190
PROBE_TIMEOUT_SECONDS = 240
HEADLESS_TIMEOUT_SECONDS = 90
HOLD_TIMEOUT_SECONDS = 210
MAX_REQUEST_BYTES = 64 * 1024 * 1024


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_text(path: Path, text: str) -> None:
    with path.open("x", encoding="utf-8") as stream:
        stream.write(text)


def terminate_owned_group(process: subprocess.Popen) -> None:
    """Stop only the dedicated process session created by this runner."""
    sent = False
    with suppress(ProcessLookupError):
        os.killpg(process.pid, signal.SIGTERM)
        sent = True
    if process.poll() is None:
        with suppress(subprocess.TimeoutExpired):
            process.wait(timeout=4)
    if sent:
        with suppress(ProcessLookupError):
            os.killpg(process.pid, signal.SIGKILL)
    if process.poll() is None:
        with suppress(subprocess.TimeoutExpired):
            process.wait(timeout=4)


def owned_group_exists(session_id: int) -> bool:
    try:
        os.killpg(session_id, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def await_owned_group_exit(session_id: int, timeout: float = 3) -> bool:
    # The parent can be reaped just before the runtime's last child exits.
    # Give normal shutdown a bounded grace period before classifying a leak.
    deadline = time.monotonic() + timeout
    while owned_group_exists(session_id):
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.05)
    return True


def read_pi_version(pi_root: Path) -> tuple[str, Path]:
    package_path = pi_root / "node_modules/@earendil-works/pi-coding-agent/package.json"
    value = json.loads(package_path.read_text(encoding="utf-8"))
    version = value.get("version") if isinstance(value, dict) else None
    if not isinstance(version, str):
        raise ValueError("Pi runtime package.json has no version")
    if version != EXPECTED_PI_VERSION:
        raise ValueError(f"runner requires Pi {EXPECTED_PI_VERSION}; found {version}")
    return version, package_path


def assert_executable(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve(strict=True)
    if not resolved.is_file() or not os.access(resolved, os.X_OK):
        raise ValueError(f"{label} must be an existing executable regular file")
    return resolved


def run_headless(program: Path, config: dict, commands: list[dict], output: Path,
                 environment: dict[str, str], workspace: Path) -> list[dict]:
    input_config = dict(config, commands=commands)
    write_json_exclusive(output / "pi-seed-commands.json", commands)
    process = subprocess.Popen(
        [str(program)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace",
        env=environment, cwd=workspace, close_fds=True, start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(
            input=json.dumps(input_config, ensure_ascii=False), timeout=HEADLESS_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired as error:
        terminate_owned_group(process)
        stdout, stderr = process.communicate()
        write_text(output / "pi-seed.stdout.log", stdout)
        write_text(output / "pi-seed.stderr.log", stderr)
        write_json_exclusive(output / "pi-seed.exit.json", {"timedOut": True})
        raise RuntimeError("public Pi session creation timed out; logs retained") from error
    write_text(output / "pi-seed.stdout.log", stdout)
    write_text(output / "pi-seed.stderr.log", stderr)
    write_json_exclusive(output / "pi-seed.exit.json", {"exitStatus": process.returncode,
                                                          "pid": process.pid,
                                                          "sessionId": process.pid})
    leaked = not await_owned_group_exit(process.pid)
    if leaked:
        terminate_owned_group(process)
    if leaked:
        raise RuntimeError("public Pi session seeder left an owned child process; group cleaned")
    if process.returncode != 0:
        raise RuntimeError("public headless Pi session creation failed; logs retained")
    rows = []
    for line in stdout.splitlines():
        if line.strip():
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise ValueError("public headless Pi output contains a non-JSON line") from error
    if (len(rows) != len(commands) + 1 or rows[0].get("ready") is not True
            or rows[0].get("kind") != "pi"
            or rows[0].get("version") != EXPECTED_PI_VERSION):
        raise ValueError("public Pi runtime handshake, version or command count differs")
    if any(row.get("op") != command.get("op") or "result" not in row
           for command, row in zip(commands, rows[1:])):
        raise ValueError("public Pi command/result pairing differs")
    ids = [row["result"].get("sessionId") for row in rows[1:]
           if row.get("op") == "create"]
    if len(ids) != 2 or any(not isinstance(value, str) or not value for value in ids) or len(set(ids)) != 2:
        raise ValueError("public Pi create commands did not return two distinct session IDs")
    return ids


def _text_parts(message: dict) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(part.get("text", "") for part in content
                         if isinstance(part, dict) and part.get("type") == "text"
                         and isinstance(part.get("text"), str))
    return ""


class MixedPiProvider:
    """A loopback Pi provider that holds only the public queue precondition request."""

    def __init__(self, image_directory: Path):
        self.image_directory = image_directory
        self.lock = threading.Lock()
        self.hold_entered = threading.Event()
        self.release_hold = threading.Event()
        self.attempts = 0
        self.observations: list[dict] = []
        self.failures: list[str] = []
        self._images: dict[str, list[dict]] = {}

    def observe(self, path: str, body: object) -> tuple[str, dict]:
        with self.lock:
            self.attempts += 1
            attempt = self.attempts
        if path.rstrip("/") != "/v1/chat/completions":
            raise ValueError("unexpected Pi provider endpoint")
        markers = []
        for marker in (DIRECT_TEXT, HOLD_TEXT, QUEUED_TEXT):
            try:
                facts = inspect_pi_user_message(body, marker)
            except ValueError:
                continue
            markers.append((marker, facts))
        if len(markers) != 1:
            raise ValueError("provider request must contain exactly one known scenario marker")
        marker, facts = markers[0]
        expected_order = [DIRECT_TEXT, HOLD_TEXT, QUEUED_TEXT]
        with self.lock:
            if attempt > len(expected_order) or marker != expected_order[attempt - 1]:
                raise ValueError("provider requests are duplicated, reordered or unexpected")
        if marker == HOLD_TEXT:
            if facts["images"] or facts["hasFilePrompt"]:
                raise ValueError("queue hold request must be text-only")
        else:
            if len(facts["images"]) != 2 or not facts["hasFilePrompt"]:
                raise ValueError("mixed Pi provider request must contain file text and two images")
            if facts["images"][0]["sha256"] == facts["images"][1]["sha256"]:
                raise ValueError("mixed Pi provider request images must be distinct")

        row = {
            "attempt": attempt,
            "marker": marker,
            "hasExpectedFilePrompt": facts["hasFilePrompt"],
            "imageCount": len(facts["images"]),
            "images": [{key: image[key] for key in ("mime", "bytes", "sha256")}
                       for image in facts["images"]],
            "requestBodyPersisted": False,
        }
        with self.lock:
            self.observations.append(row)
            self._images[marker] = facts["images"]

        if marker == HOLD_TEXT:
            self.hold_entered.set()
            if not self.release_hold.wait(HOLD_TIMEOUT_SECONDS):
                raise TimeoutError("provider queue hold was not released by validated public queue fact")
        return marker, facts

    def record_failure(self, error: BaseException) -> None:
        with self.lock:
            self.failures.append(type(error).__name__ + ": " + str(error))

    def validated_observations(self) -> dict[str, dict]:
        with self.lock:
            if self.attempts != 3 or len(self.observations) != 3 or self.failures:
                raise ValueError("Pi provider did not complete exactly three unretried requests")
            rows = [dict(row) for row in self.observations]
            if [row["marker"] for row in rows] != [DIRECT_TEXT, HOLD_TEXT, QUEUED_TEXT]:
                raise ValueError("Pi provider request sequence differs or contains a retry")
            return {row["marker"]: row for row in rows}

    def image_inputs(self, marker: str) -> list[dict]:
        with self.lock:
            return list(self._images.get(marker, []))

    def validate_and_save_images(self) -> dict:
        self.validated_observations()
        with self.lock:
            image_sets = {marker: list(images) for marker, images in self._images.items()}
            rows = [dict(row) for row in self.observations]
        for marker in (DIRECT_TEXT, QUEUED_TEXT):
            for index, image in enumerate(image_sets.get(marker, []), 1):
                evidence = decode_image_with_imageio(
                    image["_bytes"], image["mime"], self.image_directory,
                    marker.lower() + "-image-" + str(index),
                )
                rows[[row["marker"] for row in rows].index(marker)]["images"][index - 1].update(evidence)
        return {
            "requestCount": self.attempts,
            "retryCount": 0,
            "requests": rows,
            "requestBodiesPersisted": False,
            "queueHoldEntered": self.hold_entered.is_set(),
            "queueHoldReleasedAfterReadyValidation": self.release_hold.is_set(),
        }


class ProviderHandler(http.server.BaseHTTPRequestHandler):
    coordinator: MixedPiProvider

    def log_message(self, *_args) -> None:
        pass

    def setup(self) -> None:
        super().setup()
        self.connection.settimeout(20)

    def _respond(self, status: int, payload: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        with suppress(BrokenPipeError, ConnectionResetError):
            self.wfile.write(payload)

    def do_POST(self) -> None:
        try:
            length = int(self.headers.get("Content-Length", "-1"))
            if not 0 <= length <= MAX_REQUEST_BYTES:
                raise ValueError("Pi provider request size is invalid")
            body = json.loads(self.rfile.read(length))
            marker, _facts = self.coordinator.observe(self.path, body)
            frames = stream_response(self.path, body)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            for frame in frames:
                self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode("utf-8"))
                self.wfile.flush()
            if not self.path.endswith("/responses"):
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
            # Never print or retain request text, image payloads or the Pi upload path.
            _ = marker
        except (ValueError, TypeError, json.JSONDecodeError, TimeoutError) as error:
            self.coordinator.record_failure(error)
            payload = json.dumps({"error": {"message": "owned Pi fixture rejected request",
                                             "type": "invalid_request_error"}}).encode("utf-8")
            with suppress(BrokenPipeError, ConnectionResetError):
                self._respond(400, payload, "application/json")
        except (BrokenPipeError, ConnectionResetError):
            pass


class CapturedStdout:
    def __init__(self, stream, log_path: Path):
        self.lines: queue.Queue[str | None] = queue.Queue()
        self.all_lines: list[str] = []
        self._lock = threading.Lock()
        self.thread = threading.Thread(target=self._read, args=(stream, log_path), daemon=True)
        self.thread.start()

    def _read(self, stream, log_path: Path) -> None:
        with log_path.open("x", encoding="utf-8") as log:
            for raw in stream:
                line = raw.rstrip("\r\n")
                log.write(line + "\n")
                log.flush()
                with self._lock:
                    self.all_lines.append(line)
                self.lines.put(line)
        self.lines.put(None)

    def wait_ready(self, process: subprocess.Popen, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if process.poll() is not None and self.lines.empty():
                break
            try:
                line = self.lines.get(timeout=0.1)
            except queue.Empty:
                continue
            if line is None:
                break
            if READY_PREFIX in line:
                return
        raise TimeoutError("probe did not emit public Pi queue-ready event before timeout")

    def snapshot(self) -> list[str]:
        with self._lock:
            return list(self.all_lines)


def transcript_id(item: object, key: str) -> str:
    if not isinstance(item, dict) or not isinstance(item.get(key), str) or not item[key]:
        raise ValueError("probe transcript item lacks " + key)
    return item[key]


def validate_native_source(source: object, rpc_id: str, input_item: dict) -> dict:
    if not isinstance(source, dict):
        raise ValueError("probe omitted public native source projection")
    receipt = source.get("nativeReceipt")
    message = source.get("nativeMessage")
    receipt_data = receipt.get("data") if isinstance(receipt, dict) else None
    native_message = message.get("message") if isinstance(message, dict) else None
    if (not isinstance(receipt, dict) or not isinstance(receipt_data, dict)
            or not isinstance(message, dict) or not isinstance(native_message, dict)):
        raise ValueError("public native receipt/message evidence is missing")
    if (receipt.get("customType") not in ("agentkit.inputReceipt.v1", "tekes.inputReceipt.v1")
            or receipt_data.get("rpcId") != rpc_id
            or message.get("id") != transcript_id(input_item, "recordId")
            or message.get("type") != "message"
            or native_message.get("role") != "user"):
        raise ValueError("public native source entries do not match the transcript RPC and record")
    text = _text_parts(native_message)
    if (receipt_data.get("timestamp") != native_message.get("timestamp")
            or receipt_data.get("hash") != hashlib.sha256(text.encode("utf-8")).hexdigest()
            or text != input_item.get("text")):
        raise ValueError("native receipt hash/timestamp does not bind the original user text")
    content = native_message.get("content")
    raw_images = [part for part in content if isinstance(part, dict) and part.get("type") == "image"] if isinstance(content, list) else []
    if raw_images != input_item.get("attachments", []):
        raise ValueError("native user images differ from the original transcript attachments")
    files = receipt_data.get("files", [])
    metadata = input_item.get("metadata", {})
    if files != metadata.get("admittedFileAttachments", []):
        raise ValueError("native receipt files differ from the projected file facts")
    if files and metadata.get("admittedFileReceipt") != {
            "entryId": receipt.get("id"), "customType": receipt.get("customType")}:
        raise ValueError("projected file provenance differs from the original receipt entry")
    return {"nativeReceipt": receipt, "nativeMessage": message}


def validate_case(probe: dict, key: str, marker: str, expected_session: str,
                  provider_images: list[dict], *, queue_ready: dict | None = None,
                  epoch_before: int, epoch_after: int) -> dict:
    row = probe.get(key)
    if not isinstance(row, dict):
        raise ValueError("probe receipt lacks " + key + " case")
    scope = row.get("scope")
    expected_scope = {"endpointId": "pi", "threadId": expected_session,
                      "sessionId": expected_session}
    if scope != expected_scope:
        raise ValueError(key + " probe scope differs from selected Pi session")
    rpc_id = row.get("rpcId")
    if not isinstance(rpc_id, str) or not rpc_id:
        raise ValueError(key + " RPC identity is missing")
    input_item, assistant = row.get("input"), row.get("assistant")
    if (not isinstance(input_item, dict) or input_item.get("kind") != "user"
            or not has_transcript_file_prompt(input_item.get("text"), marker)
            or input_item.get("reconciliationId") != rpc_id
            or input_item.get("isProvisional") is not False
            or input_item.get("sourcePosition") is None and input_item.get("sourceSequence") is None):
        raise ValueError(key + " transcript input does not have the expected committed source")
    if (not isinstance(assistant, dict) or assistant.get("kind") != "assistant"
            or assistant.get("text", "").strip() != EXPECTED_REPLY
            or assistant.get("turnId") != input_item.get("turnId")
            or assistant.get("isFinal") is not True or assistant.get("isProvisional") is not False):
        raise ValueError(key + " assistant reply differs from the fixed synthetic response")
    image_inputs = row.get("imageInputs")
    if not isinstance(image_inputs, list) or len(image_inputs) != 2:
        raise ValueError(key + " canonical imageInputs are missing")
    actual_canonical = []
    for image in image_inputs:
        if not isinstance(image, dict) or image.get("type") != "image" or image.get("mediaType") != "image/png":
            raise ValueError(key + " canonical image block differs")
        try:
            data = base64.b64decode(image.get("data", ""), validate=True)
        except (ValueError, TypeError):
            raise ValueError(key + " canonical image data is malformed") from None
        if not data.startswith(b"\x89PNG\r\n\x1a\n"):
            raise ValueError(key + " canonical image is not PNG")
        actual_canonical.append({"mime": "image/png", "bytes": len(data),
                                 "sha256": hashlib.sha256(data).hexdigest(), "data": data})
    if actual_canonical[0]["sha256"] == actual_canonical[1]["sha256"]:
        raise ValueError(key + " canonical images must be different")
    file_receipt = row.get("fileReceipt")
    receipt_file = file_receipt.get("file") if isinstance(file_receipt, dict) else None
    if (not isinstance(file_receipt, dict) or not isinstance(receipt_file, dict)
            or not isinstance(file_receipt.get("receiptId"), str)
            or not file_receipt["receiptId"]):
        raise ValueError(key + " Pi file upload receipt is missing")
    prepared = row.get("prepared")
    plan = prepared.get("plan") if isinstance(prepared, dict) else None
    if (not isinstance(prepared, dict) or prepared.get("displayText") != marker
            or not isinstance(plan, list) or len(plan) != 4
            or any(not isinstance(part, dict) for part in plan)
            or plan[0] != {"type": "text", "text": marker}
            or [part.get("type") for part in plan] != ["text", "image", "file", "image"]
            or plan[1].get("data") != image_inputs[0].get("data")
            or plan[2].get("receiptId") != file_receipt.get("receiptId")
            or plan[3].get("data") != image_inputs[1].get("data")):
        raise ValueError(key + " frozen submission plan lost text,image,file,image order")
    if len(provider_images) != 2:
        raise ValueError(key + " provider request did not contain two images")
    for expected, observed in zip(actual_canonical, provider_images):
        if (observed["mime"] != expected["mime"] or observed["bytes"] != expected["bytes"]
                or observed["sha256"] != expected["sha256"] or observed["_bytes"] != expected["data"]):
            raise ValueError(key + " provider image bytes, MIME or order differ from RunBinding input")

    validate_file_read(row.get("fileRead"), file_receipt)
    files = row.get("files")
    if not isinstance(files, list) or len(files) != 1:
        raise ValueError(key + " transcript admitted-file list is missing")
    admitted = files[0]
    file_info = admitted.get("file") if isinstance(admitted, dict) else None
    if (not isinstance(file_info, dict)
            or file_info.get("attachmentId") != receipt_file.get("attachmentId")
            or file_info.get("name") != FILE_NAME
            or file_info.get("bytes") != len(FILE_BYTES)
            or admitted.get("contentIndex") != 2
            or admitted.get("precedingImageCount") != 1):
        raise ValueError(key + " transcript admitted-file receipt/order differs")
    expected_display_order = [
            {"kind": "image", "attachmentIndex": 0},
            {"kind": "file", "fileIndex": 0},
            {"kind": "image", "attachmentIndex": 1}]
    input_attachments = input_item.get("attachments", [])
    if (not isinstance(input_attachments, list) or len(input_attachments) != 2
            or any(not isinstance(image, dict) for image in input_attachments)
            or [image.get("data") for image in input_attachments]
            != [image.get("data") for image in image_inputs]
            or row.get("attachmentDisplayOrder") != expected_display_order):
        raise ValueError(key + " transcript attachment display order differs")

    recovered = row.get("recovered")
    recovered_input = recovered.get("input") if isinstance(recovered, dict) else None
    recovered_files = recovered.get("files") if isinstance(recovered, dict) else None
    if (not isinstance(recovered_input, dict)
            or recovered_input.get("id") != input_item.get("id")
            or recovered_input.get("recordId") != input_item.get("recordId")
            or recovered_input.get("reconciliationId") != rpc_id
            or recovered_input.get("turnId") != input_item.get("turnId")
            or recovered_input.get("attachments") != input_item.get("attachments")
            or recovered_input.get("sourcePosition") != input_item.get("sourcePosition")
            or recovered_input.get("sourceSequence") != input_item.get("sourceSequence")
            or recovered_files != files
            or recovered.get("attachmentDisplayOrder") != expected_display_order):
        raise ValueError(key + " reconnect did not preserve input and admitted-file identity")
    if recovered.get("epochBefore") != epoch_before or recovered.get("epochAfter") != epoch_after:
        raise ValueError(key + " reconnect epoch differs from the public probe witness")

    recovered_assistant = recovered.get("assistant")
    if (not isinstance(recovered_assistant, dict)
            or any(recovered_assistant.get(field) != assistant.get(field) for field in (
                "id", "recordId", "sourcePosition", "sourceSequence", "turnId", "kind", "text"))
            or recovered_assistant.get("isFinal") is not True
            or recovered_assistant.get("isProvisional") is not False):
        raise ValueError(key + " reconnect changed the final assistant reply/source")
    validate_file_read(recovered.get("fileRead"), file_receipt)
    if recovered.get("fileRead") != row.get("fileRead"):
        raise ValueError(key + " reconnect changed public file bytes or metadata")

    native_source = validate_native_source(row.get("nativeSource"), rpc_id, input_item)
    recovered_source = validate_native_source(recovered.get("nativeSource"), rpc_id, recovered_input)
    result = {
        "scope": scope,
        "rpcId": rpc_id,
        "imageInputs": image_inputs,
        "fileReceipt": file_receipt,
        "input": input_item,
        "assistant": assistant,
        "files": files,
        "attachmentDisplayOrder": row["attachmentDisplayOrder"],
        "fileRead": row["fileRead"],
        "nativeReceipt": native_source["nativeReceipt"],
        "nativeMessage": native_source["nativeMessage"],
        "recovered": {"input": recovered_input, "assistant": recovered_assistant,
                      "fileRead": recovered["fileRead"], "files": recovered_files,
                      "epochBefore": epoch_before, "epochAfter": epoch_after,
                      "nativeReceipt": recovered_source["nativeReceipt"],
                      "nativeMessage": recovered_source["nativeMessage"]},
        "providerObservation": [{key: image[key] for key in ("mime", "bytes", "sha256")}
                                 for image in provider_images],
    }
    if queue_ready is not None:
        queue_fact = row.get("queueFact")
        if queue_fact != queue_ready["queueFact"]:
            raise ValueError("probe final queue fact differs from the held public queue-ready witness")
        content = queue_fact["message"]["content"]
        if (queue_fact.get("rpcId") != rpc_id or content[2].get("receiptId") != file_receipt.get("receiptId")
                or [content[1].get("data"), content[3].get("data")]
                != [image["data"] for image in image_inputs]):
            raise ValueError("final queue fact differs from its frozen image and file receipt")
        if (row.get("queueItemId") != queue_ready["queueItemId"]
                or row.get("readyFrame") != queue_ready["ready"]):
            raise ValueError("queued receipt item identity or ready frame differs")
        result["queueFact"] = queue_fact
        result["queueItemId"] = queue_ready["queueItemId"]
    return result


def launch_probe(program: Path, config_path: Path, receipt_path: Path, cwd: Path,
                 environment: dict[str, str], output: Path, provider: MixedPiProvider,
                 queued_session_id: str) -> tuple[subprocess.Popen, CapturedStdout, dict]:
    command = [str(program), "--endpoint-config", str(config_path), "--receipt", str(receipt_path)]
    write_json_exclusive(output / "probe-command.json", {
        "argv": command,
        "headlessProgramSha256": None,
        "probeProgramSha256": sha256_file(program),
        "sessionIds": ["direct", queued_session_id],
        "credentialsPassedViaEnvironment": False,
        "requestBodiesPersisted": False,
    })
    stderr_path = output / "probe.stderr.log"
    stderr_stream = stderr_path.open("x", encoding="utf-8")
    process = subprocess.Popen(command, cwd=cwd, env=environment, text=True,
                               stdout=subprocess.PIPE, stderr=stderr_stream,
                               encoding="utf-8", errors="replace", bufsize=1,
                               close_fds=True, start_new_session=True)
    write_json_exclusive(output / "probe-process.json", {"pid": process.pid, "processGroup": process.pid})
    assert process.stdout is not None
    captured = CapturedStdout(process.stdout, output / "probe.stdout.log")
    try:
        captured.wait_ready(process, READY_TIMEOUT_SECONDS)
        if not provider.hold_entered.wait(timeout=5):
            raise ValueError("queue-ready arrived before the provider entered its hold gate")
        lines = captured.snapshot()
        queue_ready = validate_queue_ready_lines(
            lines, expected_session_id=queued_session_id,
            gate_entered=provider.hold_entered.is_set(),
        )
        write_json_exclusive(output / "queue-ready-validated.json", {
            "status": "validated-before-release",
            "scope": queue_ready["scope"],
            "rpcId": queue_ready["rpcId"],
            "queueFact": queue_ready["queueFact"],
            "queueImageSha256": [image["sha256"] for image in queue_ready["images"]],
            "fileReceiptId": queue_ready["fileReceiptId"],
            "providerGateEntered": True,
            "providerGateReleased": False,
        })
        provider.release_hold.set()
        return process, captured, queue_ready
    except BaseException:
        provider.release_hold.set()
        try:
            if process.poll() is None:
                with suppress(subprocess.TimeoutExpired):
                    process.wait(timeout=2)
            if not await_owned_group_exit(process.pid):
                terminate_owned_group(process)
        except OSError as cleanup_error:
            write_json_exclusive(output / "probe-cleanup-error.json", {
                "pid": process.pid, "errorType": type(cleanup_error).__name__,
                "errno": cleanup_error.errno, "clean": False,
            })
        captured.thread.join(timeout=5)
        raise
    finally:
        stderr_stream.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--headless-program", required=True, type=Path,
                        help="existing public tekes-agent-headless executable")
    parser.add_argument("--image-program", required=True, type=Path,
                        help="existing pi_mixed_file_probe executable")
    parser.add_argument("--pi-root", required=True, type=Path,
                        help="fixed Pi 1.3.0 runtime root; this runner never changes it")
    parser.add_argument("--output", required=True, type=Path,
                        help="new evidence directory under the existing AgentKit SSD project")
    args = parser.parse_args()

    output = args.output.expanduser().absolute()
    try:
        project_root = SSD_PROJECT.resolve(strict=True)
        if output.is_symlink():
            parser.error("--output must not be a symlink")
        output = output.resolve(strict=False)
        if not output.is_relative_to(project_root):
            parser.error("--output must be a new directory under the existing AgentKit SSD project")
        if output.exists():
            parser.error("--output must be absent; evidence directories are never overwritten")
        headless = assert_executable(args.headless_program, "--headless-program")
        probe = assert_executable(args.image_program, "--image-program")
        pi_root = args.pi_root.expanduser().resolve(strict=True)
        if not pi_root.is_dir():
            parser.error("--pi-root must be an existing directory")
        pi_version, package_path = read_pi_version(pi_root)
    except (OSError, ValueError, RuntimeError) as error:
        parser.error(str(error))

    output.mkdir(parents=True, mode=0o700)
    owned = output / "owned"
    owned.mkdir(mode=0o700)
    workspace = owned / "workspace"
    workspace.mkdir()
    write_text(workspace / "proof.txt", "owned Pi mixed-file test workspace\n")
    provider = MixedPiProvider(output / "provider-images")
    write_json_exclusive(output / "source-program-hashes.json", {
        "headlessProgram": {"path": str(headless), "sha256": sha256_file(headless)},
        "probeProgram": {"path": str(probe), "sha256": sha256_file(probe)},
        "probeSource": {"path": str(ROOT / "ui/demo/examples/pi_mixed_file_probe.rs"),
                        "sha256": sha256_file(ROOT / "ui/demo/examples/pi_mixed_file_probe.rs")},
        "runner": {"path": str(Path(__file__).resolve()), "sha256": sha256_file(Path(__file__).resolve())},
        "fixture": {"path": str(Path(__file__).with_name("pi_mixed_file_fixture.py").resolve()),
                    "sha256": sha256_file(Path(__file__).with_name("pi_mixed_file_fixture.py").resolve())},
        "piRuntime": {"version": pi_version, "packageJson": str(package_path),
                      "packageJsonSha256": sha256_file(package_path)},
    })

    class QuietProviderHandler(ProviderHandler):
        coordinator = provider

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), QuietProviderHandler)
    # server_close waits for every request handler; final acceptance is written
    # only after the probe session and all provider handlers have exited.
    server.daemon_threads = False
    server_thread = threading.Thread(target=server.serve_forever, name="owned-pi-provider", daemon=True)
    server_thread.start()

    running_probe: subprocess.Popen | None = None
    captured_stdout: CapturedStdout | None = None
    receipt_path = output / "pi-mixed-file-receipt.json"
    outcome = "failed"
    failure = None
    return_code = 1
    candidate_receipt = None
    process_receipts = {}
    try:
        provider_url = f"http://127.0.0.1:{server.server_port}/v1"
        pi_config = prepare_pi(
            pi_root, owned / "pi-data", owned / "pi-home", provider_url,
            strict_environment=True,
        )
        pi_config["expectedVersion"] = EXPECTED_PI_VERSION
        environment = dict(pi_config["environment"], LANG="C.UTF-8")
        commands = []
        for index in range(2):
            alias = "mixed-" + str(index)
            commands.extend([
                {"op": "create", "cwd": str(workspace), "saveAs": alias},
                {"op": "call", "sessionId": alias, "method": "session.selectModel",
                 "args": {"provider": "endpoint-test", "model": "endpoint-test"}},
                {"op": "open", "sessionId": alias},
            ])
        sessions = run_headless(headless, pi_config, commands, output, environment, workspace)
        seed_process = json.loads((output / "pi-seed.exit.json").read_text(encoding="utf-8"))
        process_receipts["headless"] = {"path": str(headless), "sha256": sha256_file(headless),
                                         **seed_process}
        endpoint = {key: value for key, value in pi_config.items()
                    if key not in ("expectedVersion", "commands")}
        endpoint.update(endpointId="pi", sessions=sessions)
        write_json_exclusive(output / "endpoint-config.json", endpoint)
        running_probe, captured_stdout, queue_ready = launch_probe(
            probe, output / "endpoint-config.json", receipt_path, workspace,
            environment, output, provider, sessions[1],
        )
        try:
            status = running_probe.wait(timeout=PROBE_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired as error:
            raise RuntimeError("Pi mixed probe timed out; owned process and evidence retained") from error
        captured_stdout.thread.join(timeout=5)
        process_receipts["probe"] = {"path": str(probe), "sha256": sha256_file(probe),
                                      "pid": running_probe.pid, "exitStatus": status}
        if status != 0:
            raise RuntimeError("Pi mixed probe failed; stdout/stderr and provider evidence retained")
        stdout_lines = captured_stdout.snapshot()
        # Re-run the uniqueness check after EOF so a duplicate ready marker cannot be hidden
        # after the event that released the provider.
        queue_ready = validate_queue_ready_lines(
            stdout_lines, expected_session_id=sessions[1],
            gate_entered=provider.hold_entered.is_set(),
        )
        if not receipt_path.is_file() or receipt_path.is_symlink():
            raise ValueError("Pi mixed probe did not create its new receipt")
        probe_receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        if probe_receipt.get("status") != "completed" or probe_receipt.get("endpointKind") != "pi":
            raise ValueError("Pi mixed probe receipt did not report completed Pi execution")
        if (probe_receipt.get("sessions", {}).get("direct", {}).get("sessionId") != sessions[0]
                or probe_receipt.get("sessions", {}).get("queued", {}).get("sessionId") != sessions[1]):
            raise ValueError("probe sessions differ from the public Pi sessions created by headless")
        if probe_receipt.get("draftIsolation", {}).get("verified") is not True:
            raise ValueError("probe did not verify independent Pi Composer drafts")
        reconnect = probe_receipt.get("reconnect")
        if (not isinstance(reconnect, dict)
                or type(reconnect.get("epochBefore")) is not int
                or type(reconnect.get("epochAfter")) is not int
                or reconnect["epochAfter"] <= reconnect["epochBefore"]):
            raise ValueError("public Pi reconnect did not advance the connection epoch")
        provider_by_marker = provider.validated_observations()
        direct_provider = provider.image_inputs(DIRECT_TEXT)
        queued_provider = provider.image_inputs(QUEUED_TEXT)
        direct = validate_case(
            probe_receipt, "direct", DIRECT_TEXT, sessions[0], direct_provider,
            epoch_before=reconnect["epochBefore"], epoch_after=reconnect["epochAfter"],
        )
        queued = validate_case(
            probe_receipt, "queued", QUEUED_TEXT, sessions[1], queued_provider,
            queue_ready=queue_ready, epoch_before=reconnect["epochBefore"],
            epoch_after=reconnect["epochAfter"],
        )
        if direct["fileReceipt"].get("receiptId") == queued["fileReceipt"].get("receiptId"):
            raise ValueError("direct and queued sessions unexpectedly share one upload receipt")
        # Image decoding and immutable provider-byte matching are part of the final acceptance.
        for key, marker, case in (("direct", DIRECT_TEXT, direct), ("queued", QUEUED_TEXT, queued)):
            observed = provider_by_marker[marker]
            if (observed["imageCount"] != 2 or observed["hasExpectedFilePrompt"] is not True
                    or [image["sha256"] for image in observed["images"]]
                    != [image["sha256"] for image in case["providerObservation"]]):
                raise ValueError(key + " provider evidence does not match the independently checked input")

        hold = probe_receipt.get("queued", {}).get("hold")
        if (not isinstance(hold, dict) or hold.get("input", {}).get("text") != HOLD_TEXT
                or hold.get("assistant", {}).get("text", "").strip() != EXPECTED_REPLY
                or len(provider_by_marker[HOLD_TEXT]["images"]) != 0):
            raise ValueError("queue hold was not a successful text-only public provider round trip")
        hold_native = validate_native_source(
            hold.get("nativeSource"), hold.get("rpcId", ""), hold["input"])

        # The queued queueFact file receipt is learned from the public ready event before
        # release, then must match the completed public upload and its readback.
        if queue_ready["fileReceiptId"] != queued["fileReceipt"].get("receiptId"):
            raise ValueError("queue-ready file receipt differs from the queued public upload")
        candidate_receipt = {
            "schemaVersion": 1,
            "status": "validated-awaiting-owned-cleanup",
            "endpointKind": "pi",
            "piVersion": pi_version,
            "execution": {"desktop": False, "paidModel": False,
                          "privateSessionDatabaseRead": False, "provider": "synthetic-loopback",
                          "headlessPublicSessionCreation": True},
            "direct": direct,
            "queued": dict(queued, hold={
                "rpcId": hold["rpcId"], "input": hold["input"],
                "assistant": hold["assistant"], "nativeReceipt": hold_native["nativeReceipt"],
                "nativeMessage": hold_native["nativeMessage"],
                "providerObservation": provider_by_marker[HOLD_TEXT],
            }),
            "queueReady": {"scope": queue_ready["scope"], "rpcId": queue_ready["rpcId"],
                           "queueFact": queue_ready["queueFact"], "gateEntered": True,
                           "releasedAfterStrictValidation": True},
            "providerRequestCount": len(provider_by_marker),
            "automaticRetryObserved": False,
            "reconnect": reconnect,
        }
        outcome = "validated"
        return_code = 0
    except (OSError, ValueError, RuntimeError, KeyError, TypeError, json.JSONDecodeError,
            subprocess.SubprocessError, TimeoutError) as error:
        failure = type(error).__name__ + ": " + str(error)
        with suppress(FileExistsError):
            write_text(output / "failure.txt", failure + "\n")
        return_code = 1
    finally:
        provider.release_hold.set()
        leaked_probe_group = False
        if running_probe is not None:
            leaked_probe_group = not await_owned_group_exit(running_probe.pid)
            if running_probe.poll() is None or leaked_probe_group:
                terminate_owned_group(running_probe)
            if running_probe.poll() is None:
                with suppress(subprocess.TimeoutExpired):
                    running_probe.wait(timeout=5)
        leaked_server = False
        with suppress(Exception):
            server.shutdown()
        server.server_close()
        server_thread.join(timeout=5)
        if server_thread.is_alive():
            leaked_server = True
        if captured_stdout is not None:
            captured_stdout.thread.join(timeout=5)
            if captured_stdout.thread.is_alive():
                leaked_server = True
        if running_probe is not None:
            process_receipts["probe"] = {
                "pid": running_probe.pid,
                "sessionId": running_probe.pid,
                "exitStatus": running_probe.poll(),
                "ownedSessionHadDescendantsAtCleanup": leaked_probe_group,
            }
        server_stopped = not server_thread.is_alive() and not leaked_server
        cleanup_ok = (server_stopped
                      and all(value.get("exitStatus") == 0 for value in process_receipts.values())
                      and not any(value.get("ownedSessionHadDescendantsAtCleanup") for value in process_receipts.values()))
        provider_final = None
        if outcome == "validated" and cleanup_ok:
            try:
                provider_final = provider.validate_and_save_images()
                candidate_receipt["status"] = "accepted"
                candidate_receipt["execution"]["ownedProcessExits"] = process_receipts
                candidate_receipt["execution"]["providerServerStopped"] = True
                candidate_receipt["provider"] = provider_final
                candidate_receipt["providerRequestCount"] = provider_final["requestCount"]
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                failure = "final provider validation: " + type(error).__name__ + ": " + str(error)
                outcome = "failed"
                return_code = 1
        elif outcome == "validated":
            failure = "owned Pi process/session or provider server did not stop cleanly"
            outcome = "failed"
            return_code = 1
        with suppress(FileExistsError):
            write_json_exclusive(output / "owned-process-exits.json", {
                "processes": process_receipts,
                "providerServerStopped": server_stopped,
                "providerHoldReleased": provider.release_hold.is_set(),
                "processInventoryComplete": running_probe is not None or not (output / "probe-process.json").exists(),
                "ownedGroupsClean": (not any(value.get("ownedSessionHadDescendantsAtCleanup") for value in process_receipts.values()))
                    if process_receipts and (running_probe is not None or not (output / "probe-process.json").exists()) else None,
            })
        with suppress(FileExistsError):
            write_json_exclusive(output / "provider-observations.json", {
                "attemptCount": provider.attempts,
                "requests": provider.observations,
                "failures": provider.failures,
                "requestBodiesPersisted": False,
            })
        if outcome == "validated" and candidate_receipt is not None and provider_final is not None:
            try:
                write_json_exclusive(output / "accepted-receipt.json", candidate_receipt)
                outcome = "accepted"
                return_code = 0
            except OSError as error:
                failure = "could not persist final acceptance receipt: " + str(error)
                outcome = "failed"
                return_code = 1
        if outcome != "accepted" and failure is None:
            failure = "probe did not reach final accepted status"
        if outcome != "accepted":
            with suppress(FileExistsError):
                write_text(output / "failure.txt", failure + "\n")
            return_code = 1
    return return_code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("PI_MIXED_FILE_INTERRUPTED; owned evidence retained", file=sys.stderr)
        raise SystemExit(130)
