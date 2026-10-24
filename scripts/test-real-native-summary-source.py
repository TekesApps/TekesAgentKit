#!/usr/bin/env python3
"""Capture a Native Task Summary from an existing feature-enabled headless.

The runner uses an installed Native 0.9.3 supervisor/worker and isolated
synthetic loopback provider. It never builds programs, opens a desktop window,
or reads harness-private history storage.
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import uuid

from native_owned_fixture import owned_native
from native_summary_source_fixture import validate_witness
from owned_endpoint_provider import provider_handler
from runtime_provenance import kernel_programs
from task_family_fixture import native_task_seed_commands, parse_family_exchange


ROOT = Path(__file__).resolve().parents[1]
SSD_PROJECT = Path("/Volumes/HP P900/dev/TekesAgentKit")


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_seed(commands: list[dict], rows: list[dict], parent_id: str, workspace: Path) -> dict:
    """Pair the real public family seed, keeping child history distinct from Summary."""
    manifest = parse_family_exchange("native", commands, rows, parent_id, workspace)
    if manifest.get("task") is None or manifest["task"]["row"].get("scope") != {
        "endpointId": "native", "threadId": parent_id, "sessionId": parent_id
    }:
        raise ValueError("Native Task is not a passive row in its original parent Run")
    if manifest.get("child", {}).get("readOnly") is not True:
        raise ValueError("Native child catalog did not produce read-only bounded history")

    exchanges = []
    variables: dict[str, str] = {}
    for command, row in zip(commands, rows[1:]):
        resolved = dict(command)
        for key in ("sessionId", "childSessionId"):
            if key in resolved:
                resolved[key] = variables.get(resolved[key], resolved[key])
        exchanges.append((resolved, row.get("result")))
        if command.get("saveAs"):
            result = row.get("result")
            if command.get("op") == "call" and command.get("method") == "session.create":
                variables[command["saveAs"]] = result["sessionId"]
            elif command.get("op") == "childCatalog":
                variables[command["saveAs"]] = result["entries"][1]["id"]
    history = [
        (command, result)
        for command, result in exchanges
        if command.get("op") == "childHistory"
        and command.get("sessionId") == parent_id
        and command.get("childSessionId") == manifest["child"]["address"]["sessionId"]
    ]
    if len(history) != 2 or any(not isinstance(result, dict) for _, result in history):
        raise ValueError("original parent-bound bounded child history was not read before and after reconnect")
    bounded_calls = [
        (command, result)
        for command, result in exchanges
        if command.get("op") == "call" and command.get("method") == "subagents.history"
    ]
    if len(bounded_calls) != 1:
        raise ValueError("expected one original public bounded-history request")
    request, page = bounded_calls[0]
    if request.get("args", {}).get("maxMessages") != 50:
        raise ValueError("bounded-history request did not use the established page limit")
    if not isinstance(page, dict) or page.get("parentSessionId") != parent_id:
        raise ValueError("bounded-history response is not bound to the original parent")
    if page.get("childSessionId") != manifest["child"]["address"]["sessionId"]:
        raise ValueError("bounded-history response is not bound to the catalog child")
    return {
        "parentSessionId": parent_id,
        "task": manifest["task"],
        "child": manifest["child"],
        "boundedHistory": {
            "publicChildHistoryReads": len(history),
            "publicBoundedPageLimit": request["args"]["maxMessages"],
            "sameParentAndChild": True,
            "equivalentToSummaryTaskPointer": False,
        },
        "evidenceScope": "public-bounded-history-source-consistency",
        "uiAcceptance": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--headless-program", required=True, type=Path,
                        help="existing feature-enabled tekes-agent-headless executable")
    parser.add_argument("--kernel-root", required=True, type=Path,
                        help="fixed Kernel snapshot containing fixtures/config/providers.canonical.json")
    parser.add_argument("--kernel-supervisor", required=True, type=Path,
                        help="explicit already-installed Kernel 0.9.3 supervisor")
    parser.add_argument("--kernel-worker", required=True, type=Path,
                        help="explicit already-installed Kernel 0.9.3 worker")
    parser.add_argument("--output", required=True, type=Path,
                        help="new evidence directory under the existing AgentKit SSD project")
    args = parser.parse_args()

    program = args.headless_program.resolve(strict=True)
    kernel_root = args.kernel_root.resolve(strict=True)
    supervisor_path = args.kernel_supervisor.resolve(strict=True)
    worker_path = args.kernel_worker.resolve(strict=True)
    if not program.is_file() or not os.access(program, os.X_OK):
        parser.error("--headless-program must be an executable regular file")
    if not supervisor_path.is_file() or not worker_path.is_file():
        parser.error("explicit Kernel supervisor and worker files are required")
    output = args.output.expanduser().absolute()
    if not output.resolve().is_relative_to(SSD_PROJECT):
        parser.error("--output must stay under the existing AgentKit SSD project")
    if output.is_symlink():
        parser.error("--output must not be a symlink")
    if output.exists() and (not output.is_dir() or any(output.iterdir())):
        parser.error("--output must be absent or empty")

    # Require the explicit installed binaries and fixed provider snapshot before
    # creating the isolated fixture. This performs only --describe-build.
    supervisor, worker, kernel_identity = kernel_programs(
        kernel_root, supervisor=supervisor_path, worker=worker_path,
        expected_version="0.9.3",
    )
    providers_path = kernel_root / "fixtures/config/providers.canonical.json"
    providers_source = json.loads(providers_path.read_text(encoding="utf-8"))
    if not isinstance(providers_source.get("providers"), list) or not providers_source["providers"]:
        parser.error("fixed Kernel provider snapshot has no providers")

    output.mkdir(parents=True, exist_ok=True)
    base = output / "owned"
    base.mkdir(mode=0o700)
    workspace = base / "workspace"
    workspace.mkdir()
    (workspace / "proof.txt").write_text("owned Native Task source proof\n", encoding="utf-8")
    requests: list[str] = []
    provider = provider_handler(workspace=lambda: workspace, requests=requests)

    class QuietProvider(provider):
        def log_message(self, *_args: object) -> None:
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), QuietProvider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    parent_id = str(uuid.uuid4())
    commands = native_task_seed_commands(
        parent_id, workspace,
        providers_source["providers"][0]["id"],
        providers_source["providers"][0]["models"][0]["id"],
    )
    # Reopen the seeded parent after reconnect, then capture the original
    # publication and its source authority in one presentationSource call.
    commands.extend([
        {"op": "open", "sessionId": "parent"},
        {"op": "presentationSource", "sessionId": "parent"},
    ])
    write_json(output / "commands.json", commands)
    source_paths = [Path(__file__).resolve(), Path(__file__).with_name("native_summary_source_fixture.py"),
                    Path(__file__).with_name("task_family_fixture.py"),
                    Path(__file__).with_name("native_owned_fixture.py"),
                    Path(__file__).with_name("owned_endpoint_provider.py")]
    write_json(output / "source-receipt.json", {
        "schemaVersion": 1,
        "programs": {
            "headless": {"path": str(program), "sha256": sha256(program), "size": program.stat().st_size},
            "kernel": kernel_identity,
        },
        "fixedInputs": {
            "kernelRoot": str(kernel_root),
            "providersCanonical": str(providers_path),
            "providersCanonicalSha256": sha256(providers_path),
            "expectedKernelVersion": "0.9.3",
            "scripts": [{"path": str(path), "sha256": sha256(path)} for path in source_paths],
        },
    })

    native_context = None
    native_runtime = None
    supervisor_process = None
    failure: BaseException | None = None
    run_receipt = None
    try:
        candidate_context = owned_native(
            supervisor, worker, providers_source,
            f"http://127.0.0.1:{server.server_port}/v1", base,
            {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8"},
        )
        native = candidate_context.__enter__()
        native_context = candidate_context
        native_runtime = native
        supervisor_process = native["process"]
        config = {
            "kind": "native", "url": native["url"], "bearer": native["bearer"],
            "expectedVersion": "0.9.3", "commands": commands,
        }
        home = base / "headless-home"
        home.mkdir(mode=0o700)
        env = {"HOME": str(home), "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
               "TMPDIR": str(home), "LANG": "C.UTF-8"}
        (output / "stdout.log").write_text("", encoding="utf-8")
        (output / "stderr.log").write_text("", encoding="utf-8")
        try:
            result = subprocess.run([str(program)], input=json.dumps(config), text=True,
                                    capture_output=True, timeout=180, env=env, cwd=workspace)
        except subprocess.TimeoutExpired as error:
            out = error.stdout.decode("utf-8", errors="replace") if isinstance(error.stdout, bytes) else (error.stdout or "")
            err = error.stderr.decode("utf-8", errors="replace") if isinstance(error.stderr, bytes) else (error.stderr or "")
            (output / "stdout.log").write_text(out, encoding="utf-8")
            (output / "stderr.log").write_text(err, encoding="utf-8")
            raise RuntimeError("headless timed out; retained partial stdout/stderr") from None
        (output / "stdout.log").write_text(result.stdout, encoding="utf-8")
        (output / "stderr.log").write_text(result.stderr, encoding="utf-8")
        write_json(output / "provider-requests.json", {
            "paths": requests, "requestBodiesPersisted": False,
            "provider": "synthetic-loopback",
        })
        if result.returncode != 0:
            raise RuntimeError(f"headless exited {result.returncode}; retained stdout/stderr")
        rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
        if (not rows or rows[0].get("ready") is not True or rows[0].get("kind") != "native"
                or rows[0].get("version") != "0.9.3"):
            raise ValueError("Native headless handshake did not match installed Kernel 0.9.3")
        write_json(output / "versions.json", {
            "headlessHandshake": rows[0],
            "kernelBuild": kernel_identity["build"],
        })
        if len(rows) != len(commands) + 1:
            raise ValueError("headless command/result count mismatch")
        for command, row in zip(commands, rows[1:]):
            if row.get("op") != command.get("op") or "result" not in row:
                raise ValueError("headless command/result pairing mismatch")
        seed_commands = commands[:-2]
        seed_rows = rows[:len(seed_commands) + 1]
        task_results = validate_seed(seed_commands, seed_rows, parent_id, workspace)
        # The final row is the source witness. Keeping seed command/result
        # pairing separate prevents the final capture from fabricating family
        # navigation or bounded-history facts.
        witness = rows[-1]["result"]
        write_json(output / "witness.json", witness)
        witness_receipt = validate_witness(witness)
        if witness_receipt["parentSessionId"] != parent_id:
            raise ValueError("captured Summary belongs to another Native parent")
        if not requests:
            raise ValueError("synthetic loopback provider received no request")
        write_json(output / "witness-receipt.json", witness_receipt)
        write_json(output / "task-seed-receipt.json", task_results)
        run_receipt = {
            "schemaVersion": 1,
            "result": "accepted",
            "endpoint": "native",
            "provider": "synthetic-loopback",
            "programSha256": sha256(program),
            "kernelSupervisorSha256": kernel_identity["supervisorSha256"],
            "kernelWorkerSha256": kernel_identity["workerSha256"],
            "commandCount": len(commands),
            "providerRequestCount": len(requests),
            "desktopInteraction": False,
            "paidModel": False,
            "sameRuntimeCapture": True,
            "historyReconnect": "public-bounded-history-and-new-publication-capture",
            "summaryTaskPointer": "passive-parent-scope-only",
            "serverTransactionalFreshnessProven": False,
            "limitations": [
                "JSON validation cannot detect a producer that fabricates the complete witness",
                "the public bounded child history is separate from Summary Task navigation",
                "no desktop, physical IME, accessibility, or UI acceptance is claimed",
            ],
        }
    except BaseException as error:
        failure = error
    finally:
        if native_context is not None:
            try:
                native_context.__exit__(None, None, None)
            except BaseException as error:
                if failure is None:
                    failure = error
        server.shutdown()
        server.server_close()

    supervisor_exit = supervisor_process.poll() if supervisor_process is not None else None
    write_json(output / "owned-kernel-exit.json", {
        "supervisorPid": supervisor_process.pid if supervisor_process is not None else None,
        "exitCode": supervisor_exit,
        "normalExit": supervisor_exit == 0,
        "diagnosticTail": native_runtime["diagnostic_tail"]() if native_runtime is not None else None,
    })
    if failure is not None:
        print(f"NATIVE_SUMMARY_SOURCE_FAILED: {failure}", file=sys.stderr)
        return 1
    if supervisor_exit != 0:
        print("NATIVE_SUMMARY_SOURCE_FAILED: owned Kernel supervisor did not exit normally", file=sys.stderr)
        return 1
    assert run_receipt is not None
    run_receipt["realHarnessProtocolVerified"] = True
    run_receipt["ownedKernelNormalExit"] = True
    write_json(output / "run-receipt.json", run_receipt)
    print("NATIVE_SUMMARY_SOURCE_RECEIPT " + json.dumps(witness_receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
