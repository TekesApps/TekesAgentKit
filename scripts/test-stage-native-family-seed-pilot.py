#!/usr/bin/env python3
"""Run one finite Native family seed pilot over the public headless API.

This is a diagnostic data-shape pilot, not performance acceptance. It creates
exactly one bounded child workload and one actual Earlier request; it never
retries or grows the workload to force pagination.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import threading
import time
import traceback
import uuid

from native_owned_fixture import owned_native
from performance_native_family_seed import (
    _entry_sequence,
    diagnostic_candidate_receipt,
    earlier_history_request,
    family_history_commands,
    family_native_provider,
    native_family_seed_commands,
    validate_public_child_catalog,
    validate_public_family_groundtruth,
    validate_public_family_pages,
)
from performance_process_sampler import ProcessSampler, classify, process_table

FIXTURE_PROOF = "owned Native paged child proof\n"
PROVIDER_CONFIG = Path(
    "/Users/frankqin/dev/Tekes/.build/kernel-0.9.3-installed/"
    "BuiltInKernel/fixtures/config/providers.canonical.json"
)
DEFAULT_SUPERVISOR = Path(
    "/Users/frankqin/dev/Tekes/.build/kernel-0.9.3-installed/"
    "BuiltInKernel/tekes-supervisor")
DEFAULT_WORKER = DEFAULT_SUPERVISOR.with_name("tekes-worker")
ROOT = Path("/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261025/performance")


def _load_case_tools():
    path = Path(__file__).with_name("test-stage-performance-case.py")
    spec = importlib.util.spec_from_file_location("owned_performance_case", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load the established owned performance helpers")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _write_once(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def _result(rows: list[dict], operation: str) -> dict:
    matches = [row.get("result") for row in rows if row.get("op") == operation]
    if len(matches) != 1 or not isinstance(matches[0], dict):
        raise ValueError(f"expected one public {operation} result for {method or 'operation'}")
    return matches[0]


def _call(method: str, args: dict) -> dict:
    return {"op": "call", "method": method, "args": args}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--program", type=Path, required=True)
    parser.add_argument("--program-receipt", type=Path, required=True)
    parser.add_argument("--supervisor", type=Path, default=DEFAULT_SUPERVISOR)
    parser.add_argument("--worker", type=Path, default=DEFAULT_WORKER)
    parser.add_argument("--providers-config", type=Path, default=PROVIDER_CONFIG)
    parser.add_argument("--case-name", required=True)
    args = parser.parse_args()
    if not args.case_name.replace("-", "").isalnum():
        parser.error("case name must be a simple owned name")
    case = ROOT / args.case_name
    if case.exists() or case.is_symlink():
        parser.error("the owned case already exists; evidence is never overwritten")
    ROOT.mkdir(parents=True, exist_ok=True)
    program_tools = _load_case_tools()
    program, program_receipt = program_tools.validate_program(args.program, args.program_receipt)
    runtime = program_tools.runtime_identity(argparse.Namespace(
        endpoint="native", kernel_supervisor=args.supervisor, kernel_worker=args.worker,
        providers_config=args.providers_config))
    case.mkdir(mode=0o700)
    workspace = case / "workspace"
    workspace.mkdir()
    (workspace / "proof.txt").write_text(FIXTURE_PROOF, encoding="utf-8")
    for name in ("data", "tmp"):
        (case / name).mkdir(mode=0o700)
    home = case / "host-home"
    home.mkdir(mode=0o700)
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home),
           "TMPDIR": str(case / "tmp"), "LANG": "en_US.UTF-8"}
    parent_id = str(uuid.uuid4())
    provider_requests: list[str] = []
    Provider, provider_state = family_native_provider(workspace=workspace, requests=provider_requests)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.daemon_threads = False
    server.block_on_close = True
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    sampler = ProcessSampler(case / "process-rss.jsonl", pi_embedded=False)
    sampler.register(os.getpid(), "provider")
    sampler.start()
    fixture_context = None
    protocol = None
    failure = None
    process_exits: list[dict] = []
    kernel_exit: dict | None = None
    fixture = None
    try:
        canonical = json.loads(args.providers_config.read_text(encoding="utf-8"))
        if not isinstance(canonical.get("providers"), list) or not canonical["providers"]:
            raise ValueError("fixed Kernel provider config has no provider")
        providers = dict(canonical, providers=[canonical["providers"][0]])
        url = f"http://127.0.0.1:{server.server_port}/v1"
        fixture_context = owned_native(
            args.supervisor.resolve(), args.worker.resolve(), providers, url, case, env,
            performance_case=case,
            on_process_started=lambda process: sampler.register(process, "harness"))
        fixture = fixture_context.__enter__()
        fixture_root = program_tools.fixture_root_observation("native", fixture, sampler)
        provider_info = fixture["providers"]["providers"][0]
        seed_commands = native_family_seed_commands(
            parent_id, workspace, provider_info["id"], provider_info["models"][0]["id"])
        commands = seed_commands + [
            {"op": "waitSettled", "sessionId": "parent"},
            {"op": "presentation", "sessionId": "parent"},
            {"op": "childCatalog", "sessionId": "parent", "saveAs": "bounded",
             "savePointer": "/entries/1/id"},
        ]
        rows, process_exit = program_tools.run_program(
            program, {"kind": "native", "endpointId": "native", "url": fixture["url"],
                     "bearer": fixture["bearer"], "expectedVersion": "0.9.3",
                     "commands": commands}, case, "family-seed", env, sampler, timeout=300)
        process_exits.append(process_exit)
        if not rows or rows[0].get("version") != "0.9.3":
            raise ValueError("fixed Kernel 0.9.3 handshake mismatch")
        parent_view = _result(rows, "presentation")
        if parent_view.get("run", {}).get("scope", {}).get("sessionId") != parent_id:
            raise ValueError("public Task parent presentation changed its original scope")
        catalog = _result(rows, "childCatalog")
        child_id = validate_public_child_catalog(catalog, parent_id)
        ground_command, head_command = family_history_commands(parent_id, child_id)
        history_rows, history_process_exit = program_tools.run_program(
            program, {"kind": "native", "endpointId": "native", "url": fixture["url"],
                     "bearer": fixture["bearer"], "expectedVersion": "0.9.3",
                     "commands": [ground_command, head_command]}, case, "family-groundtruth", env,
            sampler, timeout=120)
        process_exits.append({"phase": "full-history-and-initial-page", "processes": history_process_exit})
        history_results = [row.get("result") for row in history_rows if row.get("op") == "call"]
        if len(history_results) != 2 or not all(isinstance(item, dict) for item in history_results):
            raise ValueError("public groundtruth and initial page calls were not both returned")
        groundtruth, first_page = history_results
        validated = validate_public_family_groundtruth(
            groundtruth, parent={"sessionId": parent_id}, child_id=child_id)
        seqs = [_entry_sequence(item) for item in first_page.get("entries", [])]
        if not seqs:
            raise ValueError("public initial family history page returned no entries")
        frontier = min(seqs)
        earlier = earlier_history_request(parent_id, child_id, groundtruth["throughSequence"],
                                          groundtruth["historyRevision"], frontier)
        earlier_rows, earlier_process_exit = program_tools.run_program(
            program, {"kind": "native", "endpointId": "native", "url": fixture["url"],
                     "bearer": fixture["bearer"], "expectedVersion": "0.9.3",
                     "commands": [_call("subagents.history", earlier)]}, case, "family-earlier", env,
            sampler, timeout=120)
        process_exits.append({"phase": "earlier-page", "processes": earlier_process_exit})
        second_page = _result(earlier_rows, "call")
        pages = validate_public_family_pages(validated, [first_page, second_page],
                                             [head_command["args"], earlier])
        protocol = diagnostic_candidate_receipt(
            parent_session_id=parent_id, child_session_id=child_id,
            provider_state=provider_state, history=groundtruth, pages=pages)
        protocol.update({"runtime": runtime, "program": program_receipt,
                         "delegatedParentProviderRequests": len(provider_requests),
                         "childProviderRequests": provider_state["childRequests"],
                         "delegatedParentProviderRequestPaths": provider_requests,
                         "publicParentCatalogChildCount": sum(
                             item.get("kind") == "child" for item in catalog.get("entries", [])),
                         "publicHistoryPages": 2, "historyEntriesCovered": pages["pagePlanEntryCount"],
                         "historyEntryUnionMatchesGroundtruth": (
                             sorted(_entry_sequence(item) for page in (first_page, second_page)
                                    for item in page["entries"])
                             == sorted(_entry_sequence(item) for item in groundtruth["entries"])),
                         "parentTurnSettled": True,
                         "fixtureRoot": fixture_root})
    except Exception as error:
        failure = type(error).__name__ + ": " + str(error)
        (case / "failure-traceback.log").write_text(traceback.format_exc(), encoding="utf-8")
    finally:
        if fixture_context is not None:
            try:
                fixture_context.__exit__(None, None, None)
                if fixture is not None:
                    kernel_exit = {"pid": fixture["process"].pid,
                                   "exitStatus": fixture["process"].poll()}
                    if kernel_exit["exitStatus"] != 0:
                        failure = failure or "owned Kernel supervisor exited unsuccessfully"
            except Exception as error:
                failure = failure or "owned Kernel shutdown failed: " + type(error).__name__
        try:
            rss = sampler.stop()
            program_tools.write(case / "rss-summary.json", rss)
        except Exception as error:
            failure = failure or "owned process sampler did not stop: " + type(error).__name__
        try:
            with sampler.lock:
                roots, known = dict(sampler.roots), dict(sampler.known)
            remaining = [item for item in classify(process_table(), roots, known).values()
                         if item["role"] in ("agentkit", "harness")]
            if remaining:
                failure = failure or "owned AgentKit/Kernel processes remain after pilot"
        except Exception as error:
            failure = failure or "owned process retirement could not be verified: " + type(error).__name__
        try:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
        except Exception as error:
            failure = failure or "owned provider did not shut down: " + type(error).__name__
    receipt = {"diagnosticOnly": True, "officialSamples": 0, "performanceAccepted": False,
               "runtime": runtime, "program": program_receipt, "processExits": process_exits,
               "kernelExit": kernel_exit,
               "error": failure, "protocol": protocol}
    _write_once(case / "pilot-receipt.json", receipt)
    return 0 if failure is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
