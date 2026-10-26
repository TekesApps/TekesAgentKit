import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKER_PATH = ROOT / "scripts/check-agent-ui-acceptance-matrix.py"
MATRIX_PATH = ROOT / "docs/rust-agent-ui-acceptance-matrix-20261024.json"
CANDIDATE_MATRIX_PATH = ROOT / "docs/rust-agent-ui-acceptance-matrix-20261025.json"
SPEC = importlib.util.spec_from_file_location("acceptance_matrix_checker", CHECKER_PATH)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


def read_matrix():
    return json.loads(MATRIX_PATH.read_text(encoding="utf-8"))


def read_candidate_matrix():
    return json.loads(CANDIDATE_MATRIX_PATH.read_text(encoding="utf-8"))


def resolve_cell_status(matrix, requirement, harness, topology, layer):
    """Resolve one declared cell with the checker's specificity/priority rules."""
    key = (requirement, harness, topology, layer)
    matches = []
    for index, rule in enumerate(matrix["rules"]):
        pattern = tuple(rule.get(name, "*") for name in
                        ("requirement", "harness", "topology", "layer"))
        if all(wanted == "*" or wanted == actual
               for wanted, actual in zip(pattern, key)):
            matches.append((sum(value != "*" for value in pattern),
                            rule.get("priority", 0), index, rule))
    best = max((specificity, priority) for specificity, priority, _, _ in matches)
    winners = [rule for specificity, priority, _, rule in matches
               if (specificity, priority) == best]
    if len(winners) != 1:
        raise AssertionError(f"candidate does not resolve {key} uniquely")
    return winners[0]["status"]


def read_v2_fixture():
    matrix = read_matrix()
    matrix["schemaVersion"] = 2
    matrix["dimensions"]["requirements"].extend([
        {"id": "transport_recovery", "label": "transport recovery"},
        {"id": "image_payload_roundtrip", "label": "image payload roundtrip"},
        {"id": "mixed_attachment_order", "label": "mixed attachment order"},
        {"id": "busy_queue_submission_identity", "label": "busy queue identity"},
    ])
    matrix["dimensions"]["verificationLayers"].append("protocol_integration")
    all_requirements = [item["id"] for item in matrix["dimensions"]["requirements"]]
    for evidence in matrix["evidenceCatalog"]:
        evidence["harnesses"] = ["Native", "Pi", "DSH"]
        if evidence["id"] in {"ci-current-ui-tests", "tekesui-48-process-container-gate"}:
            evidence["requirements"] = all_requirements
        if evidence.get("historicalOnly"):
            evidence["harnesses"] = ["Native", "Pi", "DSH"]
    matrix["cellCount"] = 19 * 3 * 2 * 9
    return matrix


def add_rule(matrix, requirement, harness, topology, layer, evidence_id):
    matrix["rules"].append({
        "requirement": requirement, "harness": harness,
        "topology": topology, "layer": layer, "priority": 1000,
        "status": "passed", "reason": "negative fixture rule",
        "evidence": [evidence_id],
    })


class AcceptanceMatrixTests(unittest.TestCase):
    def test_real_schema2_candidate_has_full_scoped_grid(self):
        matrix = read_candidate_matrix()
        result = CHECKER.validate(matrix)
        self.assertEqual(matrix["schemaVersion"], 2)
        self.assertEqual(len(matrix["dimensions"]["requirements"]), 19)
        self.assertEqual(set(matrix["dimensions"]["harnesses"]),
                         {"Native", "Pi", "DSH"})
        self.assertEqual(set(matrix["dimensions"]["topologies"]),
                         {"in_process", "cross_process"})
        self.assertEqual(len(matrix["dimensions"]["verificationLayers"]), 9)
        self.assertEqual(result["cells"], 19 * 3 * 2 * 9)
        self.assertEqual(matrix["cellCount"], 1026)

    def test_real_schema2_candidate_keeps_native_busy_failure_historical(self):
        matrix = read_candidate_matrix()
        CHECKER.validate(matrix)
        historical = [item for item in matrix["historicalObservations"]
                      if item.get("requirement") == "busy_queue_submission_identity"
                      and item.get("harness") == "Native"
                      and item.get("topology") == "in_process"
                      and item.get("layer") == "protocol_integration"]
        self.assertEqual(len(historical), 1)
        self.assertEqual(historical[0]["assessment"], "failed")
        evidence = next(item for item in matrix["evidenceCatalog"]
                        if item["id"] == historical[0]["evidence"])
        self.assertTrue(evidence["historicalOnly"])
        self.assertEqual(resolve_cell_status(
            matrix, "busy_queue_submission_identity", "Native", "in_process",
            "protocol_integration"), "not_run")

    def test_real_manifest_expands_full_required_grid(self):
        result = CHECKER.validate(read_matrix())
        self.assertEqual(result["cells"], 15 * 3 * 2 * 8)
        self.assertEqual(result["statusCounts"]["failed"], 3)
        self.assertEqual(result["statusCounts"]["passed"], 36)
        self.assertEqual(result["statusCounts"]["not_run"], 275)
        self.assertFalse(result["localEvidenceVerified"])

    def test_public_consumer_behavior_cells_remain_unproven(self):
        matrix = read_matrix()
        result = CHECKER.validate(matrix)
        self.assertEqual(result["statusCounts"]["passed"], 36)
        rule = next(rule for rule in matrix["rules"]
                    if rule.get("layer") == "public_consumer"
                    and rule.get("topology") == "in_process"
                    and rule.get("priority") == 40)
        self.assertEqual(rule["status"], "not_run")

    def test_each_required_requirement_must_remain_present(self):
        matrix = read_matrix()
        matrix["dimensions"]["requirements"] = [
            item for item in matrix["dimensions"]["requirements"]
            if item["id"] != "accessibility"
        ]
        with self.assertRaisesRegex(ValueError, "required acceptance requirement"):
            CHECKER.validate(matrix)

    def test_unknown_dimension_is_rejected(self):
        matrix = read_matrix()
        matrix["dimensions"]["harnesses"].append("Other")
        with self.assertRaisesRegex(ValueError, "unknown or missing harness"):
            CHECKER.validate(matrix)

    def test_missing_rule_coverage_is_rejected(self):
        matrix = read_matrix()
        matrix["rules"] = [rule for rule in matrix["rules"]
                           if not (rule.get("priority") == 0 or
                                   (rule.get("topology") == "in_process" and
                                    rule.get("layer") == "component_tests" and
                                    rule.get("priority") == 30))]
        with self.assertRaisesRegex(ValueError, "coverage omitted cell"):
            CHECKER.validate(matrix)

    def test_unknown_rule_dimension_is_rejected(self):
        matrix = read_matrix()
        matrix["rules"].append({
            "requirement": "not-a-requirement", "status": "not_run",
            "reason": "negative test",
        })
        with self.assertRaisesRegex(ValueError, "unknown requirement"):
            CHECKER.validate(matrix)

    def test_passed_without_current_evidence_is_rejected(self):
        matrix = read_matrix()
        matrix["rules"].append({
            "requirement": "task", "harness": "Native",
            "topology": "in_process", "layer": "native_window",
            "priority": 100, "status": "passed", "reason": "fabricated pass",
        })
        with self.assertRaisesRegex(ValueError, "passed without current evidence"):
            CHECKER.validate(matrix)

    def test_synthetic_ime_cannot_pass_system_ime_cell(self):
        matrix = read_matrix()
        matrix["evidenceCatalog"].append({
            "id": "synthetic-ime-negative", "sourceCommit": matrix["baseline"]["agentkitCommit"],
            "report": {"url": "https://github.com/TekesApps/TekesAgentKit/issues/27"},
            "contract": "none", "topology": "in_process", "layers": ["system_ime"],
            "requirements": ["system_ime"], "nativeWindow": True,
            "ime": {"route": "synthetic_preedit", "markedTextObserved": True,
                    "commitObserved": True, "candidateWindowObserved": True,
                    "physicalKeyboard": False},
            "voiceOverAccepted": False,
        })
        matrix["rules"].append({
            "requirement": "system_ime", "harness": "Native",
            "topology": "in_process", "layer": "system_ime", "priority": 100,
            "status": "passed", "reason": "negative test",
            "evidence": ["synthetic-ime-negative"],
        })
        with self.assertRaisesRegex(ValueError, "system IME pass lacks macOS input-method route"):
            CHECKER.validate(matrix)

    def test_real_macos_ime_route_can_pass_without_physical_keyboard(self):
        matrix = read_matrix()
        matrix["evidenceCatalog"].append({
            "id": "macos-ime-route-positive",
            "sourceCommit": matrix["baseline"]["agentkitCommit"],
            "report": {"url": "https://github.com/TekesApps/TekesAgentKit/issues/27"},
            "contract": "none", "topology": "in_process", "layers": ["system_ime"],
            "requirements": ["system_ime"], "nativeWindow": True,
            "ime": {"route": "macos_native_input_method", "markedTextObserved": True,
                    "commitObserved": True, "candidateWindowObserved": True,
                    "physicalKeyboard": False},
        })
        matrix["rules"].append({
            "requirement": "system_ime", "harness": "Native",
            "topology": "in_process", "layer": "system_ime", "priority": 100,
            "status": "passed", "reason": "native macOS input-method route receipt",
            "evidence": ["macos-ime-route-positive"],
        })
        result = CHECKER.validate(matrix)
        self.assertGreater(result["statusCounts"]["passed"], 36)

    def test_observed_gate_state_cannot_drift_from_its_evidence(self):
        matrix = read_matrix()
        strict_rule = next(rule for rule in matrix["rules"]
                           if rule.get("requirement") == "reading_append"
                           and rule.get("layer") == "strict_ci"
                           and rule.get("priority") == 40)
        strict_rule["status"] = "not_run"
        with self.assertRaisesRegex(ValueError, "disagrees with observed gate evidence"):
            CHECKER.validate(matrix)

    def test_new_verified_gate_evidence_can_change_snapshot_outcomes(self):
        matrix = read_matrix()
        new_ids = {
            "strict_reading_append": "replacement-strict-ci",
            "dsh_sidechat_capability": "replacement-dsh-sidechat",
        }
        for gate in matrix["observedGates"]:
            evidence_id = new_ids[gate["kind"]]
            gate["status"] = "passed"
            gate["evidence"] = [evidence_id]
            matrix["evidenceCatalog"].append({
                "id": evidence_id,
                "sourceCommit": matrix["baseline"]["agentkitCommit"],
                "report": {"url": "https://github.com/TekesApps/TekesAgentKit/actions/runs/40041872629"},
                "contract": "none", "topology": "in_process",
                "layers": [gate["match"]["layer"]],
                "requirements": [gate["match"]["requirement"]],
                "run": {"result": "passed"},
            })
        for rule in matrix["rules"]:
            if rule.get("requirement") == "reading_append" and rule.get("layer") == "strict_ci" \
                    and rule.get("priority") == 40:
                rule["status"] = "passed"
                rule["evidence"] = [new_ids["strict_reading_append"]]
            if rule.get("requirement") == "sidechat" and rule.get("harness") == "DSH" \
                    and rule.get("layer") == "offscreen_harness" and rule.get("priority") == 50:
                rule["status"] = "passed"
                rule["evidence"] = [new_ids["dsh_sidechat_capability"]]
        result = CHECKER.validate(matrix)
        self.assertEqual(result["statusCounts"]["failed"], 0)
        self.assertEqual(result["statusCounts"]["unsupported"], 135)

    def test_historical_native_window_pass_requires_native_receipt_scope(self):
        matrix = read_matrix()
        evidence = next(item for item in matrix["evidenceCatalog"]
                        if item["id"] == "history-activity-segments")
        self.assertTrue(evidence["nativeWindow"])
        self.assertTrue(CHECKER.report_has_native_window_scope(
            json.loads((ROOT / evidence["report"]["path"]).read_text(encoding="utf-8")),
            evidence["nativeWindowReportScopes"],
        ))
        evidence["nativeWindow"] = False
        with self.assertRaisesRegex(ValueError, "historical native-window pass lacks nativeWindow=true"):
            CHECKER.validate(matrix)

    def test_native_receipt_scope_must_be_in_named_report_section(self):
        self.assertFalse(CHECKER.report_has_native_window_scope(
            {"unrelated": {"nativeWindow": True}}, ["nativeOwnedHistory"]
        ))

    def test_contenthost_cannot_pass_process_container(self):
        matrix = read_matrix()
        matrix["evidenceCatalog"].append({
            "id": "contenthost-process-negative",
            "sourceCommit": matrix["baseline"]["agentkitCommit"],
            "report": {"url": "https://github.com/TekesApps/TekesAgentKit/issues/27"},
            "contract": "ContentHost", "topology": "cross_process",
            "layers": ["process_container"], "requirements": ["task"],
        })
        matrix["rules"].append({
            "requirement": "task", "harness": "Native", "topology": "cross_process",
            "layer": "process_container", "priority": 100,
            "status": "passed", "reason": "negative test",
            "evidence": ["contenthost-process-negative"],
        })
        with self.assertRaisesRegex(ValueError, "ContentHost cannot pass formal ProcessContainer"):
            CHECKER.validate(matrix)

    def test_v2_matrix_supports_new_dimensions_and_protocol_layer(self):
        matrix = read_v2_fixture()
        current = matrix["baseline"]["agentkitCommit"]
        with tempfile.TemporaryDirectory(dir=ROOT) as directory:
            root = Path(directory)
            source = root / "source.json"
            source.write_text('{"raw":"receipt"}', encoding="utf-8")
            source_ref = {"path": source.relative_to(ROOT).as_posix(),
                          "sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
            program_path = "/fixtures/protocol-probe"
            program_sha = "a" * 64
            binder = root / "binder.json"
            binder.write_text(json.dumps({
                "sourceCommit": current,
                "sourceFiles": {"src/lib.rs": "b" * 64},
                "program": {"path": program_path, "sha256": program_sha},
                "baseCommit": current,
                "sourceReport": source_ref,
                "claims": {"Native": {"transport_recovery": {
                    "harness": "Native", "requirement": "transport_recovery",
                    "topology": "in_process", "layer": "protocol_integration",
                    "assessment": "passed",
                }}},
            }), encoding="utf-8")
            binder_ref = {
                "path": binder.relative_to(ROOT).as_posix(),
                "sha256": hashlib.sha256(binder.read_bytes()).hexdigest(),
            }
            matrix["evidenceCatalog"].append({
                "id": "protocol-only", "sourceCommit": current,
                "report": binder_ref, "sourceReport": source_ref,
                "sourceFiles": {"src/lib.rs": "b" * 64},
                "program": {"path": "fixtures/protocol-probe",
                            "reportedPath": program_path, "sha256": program_sha},
                "topology": "in_process", "contract": "none",
                "harnesses": ["Native"], "layers": ["protocol_integration"],
                "requirements": ["transport_recovery"],
                "reportClaimPath": ["claims", "Native", "transport_recovery"],
            })
            add_rule(matrix, "transport_recovery", "Native", "in_process",
                     "protocol_integration", "protocol-only")
            result = CHECKER.validate(matrix)
            self.assertEqual(result["cells"], 19 * 3 * 2 * 9)

    def test_native_failed_protocol_claim_cannot_be_relabelled_as_passed(self):
        matrix = read_v2_fixture()
        current = matrix["baseline"]["agentkitCommit"]
        with tempfile.TemporaryDirectory(dir=ROOT) as directory:
            root = Path(directory)
            source = root / "source.json"
            source.write_text('{"raw":"receipt"}', encoding="utf-8")
            source_ref = {"path": source.relative_to(ROOT).as_posix(),
                          "sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
            program_path = "/fixtures/protocol-probe"
            program_sha = "a" * 64
            binder = root / "binder.json"
            binder.write_text(json.dumps({
                "sourceCommit": current,
                "sourceFiles": {"src/lib.rs": "b" * 64},
                "program": {"path": program_path, "sha256": program_sha},
                "baseCommit": current,
                "sourceReport": source_ref,
                "claims": {"Native": {"busy_queue_submission_identity": {
                    "harness": "Native", "requirement": "busy_queue_submission_identity",
                    "topology": "in_process", "layer": "protocol_integration",
                    "assessment": "failed",
                }}},
            }), encoding="utf-8")
            binder_ref = {
                "path": binder.relative_to(ROOT).as_posix(),
                "sha256": hashlib.sha256(binder.read_bytes()).hexdigest(),
            }
            matrix["evidenceCatalog"].append({
                "id": "native-failed-claim", "sourceCommit": current,
                "report": binder_ref, "sourceReport": source_ref,
                "sourceFiles": {"src/lib.rs": "b" * 64},
                "program": {"path": "fixtures/protocol-probe",
                            "reportedPath": program_path, "sha256": program_sha},
                "topology": "in_process", "contract": "none",
                "harnesses": ["Native"], "layers": ["protocol_integration"],
                "requirements": ["busy_queue_submission_identity"],
                "reportClaimPath": ["claims", "Native", "busy_queue_submission_identity"],
            })
            add_rule(matrix, "busy_queue_submission_identity", "Native", "in_process",
                     "protocol_integration", "native-failed-claim")
            with self.assertRaisesRegex(ValueError, "protocol report claim assessment mismatch"):
                CHECKER.validate(matrix)

    def test_v2_rejects_old_commit_as_current_pass(self):
        matrix = read_v2_fixture()
        item = copy.deepcopy(matrix["evidenceCatalog"][0])
        item.update({"id": "old-commit-current-pass",
                     "sourceCommit": "f" * 40,
                     "requirements": ["composer_ime"],
                     "layers": ["component_tests"],
                     "harnesses": ["Native"],
                     "topology": "in_process"})
        item.pop("run", None)
        item["report"] = {"url": "https://github.com/TekesApps/TekesAgentKit/actions/runs/2"}
        matrix["evidenceCatalog"].append(item)
        add_rule(matrix, "composer_ime", "Native", "in_process", "component_tests",
                 "old-commit-current-pass")
        with self.assertRaisesRegex(ValueError, "revision differs from matrix baseline"):
            CHECKER.validate(matrix)

    def test_run_head_sha_must_match_evidence_source_commit(self):
        matrix = read_v2_fixture()
        matrix["evidenceCatalog"][0]["run"] = {"headSha": "f" * 40}
        with self.assertRaisesRegex(ValueError, "run head/source revision mismatch"):
            CHECKER.validate(matrix)

    def test_v2_rejects_native_evidence_borrowed_by_pi(self):
        matrix = read_v2_fixture()
        item = copy.deepcopy(matrix["evidenceCatalog"][0])
        item.update({"id": "native-only", "harnesses": ["Native"],
                     "requirements": ["composer_ime"], "layers": ["component_tests"],
                     "topology": "in_process"})
        item["report"] = {"url": "https://github.com/TekesApps/TekesAgentKit/actions/runs/3"}
        matrix["evidenceCatalog"].append(item)
        add_rule(matrix, "composer_ime", "Pi", "in_process", "component_tests", "native-only")
        with self.assertRaisesRegex(ValueError, "evidence scope mismatch"):
            CHECKER.validate(matrix)

    def test_passed_in_process_cell_rejects_cross_process_evidence(self):
        matrix = read_v2_fixture()
        item = copy.deepcopy(matrix["evidenceCatalog"][0])
        item.update({"id": "cross-only", "topology": "cross_process",
                     "contract": "ProcessContainer", "requirements": ["composer_ime"],
                     "layers": ["component_tests"], "harnesses": ["Native"]})
        item["report"] = {"url": "https://github.com/TekesApps/TekesAgentKit/actions/runs/4"}
        matrix["evidenceCatalog"].append(item)
        add_rule(matrix, "composer_ime", "Native", "in_process", "component_tests", "cross-only")
        with self.assertRaisesRegex(ValueError, "evidence scope mismatch"):
            CHECKER.validate(matrix)

    def test_protocol_only_evidence_cannot_pass_ui_layer(self):
        matrix = read_v2_fixture()
        item = copy.deepcopy(matrix["evidenceCatalog"][0])
        item.update({"id": "protocol-only-ui", "requirements": ["composer_ime"],
                     "layers": ["protocol_integration"], "harnesses": ["Native"],
                     "topology": "in_process", "contract": "none"})
        item["report"] = {"url": "https://github.com/TekesApps/TekesAgentKit/actions/runs/5"}
        matrix["evidenceCatalog"].append(item)
        add_rule(matrix, "composer_ime", "Native", "in_process", "offscreen_harness",
                 "protocol-only-ui")
        with self.assertRaisesRegex(ValueError, "evidence scope mismatch"):
            CHECKER.validate(matrix)

    def test_v2_requires_nonempty_unique_evidence_harnesses(self):
        matrix = read_v2_fixture()
        del matrix["evidenceCatalog"][0]["harnesses"]
        with self.assertRaisesRegex(ValueError, "invalid/missing evidence harness scope"):
            CHECKER.validate(matrix)
        matrix = read_v2_fixture()
        matrix["evidenceCatalog"][0]["harnesses"] = ["Native", "Native"]
        with self.assertRaisesRegex(ValueError, "duplicate evidence harness scope"):
            CHECKER.validate(matrix)

    def test_tracked_report_hash_and_program_source_linkage_are_checked_by_default(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "receipt.json"
            program_path = "/fixtures/program"
            program_sha = "a" * 64
            source_commit = "b" * 40
            report_path.write_text(json.dumps({
                "program": {"path": program_path, "sha256": program_sha},
                "baseCommit": source_commit,
            }), encoding="utf-8")
            evidence = {
                "id": "temporary", "sourceCommit": source_commit,
                "report": {"path": "receipt.json", "sha256": hashlib.sha256(
                    report_path.read_bytes()).hexdigest()},
                "program": {"path": "fixture-bin", "reportedPath": program_path,
                            "sha256": program_sha},
            }
            old_root = CHECKER.ROOT
            CHECKER.ROOT = root
            try:
                CHECKER.verify_report_identity(evidence)
                bad_program = copy.deepcopy(evidence)
                bad_program["program"]["sha256"] = "c" * 64
                with self.assertRaisesRegex(ValueError, "program/source report identity mismatch"):
                    CHECKER.verify_report_identity(bad_program)
                bad_commit = copy.deepcopy(evidence)
                bad_commit["sourceCommit"] = "d" * 40
                with self.assertRaisesRegex(ValueError, "program source revision mismatch"):
                    CHECKER.verify_report_identity(bad_commit)
                bad_report_hash = copy.deepcopy(evidence)
                bad_report_hash["report"]["sha256"] = "e" * 64
                with self.assertRaisesRegex(ValueError, "evidence report hash mismatch"):
                    CHECKER.verify_report_identity(bad_report_hash)
            finally:
                CHECKER.ROOT = old_root

    def test_bound_source_report_hash_cannot_be_redeclared_as_trusted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source_report = root / "source.json"
            source_report.write_text('{"result":"original"}', encoding="utf-8")
            source_sha = hashlib.sha256(source_report.read_bytes()).hexdigest()
            source_commit = "b" * 40
            source_files = {"src/lib.rs": "c" * 64}
            program_path = "/fixtures/recovery-probe"
            program_sha = "a" * 64
            binder_data = {
                "sourceCommit": source_commit,
                "sourceFiles": source_files,
                "program": {"path": program_path, "sha256": program_sha},
                "baseCommit": source_commit,
                "sourceReport": {"path": "source.json", "sha256": source_sha},
            }
            binder = root / "binder.json"
            binder.write_text(json.dumps(binder_data), encoding="utf-8")
            evidence = {
                "id": "bound-report", "sourceCommit": source_commit,
                "report": {"path": "binder.json", "sha256": hashlib.sha256(
                    binder.read_bytes()).hexdigest()},
                "sourceReport": binder_data["sourceReport"],
                "sourceFiles": source_files,
                "program": {"path": "fixtures/recovery-probe",
                            "reportedPath": program_path, "sha256": program_sha},
            }
            old_root = CHECKER.ROOT
            CHECKER.ROOT = root
            try:
                CHECKER.verify_report_identity(evidence)
                forged = copy.deepcopy(evidence)
                forged_source = {"path": "source.json", "sha256": "d" * 64}
                binder_data["sourceReport"] = forged_source
                binder.write_text(json.dumps(binder_data), encoding="utf-8")
                forged["report"]["sha256"] = hashlib.sha256(binder.read_bytes()).hexdigest()
                forged["sourceReport"] = forged_source
                with self.assertRaisesRegex(ValueError, "bound source report hash mismatch"):
                    CHECKER.verify_report_identity(forged)
            finally:
                CHECKER.ROOT = old_root


if __name__ == "__main__":
    unittest.main()
