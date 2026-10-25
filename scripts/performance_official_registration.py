"""Fail-closed registration checks for an original official performance run.

This module only reads registration inputs. Call it before creating a case or
starting any subprocess. A completed diagnostic has no path through this API.
"""
from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path


SHA = re.compile(r"^[0-9a-f]{64}$")
COUNTS = {"cold": 1, "warm": 11, "reconnectLifecycle": 11}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _file(path_value: object, expected: object, label: str) -> Path:
    if not isinstance(path_value, str) or not Path(path_value).is_absolute():
        raise ValueError(f"{label} path must be absolute")
    path = Path(path_value)
    if path.is_symlink() or not path.is_file() or not isinstance(expected, str) or not SHA.fullmatch(expected):
        raise ValueError(f"{label} must be a pinned regular file")
    if sha256(path) != expected:
        raise ValueError(f"{label} SHA-256 differs from registration")
    return path


def _source_manifest(receipt: dict, expected_commit: str, label: str) -> dict:
    value = receipt.get("sourceManifest")
    manifest_path = _file(value, receipt.get("sourceManifestSha256"), f"{label} source manifest")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("head") != expected_commit or not isinstance(manifest.get("files"), dict):
        raise ValueError(f"{label} source manifest revision/closure is malformed")
    source = manifest.get("source")
    if not isinstance(source, str) or not Path(source).is_absolute():
        raise ValueError(f"{label} source tree path is missing")
    source_root = Path(source)
    if source_root.is_symlink():
        raise ValueError(f"{label} source tree root may not be a symlink")
    for relative, expected in manifest["files"].items():
        path = source_root / relative
        cursor = source_root
        for part in Path(relative).parts:
            cursor = cursor / part
            if cursor.is_symlink():
                raise ValueError(f"{label} source closure contains symlink: {relative}")
        if (not path.is_file() or not isinstance(expected, str) or not SHA.fullmatch(expected)
                or sha256(path) != expected):
            raise ValueError(f"{label} source input changed: {relative}")
    return manifest


def canonical_sha(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=False).encode("utf-8")).hexdigest()


def validate_registration(registration_path: Path, *, driver_path: Path,
                          official_runner_path: Path, caller_role: str = "outer") -> dict:
    """Validate all source, executable, runtime, and profile pins without spawning."""
    registration_path = Path(registration_path)
    if not registration_path.is_absolute() or registration_path.is_symlink() or not registration_path.is_file():
        raise ValueError("registration must be an absolute regular file")
    registration = json.loads(registration_path.read_text(encoding="utf-8"))
    if registration.get("format") != 1 or registration.get("official") is not True:
        raise ValueError("registration must explicitly declare official format 1")
    profile_path = _file(registration.get("profilePath"), registration.get("profileSha256"), "profile")
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    if profile.get("format") != 1:
        raise ValueError("unsupported frozen profile format")
    role, endpoint, mode = (registration.get("role"), registration.get("endpoint"),
                            registration.get("mode"))
    if role not in ("baseline", "candidate") or endpoint not in ("native", "pi", "dsh") or mode not in COUNTS:
        raise ValueError("registration role, endpoint, or mode is outside the frozen matrix")
    expected_source = profile.get("baseline") if role == "baseline" else profile.get("resolvedProductCandidate")
    if registration.get("sourceCommit") != expected_source:
        raise ValueError("registration sourceCommit differs from the frozen role pin")
    case_index = registration.get("caseIndex")
    expected_case_count = 5 if mode == "cold" else 1
    if type(case_index) is not int or not 0 <= case_index < expected_case_count:
        raise ValueError("caseIndex is outside the frozen trajectory")
    if registration.get("iterations") != COUNTS[mode]:
        raise ValueError("registered iteration count differs from the frozen profile")
    for name in ("sampleId", "caseName"):
        value = registration.get(name)
        if not isinstance(value, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{2,95}", value):
            raise ValueError(f"{name} must be a unique simple owned identifier")

    program = _file(registration.get("programPath"), registration.get("programSha256"), "program")
    program_receipt_path = _file(registration.get("programReceiptPath"),
                                 registration.get("programReceiptSha256"), "program receipt")
    program_receipt = json.loads(program_receipt_path.read_text(encoding="utf-8"))
    if (program_receipt.get("path") != str(program)
            or program_receipt.get("sha256") != sha256(program)
            or program_receipt.get("sourceCommit") != expected_source
            or program_receipt.get("compiled") is not True
            or program_receipt.get("clippyPassed") is not True):
        raise ValueError("program receipt does not prove the registered binary and role source")
    product_manifest = _source_manifest(program_receipt, expected_source, "program")
    seeder = _file(registration.get("seederPath"), registration.get("seederSha256"), "seeder")
    seeder_receipt_path = _file(registration.get("seederReceiptPath"),
                                registration.get("seederReceiptSha256"), "seeder receipt")
    seeder_receipt = json.loads(seeder_receipt_path.read_text(encoding="utf-8"))
    if (seeder_receipt.get("path") != str(seeder)
            or seeder_receipt.get("sha256") != sha256(seeder)):
        raise ValueError("seeder receipt does not match its distinct fixed executable")
    seeder_manifest = _source_manifest(seeder_receipt, seeder_receipt.get("sourceCommit", ""), "seeder")

    consumer = _file(registration.get("consumerSourcePath"), registration.get("consumerSha256"),
                     "consumer source")
    if program_receipt.get("consumerSourceSha256") != sha256(consumer):
        raise ValueError("program receipt and registered consumer source differ")
    consumer_relative = registration.get("consumerSourceRelativePath")
    if (not isinstance(consumer_relative, str)
            or product_manifest["files"].get(consumer_relative) != sha256(consumer)):
        raise ValueError("registered consumer source is outside the exact product manifest closure")
    seeder_source = registration.get("seederSourceCommit")
    if seeder_manifest.get("head") != seeder_source:
        raise ValueError("seeder source revision differs from its explicit registration")
    runner_files = registration.get("runnerFiles")
    if not isinstance(runner_files, list) or not runner_files:
        raise ValueError("registration must pin the runner dependency closure")
    for row in runner_files:
        if not isinstance(row, dict):
            raise ValueError("runner input row is malformed")
        _file(row.get("path"), row.get("sha256"), "runner input")
    if registration.get("runnerSha256") != canonical_sha(runner_files):
        raise ValueError("runner closure digest differs from registration")
    driver = _file(str(Path(driver_path)), registration.get("driverPythonSha256"), "Python driver")
    if str(driver) != registration.get("driverPythonPath"):
        raise ValueError("Python driver path differs from registration")
    official_runner = _file(registration.get("officialRunnerPath"),
                            registration.get("officialRunnerSha256"), "official sample runner")
    case_runner = _file(registration.get("caseRunnerPath"),
                        registration.get("caseRunnerSha256"), "registered execution runner")
    actual_caller = Path(official_runner_path).resolve(strict=True)
    expected_caller = {"outer": official_runner, "capture": case_runner}.get(caller_role)
    if expected_caller is None or expected_caller.resolve() != actual_caller:
        raise ValueError("caller path does not match its registered official execution role")

    runtime_files = registration.get("runtimeFiles")
    if not isinstance(runtime_files, list) or not runtime_files:
        raise ValueError("registration must pin fixed runtime critical files")
    for row in runtime_files:
        if not isinstance(row, dict):
            raise ValueError("runtime input row is malformed")
        _file(row.get("path"), row.get("sha256"), "runtime input")
    if registration.get("runtimeSha256") != canonical_sha(runtime_files):
        raise ValueError("runtime digest differs from exact critical file pins")
    if not isinstance(registration.get("runtimeArguments"), dict):
        raise ValueError("runtime arguments must be explicitly registered")
    args = registration["runtimeArguments"]
    required = {"native": ("kernel-supervisor", "kernel-worker", "providers-config"),
                "pi": ("pi-root",), "dsh": ("dsh-root",)}[endpoint]
    if any(not isinstance(args.get(key), str) or not Path(args[key]).is_absolute() for key in required):
        raise ValueError("fixed endpoint runtime arguments are incomplete")
    return {"registration": registration, "registrationBytes": registration_path.read_bytes(),
            "profile": profile, "profilePath": profile_path, "program": program,
            "programReceipt": program_receipt, "seeder": seeder,
            "seederReceipt": seeder_receipt, "consumer": consumer,
            "productManifest": product_manifest, "seederManifest": seeder_manifest,
            "driver": driver, "officialRunner": official_runner, "caseRunner": case_runner}
