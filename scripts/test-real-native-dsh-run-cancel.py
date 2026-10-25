#!/usr/bin/env python3
"""Run one frozen Native 0.9.3 or DSH 0.2.1-rc.2 RunController cancel slice.

This runner uses the existing public headless consumer and owned installed
runtime fixtures. It never builds, installs, contacts a real provider, or opens
a desktop. Each invocation accepts one endpoint so Cargo and runtime work can
be serialized by the stage coordinator.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading
import time
from typing import Any

from native_dsh_run_cancel_validation import LifecycleEvidenceError, validate_cancel
from runtime_provenance import kernel_programs

SSD = Path("/Volumes/HP P900/dev/TekesAgentKit")
CONSUMER_SHA = "9a6a919215dc30c51df3b639188acb4783fd26f164592d15145a7f9e24734447"
FROZEN_PRODUCT_HEAD = "5b7c84361206b45ac381b10437097d3b76c322c1"
FROZEN_RUNNER_HEAD = "7d773ea0d89eb2bc5a4228df6f6409a05644ec3c"
KERNEL_VERSION = "0.9.3"
DSH_VERSION = "0.2.1-rc.2"
MAX_BODY = 16 * 1024 * 1024
MAX_LINE = 8 * 1024 * 1024
PROMPT = "AGENTKIT_NATIVE_DSH_CANCEL_PROOF_20261026 submit once and keep this run held."
DRAFT = "Preserve this draft while the original Run is cancelled."


def digest(path: Path) -> str:
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def write_new(path: Path, value: Any) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2)
        stream.write("\n")


def import_file(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load fixed helper {path.name}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


HERE = Path(__file__).resolve().parent
PI_RUNNER = import_file("_fixed_pi_lifecycle_runner", HERE / "test-real-pi-run-lifecycle.py")
NATIVE_FIXTURE = import_file("_fixed_native_owned_fixture", HERE / "native_owned_fixture.py")
DSH_FIXTURE = import_file("_fixed_dsh_owned_fixture", HERE / "dsh_owned_fixture.py")
DSH_CLASSIFIER = import_file("_fixed_dsh_mixed_fixture", HERE / "native_dsh_mixed_fixture.py")
ProcessCapture = PI_RUNNER.ProcessCapture
process_group_snapshot = PI_RUNNER.process_group_snapshot
stop_owned_group = PI_RUNNER.stop_owned_group


def client_closed(connection: socket.socket) -> bool:
    import select
    try:
        ready, _, _ = select.select([connection], [], [], 0)
        return bool(ready and connection.recv(1, socket.MSG_PEEK | socket.MSG_DONTWAIT) == b"")
    except (BlockingIOError, ConnectionResetError, OSError):
        return True


def wait_closed(connection: socket.socket, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if client_closed(connection):
            return True
        time.sleep(0.025)
    return client_closed(connection)


def provider_handler(endpoint: str, state: dict, lock: threading.Lock):
    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def setup(self):
            super().setup()
            with lock:
                state["activeHandlers"] += 1

        def finish(self):
            try:
                super().finish()
            finally:
                with lock:
                    state["activeHandlers"] -= 1

        def do_POST(self):
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= MAX_BODY:
                    raise ValueError("provider request length outside bound")
                raw = self.rfile.read(length)
                body = json.loads(raw)
                if not isinstance(body, dict):
                    raise ValueError("provider request body is not an object")
                kind, title_text = (DSH_CLASSIFIER.classify_dsh_request(body)
                                    if endpoint == "dsh" else ("prompt", None))
                messages = body.get("input" if endpoint == "native" else "messages")
                marker_match = PROMPT in json.dumps(messages, ensure_ascii=False)
                with lock:
                    state["requests"].append({"path": self.path, "kind": kind,
                        "titlePrompt": bool(kind == "title"), "titleTextSha256":
                        hashlib.sha256((title_text or "").encode()).hexdigest() if kind == "title" else None,
                        "markerPresent": marker_match, "bodySha256": hashlib.sha256(raw).hexdigest()})
                    if kind == "title":
                        state["titleRequests"] += 1
                    elif marker_match:
                        state["businessRequests"] += 1
                        ordinal = state["businessRequests"]
                    else:
                        state["unexpectedRequestCount"] += 1
                        raise ValueError("unrecognized non-title provider request")
                if kind == "title":
                    self._complete("Fixture title.")
                    return
                if ordinal != 1:
                    raise ValueError("duplicate original business prompt reached provider")
                if not self.path.endswith("/responses" if endpoint == "native" else "/chat/completions"):
                    raise ValueError("unexpected synthetic provider route")
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                frame = {"id": "native-dsh-cancel-held", "object": "chat.completion.chunk",
                         "created": 1, "model": "endpoint-test",
                         "choices": [{"index": 0, "delta": {"role": "assistant",
                          "content": "CANCEL_PENDING"}, "finish_reason": None}]}
                if endpoint == "native":
                    frames = [
                        {"type": "response.output_item.added", "output_index": 0,
                         "item": {"id": "native-cancel-output", "type": "message", "role": "assistant",
                                  "phase": "final_answer", "content": []}},
                        {"type": "response.output_text.delta", "output_index": 0, "content_index": 0,
                         "delta": "CANCEL_PENDING"},
                    ]
                else:
                    frames = [frame]
                for chunk in frames:
                    self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
                    self.wfile.flush()
                with lock:
                    state["firstStreamFrameSent"] = True
                state["entered"].set()
                closed = wait_closed(self.connection, 100)
                with lock:
                    state["connectionClosedAfterCancel"] = closed
                    state["handlerTimedOut"] = not closed
            except (BrokenPipeError, ConnectionResetError, OSError):
                with lock:
                    if state["firstStreamFrameSent"]:
                        state["connectionClosedAfterCancel"] = True
            except (ValueError, TypeError, json.JSONDecodeError) as error:
                with lock:
                    state["providerError"] = type(error).__name__
                try:
                    self.send_error(400)
                except OSError:
                    pass

        def _complete(self, text: str):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            for frame in (
                {"id": "native-dsh-title", "object": "chat.completion.chunk", "created": 1,
                 "model": "endpoint-test", "choices": [{"index": 0, "delta":
                 {"role": "assistant", "content": text}, "finish_reason": None}]},
                {"id": "native-dsh-title", "object": "chat.completion.chunk", "created": 1,
                 "model": "endpoint-test", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]},
            ):
                self.wfile.write(("data: " + json.dumps(frame) + "\n\n").encode())
                self.wfile.flush()
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()

    return Provider


def provider_receipt(state: dict, lock: threading.Lock) -> dict:
    with lock:
        requests = list(state["requests"])
        return {"provider": "synthetic-loopback-openai-chat-completions",
            "requestBodiesPersisted": False, "requests": requests,
            "requestCount": len(requests), "matchingBusinessRequests": state["businessRequests"],
            "titleRequests": state["titleRequests"],
            "unexpectedRequestCount": state["unexpectedRequestCount"],
            "firstStreamFrameSent": state["firstStreamFrameSent"],
            "connectionClosedAfterCancel": state["connectionClosedAfterCancel"],
            "heldHandlerTimedOut": state["handlerTimedOut"],
            "providerErrorType": state["providerError"],
            "activeHandlers": state["activeHandlers"],
            "handlerThreadsStopped": state["activeHandlers"] == 0}


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", choices=("native", "dsh"), required=True)
    parser.add_argument("--program", type=Path, required=True, help="existing fixed headless consumer")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory on AgentKit SSD")
    parser.add_argument("--kernel-supervisor", type=Path, help="fixed installed Kernel 0.9.3 supervisor")
    parser.add_argument("--kernel-worker", type=Path, help="fixed installed Kernel 0.9.3 worker")
    parser.add_argument("--providers-config", type=Path, help="fixed canonical provider config for owned Native fixture")
    parser.add_argument("--kernel-revision", help="exact 40-hex source revision for the fixed Kernel runtime")
    parser.add_argument("--dsh-root", type=Path, help="fixed installed DSH 0.2.1-rc.2 root")
    args = parser.parse_args(argv)
    args.program = args.program.resolve(strict=True)
    args.output = args.output.expanduser().absolute()
    if not args.program.is_file() or not os.access(args.program, os.X_OK) or digest(args.program) != CONSUMER_SHA:
        parser.error("--program must be the fixed accepted consumer at the pinned SHA256")
    if not args.output.is_relative_to(SSD) or args.output.exists() or args.output.is_symlink() \
            or not args.output.parent.is_dir():
        parser.error("--output must be new under the existing SSD AgentKit project")
    if args.endpoint == "native":
        if args.kernel_revision != "09508a9083c24c7accefc99516739f279b85913e":
            parser.error("Native requires the exact Kernel 0.9.3 source revision 09508a9083c24c7accefc99516739f279b85913e")
        if not all((args.kernel_supervisor, args.kernel_worker, args.providers_config)) or args.dsh_root:
            parser.error("Native requires only --kernel-supervisor, --kernel-worker and --providers-config")
        args.kernel_supervisor = args.kernel_supervisor.resolve(strict=True)
        args.kernel_worker = args.kernel_worker.resolve(strict=True)
        args.providers_config = args.providers_config.resolve(strict=True)
    else:
        if not args.dsh_root or any((args.kernel_supervisor, args.kernel_worker, args.providers_config)):
            parser.error("DSH requires only --dsh-root")
        args.dsh_root = args.dsh_root.resolve(strict=True)
    return args


def run(args) -> int:
    if args.output.exists():
        raise ValueError("output path must not exist")
    source_paths = [Path(__file__), Path(__file__).with_name("native_dsh_run_cancel_validation.py"),
                    Path(__file__).with_name("native_owned_fixture.py"),
                    Path(__file__).with_name("dsh_owned_fixture.py"),
                    Path(__file__).with_name("native_dsh_mixed_fixture.py"),
                    HERE / "test-real-pi-run-lifecycle.py",
                    HERE.parent / "crates/headless/examples/run_lifecycle_acceptance.rs"]
    input_hashes = {str(path.resolve()): digest(path.resolve()) for path in source_paths}
    current_head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=HERE.parent,
        capture_output=True, text=True, timeout=5, check=True).stdout.strip()
    if current_head != FROZEN_RUNNER_HEAD:
        raise ValueError(f"AgentKit runner/tool HEAD {current_head} does not match frozen {FROZEN_RUNNER_HEAD}")
    args.output.mkdir(mode=0o700)
    owned = args.output / "owned"
    owned.mkdir(mode=0o700)
    workspace = owned / "workspace"
    workspace.mkdir(mode=0o700)
    (workspace / "proof.txt").write_text("owned Native/DSH cancellation proof\n", encoding="utf-8")
    home, tmp = owned / "home", owned / "tmp"
    home.mkdir(mode=0o700)
    tmp.mkdir(mode=0o700)
    state = {"requests": [], "businessRequests": 0, "titleRequests": 0,
        "unexpectedRequestCount": 0, "firstStreamFrameSent": False,
        "connectionClosedAfterCancel": False, "handlerTimedOut": False,
        "providerError": None, "activeHandlers": 0, "entered": threading.Event()}
    lock = threading.Lock()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), provider_handler(args.endpoint, state, lock))
    server.daemon_threads = True
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    provider_url = f"http://127.0.0.1:{server.server_port}/v1"
    process = None
    capture = None
    pgid = None
    dsh_context = None
    harness_process = None
    harness_diagnostic = None
    kernel_receipt = None
    runtime_identity = {}
    config: dict[str, Any] = {}
    cancel_signal = owned / "consumer-cancel.signal"
    result = None
    validation = None
    error = None
    exit_status = None
    process_snapshots = []
    cleanup = None
    try:
        if args.endpoint == "native":
            sup, worker, kernel_receipt = kernel_programs(
                "/Users/frankqin/dev/TekesKernel", supervisor=args.kernel_supervisor,
                worker=args.kernel_worker, expected_version=KERNEL_VERSION)
            build_revision = str(kernel_receipt["build"].get("source_revision", ""))
            if build_revision != args.kernel_revision:
                raise ValueError("installed Native binary revision does not equal --kernel-revision")
            if build_revision != "09508a9083c24c7accefc99516739f279b85913e":
                raise ValueError("Native binary is not the pinned Kernel 0.9.3/09508a9 build")
            providers = json.loads(args.providers_config.read_text(encoding="utf-8"))
            base = owned / "native"
            base.mkdir(mode=0o700)
            env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home),
                   "TMPDIR": str(tmp), "LANG": "en_US.UTF-8", "AGENTKIT_SYNTHETIC_KEY": "synthetic-key"}
            native_ctx = NATIVE_FIXTURE.owned_native(sup, worker, providers, provider_url, base,
                                                     env)
            fixture = native_ctx.__enter__()
            dsh_context = native_ctx
            native_process = fixture["process"]
            harness_process = native_process
            harness_diagnostic = fixture["diagnostic_tail"]
            config = {"kind": "native", "identity": "native", "url": fixture["url"],
                "bearer": fixture["bearer"], "cwd": str(workspace),
                "provider": providers["providers"][0]["id"],
                "model": providers["providers"][0]["models"][0]["id"], "action": "cancel", "prompt": PROMPT,
                "postPrepareDraft": DRAFT, "cancelSignal": str(cancel_signal)}
            runtime_identity["kernel"] = kernel_receipt
            process_snapshots.append({"phase": "kernel-owned-started", "pid": native_process.pid})
        else:
            runtime = args.dsh_root
            package = json.loads((runtime / "node_modules/@deepseek-ai/dsh/package.json").read_text(encoding="utf-8"))
            node = runtime / "node/bin/node"
            node_version = subprocess.run([str(node), "--version"], capture_output=True,
                text=True, timeout=5, check=True, env={"PATH": "/usr/bin:/bin"}).stdout.strip()
            runtime_identity["dsh"] = {"packageVersion": package.get("version"),
                "nodeVersion": node_version, "nodePath": str(node), "nodeSha256": digest(node),
                "entryPath": str(runtime / "node_modules/@deepseek-ai/dsh/lib/bin.js"),
                "entrySha256": digest(runtime / "node_modules/@deepseek-ai/dsh/lib/bin.js"),
                "packageSha256": digest(runtime / "node_modules/@deepseek-ai/dsh/package.json"),
                "acpSdkVersion": json.loads((runtime / "node_modules/@agentclientprotocol/sdk/package.json").read_text(encoding="utf-8")).get("version"),
                "acpSdkPath": str(runtime / "node_modules/@agentclientprotocol/sdk/dist/acp.d.ts"),
                "acpSdkSha256": digest(runtime / "node_modules/@agentclientprotocol/sdk/dist/acp.d.ts")}
            if package.get("version") != DSH_VERSION:
                raise ValueError("fixed DSH package version does not match 0.2.1-rc.2")
            dsh_home, data_root = owned / "dsh-home", owned / "dsh-data"
            env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(dsh_home),
                "DSH_HOME": str(dsh_home), "DSH_AGENTS_HOME": str(dsh_home / "agents"),
                "DSH_TELEMETRY_DISABLED": "1", "AGENTKIT_SYNTHETIC_KEY": "synthetic-key",
                "TMPDIR": str(tmp), "LANG": "en_US.UTF-8"}
            dsh_context = DSH_FIXTURE.owned_dsh(runtime, data_root, dsh_home, workspace,
                provider_url, env, query_sqlite=False)
            dsh = dsh_context.__enter__()
            harness_process = dsh.process
            harness_diagnostic = dsh.diagnostic_tail
            if dsh.version != DSH_VERSION:
                raise ValueError("owned DSH fixture version does not match 0.2.1-rc.2")
            config = {**dsh.connector_config([]), "identity": "dsh", "cwd": str(workspace),
                "provider": "endpoint-test", "model": "endpoint-test", "action": "cancel",
                "prompt": PROMPT, "postPrepareDraft": DRAFT, "cancelSignal": str(cancel_signal)}
            runtime_identity["dsh"]["fixtureVersion"] = dsh.version
        program_record = {"path": str(args.program), "sha256": digest(args.program),
            "expectedSha256": CONSUMER_SHA, "size": args.program.stat().st_size}
        consumer_example = HERE.parent / "crates/headless/examples/run_lifecycle_acceptance.rs"
        write_new(args.output / "source-inputs.json", {"productHead": FROZEN_PRODUCT_HEAD,
            "runnerHead": FROZEN_RUNNER_HEAD, "actualRunnerHead": current_head,
            "kernelRevisionArg": args.kernel_revision, "endpoint": args.endpoint,
            "program": program_record, "consumerBuild": {
                "productBaseHead": FROZEN_PRODUCT_HEAD,
                "acceptanceExampleOverlay": str(consumer_example.resolve()),
                "acceptanceExampleSha256": digest(consumer_example)},
            "runnerInputs": input_hashes,
            "providersConfig": ({"path": str(args.providers_config),
                "sha256": digest(args.providers_config)} if args.endpoint == "native" else None),
            "runtimeBefore": runtime_identity, "provider": "synthetic-loopback-only",
            "credentialPersistence": False})
        write_new(args.output / "provider-port.json", {"host": "127.0.0.1", "port": server.server_port})
        host_env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home),
                    "TMPDIR": str(tmp), "LANG": "en_US.UTF-8"}
        process = subprocess.Popen([str(args.program)], cwd=workspace, env=host_env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            close_fds=True, start_new_session=True, bufsize=0)
        pgid = process.pid
        process_snapshots.append({"phase": "consumer-launched", "processes": process_group_snapshot(pgid)})
        capture = ProcessCapture(process, args.output)
        assert process.stdin is not None
        process.stdin.write(json.dumps(config, ensure_ascii=False).encode() + b"\n")
        process.stdin.close()
        event = capture.wait_event("run_cancellable", 120)
        process_snapshots.append({"phase": "cancellable", "processes": process_group_snapshot(pgid)})
        if not state["entered"].wait(40):
            raise TimeoutError("synthetic provider did not receive original prompt")
        with lock:
            if state["businessRequests"] != 1 or not state["firstStreamFrameSent"]:
                raise ValueError("provider lacks exactly one held original business request and first frame")
        with cancel_signal.open("xb") as stream:
            stream.write(b"cancel\n")
        result = capture.wait_event("cancel_settled", 50)
        if not state["entered"].wait(2):
            raise TimeoutError("provider handler ended before cancellation settlement")
        if not wait_until(lambda: state["connectionClosedAfterCancel"] or state["handlerTimedOut"], 10):
            raise TimeoutError("provider has no post-cancel connection outcome")
        if not wait_until(lambda: state["activeHandlers"] == 0, 10):
            raise TimeoutError("held provider request handler did not stop after cancellation")
        exit_status = capture.wait_exit(10)
        receipt = provider_receipt(state, lock)
        after = {**result.get("after", {}), "ticketId": result.get("ticketId"),
                 "cancelCallCompleted": result.get("cancelCallCompleted")}
        validation = validate_cancel(args.endpoint, result.get("before"), after,
            ticket_id=result.get("ticketId"), expected_prompt=PROMPT,
            expected_draft=DRAFT, provider=receipt)
        if exit_status != 0:
            raise RuntimeError(f"public headless consumer exited {exit_status}")
        if args.endpoint == "native":
            _, _, after_identity = kernel_programs("/Users/frankqin/dev/TekesKernel",
                supervisor=args.kernel_supervisor, worker=args.kernel_worker,
                expected_version=KERNEL_VERSION)
            if after_identity != kernel_receipt:
                raise ValueError("Native runtime identity changed during acceptance")
            runtime_identity["kernelAfter"] = after_identity
        else:
            current = {key: value for key, value in runtime_identity["dsh"].items()
                       if key not in ("fixtureVersion",)}
            if digest(Path(current["nodePath"])) != current["nodeSha256"] \
                    or digest(Path(current["entryPath"])) != current["entrySha256"] \
                    or digest(Path(args.dsh_root) / "node_modules/@deepseek-ai/dsh/package.json") != current["packageSha256"] \
                    or digest(Path(current["acpSdkPath"])) != current["acpSdkSha256"]:
                raise ValueError("DSH fixed runtime files changed during acceptance")
            runtime_identity["dshAfter"] = current
        write_new(args.output / "lifecycle-validation.json", validation)
    except (OSError, ValueError, RuntimeError, TimeoutError, subprocess.SubprocessError,
            KeyError, TypeError, LifecycleEvidenceError) as exc:
        error = f"{type(exc).__name__}: {str(exc)[:1000]}"
    finally:
        if process is not None and pgid is not None:
            if capture is not None and capture.stdout_thread.is_alive():
                try:
                    exit_status = capture.wait_exit(3)
                except (subprocess.TimeoutExpired, RuntimeError, OSError):
                    pass
            cleanup = stop_owned_group(process, pgid)
            process_snapshots.append({"phase": "consumer-owned-group-stopped",
                "processes": process_group_snapshot(pgid), "cleanup": cleanup})
            if capture is not None:
                if not (args.output / "consumer.stdout.jsonl").exists():
                    try:
                        exit_status = capture.wait_exit(3)
                    except (subprocess.TimeoutExpired, RuntimeError, OSError):
                        error = error or "could not flush retained consumer stdout/stderr after cleanup"
                else:
                    capture.stdout_thread.join(timeout=3)
                    capture.stderr_thread.join(timeout=3)
                if capture.stdout_thread.is_alive() or capture.stderr_thread.is_alive():
                    error = error or "consumer stdout/stderr capture threads did not stop"
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=3)
        if server_thread.is_alive():
            error = error or "owned provider server thread did not stop"
        if dsh_context is not None:
            try:
                dsh_context.__exit__(None, None, None)
            except Exception as exc:
                error = error or f"{type(exc).__name__}: owned runtime cleanup failed"
        if harness_process is not None and harness_process.poll() != 0:
            error = error or "owned Native/DSH harness did not exit normally after fixture cleanup"
        if harness_diagnostic is not None:
            try:
                diagnostic_path = args.output / "harness-diagnostic-tail.log"
                with diagnostic_path.open("x", encoding="utf-8") as stream:
                    stream.write(harness_diagnostic())
            except OSError:
                error = error or "could not retain redacted harness diagnostic tail"
        try:
            if {str(path.resolve()): digest(path.resolve()) for path in source_paths} != input_hashes:
                error = error or "acceptance source inputs changed during the frozen run"
        except OSError:
            error = error or "cannot recheck frozen acceptance source inputs"
        with lock:
            active = state["activeHandlers"]
        if active != 0:
            error = error or "owned provider handler remains active after shutdown"
        final_provider = provider_receipt(state, lock)
        if process is not None and (cleanup is None or cleanup.get("groupAbsent") is not True):
            error = error or "owned consumer process group did not stop cleanly"
        if error is None and validation is not None:
            try:
                write_new(args.output / "provider-observation.json", final_provider)
                write_new(args.output / "process-cleanup.json", {"exitStatus": exit_status,
                    "consumerProcessGroup": cleanup, "ownedProviderHandlers": active,
                    "harnessPid": harness_process.pid if harness_process is not None else None,
                    "harnessExited": harness_process.poll() is not None if harness_process is not None else None,
                    "harnessExitStatus": harness_process.poll() if harness_process is not None else None,
                    "runtimeFixtureCleanup": "completed", "processSnapshots": process_snapshots})
                write_new(args.output / "acceptance.json", {"accepted": True,
                    "endpoint": args.endpoint, "productHead": FROZEN_PRODUCT_HEAD,
                    "runnerHead": FROZEN_RUNNER_HEAD,
                    "programSha256": CONSUMER_SHA, "runtime": runtime_identity,
                    "validation": validation, "desktop": False, "physicalIme": False,
                    "accessibility": False})
            except OSError as exc:
                error = f"{type(exc).__name__}: final evidence write failed"
        if error is not None:
            for name, value in (("provider-observation.json", final_provider),
                ("process-cleanup.json", {"exitStatus": exit_status,
                 "consumerProcessGroup": cleanup, "ownedProviderHandlers": active,
                 "harnessPid": harness_process.pid if harness_process is not None else None,
                 "harnessExited": harness_process.poll() is not None if harness_process is not None else None,
                    "harnessExitStatus": harness_process.poll() if harness_process is not None else None,
                 "runtimeFixtureCleanup": "attempted", "processSnapshots": process_snapshots}),
                ("acceptance.json", {"accepted": False, "endpoint": args.endpoint,
                 "productHead": FROZEN_PRODUCT_HEAD, "runnerHead": FROZEN_RUNNER_HEAD,
                 "programSha256": CONSUMER_SHA,
                 "runtime": runtime_identity, "error": error,
                 "preserveFailedEvidence": True})):
                path = args.output / name
                if not path.exists():
                    try:
                        write_new(path, value)
                    except OSError:
                        pass
    if error is not None:
        print(f"NATIVE_DSH_RUN_CANCEL_FAILED endpoint={args.endpoint} evidence={args.output} error={error}", file=sys.stderr)
        return 1
    print(f"NATIVE_DSH_RUN_CANCEL_ACCEPTED endpoint={args.endpoint} evidence={args.output}")
    return 0


def wait_until(predicate, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.025)
    return bool(predicate())


if __name__ == "__main__":
    raise SystemExit(run(parse_args()))
