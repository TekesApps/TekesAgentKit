import json
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/check-public-agent-ui-consumer.py"


class PublicAgentUiConsumerTests(unittest.TestCase):
    def test_prepares_clean_explicit_path_consumer_without_building(self):
        with tempfile.TemporaryDirectory(prefix="agentkit-public-consumer-") as directory:
            output = Path(directory) / "consumer"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(ROOT), "--output", str(output)],
                check=True,
                capture_output=True,
                text=True,
            )
            receipt = json.loads(result.stdout)
            source = (output / "src/main.rs").read_text()
            manifest = (output / "Cargo.toml").read_text()
            seed = (output / "Cargo.lock.seed").read_text()

            self.assertEqual(receipt["status"], "prepared-not-built")
            self.assertFalse(receipt["cargo_invoked"])
            self.assertFalse(receipt["target_created"])
            self.assertTrue(receipt["path_source_sha256"])
            self.assertEqual(receipt["agentkit_head"], subprocess.check_output(
                ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
            ).strip())
            self.assertEqual(receipt["tekesui_revision"], "50ec49f3446924fdbc66b3cff22cd8cfe150fe7c")
            self.assertEqual(receipt["consumer_direct_dependencies"], ["tekes-agent-components-ui"])
            self.assertIn("RunPort::standard", source)
            self.assertIn("Sidebar::new", source)
            self.assertIn("SidePanel::new", source)
            self.assertEqual(source.count(".into_host()?"), 3)
            self.assertNotIn("RunSurface", source)
            self.assertNotIn("ComponentDemo", source)
            self.assertNotIn("consumer-check", manifest)
            self.assertIn("path = ", manifest)
            self.assertIn("[[package]]", seed)
            self.assertIn("[workspace]", manifest)

    def test_workspace_manifests_change_provenance_and_report_dirty_state(self):
        spec = importlib.util.spec_from_file_location("public_consumer", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _, inputs, _ = module.source_fingerprint(ROOT)
        with tempfile.TemporaryDirectory(prefix="agentkit-consumer-source-") as directory:
            root = Path(directory)
            for name in inputs:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / name).read_bytes())
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run([
                "git", "-C", str(root), "-c", "commit.gpgsign=false",
                "-c", "core.hooksPath=/dev/null", "-c", "user.name=consumer-test",
                "-c", "user.email=consumer-test@example.invalid", "commit", "-qm", "fixture",
            ], check=True)
            original, names, status = module.source_fingerprint(root)
            self.assertEqual(status, "")
            self.assertIn("Cargo.toml", names)
            self.assertIn("ui/Cargo.toml", names)
            for name in ["Cargo.toml", "ui/Cargo.toml"]:
                path = root / name
                before = path.read_bytes()
                path.write_bytes(before + b"\n# changed dependency declaration\n")
                changed, _, status = module.source_fingerprint(root)
                self.assertNotEqual(original, changed)
                self.assertIn(name, status)
                path.write_bytes(before)

    def test_refuses_overwriting_fixture_with_different_content(self):
        with tempfile.TemporaryDirectory(prefix="agentkit-public-consumer-") as directory:
            output = Path(directory) / "consumer"
            output.mkdir()
            (output / "Cargo.toml").write_text("user wip\n")
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--root", str(ROOT), "--output", str(output)],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("refusing to overwrite", result.stderr)
            self.assertEqual((output / "Cargo.toml").read_text(), "user wip\n")


if __name__ == "__main__":
    unittest.main()
