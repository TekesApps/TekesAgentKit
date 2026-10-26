"""Fail-closed lifecycle receipts for the owned #55/#56 performance fixtures.

Receipts record only executable/configuration digests, process identity, and
opaque file inventory. They do not inspect runtime databases or establish
protocol behavior; callers must prove behavior through the public endpoints.
"""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from urllib.parse import urlsplit


PERFORMANCE_ROOT = Path(
    "/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/"
    "stage-close-20261025/performance"
)
IDENTITY_NAME = ".agentkit-performance-identity.json"
RECEIPT_NAME = ".agentkit-performance-closed-seed.json"
_FORMAT = "agentkit-owned-performance-seed/1"
_CLEAN_ENV = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C", "LANG": "C"}


def _json_bytes(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _canonical_existing(path, label):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError(label + " must be absolute")
    cursor = Path(path.anchor)
    for component in path.parts[1:]:
        cursor = cursor / component
        if cursor.is_symlink():
            raise ValueError(label + " may not contain symlinks")
    resolved = path.resolve(strict=True)
    if resolved != path:
        raise ValueError(label + " must be canonical")
    return resolved


def require_canonical_path(path, label):
    """Public fixture guard for an explicitly supplied existing path."""
    return _canonical_existing(path, label)


def require_performance_case(path):
    """Check a performance case against the fixed SSD root without writing."""
    return _case_root(path)


def _case_root(value):
    root = _canonical_existing(value, "performance_case")
    allowed = Path(PERFORMANCE_ROOT)
    allowed = _canonical_existing(allowed, "performance root")
    if root == allowed or allowed not in root.parents:
        raise ValueError("performance_case must be a named child of the SSD performance root")
    if len(root.relative_to(allowed).parts) != 1:
        raise ValueError("performance_case must be one direct case directory")
    return root


def _owned_roots(case, values):
    result = {}
    for label, value in values.items():
        path = Path(value)
        if not path.is_absolute():
            raise ValueError(label + " must be absolute")
        cursor = Path(path.anchor)
        for component in path.parts[1:]:
            cursor = cursor / component
            if cursor.is_symlink():
                raise ValueError(label + " may not contain symlinks")
        path = path.resolve(strict=False)
        if Path(value) != path:
            raise ValueError(label + " must be canonical")
        if path == case or case not in path.parents:
            raise ValueError(label + " must be inside performance_case")
        result[label] = str(path)
    if len(set(result.values())) != len(result):
        raise ValueError("owned paths must be distinct")
    return result


def _inventory(roots):
    rows = []
    for label, raw_root in sorted(roots.items()):
        root = Path(raw_root)
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            if path.is_symlink():
                raise ValueError("owned state may not contain symlinks")
            if path.is_file():
                rows.append({
                    "root": label,
                    "path": str(path.relative_to(root)),
                    "bytes": path.stat().st_size,
                    "sha256": _sha256(path),
                })
            elif not path.is_dir():
                raise ValueError("owned state contains a non-regular file")
    return rows


def _process_start_identity(pid):
    result = subprocess.run(
        ["/bin/ps", "-p", str(pid), "-o", "lstart="],
        check=False, capture_output=True, text=True, timeout=5, env=_CLEAN_ENV,
    )
    if result.returncode != 0 or not result.stdout.strip():
        raise ValueError("owned process is not running when its identity is recorded")
    return " ".join(result.stdout.split())


def _pid_matches(pid, start_identity):
    try:
        current = _process_start_identity(pid)
    except ValueError:
        return False
    return current == start_identity


def _no_open_files(roots):
    for raw_root in roots.values():
        result = subprocess.run(
            ["/usr/sbin/lsof", "-nP", "+D", raw_root],
            check=False, capture_output=True, text=True, timeout=30, env=_CLEAN_ENV,
        )
        if result.returncode == 0:
            lines = [line for line in result.stdout.splitlines() if line.strip()]
            if len(lines) > 1:
                raise ValueError("owned state still has open files")
            if (result.stderr.strip()
                    or (lines and not lines[0].lstrip().startswith("COMMAND"))):
                raise ValueError("lsof reported a warning; refusing the performance receipt")
        elif result.returncode != 1 or result.stderr.strip() or result.stdout.strip():
            raise ValueError("lsof failed; refusing the performance receipt")


def _read_json(path, label):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError(label + " is missing or is a symlink")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(label + " is not valid JSON") from error


def _atomic_json(path, value):
    path = Path(path)
    temp = path.with_name(path.name + ".tmp")
    if temp.exists() or temp.is_symlink() or path.exists() or path.is_symlink():
        raise ValueError("performance receipt path already exists")
    descriptor = os.open(str(temp), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(_json_bytes(value))
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, path)
    path.chmod(0o600)


def _validate_public_config(config, case):
    """Accept only public synthetic-provider descriptions and loopback URLs."""
    found_endpoint = False
    forbidden = {"apikey", "token", "secret", "cookie", "password", "credential"}

    def visit(value):
        nonlocal found_endpoint
        if isinstance(value, dict):
            for key, child in value.items():
                normalized = str(key).lower().replace("_", "").replace("-", "")
                if (normalized in forbidden or
                        any(normalized.endswith(word) for word in ("token", "secret", "password", "cookie", "authorization")) or
                        ("apikey" in normalized and not normalized.endswith("env"))):
                    raise ValueError("public fixture config may not contain credential field " + str(key))
                if normalized in ("endpoint", "baseurl"):
                    found_endpoint = True
                    try:
                        parsed = urlsplit(child)
                        port = parsed.port
                    except (TypeError, ValueError) as error:
                        raise ValueError("performance fixture endpoints must be valid loopback HTTP") from error
                    if (parsed.scheme != "http" or parsed.hostname not in ("127.0.0.1", "localhost", "::1")
                            or parsed.username is not None or parsed.password is not None
                            or (port is not None and not 1 <= port <= 65535)):
                        raise ValueError("performance fixture endpoints must be loopback HTTP")
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(config)
    if not found_endpoint:
        raise ValueError("performance fixture config must declare its loopback provider")
    if isinstance(config, dict) and "cwd" in config:
        cwd = _canonical_existing(config["cwd"], "fixture cwd")
        if cwd == case or case not in cwd.parents:
            raise ValueError("performance fixture cwd must be inside performance_case")


def begin_fresh(*, performance_case, fixture_kind, owned_paths,
                runtime_files, public_config):
    """Create a no-secret identity marker for a fresh performance fixture."""
    case = _case_root(performance_case)
    owned = _owned_roots(case, owned_paths)
    _validate_public_config(public_config, case)
    files = {}
    for label, value in runtime_files.items():
        path = _canonical_existing(value, label)
        if not path.is_file():
            raise ValueError(label + " must be a regular file")
        files[label] = {"path": str(path), "sha256": _sha256(path)}
    marker_path = case / IDENTITY_NAME
    if marker_path.exists() or marker_path.is_symlink():
        raise ValueError("performance identity marker already exists")
    marker = {
        "format": _FORMAT,
        "fixtureKind": fixture_kind,
        "performanceCase": str(case),
        "ownedPaths": owned,
        "runtimeFiles": files,
        "publicConfigSha256": hashlib.sha256(_json_bytes(public_config)).hexdigest(),
        "providerTransport": "loopback-only",
        "credentialMode": "synthetic-only",
        "createdAtUnix": int(time.time()),
    }
    _atomic_json(marker_path, marker)
    return marker_path


def record_started(marker_path, process):
    """Attach the exact owned child PID/start identity after Popen returns."""
    marker_path = Path(marker_path)
    marker = _read_json(marker_path, "performance identity marker")
    if "ownedProcess" in marker:
        raise ValueError("performance identity already has a process identity")
    marker["ownedProcess"] = {
        "pid": int(process.pid),
        "startIdentity": _process_start_identity(process.pid),
    }
    # Replace the marker atomically after ensuring no caller substituted it.
    temp = marker_path.with_name(marker_path.name + ".tmp")
    descriptor = os.open(str(temp), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(_json_bytes(marker))
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, marker_path)
    return marker


def seal_closed_seed_receipt(marker_path, *, exit_code, known_processes=None,
                             _process_probe=None, _open_files_probe=None):
    """Seal the seed lifecycle only after all known owned PIDs and files close."""
    marker_path = Path(marker_path)
    marker = _read_json(marker_path, "performance identity marker")
    if marker.get("format") != _FORMAT or "ownedProcess" not in marker:
        raise ValueError("performance identity marker has no owned process")
    if exit_code != 0:
        raise ValueError("seed process must exit successfully")
    root_process = marker["ownedProcess"]
    if known_processes is None:
        checked_processes = [{
            "pid": root_process["pid"],
            "startIdentity": root_process["startIdentity"],
            "role": "owned-root",
        }]
    else:
        if not isinstance(known_processes, list) or not known_processes:
            raise ValueError("known_processes must be a non-empty list")
        checked_processes = []
        for item in known_processes:
            if not isinstance(item, dict):
                raise ValueError("known_processes entries must be mappings")
            pid = item.get("pid")
            start_identity = item.get("startIdentity")
            role = item.get("role", "owned-child")
            if not isinstance(pid, int) or pid <= 0 or not isinstance(start_identity, str) or not start_identity:
                raise ValueError("known_processes requires positive PIDs and start identities")
            checked_processes.append({"pid": pid, "startIdentity": start_identity, "role": str(role)})
        if not any(item["pid"] == root_process["pid"]
                   and item["startIdentity"] == root_process["startIdentity"]
                   for item in checked_processes):
            raise ValueError("known_processes must include the owned fixture root identity")
    identities = {(item["pid"], item["startIdentity"]) for item in checked_processes}
    if len(identities) != len(checked_processes):
        raise ValueError("known_processes contains duplicate identities")
    for item in checked_processes:
        is_running = _process_probe(item["pid"], item["startIdentity"]) if _process_probe else _pid_matches(item["pid"], item["startIdentity"])
        if is_running:
            raise ValueError("owned seed process identity is still running")
    roots = marker["ownedPaths"]
    if _open_files_probe:
        if not _open_files_probe(roots):
            raise ValueError("owned state still has open files or lsof failed")
    else:
        _no_open_files(roots)
    receipt = {
        "format": _FORMAT,
        "identityMarkerSha256": _sha256(marker_path),
        "fixtureKind": marker["fixtureKind"],
        "performanceCase": marker["performanceCase"],
        "ownedPaths": roots,
        "runtimeFiles": marker["runtimeFiles"],
        "publicConfigSha256": marker["publicConfigSha256"],
        "providerTransport": marker["providerTransport"],
        "credentialMode": marker["credentialMode"],
        "closedSeedProcesses": checked_processes,
        "exitCode": 0,
        "opaqueStateInventory": _inventory(roots),
        "sealedAtUnix": int(time.time()),
    }
    receipt_path = marker_path.parent / RECEIPT_NAME
    _atomic_json(receipt_path, receipt)
    return receipt_path


def validate_resume(*, performance_case, resume_receipt, fixture_kind,
                    owned_paths, runtime_files, public_config,
                    process_probe=None,
                    open_files_probe=None):
    """Validate a closed seed receipt and unchanged owned performance inputs."""
    case = _case_root(performance_case)
    receipt_path = _canonical_existing(resume_receipt, "resume_receipt")
    if receipt_path != case / RECEIPT_NAME:
        raise ValueError("resume_receipt must be the closed-seed receipt in performance_case")
    marker_path = case / IDENTITY_NAME
    marker = _read_json(marker_path, "performance identity marker")
    receipt = _read_json(receipt_path, "closed-seed receipt")
    if (receipt.get("format") != _FORMAT or receipt.get("exitCode") != 0
            or receipt.get("identityMarkerSha256") != _sha256(marker_path)):
        raise ValueError("closed-seed receipt does not match its identity marker")
    owned = _owned_roots(case, owned_paths)
    _validate_public_config(public_config, case)
    files = {}
    for label, value in runtime_files.items():
        path = _canonical_existing(value, label)
        if not path.is_file():
            raise ValueError(label + " must be a regular file")
        files[label] = {"path": str(path), "sha256": _sha256(path)}
    config_hash = hashlib.sha256(_json_bytes(public_config)).hexdigest()
    expected = (fixture_kind, str(case), owned, files, config_hash)
    recorded = (receipt.get("fixtureKind"), receipt.get("performanceCase"),
                receipt.get("ownedPaths"), receipt.get("runtimeFiles"),
                receipt.get("publicConfigSha256"))
    if expected != recorded or marker.get("fixtureKind") != fixture_kind:
        raise ValueError("closed-seed receipt does not match runtime, config, or owned paths")
    if receipt.get("providerTransport") != "loopback-only" or receipt.get("credentialMode") != "synthetic-only":
        raise ValueError("closed-seed receipt is not synthetic loopback-only")
    processes = receipt.get("closedSeedProcesses")
    if not isinstance(processes, list) or not processes:
        raise ValueError("closed-seed receipt lacks checked process identities")
    root_process = marker.get("ownedProcess", {})
    if not any(item.get("pid") == root_process.get("pid")
               and item.get("startIdentity") == root_process.get("startIdentity")
               for item in processes if isinstance(item, dict)):
        raise ValueError("closed-seed receipt omits the owned fixture root identity")
    for process in processes:
        is_running = process_probe(process["pid"], process["startIdentity"]) if process_probe else _pid_matches(process["pid"], process["startIdentity"])
        if is_running:
            raise ValueError("seed process identity is still running")
    if open_files_probe:
        if not open_files_probe(owned):
            raise ValueError("owned state still has open files or lsof failed")
    else:
        _no_open_files(owned)
    if _inventory(owned) != receipt.get("opaqueStateInventory"):
        raise ValueError("owned state changed after the seed was closed")
    return marker_path
