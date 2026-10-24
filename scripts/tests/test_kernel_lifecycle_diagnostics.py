from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from scripts.kernel_lifecycle_diagnostics import KernelLifecycleDiagnostics


class KernelLifecycleDiagnosticsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.state = self.base / "isolated-kernel-state"
        self.state.mkdir()
        self.output = self.base / "evidence"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def make_sampler(self, runner: Any, **kwargs: Any) -> KernelLifecycleDiagnostics:
        return KernelLifecycleDiagnostics(
            self.state,
            self.output,
            interval=0.005,
            timeout=0.05,
            join_timeout=0.2,
            runner=runner,
            **kwargs,
        )

    def wait_until(self, predicate: Any, timeout: float = 1.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.002)
        self.fail("condition did not become true before timeout")

    def read_records(self) -> list[dict[str, Any]]:
        return [
            json.loads(line)
            for line in (self.output / "samples.jsonl").read_text(encoding="utf-8").splitlines()
        ]

    def test_rejects_unsafe_paths_and_nonempty_output(self) -> None:
        with self.assertRaises(ValueError):
            KernelLifecycleDiagnostics("relative", self.output)
        with self.assertRaises(ValueError):
            KernelLifecycleDiagnostics(self.state, self.state / "nested-evidence")
        with self.assertRaises(ValueError):
            KernelLifecycleDiagnostics(Path(self.state.anchor), self.output)
        with self.assertRaises(OSError):
            KernelLifecycleDiagnostics(self.base / "missing", self.output)

        self.output.mkdir()
        (self.output / "keep.txt").write_text("existing evidence", encoding="utf-8")
        sampler = KernelLifecycleDiagnostics(self.state, self.output, runner=lambda *_a, **_k: None)
        sampler.start()
        self.assertEqual("start_error", sampler.metadata["errors"][0]["kind"])
        sampler.stop()
        self.assertEqual("existing evidence", (self.output / "keep.txt").read_text())

    def test_cli_rejects_ui_and_mixed_harness_profiles_before_runtime_start(self) -> None:
        script = Path(__file__).resolve().parents[1] / "test-real-session-endpoints.py"
        for extra in [
            [],
            ["--kernel-root", str(self.state), "--ui-components"],
            ["--kernel-root", str(self.state), "--kernel-ui-only"],
            ["--kernel-root", str(self.state), "--pi-root", str(self.state)],
        ]:
            with self.subTest(extra=extra):
                result = subprocess.run(
                    [sys.executable, str(script), "--kernel-lifecycle-diagnostics", str(self.output), *extra],
                    capture_output=True, text=True, timeout=10, check=False,
                )
                self.assertEqual(2, result.returncode)
                self.assertIn("requires an isolated Kernel headless profile", result.stderr)
                self.assertNotIn("NATIVE_RUNTIME_BUILD_RECEIPT", result.stdout)
                self.assertFalse(self.output.exists())

    def test_duration_limits_must_be_finite(self) -> None:
        for key in ["interval", "timeout", "join_timeout"]:
            for value in [float("inf"), float("nan"), 0, -1]:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    KernelLifecycleDiagnostics(self.state, self.output, **{key: value})

    def test_lsof_exit_one_is_a_normal_empty_sample_and_command_is_read_only(self) -> None:
        calls: list[tuple[tuple[Any, ...], dict[str, Any]]] = []

        def runner(*args: Any, **kwargs: Any) -> Any:
            calls.append((args, kwargs))
            return SimpleNamespace(returncode=1, stdout="", stderr="")

        sampler = self.make_sampler(runner, max_samples=3)
        sampler.start()
        self.wait_until(lambda: len(self.read_records()) >= 2)
        metadata = sampler.stop()

        records = self.read_records()
        samples = [record for record in records if record["type"] == "sample"]
        self.assertGreaterEqual(len(samples), 1)
        self.assertEqual("empty", samples[0]["status"])
        self.assertEqual(1, samples[0]["returncode"])
        self.assertFalse(metadata["lock_availability_proven"])
        self.assertFalse(metadata["missing_sample_proves_no_holder"])
        self.assertTrue(calls)
        args, kwargs = calls[0]
        self.assertEqual(
            ["lsof", "-nP", "-Fpcflnt", "+D", str(self.state.resolve())], list(args[0])
        )
        self.assertIs(kwargs["shell"], False)
        self.assertTrue(kwargs["capture_output"])
        self.assertTrue(kwargs["check"] is False)

    def test_timeout_and_runner_error_are_preserved(self) -> None:
        outcomes = iter(
            [
                subprocess.TimeoutExpired("lsof", 0.05, output=b"partial", stderr=b"slow"),
                OSError("lsof missing"),
            ]
        )

        def runner(*_args: Any, **_kwargs: Any) -> Any:
            outcome = next(outcomes)
            if isinstance(outcome, BaseException):
                raise outcome
            return outcome

        sampler = self.make_sampler(runner, max_samples=2)
        sampler.start()
        self.wait_until(lambda: len(self.read_records()) >= 2)
        metadata = sampler.stop()

        samples = [record for record in self.read_records() if record["type"] == "sample"]
        self.assertEqual("timeout", samples[0]["status"])
        self.assertEqual("partial", samples[0]["stdout"])
        self.assertEqual("slow", samples[0]["stderr"])
        self.assertEqual("error", samples[1]["status"])
        self.assertIn("OSError: lsof missing", samples[1]["error"])
        self.assertEqual(["timeout", "error"], [error["kind"] for error in metadata["errors"]])

    def test_partial_output_is_never_reported_as_empty(self) -> None:
        sampler = self.make_sampler(
            lambda *_a, **_k: SimpleNamespace(
                returncode=1, stdout="p42\nf3\nlW\nn/owned/lock\n", stderr="warning"
            ),
            max_samples=1,
        )
        sampler.start()
        self.wait_until(lambda: sampler.metadata["sample_count"] == 1)
        metadata = sampler.stop()
        sample = self.read_records()[0]
        self.assertEqual("partial", sample["status"])
        self.assertIn("p42", sample["stdout"])
        self.assertEqual("warning", sample["stderr"])
        self.assertEqual("partial", metadata["errors"][0]["kind"])

    def test_stop_preserves_the_observation_already_in_flight(self) -> None:
        entered, release = threading.Event(), threading.Event()

        def runner(*_a: Any, **_k: Any) -> Any:
            entered.set()
            self.assertTrue(release.wait(1))
            return SimpleNamespace(returncode=0, stdout="p42\nlW\n", stderr="")

        sampler = self.make_sampler(runner)
        sampler.start()
        self.assertTrue(entered.wait(1))
        stopper = threading.Thread(target=sampler.stop)
        stopper.start()
        self.assertTrue(sampler._stop_event.wait(1))
        release.set()
        stopper.join(1)
        self.assertFalse(stopper.is_alive())
        self.assertEqual("p42\nlW\n", self.read_records()[0]["stdout"])
        self.assertEqual(1, sampler.metadata["sample_count"])
        self.assertEqual([], sampler.metadata["errors"])

    def test_max_samples_emits_explicit_truncation_and_honors_marks(self) -> None:
        calls = 0

        def runner(*_args: Any, **_kwargs: Any) -> Any:
            nonlocal calls
            calls += 1
            return SimpleNamespace(returncode=0, stdout=f"p{calls}\n", stderr="")

        sampler = self.make_sampler(runner, max_samples=2)
        sampler.start()
        sampler.mark("baseline-ready")
        self.wait_until(lambda: any(r["type"] == "truncated" for r in self.read_records()))
        sampler.mark("batch-start")
        metadata = sampler.stop()

        records = self.read_records()
        self.assertEqual(2, sum(record["type"] == "sample" for record in records))
        self.assertEqual(1, sum(record["type"] == "truncated" for record in records))
        self.assertEqual(2, metadata["sample_count"])
        self.assertTrue(metadata["truncated"])
        self.assertEqual(["baseline-ready", "batch-start"], [mark["label"] for mark in metadata["marks"]])
        self.assertLessEqual(calls, 3)

    def test_stop_joins_and_context_manager_records_final_metadata(self) -> None:
        finished = threading.Event()

        def runner(*_args: Any, **_kwargs: Any) -> Any:
            finished.set()
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with self.make_sampler(runner, max_samples=20) as sampler:
            sampler.mark("ready")
            self.assertTrue(finished.wait(0.5))
        self.assertIsNotNone(sampler._thread)
        self.assertFalse(sampler._thread.is_alive())
        metadata = json.loads((self.output / "metadata.json").read_text(encoding="utf-8"))
        self.assertIsNotNone(metadata["stopped_at"])
        self.assertEqual(metadata, sampler.stop())
        self.assertEqual(0o700, os.stat(self.output).st_mode & 0o777)
        self.assertEqual(0o600, os.stat(self.output / "samples.jsonl").st_mode & 0o777)
        self.assertEqual(0o600, os.stat(self.output / "metadata.json").st_mode & 0o777)


if __name__ == "__main__":
    unittest.main()
