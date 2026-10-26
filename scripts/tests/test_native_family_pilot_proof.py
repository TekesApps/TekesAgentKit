from __future__ import annotations

import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1]
REPO = SCRIPTS.parent
PILOT = REPO / ".runtime-fixtures/stage-close-20261025/performance/native-family-toolchain-pilot-v3"
sys.path.insert(0, str(SCRIPTS))
import performance_native_family_pilot_proof as proof


@unittest.skipUnless(PILOT.is_dir(), "local frozen pilot evidence is not in public CI")
class FrozenPilotEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.receipt_path = PILOT / "official-execution-receipt.json"
        self.sample_path = PILOT / "official-sample.json"
        self.receipt = json.loads(self.receipt_path.read_text(encoding="utf-8"))
        self.registration = json.loads(self.sample_path.read_text(encoding="utf-8"))["registration"]

    def test_original_pilot_is_rechecked_and_corrected_without_rewriting_evidence(self):
        before = {name: proof._hash_file(PILOT / name) for name in (
            "inputs.json", "official-execution-receipt.json", "measurement-rss.jsonl",
            "last-rss-summary.json", "consumer-process-boundaries.jsonl")}
        result = proof.build_pilot_proof(PILOT, self.registration)
        after = {name: proof._hash_file(PILOT / name) for name in before}
        self.assertEqual(before, after)
        self.assertFalse(result["official"])
        self.assertTrue(result["diagnosticOnly"])
        self.assertFalse(result["performanceAccepted"])
        self.assertTrue(result["identityRecheckedDerived"])
        self.assertFalse(result["originalIdentityRechecked"])
        self.assertEqual(self.receipt["identityRechecked"], False)
        self.assertEqual(self.receipt["status"], "failed")
        self.assertEqual(len(result["rss"]["roleCorrections"]), 1)
        self.assertEqual(result["rss"]["roleCorrections"][0]["fromRole"], "provider")
        self.assertEqual(result["rss"]["roleCorrections"][0]["toRole"], "agentkit")
        self.assertEqual(result["rss"]["rawPeaksKiB"], result["rss"]["correctedPeaksKiB"])
        self.assertEqual(result["rss"]["conservativePeaksKiB"], result["rss"]["rawPeaksKiB"])

    def test_changed_registration_view_is_rejected(self):
        altered = dict(self.registration, role="baseline")
        with self.assertRaisesRegex(ValueError, "changed original field role"):
            proof.build_pilot_proof(PILOT, altered)

    def test_unknown_rss_role_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            case = Path(directory)
            for name in ("measurement-rss.jsonl", "last-rss-summary.json",
                         "consumer-process-boundaries.jsonl", "consumer-exit.json",
                         "owned-process-retirement.json", "fixture-lifecycle.json"):
                shutil.copy2(PILOT / name, case / name)
            rows = [json.loads(line) for line in (case / "measurement-rss.jsonl").read_text().splitlines()]
            target = next(process for process in rows[13]["processes"]
                          if process["pid"] == self.receipt["consumerExit"]["pid"])
            target["role"] = "mystery"
            (case / "measurement-rss.jsonl").write_text(
                "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows))
            with self.assertRaisesRegex(ValueError, "unknown"):
                proof._rss_proof(case, self.registration, self.receipt,
                                 self.receipt["programSha256"])


if __name__ == "__main__":
    unittest.main()
