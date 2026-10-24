"""Run an isolated, owned Native Kernel fixture around a caller's probes.

The helper uses only explicit installed supervisor/worker paths and caller
provided fixture inputs. It never builds Kernel, reads a user's home, or
retrieves credentials from disk.
"""

from contextlib import contextmanager
import base64
import json
import os
from pathlib import Path
import select
import secrets
import struct
import subprocess
import threading
from urllib.parse import urlsplit


_BOOTSTRAP = b"TEKES-KERNEL-BOOTSTRAP/1\n"
_CREDENTIAL_BINDING = "openai-main"
_SYNTHETIC_KEY = "synthetic-key"


def _owned_path(value, label):
    path = Path(value)
    if not path.is_absolute() or path.is_symlink():
        raise ValueError(label + " must be an absolute non-symlink path")
    return path.resolve()


def _safe_stderr(chunks, token):
    raw = b"".join(chunks).decode("utf-8", errors="replace")
    # Bootstrap material and fixture credentials must never escape in errors.
    for secret in (token.hex(), base64.urlsafe_b64encode(token).decode().rstrip("="), _SYNTHETIC_KEY):
        if secret:
            raw = raw.replace(secret, "[redacted]")
    return raw[-4096:]


def _stop(process):
    if process is None:
        return
    try:
        if process.stdin is not None and not process.stdin.closed:
            process.stdin.close()
    except (BrokenPipeError, OSError):
        pass
    try:
        process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        # Popen.terminate targets only the supervisor PID owned by this helper.
        if process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            # Do not silently accept a fixture with a still-running supervisor.
            # Preserve the original runner's shutdown failure, without killing
            # any process group or unrelated descendants.
            raise RuntimeError("Owned Native supervisor did not exit after termination") from None


@contextmanager
def owned_native(supervisor, worker, providers, provider_url, base_root,
                 environment, *, readiness_timeout=15):
    """Yield the running fixture as ``process,url,bearer,state_root,providers``.

    ``providers`` is copied and its first endpoint is pointed at the local
    provider. ``environment`` is an explicit caller supplied mapping; the two
    Kernel secrets are always removed from the child environment and HOME is
    redirected to an empty owned directory.
    """
    supervisor_path = _owned_path(supervisor, "supervisor")
    worker_path = _owned_path(worker, "worker")
    root = _owned_path(base_root, "base_root")
    if not supervisor_path.is_file() or not worker_path.is_file():
        raise ValueError("explicit Kernel supervisor and worker files are required")
    if not root.is_dir():
        raise ValueError("base_root must be an existing directory")
    if not isinstance(providers, dict) or not isinstance(providers.get("providers"), list) or not providers["providers"]:
        raise ValueError("providers must contain at least one fixture provider")
    try:
        parsed_provider_url = urlsplit(provider_url)
        provider_port = parsed_provider_url.port
    except (TypeError, ValueError):
        parsed_provider_url = None
        provider_port = None
    if (parsed_provider_url is None or parsed_provider_url.scheme.lower() != "http"
            or parsed_provider_url.hostname not in ("127.0.0.1", "localhost", "::1")
            or parsed_provider_url.username is not None or parsed_provider_url.password is not None
            or (provider_port is not None and not 1 <= provider_port <= 65535)):
        raise ValueError("provider_url must identify the local fixture provider")
    if not isinstance(environment, dict) or any(not isinstance(k, str) or not isinstance(v, str) for k, v in environment.items()):
        raise ValueError("environment must be a string mapping")
    if readiness_timeout <= 0:
        raise ValueError("readiness_timeout must be positive")

    launch_path = root / "kernel-launch.json"
    if launch_path.exists() or launch_path.is_symlink():
        raise ValueError("owned Kernel launch path already exists")
    state_root = root / "kernel"
    if state_root.exists() or state_root.is_symlink():
        raise ValueError("owned Kernel state path already exists")
    home = root / "kernel-home"
    if home.exists() or home.is_symlink():
        raise ValueError("owned Kernel home path already exists")

    fixture_providers = json.loads(json.dumps(providers))
    first_provider = fixture_providers["providers"][0]
    credential_key = first_provider.get("credential_key", _CREDENTIAL_BINDING)
    if not isinstance(credential_key, str) or not credential_key:
        raise ValueError("fixture provider must name its credential binding")
    first_provider["endpoint"] = provider_url
    state_root.mkdir(mode=0o700)
    try:
        config_dir = state_root / "config"
        config_dir.mkdir(mode=0o700)
        settings_path = config_dir / "settings.json"
        settings_path.write_text(json.dumps({
            "format": 1, "revision": 1,
            "limits": {"max_workers": 8, "max_provider_leases": 8},
        }, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        settings_path.chmod(0o600)
        home.mkdir(mode=0o700)
        launch_path = root / "kernel-launch.json"
        launch_path.write_text(json.dumps({
            "format": 1,
            "bootstrap_version": 1,
            "root": str(state_root),
            "worker": str(worker_path),
            "listen": "127.0.0.1:0",
            "providers": fixture_providers,
            "credential_bindings": {credential_key: "AGENTKIT_SYNTHETIC_KEY"},
        }, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        launch_path.chmod(0o600)

        child_env = dict(environment)
        child_env.pop("TEKES_KERNEL_ENDPOINT_TOKEN", None)
        child_env.pop("AGENTKIT_SYNTHETIC_KEY", None)
        child_env["HOME"] = str(home)
        process = subprocess.Popen(
            [str(supervisor_path), "--built-in", str(launch_path)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env=child_env,
        )
    except BaseException:
        # State is uniquely created below the supplied root. Retain it on
        # failure for diagnosis; never recursively remove caller-owned data.
        raise

    stderr_chunks = []
    stderr_lock = threading.Lock()

    def drain_stderr():
        try:
            for chunk in iter(process.stderr.readline, b""):
                with stderr_lock:
                    stderr_chunks.append(chunk)
                    # Bound memory while preserving the recent diagnostic tail.
                    if len(stderr_chunks) > 256:
                        del stderr_chunks[:len(stderr_chunks) - 256]
        except (OSError, ValueError):
            pass

    stderr_thread = threading.Thread(target=drain_stderr, name="native-fixture-stderr", daemon=True)
    stderr_thread.start()
    token = secrets.token_bytes(32)
    try:
        payload = json.dumps({
            "endpointToken": token.hex(),
            "credentials": {credential_key: _SYNTHETIC_KEY},
        }, separators=(",", ":")).encode("utf-8")
        process.stdin.write(_BOOTSTRAP + struct.pack("!I", len(payload)) + payload)
        process.stdin.flush()
        if not select.select([process.stdout], [], [], readiness_timeout)[0]:
            raise TimeoutError("Native Kernel readiness timed out")
        line = process.stdout.readline()
        if not line:
            with stderr_lock:
                detail = _safe_stderr(stderr_chunks, token)
            raise RuntimeError("Native Kernel exited before readiness" + ((": " + detail) if detail else ""))
        try:
            ready = json.loads(line)
            url = ready["url"]
            if not isinstance(url, str) or not url.startswith("http://127.0.0.1:"):
                raise ValueError
        except (ValueError, TypeError, KeyError):
            raise RuntimeError("Native Kernel returned invalid readiness data") from None
        bearer = base64.urlsafe_b64encode(token).decode("ascii").rstrip("=")
        yield {
            "process": process,
            "url": url,
            "bearer": bearer,
            "state_root": state_root,
            "providers": fixture_providers,
            "diagnostic_tail": lambda: _safe_stderr(list(stderr_chunks), token),
        }
    finally:
        _stop(process)
        try:
            process.stdout.close()
        except (OSError, ValueError):
            pass
        try:
            process.stderr.close()
        except (OSError, ValueError):
            pass
        stderr_thread.join(timeout=1)
