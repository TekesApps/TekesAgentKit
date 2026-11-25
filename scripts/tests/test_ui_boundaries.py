import shutil
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
check_ui = __import__("check-ui-boundaries")


def copy_ui_metadata_tree(dst_root: Path) -> None:
    (dst_root / "Cargo.toml").write_text((ROOT / "Cargo.toml").read_text())
    dst_ui = dst_root / "ui"
    dst_ui.mkdir()
    for name in ("Cargo.toml", "tekesui-source.json"):
        (dst_ui / name).write_text((ROOT / "ui" / name).read_text())
    for member in check_ui.EXPECTED_MEMBERS:
        src_member = ROOT / "ui" / member
        dst_member = dst_ui / member
        dst_member.mkdir(parents=True)
        (dst_member / "Cargo.toml").write_text((src_member / "Cargo.toml").read_text())
        if (src_member / "src").exists():
            shutil.copytree(src_member / "src", dst_member / "src")


class UiBoundariesTest(unittest.TestCase):
    def test_repo_ui_boundaries_pass(self):
        check_ui.check_ui_boundaries(ROOT)

    def test_detects_missing_unsafe_forbid(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_root = Path(tmp)
            copy_ui_metadata_tree(tmp_root)
            ui_toml = tmp_root / "ui/Cargo.toml"
            ui_toml.write_text(ui_toml.read_text().replace('unsafe_code = "forbid"', 'unsafe_code = "warn"'))
            with self.assertRaises(AssertionError):
                check_ui.check_ui_boundaries(tmp_root)


if __name__ == "__main__":
    unittest.main()
