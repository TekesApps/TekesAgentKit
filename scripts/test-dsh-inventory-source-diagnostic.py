#!/usr/bin/env python3
"""Capture DSH session inventory summaries around one public reconnect probe.

This is a bounded diagnostic, not a performance sample. It uses the existing
headless consumer, fixed DSH runtime, public SessionEndpoint commands, and the
synthetic provider fixture. It never reads DSH private storage.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import threading
import time
import traceback

from dsh_owned_fixture import owned_dsh
from performance_process_sampler import ProcessSampler, classify, process_table
from performance_provider_fixture import (
    RequestStats,
    create_provider_handler,
    measure_commands,
    read_source_bytes,
    read_source_path,
)


SSD_ROOT = Path("/Volumes/HP P900/dev/TekesAgentKit")
CASE_ROOT = SSD_ROOT / ".runtime-fixtures/stage-close-20261025/performance"
FIXED_PROGRAM = SSD_ROOT / "acceptance-tools/headless/current-feature/tekes-agent-headless"
FIXED_RECEIPT = SSD_ROOT / ".runtime-fixtures/stage-close-20261025/stage-issue68-headless-program-20261026.json"
FIXED_DSH_ROOT = SSD_ROOT / ".runtime-fixtures/dsh-0.2.1-rc.2-sidebar-20261019"
FIXED_PROGRAM_SHA256 = "18d8f643e8cba5ee94202c8c2f481bfa31add7bc5ac4fc5df44276100532180b"
FIXED_SOURCE_COMMIT = "98b1d9a2960b90774ea5454e7e8dad870538f7bc"
FIXED_SOURCE_MANIFEST_SHA256 = "7986998349fada6c73c7316ffde93cea501f3148b8557c7e4426cc4633cc3639"


def write_json(path: Path, value) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def load_case_tools():
    path = Path(__file__).with_name("test-stage-performance-case.py")
    spec = importlib.util.spec_from_file_location("agentkit_stage_performance_case", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load existing stage performance runner")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def flatten(value, prefix=""):
    """Return stable field paths without interpreting or normalizing values."""
    output = {}
    if isinstance(value, dict):
        if not value:
            output[prefix] = {}
        for key in sorted(value):
            path = f"{prefix}.{key}" if prefix else str(key)
            output.update(flatten(value[key], path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            output.update(flatten(child, f"{prefix}[{index}]"))
        if not value:
            output[prefix] = []
    else:
        output[prefix] = value
    return output


def field_diff(before, after):
    left, right = flatten(before), flatten(after)
    return [
        {"field": key, "before": left.get(key), "after": right.get(key),
         "change": "added" if key not in left else "removed" if key not in right else "changed"}
        for key in sorted(left.keys() | right.keys())
        if key not in left or key not in right or left[key] != right[key]
    ]


def command_result(rows, expected_ops):
    if not rows or rows[0].get("ready") is not True or rows[0].get("kind") != "dsh":
        raise ValueError("headless DSH handshake was missing")
    if rows[0].get("version") != "0.2.1-rc.2":
        raise ValueError("actual DSH handshake does not match the pinned runtime")
    commands = rows[1:]
    if len(commands) != len(expected_ops):
        raise ValueError("headless command result count differs from requested command count")
    for row, op in zip(commands, expected_ops, strict=True):
        if row.get("op") != op or "result" not in row:
            raise ValueError(f"public headless command {op!r} did not return its result")
    return [row["result"] for row in commands]


def snapshot_session(snapshot, session_id):
    sessions = snapshot.get("sessions")
    if not isinstance(sessions, dict) or not isinstance(sessions.get(session_id), dict):
        raise ValueError("public snapshot omitted the requested session")
    return sessions[session_id]


def ordered_entries(session_projection):
    entries = session_projection.get("entries")
    if not isinstance(entries, dict):
        raise ValueError("public snapshot omitted session journal entries")
    try:
        return [entries[key] for key in sorted(entries, key=lambda key: int(key))]
    except (TypeError, ValueError) as error:
        raise ValueError("public snapshot entry keys are not integer sequences") from error


def run_scope(case_tools, program, config, case, env, sampler, session_id, index):
    prefix = f"scope-{index}"
    commands = [
        {"op": "open", "sessionId": session_id},
        {"op": "snapshot"},
        *measure_commands(session_id, index, 0),
        {"op": "snapshot"},
        {"op": "list"},
        {"op": "reconnect"},
        {"op": "open", "sessionId": session_id},
        {"op": "snapshot"},
        {"op": "list"},
    ]
    write_json(case / f"{prefix}-commands.json", commands)
    rows, exit_receipt = case_tools.run_program(
        program, dict(config, commands=commands), case, prefix, env, sampler, timeout=300
    )
    results = command_result(rows, [command["op"] for command in commands])
    pre_measure, post_measure, list_after_measure, post_reconnect, list_after_reconnect = (
        results[1], results[4], results[5], results[8], results[9]
    )
    before_session = snapshot_session(post_measure, session_id)
    after_session = snapshot_session(post_reconnect, session_id)
    if not isinstance(list_after_measure, list) or not isinstance(list_after_reconnect, list):
        raise ValueError("public DSH list operation did not return a summary array")
    before_entries, after_entries = ordered_entries(before_session), ordered_entries(after_session)
    summary_before = before_session.get("summary")
    summary_after = after_session.get("summary")
    record = {
        "sessionId": session_id,
        "scopeIndex": index,
        "commands": commands,
        "publicSnapshots": {
            "beforeMeasure": pre_measure,
            "afterMeasure": post_measure,
            "afterReconnect": post_reconnect,
        },
        "publicLists": {
            "afterMeasure": list_after_measure,
            "afterReconnect": list_after_reconnect,
        },
        "summaryDiffAfterMeasureToReconnect": field_diff(summary_before, summary_after),
        "summary": {"afterMeasure": summary_before, "afterReconnect": summary_after},
        "historyComparison": {
            "entryCountAfterMeasure": len(before_entries),
            "entryCountAfterReconnect": len(after_entries),
            "sameFullOrderedEntries": before_entries == after_entries,
            "entryDiff": field_diff(before_entries, after_entries),
        },
        "listDiffAfterMeasureToReconnect": field_diff(list_after_measure, list_after_reconnect),
        "headlessExit": exit_receipt,
    }
    write_json(case / f"{prefix}-inventory-diagnostic.json", record)
    return record


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--program", required=True, type=Path)
    parser.add_argument("--program-receipt", required=True, type=Path)
    parser.add_argument("--dsh-root", required=True, type=Path)
    parser.add_argument("--case-name", required=True)
    args = parser.parse_args(argv)
    if not re.fullmatch(r"dsh-inventory-source-[a-z0-9][a-z0-9-]{1,60}", args.case_name):
        parser.error("case name must be unique and start with dsh-inventory-source-")
    for label, actual, expected in (
        ("program", args.program, FIXED_PROGRAM),
        ("program receipt", args.program_receipt, FIXED_RECEIPT),
        ("DSH runtime", args.dsh_root, FIXED_DSH_ROOT),
    ):
        if actual.resolve(strict=True) != expected.resolve(strict=True):
            parser.error(f"{label} must use the explicitly pinned SSD path: {expected}")
    return args


def main(argv=None):
    args = parse_args(argv)
    case_tools = load_case_tools()
    program, program_receipt = case_tools.validate_program(args.program, args.program_receipt)
    if (program_receipt.get("sha256") != FIXED_PROGRAM_SHA256
            or program_receipt.get("sourceCommit") != FIXED_SOURCE_COMMIT
            or program_receipt.get("sourceManifestSha256") != FIXED_SOURCE_MANIFEST_SHA256):
        raise ValueError("headless program receipt is not the frozen 18d8 / source 98b receipt")
    runtime_args = argparse.Namespace(endpoint="dsh", dsh_root=args.dsh_root)
    runtime_before = case_tools.runtime_identity(runtime_args)
    if runtime_before["versions"] != "DSH0.2.1-rc.2/Node24.20.0":
        raise ValueError("pinned DSH runtime identity did not match the frozen version")

    CASE_ROOT.mkdir(parents=True, exist_ok=True)
    case = CASE_ROOT / args.case_name
    if case.exists():
        raise ValueError("diagnostic case already exists; evidence will never be overwritten")
    case.mkdir(mode=0o700)
    workspace = case / "workspace"
    workspace.mkdir()
    read_source_path(workspace).write_bytes(read_source_bytes())
    host_home = case / "host-home"
    host_home.mkdir(mode=0o700)
    tmp = case / "tmp"
    tmp.mkdir(mode=0o700)
    dsh_home = case / "dsh-home"
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(host_home),
           "TMPDIR": str(tmp), "LANG": "en_US.UTF-8"}

    stats = RequestStats()
    handler = create_provider_handler(workspace, stats, endpoint="dsh")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    server.daemon_threads = False
    server.block_on_close = True
    server_thread = threading.Thread(target=server.serve_forever, name="dsh-inventory-provider", daemon=True)
    server_started = False
    provider_url = f"http://127.0.0.1:{server.server_port}/v1"
    dsh_env = dict(env, HOME=str(dsh_home), DSH_HOME=str(dsh_home),
                   DSH_AGENTS_HOME=str(dsh_home / "agents"), DSH_TELEMETRY_DISABLED="1",
                   AGENTKIT_SYNTHETIC_KEY="synthetic-key")
    sampler = ProcessSampler(case / "process-rss.jsonl", pi_embedded=False)
    sampler_started = False
    context = None
    fixture = None
    error = None
    fixture_errors = []
    scope_records = []
    ids = []
    fixture_exit_status = None
    fixture_diagnostic_tail = None
    sampler_receipt = None
    orphan_census = None

    input_record = {
        "diagnosticOnly": True,
        "officialSamples": 0,
        "performanceAccepted": False,
        "endpoint": "dsh",
        "sourceRole": "candidate",
        "caseName": args.case_name,
        "program": program_receipt,
        "programPath": str(program),
        "programSha256": hashlib.sha256(program.read_bytes()).hexdigest(),
        "runtimeBefore": runtime_before,
        "dshRoot": str(args.dsh_root.resolve(strict=True)),
        "scopeCount": 2,
        "seedProfile": {"sessions": 2, "turnsPerSession": 50, "toolGroupsPerSession": 10},
        "measurement": "one unmeasured round-0 send per session",
        "publicListOperation": "headless op=list (confirmed in crates/headless/src/main.rs)",
        "listIsCachedRuntimeView": True,
        "independentFreshEndpointListClaimed": False,
        "privateStorageRead": False,
        "baselineClaimed": False,
    }
    try:
        write_json(case / "inputs.json", input_record)
        server_thread.start()
        server_started = True
        sampler.register(os.getpid(), "provider")
        sampler.start()
        sampler_started = True
        context = owned_dsh(
            args.dsh_root, case / "data", dsh_home, workspace, provider_url,
            dict(dsh_env), query_sqlite=False, performance_case=case,
            on_process_started=lambda process: sampler.register(process, "harness"),
        )
        fixture = context.__enter__()
        connector = dict(fixture.connector_config([]), endpointId="dsh")
        ids, seed_proofs = case_tools.public_seed(
            "dsh", connector, program, workspace, case, env, sampler,
            "endpoint-test", "endpoint-test",
        )
        write_json(case / "seed-public-proof.json", {
            "sessions": ids,
            "seedProofs": seed_proofs,
            "profile": input_record["seedProfile"],
        })
        for index, session_id in enumerate(ids):
            scope_records.append(run_scope(case_tools, program, connector, case, env,
                                           sampler, session_id, index))
        after_runtime = case_tools.runtime_identity(runtime_args)
        if after_runtime != runtime_before:
            raise ValueError("pinned DSH runtime identity changed during public probe")
    except BaseException as caught:
        error = f"{type(caught).__name__}: {caught}"
        write_json(case / "failure.json", {"error": error, "traceback": traceback.format_exc()})
    finally:
        if fixture is not None:
            try:
                fixture_diagnostic_tail = fixture.diagnostic_tail()
                context.__exit__(None, None, None)
                fixture_exit_status = fixture.process.poll()
            except BaseException as caught:
                fixture_errors.append(f"DSH fixture close: {type(caught).__name__}: {caught}")
                try:
                    fixture_exit_status = fixture.process.poll()
                except BaseException:
                    pass
        if sampler_started:
            try:
                sampler_receipt = sampler.stop()
            except BaseException as caught:
                fixture_errors.append(f"process sampler stop: {type(caught).__name__}: {caught}")
        try:
            if server_started:
                server.shutdown()
            server.server_close()
            if server_started:
                server_thread.join(timeout=10)
            if server_started and server_thread.is_alive():
                fixture_errors.append("provider server thread did not retire")
        except BaseException as caught:
            fixture_errors.append(f"provider close: {type(caught).__name__}: {caught}")
        try:
            rows = process_table()
            with sampler.lock:
                roots, known = dict(sampler.roots), dict(sampler.known)
            owned = classify(rows, roots, known, pi_embedded=False)
            orphan_census = [row for row in owned.values() if row["role"] in ("agentkit", "harness")]
            if orphan_census:
                fixture_errors.append('owned AgentKit/DSH process remains after close')
        except BaseException as caught:
            fixture_errors.append(f"post-close process census: {type(caught).__name__}: {caught}")
        provider_receipt = stats.snapshot()
        if provider_receipt['errorResponses']:
            fixture_errors.append('provider reported an error response')
        if fixture is not None and fixture_exit_status != 0:
            fixture_errors.append('owned DSH did not exit successfully')
        try:
            case_tools.validate_program(args.program, args.program_receipt)
            if case_tools.runtime_identity(runtime_args) != runtime_before:
                fixture_errors.append('fixed runtime inputs changed')
        except Exception as caught:
            fixture_errors.append('input identity recheck failed: ' + type(caught).__name__)
        write_json(case / "provider-receipt.json", provider_receipt)
        final = {
            "diagnosticOnly": True,
            "officialSamples": 0,
            "performanceAccepted": False,
            "caseName": args.case_name,
            "sessionIds": ids,
            "scopeCountCompleted": len(scope_records),
            "scopeRecords": [str(case / f"scope-{index}-inventory-diagnostic.json")
                             for index in range(len(scope_records))],
            "error": error,
            "cleanupErrors": fixture_errors,
            "dshExitStatus": fixture_exit_status,
            "dshDiagnosticTail": fixture_diagnostic_tail,
            "samplerReceipt": sampler_receipt,
            "postCloseOwnedProcessCensus": orphan_census,
            "providerReceipt": str(case / "provider-receipt.json"),
            "complete": error is None and not fixture_errors and len(scope_records) == 2,
        }
        write_json(case / "diagnostic-receipt.json", final)
    return 0 if error is None and not fixture_errors and len(scope_records) == 2 else 1


if __name__ == "__main__":
    raise SystemExit(main())
