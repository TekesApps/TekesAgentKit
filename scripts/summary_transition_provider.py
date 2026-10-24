"""Local synthetic provider gates for Summary transition acceptance.

This module only holds a matching HTTP request until a test runner has captured
the intended public presentation. All non-matching traffic is delegated to the
existing owned endpoint fixture without changing its behavior.
"""

from __future__ import annotations

import io
import json
import math
import threading
import time
from typing import Mapping, MutableSequence

from owned_endpoint_provider import provider_handler


_MAX_TIMEOUT_SECONDS = 30.0


class HeldFailureGate:
    """A bounded, one-way gate that returns a controlled HTTP 400 when released."""

    def __init__(self, marker: str, timeout_seconds: float = 30):
        if not isinstance(marker, str) or not marker.strip():
            raise ValueError("marker must be a non-empty string")
        if (isinstance(timeout_seconds, bool)
                or not isinstance(timeout_seconds, (int, float))
                or not math.isfinite(timeout_seconds)
                or not 0 < timeout_seconds <= _MAX_TIMEOUT_SECONDS):
            raise ValueError("timeout_seconds must be positive and at most 30 seconds")
        self.marker = marker
        self.timeout_seconds = float(timeout_seconds)
        self.entered = threading.Event()
        self._released_event = threading.Event()
        self._lock = threading.Lock()
        self._state = "pending"
        self._deadline: float | None = None
        self._request_count = 0

    def release_after_capture(self) -> None:
        """Release the held request after the caller has verified its capture."""
        with self._lock:
            if not self.entered.is_set():
                raise RuntimeError("cannot release before the matching request enters the gate")
            if self._state != "entered":
                raise RuntimeError("gate is no longer awaiting an explicit release")
            self._state = "explicitly_released"
            self._released_event.set()

    def abort(self) -> None:
        """Unblock a held request during cleanup; this never counts as acceptance."""
        with self._lock:
            if self._state in ("pending", "entered"):
                self._state = "aborted"
                self._released_event.set()

    def receipt(self) -> dict[str, int | bool]:
        """Return body-free evidence about the gate outcome."""
        with self._lock:
            return {
                "requestCount": self._request_count,
                "entered": self.entered.is_set(),
                "explicitlyReleased": self._state == "explicitly_released",
                "aborted": self._state == "aborted",
                "timedOut": self._state == "timed_out",
            }

    def _wait_for_release(self) -> str:
        with self._lock:
            self._request_count += 1
            if self._state == "pending":
                self._state = "entered"
                self._deadline = time.monotonic() + self.timeout_seconds
                self.entered.set()
            if self._state != "entered":
                return self._state
            assert self._deadline is not None
            remaining = max(0.0, self._deadline - time.monotonic())

        if self._released_event.wait(remaining):
            with self._lock:
                return self._state

        with self._lock:
            if self._state == "entered":
                self._state = "timed_out"
                self._released_event.set()
            return self._state


def provider_with_failure_gates(*, workspace, requests: MutableSequence[str],
                                gates: Mapping[str, HeldFailureGate]):
    """Return an owned endpoint provider handler with request-local held errors.

    Request bodies are inspected once. A matching request is recorded as a path
    and held. Other requests have their already-read bytes replayed to the
    original handler, which remains the sole owner of normal parsing, request
    recording, and SSE output.
    """
    if not callable(workspace):
        raise ValueError("workspace must be a getter")
    if not isinstance(requests, MutableSequence):
        raise ValueError("requests must be a mutable sequence of request paths")
    if not isinstance(gates, Mapping) or not gates:
        raise ValueError("gates must be a non-empty mapping of kind to HeldFailureGate")
    markers: set[str] = set()
    for kind, gate in gates.items():
        if not isinstance(kind, str) or not kind.strip():
            raise ValueError("gate kinds must be non-empty strings")
        if not isinstance(gate, HeldFailureGate):
            raise ValueError("each gate must be a HeldFailureGate")
        if gate.marker in markers:
            raise ValueError("gate markers must be unique")
        markers.add(gate.marker)

    original_handler = provider_handler(workspace=workspace, requests=requests)
    gates_by_kind = dict(gates)

    class TransitionProvider(original_handler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            original_rfile = self.rfile
            body = original_rfile.read(int(self.headers.get("Content-Length", "0")))
            body_text = body.decode("utf-8", errors="replace")
            matching_gate = next(
                (gate for gate in gates_by_kind.values()
                 if gate.marker in body_text),
                None,
            )
            if matching_gate is None:
                self.rfile = io.BytesIO(body)
                try:
                    original_handler.do_POST(self)
                finally:
                    self.rfile.close()
                    self.rfile = original_rfile
                return

            requests.append(self.path)
            state = matching_gate._wait_for_release()
            if state == "explicitly_released":
                payload = {
                    "error": {
                        "message": f"{matching_gate.marker}: controlled invalid request",
                        "type": "invalid_request_error",
                        "code": "invalid_request_error",
                    }
                }
                status = 400
            elif state == "timed_out":
                payload = {"error": {"message": "failure gate timed out", "type": "timeout_error"}}
                status = 504
            else:
                payload = {"error": {"message": "failure gate aborted", "type": "aborted_error"}}
                status = 503

            encoded = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    return TransitionProvider
