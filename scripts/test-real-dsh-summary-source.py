#!/usr/bin/env python3
"""Capture and validate DSH's same-publication completed-child Summary.

This runner uses an already-built feature-enabled headless program and the
fixed DSH 0.2.1-rc.2 runtime. It never builds, upgrades, opens a desktop, or
reads DSH-private session files.
"""

from __future__ import annotations

import argparse
import hashlib
import http.server
import json
import os
import subprocess
import sys
import threading
from pathlib import Path

from dsh_owned_fixture import owned_dsh
from owned_endpoint_provider import provider_handler
from summary_source_fixture import validate_witness
from task_family_fixture import dsh_subagent_seed_commands


ROOT = Path(__file__).resolve().parents[1]
SSD_PROJECT = Path("/Volumes/HP P900/dev/TekesAgentKit")
PINNED_DSH_VERSION = "0.2.1-rc.2"


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _source_hashes(program: Path, frozen_manifest: Path | None = None) -> dict[str, object]:
    paths = [
        Path(__file__).resolve(),
        ROOT / "scripts/summary_source_fixture.py",
        ROOT / "scripts/dsh_owned_fixture.py",
        ROOT / "scripts/owned_endpoint_provider.py",
        ROOT / "scripts/task_family_fixture.py",
        ROOT / "crates/headless/src/main.rs",
        ROOT / "crates/presentation/src/controller.rs",
        ROOT / "crates/presentation/src/summary.rs",
    ]
    sources = {str(path): _sha256(path) for path in paths if path.is_file()}
    if frozen_manifest is None:
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=True
        ).stdout.strip()
        identity = {"kind": "git-worktree"}
    else:
        frozen_manifest = frozen_manifest.resolve(strict=True)
        manifest = json.loads(frozen_manifest.read_text())
        revision = manifest.get("head")
        if (not isinstance(revision, str) or len(revision) != 40
                or any(c not in "0123456789abcdef" for c in revision)):
            raise ValueError("frozen manifest must identify an exact commit")
        files = manifest.get("files")
        if not isinstance(files, dict) or not {
            "Cargo.toml", "Cargo.lock", "crates/headless/src/main.rs",
        }.issubset(files):
            raise ValueError("frozen manifest lacks the headless source closure")
        checked = {}
        for name, expected in files.items():
            relative = Path(name)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("unsafe frozen source path")
            if name in ("Cargo.toml", "Cargo.lock") or name.startswith("crates/"):
                source = ROOT / relative
                if not source.is_file() or _sha256(source) != expected:
                    raise ValueError(f"frozen product input changed: {name}")
                checked[name] = expected
        identity = {"kind": "frozen-manifest", "path": str(frozen_manifest),
                    "sha256": _sha256(frozen_manifest), "checkedProductInputs": checked}
    return {"gitRevision": revision, "identity": identity, "sources": sources,
            "headlessProgramSha256": _sha256(program)}


def _rows(stdout: str, commands: list[dict[str, object]]) -> list[dict[str, object]]:
    rows = [json.loads(line) for line in stdout.splitlines() if line.strip()]
    if len(rows) != len(commands) + 1 or rows[0].get("kind") != "dsh" or rows[0].get("ready") is not True or rows[0].get("version") != PINNED_DSH_VERSION:
        raise ValueError("headless handshake or command/result count mismatch")
    for command, row in zip(commands, rows[1:]):
        if row.get("op") != command.get("op") or "result" not in row:
            raise ValueError("headless command/result pairing mismatch")
    return rows


def _result(
    rows: list[dict[str, object]], commands: list[dict[str, object]], op: str,
    *, save_as: str | None = None,
) -> dict[str, object]:
    matches = [
        row["result"]
        for command, row in zip(commands, rows[1:])
        if command.get("op") == op and row.get("op") == op
        and (save_as is None or command.get("saveAs") == save_as)
        and isinstance(row.get("result"), dict)
    ]
    if matches:
        return matches[-1]
    raise ValueError(f"missing public {op} result")


def _assert_profile(rows: list[dict[str, object]], commands: list[dict[str, object]], receipt: dict[str, object]) -> None:
    before = _result(rows, commands, "presentationSource", save_as="beforeReconnect")
    witness = _result(rows, commands, "presentationSource")
    proof = receipt["sourceEvidence"]
    child_id = proof["childSessionId"]
    snapshot = witness["snapshot"]
    initial_snapshot = before["snapshot"]
    final_row = proof["summaryRow"]
    if final_row.get("title") != "Catalog proof child":
        raise ValueError("published DSH Summary title did not use the original JSON description")
    tools = snapshot["transcript"]["tools"]
    matching = [tool for tool in tools if tool.get("id") == final_row.get("id")]
    if len(matching) != 1:
        raise ValueError("expected one original DSH subagent tool in parent Transcript")
    tool = matching[0]
    arguments = tool.get("arguments")
    if not isinstance(arguments, str):
        raise ValueError("original DSH tool arguments must remain a JSON string")
    parsed_arguments = json.loads(arguments)
    if parsed_arguments.get("description") != "Catalog proof child":
        raise ValueError("original DSH JSON-string arguments lost the description value")
    if tool.get("resultPosition", {}).get("kind") != "journal" or tool["resultPosition"].get("sequence") != 19:
        raise ValueError("original DSH tool result must remain linked to journal sequence 19")

    position = tool.get("sourcePosition", {})
    if position != {"kind": "journal", "sequence": 16, "contentIndex": 0}:
        raise ValueError("DSH call lost its original journal/content position")
    parent = witness["authority"]["sessions"][proof["parentSessionId"]]
    event = parent["entries"]["16"]["event"]
    message = event.get("data", {}).get("message", {})
    content = message.get("content", [])
    if event.get("type") != "assistant/message" or message.get("role") != "assistant" or not content:
        raise ValueError("DSH call position does not reference an original assistant message")
    call = content[0]
    if (call.get("type") != "tool-call" or call.get("name") != tool.get("name")
            or call.get("arguments") != arguments
            or call.get("id") != tool.get("result", {}).get("toolCallId")):
        raise ValueError("DSH Summary arguments are not paired with their original public call")

    initial_rows = initial_snapshot.get("summary", {}).get("subagents", [])
    if not any(row.get("sessionId") == child_id for row in initial_rows):
        raise ValueError("pre-reconnect parent source did not contain the child pointer")
    initial_epoch = initial_snapshot.get("connectionEpoch")
    final_epoch = snapshot.get("connectionEpoch")
    if type(initial_epoch) is not int or type(final_epoch) is not int or final_epoch <= initial_epoch:
        raise ValueError("reconnect did not preserve the child pointer in a newer source epoch")
    if not any(row.get("sessionId") == child_id for row in snapshot.get("summary", {}).get("subagents", [])):
        raise ValueError("post-reconnect parent source lost the child pointer")

    history = _result(rows, commands, "childHistory")
    history_text = json.dumps(history, ensure_ascii=False)
    if "CHILD_INITIAL_PROOF" not in history_text or "CHILD_CONTINUATION_PROOF" not in history_text:
        raise ValueError("public childHistory omitted the initial or continuation prompt")
    if "ENDPOINT_SUBAGENT_CATALOG" in history_text:
        raise ValueError("public childHistory contains the parent probe")
    # The actual exchange list is retained to make the exercised sequence
    # inspectable without reading harness-private session storage.
    if not any(command.get("op") == "childHistory" for command in commands):
        raise ValueError("profile omitted the public childHistory operation")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--headless-program", required=True, type=Path,
                        help="existing feature-enabled tekes-agent-headless executable")
    parser.add_argument("--dsh-root", required=True, type=Path,
                        help="fixed installed DSH 0.2.1-rc.2 runtime root")
    parser.add_argument("--output", required=True, type=Path,
                        help="new evidence directory under the existing AgentKit SSD project")
    parser.add_argument("--source-manifest", type=Path,
                        help="exact frozen source manifest; validates product inputs without a Git checkout")
    args = parser.parse_args()
    program = args.headless_program.resolve(strict=True)
    dsh_root = args.dsh_root.resolve(strict=True)
    output = args.output.expanduser().absolute()
    if not program.is_file() or not os.access(program, os.X_OK):
        parser.error("--headless-program must be an executable regular file")
    source_identity = _source_hashes(program, args.source_manifest)
    if not output.resolve().is_relative_to(SSD_PROJECT.resolve(strict=True)):
        parser.error("--output must stay under the existing AgentKit SSD project")
    if output.exists() and (not output.is_dir() or any(output.iterdir())):
        parser.error("--output must be absent or empty")
    output.mkdir(parents=True, exist_ok=True)

    base = output / "owned"
    base.mkdir(mode=0o700)
    workspace = base / "workspace"
    workspace.mkdir()
    (workspace / "proof.txt").write_text("owned DSH summary source proof\n", encoding="utf-8")
    home, data_root, host_home, tmp = base / "dsh-home", base / "dsh-data", base / "headless-home", base / "tmp"
    host_home.mkdir(mode=0o700)
    tmp.mkdir(mode=0o700)
    requests: list[str] = []
    provider_cls = provider_handler(workspace=lambda: workspace, requests=requests)

    class QuietProvider(provider_cls):
        def log_message(self, *_args: object) -> None:
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), QuietProvider)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    provider_url = f"http://127.0.0.1:{server.server_port}/v1"
    dsh_process = None
    success = False
    result = None
    commands: list[dict[str, object]] = []
    requests_seen = 0
    failure: str | None = None
    try:
        dsh_env = {
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
            "HOME": str(home),
            "DSH_HOME": str(home),
            "DSH_AGENTS_HOME": str(home / "agents"),
            "DSH_TELEMETRY_DISABLED": "1",
            "AGENTKIT_SYNTHETIC_KEY": "synthetic-key",
            "TMPDIR": str(tmp),
            "LANG": "en_US.UTF-8",
        }
        with owned_dsh(dsh_root, data_root, home, workspace, provider_url, dsh_env,
                       query_sqlite=False) as dsh:
            dsh_process = dsh.process
            if dsh.version != PINNED_DSH_VERSION:
                raise ValueError(f"expected pinned DSH {PINNED_DSH_VERSION}; received {dsh.version}")
            commands = dsh_subagent_seed_commands("parent", workspace, "endpoint-test", "endpoint-test")
            reconnect_index = next(index for index, command in enumerate(commands) if command.get("op") == "reconnect")
            commands[reconnect_index:reconnect_index] = [
                {"op": "open", "sessionId": "parent"},
                {"op": "presentationSource", "sessionId": "parent", "saveAs": "beforeReconnect",
                 "savePointer": "/snapshot/summary/subagents/0/sessionId"},
            ]
            commands += [
                {"op": "open", "sessionId": "parent"},
                {"op": "presentationSource", "sessionId": "parent"},
                {"op": "childHistory", "sessionId": "parent", "childSessionId": "child"},
            ]
            _write_json(output / "commands.json", commands)
            _write_json(output / "program.json", {
                "path": str(program), "sha256": _sha256(program), "size": program.stat().st_size,
            })
            package = json.loads((dsh_root / "node_modules/@deepseek-ai/dsh/package.json").read_text(encoding="utf-8"))
            _write_json(output / "versions.json", {
                "dsh": dsh.version,
                "dshPackage": package.get("version"),
                "node": subprocess.run([str(dsh.node), "--version"], text=True, capture_output=True,
                                        check=True, env=dsh_env).stdout.strip(),
                "provider": "synthetic-loopback",
                "agentkitGitRevision": source_identity["gitRevision"],
                "identityKind": source_identity["identity"]["kind"],
            })
            _write_json(output / "source-hashes.json", source_identity)
            headless_env = {
                "PATH": dsh_env["PATH"], "HOME": str(host_home), "TMPDIR": str(tmp), "LANG": dsh_env["LANG"],
            }
            run = subprocess.run([str(program)], input=json.dumps(dsh.connector_config(commands), ensure_ascii=False),
                                 text=True, capture_output=True, cwd=workspace, env=headless_env, timeout=240)
            (output / "stdout.log").write_text(run.stdout, encoding="utf-8")
            (output / "stderr.log").write_text(run.stderr, encoding="utf-8")
            requests_seen = len(requests)
            _write_json(output / "provider-requests.json", {
                "paths": requests, "requestBodiesPersisted": False,
                "providerUrl": "http://127.0.0.1:<ephemeral>/v1",
            })
            if run.returncode != 0:
                raise RuntimeError(f"headless exited {run.returncode}; see retained stdout.log and stderr.log")
            rows = _rows(run.stdout, commands)
            before = _result(rows, commands, "presentationSource", save_as="beforeReconnect")
            _write_json(output / "before-reconnect-witness.json", before)
            witness = _result(rows, commands, "presentationSource")
            _write_json(output / "witness.json", witness)
            source_receipt = validate_witness(witness, endpoint="dsh", completed_journal=True)
            receipt = {"sourceEvidence": source_receipt}
            _assert_profile(rows, commands, receipt)
            if not requests_seen:
                raise ValueError("synthetic loopback provider received no request")
            _write_json(output / "witness-receipt.json", source_receipt)
            success = True
    except (OSError, ValueError, RuntimeError, StopIteration, subprocess.SubprocessError, json.JSONDecodeError) as error:
        failure = f"{type(error).__name__}: {error}"
        (output / "failure.txt").write_text(failure + "\n", encoding="utf-8")
    finally:
        server.shutdown()
        server.server_close()
        server_thread.join(timeout=5)

    exit_status = dsh_process.poll() if dsh_process is not None else None
    _write_json(output / "owned-dsh-exit.json", {
        "pid": dsh_process.pid if dsh_process is not None else None,
        "exitStatus": exit_status,
        "exitedBeforeReceipt": dsh_process is None or exit_status is not None,
        "stoppedByOwnedFixture": dsh_process is not None and exit_status is not None,
    })
    if success and dsh_process is not None and exit_status == 0:
        _write_json(output / "run-receipt.json", {
            "schemaVersion": 1,
            "result": "accepted",
            "endpoint": "dsh",
            "harnessVersion": PINNED_DSH_VERSION,
            "provider": "synthetic-loopback",
            "headlessProgramSha256": _sha256(program),
            "commandCount": len(commands),
            "providerRequestCount": requests_seen,
            "ownedDshExitStatus": exit_status,
            "ownedFixtureStoppedDshAfterCapture": True,
            "completedJournalOptIn": True,
            "sourceScope": "same-publication-memory-authority-and-public-journal",
            "reconnectChildPointer": "retained-in-new-connection-epoch",
            "desktopInteraction": False,
            "paidModel": False,
            "privateHarnessHistoryRead": False,
            "serverTransactionalFreshnessProven": False,
            "limitations": ["This proves the captured public projection, not server-side transactional freshness."],
        })
        print("DSH_SUMMARY_SOURCE_ACCEPTED " + json.dumps(receipt["sourceEvidence"], ensure_ascii=False, sort_keys=True))
        return 0
    if success and (dsh_process is None or exit_status != 0):
        failure = "owned DSH did not exit normally; acceptance receipt withheld"
        (output / "failure.txt").write_text(failure + "\n", encoding="utf-8")
    if failure:
        print("DSH_SUMMARY_SOURCE_FAILED: " + failure, file=sys.stderr)
    else:
        print("DSH_SUMMARY_SOURCE_FAILED: acceptance conditions were not met", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
