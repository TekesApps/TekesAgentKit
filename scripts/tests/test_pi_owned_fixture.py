import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pi_owned_fixture import prepare_pi


class PiOwnedFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.pi_root = self.base / "pi-runtime"
        (self.pi_root / "node/bin").mkdir(parents=True)
        (self.pi_root / "node/bin/node").write_text("fake node entry")
        cli = self.pi_root / "node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js"
        cli.parent.mkdir(parents=True)
        cli.write_text("fake Pi entry")

    def tearDown(self):
        self.temp.cleanup()

    def paths(self):
        return self.base / "owned-data", self.base / "owned-home"

    def test_default_configuration_matches_headless_connector_and_model_contract(self):
        data, home = self.paths()
        with patch.dict(os.environ, {"PATH": "/controlled-by-caller/bin"}):
            result = prepare_pi(self.pi_root, data, home, "http://127.0.0.1:43123/v1")

        self.assertEqual(result, {
            "kind": "pi",
            "executable": str(self.pi_root.resolve() / "node/bin/node"),
            "arguments": [str(self.pi_root.resolve() / "node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js")],
            "dataRoot": str(data),
            "environment": {"HOME": str(home), "PATH": "/controlled-by-caller/bin"},
        })
        config = json.loads((data / "agent/models.json").read_text())
        self.assertEqual(config, {"providers": {"endpoint-test": {
            "baseUrl": "http://127.0.0.1:43123/v1",
            "api": "openai-completions",
            "apiKey": "synthetic-key",
            "models": [{
                "id": "endpoint-test", "name": "Endpoint test",
                "contextWindow": 32768, "maxTokens": 1024,
                "input": ["text", "image"],
            }],
        }}})
        self.assertEqual(stat.S_IMODE(data.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((data / "agent").stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(home.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((data / "agent/models.json").stat().st_mode), 0o600)

    def test_reasoning_is_added_only_when_explicitly_enabled(self):
        data, home = self.paths()
        prepare_pi(self.pi_root, data, home, "http://localhost:43123", reasoning=True)
        config = json.loads((data / "agent/models.json").read_text())
        model = config["providers"]["endpoint-test"]["models"][0]
        self.assertIs(model["reasoning"], True)

    def test_strict_environment_contains_only_controlled_runtime_values(self):
        data, home = self.paths()
        with patch.dict(os.environ, {"PATH": "/user/path", "OPENAI_API_KEY": "must-not-leak"}):
            result = prepare_pi(self.pi_root, data, home, "http://[::1]:43123/v1", strict_environment=True)
        environment = result["environment"]
        self.assertEqual(set(environment), {"HOME", "PATH", "TMPDIR"})
        self.assertEqual(environment["HOME"], str(home.absolute()))
        self.assertEqual(environment["TMPDIR"], str(home.absolute()))
        self.assertEqual(environment["PATH"].split(os.pathsep), [
            str(self.pi_root.resolve() / "node/bin"), "/usr/bin", "/bin", "/usr/sbin", "/sbin",
        ])
        self.assertNotIn("must-not-leak", repr(environment))

    def test_default_path_fallback_matches_existing_connector(self):
        data, home = self.paths()
        with patch.dict(os.environ, {}, clear=True):
            result = prepare_pi(self.pi_root, data, home, "http://127.0.0.1")
        self.assertEqual(result["environment"]["PATH"], "/usr/bin:/bin")

    def test_existing_empty_directories_are_supported_but_nonempty_are_rejected(self):
        data, home = self.paths()
        data.mkdir()
        home.mkdir()
        prepare_pi(self.pi_root, data, home, "http://127.0.0.1")

        second_data, second_home = self.base / "second-data", self.base / "second-home"
        second_data.mkdir()
        existing = second_data / "agent"
        existing.mkdir()
        models = existing / "models.json"
        models.write_text("preserve me")
        with self.assertRaisesRegex(ValueError, "empty"):
            prepare_pi(self.pi_root, second_data, second_home, "http://127.0.0.1")
        self.assertEqual(models.read_text(), "preserve me")
        self.assertFalse(second_home.exists())

    def test_runtime_root_and_child_paths_are_rejected(self):
        data, home = self.paths()
        with self.assertRaisesRegex(ValueError, "outside pi_root"):
            prepare_pi(self.pi_root, self.pi_root / "data", home, "http://127.0.0.1")
        data, home = self.paths()
        with self.assertRaisesRegex(ValueError, "outside pi_root"):
            prepare_pi(self.pi_root, data, self.pi_root / "home", "http://127.0.0.1")

    def test_relative_and_overlapping_owned_paths_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "absolute"):
            prepare_pi(self.pi_root, "relative-data", self.base / "home", "http://127.0.0.1")
        shared = self.base / "shared"
        with self.assertRaisesRegex(ValueError, "separate"):
            prepare_pi(self.pi_root, shared, shared, "http://127.0.0.1")
        parent = self.base / "parent"
        with self.assertRaisesRegex(ValueError, "separate"):
            prepare_pi(self.pi_root, parent, parent / "home", "http://127.0.0.1")

    def test_invalid_runtime_root_is_rejected_read_only(self):
        data, home = self.paths()
        with self.assertRaisesRegex(ValueError, "entry files"):
            prepare_pi(self.base, data, home, "http://127.0.0.1")
        self.assertFalse(data.exists())
        self.assertFalse(home.exists())

    def test_only_http_loopback_without_credentials_is_accepted(self):
        invalid = [
            "https://127.0.0.1/v1",
            "http://example.com/v1",
            "http://user@127.0.0.1/v1",
            "http://127.0.0.1:99999/v1",
            "http://0.0.0.0/v1",
        ]
        for endpoint in invalid:
            with self.subTest(endpoint=endpoint):
                data, home = self.paths()
                with self.assertRaises(ValueError):
                    prepare_pi(self.pi_root, data, home, endpoint)
                self.assertFalse(data.exists())
                self.assertFalse(home.exists())

    def test_cleanup_does_not_remove_a_directory_created_by_another_owner(self):
        data, home = self.paths()
        original_mkdir = Path.mkdir

        def competing_mkdir(path, *args, **kwargs):
            if path == data:
                original_mkdir(path, *args, **kwargs)
                raise FileExistsError("another owner won the create race")
            return original_mkdir(path, *args, **kwargs)

        with patch.object(Path, "mkdir", competing_mkdir), self.assertRaises(FileExistsError):
            prepare_pi(self.pi_root, data, home, "http://127.0.0.1")
        self.assertTrue(data.is_dir())
        self.assertFalse(home.exists())


if __name__ == "__main__":
    unittest.main()
