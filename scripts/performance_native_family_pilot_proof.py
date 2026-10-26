"""Independent, read-only proof for the one failed Native Family toolchain pilot.

This does not repair the capture receipt or create an official sample.  It
derives a diagnostic proof only when the exact frozen registration, receipt,
process identities and RSS rows establish the two known recorder defects.
"""
from __future__ import annotations

import ast
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import Any


CAPTURE_ERROR = (
    "registered identity changed after execution: ValueError: diagnostic pilot "
    "registration requires --pilot and cannot enter the official matrix"
)


def _sha_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _read_json(path: Path) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"required regular evidence file missing: {path.name}")
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"evidence object expected: {path.name}")
    return value


def _read_json_value(path: Path) -> Any:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"required regular evidence file missing: {path.name}")
    return json.loads(path.read_text(encoding="utf-8"))


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"required regular evidence file missing: {path.name}")
    data = path.read_bytes()
    if data and not data.endswith(b"\n"):
        raise ValueError(f"truncated JSONL evidence: {path.name}")
    rows = [json.loads(line) for line in data.splitlines()]
    if any(not isinstance(row, dict) for row in rows):
        raise ValueError(f"JSONL evidence contains a non-object: {path.name}")
    return rows


def _hash_file(path: Path) -> str:
    return _sha_bytes(path.read_bytes())


def _verify_registrations(case: Path, registration_view: dict[str, Any],
                          inputs: dict[str, Any], receipt: dict[str, Any]
                          ) -> tuple[dict[str, Any], Path, str, str]:
    embedded = inputs.get("registrationBytes")
    receipt_registration = receipt.get("registration")
    if not isinstance(embedded, str) or not isinstance(receipt_registration, dict):
        raise ValueError("original registration bytes are not retained twice")
    receipt_bytes = receipt_registration.get("bytes")
    if not isinstance(receipt_bytes, str) or receipt_bytes != embedded:
        raise ValueError("input and execution receipt registration bytes differ")
    original_bytes = embedded.encode("utf-8")
    original_sha = _sha_bytes(original_bytes)
    if (inputs.get("registrationSha256") != original_sha
            or receipt.get("registrationSha256") != original_sha
            or receipt_registration.get("sha256") != original_sha):
        raise ValueError("exact original registration SHA does not match every receipt binding")
    try:
        original = json.loads(embedded)
    except ValueError as error:
        raise ValueError("original registration JSON is invalid") from error
    if (not isinstance(original, dict) or original.get("purpose") != "toolchain-pilot"
            or original.get("diagnosticOnly") is not True or original.get("official") is not True):
        raise ValueError("registration is not the preregistered diagnostic pilot")
    registration_path_value = receipt_registration.get("path")
    if not isinstance(registration_path_value, str):
        raise ValueError("receipt does not bind the registration path")
    registration_path = Path(registration_path_value)
    if registration_path.is_symlink() or not registration_path.is_file():
        raise ValueError("original registration path is unavailable or not regular")
    if registration_path.read_bytes() != original_bytes:
        raise ValueError("on-disk original registration differs from captured bytes")
    if registration_view.get("registrationBytes") not in (None, embedded):
        raise ValueError("report registration view carries different original bytes")
    # The reporter adds derived fields and toggles the diagnostic report view.
    # All preregistered fields must still equal their original values.
    derived_keys = {"path", "official", "diagnosticOnly", "registrationSha256",
                    "baseProfileSha256", "familyProfileSha256", "programSha256",
                    "consumerSha256", "observerSha256", "runtimeSha256", "runnerSha256"}
    for key, value in original.items():
        if key not in derived_keys and registration_view.get(key) != value:
            raise ValueError(f"report registration changed original field {key}")
    for key in ("sampleId", "caseName", "sourceCommit", "role", "mode", "caseIndex",
                "familyPath", "programSha256", "consumerSha256", "runtimeSha256"):
        if inputs.get(key, inputs.get("familyPath") if key == "path" else None) != original.get(key):
            # case identity fields are copied at top level; source pins are in the program receipt.
            if key in ("programSha256", "consumerSha256", "runtimeSha256"):
                continue
            raise ValueError(f"inputs sidecar differs from original registration field {key}")
    if receipt.get("sampleId") != original.get("sampleId") or receipt.get("caseName") != original.get("caseName"):
        raise ValueError("execution receipt belongs to a different registered case")
    program_receipt = receipt.get("program")
    if not isinstance(program_receipt, dict):
        raise ValueError("execution receipt lacks its original program receipt")
    program_path = Path(original.get("programPath", ""))
    program_sha = original.get("programSha256")
    if (not program_path.is_absolute() or program_path.is_symlink() or not program_path.is_file()
            or not isinstance(program_sha, str) or _hash_file(program_path) != program_sha
            or program_receipt.get("sha256") != program_sha
            or receipt.get("programSha256") != program_sha):
        raise ValueError("consumer executable no longer matches its exact program pin")
    return original, registration_path, original_sha, program_sha


def _capture_recheck(original: dict[str, Any], registration_path: Path,
                     original_sha: str) -> dict[str, Any]:
    runner_path = Path(original.get("caseRunnerPath", ""))
    runner_sha = original.get("caseRunnerSha256")
    if (not runner_path.is_absolute() or runner_path.is_symlink() or not runner_path.is_file()
            or _hash_file(runner_path) != runner_sha):
        raise ValueError("frozen case runner differs from its registered SHA")
    closure = original.get("runnerFiles")
    if not isinstance(closure, list) or not closure:
        raise ValueError("frozen runner dependency closure is absent")
    closure_rows = []
    for item in closure:
        if not isinstance(item, dict):
            raise ValueError("frozen runner closure row is malformed")
        path = Path(item.get("path", ""))
        expected = item.get("sha256")
        if path.is_symlink() or not path.is_file() or _hash_file(path) != expected:
            raise ValueError(f"frozen runner dependency changed: {path.name}")
        closure_rows.append({"path": str(path), "sha256": expected})
    canonical = json.dumps(closure_rows, sort_keys=True, separators=(",", ":")).encode()
    if hashlib.sha256(canonical).hexdigest() != original.get("runnerSha256"):
        raise ValueError("frozen runner closure digest differs from the registration")

    # Parse the unchanged runner to bind the known omission to its exact call site.
    source = runner_path.read_text(encoding="utf-8")
    tree = ast.parse(source, filename=str(runner_path))
    omitted_sites = []
    pilot_sites = []
    parents: dict[ast.AST, ast.AST] = {}
    for parent in ast.walk(tree):
        for child in ast.iter_child_nodes(parent):
            parents[child] = parent
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name):
            continue
        if node.func.id != "_validate_capture":
            continue
        if len(node.args) == 1 and not node.keywords:
            ancestor = parents.get(node)
            while ancestor is not None and not isinstance(ancestor, ast.Try):
                ancestor = parents.get(ancestor)
            if ancestor is not None:
                handlers = [ast.get_source_segment(source, handler)
                            for handler in ancestor.handlers]
                if any("identity_rechecked = False" in (text or "")
                       and "registered identity changed after execution" in (text or "")
                       for text in handlers):
                    omitted_sites.append(node.lineno)
        if any(keyword.arg == "pilot" and isinstance(keyword.value, ast.Name)
               and keyword.value.id == "pilot" for keyword in node.keywords):
            pilot_sites.append(node.lineno)
    if len(omitted_sites) != 1 or not pilot_sites:
        raise ValueError("frozen runner no longer has the preregistered pilot capture call omission")

    code = r'''import importlib.util,json,sys
from pathlib import Path
root=Path(sys.argv[1]); runner=Path(sys.argv[2]); reg=Path(sys.argv[3]); expected=sys.argv[4]
sys.path.insert(0,str(root))
spec=importlib.util.spec_from_file_location("frozen_native_family_runner",runner)
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
pins,family=module._validate_capture(reg,pilot=True)
print(json.dumps({"registrationSha256":module.file_sha256(reg),
 "programSha256":module.file_sha256(pins["program"]),
 "familyProfileSha256":module.file_sha256(family["familyPath"]),
 "sourceCommit":pins["registration"]["sourceCommit"],
 "captureValidation":"passed-read-only","purpose":"toolchain-pilot"}))
'''
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "PYTHONDONTWRITEBYTECODE": "1",
           "LANG": "en_US.UTF-8"}
    completed = subprocess.run(
        [sys.executable, "-c", code, str(runner_path.parent), str(runner_path),
         str(registration_path), original_sha], capture_output=True, text=True,
        env=env, timeout=30, check=False)
    if completed.returncode != 0:
        raise ValueError(f"frozen read-only capture validation failed: {completed.stderr[-2000:]}")
    try:
        result = json.loads(completed.stdout.splitlines()[-1])
    except (ValueError, IndexError) as error:
        raise ValueError("frozen capture validator returned no JSON receipt") from error
    if (result.get("registrationSha256") != original_sha
            or result.get("programSha256") != original.get("programSha256")
            or result.get("familyProfileSha256") != original.get("familyProfileSha256")
            or result.get("sourceCommit") != original.get("sourceCommit")
            or result.get("captureValidation") != "passed-read-only"):
        raise ValueError("frozen capture validator did not recheck the exact original pins")
    return {"runnerPath": str(runner_path), "runnerSha256": runner_sha,
            "runnerClosureSha256": original.get("runnerSha256"),
            "runnerClosureFiles": closure_rows,
            "captureValidation": result,
            "omittedPilotCaptureCallLine": omitted_sites[0],
            "validPilotCaptureCallLines": pilot_sites,
            "pilotCallSiteSourceSha256": _sha_bytes(source.encode("utf-8"))}


def _rss_proof(case: Path, registration: dict[str, Any], receipt: dict[str, Any],
               program_sha: str) -> dict[str, Any]:
    raw_path = case / "measurement-rss.jsonl"
    summary_path = case / "last-rss-summary.json"
    raw_rows = _read_jsonl(raw_path)
    summary = _read_json(summary_path)
    boundaries = _read_jsonl(case / "consumer-process-boundaries.jsonl")
    consumer_exit = _read_json(case / "consumer-exit.json")
    retirement = _read_json(case / "owned-process-retirement.json")
    fixture_rows = _read_json_value(case / "fixture-lifecycle.json")
    if receipt.get("rssSummary") != summary:
        raise ValueError("raw RSS summary differs from the original execution receipt")
    if receipt.get("consumerExit") != consumer_exit or receipt.get("ownedProcessRetirement") != retirement:
        raise ValueError("process exit/retirement evidence differs from original receipt")
    if receipt.get("fixtureLifecycle") != fixture_rows:
        raise ValueError("fixture lifecycle differs from original receipt")
    root = receipt.get("consumerExit")
    if not isinstance(root, dict):
        raise ValueError("consumer exit identity is absent")
    root_id = (root.get("pid"), root.get("startIdentity"))
    consumer_roots = summary.get("roots", {})
    agentkit_root = consumer_roots.get(str(root_id[0]), {}) if isinstance(consumer_roots, dict) else {}
    if (agentkit_root.get("role") != "agentkit"
            or agentkit_root.get("startIdentity") != root_id[1]
            or root.get("exitStatus") != 0 or root.get("ownedGroupAbsent") is not True
            or retirement.get("status") != "verified"
            or retirement.get("consumerRoot") != {"pid": root_id[0], "startIdentity": root_id[1]}):
        raise ValueError("explicit consumer root is not tied to a clean process retirement")
    known_rows = summary.get("knownProcesses")
    if not isinstance(known_rows, list):
        raise ValueError("final sampler process identity census is missing")
    known: dict[tuple[Any, Any], str] = {}
    for row in known_rows:
        key = (row.get("pid"), row.get("startIdentity"))
        if key in known and known[key] != row.get("role"):
            raise ValueError("final sampler identity has conflicting ownership roles")
        known[key] = row.get("role")
    if known.get(root_id) != "agentkit":
        raise ValueError("final sampler census does not identify the explicit consumer root")

    executable = Path(registration.get("programPath", ""))
    if not executable.is_absolute() or _hash_file(executable) != program_sha:
        raise ValueError("pinned consumer executable is unavailable during RSS identity check")
    start_bound = root.get("spawnMonotonicNsLowerBound")
    if type(start_bound) is not int:
        raise ValueError("consumer spawn lower bound is missing")
    first_boundary = next((row for row in boundaries if row.get("event") == "consumer_started"), None)
    if first_boundary is None or type(first_boundary.get("monotonicNs")) is not int:
        raise ValueError("first OS consumer boundary is missing")
    root_boundary = (root_id[0], root_id[1])
    boundary_seen = []
    for row in boundaries:
        if row.get("event") not in ("consumer_started", "iteration_started", "runtime_ready",
                                      "cleanup_intent", "runtime_cleanup_completed"):
            continue
        if type(row.get("monotonicNs")) is not int:
            raise ValueError("consumer process boundary lacks OS observation time")
        matches = [process for process in row.get("osProcesses", [])
                   if (process.get("pid"), process.get("startIdentity")) == root_boundary]
        if row["monotonicNs"] < root.get("spawnMonotonicNsLowerBound"):
            continue
        if row["event"] in ("consumer_started", "iteration_started", "runtime_ready") and len(matches) != 1:
            raise ValueError("active OS boundary does not retain the exact consumer PID/start identity")
        if matches and matches[0].get("role") != "agentkit":
            raise ValueError("consumer boundary assigns its explicit root to another role")
        if matches:
            boundary_seen.append(row["monotonicNs"])
    if not boundary_seen or len(set(boundary_seen)) < 3:
        raise ValueError("multiple OS boundaries do not independently retain the consumer identity")

    roots = summary.get("roots", {})
    provider_candidates = [(int(pid), item.get("startIdentity"))
                           for pid, item in roots.items()
                           if isinstance(item, dict) and item.get("role") == "provider"]
    provider_ids = {key for key, value in known.items() if value == "provider"}
    if len(provider_candidates) != 1:
        raise ValueError("exactly one registered provider root is required for the race proof")
    corrections = []
    observed_ids: dict[tuple[Any, Any], set[str]] = {}
    raw_peaks = {"agentkit": 0, "harness": 0, "provider": 0}
    corrected_peaks = {"agentkit": 0, "harness": 0, "provider": 0}
    clocks = []
    for index, sample in enumerate(raw_rows):
        stamp = sample.get("monotonicNs")
        if type(stamp) is not int:
            raise ValueError("raw RSS sample lacks monotonic timestamp")
        clocks.append(stamp)
        processes = sample.get("processes")
        if not isinstance(processes, list):
            raise ValueError("raw RSS sample lacks process identity rows")
        raw_totals = {role: 0 for role in raw_peaks}
        corrected_totals = {role: 0 for role in corrected_peaks}
        sample_corrections = []
        seen = set()
        for process in processes:
            pid, start = process.get("pid"), process.get("startIdentity")
            key = (pid, start)
            if key in seen:
                raise ValueError("raw RSS sample repeats a PID/start identity")
            seen.add(key)
            if key not in known:
                raise ValueError("raw RSS sample contains an identity absent from final census")
            if process.get("role") not in raw_peaks or known[key] not in raw_peaks:
                raise ValueError("raw/final process role is unknown")
            role = process["role"]
            expected = known[key]
            if role != expected:
                # One narrowly allowed race: the explicitly registered Rust
                # consumer root was first sampled before ProcessSampler.register.
                if key != root_id or role != "provider" or expected != "agentkit":
                    raise ValueError("unexplained RSS process role change")
                if not (start_bound < stamp < first_boundary["monotonicNs"]):
                    raise ValueError("consumer role correction falls outside its spawn/boundary window")
                if (process.get("executablePath") != str(executable)
                        or process.get("command") != str(executable)):
                    raise ValueError("early consumer root executable differs from the pinned program")
                parent = (process.get("ppid"), None)
                if parent[0] != provider_candidates[0][0]:
                    raise ValueError("early consumer root is not directly parented by its registered provider")
                parent_proc = next((item for item in processes if item.get("pid") == parent[0]), None)
                provider_identity = next((identity for identity in provider_ids if identity[0] == parent[0]), None)
                if (parent_proc is None or provider_identity is None
                        or parent_proc.get("startIdentity") != provider_identity[1]
                        or parent_proc.get("role") != "provider"):
                    raise ValueError("early consumer root parent lacks its exact registered provider identity")
                sample_corrections.append({"sampleIndex": index, "pid": pid,
                    "startIdentity": start, "fromRole": role, "toRole": expected,
                    "rssKiB": process.get("rssKiB")})
                observed_ids.setdefault(key, set()).add(role)
                observed_ids[key].add(expected)
            else:
                observed_ids.setdefault(key, set()).add(role)
            rss_value = process.get("rssKiB")
            if type(rss_value) is not int or rss_value < 0:
                raise ValueError("raw RSS resident size is invalid")
            raw_totals[role] += rss_value
            corrected_totals[expected] += rss_value
        if sample.get("treeRssKiB") != raw_totals:
            raise ValueError("raw process rows do not sum to the captured per-role RSS totals")
        for role in raw_peaks:
            raw_peaks[role] = max(raw_peaks[role], raw_totals[role])
            corrected_peaks[role] = max(corrected_peaks[role], corrected_totals[role])
        corrections.extend(sample_corrections)

    if len(clocks) != summary.get("sampleCount") or len(clocks) < 2 or clocks != sorted(clocks):
        raise ValueError("raw RSS sample count/order differs from the original sampler receipt")
    gaps = [right - left for left, right in zip(clocks, clocks[1:])]
    max_gap = max(gaps)
    if (summary.get("maxObservedIntervalNs") != max_gap or max_gap > 250_000_000
            or summary.get("requestedIntervalMs") != 50):
        raise ValueError("raw RSS cadence differs from the original frozen sampler contract")
    if not corrections:
        raise ValueError("no explicit provider-to-consumer root race was found")
    if any(key != root_id and len(roles) > 1 for key, roles in observed_ids.items()):
        raise ValueError("RSS role correction would affect a non-root process")
    if summary.get("sampledTreePeakRssKiB") != raw_peaks:
        raise ValueError("original RSS peak summary differs from the complete raw sample rows")
    conservative = {role: max(raw_peaks[role], corrected_peaks[role]) for role in raw_peaks}
    if retirement.get("allOwnedRootsExitedMonotonicNs") is None:
        raise ValueError("OS retirement proof lacks its all-roots-exited observation")
    return {"rawPeaksKiB": raw_peaks, "correctedPeaksKiB": corrected_peaks,
            "conservativePeaksKiB": conservative, "roleCorrections": corrections,
            "sampleCount": len(raw_rows), "maxSampleGapNs": max_gap,
            "consumerRoot": {"pid": root_id[0], "startIdentity": root_id[1],
                "executablePath": str(executable), "programSha256": program_sha,
                "spawnMonotonicNsLowerBound": start_bound,
                "firstConsumerBoundaryMonotonicNs": first_boundary["monotonicNs"],
                "observedBoundariesMonotonicNs": boundary_seen,
                "exitStatus": root.get("exitStatus"),
                "ownedGroupAbsent": root.get("ownedGroupAbsent")}}


def build_pilot_proof(case: Path, registration: dict[str, Any]) -> dict[str, Any]:
    """Derive a diagnostic correction proof without changing original evidence."""
    case = Path(case).resolve(strict=True)
    receipt_path = case / "official-execution-receipt.json"
    inputs_path = case / "inputs.json"
    receipt_before = _hash_file(receipt_path)
    inputs_before = _hash_file(inputs_path)
    receipt = _read_json(receipt_path)
    inputs = _read_json(inputs_path)
    original, registration_path, original_sha, program_sha = _verify_registrations(
        case, registration, inputs, receipt)
    expected_failure = CAPTURE_ERROR
    failures = receipt.get("failureReasons")
    if (receipt.get("purpose") != "toolchain-pilot" or receipt.get("official") is not False
            or receipt.get("diagnosticOnly") is not True or receipt.get("status") != "failed"
            or receipt.get("identityRechecked") is not False
            or receipt.get("failureReason") != expected_failure
            or not isinstance(failures, list) or not failures
            or any(value != expected_failure for value in failures)):
        raise ValueError("original pilot receipt is not the sole known capture-call failure")
    capture = _capture_recheck(original, registration_path, original_sha)
    rss = _rss_proof(case, original, receipt, program_sha)
    evidence_names = (
        "official-execution-receipt.json", "inputs.json", "measurement-rss.jsonl",
        "last-rss-summary.json", "consumer-process-boundaries.jsonl", "consumer-exit.json",
        "owned-process-retirement.json", "fixture-lifecycle.json", "consumer.stdout.jsonl",
    )
    raw_hashes = {name: _hash_file(case / name) for name in evidence_names}
    if _hash_file(receipt_path) != receipt_before or _hash_file(inputs_path) != inputs_before:
        raise ValueError("proof generation changed original receipt/input bytes")
    return {"kind": "native-family-diagnostic-pilot-replay-proof", "format": 1,
            "official": False, "diagnosticOnly": True, "performanceAccepted": False,
            "receiptPath": str(receipt_path), "receiptSha256": receipt_before,
            "inputsPath": str(inputs_path), "inputsSha256": inputs_before,
            "registrationPath": str(registration_path), "registrationSha256": original_sha,
            "programSha256": program_sha, "rawEvidenceSha256": raw_hashes,
            "frozenCaptureRecheck": capture,
            "identityRecheckedDerived": True,
            "originalIdentityRechecked": False,
            "originalFailureReason": expected_failure,
            "rss": rss, "generatedAtUnixNs": time.time_ns()}
