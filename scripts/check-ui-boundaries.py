#!/usr/bin/env python3
"""Static ownership, unsafe=forbid, and immutable TekesUI pin guard."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re

try:
    import tomllib  # type: ignore[import-not-found]
except ModuleNotFoundError:  # Python < 3.11 fallback for macOS system python3
    tomllib = None  # type: ignore[assignment]


def _parse_toml_value(raw: str):
    raw = raw.strip()
    if raw == "true":
        return True
    if raw == "false":
        return False
    if raw.startswith('"') and raw.endswith('"'):
        return raw[1:-1]
    if raw.startswith("[") and raw.endswith("]"):
        inner = raw[1:-1].strip()
        if not inner:
            return []
        return [_parse_toml_value(part) for part in _split_top_level(inner)]
    if raw.startswith("{") and raw.endswith("}"):
        inner = raw[1:-1].strip()
        obj = {}
        if not inner:
            return obj
        for part in _split_top_level(inner):
            key, val = part.split("=", 1)
            obj[key.strip()] = _parse_toml_value(val)
        return obj
    if re.fullmatch(r"-?\d+", raw):
        return int(raw)
    return raw


def _split_top_level(text: str) -> list[str]:
    parts: list[str] = []
    cur: list[str] = []
    depth = 0
    in_str = False
    escape = False
    for ch in text:
        if escape:
            cur.append(ch)
            escape = False
            continue
        if ch == "\\" and in_str:
            cur.append(ch)
            escape = True
            continue
        if ch == '"':
            in_str = not in_str
            cur.append(ch)
            continue
        if not in_str:
            if ch in "[{":
                depth += 1
            elif ch in "]}":
                depth -= 1
            elif ch == "," and depth == 0:
                item = "".join(cur).strip()
                if item:
                    parts.append(item)
                cur = []
                continue
        cur.append(ch)
    tail = "".join(cur).strip()
    if tail:
        parts.append(tail)
    return parts


def load_toml(text: str) -> dict:
    if tomllib is not None:
        return tomllib.loads(text)
    doc: dict = {}
    section = doc
    lines = text.splitlines()
    idx = 0
    while idx < len(lines):
        line = lines[idx].strip()
        idx += 1
        if not line or line.startswith("#"):
            continue
        # Strip inline comments outside quotes
        in_str = False
        cleaned = []
        for ch in line:
            if ch == '"':
                in_str = not in_str
            if ch == "#" and not in_str:
                break
            cleaned.append(ch)
        line = "".join(cleaned).strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            header = line[1:-1].strip()
            section = doc
            for part in header.split("."):
                section = section.setdefault(part.strip(), {})
            continue
        if "=" in line:
            key, val = line.split("=", 1)
            val = val.strip()
            while val.startswith("[") and not val.endswith("]") and idx < len(lines):
                nxt = lines[idx].strip()
                idx += 1
                if nxt.startswith("#"):
                    continue
                val += " " + nxt
            target = section
            key_parts = [k.strip() for k in key.split(".")]
            for part in key_parts[:-1]:
                target = target.setdefault(part, {})
            target[key_parts[-1]] = _parse_toml_value(val)
    return doc


EXPECTED_MEMBERS = ["components", "bindings", "apphost", "demo", "consumer"]

EXPECTED_DEPS = {
    "components": {
        "tekes-agent-contract",
        "tekes-agent-presentation",
        "tekes-ui-controls",
        "tekes-ui-scene",
        "tekes-ui-tokens",
        "tekes-ui-text",
        "tekes-ui-motion",
        "unicode-segmentation",
    },
    "bindings": {
        "tekes-agent-components-ui",
        "tekes-agent-contract",
        "tekes-agent-presentation",
        "tekes-agent-session",
        "tekes-agent-web",
        "async-trait",
        "base64",
        "cap-std",
        "futures-util",
        "image",
        "serde",
        "serde_json",
        "sha2",
        "tokio",
        "uuid",
    },
    "apphost": {
        "anyhow",
        "serde",
        "serde_json",
        "tokio",
        "tekes-agent-components-ui",
        "tekes-agent-contract",
        "tekes-agent-presentation",
        "tekes-agent-session",
        "tekes-agent-ui-bindings",
        "tekes-ui-app-host",
        "tekes-ui-app-protocol",
        "tekes-ui-controls",
        "tekes-ui-icons",
        "tekes-ui-platform-macos",
        "tekes-ui-render",
        "tekes-ui-scene",
        "tekes-ui-settings",
        "tekes-ui-text",
        "tekes-ui-tokens",
    },
    "consumer": {
        "anyhow",
        "serde_json",
        "tokio",
        "tekes-agent-components-ui",
        "tekes-agent-contract",
        "tekes-agent-presentation",
        "tekes-agent-session",
        "tekes-agent-ui-bindings",
        "tekes-agent-apphost",
        "tekes-ui-app-host",
        "tekes-ui-app-protocol",
        "tekes-ui-controls",
        "tekes-ui-platform-macos",
        "tekes-ui-render",
        "tekes-ui-scene",
        "tekes-ui-settings",
        "tekes-ui-tokens",
    },
    "demo": {
        "anyhow",
        "async-trait",
        "futures-util",
        "png",
        "serde_json",
        "tokio",
        "tekes-agent-components-ui",
        "tekes-agent-ui-bindings",
        "tekes-agent-apphost",
        "tekes-agent-contract",
        "tekes-agent-endpoint-kernel",
        "tekes-agent-presentation",
        "tekes-agent-session",
        "tekes-ui-app-host",
        "tekes-ui-app-protocol",
        "tekes-ui-controls",
        "tekes-ui-platform-macos",
        "tekes-ui-render",
        "tekes-ui-scene",
        "tekes-ui-settings",
        "tekes-ui-text",
        "tekes-ui-tokens",
    },
}

FORBIDDEN_UI_TOKENS = ("objc2", "tekes_kernel", "tekes_ui_gallery", "tekes_agent_ui_demo")


def check_ui_boundaries(root: Path) -> None:
    root = root.resolve(strict=True)
    ui = root / "ui"
    root_toml = load_toml((root / "Cargo.toml").read_text())
    assert "ui" in root_toml["workspace"]["exclude"], "ui must remain excluded from root workspace"

    ui_toml = load_toml((ui / "Cargo.toml").read_text())
    workspace = ui_toml["workspace"]
    assert workspace["members"] == EXPECTED_MEMBERS, f"unexpected ui members: {workspace['members']}"
    assert workspace["package"]["publish"] is False
    assert workspace["package"]["version"] == "0.0.0"
    assert workspace["lints"]["rust"]["unsafe_code"] == "forbid", "ui workspace must forbid unsafe_code"

    lock = json.loads((ui / "tekesui-source.json").read_text())
    assert lock["kind"] == "published-git-revision", "ui must use published-git-revision pin"
    assert lock["repository"] == "https://github.com/TekesApps/TekesUI.git"
    assert re.fullmatch(r"[0-9a-f]{40}", lock["rev"]), "invalid TekesUI commit SHA"

    for name, dependency in workspace["dependencies"].items():
        if name.startswith("tekes-ui-"):
            assert dependency.get("git") == lock["repository"], f"{name} git mismatch"
            assert dependency.get("rev") == lock["rev"], f"{name} rev mismatch"
            assert "path" not in dependency and "branch" not in dependency, f"{name} must not use path/branch"

    for member in EXPECTED_MEMBERS:
        manifest = load_toml((ui / member / "Cargo.toml").read_text())
        assert manifest["package"]["publish"]["workspace"] is True
        assert manifest["lints"]["workspace"] is True
        deps = manifest.get("dependencies", {})
        assert set(deps) == EXPECTED_DEPS[member], (
            f"{member} deps mismatch: actual={sorted(deps)} expected={sorted(EXPECTED_DEPS[member])}"
        )
        for dep_name, dep_val in deps.items():
            if isinstance(dep_val, dict):
                assert not {"path", "git"}.intersection(dep_val), f"{member}.{dep_name} bypasses workspace"
                if dep_name == "tekes-ui-platform-macos" and "selftest" in dep_val.get("features", []):
                    assert member == "demo", f"selftest feature forbidden in {member}"

    # Strict static prohibition on private/platform/harness imports in reusable crates & second consumer
    for member in ("components", "bindings", "apphost", "consumer"):
        for folder in ("src", "tests", "examples"):
            base = ui / member / folder
            if not base.exists():
                continue
            for source in base.rglob("*.rs"):
                text = source.read_text()
                for token in FORBIDDEN_UI_TOKENS:
                    assert token not in text, f"forbidden token {token!r} in {source}"
    # Also ensure components and bindings remain completely free of AppHost/Platform/Renderer types
    for member in ("components", "bindings"):
        for folder in ("src", "tests"):
            base = ui / member / folder
            if not base.exists():
                continue
            for source in base.rglob("*.rs"):
                text = source.read_text()
                for token in ("tekes_ui_app_host", "tekes_ui_platform_macos", "tekes_ui_render", "coretext"):
                    assert token not in text, f"forbidden platform/apphost token {token!r} in {source}"

    # No unsafe or Swift anywhere under ui/
    for source in ui.rglob("*.rs"):
        if ".deps" in source.parts or "target" in source.parts:
            continue
        assert "unsafe " not in source.read_text() and "unsafe{" not in source.read_text(), source
    assert not [p for p in ui.rglob("*.swift") if ".deps" not in p.parts]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    check_ui_boundaries(args.root)
    tag = json.loads((args.root / "ui/tekesui-source.json").read_text())["tag"]
    print(f"UI_BOUNDARIES_OK static-only; TekesUI {tag} pin and five-crate AppHost boundaries verified")


if __name__ == "__main__":
    main()
