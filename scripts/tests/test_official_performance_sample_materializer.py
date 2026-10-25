from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "run-official-performance-sample.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("official_sample_runner", SCRIPT)
runner = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(runner)


class OfficialSampleMaterializerTests(unittest.TestCase):
    def test_non_ascii_seed_digest_is_compatible_with_strict_report(self):
        from performance_provider_fixture import body_text
        from performance_sample_report import _seed_digest
        content = {"bodyMessages": [body_text('seed', 0, turn, role)
                    for turn in range(1, 51) for role in ('user', 'assistant')],
                   "sourceBytesSha256": ['a' * 64] * 10,
                   "callIds": [f'original-{turn}' for turn in range(1, 11)]}
        self.assertEqual(runner._digest(content), _seed_digest({'normalizedContent': content}))

    def test_failed_native_run_is_retained_and_diagnostic_receipt_is_not_promoted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            registration_path = root / "registration.json"
            registration_path.write_text("{}\n", encoding="utf-8")
            case_name = "native-cold-b0"
            registration = {"sampleId": "native-cold-b0", "caseName": case_name,
                "role": "baseline", "endpoint": "native", "mode": "cold", "caseIndex": 0,
                "sourceCommit": "a" * 40, "programSha256": "b" * 64,
                "consumerSha256": "c" * 64, "runnerSha256": "d" * 64,
                "runtimeSha256": "e" * 64, "runtimeFiles": [], "runtimeArguments": {},
                "programReceiptPath": str(root / "program-receipt.json"),
                "seederReceiptPath": str(root / "seeder-receipt.json")}
            pins = {"registration": registration, "profile": {"dataset": {"seed": 1}},
                "driver": Path("/usr/bin/python3"), "caseRunner": Path("/tmp/case-runner.py"),
                "program": Path("/tmp/program"), "seeder": Path("/tmp/seeder"),
                "profilePath": root / "profile.json"}
            calls = []

            def validate(*args, **kwargs):
                calls.append(kwargs.get("caller_role"))
                return pins

            def captured_subprocess(commands, **kwargs):
                assert calls == ["outer"]  # registration was consumed before launch
                case = runner.ROOT / case_name
                case.mkdir(parents=True)
                (case / "inputs.json").write_text(json.dumps({"machineBefore": {}}), encoding="utf-8")
                (case / "seed-commands.json").write_text(json.dumps([
                    {"op": "call", "method": "session.create", "args": {"sessionId": "native-A"}},
                    {"op": "call", "method": "session.create", "args": {"sessionId": "native-B"}},
                ]), encoding="utf-8")
                (case / "consumer.stdout.jsonl").write_text(
                    json.dumps({"event": "consumer_started", "sessionIds": ["native-A", "native-B"]}) + "\n",
                    encoding="utf-8")
                (case / "official-execution-receipt.json").write_text(json.dumps({
                    "official": True, "diagnosticOnly": False, "status": "failed",
                    "failureReason": "simulated preflight failure"}), encoding="utf-8")
                # A successful diagnostic receipt in the same directory must
                # never turn this registered official run into a success.
                (case / "diagnostic-receipt.json").write_text(json.dumps({
                    "acceptedDiagnosticPlumbing": True, "officialSamples": 0}), encoding="utf-8")
                class Result:
                    returncode = 1
                    stdout = ""
                    stderr = "simulated preflight failure"
                return Result()

            with patch.object(runner, "ROOT", root / "runs"), \
                 patch.object(runner, "validate_registration", side_effect=validate), \
                 patch.object(runner.subprocess, "run", side_effect=captured_subprocess):
                output = runner.execute(registration_path)

            sample = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(calls, ["outer", "outer"])
            self.assertIs(sample["official"], True)
            self.assertIs(sample["diagnosticOnly"], False)
            self.assertEqual(sample["status"], "failed")
            self.assertEqual(sample["sessionIds"], ["native-A", "native-B"])
            self.assertEqual(sample["failureReason"], "simulated preflight failure")


if __name__ == "__main__":
    unittest.main()
