from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "run-native-family-performance-case.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("native_family_case_runner", SCRIPT)
runner = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


class NativeFamilyRegistrationTests(unittest.TestCase):
    def _pins(self, root: Path, *, path="common50", role="baseline", mode="cold",
              feature_override=None, thresholds_override=None, case_index=0):
        base_path = root / "agentkit-stage-performance-profile-20261025.json"
        thresholds = {"latencyRegressionRatioMax": 1.2,
            "peakRssRegressionRatioMax": 1.15, "firstUsableSnapshotP95Ms": 10000,
            "sendToFirstPublicFrameP95Ms": 2000, "historyPageToProjectionP95Ms": 1000,
            "explicitReconnectToConvergedP95Ms": 5000, "agentkitPeakRssMiBMax": 512,
            "ownedHarnessPeakRssMiBMax": 2048, "ownedCleanupDeadlineMs": 10000}
        base = {"format": 1, "thresholds": thresholds,
                "baseline": "f1daa0ffcf6ced41e21f351e22845455ca78006c",
                "resolvedProductCandidate": "10263722b5fb128922c49a31b5942c41333cd6e2"}
        base_path.write_text(json.dumps(base), encoding="utf-8")
        family_path = root / runner.EXPECTED_PROFILE_NAME
        family = {
            "format": 1, "kind": runner.FAMILY_KIND, "issue": 56,
            "status": "preregistered-not-run", "baseProfile": base_path.name,
            "baseProfileSha256": runner.file_sha256(base_path),
            "sourceProducts": {"baseline": "f1daa0ffcf6ced41e21f351e22845455ca78006c",
                "candidate": "10263722b5fb128922c49a31b5942c41333cd6e2"},
            "runtime": {"kind": "native", "kernelVersion": "0.9.3",
                "source": "09508a9083c24c7accefc99516739f279b85913e"},
            "paths": {"common50": {"roles": ["baseline", "candidate"]},
                "product20": {"roles": ["candidate"]}},
            "caseCountsPerPathRole": {"cold": 5, "warm": 1, "reconnectLifecycle": 1},
            "plannedCaseUpperBound": 21,
            "thresholds": thresholds_override or thresholds,
            "seed": {"semanticCorpusSha256": "a" * 64,
                     "producerSourceSha256": runner.file_sha256(
                         SCRIPT.with_name("performance_native_family_seed.py"))},
        }
        family_path.write_text(json.dumps(family), encoding="utf-8")
        feature = feature_override
        if feature is None:
            feature = [] if path == "common50" else ["acceptance-native-family-page20"]
        commit = family["sourceProducts"][role]
        registration = {
            "format": 1, "official": True, "endpoint": "native", "role": role,
            "mode": mode, "caseIndex": case_index, "iterations": runner.EXPECTED_ITERATIONS[mode],
            "caseName": "native-family-case", "sampleId": "native-family-sample",
            "sourceCommit": commit, "familyPath": path,
            "familyProfilePath": str(family_path),
            "familyProfileSha256": runner.file_sha256(family_path),
            "plannedCaseUpperBound": 21,
        }
        pins = {"registration": registration, "profile": base, "profilePath": base_path,
            "programReceipt": {"cargoFeatures": feature, "sourceCommit": commit},
            "program": root / "agentkit", "family": family}
        pins["program"].write_bytes(b"binary")
        return pins

    def test_common50_accepts_empty_features_for_each_permitted_role(self):
        for role in ("baseline", "candidate"):
            with self.subTest(role=role), tempfile.TemporaryDirectory() as temporary:
                pins = self._pins(Path(temporary), role=role)
                self.assertEqual(runner.validate_family_registration(pins)["familyProfile"]["kind"],
                                 runner.FAMILY_KIND)

    def test_product20_is_candidate_only_and_requires_exact_feature(self):
        with tempfile.TemporaryDirectory() as temporary:
            pins = self._pins(Path(temporary), path="product20", role="candidate")
            runner.validate_family_registration(pins)
            pins["registration"]["role"] = "baseline"
            pins["registration"]["sourceCommit"] = pins["family"]["sourceProducts"]["baseline"]
            pins["programReceipt"]["sourceCommit"] = pins["registration"]["sourceCommit"]
            with self.assertRaisesRegex(ValueError, "not permitted|candidate-only"):
                runner.validate_family_registration(pins)

    def test_product20_rejects_missing_or_extra_features(self):
        for features in ([], ["acceptance-native-family-page20", "other"]):
            with self.subTest(features=features), tempfile.TemporaryDirectory() as temporary:
                pins = self._pins(Path(temporary), path="product20", role="candidate",
                                  feature_override=features)
                with self.assertRaisesRegex(ValueError, "cargoFeatures"):
                    runner.validate_family_registration(pins)

    def test_profile_thresholds_and_case_boundaries_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pins = self._pins(root, thresholds_override={"ownedCleanupDeadlineMs": 1})
            with self.assertRaisesRegex(ValueError, "thresholds"):
                runner.validate_family_registration(pins)
        for mode, index in (("cold", 5), ("warm", 1), ("reconnectLifecycle", 1)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                pins = self._pins(Path(temporary), mode=mode, case_index=index)
                with self.assertRaisesRegex(ValueError, "caseIndex"):
                    runner.validate_family_registration(pins)

    def test_preflight_failure_precedes_evidence_directory_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            registration = root / "registration.json"
            registration.write_text("{}\n", encoding="utf-8")
            evidence_root = root / "evidence"
            with patch.object(runner, "ROOT", evidence_root), \
                 patch.object(runner, "_validate_outer", side_effect=ValueError("bad immutable pin")), \
                 patch.object(runner, "_load_case_tools") as load_tools:
                with self.assertRaisesRegex(ValueError, "bad immutable pin"):
                    runner.execute(registration)
                load_tools.assert_not_called()
            self.assertFalse(evidence_root.exists())

    def test_pilot_requires_explicit_diagnostic_purpose_and_never_promotes(self):
        reg = {"official": True, "purpose": "toolchain-pilot", "diagnosticOnly": True}
        runner._validate_purpose(reg, pilot=True)
        with self.assertRaisesRegex(ValueError, "requires --pilot"):
            runner._validate_purpose(reg, pilot=False)
        with self.assertRaisesRegex(ValueError, "explicitly registered"):
            runner._validate_purpose({"official": True}, pilot=True)
        with self.assertRaisesRegex(ValueError, "explicitly registered"):
            runner._validate_purpose({"official": False, "purpose": "toolchain-pilot",
                                      "diagnosticOnly": True}, pilot=True)

    def test_official_matrix_rejects_diagnostic_or_unknown_registration_purpose(self):
        for reg in (
            {"official": True, "purpose": "toolchain-pilot", "diagnosticOnly": True},
            {"official": True, "purpose": "other", "diagnosticOnly": False},
            {"official": True, "purpose": "official", "diagnosticOnly": True},
        ):
            with self.subTest(reg=reg), self.assertRaises(ValueError):
                runner._validate_purpose(reg, pilot=False)
        runner._validate_purpose({"official": True}, pilot=False)

    def test_runner_has_explicit_non_promoting_pilot_flag(self):
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn("diagnostic_candidate_receipt", source)
        self.assertNotIn("retry(", source.lower())
        self.assertIn('parser.add_argument("--registration"', source)
        self.assertIn('parser.add_argument("--pilot"', source)
        self.assertIn('materialize_sample(case, report_registration, pins["profile"], family, pilot=pilot)', source)


if __name__ == "__main__":
    unittest.main()
