#!/usr/bin/env python3
"""Check TekesWebUI's pinned AgentKit DTOs and JSONL bridge against this checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

TYPE_IMPORT = re.compile(
    r"import\s+type\s*\{(?P<names>[^}]+)}\s*from\s*['\"]@tekesagentkit/client-contract['\"]",
    re.S,
)
EXPORT = re.compile(r"export\s+(?:declare\s+)?(?:interface|type|const)\s+(\w+)")

def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)

def checked(path: Path) -> str:
    try:
        return path.read_text()
    except OSError as error:
        fail(f"cannot read {path}: {error}")

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agentkit", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--webui", type=Path, default=Path("/Users/frankqin/dev/TekesWebUI"))
    parser.add_argument("--base-commit", default="1dcc52153ea45ba6ce73a135a8d8fe8d3406360c")
    args = parser.parse_args()
    agentkit, webui = args.agentkit.resolve(), args.webui.resolve()

    package = json.loads(checked(webui / "package.json"))
    lock = json.loads(checked(webui / "package-lock.json"))
    pin_url = package.get("dependencies", {}).get("@tekesagentkit/client-contract", "")
    pin_match = re.search(r"/tar\.gz/([0-9a-f]{40})$", pin_url)
    if pin_match is None:
        fail("WebUI package.json must pin an immutable 40-character contract commit")
    pin = pin_match.group(1)
    lock_entry = lock.get("packages", {}).get("node_modules/@tekesagentkit/client-contract", {})
    if lock_entry.get("resolved") != pin_url:
        fail("WebUI package-lock resolved URL differs from package.json")
    installed = json.loads(checked(webui / "node_modules/@tekesagentkit/client-contract/package.json"))
    declaration = checked(webui / "node_modules/@tekesagentkit/client-contract/web-contract/index.d.ts")
    try:
        published = subprocess.check_output(
            ["git", "show", f"{pin}:web-contract/index.d.ts"], cwd=agentkit,
            stderr=subprocess.PIPE,
        ).decode()
    except subprocess.CalledProcessError:
        fail(f"public contract commit {pin} is unavailable locally; fetch it before checking")
    if declaration != published:
        fail("installed declarations differ from immutable public pin (development override); "
             f"installed SHA {hashlib.sha256(declaration.encode()).hexdigest()}, "
             f"public SHA {hashlib.sha256(published.encode()).hexdigest()}")
    exports = set(EXPORT.findall(declaration))
    used: set[str] = set()
    for source in (webui / "src").rglob("*.ts*"):
        text = source.read_text()
        for match in TYPE_IMPORT.finditer(text):
            used.update(re.findall(r"\b[A-Z][A-Za-z0-9]*\b", match.group("names")))
    missing = sorted(used - exports)
    if missing:
        fail("pinned public contract lacks imported WebUI declarations: " + ", ".join(missing))

    bridge = checked(webui / "host/bridge.ts")
    api = checked(webui / "src/api.ts")
    http = checked(webui / "host/http.ts")
    stdio = checked(agentkit / "crates/web/src/stdio.rs")
    service = checked(agentkit / "crates/web/src/lib.rs")
    operations = {"endpoints", "describe", "snapshot", "call", "follow", "history", "respond", "earlier"}
    missing_http = sorted(op for op in operations if f"'{op}'" not in http)
    if missing_http:
        fail("WebUI HTTP bridge no longer accepts service operations: " + ", ".join(missing_http))
    missing_service = sorted(
        op for op in operations
        if (f'"{op}"' not in service if op != "follow" else "pub async fn follow" not in service)
    )
    if missing_service:
        fail("AgentKit service does not implement WebUI operations: " + ", ".join(missing_service))
    for token, source_name, source in [
        ("cancelObservation", "WebUI bridge", bridge),
        ("cancelObservation", "AgentKit host request", stdio),
        ('"value"', "AgentKit value envelope", stdio),
        ('"error"', "AgentKit error envelope", stdio),
        ('"done"', "AgentKit terminal envelope", stdio),
    ]:
        if token not in source:
            fail(f"{source_name} is missing wire element {token}")
    for field in ("id", "op"):
        if field not in bridge or field not in stdio:
            fail(f"transport correlation field {field!r} is missing")
    if "rpcId" not in api or 'string(request, "rpcId")?' not in service:
        fail("stable caller-owned mutation rpcId is not preserved through the service")

    try:
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", args.base_commit, "HEAD", "--", "crates/web", "crates/contract/src", "crates/presentation/src"],
            cwd=agentkit, text=True, stderr=subprocess.PIPE,
        ).splitlines()
    except subprocess.CalledProcessError as error:
        fail(f"cannot compare base commit {args.base_commit}: {error.stderr.strip()}")
    changed_web = [name for name in changed if name.startswith("crates/web/")]
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=agentkit, text=True).strip()
    print(f"PASS: installed contract matches public pin {pin} ({installed.get('version')}); {len(used)} consumed declarations exist")
    print("PASS: HTTP operations and JSONL request/response/cancellation envelopes match AgentKit")
    print(f"Compared AgentKit {args.base_commit[:12]}..{head[:12]}; changed web files: {', '.join(changed_web) if changed_web else 'none'}")
    if "artifactReference" not in declaration and "artifact_reference" in checked(agentkit / "crates/contract/src/annotations.rs"):
        print("NOTE: candidate adds optional artifactReference to ComposerAnnotation; WebUI does not import or send it")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
