import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dsh_owned_fixture import _redact, owned_dsh
from performance_owned_receipt import IDENTITY_NAME, seal_closed_seed_receipt


class _Process:
    def __init__(self, output, pid=42001):
        from io import StringIO
        self.stdout = StringIO(output)
        self.pid = pid
        self.returncode = 0
        self.wait_calls = []

    def poll(self):
        return self.returncode

    def wait(self, timeout=None):
        self.wait_calls.append(timeout)
        if self.returncode is None:
            self.returncode = 0
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
        self.base = Path(self.temp.name).resolve()
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

    def test_image_model_capability_is_an_explicit_fixture_input(self):
        with self.invoke(image_input=True) as fixture:
            patch_value = json.loads(fixture.patch_file.read_text())
            provider = next(row for row in patch_value if row.get("id") == "llm-pi-ai")
            model = provider["config"]["providers"]["endpoint-test"]["models"][0]
            self.assertEqual(model["input"], ["text", "image"])

    def test_image_capability_rejects_non_boolean_before_writing_data(self):
        with self.assertRaisesRegex(ValueError, "image_input must be boolean"):
            with self.invoke(image_input="yes"):
                self.fail("invalid fixture input was accepted")
        self.assertFalse(self.data.exists())

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

    def test_performance_resume_requires_closed_receipt_and_reuses_seed_files(self):
        performance_root = self.base / "ssd-performance"
        performance_root.mkdir()
        case = performance_root / "dsh-cold5"
        case.mkdir()
        self.data = case / "dsh-data"
        self.home = case / "dsh-home"
        self.cwd = case / "workspace"
        self.cwd.mkdir()
        processes = [
            _Process("DSH seed\ndsh web: " + self.launch + "\n", 42001),
            _Process("DSH resume\ndsh web: " + self.launch + "\n", 42002),
        ]
        callbacks = []
        with patch("performance_owned_receipt._process_start_identity", return_value="fake-start"), \
                patch("performance_owned_receipt.PERFORMANCE_ROOT", performance_root):
            with owned_dsh(
                self.runtime, self.data, self.home, self.cwd,
                "http://127.0.0.1:43124/v1", self.env(),
                opener_factory=lambda handler: self.opener,
                popen_factory=lambda *args, **options: processes.pop(0),
                performance_case=case,
                on_process_started=callbacks.append,
            ) as seed:
                seeded_patch = seed.patch_file.read_bytes()
            receipt = seal_closed_seed_receipt(
                case / IDENTITY_NAME, exit_code=0,
                _process_probe=lambda *_: False, _open_files_probe=lambda _: True,
            )
            with owned_dsh(
                self.runtime, self.data, self.home, self.cwd,
                "http://127.0.0.1:43124/v1", self.env(),
                opener_factory=lambda handler: self.opener,
                popen_factory=lambda *args, **options: processes.pop(0),
                performance_case=case, resume_receipt=receipt,
                _process_probe=lambda *_: False, _open_files_probe=lambda _: True,
                on_process_started=callbacks.append,
            ) as resumed:
                self.assertEqual(resumed.patch_file.read_bytes(), seeded_patch)
        self.assertEqual(len(callbacks), 2)
        receipt_text = receipt.read_text(encoding="utf-8")
        self.assertNotIn("synthetic-secret", receipt_text)
        self.assertNotIn("private-cookie", receipt_text)

    def test_performance_runtime_symlink_binds_and_executes_canonical_node(self):
        performance_root = self.base / 'ssd-performance'
        case = performance_root / 'dsh-canonical-node'
        case.mkdir(parents=True)
        self.data, self.home = case/'data', case/'home'
        self.cwd = case/'workspace'; self.cwd.mkdir()
        real_node = self.base/'shared-node'
        real_node.write_text('immutable node fixture')
        self.node.unlink(); self.node.symlink_to(real_node)
        launched = []
        def popen(argv, **_options):
            launched.append(argv)
            return self.process
        with patch('performance_owned_receipt._process_start_identity', return_value='fake-start'), \
                patch('performance_owned_receipt.PERFORMANCE_ROOT', performance_root):
            with owned_dsh(self.runtime, self.data, self.home, self.cwd,
                    'http://127.0.0.1:43124/v1', self.env(), performance_case=case,
                    opener_factory=lambda handler: self.opener, popen_factory=popen):
                marker = json.loads((case/IDENTITY_NAME).read_text())
                self.assertEqual(marker['runtimeFiles']['node']['path'], str(real_node))
                self.assertEqual(launched[0][0], str(real_node))

    def test_process_started_callback_failure_reaps_only_owned_process(self):
        process = self.process
        process.returncode = None
        with patch("dsh_owned_fixture.os.killpg") as killpg:
            with self.assertRaisesRegex(RuntimeError, "sampler failed"):
                with owned_dsh(
                    self.runtime, self.data, self.home, self.cwd,
                    "http://127.0.0.1:43124/v1", self.env(),
                    opener_factory=lambda handler: self.opener,
                    popen_factory=lambda *args, **options: process,
                    on_process_started=lambda _process: (_ for _ in ()).throw(RuntimeError("sampler failed")),
                ):
                    self.fail("callback failure must prevent yielding")
        killpg.assert_called_once()
        self.assertEqual(process.returncode, 0)


if __name__ == "__main__":
    unittest.main()
