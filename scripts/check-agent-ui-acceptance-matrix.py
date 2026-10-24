#!/usr/bin/env python3
"""Validate the AgentKit UI acceptance matrix and, optionally, its local evidence.

The default mode is CI-safe: it validates the checked-in matrix structure and
identity declarations without requiring ignored .runtime-fixtures files.
--verify-local additionally hashes every referenced report, program and raw
artifact, and checks source-file hashes against the declared Git revision.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MATRIX = ROOT / "docs/rust-agent-ui-acceptance-matrix-20261024.json"
STATUSES = {"unsupported", "blocked", "failed", "not_run", "passed"}
TOPOLOGIES = {"in_process", "cross_process"}
REQUIRED_LAYERS = {
    "component_tests",
    "public_consumer",
    "offscreen_harness",
    "strict_ci",
    "native_window",
    "system_ime",
    "accessibility",
    "process_container",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")
REQUIRED_REQUIREMENTS = {
    "composer_ime", "draft_isolation", "send_cancel", "stream",
    "reading_append", "pagination_anchor", "reconnect_no_duplicate",
    "views_layout", "sidebar_operations", "task", "subagent", "sidechat",
    "summary", "system_ime", "accessibility",
}
REQUIRED_OBSERVED_GATE_KINDS = {"strict_reading_append", "dsh_sidechat_capability"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_relative(value: object) -> Path:
    require(isinstance(value, str) and bool(value), f"invalid repository path: {value!r}")
    path = Path(value)
    require(not path.is_absolute() and ".." not in path.parts and "\\" not in value,
            f"unsafe repository path: {value!r}")
    return path


def walk(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def report_programs(report):
    for obj in walk(report):
        path = obj.get("path") or obj.get("program")
        sha = obj.get("sha256")
        if isinstance(path, str) and isinstance(sha, str):
            yield path, sha


def report_commits(report):
    names = {"basecommit", "sourcebase", "sourcecommit", "rusthead", "commit"}
    for obj in walk(report):
        for key, value in obj.items():
            if key.lower().replace("_", "") in names and isinstance(value, str):
                yield value


def report_source_hashes(report):
    names = {"sourcesha256", "source_hashes", "sourcefiles", "files"}
    for obj in walk(report):
        for key, value in obj.items():
            normalized = key.lower().replace("_", "")
            if normalized in {n.replace("_", "") for n in names} and isinstance(value, dict):
                yield value


def contains_native_window_receipt(value) -> bool:
    if isinstance(value, dict):
        if value.get("nativeWindow") is True:
            return True
        return any(contains_native_window_receipt(child) for child in value.values())
    if isinstance(value, list):
        return any(contains_native_window_receipt(child) for child in value)
    return False


def report_has_native_window_scope(report, scope_names) -> bool:
    return any(scope in report and contains_native_window_receipt(report[scope])
               for scope in scope_names)


def load_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read JSON {path}: {exc}") from exc


def verify_report_identity(evidence) -> None:
    """Verify committed report bytes and their declared program/source identity.

    This deliberately does not require ignored binaries or raw logs. It makes
    the checked-in receipt the CI trust boundary while --verify-local can also
    re-hash the referenced local artifacts.
    """
    report = evidence["report"]
    if "path" not in report:
        return  # External CI record: the immutable run/job URL is its identity.
    report_path = ROOT / safe_relative(report["path"])
    require(report_path.is_file(), f"missing evidence report: {report['path']}")
    require(digest(report_path) == report["sha256"],
            f"evidence report hash mismatch: {report['path']}")
    report_data = load_json(report_path)
    program = evidence.get("program")
    if program is not None:
        pair = (program["reportedPath"], program["sha256"])
        require(pair in set(report_programs(report_data)),
                f"program/source report identity mismatch: {evidence['id']}")
        require(evidence["sourceCommit"] in set(report_commits(report_data)),
                f"program source revision mismatch: {evidence['id']}")


def verify_local_evidence(evidence, verify_sources: bool) -> None:
    verify_report_identity(evidence)
    report = evidence["report"]
    if "path" not in report:
        return
    program = evidence.get("program")
    if program is not None:
        local_program = ROOT / safe_relative(program["path"])
        require(local_program.is_file(), f"missing evidence program: {program['path']}")
        require(digest(local_program) == program["sha256"],
                f"program hash mismatch: {program['path']}")

    for artifact in evidence.get("artifacts", []):
        path = ROOT / safe_relative(artifact["path"])
        require(path.is_file(), f"missing raw artifact: {artifact['path']}")
        require(digest(path) == artifact["sha256"],
                f"raw artifact hash mismatch: {artifact['path']}")

    if verify_sources:
        revision = evidence["sourceCommit"]
        exists = subprocess.run(["git", "cat-file", "-e", f"{revision}^{{commit}}"],
                                cwd=ROOT, capture_output=True)
        require(exists.returncode == 0, f"source revision is unavailable: {revision}")
        for name, expected in evidence.get("sourceFiles", {}).items():
            path = safe_relative(name)
            result = subprocess.run(["git", "show", f"{revision}:{path.as_posix()}"],
                                    cwd=ROOT, capture_output=True)
            require(result.returncode == 0,
                    f"source file missing at {revision}: {name}")
            actual = hashlib.sha256(result.stdout).hexdigest()
            require(actual == expected, f"source identity mismatch: {revision}:{name}")


def validate(matrix, verify_local=False) -> dict:
    require(isinstance(matrix, dict) and matrix.get("schemaVersion") == 1,
            "unsupported matrix schema")
    dimensions = matrix.get("dimensions")
    require(isinstance(dimensions, dict), "missing dimensions")
    requirements = dimensions.get("requirements")
    harnesses = dimensions.get("harnesses")
    topologies = dimensions.get("topologies")
    layers = dimensions.get("verificationLayers")
    for name, values in (("requirements", requirements), ("harnesses", harnesses),
                         ("topologies", topologies), ("verificationLayers", layers)):
        require(isinstance(values, list) and values, f"missing/empty {name}")
        require(len(values) == len(set(values if name != "requirements" else
                                       [item.get("id") for item in values])),
                f"duplicate {name}")
    req_ids = [r.get("id") for r in requirements]
    require(all(isinstance(value, str) and value for value in req_ids),
            "invalid requirement id")
    require(set(topologies) == TOPOLOGIES, "unknown or missing process topology")
    require(set(layers) == REQUIRED_LAYERS, "unknown or missing verification layer")
    require(set(harnesses) == {"Native", "Pi", "DSH"}, "unknown or missing harness")
    require(set(req_ids) == REQUIRED_REQUIREMENTS,
            "unknown or missing required acceptance requirement id")

    catalog = matrix.get("evidenceCatalog")
    require(isinstance(catalog, list), "missing evidence catalog")
    evidence_by_id = {}
    for item in catalog:
        require(isinstance(item, dict), "invalid evidence entry")
        evidence_id = item.get("id")
        require(isinstance(evidence_id, str) and evidence_id and evidence_id not in evidence_by_id,
                f"duplicate/invalid evidence id: {evidence_id!r}")
        source_commit = item.get("sourceCommit")
        require(isinstance(source_commit, str) and HEX40.fullmatch(source_commit),
                f"invalid source commit in {evidence_id}")
        report = item.get("report")
        require(isinstance(report, dict), f"invalid report identity in {evidence_id}")
        if "path" in report:
            require(HEX64.fullmatch(str(report.get("sha256", ""))),
                    f"invalid report SHA256 in {evidence_id}")
            safe_relative(report.get("path"))
        else:
            require(isinstance(report.get("url"), str)
                    and report["url"].startswith("https://github.com/"),
                    f"external evidence requires a GitHub run/job URL: {evidence_id}")
        require(isinstance(item.get("requirements"), list)
                and set(item["requirements"]).issubset(set(req_ids)),
                f"unknown/missing evidence requirement scope: {evidence_id}")
        require(isinstance(item.get("layers"), list)
                and set(item["layers"]).issubset(REQUIRED_LAYERS),
                f"unknown/missing evidence layer scope: {evidence_id}")
        if item.get("program") is not None:
            program = item["program"]
            require(isinstance(program, dict) and HEX64.fullmatch(str(program.get("sha256", ""))),
                    f"invalid program identity in {evidence_id}")
            safe_relative(program.get("path"))
            require(isinstance(program.get("reportedPath"), str),
                    f"missing report program path in {evidence_id}")
            require(item.get("contract") in {"ContentHost", "ProcessContainer", "none"},
                    f"unknown process contract in {evidence_id}")
        if "path" in report:
            # The checked-in report is small and tracked, so default CI mode
            # always verifies its content address and identity linkage.
            verify_report_identity(item)
        for artifact in item.get("artifacts", []):
            require(isinstance(artifact, dict)
                    and HEX64.fullmatch(str(artifact.get("sha256", ""))),
                    f"invalid raw artifact identity in {evidence_id}")
            safe_relative(artifact.get("path"))
        for name, sha in item.get("sourceFiles", {}).items():
            safe_relative(name)
            require(isinstance(sha, str) and HEX64.fullmatch(sha),
                    f"invalid source file hash in {evidence_id}: {name}")
        evidence_by_id[evidence_id] = item

    expected_keys = set(itertools.product(req_ids, harnesses, topologies, layers))
    rules = matrix.get("rules")
    require(isinstance(rules, list) and rules, "missing matrix rules")
    checked_rules = []
    for index, rule in enumerate(rules):
        require(isinstance(rule, dict), f"invalid matrix rule {index}")
        pattern = tuple(rule.get(name, "*") for name in
                        ("requirement", "harness", "topology", "layer"))
        allowed = (set(req_ids) | {"*"}, set(harnesses) | {"*"},
                   set(topologies) | {"*"}, set(layers) | {"*"})
        for dimension, value, values in zip(
                ("requirement", "harness", "topology", "layer"), pattern, allowed):
            require(value in values, f"unknown {dimension} in rule {index}: {value}")
        status = rule.get("status")
        require(status in STATUSES, f"unknown status {status!r} in rule {index}")
        require(isinstance(rule.get("reason"), str) and rule["reason"].strip(),
                f"missing rule reason {index}")
        checked_rules.append((pattern, rule.get("priority", 0), rule, index))

    actual = {}
    resolved_cells = []
    for key in sorted(expected_keys):
        matches = [(sum(value != "*" for value in pattern), priority, rule, index)
                   for pattern, priority, rule, index in checked_rules
                   if all(wanted == "*" or wanted == actual_value
                          for wanted, actual_value in zip(pattern, key))]
        require(matches, f"matrix rule coverage omitted cell: {key}")
        best_specificity = max(item[0] for item in matches)
        matches = [item for item in matches if item[0] == best_specificity]
        best_priority = max(item[1] for item in matches)
        winners = [item for item in matches if item[1] == best_priority]
        require(len(winners) == 1, f"ambiguous matrix rules for cell: {key}")
        _, _, rule, _ = winners[0]
        requirement, harness, topology, layer = key
        cell = {**rule, "requirement": requirement, "harness": harness,
                "topology": topology, "layer": layer}
        status = cell.get("status")
        explanation = cell.get("reason")
        evidence_ids = cell.get("evidence", [])
        history_ids = cell.get("historicalEvidence", [])
        require(isinstance(explanation, str) and explanation.strip(),
                f"missing status explanation: {key}")
        require(isinstance(evidence_ids, list) and isinstance(history_ids, list),
                f"invalid evidence references in {key}")
        require(isinstance(evidence_ids, list) and isinstance(history_ids, list),
                f"invalid evidence references in {key}")
        for evidence_id in evidence_ids + history_ids:
            require(evidence_id in evidence_by_id,
                    f"unknown evidence {evidence_id!r} in {key}")
        if status in {"passed", "failed"}:
            require(evidence_ids, f"{status} without current evidence: {key}")
        if status in {"blocked", "unsupported", "not_run"}:
            require(explanation.strip(), f"missing status reason: {key}")

        if status == "passed":
            for evidence_id in evidence_ids:
                item = evidence_by_id[evidence_id]
                require(not item.get("historicalOnly", False),
                        f"historical evidence cannot satisfy current passed cell: {key}")
                require(cell["layer"] in item.get("layers", []),
                        f"evidence layer mismatch in {key}: {evidence_id}")
                require(cell["requirement"] in item.get("requirements", []),
                        f"evidence requirement mismatch in {key}: {evidence_id}")
                contract = item.get("contract", "none")
                if cell["layer"] == "process_container":
                    require(contract == "ProcessContainer",
                            f"ContentHost cannot pass formal ProcessContainer acceptance: {key}")
                    require(item.get("topology") == cell["topology"],
                            f"ProcessContainer topology mismatch in {key}")
                if cell["topology"] == "cross_process":
                    require(contract == "ProcessContainer",
                            f"non-ProcessContainer evidence cannot pass cross-process: {key}")
                if cell["layer"] == "system_ime":
                    ime = item.get("ime", {})
                    require(item.get("nativeWindow") is True
                            and ime.get("route") == "macos_native_input_method"
                            and ime.get("markedTextObserved") is True
                            and ime.get("commitObserved") is True,
                            f"system IME pass lacks macOS input-method route/marked-text/commit evidence: {key}")
                if cell["layer"] == "accessibility":
                    require(item.get("voiceOverAccepted") is True,
                            f"accessibility pass lacks VoiceOver receipt: {key}")

        if verify_local:
            for evidence_id in set(evidence_ids + history_ids):
                verify_local_evidence(evidence_by_id[evidence_id], verify_sources=True)
        actual[key] = status
        resolved_cells.append(cell)

    expected_count = len(expected_keys)
    require(matrix.get("cellCount") == expected_count,
            f"cellCount mismatch: expected {expected_count}, got {matrix.get('cellCount')}")

    gates = matrix.get("observedGates")
    require(isinstance(gates, list), "missing observed gate evidence records")
    gates_by_kind = {}
    for gate in gates:
        require(isinstance(gate, dict), "invalid observed gate record")
        kind = gate.get("kind")
        require(isinstance(kind, str) and kind and kind not in gates_by_kind,
                f"duplicate/invalid observed gate kind: {kind!r}")
        gates_by_kind[kind] = gate
    require(REQUIRED_OBSERVED_GATE_KINDS.issubset(gates_by_kind),
            "missing required observed gate record")
    for kind, gate in gates_by_kind.items():
        match = gate.get("match")
        require(isinstance(match, dict), f"missing observed gate dimensions: {kind}")
        requirement = match.get("requirement")
        topology = match.get("topology")
        layer = match.get("layer")
        harnesses_for_gate = match.get("harnesses")
        require(requirement in req_ids and topology in topologies and layer in layers,
                f"unknown observed gate dimensions: {kind}")
        require(isinstance(harnesses_for_gate, list) and harnesses_for_gate
                and set(harnesses_for_gate).issubset(set(harnesses)),
                f"unknown observed gate harnesses: {kind}")
        status = gate.get("status")
        require(status in STATUSES, f"unknown observed gate status: {kind}")
        evidence_ids = gate.get("evidence")
        require(isinstance(evidence_ids, list) and evidence_ids,
                f"observed gate requires evidence: {kind}")
        for evidence_id in evidence_ids:
            require(evidence_id in evidence_by_id,
                    f"unknown observed gate evidence {evidence_id!r}: {kind}")
            item = evidence_by_id[evidence_id]
            require(requirement in item.get("requirements", [])
                    and layer in item.get("layers", []),
                    f"observed gate evidence scope mismatch: {kind}/{evidence_id}")
            if status == "passed":
                require(not item.get("historicalOnly", False),
                        f"historical evidence cannot satisfy a current passed gate: {kind}")
        matched = [cell for cell in resolved_cells
                   if cell["requirement"] == requirement
                   and cell["topology"] == topology and cell["layer"] == layer
                   and cell["harness"] in harnesses_for_gate]
        require(len(matched) == len(harnesses_for_gate),
                f"observed gate does not resolve every declared cell: {kind}")
        for cell in matched:
            require(cell["status"] == status,
                    f"matrix status disagrees with observed gate evidence: {kind}/{cell['harness']}")
            cited = set(cell.get("evidence", [])) | set(cell.get("historicalEvidence", []))
            require(set(evidence_ids).issubset(cited),
                    f"matrix cell does not cite observed gate evidence: {kind}/{cell['harness']}")

    observations = matrix.get("historicalObservations")
    require(isinstance(observations, list), "missing historical observations list")
    seen_observations = set()
    for observation in observations:
        require(isinstance(observation, dict), "invalid historical observation")
        key = tuple(observation.get(name) for name in
                    ("requirement", "harness", "topology", "layer"))
        require(key in expected_keys, f"unknown historical observation dimensions: {key}")
        require(key not in seen_observations, f"duplicate historical observation: {key}")
        seen_observations.add(key)
        evidence_id = observation.get("evidence")
        require(evidence_id in evidence_by_id
                and evidence_by_id[evidence_id].get("historicalOnly") is True,
                f"historical observation must reference frozen historical evidence: {key}")
        require(observation.get("assessment") in
                {"partial", "passed", "failed", "unsupported"},
                f"invalid historical assessment: {key}")
        item = evidence_by_id[evidence_id]
        require(key[0] in item.get("requirements", []) and key[3] in item.get("layers", []),
                f"historical evidence scope mismatch: {key}")
        if observation["assessment"] == "passed":
            require(key[2] == "in_process" and item.get("contract") == "ContentHost",
                    f"historical ContentHost result cannot pass cross-process/ProcessContainer: {key}")
            if key[3] == "native_window":
                require(item.get("nativeWindow") is True,
                        f"historical native-window pass lacks nativeWindow=true evidence: {key}")
                report_ref = item.get("report", {})
                require(isinstance(report_ref, dict) and "path" in report_ref,
                        f"historical native-window pass requires a local native receipt report: {key}")
                report_data = load_json(ROOT / safe_relative(report_ref["path"]))
                scopes = item.get("nativeWindowReportScopes", [])
                require(isinstance(scopes, list) and scopes
                        and report_has_native_window_scope(report_data, scopes),
                        f"historical native-window pass report has no native receipt scope: {key}")
        if key[3] == "system_ime" and observation["assessment"] == "passed":
            ime = item.get("ime", {})
            require(item.get("nativeWindow") is True
                    and ime.get("route") == "macos_native_input_method"
                    and ime.get("markedTextObserved") is True
                    and ime.get("commitObserved") is True,
                    f"historical system IME pass lacks native input-method evidence: {key}")
        if verify_local:
            verify_local_evidence(item, verify_sources=True)

    if verify_local:
        for evidence in evidence_by_id.values():
            verify_local_evidence(evidence, verify_sources=True)

    totals = {status: sum(value == status for value in actual.values()) for status in sorted(STATUSES)}
    return {"cells": expected_count, "statusCounts": totals,
            "evidenceEntries": len(evidence_by_id), "historicalObservations": len(observations),
            "localEvidenceVerified": verify_local}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path, default=DEFAULT_MATRIX)
    parser.add_argument("--verify-local", action="store_true",
                        help="hash local reports/programs/artifacts and verify Git source identities")
    args = parser.parse_args()
    try:
        result = validate(load_json(args.matrix), verify_local=args.verify_local)
    except (ValueError, OSError) as exc:
        print(f"acceptance matrix: FAILED: {exc}", file=sys.stderr)
        return 1
    counts = ", ".join(f"{key}={value}" for key, value in result["statusCounts"].items())
    print(f"acceptance matrix: OK: {result['cells']} cells, {counts}; "
          f"{result['evidenceEntries']} evidence entries, "
          f"{result['historicalObservations']} historical observations; "
          f"local evidence {'verified' if args.verify_local else 'not required'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
