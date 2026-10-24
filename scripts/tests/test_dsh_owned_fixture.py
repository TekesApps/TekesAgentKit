import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dsh_owned_fixture import _redact, owned_dsh


class _Process:
    def __init__(self, output):
        from io import StringIO
        self.stdout = StringIO(output)
        self.pid = 987654
        self.returncode = 0

    def poll(self):
        return self.returncode

    def wait(self, timeout=None):
        return self.returncode


class _Response:
    headers = {"Set-Cookie": "dsh-session=private-cookie; HttpOnly; Path=/"}


class _Opener:
    def __init__(self):
        self.url = None

    def open(self, url, timeout=None):
        self.url = url
        return _Response()


class DshOwnedFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.runtime = self.base / "runtime"
        self.node = self.runtime / "node/bin/node"
        self.entry = self.runtime / "node_modules/@deepseek-ai/dsh/lib/bin.js"
        self.boot = self.runtime / "node_modules/@deepseek-ai/dsh-app-boot/lib/index.js"
        self.package = self.runtime / "node_modules/@deepseek-ai/dsh/package.json"
        for path in (self.node, self.entry, self.boot):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture", encoding="utf-8")
        self.package.write_text(json.dumps({"version": "0.2.1-rc.2"}), encoding="utf-8")
        self.data = self.base / "owned-data"
        self.home = self.base / "owned-home"
        self.cwd = self.base / "workspace"
        self.cwd.mkdir()
        self.opener = _Opener()
        self.launch = "http://127.0.0.1:43123/?token=launch-secret"
        self.process = _Process("DSH startup detail\ndsh web: " + self.launch + "\n")

    def tearDown(self):
        self.temp.cleanup()

    def env(self):
        return {
            "HOME": str(self.home), "DSH_HOME": str(self.home),
            "DSH_AGENTS_HOME": str(self.home / "agents"),
            "DSH_TELEMETRY_DISABLED": "1",
            "AGENTKIT_SYNTHETIC_KEY": "synthetic-secret",
        }

    def invoke(self, **kwargs):
        return owned_dsh(
            self.runtime, self.data, self.home, self.cwd, "http://127.0.0.1:43124/v1",
            self.env(), opener_factory=lambda handler: self.opener,
            popen_factory=lambda *args, **options: self.process, **kwargs,
        )

    def test_prepares_owned_patch_and_yields_compatible_connector_config(self):
        with self.invoke(query_sqlite=True) as fixture:
            self.assertEqual(fixture.version, "0.2.1-rc.2")
            self.assertEqual(fixture.url, "http://127.0.0.1:43123/")
            self.assertEqual(fixture.cookie, "dsh-session=private-cookie")
            config = fixture.connector_config([{"op": "snapshot"}])
            self.assertEqual(config, {
                "kind": "dsh", "url": "http://127.0.0.1:43123/",
                "cookie": "dsh-session=private-cookie", "expectedVersion": "0.2.1-rc.2",
                "commands": [{"op": "snapshot"}],
            })
            patch_value = json.loads(fixture.patch_file.read_text())
            self.assertIn({"id": "session-query-sqlite", "config": {"path": ":memory:", "openAt": "first-search"}}, patch_value)
            self.assertEqual(patch_value[-1], {"insert": [{"name": fixture.plugin_file.as_uri()}]})
            self.assertIn("getDshRuntimeVersion", fixture.plugin_file.read_text())
            self.assertTrue(fixture.sessions_root.is_absolute())
            self.assertEqual(self.opener.url, self.launch)
        self.assertEqual(self.process.returncode, 0)

    def test_default_patch_preserves_public_query_plugin_default(self):
        with self.invoke() as fixture:
            patch_value = json.loads(fixture.patch_file.read_text())
            self.assertFalse(any(row.get("id") == "session-query-sqlite" for row in patch_value))

    def test_readiness_and_connector_logs_redact_launch_and_credentials(self):
        with self.invoke() as fixture:
            tail = fixture.diagnostic_tail()
            self.assertNotIn("launch-secret", tail)
            self.assertNotIn("synthetic-secret", tail)
            self.assertIn("[REDACTED]", tail)
            self.assertNotIn("private-cookie", _redact("Cookie: private-cookie", ["private-cookie"]))
            self.assertNotIn("loose-cookie", _redact("dsh-session=loose-cookie; Path=/"))

    def test_explicit_sensitive_environment_values_are_redacted_from_child_logs(self):
        self.process = _Process("runtime credential API_SECRET=private-env-secret\n" +
                                "dsh web: " + self.launch + "\n")
        env = self.env()
        env["API_SECRET"] = "private-env-secret"
        with owned_dsh(
            self.runtime, self.data, self.home, self.cwd, "http://127.0.0.1:43124/v1",
            env, opener_factory=lambda handler: self.opener,
            popen_factory=lambda *args, **options: self.process,
        ) as fixture:
            self.assertNotIn("private-env-secret", fixture.diagnostic_tail())

    def test_environment_must_be_explicit_and_is_not_ambient(self):
        self.assertNotIn("OPENAI_API_KEY", self.env())
        bad = self.env()
        del bad["AGENTKIT_SYNTHETIC_KEY"]
        with self.assertRaisesRegex(ValueError, "explicitly provide"):
            with owned_dsh(self.runtime, self.data, self.home, self.cwd,
                           "http://127.0.0.1:43124", bad):
                pass
        self.assertFalse(self.data.exists())
        self.assertFalse(self.home.exists())

    def test_rejects_non_loopback_or_credential_bearing_provider(self):
        for url in ("https://127.0.0.1:43124", "http://example.com", "http://user@127.0.0.1"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                with owned_dsh(self.runtime, self.data, self.home, self.cwd, url, self.env()):
                    pass
            self.assertFalse(self.data.exists())
            self.assertFalse(self.home.exists())

    def test_fresh_paths_are_required_and_existing_data_is_preserved(self):
        self.data.mkdir()
        sentinel = self.data / "keep"
        sentinel.write_text("preserve", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "must be absent"):
            with self.invoke():
                pass
        self.assertEqual(sentinel.read_text(), "preserve")
        self.assertFalse(self.home.exists())

    def test_owned_directories_must_not_modify_the_readonly_runtime(self):
        with self.assertRaisesRegex(ValueError, "outside root"):
            with owned_dsh(self.runtime, self.runtime / "fixture-data", self.home,
                           self.cwd, "http://127.0.0.1:43124", self.env()):
                pass
        self.assertFalse((self.runtime / "fixture-data").exists())

    def test_non_loopback_readiness_is_rejected_without_logging_secret_url(self):
        self.process = _Process("dsh web: http://192.0.2.2/?token=should-not-leak\n")
        with self.assertRaisesRegex(RuntimeError, "not loopback"):
            with self.invoke():
                pass


if __name__ == "__main__":
    unittest.main()
