#!/usr/bin/env python3
"""Verify TekesUI source provenance against ui/tekesui-source.json without mutating TekesUI."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="Optional local TekesUI checkout to verify against pinned rev")
    args = parser.parse_args()
    lock = json.loads((ROOT / "ui/tekesui-source.json").read_text())
    if lock.get("kind") == "published-git-revision":
        if args.source is not None:
            src = args.source.resolve(strict=True)
            head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=src, text=True).strip()
            tag = subprocess.check_output(["git", "rev-parse", f"{lock['tag']}^{{commit}}"], cwd=src, text=True).strip()
            assert tag == lock["rev"], f"tag {lock['tag']} points to {tag}, expected {lock['rev']}"
            if head != lock["rev"]:
                print(f"NOTE: local TekesUI HEAD is {head}; pin requires {lock['rev']} ({lock['tag']})")
        print("TEKESUI_PIN_READY", lock["repository"], lock["tag"], lock["rev"])
        return

    # Legacy snapshot verification fallback
    assert args.source is not None, "--source is required for development-snapshot manifests"
    src = args.source.resolve(strict=True)
    digest = hashlib.sha256(json.dumps(lock["files"], sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    assert digest == lock["sha256"], "manifest digest differs"
    for name, expected in lock["files"].items():
        actual = hashlib.sha256((src / name).read_bytes()).hexdigest()
        assert actual == expected, f"source drift: {name}"
    print("TEKESUI_SNAPSHOT_READY", lock["sha256"])


if __name__ == "__main__":
    main()
