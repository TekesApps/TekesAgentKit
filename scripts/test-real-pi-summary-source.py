#!/usr/bin/env python3
"""Capture one real Pi Summary with the source authority used to publish it.

The provider is synthetic and loopback-only. No desktop UI or harness-private
history access is used. A caller supplies an already-built feature-enabled
headless executable; this script never invokes Cargo.
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

from owned_endpoint_provider import provider_handler
from pi_owned_fixture import prepare_pi
from summary_source_fixture import validate_witness


ROOT = Path(__file__).resolve().parents[1]


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--headless-program", required=True, type=Path,
                        help="existing feature-enabled tekes-agent-headless executable")
    parser.add_argument("--pi-root", required=True, type=Path,
                        help="fixed Pi installation root")
    parser.add_argument("--output", required=True, type=Path,
                        help="new evidence directory under the existing AgentKit SSD project")
    args = parser.parse_args()
    program = args.headless_program.resolve(strict=True)
    pi_root = args.pi_root.resolve(strict=True)
    output = args.output.expanduser().absolute()
    if not program.is_file() or not os.access(program, os.X_OK):
        parser.error("--headless-program must be an executable regular file")
    if not output.resolve().is_relative_to(Path("/Volumes/HP P900/dev/TekesAgentKit")):
        parser.error("--output must stay under the existing AgentKit SSD project")
    if output.exists() and any(output.iterdir()):
        parser.error("--output must be absent or empty")
    output.mkdir(parents=True, exist_ok=True)
    base = output / "owned"
    base.mkdir(mode=0o700)
    workspace = base / "workspace"
    workspace.mkdir()
    (workspace / "proof.txt").write_text("owned Pi summary source proof\n", encoding="utf-8")
    data_root, home = base / "pi-data", base / "pi-home"
    requests: list[str] = []
    provider = provider_handler(workspace=lambda: workspace, requests=requests)

    class QuietProvider(provider):
        def log_message(self, *_args: object) -> None:
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), QuietProvider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    provider_url = f"http://127.0.0.1:{server.server_port}/v1"
    try:
        pi_config = prepare_pi(pi_root, data_root, home, provider_url,
                               strict_environment=True)
        config = {
            **pi_config,
            "kind": "pi",
            "identity": "pi",
            "commands": [
                {"op": "create", "cwd": str(workspace), "saveAs": "parent"},
                {"op": "call", "sessionId": "parent", "method": "session.selectModel",
                 "args": {"provider": "endpoint-test", "model": "endpoint-test"}},
                {"op": "open", "sessionId": "parent"},
                {"op": "compose", "sessionId": "parent", "text": "ENDPOINT_PI_CHILD"},
                {"op": "waitSettled", "sessionId": "parent"},
                {"op": "presentationSource", "sessionId": "parent", "saveAs":"child",
                 "savePointer":"/snapshot/summary/subagents/0/sessionId"},
                {"op":"open", "sessionId":"child"},
                {"op":"waitSettled", "sessionId":"child", "inputText":"CHILD_PI_TOOL_PROOF"},
                {"op":"childHistory", "sessionId":"parent", "childSessionId":"child"},
                {"op":"reconnect"},
                {"op":"open", "sessionId":"parent"},
                {"op": "presentationSource", "sessionId": "parent"},
            ],
        }
        commands = config["commands"]
        write_json(output / "commands.json", commands)
        digest = hashlib.sha256(program.read_bytes()).hexdigest()
        write_json(output / "program.json", {"path": str(program), "sha256": digest,
                                               "size": program.stat().st_size})
        env = {
            "HOME": str(home),
            "PATH": pi_config["environment"]["PATH"],
            "TMPDIR": str(home),
            "LANG": "C.UTF-8",
        }
        result = subprocess.run([str(program)], input=json.dumps(config), text=True,
                                capture_output=True, timeout=180, env=env, cwd=workspace)
        (output / "stdout.log").write_text(result.stdout, encoding="utf-8")
        (output / "stderr.log").write_text(result.stderr, encoding="utf-8")
        write_json(output / "provider-requests.json", {"paths": requests,
                                                         "requestBodiesPersisted": False})
        if result.returncode != 0:
            raise RuntimeError(f"headless exited {result.returncode}; see retained logs")
        rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
        if len(rows) != len(commands) + 1 or rows[0].get("ready") is not True or rows[0].get("kind") != "pi":
            raise ValueError("headless handshake or command/result count mismatch")
        for command, row in zip(commands, rows[1:]):
            if row.get("op") != command["op"] or "result" not in row:
                raise ValueError("headless command/result pairing mismatch")
        witness = rows[-1]["result"]
        write_json(output / "witness.json", witness)
        receipt = validate_witness(witness)
        if receipt["childStatus"] not in ("completed", "idle"):
            raise ValueError("child endpoint status is not an observed normal terminal state")
        initial = next(row["result"] for row in rows[1:] if row["op"] == "presentationSource")
        if initial["snapshot"]["summary"]["subagents"][0]["sessionId"] != receipt["childSessionId"]:
            raise ValueError("child pointer changed after reconnect")
        if witness["snapshot"]["connectionEpoch"] <= initial["snapshot"]["connectionEpoch"]:
            raise ValueError("reconnect did not advance the source epoch")
        history = next(row["result"] for row in rows[1:] if row["op"] == "childHistory")
        if "CHILD_PI_TOOL_PROOF" not in json.dumps(history) or "ENDPOINT_PI_CHILD" in json.dumps(history):
            raise ValueError("child history missing or contains the parent input")
        # The same public presentation must also show the provider-created
        # model child and preserve the parent request in its transcript.
        snapshot = witness["snapshot"]
        transcript_text = json.dumps(snapshot.get("transcript", {}), ensure_ascii=False)
        if "ENDPOINT_PI_CHILD" not in transcript_text:
            raise ValueError("parent prompt is absent from the captured Transcript")
        if not requests:
            raise ValueError("synthetic loopback provider received no request")
        write_json(output / "witness-receipt.json", receipt)
        write_json(output / "witness.json", witness)
        write_json(output / "run-receipt.json", {
            "schemaVersion": 1,
            "result": "accepted",
            "endpoint": "pi",
            "provider": "synthetic-loopback",
            "programSha256": digest,
            "commandCount": len(commands),
            "providerRequestCount": len(requests),
            "desktopInteraction": False,
            "paidModel": False,
            "sameRuntimeCapture": True,
            "historyReconnect": "public-child-history-and-new-epoch-source-capture",
            "parentDraftIsolation": "not-run",
            "runningTransition": "not-run",
            "serverTransactionalFreshnessProven": False,
            "limitations": ["JSON validation cannot detect a producer that fabricates the complete witness"],
        })
        print("PI_SUMMARY_SOURCE_RECEIPT " + json.dumps(receipt, sort_keys=True))
        return 0
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"PI_SUMMARY_SOURCE_FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
