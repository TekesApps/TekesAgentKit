#!/usr/bin/env python3
"""Transparent stdio wrapper that gates Pi responses after one marked prompt.

Use an explicit argv after ``--``; no shell is involved. The wrapper never
interprets a timeout as Unknown, retries a request, or fabricates an ACK.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import select
import threading
import time
from typing import BinaryIO

MAX_LINE = 64 * 1024 * 1024  # Match AgentKit's Pi RPC line bound.
DEFAULT_GATE_BYTES = 4 * 1024 * 1024


class GateFailure(RuntimeError):
    pass


def emit(path: Path, event: dict) -> None:
    """Append a small receipt; never include protocol bodies or credentials."""
    with path.open("ab", buffering=0) as stream:
        stream.write(json.dumps(event, sort_keys=True, separators=(",", ":")).encode() + b"\n")


def marked_prompt(raw: bytes, marker: str) -> bool:
    try:
        message = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return False
    if not isinstance(message, dict) or message.get("type") != "prompt":
        return False
    # Pi's RPC schema uses `message`; accept `input` as well for the public
    # protocol versions that name the submitted prompt payload that way.
    payload = message.get("input", message.get("message"))
    if isinstance(payload, str):
        return marker in payload
    return marker in json.dumps(payload, ensure_ascii=False, separators=(",", ":"))


def stop_child(child: subprocess.Popen) -> None:
    if child.poll() is None:
        child.terminate()
    try:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=2)


def write_all(stream: BinaryIO, data: bytes) -> None:
    remaining = memoryview(data)
    while remaining:
        written = stream.write(remaining)
        if written is None or written <= 0:
            raise BrokenPipeError("child input made no progress")
        remaining = remaining[written:]
    stream.flush()


def pump_stdin(source: BinaryIO, child: BinaryIO, child_process_for_stop: subprocess.Popen,
              done: threading.Event, child_done: threading.Event, lock: threading.Lock, receipt: Path,
              marker: str, state: dict, stop: threading.Event) -> None:
    """Inspect client→Pi request lines, forwarding every accepted byte exactly."""
    pending = bytearray()

    def forward(line: bytes) -> None:
        if marked_prompt(line, marker):
            with lock:
                state["prompt_count"] += 1
                if state["prompt_count"] != 1:
                    raise GateFailure("more than one marked client prompt was emitted")
                state["gated"] = True
                emit(receipt, {"event": "gate_ready", "promptOrdinal": 1})
                write_all(child, line)
                emit(receipt, {"event": "mutation_forwarded", "promptOrdinal": 1})
        else:
            write_all(child, line)

    try:
        while not child_done.is_set():
            ready, _, _ = select.select([source], [], [], 0.1)
            if not ready:
                continue
            chunk = os.read(source.fileno(), 65536)
            if not chunk:
                if pending:
                    forward(bytes(pending))
                break
            pending.extend(chunk)
            while True:
                newline = pending.find(b"\n")
                if newline < 0:
                    if len(pending) > MAX_LINE:
                        raise GateFailure("Pi RPC request line exceeded the protocol line bound")
                    break
                if newline + 1 > MAX_LINE:
                    raise GateFailure("Pi RPC request line exceeded the protocol line bound")
                line = bytes(pending[:newline + 1])
                del pending[:newline + 1]
                forward(line)
    except (BrokenPipeError, OSError, ValueError, GateFailure) as error:
        if isinstance(error, GateFailure):
            state["error"] = error
            stop.set()
            stop_child(child_process_for_stop)
    finally:
        done.set()
        try:
            child.close()
        except (BrokenPipeError, OSError, ValueError):
            pass


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--marker", required=True, help="unique marker in exactly one intended prompt")
    parser.add_argument("--receipt", required=True, type=Path, help="new owned JSONL receipt path")
    parser.add_argument("--release-file", required=True, type=Path,
                        help="create this empty file to release withheld Pi stdout bytes")
    parser.add_argument("--max-gate-bytes", type=int, default=DEFAULT_GATE_BYTES)
    parser.add_argument("command", nargs=argparse.REMAINDER,
                        help="explicit Pi launch argv after --")
    args = parser.parse_args(argv)
    if not args.marker or len(args.marker) < 12:
        parser.error("--marker must be a unique value of at least 12 characters")
    if not 0 < args.max_gate_bytes <= MAX_LINE * 4:
        parser.error("--max-gate-bytes is outside the bounded range")
    if args.command[:1] == ["--"]:
        command = args.command[1:]
    else:
        command = args.command
    if not command:
        parser.error("an explicit launch command is required after --")
    # PiConnector probes the configured argv with an appended --version before
    # starting RPC. Preserve that probe exactly and do not consume the RPC
    # receipt/release paths during version detection.
    if "--version" in command:
        os.execvp(command[0], command)
    # {pid} keeps each owned RPC child/reconnect on a fresh evidence pair,
    # while the spawning host can derive the same paths from its child PID.
    receipt = Path(str(args.receipt).replace("{pid}", str(os.getpid()))).expanduser().absolute()
    release = Path(str(args.release_file).replace("{pid}", str(os.getpid()))).expanduser().absolute()
    if receipt.exists() or release.exists() or not receipt.parent.is_dir() or not release.parent.is_dir():
        parser.error("receipt and release paths must be new files beneath existing owned directories")
    if receipt == release:
        parser.error("receipt and release paths must differ")
    try:
        receipt.open("xb").close()
    except FileExistsError:
        parser.error("receipt path already exists")

    child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None, bufsize=0)
    assert child.stdin is not None and child.stdout is not None
    stdin_done = threading.Event()
    child_done = threading.Event()
    stop_requested = threading.Event()
    lock = threading.Lock()
    state = {"gated": False, "released": False, "prompt_count": 0, "error": None}
    buffered = bytearray()
    overflow = threading.Event()
    stdout_error: list[BaseException] = []
    closed = threading.Event()

    def release_monitor() -> None:
        while not closed.wait(0.01):
            if not release.exists():
                continue
            try:
                if release.stat().st_size != 0:
                    stdout_error.append(GateFailure("release signal must be an empty file"))
                    overflow.set()
                    stop_child(child)
                    return
            except OSError as error:
                stdout_error.append(error)
                overflow.set()
                stop_child(child)
                return
            with lock:
                if not state["gated"]:
                    continue
                if not state["released"]:
                    try:
                        sys.stdout.buffer.write(buffered)
                        sys.stdout.buffer.flush()
                    except (BrokenPipeError, OSError) as error:
                        stdout_error.append(error)
                        overflow.set()
                        return
                    size = len(buffered)
                    buffered.clear()
                    state["gated"] = False
                    state["released"] = True
                    emit(receipt, {"event": "released", "bufferedBytes": size})
            return

    stdin_thread = threading.Thread(target=pump_stdin,
        args=(sys.stdin.buffer, child.stdin, child, stdin_done, child_done, lock, receipt,
              args.marker, state, stop_requested), daemon=True)
    stdin_thread.start()
    releaser = threading.Thread(target=release_monitor, daemon=True)
    releaser.start()
    try:
        while True:
            line = child.stdout.readline(MAX_LINE + 1)
            if not line:
                break
            if len(line) > MAX_LINE:
                raise GateFailure("Pi RPC line exceeded the protocol line bound")
            with lock:
                if state["gated"]:
                    if len(buffered) + len(line) > args.max_gate_bytes:
                        overflow.set()
                        raise GateFailure("withheld Pi stdout exceeded the configured memory bound")
                    buffered.extend(line)
                else:
                    sys.stdout.buffer.write(line)
                    sys.stdout.buffer.flush()
        if stop_requested.is_set():
            raise state["error"] or GateFailure("client input pump failed")
        if stdout_error:
            raise GateFailure("release control failed") from stdout_error[0]
        try:
            status = child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            stop_child(child)
            raise GateFailure("Pi process did not exit after stdout closed")
        prompt_count = state["prompt_count"]
        gated = state["gated"]
        released = state["released"]
        if prompt_count != 1:
            emit(receipt, {"event": "unarmed_exit", "promptCount": prompt_count, "exitStatus": status})
            return status
        if gated and buffered:
            # Child exited while gated: never erase or silently flush the captured evidence.
            emit(receipt, {"event": "child_exit_while_gated", "bufferedBytes": len(buffered), "exitStatus": status})
            return status or 70
        if gated and not released:
            emit(receipt, {"event": "child_exit_while_gated", "bufferedBytes": 0, "exitStatus": status})
            return status or 70
        emit(receipt, {"event": "exit", "exitStatus": status, "promptCount": prompt_count})
        return status
    except (OSError, GateFailure) as error:
        emit(receipt, {"event": "gate_failure", "errorType": type(error).__name__})
        stop_child(child)
        return 70
    finally:
        closed.set()
        child_done.set()
        releaser.join(timeout=1)
        stdin_thread.join(timeout=1)
        if child.poll() is None:
            stop_child(child)
        emit(receipt, {"event": "process_exit", "exitStatus": child.returncode,
                       "promptCount": state["prompt_count"], "released": state["released"],
                       "bufferedBytesAtExit": len(buffered)})


def main() -> int:
    return run()


if __name__ == "__main__":
    raise SystemExit(main())
