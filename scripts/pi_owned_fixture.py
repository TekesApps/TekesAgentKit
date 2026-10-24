"""Create an isolated Pi data root and headless connector configuration.

This helper only prepares owned fixture files. It never launches Pi or reads
session history, user credentials, or the full process environment.
"""

import json
import os
from pathlib import Path
from urllib.parse import urlsplit


def _is_within(path, root):
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _absolute_path(value, label, *, resolve=True):
    path = Path(value)
    if not path.is_absolute():
        raise ValueError(label + " must be an absolute path")
    if path.is_symlink():
        raise ValueError(label + " must not be a symlink")
    if resolve:
        return path.resolve()
    return Path(os.path.abspath(str(path)))


def _require_fresh_directory(path, label):
    if path.exists():
        if not path.is_dir():
            raise ValueError(label + " must be a directory")
        if any(path.iterdir()):
            raise ValueError(label + " must be empty")
    elif not path.parent.is_dir():
        raise ValueError(label + " parent directory must already exist")


def _loopback_http_url(value):
    if not isinstance(value, str) or not value:
        raise ValueError("provider_url must be an HTTP loopback URL")
    try:
        parsed = urlsplit(value)
        port = parsed.port
    except ValueError as error:
        raise ValueError("provider_url must be a valid HTTP loopback URL") from error
    if parsed.scheme.lower() != "http" or parsed.hostname is None:
        raise ValueError("provider_url must be an HTTP loopback URL")
    if parsed.username is not None or parsed.password is not None or "@" in parsed.netloc:
        raise ValueError("provider_url must not contain user information")
    if parsed.hostname.lower() not in ("127.0.0.1", "localhost", "::1"):
        raise ValueError("provider_url host must be loopback")
    if port is not None and not 1 <= port <= 65535:
        raise ValueError("provider_url port is out of range")
    return value


def prepare_pi(pi_root, data_root, home, provider_url, *, reasoning=False,
               strict_environment=False):
    """Prepare an owned Pi fixture and return its existing connector shape.

    ``data_root`` and ``home`` may be absent or already-empty directories.
    The runtime installation is inspected read-only; it is never modified.
    Strict overrides do not clear a parent's inherited environment: the new
    isolated runner must also launch its Rust consumer with a closed environment.
    """
    root = _absolute_path(pi_root, "pi_root")
    if not root.is_dir():
        raise ValueError("pi_root must be an existing directory")
    node = root / "node/bin/node"
    cli = root / "node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js"
    if not node.is_file() or not cli.is_file():
        raise ValueError("pi_root must contain the Node and Pi CLI entry files")

    data = _absolute_path(data_root, "data_root", resolve=False)
    owned_home = _absolute_path(home, "home", resolve=False)
    canonical_data = data.resolve()
    canonical_home = owned_home.resolve()
    for label, candidate in (("data_root", data), ("home", owned_home)):
        canonical = candidate.resolve()
        if _is_within(canonical, root):
            raise ValueError(label + " must be outside pi_root")
    if (_is_within(canonical_data, canonical_home) or _is_within(canonical_home, canonical_data)):
        raise ValueError("data_root and home must be separate directories")
    for label, candidate in (("data_root", data), ("home", owned_home)):
        _require_fresh_directory(candidate, label)
    endpoint = _loopback_http_url(provider_url)

    if strict_environment:
        path_value = os.pathsep.join((str(node.parent), "/usr/bin", "/bin", "/usr/sbin", "/sbin"))
        environment = {"HOME": str(owned_home), "PATH": path_value, "TMPDIR": str(owned_home)}
    else:
        path_value = os.environ.get("PATH", "/usr/bin:/bin")
        environment = {"HOME": str(owned_home), "PATH": path_value}

    # Both targets have passed the emptiness checks. Create only these owned
    # directories and the provider configuration beneath data_root.
    needs_data = not data.exists()
    needs_home = not owned_home.exists()
    created_data = False
    created_home = False
    models_path = data / "agent/models.json"
    created_agent = False
    created_models = False
    try:
        if needs_data:
            data.mkdir(mode=0o700)
            created_data = True
        data.chmod(0o700)
        if needs_home:
            owned_home.mkdir(mode=0o700)
            created_home = True
        owned_home.chmod(0o700)

        agent_dir = data / "agent"
        agent_dir.mkdir(mode=0o700)
        created_agent = True
        agent_dir.chmod(0o700)

        model = {
            "id": "endpoint-test",
            "name": "Endpoint test",
            "contextWindow": 32768,
            "maxTokens": 1024,
            "input": ["text", "image"],
        }
        if reasoning:
            model["reasoning"] = True
        configuration = {
            "providers": {
                "endpoint-test": {
                    "baseUrl": endpoint,
                    "api": "openai-completions",
                    "apiKey": "synthetic-key",
                    "models": [model],
                }
            }
        }
        descriptor = os.open(str(models_path), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        created_models = True
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(configuration, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fchmod(stream.fileno(), 0o600)
    except Exception:
        if created_models:
            try:
                models_path.unlink()
            except FileNotFoundError:
                pass
        if created_agent:
            try:
                (data / "agent").rmdir()
            except OSError:
                pass
        if created_data:
            try:
                data.rmdir()
            except OSError:
                pass
        if created_home:
            try:
                owned_home.rmdir()
            except OSError:
                pass
        raise

    return {
        "kind": "pi",
        "executable": str(node),
        "arguments": [str(cli)],
        "dataRoot": str(data),
        "environment": environment,
    }

