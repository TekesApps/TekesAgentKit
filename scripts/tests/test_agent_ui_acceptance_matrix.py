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
SPEC = importlib.util.spec_from_file_location("acceptance_matrix_checker", CHECKER_PATH)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


def read_matrix():
    return json.loads(MATRIX_PATH.read_text(encoding="utf-8"))


class AcceptanceMatrixTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
