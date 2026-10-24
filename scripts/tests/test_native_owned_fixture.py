import base64
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_owned_fixture import owned_native, _stop


class _Input(io.BytesIO):
    def __init__(self):
        super().__init__()
        self.closed_by_helper = False
        self.saved_value = None

    def close(self):
        self.closed_by_helper = True
        self.saved_value = self.getvalue()
        super().close()


class _Process:
    def __init__(self, *, ready=True, stderr=b"", wait_times_out=False):
        self.stdin = _Input()
        self.stdout = io.BytesIO((b'{"url":"http://127.0.0.1:43210"}\n' if ready else b""))
        self.stderr = io.BytesIO(stderr)
        self.returncode = None
        self.terminate_calls = 0
        self.wait_calls = []
        self.wait_times_out = wait_times_out

    def poll(self):
        return self.returncode

    def wait(self, timeout=None):
        self.wait_calls.append(timeout)
        if self.wait_times_out and len(self.wait_calls) == 1:
            raise subprocess.TimeoutExpired("owned-supervisor", timeout)
        self.returncode = 0
        return 0

    def terminate(self):
        self.terminate_calls += 1
        self.returncode = -15


class NativeOwnedFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.supervisor = self.base / "tekes-supervisor"
        self.worker = self.base / "tekes-worker"
        self.supervisor.write_bytes(b"supervisor")
        self.worker.write_bytes(b"worker")
        self.providers = {"format": 1, "providers": [{
            "id": "openai", "credential_key": "openai-main",
            "endpoint": "https://example.invalid/v1",
            "models": [{"id": "gpt-test"}],
        }]}
        self.environment = {
            "PATH": "/controlled/bin",
            "HOME": "/caller/home",
            "TEKES_KERNEL_ENDPOINT_TOKEN": "inherited-token",
            "AGENTKIT_SYNTHETIC_KEY": "inherited-key",
        }

    def tearDown(self):
        self.temp.cleanup()

    def invoke(self, process, *, ready_timeout=15):
        return owned_native(
            self.supervisor, self.worker, self.providers,
            "http://127.0.0.1:43123/v1", self.base, self.environment,
            readiness_timeout=ready_timeout,
        )

    def test_ready_yields_owned_handles_and_private_bootstrap_configuration(self):
        process = _Process()
        captured = {}

        def popen(argv, **kwargs):
            captured["argv"] = argv
            captured["env"] = kwargs["env"]
            captured["launch"] = Path(argv[-1])
            return process

        with patch("native_owned_fixture.subprocess.Popen", side_effect=popen), \
                patch("native_owned_fixture.select.select", return_value=([process.stdout], [], [])), \
                patch("native_owned_fixture.secrets.token_bytes", return_value=bytes(range(32))):
            with self.invoke(process) as fixture:
                self.assertIs(fixture["process"], process)
                self.assertEqual(fixture["url"], "http://127.0.0.1:43210")
                self.assertEqual(fixture["bearer"], base64.urlsafe_b64encode(bytes(range(32))).decode().rstrip("="))
                self.assertEqual(fixture["state_root"], (self.base / "kernel").resolve())
                self.assertEqual(fixture["providers"]["providers"][0]["endpoint"], "http://127.0.0.1:43123/v1")
                self.assertEqual(self.providers["providers"][0]["endpoint"], "https://example.invalid/v1")
                self.assertNotIn("TEKES_KERNEL_ENDPOINT_TOKEN", captured["env"])
                self.assertNotIn("AGENTKIT_SYNTHETIC_KEY", captured["env"])
                self.assertEqual(captured["env"]["PATH"], "/controlled/bin")
                self.assertEqual(captured["env"]["HOME"], str((self.base / "kernel-home").resolve()))
                self.assertEqual(captured["argv"][:2], [str(self.supervisor.resolve()), "--built-in"])
                launch = json.loads(captured["launch"].read_text())
                self.assertEqual(launch["worker"], str(self.worker.resolve()))
                self.assertEqual(launch["credential_bindings"], {"openai-main": "AGENTKIT_SYNTHETIC_KEY"})
                settings = json.loads((self.base / "kernel/config/settings.json").read_text())
                self.assertEqual(settings["limits"], {"max_workers": 8, "max_provider_leases": 8})
                bootstrap = process.stdin.getvalue()
                prefix = b"TEKES-KERNEL-BOOTSTRAP/1\n"
                self.assertTrue(bootstrap.startswith(prefix))
                payload_size = int.from_bytes(bootstrap[len(prefix):len(prefix) + 4], "big")
                payload = json.loads(bootstrap[len(prefix) + 4:])
                self.assertEqual(payload_size, len(bootstrap[len(prefix) + 4:]))
                self.assertEqual(payload["endpointToken"], bytes(range(32)).hex())
                self.assertEqual(payload["credentials"], {"openai-main": "synthetic-key"})
                self.assertEqual(stat.S_IMODE((self.base / "kernel").stat().st_mode), 0o700)
                self.assertEqual(stat.S_IMODE((self.base / "kernel/config").stat().st_mode), 0o700)
                self.assertEqual(stat.S_IMODE((self.base / "kernel/config/settings.json").stat().st_mode), 0o600)
                self.assertEqual(stat.S_IMODE((self.base / "kernel-home").stat().st_mode), 0o700)
                self.assertEqual(stat.S_IMODE(captured["launch"].stat().st_mode), 0o600)
            self.assertTrue(process.stdin.closed_by_helper)
            self.assertEqual(process.wait_calls, [15])
            self.assertEqual(process.terminate_calls, 0)

    def test_existing_launch_evidence_is_never_overwritten(self):
        launch = self.base / "kernel-launch.json"
        launch.write_text("keep-existing-evidence")
        with patch("native_owned_fixture.subprocess.Popen") as popen:
            with self.assertRaisesRegex(ValueError, "launch path already exists"):
                with self.invoke(_Process()):
                    self.fail("must refuse pre-existing evidence")
            popen.assert_not_called()
        self.assertEqual(launch.read_text(), "keep-existing-evidence")
        self.assertFalse((self.base / "kernel").exists())

    def test_readiness_timeout_closes_and_stops_only_owned_supervisor(self):
        process = _Process(wait_times_out=True)
        with patch("native_owned_fixture.subprocess.Popen", return_value=process), \
                patch("native_owned_fixture.select.select", return_value=([], [], [])), \
                patch("native_owned_fixture.os.killpg") as killpg:
            with self.assertRaisesRegex(TimeoutError, "readiness timed out"):
                with self.invoke(process, ready_timeout=0.01):
                    self.fail("timeout must not yield a fixture")
        self.assertTrue(process.stdin.closed_by_helper)
        self.assertEqual(process.terminate_calls, 1)
        self.assertEqual(process.wait_calls, [15, 10])
        killpg.assert_not_called()

    def test_early_exit_reports_sanitized_stderr_without_secrets(self):
        process = _Process(ready=False, stderr=b"provider error synthetic-key")
        with patch("native_owned_fixture.subprocess.Popen", return_value=process), \
                patch("native_owned_fixture.select.select", return_value=([process.stdout], [], [])), \
                patch("native_owned_fixture.secrets.token_bytes", return_value=b"x" * 32):
            with self.assertRaises(RuntimeError) as caught:
                with self.invoke(process):
                    self.fail("early process exit must not yield a fixture")
        message = str(caught.exception)
        self.assertIn("provider error [redacted]", message)
        self.assertNotIn("synthetic-key", message)
        self.assertNotIn((b"x" * 32).hex(), message)
        self.assertTrue(process.stdin.closed_by_helper)
        self.assertEqual(process.terminate_calls, 0)

    def test_invalid_readiness_is_generic_and_keeps_token_out_of_error(self):
        process = _Process()
        process.stdout = io.BytesIO(b'{"url":"https://not-local.invalid"}\n')
        with patch("native_owned_fixture.subprocess.Popen", return_value=process), \
                patch("native_owned_fixture.select.select", return_value=([process.stdout], [], [])), \
                patch("native_owned_fixture.secrets.token_bytes", return_value=b"z" * 32):
            with self.assertRaisesRegex(RuntimeError, "invalid readiness data") as caught:
                with self.invoke(process):
                    self.fail("invalid readiness must not yield a fixture")
        self.assertNotIn((b"z" * 32).hex(), str(caught.exception))
        self.assertTrue(process.stdin.closed_by_helper)

    def test_bad_inputs_fail_before_creating_owned_state_or_launching(self):
        with patch("native_owned_fixture.subprocess.Popen") as popen:
            with self.assertRaisesRegex(ValueError, "local fixture provider"):
                with owned_native(self.supervisor, self.worker, self.providers,
                                  "https://remote.invalid/v1", self.base, self.environment):
                    self.fail("invalid provider must not start")
            popen.assert_not_called()
        self.assertFalse((self.base / "kernel").exists())


if __name__ == "__main__":
    unittest.main()


class NativeShutdownFailure(unittest.TestCase):
    def test_unstopped_owned_supervisor_is_reported_without_group_kill(self):
        process = _Process()
        with patch.object(process, "wait", side_effect=subprocess.TimeoutExpired("owned", 10)), \
                patch("os.killpg") as kill_group:
            with self.assertRaisesRegex(RuntimeError, "did not exit"):
                _stop(process)
        self.assertEqual(process.terminate_calls, 1)
        kill_group.assert_not_called()
