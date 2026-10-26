from __future__ import annotations

import importlib.util
import io
from pathlib import Path
import sys
import threading
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "performance_process_sampler.py"
SPEC = importlib.util.spec_from_file_location("performance_process_sampler_guard_test", SCRIPT)
sampler_module = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(sampler_module)


class RootLaunchGuardTests(unittest.TestCase):
    def test_snapshot_taken_after_spawn_waits_for_explicit_root_registration(self):
        sampler = sampler_module.ProcessSampler(Path("unused"))
        sampler.stream = io.StringIO()
        os_row = {123: {"pid": 123, "ppid": 10, "rssKiB": 32,
                        "startIdentity": "Mon Oct 26 08:05:40 2026",
                        "command": "/fixed/consumer", "executablePath": "/fixed/consumer"}}
        observed = threading.Event()
        failures = []

        def process_table():
            observed.set()
            return os_row

        def sample():
            try:
                sampler._sample()
            except Exception as error:  # surfaced in the owning test thread
                failures.append(error)

        with patch.object(sampler_module, "process_table", side_effect=process_table):
            with sampler.root_launch_guard():
                worker = threading.Thread(target=sample)
                worker.start()
                self.assertTrue(observed.wait(timeout=2))
                root = sampler.register(type("Process", (), {"pid": 123})(), "agentkit")
                self.assertEqual(root["startIdentity"], os_row[123]["startIdentity"])
            worker.join(timeout=2)

        self.assertFalse(worker.is_alive())
        self.assertEqual(failures, [])
        self.assertEqual(sampler.peaks["agentkit"], 32)
        self.assertEqual(sampler.peaks["provider"], 0)
        self.assertIn('"role": "agentkit"', sampler.stream.getvalue())

    def test_process_table_snapshot_before_spawn_cannot_misclassify_new_root(self):
        sampler = sampler_module.ProcessSampler(Path("unused"))
        sampler.stream = io.StringIO()
        snapshot_taken = threading.Event()
        release_snapshot = threading.Event()
        before = {}
        after = {123: {"pid": 123, "ppid": 10, "rssKiB": 32,
                       "startIdentity": "Mon Oct 26 08:05:40 2026",
                       "command": "/fixed/consumer", "executablePath": "/fixed/consumer"}}
        snapshots = 0
        failures = []

        def process_table():
            nonlocal snapshots
            snapshots += 1
            if snapshots == 1:
                snapshot_taken.set()
                self.assertTrue(release_snapshot.wait(timeout=2))
                return before
            return after

        def sample():
            try:
                sampler._sample()
            except Exception as error:
                failures.append(error)

        with patch.object(sampler_module, "process_table", side_effect=process_table):
            worker = threading.Thread(target=sample)
            worker.start()
            self.assertTrue(snapshot_taken.wait(timeout=2))
            with sampler.root_launch_guard():
                sampler.register(type("Process", (), {"pid": 123})(), "agentkit")
            release_snapshot.set()
            worker.join(timeout=2)
            sampler._sample()

        self.assertFalse(worker.is_alive())
        self.assertEqual(failures, [])
        self.assertEqual(sampler.peaks["agentkit"], 32)
        self.assertEqual(sampler.peaks["provider"], 0)
        self.assertEqual(sampler.samples, 2)


if __name__ == "__main__":
    unittest.main()
