#!/usr/bin/env python3
"""Transparent, metadata-only TCP observer for one owned loopback Kernel run.

The relay never terminates TLS, rewrites bytes, or stores message bodies or
headers. Its passive decoder understands Content-Length JSON and WebSocket
frames; everything else is explicitly labelled unknown.
"""
from __future__ import annotations

import asyncio
from contextlib import contextmanager
import hashlib
import ipaddress
import json
import re
import threading
import time
from urllib.parse import urlsplit


_SAFE_VALUE = re.compile(r"^[A-Za-z0-9_./:-]{1,64}$")
_LABEL_KEYS = {"op", "type", "event", "kind", "frametype", "status", "method"}
_ID_KEYS = {"id", "requestid", "rpcid", "callid", "sessionid", "threadid", "turnid",
            "messageid", "streamid", "sequence", "parentid"}
_BOOLEAN_KEYS = {"accepted", "durable", "committed"}


def _loopback_host(host: str) -> bool:
    if not isinstance(host, str) or not host:
        return False
    try:
        return ipaddress.ip_address(host.split("%", 1)[0]).is_loopback
    except ValueError:
        return host.lower() == "localhost"


def parse_loopback_http_url(url: str) -> tuple[str, int]:
    parsed = urlsplit(url)
    try:
        port = parsed.port
    except ValueError as exc:
        raise ValueError("owned Kernel URL has an invalid port") from exc
    if (parsed.scheme != "http" or not _loopback_host(parsed.hostname or "")
            or port is None or not 1 <= port <= 65535
            or parsed.username is not None or parsed.password is not None):
        raise ValueError("wire probe only accepts explicit loopback http Kernel URLs")
    return parsed.hostname or "", port


def _safe_fields(value: object) -> dict[str, object]:
    """Return a small allowlisted projection; never retain arbitrary strings."""
    result: dict[str, object] = {}
    labels = []
    def visit(node, depth):
        if depth > 6:
            return
        if isinstance(node, list):
            for item in node[:64]:
                visit(item, depth + 1)
            return
        if not isinstance(node, dict):
            return
        for key, item in node.items():
            if not isinstance(key, str):
                continue
            normalized = re.sub(r"[^a-z]", "", key.lower())
            if normalized in _LABEL_KEYS and isinstance(item, str) and _SAFE_VALUE.fullmatch(item):
                if item not in labels and len(labels) < 16:
                    labels.append(item)
            elif normalized in _ID_KEYS and isinstance(item, (str, int)) and not isinstance(item, bool):
                result.setdefault(normalized, hashlib.sha256(str(item).encode()).hexdigest()[:16])
            elif normalized in _BOOLEAN_KEYS and isinstance(item, bool):
                result.setdefault(normalized, item)
            elif isinstance(item, (dict, list)):
                visit(item, depth + 1)
    visit(value, 0)
    if labels:
        result["labels"] = labels
        result["label"] = labels[0]
    return result


def _safe_json(payload: bytes) -> tuple[dict[str, object], str]:
    try:
        decoded = payload.decode("utf-8")
        value = json.loads(decoded)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return {}, "unknown"
    if not isinstance(value, dict):
        return {}, "unknown"
    return _safe_fields(value), "json"


class PassiveObserver:
    """Incremental HTTP/WS observer. feed() never controls forwarding."""

    def __init__(self, emit, direction: str):
        self.emit = emit
        self.direction = direction
        self.buffer = bytearray()
        self.mode = "http"
        self.http_state = "headers"
        self.http_started_ns: int | None = None
        self.http_body_length = 0
        self.http_meta: dict[str, object] = {}
        self.fragment_opcode: int | None = None
        self.fragment_started_ns: int | None = None
        self.fragments = bytearray()
        self.opaque = False

    def _event(self, transport, kind, timestamp_ns, **fields):
        self.emit({"monotonicNs": timestamp_ns, "direction": self.direction,
                   "transport": transport, "kind": kind, **fields})

    def feed(self, data: bytes, timestamp_ns: int | None = None):
        if not data:
            return
        now = timestamp_ns if timestamp_ns is not None else time.monotonic_ns()
        self.buffer.extend(data)
        if self.opaque:
            self.buffer.clear()
            return
        while True:
            before = len(self.buffer)
            if self.mode == "http":
                self._http(now)
            else:
                self._websocket(now)
            if len(self.buffer) == before:
                break

    def _http(self, now):
        if self.http_state == "headers":
            boundary = self.buffer.find(b"\r\n\r\n")
            if boundary < 0:
                return
            raw = bytes(self.buffer[:boundary + 4])
            del self.buffer[:boundary + 4]
            self.http_started_ns = now
            self.http_body_length = 0
            self.http_meta = {"headersBytes": len(raw)}
            try:
                lines = raw[:-4].split(b"\r\n")
                first = lines[0].decode("ascii")
                parts = first.split(" ")
                if len(parts) < 2:
                    raise ValueError
                headers = {}
                for line in lines[1:]:
                    name, value = line.split(b":", 1)
                    headers[name.decode("ascii").strip().lower()] = value.decode("latin1").strip().lower()
                if "transfer-encoding" in headers:
                    raise LookupError
                length = int(headers.get("content-length", "0"))
                if length < 0 or length > 16 * 1024 * 1024:
                    raise LookupError
                websocket = headers.get("upgrade") == "websocket"
                is_response = parts[0].startswith("HTTP/")
                method = "response" if is_response else parts[0].upper()
                target_hash = hashlib.sha256(parts[1].encode("utf-8", "replace")).hexdigest()[:16]
                category = "websocket-upgrade" if websocket else "http"
                self.http_meta.update({"method": method, "targetHash": target_hash,
                                       "routeCategory": category, "bodyBytes": length})
                self.http_body_length = length
                self.http_state = "body"
                if websocket:
                    self.http_meta["switchToWebSocket"] = True
            except LookupError:
                self._event("http", "unknown", now, reason="unsupported-transfer-encoding",
                            bytes=len(raw))
                self.opaque = True
                return
            except (ValueError, UnicodeError, IndexError):
                self._event("http", "unknown", now, reason="incomplete-or-unsupported-headers",
                            bytes=len(raw))
                self.opaque = True
                return
        if self.http_state == "body":
            if len(self.buffer) < self.http_body_length:
                return
            body = bytes(self.buffer[:self.http_body_length])
            del self.buffer[:self.http_body_length]
            fields, encoding = _safe_json(body) if body else ({}, "empty")
            meta = self.http_meta
            switch = bool(meta.pop("switchToWebSocket", False))
            self._event("http", "message", now, startedMonotonicNs=self.http_started_ns,
                        encoding=encoding, fields=fields, **meta)
            self.http_state = "headers"
            self.http_meta = {}
            self.http_started_ns = None
            if switch:
                self.mode = "websocket"

    def _websocket(self, now):
        while len(self.buffer) >= 2:
            first, second = self.buffer[0], self.buffer[1]
            fin, rsv, opcode = bool(first & 0x80), first & 0x70, first & 0x0f
            masked = bool(second & 0x80)
            length = second & 0x7f
            offset = 2
            if length == 126:
                if len(self.buffer) < 4:
                    return
                length = int.from_bytes(self.buffer[2:4], "big"); offset = 4
            elif length == 127:
                if len(self.buffer) < 10:
                    return
                length = int.from_bytes(self.buffer[2:10], "big"); offset = 10
                if length >> 63:
                    self._ws_unknown(now, "invalid-length", min(len(self.buffer), 10)); return
            if length > 16 * 1024 * 1024:
                self._ws_unknown(now, "frame-too-large", offset); return
            mask = b""
            if masked:
                if len(self.buffer) < offset + 4:
                    return
                mask = bytes(self.buffer[offset:offset + 4]); offset += 4
            if len(self.buffer) < offset + length:
                return
            payload = bytes(self.buffer[offset:offset + length])
            del self.buffer[:offset + length]
            if masked:
                payload = bytes(value ^ mask[i & 3] for i, value in enumerate(payload))
            if rsv:
                self._event("websocket", "unknown", now, reason="reserved-bits-set",
                            opcode=opcode, payloadBytes=length)
                continue
            if opcode >= 8:
                self._event("websocket", "control", now, opcode=opcode,
                            payloadBytes=length)
                continue
            if opcode in (1, 2):
                if self.fragment_opcode is not None:
                    self._event("websocket", "unknown", now, reason="nested-fragment")
                    self.fragment_opcode = None; self.fragments.clear()
                if fin:
                    self._ws_message(opcode, payload, now, now)
                else:
                    self.fragment_opcode = opcode
                    self.fragment_started_ns = now
                    self.fragments = bytearray(payload)
            elif opcode == 0 and self.fragment_opcode is not None:
                self.fragments.extend(payload)
                if len(self.fragments) > 16 * 1024 * 1024:
                    self._event("websocket", "unknown", now, reason="fragmented-message-too-large")
                    self.fragment_opcode = None; self.fragments.clear()
                elif fin:
                    self._ws_message(self.fragment_opcode, bytes(self.fragments),
                                     self.fragment_started_ns or now, now)
                    self.fragment_opcode = None; self.fragments.clear(); self.fragment_started_ns = None
            else:
                self._event("websocket", "unknown", now, reason="unexpected-opcode",
                            opcode=opcode, payloadBytes=length)

    def _ws_message(self, opcode, payload, started, now):
        if opcode != 1:
            self._event("websocket", "unknown", now, startedMonotonicNs=started,
                        reason="binary-message", payloadBytes=len(payload))
            return
        fields, encoding = _safe_json(payload)
        self._event("websocket", "message", now, startedMonotonicNs=started,
                    encoding=encoding, payloadBytes=len(payload), fields=fields)

    def _ws_unknown(self, now, reason, consumed):
        del self.buffer[:consumed]
        self._event("websocket", "unknown", now, reason=reason,
                    bytes=consumed)


class KernelWireProbe:
    """Loopback-only TCP relay that records a redacted observation JSONL."""

    def __init__(self, upstream_url: str, observations_path):
        self.upstream_host, self.upstream_port = parse_loopback_http_url(upstream_url)
        self.observations_path = observations_path
        self._ready = threading.Event()
        self._stopped = threading.Event()
        self._error = None
        self._loop = None
        self._server = None
        self._file_lock = threading.Lock()
        self._file = None
        self._thread = None
        self.url = None
        self._connection_serial = 0

    def _emit(self, record):
        with self._file_lock:
            self._file.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
            self._file.flush()

    async def _serve(self):
        self._server = await asyncio.start_server(self._handle, "127.0.0.1", 0)
        port = self._server.sockets[0].getsockname()[1]
        self.url = f"http://127.0.0.1:{port}"
        self._ready.set()
        async with self._server:
            await self._server.serve_forever()

    async def _handle(self, client_reader, client_writer):
        peer = client_writer.get_extra_info("peername")
        if not peer or not _loopback_host(str(peer[0])):
            client_writer.close()
            await client_writer.wait_closed()
            return
        try:
            upstream_reader, upstream_writer = await asyncio.open_connection(
                self.upstream_host, self.upstream_port)
        except OSError:
            client_writer.close()
            await client_writer.wait_closed()
            return
        self._connection_serial += 1
        connection_id = self._connection_serial
        def emit_connection(record):
            self._emit(dict(record, connectionId=connection_id))
        observers = {"client-to-kernel": PassiveObserver(emit_connection, "client-to-kernel"),
                     "kernel-to-client": PassiveObserver(emit_connection, "kernel-to-client")}

        async def pump(reader, writer, direction):
            try:
                while True:
                    chunk = await reader.read(65536)
                    if not chunk:
                        break
                    writer.write(chunk)
                    await writer.drain()
                    observers[direction].feed(chunk, time.monotonic_ns())
            except (ConnectionError, OSError, asyncio.CancelledError):
                pass
            finally:
                try:
                    writer.write_eof()
                except (AttributeError, OSError, RuntimeError):
                    pass

        tasks = [asyncio.create_task(pump(client_reader, upstream_writer, "client-to-kernel")),
                 asyncio.create_task(pump(upstream_reader, client_writer, "kernel-to-client"))]
        done, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
        for task in pending:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        upstream_writer.close(); client_writer.close()
        await asyncio.gather(upstream_writer.wait_closed(), client_writer.wait_closed(),
                             return_exceptions=True)

    def _thread_main(self):
        try:
            self._file = open(self.observations_path, "x", encoding="utf-8")
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            self._loop.create_task(self._serve())
            self._loop.run_forever()
        except BaseException as exc:
            self._error = exc
            self._ready.set()
        finally:
            if self._loop is not None:
                pending = asyncio.all_tasks(self._loop)
                for task in pending:
                    task.cancel()
                if pending:
                    self._loop.run_until_complete(asyncio.gather(*pending, return_exceptions=True))
                self._loop.close()
            if self._file is not None:
                self._file.close()
            self._stopped.set()

    def start(self, timeout=5):
        self._thread = threading.Thread(target=self._thread_main, name="kernel-wire-probe", daemon=True)
        self._thread.start()
        if not self._ready.wait(timeout):
            self.stop()
            raise TimeoutError("wire probe listener did not become ready")
        if self._error:
            raise RuntimeError("wire probe failed to start") from self._error
        return self

    def stop(self, timeout=5):
        if self._loop is not None and self._loop.is_running():
            self._loop.call_soon_threadsafe(self._loop.stop)
        if self._thread is not None:
            self._thread.join(timeout)
            if self._thread.is_alive():
                raise TimeoutError("wire probe thread did not stop")

    def __enter__(self):
        return self.start()

    def __exit__(self, *_):
        self.stop()


@contextmanager
def proxied_native_fixture(original_factory, observations_path, *args, **kwargs):
    """Use direct Native fixture for seed; proxy only the resumed measurement."""
    with original_factory(*args, **kwargs) as fixture:
        resume = kwargs.get("resume_receipt") is not None
        if not resume:
            yield fixture
            return
        probe = KernelWireProbe(fixture["url"], observations_path).start()
        try:
            measured = dict(fixture)
            measured["url"] = probe.url
            yield measured
        finally:
            # Retire the observer before the caller retires the owned Kernel.
            probe.stop()

