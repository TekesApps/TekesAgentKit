#!/usr/bin/env python3
"""Capture one preregistered Native family performance case.

The registration and both profile layers are validated before creating an
evidence directory, probing a runtime, or starting a process. This runner
records one original execution only; it never upgrades a diagnostic pilot or
retries a failed sample.
"""
from __future__ import annotations

import argparse
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
import time
import traceback
import uuid

from native_owned_fixture import owned_native
from performance_native_family_seed import (
    _entry_sequence,
    earlier_history_request,
    family_history_commands,
    family_native_provider,
    native_family_seed_commands,
    validate_public_child_catalog,
    validate_public_family_groundtruth,
    validate_public_family_pages,
)
from performance_official_registration import (
    sha256 as file_sha256,
    validate_registration,
)
from performance_owned_receipt import IDENTITY_NAME, seal_closed_seed_receipt
from performance_process_sampler import ProcessSampler, classify, process_table
from performance_provider_fixture import read_source_bytes, read_source_path

ROOT = Path("/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261025/performance")
FAMILY_KIND = "agentkit-native-family-profile"
PARENT_DRAFT = "AKFAMILY_PARENT_DRAFT_KEEP"
EXPECTED_PROFILE_NAME = "agentkit-native-family-performance-profile-20261026.json"
EXPECTED_CASES = {"cold": 5, "warm": 1, "reconnectLifecycle": 1}
EXPECTED_ITERATIONS = {"cold": 1, "warm": 11, "reconnectLifecycle": 11}


def _load_case_tools():
    path = Path(__file__).with_name("test-stage-performance-case.py")
    spec = importlib.util.spec_from_file_location("owned_performance_case", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load the established owned performance helpers")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def _rows(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _write_once(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def _hash_value(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=False).encode("utf-8")).hexdigest()


def _result(rows: list[dict], operation: str) -> dict:
    matches = [row.get("result") for row in rows if row.get("op") == operation]
    if len(matches) != 1 or not isinstance(matches[0], dict):
        raise ValueError(f"expected one public {operation} result")
    return matches[0]


def _call(method: str, args: dict) -> dict:
    return {"op": "call", "method": method, "args": args}


def _file_pin(path_value: object, digest: object, label: str) -> Path:
    if not isinstance(path_value, str) or not Path(path_value).is_absolute():
        raise ValueError(f"{label} path must be absolute")
    path = Path(path_value)
    if path.is_symlink() or not path.is_file() or not isinstance(digest, str):
        raise ValueError(f"{label} must be a pinned regular file")
    if file_sha256(path) != digest:
        raise ValueError(f"{label} SHA-256 differs from registration")
    return path.resolve(strict=True)


def validate_family_registration(pins: dict, *, family_path: Path | None = None) -> dict:
    """Add the frozen family contract checks to the general registration gate."""
    registration = pins["registration"]
    family_file = _file_pin(registration.get("familyProfilePath"),
                            registration.get("familyProfileSha256"), "family profile")
    family = _json(family_file)
    if family.get("kind") != FAMILY_KIND or family.get("format") != 1:
        raise ValueError("registration family profile is not the frozen Native family format")
    if family.get("status") != "preregistered-not-run" or family.get("issue") != 56:
        raise ValueError("family profile is not the frozen #56 preregistration")
    if family.get("baseProfile") != pins["profilePath"].name:
        raise ValueError("family baseProfile filename differs from the registered stage profile")
    if family.get("baseProfileSha256") != file_sha256(pins["profilePath"]):
        raise ValueError("family profile baseProfileSHA differs from the registered base profile")
    if family.get("sourceProducts") != {
            "baseline": pins["profile"].get("baseline"),
            "candidate": pins["profile"].get("resolvedProductCandidate")}:
        raise ValueError("family role source pins differ from the frozen stage profile")
    if family.get("thresholds") != pins["profile"].get("thresholds"):
        raise ValueError("family thresholds differ from the frozen base profile")
    producer = Path(__file__).with_name("performance_native_family_seed.py")
    if (not producer.is_file() or producer.is_symlink()
            or file_sha256(producer) != family.get("seed", {}).get("producerSourceSha256")):
        raise ValueError("Native family seed producer source differs from the preregistered profile pin")
    if family.get("runtime") != {"kind": "native", "kernelVersion": "0.9.3",
                                 "source": "09508a9083c24c7accefc99516739f279b85913e"}:
        raise ValueError("family runtime differs from the frozen Native 0.9.3 pin")
    role = registration.get("role")
    product_path = registration.get("familyPath")
    if product_path not in ("common50", "product20"):
        raise ValueError("registration familyPath must be common50 or product20")
    path_profile = family.get("paths", {}).get(product_path)
    if not isinstance(path_profile, dict) or role not in path_profile.get("roles", []):
        raise ValueError("registered role is not permitted for the selected family path")
    if product_path == "product20" and role != "candidate":
        raise ValueError("product20 is candidate-only")
    if family_path is not None and family_path != Path(product_path):
        raise ValueError("CLI family path differs from the immutable registration")
    if registration.get("endpoint") != "native":
        raise ValueError("family registrations are Native only")
    if registration.get("sourceCommit") != family.get("sourceProducts", {}).get(role):
        raise ValueError("registered sourceCommit differs from the frozen family role pin")
    if registration.get("iterations") != EXPECTED_ITERATIONS.get(registration.get("mode")):
        raise ValueError("registered iteration count differs from the family mode")
    if (type(registration.get("caseIndex")) is not int
            or not 0 <= registration["caseIndex"] < EXPECTED_CASES[registration["mode"]]):
        raise ValueError("registered caseIndex is outside the family mode trajectory")
    if type(registration.get("caseIndex")) is not int:
        raise ValueError("caseIndex must be an integer")
    if registration.get("plannedCaseUpperBound", family.get("plannedCaseUpperBound")) != 21:
        raise ValueError("family plan exceeds the frozen 21-case upper bound")
    counts = family.get("caseCountsPerPathRole", {})
    planned_total = sum(len(path_value.get("roles", [])) *
                        (counts.get("cold", 0) + counts.get("warm", 0)
                         + counts.get("reconnectLifecycle", 0))
                        for path_value in family.get("paths", {}).values()
                        if isinstance(path_value, dict))
    if (planned_total != 21 or family.get("plannedCaseUpperBound") != 21
            or counts != EXPECTED_CASES):
        raise ValueError("frozen family matrix does not define exactly the bounded 21-case plan")

    receipt = pins["programReceipt"]
    expected_features = ([] if product_path == "common50" else
                         ["acceptance-native-family-page20"])
    if receipt.get("cargoFeatures") != expected_features:
        raise ValueError("program cargoFeatures differ from the selected registered family path")
    if product_path == "product20" and role != "candidate":
        raise ValueError("product20 requires a candidate binary")
    if product_path == "product20" and pins["programReceipt"].get("sourceCommit") != family["sourceProducts"]["candidate"]:
        raise ValueError("product20 binary is not built from the candidate source pin")
    if family_file.name != EXPECTED_PROFILE_NAME:
        raise ValueError("registration does not pin the dated #56 family profile")
    return {"familyPath": family_file, "familyProfile": family}


def _validate_outer(registration_path: Path, *, pilot: bool = False) -> tuple[dict, dict]:
    driver = Path(sys.executable).resolve(strict=True)
    own = Path(__file__).resolve(strict=True)
    pins = validate_registration(registration_path, driver_path=driver,
                                 official_runner_path=own, caller_role="outer")
    family = validate_family_registration(pins)
    registration = pins["registration"]
    _validate_purpose(registration, pilot=pilot)
    if registration.get("officialRunnerPath") != str(own):
        raise ValueError("officialRunnerPath must identify this fixed Native family runner")
    if registration.get("caseRunnerPath") != str(own):
        raise ValueError("caseRunnerPath must identify this fixed Native family runner")
    return pins, family


def _validate_capture(registration_path: Path, *, pilot: bool = False) -> tuple[dict, dict]:
    driver = Path(sys.executable).resolve(strict=True)
    own = Path(__file__).resolve(strict=True)
    pins = validate_registration(registration_path, driver_path=driver,
                                 official_runner_path=own, caller_role="capture")
    family = validate_family_registration(pins)
    _validate_purpose(pins["registration"], pilot=pilot)
    if pins["registration"].get("officialRunnerPath") != str(own):
        raise ValueError("officialRunnerPath must identify this fixed Native family runner")
    if pins["registration"].get("caseRunnerPath") != str(own):
        raise ValueError("caseRunnerPath must identify this fixed Native family runner")
    return pins, family


def _validate_purpose(registration: dict, *, pilot: bool) -> None:
    purpose = registration.get("purpose")
    diagnostic = registration.get("diagnosticOnly")
    if pilot:
        if purpose != "toolchain-pilot" or diagnostic is not True or registration.get("official") is not True:
            raise ValueError("--pilot requires an explicitly registered diagnostic toolchain-pilot")
    elif purpose == "toolchain-pilot" or diagnostic is True:
        raise ValueError("diagnostic pilot registration requires --pilot and cannot enter the official matrix")
    elif purpose not in (None, "official") or diagnostic not in (None, False):
        raise ValueError("unknown Native family registration purpose")


def _program_feature_preflight(program: Path, receipt: dict, path: str, role: str) -> None:
    features = receipt.get("cargoFeatures")
    required = [] if path == "common50" else ["acceptance-native-family-page20"]
    if features != required or (path == "product20" and role != "candidate"):
        raise ValueError("program feature/role preflight rejected the selected Native family path")
    if not os.access(program, os.X_OK):
        raise ValueError("registered acceptance program is not executable")


def _provider_snapshot(requests: list[str], state: dict) -> dict:
    counts = {"delegatedParentRequests": len(requests),
              "boundedChildRequests": state.get("childRequests", 0)}
    return {"requestCounts": counts,
            "delegatedParentProviderRequests": len(requests),
            "delegatedParentProviderRequestPaths": list(requests),
            "childProviderRequests": state.get("childRequests"),
            "stepReceipts": state.get("stepReceipts"),
            "finalResponseId": state.get("finalResponseId"),
            "diagnosticSeedMetadata": {key: state.get(key) for key in
                ("diagnosticOnly", "officialSamples")}}


def _machine_evidence(tools) -> dict:
    value = tools.machine_identity()
    from performance_sample_report import _stable_machine_identity
    value["stableIdentity"] = _stable_machine_identity(value)
    return value


def _runtime_evidence(runtime: dict | None) -> dict | None:
    if runtime is None:
        return None
    return {"kind": "native", "version": "0.9.3",
            "source": "09508a9083c24c7accefc99516739f279b85913e",
            "criticalFiles": runtime.get("criticalFiles")}


def _semantic_seed_proof(history: dict, validated: dict, family: dict) -> dict:
    """Derive the cross-role corpus digest from the original public history."""
    from performance_native_family_seed import (
        COMMENTARY_COUNT, _original_call_id, _event_text, _message_text, commentary_body,
    )

    entries = history["entries"]
    assistant = [item for item in entries
                 if item.get("event", {}).get("type") == "assistant/message"
                 and item.get("event", {}).get("data", {}).get("message", {}).get("role") == "assistant"]
    commentary = [_event_text(item) for item in assistant
                  if _event_text(item).startswith("AKFAMILY|native|")]
    if commentary != [commentary_body(i) for i in range(1, COMMENTARY_COUNT + 1)]:
        raise ValueError("semantic seed proof does not contain the original ordered 61 commentary bodies")
    calls = [item for item in entries if item.get("event", {}).get("type") == "tool/call"
             and item.get("event", {}).get("data", {}).get("name") == "read"]
    results = [item for item in entries if item.get("event", {}).get("type") == "tool/result"]
    if len(calls) != 61 or len(results) != 61:
        raise ValueError("semantic seed proof must retain all 61 original read call/result groups")
    read_groups = []
    original_ids = []
    for ordinal, (call, result) in enumerate(zip(calls, results), 1):
        call_data = call["event"].get("data", {})
        result_data = result["event"].get("data", {})
        call_id = _original_call_id(call_data)
        result_id = _original_call_id(result_data)
        if call_id is None or result_id != call_id:
            raise ValueError("original read call/result IDs do not match")
        args = call_data.get("arguments")
        if isinstance(args, str):
            args = json.loads(args)
        relative = args.get("path", args.get("file_path")) if isinstance(args, dict) else None
        result_message = result_data.get("message")
        result_text = _message_text(result_message)
        result_payload = json.loads(result_text)
        expected_text = "owned Native paged child proof\n"
        expected_bytes = len(expected_text.encode("utf-8"))
        expected_hash = hashlib.sha256(expected_text.encode("utf-8")).hexdigest()
        if (relative != "proof.txt" or result_payload.get("path") != relative
                or result_payload.get("bytes") != expected_bytes
                or result_payload.get("sha256") != "sha256-" + expected_hash
                or result_payload.get("truncated") is not False):
            raise ValueError("original read group differs from the fixed public proof file")
        read_groups.append({"ordinal": ordinal, "name": "read", "path": relative,
                            "bytes": expected_bytes, "sha256": expected_hash})
        original_ids.append({"ordinal": ordinal, "callId": call_id, "resultId": result_id})
    final = [item for item in assistant
             if item.get("event", {}).get("data", {}).get("sessionFinal") is True]
    final_text = _event_text(final[-1]) if final else None
    semantic = {"commentaryBodies": commentary, "readGroups": read_groups,
                "finalText": final_text}
    digest = _hash_value(semantic)
    expected_digest = family.get("seed", {}).get("semanticCorpusSha256")
    if digest != expected_digest:
        raise ValueError("normalized public seed corpus SHA-256 differs from the frozen family profile")
    return {"normalizedContent": semantic, "semanticCorpus": semantic,
            "semanticCorpusSha256": digest,
            "originalReadCallResultIds": original_ids,
            "parentSessionId": validated["parentSessionId"],
            "childSessionId": validated["childSessionId"],
            "throughSequence": history["throughSequence"],
            "historyRevision": history["historyRevision"],
            "entryCount": len(history["entries"]),
            "publicHeadEntryCount": len(validated["pagePlan"][0]["entries"]),
            "publicEarlierEntryCount": len(validated["pagePlan"][1]["entries"]),
            "publicPagePlanEntryCount": validated["pagePlanEntryCount"],
            "rawHistoryRetained": True}


def _assert_native_args(registration: dict) -> dict:
    args = registration["runtimeArguments"]
    required = ("kernel-supervisor", "kernel-worker", "providers-config")
    if any(not isinstance(args.get(key), str) or not Path(args[key]).is_absolute() for key in required):
        raise ValueError("Native runtime arguments are incomplete")
    return {key.replace("-", "_"): Path(args[key]) for key in required}


def execute(registration_path: Path, *, pilot: bool = False) -> Path:
    # All input/profile/source/runtime pins are read and checked before mkdir,
    # version probes, or any child process.
    registration_path = Path(registration_path).resolve(strict=True)
    pins, family_pins = _validate_outer(registration_path, pilot=pilot)
    reg = pins["registration"]
    _validate_purpose(reg, pilot=pilot)
    family = family_pins["familyProfile"]
    run_args = _assert_native_args(reg)
    _program_feature_preflight(pins["program"], pins["programReceipt"],
                               reg["familyPath"], reg["role"])
    case = ROOT / reg["caseName"]
    if case.exists() or case.is_symlink():
        raise ValueError("registered case already exists; evidence is never overwritten")

    # This second, independent full validation is the capture-side guard and
    # still precedes the first directory or runtime probe.
    capture_pins, capture_family = _validate_capture(registration_path, pilot=pilot)
    if capture_family["familyProfile"] != family:
        raise ValueError("outer and capture family profiles differ")
    case.parent.mkdir(parents=True, exist_ok=True)
    case.mkdir(mode=0o700)

    tools = _load_case_tools()
    failure: str | None = None
    failure_reasons: list[str] = []
    fixture_context = None
    fixture = None
    server = None
    provider_thread = None
    sampler = None
    consumer_exit = None
    consumer_events: list[dict] = []
    fixture_lifecycle: list[dict] = []
    fixture_roots: list[dict] = []
    cold_kernel_restart = None
    seed_rss_summary = None
    measured_rss = None
    retirement = None
    provider_requests: list[str] = []
    provider_state: dict = {}
    runtime_before = None
    machine_before = None
    identity_rechecked = None
    parent_id = str(uuid.uuid4())
    child_id = None
    source_history = None
    seed_proof = None
    source_commands = []
    source_rows = []
    seed_provider_record = None
    registered_sha = file_sha256(registration_path)
    case_record = {"format": 1, "official": not pilot, "diagnosticOnly": pilot,
                   "purpose": "toolchain-pilot" if pilot else reg.get("purpose", "official"),
                   "sampleId": reg["sampleId"], "caseName": reg["caseName"],
                   "role": reg["role"], "sourceCommit": reg["sourceCommit"],
                   "familyPath": reg["familyPath"], "mode": reg["mode"],
                   "caseIndex": reg["caseIndex"], "registrationSha256": registered_sha}
    try:
        # Exact runtime hashes/versions and machine state are checked only once
        # the complete registration/profile validation above has succeeded.
        machine_before = _machine_evidence(tools)
        workspace = case / "workspace"
        workspace.mkdir(mode=0o700)
        (workspace / "proof.txt").write_text(family["seed"]["proofText"], encoding="utf-8")
        (case / "data").mkdir(mode=0o700)
        home = case / "host-home"
        home.mkdir(mode=0o700)
        tmp = case / "tmp"
        tmp.mkdir(mode=0o700)
        env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home),
               "TMPDIR": str(tmp), "LANG": "en_US.UTF-8"}
        _write_once(case / "inputs.json", {
            **case_record,
            "machineBefore": machine_before,
            "registrationSha256": registered_sha,
            "registrationBytes": pins["registrationBytes"].decode("utf-8"),
            "baseProfilePath": str(pins["profilePath"]),
            "baseProfileSha256": file_sha256(pins["profilePath"]),
            "familyProfilePath": str(family_pins["familyPath"]),
            "familyProfileSha256": file_sha256(family_pins["familyPath"]),
            "profileSha256": file_sha256(pins["profilePath"]),
            "familyPath": reg["familyPath"],
            "program": pins["programReceipt"], "programPath": str(pins["program"]),
            "programSha256": file_sha256(pins["program"]),
            "programReceiptSha256": file_sha256(Path(reg["programReceiptPath"])),
            "seeder": pins["seederReceipt"], "seederSha256": file_sha256(pins["seeder"]),
            "consumerSourcePath": str(pins["consumer"]),
            "consumerSourceSha256": file_sha256(pins["consumer"]),
            "runnerPath": str(Path(__file__).resolve()),
            "runnerSourceSha256": file_sha256(Path(__file__).resolve()),
            "runtimeFiles": reg["runtimeFiles"],
            "runtimeSha256": reg["runtimeSha256"],
            "runtimeArguments": reg["runtimeArguments"],
            "sourceCommit": reg["sourceCommit"],
            "consumerSha256": reg["consumerSha256"],
            "observerSha256": reg.get("observerSha256", reg["consumerSha256"]),
            "runnerSha256": reg["runnerSha256"],
            "programSha256": reg["programSha256"],
        })
        runtime_before = tools.runtime_identity(argparse.Namespace(
            endpoint="native", kernel_supervisor=run_args["kernel_supervisor"],
            kernel_worker=run_args["kernel_worker"], providers_config=run_args["providers_config"]))
        canonical_providers = _json(run_args["providers_config"])
        if not isinstance(canonical_providers.get("providers"), list) or not canonical_providers["providers"]:
            raise ValueError("fixed Native provider configuration is empty")
        provider_config = dict(canonical_providers, providers=[canonical_providers["providers"][0]])
        provider = provider_config["providers"][0]
        model_id = provider["models"][0]["id"]
        Provider, provider_state = family_native_provider(workspace=workspace, requests=provider_requests)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        server.daemon_threads = False
        server.block_on_close = True
        provider_thread = threading.Thread(target=server.serve_forever,
                                           name="native-family-loopback-provider", daemon=True)
        provider_thread.start()
        provider_url = f"http://127.0.0.1:{server.server_port}/v1"

        sampler = ProcessSampler(case / "seed-rss.jsonl", pi_embedded=False, interval_ms=50)
        sampler.register(os.getpid(), "provider")
        sampler.start()

        def start_kernel(resume_receipt=None):
            nonlocal fixture_context, fixture
            intent = time.monotonic_ns()
            fixture_context = owned_native(
                run_args["kernel_supervisor"].resolve(strict=True),
                run_args["kernel_worker"].resolve(strict=True), provider_config,
                provider_url, case, env, performance_case=case,
                resume_receipt=resume_receipt,
                on_process_started=lambda process: sampler.register(process, "harness"))
            try:
                fixture = fixture_context.__enter__()
            except Exception:
                # owned_native unwinds its own partially started supervisor.
                fixture_context = None
                raise
            identity = tools.fixture_root_observation("native", fixture, sampler)
            fixture_roots.append(identity)
            fixture_lifecycle.append({"operation": "resume" if resume_receipt else "start",
                "intentMonotonicNs": intent, "readyMonotonicNs": time.monotonic_ns(),
                "process": identity, "exitStatus": None})
            return {"kind": "native", "identity": "native", "endpointId": "native", "url": fixture["url"],
                    "bearer": fixture["bearer"], "expectedVersion": "0.9.3"}

        connector = start_kernel()
        seed_commands = native_family_seed_commands(parent_id, workspace,
                                                    provider["id"], model_id)
        source_commands.extend(seed_commands)
        source_commands.extend([
            {"op": "waitSettled", "sessionId": "parent"},
            {"op": "presentation", "sessionId": "parent"},
            {"op": "childCatalog", "sessionId": "parent", "saveAs": "bounded",
             "savePointer": "/entries/1/id"},
        ])
        _write_once(case / "seed-commands.json", source_commands)
        source_rows, _seed_exit = tools.run_program(
            pins["seeder"], {"kind": "native", **connector, "commands": source_commands},
            case, "seed", env, sampler, timeout=600)
        if not source_rows or source_rows[0].get("version") != "0.9.3":
            raise ValueError("Native family seeder did not handshake with Kernel 0.9.3")
        parent_view = _result(source_rows, "presentation")
        parent_scope = parent_view.get("run", {}).get("scope", {})
        if parent_scope.get("sessionId") != parent_id:
            raise ValueError("real Native Task parent presentation changed its registered pointer")
        catalog = _result(source_rows, "childCatalog")
        child_id = validate_public_child_catalog(catalog, parent_id)
        ground_command, head_command = family_history_commands(parent_id, child_id)
        history_rows, _history_exit = tools.run_program(
            pins["seeder"], {"kind": "native", **connector,
                "commands": [ground_command, head_command]}, case, "seed-history", env,
            sampler, timeout=300)
        history_results = [row.get("result") for row in history_rows if row.get("op") == "call"]
        if len(history_results) != 2 or not all(isinstance(row, dict) for row in history_results):
            raise ValueError("public full200 groundtruth and max20 head were not returned")
        source_history, first_page = history_results
        validated = validate_public_family_groundtruth(
            source_history, parent={"sessionId": parent_id}, child_id=child_id)
        first_seqs = [_entry_sequence(item) for item in first_page.get("entries", [])]
        if not first_seqs:
            raise ValueError("public head20 page is empty")
        frontier = min(first_seqs)
        earlier = earlier_history_request(parent_id, child_id,
            source_history["throughSequence"], source_history["historyRevision"], frontier)
        earlier_rows, _earlier_exit = tools.run_program(
            pins["seeder"], {"kind": "native", **connector,
                "commands": [_call("subagents.history", earlier)]}, case,
            "seed-earlier", env, sampler, timeout=180)
        second_page = _result(earlier_rows, "call")
        validated = validate_public_family_pages(validated, [first_page, second_page],
                                                 [head_command["args"], earlier])
        seed_proof = _semantic_seed_proof(source_history, validated, family)
        if (provider_state.get("childRequests") != 62
                or len(provider_state.get("stepReceipts", [])) != 61
                or provider_state.get("finalResponseId") != "native-family-final"):
            raise ValueError("finite provider did not produce the exact 61-read child and final seed")
        _write_once(case / "seed-source-history.json", source_history)
        _write_once(case / "seed-proof.json", seed_proof)
        fixture_lifecycle.append({
            "operation": "seed_public_history_verified",
            "monotonicNs": time.monotonic_ns(),
            "parentSessionId": parent_id, "childSessionId": child_id,
            "sourceHistorySha256": file_sha256(case / "seed-source-history.json"),
            "seedProofSha256": file_sha256(case / "seed-proof.json"),
            "runOpen": False, "privateDatabaseRewrite": False})

        seed_provider_record = _provider_snapshot(provider_requests, provider_state)
        _write_once(case / "seed-provider-before-measurement.json", seed_provider_record)
        old_process = fixture["process"]
        if reg["mode"] == "cold":
            stop_intent = time.monotonic_ns()
            fixture_context.__exit__(None, None, None)
            fixture_context = None
            fixture_lifecycle.append({"operation": "seed_retire",
                "intentMonotonicNs": stop_intent, "completedMonotonicNs": time.monotonic_ns(),
                "process": next((row for row in reversed(fixture_roots)
                                 if row["pid"] == old_process.pid), None),
                "exitStatus": old_process.poll()})
            seed_rss_summary = sampler.stop()
            _write_once(case / "seed-rss-summary.json", seed_rss_summary)
            owned = [row for row in seed_rss_summary["knownProcesses"] if row["role"] != "provider"]
            seal_intent = time.monotonic_ns()
            closed_receipt = seal_closed_seed_receipt(
                case / IDENTITY_NAME, exit_code=old_process.poll(), known_processes=owned)
            fixture_lifecycle.append({
                "operation": "seed_receipt_sealed",
                "intentMonotonicNs": seal_intent,
                "completedMonotonicNs": time.monotonic_ns(),
                "receiptPath": str(closed_receipt),
                "receiptSha256": file_sha256(closed_receipt),
                "oldKernelRoot": next(row for row in reversed(fixture_roots)
                                       if row["pid"] == old_process.pid),
                "seedExitStatus": old_process.poll()})
            if old_process.poll() != 0:
                raise ValueError("seed Kernel did not retire cleanly before cold measurement")
            sampler = ProcessSampler(case / "measurement-rss.jsonl", pi_embedded=False,
                                     interval_ms=50)
            sampler.register(os.getpid(), "provider")
            sampler.start()
            connector = start_kernel(closed_receipt)
            if fixture["process"].pid == old_process.pid:
                raise ValueError("cold Kernel resume reused the retired PID")
            old_root = next(row for row in reversed(fixture_roots)
                            if row["pid"] == old_process.pid)
            new_root = next(row for row in reversed(fixture_roots)
                            if row["pid"] == fixture["process"].pid)
            cold_kernel_restart = {
                "seedClosed": True, "seedStateReused": True,
                "seedStateReuseScope": (
                    "only the sealed public dataset and original parent/child pointers are reused; "
                    "the Native supervisor starts with a new PID/start identity"),
                "oldPid": old_process.pid, "oldStartIdentity": old_root["startIdentity"],
                "seedExitStatus": old_process.poll(),
                "newPid": fixture["process"].pid, "newStartIdentity": new_root["startIdentity"],
                "consumerPid": None, "consumerStartIdentity": None,
                "parentSessionId": parent_id, "childSessionId": child_id,
                "seedReceiptSha256": file_sha256(closed_receipt),
                "privateDatabaseRewrite": False}
        else:
            seed_rss_summary = sampler.stop()
            _write_once(case / "seed-rss-summary.json", seed_rss_summary)
            sampler = ProcessSampler(case / "measurement-rss.jsonl", pi_embedded=False,
                                     interval_ms=50)
            sampler.register(os.getpid(), "provider")
            sampler.register(fixture["process"], "harness")
            sampler.start()

        payload = {"familyFormat": 1, "mode": reg["mode"], "path": reg["familyPath"],
                   "role": reg["role"], "caseIndex": reg["caseIndex"],
                   "connector": connector, "parentSessionId": parent_id,
                   "childSessionId": child_id, "sourceHistory": source_history,
                   "parentDraft": PARENT_DRAFT}
        _write_once(case / "family-consumer-input.json", payload)
        consumer_events, consumer_exit = tools.run_program(
            pins["program"], payload, case, "consumer", env, sampler, timeout=900)
        # Keep the consumer output byte-for-byte as emitted by run_program;
        # every parsed event remains in consumer.stdout.jsonl unchanged.
        consumer_events = _rows(case / "consumer.stdout.jsonl")
        expected_rounds = EXPECTED_ITERATIONS[reg["mode"]]
        starts = [row for row in consumer_events if row.get("event") == "family_iteration_started"]
        completed = [row for row in consumer_events if row.get("event") == "family_iteration_completed"]
        failed_rounds = [row for row in consumer_events if row.get("event") == "family_iteration_failed"]
        if (len(starts) != expected_rounds or len(completed) != expected_rounds or failed_rounds
                or any(row.get("path") != reg["familyPath"] or row.get("role") != reg["role"]
                       for row in starts + completed)):
            raise ValueError("original Rust family iteration events do not match the finite registered case")
        if consumer_exit.get("exitStatus") != 0:
            raise ValueError("original Native family consumer failed")
        if cold_kernel_restart is not None:
            cold_kernel_restart["consumerPid"] = consumer_exit.get("pid")
            cold_kernel_restart["consumerStartIdentity"] = consumer_exit.get("startIdentity")
            _write_once(case / "cold-kernel-restart.json", cold_kernel_restart)
    except Exception as error:
        failure = type(error).__name__ + ": " + str(error)
        failure_reasons.append(failure)
        try:
            _write_once(case / "failure-traceback.log", traceback.format_exc())
        except FileExistsError:
            pass
    finally:
        if fixture_context is not None:
            try:
                process = fixture["process"]
                intent = time.monotonic_ns()
                fixture_context.__exit__(None, None, None)
                fixture_context = None
                fixture_lifecycle.append({"operation": "measurement_retire",
                    "intentMonotonicNs": intent, "completedMonotonicNs": time.monotonic_ns(),
                    "process": next((row for row in reversed(fixture_roots)
                                     if row["pid"] == process.pid), None),
                    "exitStatus": process.poll()})
            except Exception as error:
                failure = failure or "owned Native fixture cleanup failed: " + type(error).__name__
                failure_reasons.append(failure)
        try:
            if sampler is not None:
                if consumer_exit is None and (case / "consumer-exit.json").is_file():
                    consumer_exit = _json(case / "consumer-exit.json")
                boundaries = _rows(case / "consumer-process-boundaries.jsonl") \
                    if (case / "consumer-process-boundaries.jsonl").is_file() else []
                if consumer_exit is not None:
                    retirement = tools.verify_owned_retirement(
                        sampler, consumer_exit, boundaries,
                        family["thresholds"]["ownedCleanupDeadlineMs"], fixture_lifecycle)
                    retirement["consumerRoot"] = {
                        "pid": consumer_exit.get("pid"),
                        "startIdentity": consumer_exit.get("startIdentity")}
                    _write_once(case / "owned-process-retirement.json", retirement)
        except Exception as error:
            failure = failure or "owned process retirement proof failed: " + type(error).__name__ + ": " + str(error)
            failure_reasons.append(failure)
            if not (case / "owned-process-retirement.json").exists():
                _write_once(case / "owned-process-retirement.json", {
                    "status": "failed", "error": type(error).__name__ + ": " + str(error)})
        try:
            if sampler is not None:
                measured_rss = sampler.stop()
                _write_once(case / "last-rss-summary.json", measured_rss)
        except Exception as error:
            failure = failure or "RSS sampler did not stop: " + type(error).__name__ + ": " + str(error)
            failure_reasons.append(failure)
        try:
            if server is not None:
                server.shutdown()
                server.server_close()
            if provider_thread is not None:
                provider_thread.join(timeout=10)
                if provider_thread.is_alive():
                    raise RuntimeError("loopback provider server thread did not retire")
            provider_observation = _provider_snapshot(provider_requests, provider_state)
            if not (case / "provider-observation.json").exists():
                _write_once(case / "provider-observation.json", provider_observation)
            if not (case / "provider-end-receipt.json").exists():
                _write_once(case / "provider-end-receipt.json", provider_observation)
        except Exception as error:
            failure = failure or "provider cleanup/observation failed: " + type(error).__name__ + ": " + str(error)
            failure_reasons.append(failure)
        try:
            machine_after = _machine_evidence(tools)
            if not (case / "machine-after.json").exists():
                _write_once(case / "machine-after.json", machine_after)
        except Exception as error:
            machine_after = {"error": type(error).__name__ + ": " + str(error)}
            failure = failure or "machine after identity failed: " + str(error)
            failure_reasons.append(failure)
        try:
            _validate_capture(registration_path, pilot=pilot)
            if file_sha256(registration_path) != registered_sha:
                raise ValueError("registration bytes changed during the case")
            identity_rechecked = True
        except Exception as error:
            identity_rechecked = False
            failure = failure or "registered identity changed after execution: " + type(error).__name__ + ": " + str(error)
            failure_reasons.append(failure)
        try:
            _write_once(case / "fixture-lifecycle.json", fixture_lifecycle)
        except Exception as error:
            failure = failure or "fixture lifecycle receipt failed: " + str(error)
            failure_reasons.append(failure)
        if cold_kernel_restart is not None and not (case / "cold-kernel-restart.json").exists():
            try:
                if isinstance(consumer_exit, dict):
                    cold_kernel_restart["consumerPid"] = consumer_exit.get("pid")
                    cold_kernel_restart["consumerStartIdentity"] = consumer_exit.get("startIdentity")
                _write_once(case / "cold-kernel-restart.json", cold_kernel_restart)
            except Exception:
                pass
        try:
            if not (case / "machine-after.json").exists():
                _write_once(case / "machine-after.json", {"error": "machine probe not completed"})
            if not (case / "provider-observation.json").exists():
                _write_once(case / "provider-observation.json",
                            _provider_snapshot(provider_requests, provider_state))
        except Exception:
            pass

    # Preserve the fixed file shape even when execution failed before a
    # particular phase began. These receipts say not-run; they never imply a
    # successful sample or substitute for missing measurements.
    if not (case / "inputs.json").exists():
        try:
            _write_once(case / "inputs.json", {
                **case_record, "machineBefore": machine_before or {"error": "machine probe did not complete"},
                "registrationSha256": registered_sha,
                "baseProfileSha256": file_sha256(pins["profilePath"]),
                "familyProfileSha256": file_sha256(family_pins["familyPath"]),
                "familyPath": reg["familyPath"],
                "programSha256": file_sha256(pins["program"]),
                "consumerSha256": file_sha256(pins["consumer"]),
                "observerSha256": reg.get("observerSha256", reg["consumerSha256"]),
                "runnerSha256": reg["runnerSha256"],
                "runtimeSha256": reg["runtimeSha256"],
                "runtimeFiles": reg["runtimeFiles"],
                "runtimeArguments": reg["runtimeArguments"],
                "sourceCommit": reg["sourceCommit"],
            })
        except Exception as error:
            failure = failure or "inputs evidence could not be written: " + str(error)
            failure_reasons.append(failure)
    fallback_json = {
        "seed-provider-before-measurement.json": {
            "status": "not_run", "reason": "seed provider phase did not complete"},
        "provider-observation.json": _provider_snapshot(provider_requests, provider_state),
        "provider-end-receipt.json": _provider_snapshot(provider_requests, provider_state),
        "consumer-exit.json": {"exitStatus": None, "error": "consumer was not started"},
        "last-rss-summary.json": {"status": "not_run", "reason": "measurement sampler did not start"},
        "owned-process-retirement.json": {"status": "not_run", "reason": "consumer cleanup was not observed"},
        "machine-after.json": {"error": "machine probe did not complete"},
        "fixture-lifecycle.json": fixture_lifecycle,
    }
    if cold_kernel_restart is None:
        fallback_json["cold-kernel-restart.json"] = {
            "status": "not_applicable" if reg["mode"] != "cold" else "not_run",
            "mode": reg["mode"]}
    for name, value in fallback_json.items():
        if not (case / name).exists():
            try:
                _write_once(case / name, value)
            except FileExistsError:
                pass
    for name in ("consumer.stdout.jsonl", "consumer-process-boundaries.jsonl",
                 "measurement-rss.jsonl"):
        path = case / name
        if not path.exists():
            with path.open("x", encoding="utf-8"):
                pass

    machine_after = _json(case / "machine-after.json") if (case / "machine-after.json").is_file() else None
    provider_observation = _json(case / "provider-observation.json") if (
        case / "provider-observation.json").is_file() else None
    runtime_after = None
    if runtime_before is not None:
        try:
            runtime_after = tools.runtime_identity(argparse.Namespace(
                endpoint="native", kernel_supervisor=run_args["kernel_supervisor"],
                kernel_worker=run_args["kernel_worker"],
                providers_config=run_args["providers_config"]))
            if runtime_after != runtime_before:
                raise ValueError("fixed Native runtime identity changed during the case")
        except Exception as error:
            failure = failure or "runtime identity recheck failed: " + type(error).__name__ + ": " + str(error)
            failure_reasons.append(failure)
    if identity_rechecked is not True:
        failure = failure or "registration identity recheck did not pass"
        failure_reasons.append(failure)
    receipt = {
        **case_record,
        "path": reg["familyPath"], "sourceCommit": reg["sourceCommit"],
        "registrationSha256": registered_sha,
        "baseProfileSha256": file_sha256(pins["profilePath"]),
        "familyProfileSha256": file_sha256(family_pins["familyPath"]),
        "profileSha256": file_sha256(family_pins["familyPath"]),
        "programSha256": file_sha256(pins["program"]),
        "consumerSha256": file_sha256(pins["consumer"]),
        "observerSha256": file_sha256(pins["consumer"]),
        "runnerSha256": reg["runnerSha256"],
        "runtimeSha256": reg["runtimeSha256"],
        "runtimeBefore": _runtime_evidence(runtime_before),
        "runtimeAfter": _runtime_evidence(runtime_after),
        "status": "success" if failure is None else "failed",
        "failureReason": failure,
        "failureReasons": failure_reasons,
        "identityRechecked": identity_rechecked,
        "official": not pilot, "diagnosticOnly": pilot, "purpose": case_record["purpose"],
        "performanceAccepted": False,
        "registration": {"path": str(registration_path), "sha256": registered_sha,
                         "profilePath": str(pins["profilePath"]),
                         "profileSha256": file_sha256(pins["profilePath"]),
                         "familyProfilePath": str(family_pins["familyPath"]),
                         "familyProfileSha256": file_sha256(family_pins["familyPath"]),
                         "bytes": pins["registrationBytes"].decode("utf-8")},
        "source": {"role": reg["role"], "commit": reg["sourceCommit"],
                   "familyPath": reg["familyPath"]},
        "program": {"path": str(pins["program"]), "sha256": file_sha256(pins["program"]),
                    "receiptSha256": file_sha256(Path(reg["programReceiptPath"])),
                    "sourceCommit": pins["programReceipt"].get("sourceCommit"),
                    "cargoFeatures": pins["programReceipt"].get("cargoFeatures"),
                    "consumerSourceSha256": file_sha256(pins["consumer"])},
        "seeder": {"path": str(pins["seeder"]), "sha256": file_sha256(pins["seeder"]),
                   "receiptSha256": file_sha256(Path(reg["seederReceiptPath"])),
                   "sourceCommit": pins["seederReceipt"].get("sourceCommit")},
        "runtime": {"version": "Kernel0.9.3/09508a9083c24c7accefc99516739f279b85913e",
                    "criticalFilesBefore": runtime_before,
                    "criticalFilesAfter": runtime_after},
        "machineBefore": machine_before, "machineAfter": machine_after,
        "seed": {"parentSessionId": parent_id, "childSessionId": child_id,
                 "sourceHistorySha256": file_sha256(case / "seed-source-history.json")
                    if (case / "seed-source-history.json").is_file() else None,
                 "proofSha256": file_sha256(case / "seed-proof.json")
                    if (case / "seed-proof.json").is_file() else None,
                 "seedCliExitSha256": file_sha256(case / "seed-exit.json")
                    if (case / "seed-exit.json").is_file() else None,
                 "closedSeedReceiptSha256": file_sha256(case / ".agentkit-performance-closed-seed.json")
                    if (case / ".agentkit-performance-closed-seed.json").is_file() else None},
        "seedExitStatus": (next((row.get("exitStatus") for row in fixture_lifecycle
                                  if row.get("operation") == "seed_retire"), None)),
        "seedReceiptSha256": (file_sha256(case / ".agentkit-performance-closed-seed.json")
                              if (case / ".agentkit-performance-closed-seed.json").is_file()
                              else None),
        "seedConsumerExit": (_json(case / "seed-exit.json")
                             if (case / "seed-exit.json").is_file() else None),
        "providerObservation": provider_observation,
        "consumerExit": consumer_exit,
        "consumerEventsSha256": file_sha256(case / "consumer.stdout.jsonl")
            if (case / "consumer.stdout.jsonl").is_file() else None,
        "consumerEventCount": len(consumer_events),
        "rssSummary": measured_rss,
        "fixtureLifecycle": fixture_lifecycle,
        "ownedProcessRetirement": retirement,
        "executionScope": "single-diagnostic-toolchain-pilot-no-retry" if pilot else "single-original-official-case-no-retry",
        "parentSessionId": parent_id,
        "childSessionId": child_id,
    }
    _write_once(case / "official-execution-receipt.json", receipt)
    # The isolated #56 reporter consumes this case's original evidence only.
    try:
        from performance_native_family_report import materialize_sample
        report_registration = dict(reg)
        report_registration.update({
            "path": reg["familyPath"], "official": not pilot, "diagnosticOnly": pilot,
            "purpose": case_record["purpose"],
            "registrationSha256": registered_sha,
            "baseProfileSha256": file_sha256(pins["profilePath"]),
            "familyProfileSha256": file_sha256(family_pins["familyPath"]),
            "programSha256": file_sha256(pins["program"]),
            "consumerSha256": file_sha256(pins["consumer"]),
            "observerSha256": file_sha256(pins["consumer"]),
            "runnerSha256": reg["runnerSha256"],
            "runtimeSha256": reg["runtimeSha256"],
            "cargoFeatures": pins["programReceipt"].get("cargoFeatures"),
            "parentSessionId": parent_id,
            "childSessionId": child_id,
        })
        sample = materialize_sample(case, report_registration, pins["profile"], family, pilot=pilot)
        sample["registrationOriginal"] = reg
        _write_once(case / "official-sample.json", sample)
    except ImportError:
        _write_once(case / "official-sample-materialization-error.json", {
            "status": "not_run", "reason": "performance_native_family_report is not available"})
    except Exception as error:
        _write_once(case / "official-sample-materialization-error.json", {
            "official": not pilot, "diagnosticOnly": pilot,
            "purpose": case_record["purpose"], "status": "failed",
            "sampleId": reg["sampleId"],
            "failureReason": type(error).__name__ + ": " + str(error),
            "traceback": traceback.format_exc()})
    return case / "official-execution-receipt.json"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registration", type=Path, required=True)
    parser.add_argument("--pilot", action="store_true",
                        help="run a preregistered toolchain pilot that can never enter the official matrix")
    args = parser.parse_args()
    try:
        result = execute(args.registration, pilot=args.pilot)
        receipt = _json(result)
        print(result)
        materialization_error = result.parent / "official-sample-materialization-error.json"
        sample_path = result.parent / "official-sample.json"
        sample_ok = sample_path.is_file() and _json(sample_path).get("status") == "success"
        return 0 if receipt.get("status") == "success" and sample_ok and not materialization_error.exists() else 1
    except Exception as error:
        print(f"NATIVE_FAMILY_CASE_PREFLIGHT_OR_CAPTURE_FAILED: {type(error).__name__}: {error}",
              file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
