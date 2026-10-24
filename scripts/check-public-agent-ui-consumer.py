#!/usr/bin/env python3
"""Prepare a clean public-API consumer fixture without invoking Cargo or Rust."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys


PIN = "50ec49f3446924fdbc66b3cff22cd8cfe150fe7c"
FIXTURE_ROOT = Path(".runtime-fixtures/issue15-contract-20261024")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def locked_tekesui_packages(cargo_lock: str, revision: str) -> list[str]:
    packages = []
    for block in cargo_lock.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"$', block, re.MULTILINE)
        version = re.search(r'^version = "([^"]+)"$', block, re.MULTILINE)
        source = re.search(r'^source = "([^"]+)"$', block, re.MULTILINE)
        if (
            name
            and version
            and source
            and source.group(1).startswith(
                f"git+https://github.com/TekesApps/TekesUI.git?rev={revision}#"
            )
        ):
            packages.append(f"{name.group(1)}@{version.group(1)}")
    return sorted(packages)


def source_fingerprint(root: Path) -> tuple[str, list[str], str]:
    paths = [root / "Cargo.toml", root / "ui/Cargo.toml",
             root / "ui/components/Cargo.toml", root / "crates/contract/Cargo.toml",
             root / "crates/presentation/Cargo.toml", root / "crates/session/Cargo.toml",
             root / "ui/Cargo.lock",
             root / "ui/tekesui-source.json"]
    for directory in (root / "ui/components/src", root / "crates/contract/src",
                      root / "crates/presentation/src", root / "crates/session/src"):
        paths.extend(directory.rglob("*.rs"))
    paths = sorted(set(path.resolve() for path in paths))
    digest = hashlib.sha256()
    names = []
    for path in paths:
        relative = path.relative_to(root.resolve()).as_posix()
        names.append(relative)
        digest.update(relative.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    status = subprocess.check_output(
        ["git", "-C", str(root), "status", "--porcelain", "--untracked-files=all", "--",
         "ui/components", "crates/contract", "crates/presentation", "ui/Cargo.lock",
         "crates/session", "ui/tekesui-source.json", "Cargo.toml", "ui/Cargo.toml"], text=True
    ).strip()
    return digest.hexdigest(), names, status


def write_unchanged_or_new(path: Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        if path.read_text() != value:
            raise RuntimeError(f"refusing to overwrite different existing file: {path}")
        return
    path.write_text(value)


def prepare(root: Path, output: Path) -> dict[str, object]:
    root = root.resolve(strict=True)
    ui = root / "ui"
    workspace_manifest = (ui / "Cargo.toml").read_text()
    tekesui = json.loads((ui / "tekesui-source.json").read_text())
    if not re.search(r'^version = "0\.0\.0"$', workspace_manifest, re.MULTILINE):
        raise RuntimeError("expected experimental AgentKit UI workspace version 0.0.0")
    if not re.search(r"^publish = false$", workspace_manifest, re.MULTILINE):
        raise RuntimeError("experimental AgentKit UI workspace must remain unpublished")
    if tekesui.get("kind") != "published-git-revision" or tekesui.get("rev") != PIN:
        raise RuntimeError("TekesUI pin differs from the reviewed public revision")
    for name, dependency in re.findall(
        r'^(tekes-ui-[^ ]+) = \{ git = "[^"]+", rev = "([^"]+)" \}$',
        workspace_manifest,
        re.MULTILINE,
    ):
        if dependency != PIN:
            raise RuntimeError(f"{name} does not use the exact TekesUI revision")

    component_path = (ui / "components").resolve()
    manifest = (
        "[workspace]\n\n"
        "[package]\n"
        'name = "agentkit-public-ui-consumer"\n'
        'version = "0.1.0"\n'
        'edition = "2024"\n'
        "publish = false\n\n"
        "[dependencies]\n"
        f"tekes-agent-components-ui = {{ path = {json.dumps(str(component_path))} }}\n"
    )
    source = '''use tekes_agent_components_ui::{
    RunPort, RunScope, SidePanel, SidePanelInput, SidePanelLabels, SidePanelTab,
    SidePanelTabKind, SideChatSnapshot, Sidebar, SidebarInput, SidebarLabels,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scope = RunScope::main("native", "standalone-consumer-session");

    let run_port = RunPort::standard(0x8100, 0..256, scope.clone())?;
    let _run_host = run_port.into_host()?;

    let mut sidebar = Sidebar::new(0x8200, SidebarLabels::default());
    sidebar.update(&SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: 1,
        loaded: true,
        error: None,
        rows: Vec::new(),
        selected: None,
    })?;
    let _sidebar_host = sidebar.into_host()?;

    let mut side_panel = SidePanel::new(0x8300, SidePanelLabels::default(), 4);
    side_panel.update(&SidePanelInput {
        revision: 1,
        parent: scope,
        projection_ready: true,
        tabs: vec![SidePanelTab::summary("Summary")],
        selected: Some(SidePanelTabKind::Summary),
        side_chats: SideChatSnapshot::default(),
        supports_side_chat: None,
    })?;
    let _side_panel_host = side_panel.into_host()?;

    Ok(())
}
'''
    source_digest, source_files, source_status = source_fingerprint(root)
    head = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if not output.is_absolute():
        output = root / output
    write_unchanged_or_new(output / "Cargo.toml", manifest)
    write_unchanged_or_new(output / "src/main.rs", source)
    seed_path = output / "Cargo.lock.seed"
    seed_value = (ui / "Cargo.lock").read_text()
    write_unchanged_or_new(seed_path, seed_value)
    cargo_lock = ui / "Cargo.lock"
    sdk_packages = locked_tekesui_packages(cargo_lock.read_text(), PIN)
    receipt: dict[str, object] = {
        "kind": "prepared-standalone-public-agent-ui-consumer",
        "status": "prepared-not-built",
        "fixture_directory": str(output.resolve()),
        "agentkit_head": head,
        "path_source_sha256": source_digest,
        "path_source_file_count": len(source_files),
        "path_source_files": source_files,
        "path_source_worktree_status": source_status,
        "path_source_matches_git_head": not bool(source_status),
        "agentkit_ui_version": "0.0.0",
        "agentkit_ui_publish": False,
        "explicit_development_path": str(component_path),
        "consumer_direct_dependencies": ["tekes-agent-components-ui"],
        "forbidden_wrappers": [
            "tekes-agent-ui-consumer-check",
            "RunSurface",
            "ComponentDemo",
        ],
        "tekesui_revision": PIN,
        "tekesui_source_manifest_sha256": sha256(ui / "tekesui-source.json"),
        "tekesui_closure_manifest_sha256": tekesui["sha256"],
        "tekesui_closure_input_count": len(tekesui["files"]),
        "tekesui_locked_packages": sdk_packages,
        "agentkit_ui_lock_sha256": sha256(cargo_lock),
        "seed_lock_sha256": sha256(seed_path),
        "consumer_manifest_sha256": hashlib.sha256(manifest.encode()).hexdigest(),
        "consumer_source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        "cargo_invoked": False,
        "target_created": False,
        "not_claimed": [
            "native-window acceptance",
            "ProcessContainer same-process or cross-process integration",
            "IME/accessibility acceptance",
            "complete ClientKit View parity",
        ],
    }
    write_unchanged_or_new(output / "prepare-receipt.json", json.dumps(receipt, indent=2) + "\n")
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    source_digest, _, _ = source_fingerprint(root)
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
    output = args.output or (root / FIXTURE_ROOT / f"external-consumer-{head[:7]}-{source_digest[:10]}")
    try:
        receipt = prepare(root, output)
    except Exception as error:
        print(f"consumer fixture preparation failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
