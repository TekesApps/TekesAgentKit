"""Own and run one isolated DSH Web fixture process.

The caller supplies the complete child environment and owns the parent fixture
directory. This module never reads ambient credentials, user data, or process
environment values. It prepares only the documented DSH patch/plugin inputs,
exchanges the one-time launch token for a cookie without following redirects,
and terminates only the process group it created.
"""

from contextlib import contextmanager
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
from urllib.parse import urlsplit, urlunsplit
import urllib.error
import urllib.request


_LAUNCH = re.compile(r"dsh web: (https?://[^\s]+)")
_SENSITIVE_QUERY = re.compile(r"([?&])([^=&#\s]+)=([^&#\s]*)")
_AUTH_TEXT = re.compile(
    r"(?i)(authorization|cookie|set-cookie|token|api[_-]?key)(\s*[:=]\s*)([^\s,;]+)"
)
_COOKIE_VALUE = re.compile(r"(?i)\b(?:dsh[-_])?(?:session|cookie)(?:[-_][a-z0-9]+)?=([^;\s,]+)")


def _redact(text, secrets=()):
    """Remove launch query values and known credentials from diagnostic text."""
    text = _SENSITIVE_QUERY.sub(lambda match: match.group(1) + match.group(2) + "[REDACTED]", text)
    text = _AUTH_TEXT.sub(lambda match: match.group(1) + match.group(2) + "[REDACTED]", text)
    text = _COOKIE_VALUE.sub(lambda match: match.group(0).split("=", 1)[0] + "=[REDACTED]", text)
    for secret in secrets:
        if secret:
            text = text.replace(secret, "[REDACTED]")
    return text


def _fresh_dir(path, label):
    path = Path(path)
    if not path.is_absolute() or path.is_symlink():
        raise ValueError(label + " must be an absolute non-symlink path")
    if path.exists():
        raise ValueError(label + " must be absent")
    if not path.parent.is_dir():
        raise ValueError(label + " parent directory must already exist")
    return path


def _loopback_base_url(value):
    try:
        parsed = urlsplit(value)
        port = parsed.port
    except (TypeError, ValueError) as error:
        raise ValueError("base_url must be a valid loopback HTTP URL") from error
    if (parsed.scheme != "http" or parsed.hostname not in ("127.0.0.1", "localhost", "::1")
            or parsed.username is not None or parsed.password is not None or "@" in parsed.netloc
            or parsed.query or parsed.fragment or (port is not None and not 1 <= port <= 65535)):
        raise ValueError("base_url must be loopback HTTP without credentials, query, or fragment")
    return value


@dataclass(frozen=True)
class DshFixture:
    """Connection and provenance fields needed by the existing endpoint runner."""

    root: Path
    node: Path
    entry: Path
    home: Path
    data_root: Path
    sessions_root: Path
    patch_file: Path
    plugin_file: Path
    process: object
    url: str
    cookie: str
    version: str
    launch_seconds: float
    _logs: object

    def connector_config(self, commands):
        return {
            "kind": "dsh",
            "url": self.url,
            "cookie": self.cookie,
            "expectedVersion": self.version,
            "commands": commands,
        }

    def diagnostic_tail(self, limit=5000):
        """Return bounded redacted child output suitable for assertion errors."""
        with self._logs["lock"]:
            return "".join(self._logs["lines"])[-limit:]


@contextmanager
def owned_dsh(root, data_root, home, cwd, base_url, environment, *,
              query_sqlite=False, readiness_timeout=60, shutdown_timeout=15,
              opener_factory=None, popen_factory=None, monotonic=time.monotonic,
              sleep=time.sleep):
    """Prepare and run isolated DSH, yielding a :class:`DshFixture`.

    ``environment`` is the complete child environment. It must explicitly
    include any synthetic provider key required by the patch; ambient values
    are never inherited. DSH writes session history below ``data_root``.
    Fixture directories/files are left for the caller's evidence lifecycle.
    """
    runtime = Path(root).resolve(strict=True)
    if not runtime.is_dir():
        raise ValueError("root must be an existing DSH runtime directory")
    node = runtime / "node/bin/node"
    entry = runtime / "node_modules/@deepseek-ai/dsh/lib/bin.js"
    boot = runtime / "node_modules/@deepseek-ai/dsh-app-boot/lib/index.js"
    package_file = runtime / "node_modules/@deepseek-ai/dsh/package.json"
    if not all(path.is_file() for path in (node, entry, boot, package_file)):
        raise ValueError("root must contain DSH runtime, boot package, and package metadata")
    data = _fresh_dir(data_root, "data_root")
    owned_home = _fresh_dir(home, "home")
    for label, candidate in (("data_root", data), ("home", owned_home)):
        if candidate.resolve() == runtime or runtime in candidate.resolve().parents:
            raise ValueError(label + " must be outside root")
    if data.resolve() == owned_home.resolve() or data.resolve() in owned_home.resolve().parents or owned_home.resolve() in data.resolve().parents:
        raise ValueError("data_root and home must be separate directories")
    working_directory = Path(cwd).resolve(strict=True)
    if not working_directory.is_dir():
        raise ValueError("cwd must be an existing directory")
    endpoint = _loopback_base_url(base_url)
    if not isinstance(environment, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in environment.items()):
        raise ValueError("environment must be an explicit mapping of string names and values")
    if environment.get("HOME") != str(owned_home):
        raise ValueError("environment HOME must equal the owned home path")
    if environment.get("DSH_HOME") != str(owned_home):
        raise ValueError("environment DSH_HOME must equal the owned home path")
    if "AGENTKIT_SYNTHETIC_KEY" not in environment:
        raise ValueError("environment must explicitly provide AGENTKIT_SYNTHETIC_KEY")
    if "DSH_TELEMETRY_DISABLED" not in environment:
        raise ValueError("environment must explicitly set DSH_TELEMETRY_DISABLED")
    if not isinstance(readiness_timeout, (int, float)) or readiness_timeout <= 0:
        raise ValueError("readiness_timeout must be positive")
    if not isinstance(shutdown_timeout, (int, float)) or shutdown_timeout <= 0:
        raise ValueError("shutdown_timeout must be positive")

    data.mkdir(mode=0o700)
    owned_home.mkdir(mode=0o700)
    sessions = data / "dsh-sessions"
    plugin = data / "version-probe.mjs"
    patch_file = data / "dsh.patch.yml"
    package_uri = boot.as_uri()
    plugin_text = (
        "import { getDshRuntimeVersion } from " + json.dumps(package_uri) + ";\n"
        "export const name='agentkit-version-probe';export const inject=['webServer'];"
        "export function apply(ctx){ctx.effect(()=>ctx.webServer.register({kind:'exact',"
        "path:'/api/tekes/harness-version',handler(req,res){res.writeHead(200,"
        "{'Content-Type':'application/json'});res.end(JSON.stringify({product:'@deepseek-ai/dsh',"
        "version:getDshRuntimeVersion(),source:'@deepseek-ai/dsh-app-boot#getDshRuntimeVersion'}));}}));}\n"
    )
    plugin.write_text(plugin_text, encoding="utf-8", newline="\n")
    plugin.chmod(0o600)
    patch = [
        {"id": "llm-deepseek", "disabled": True},
        {"id": "agent-default-model", "config": {"provider": "endpoint-test", "model": "endpoint-test"}},
        {"id": "llm-pi-ai", "config": {"providers": {"endpoint-test": {
            "api": "openai-completions", "apiKeyEnv": "AGENTKIT_SYNTHETIC_KEY",
            "baseURL": endpoint, "models": [{"id": "endpoint-test", "contextWindow": 32768, "maxTokens": 1024}],
        }}}},
        {"id": "session-persistence-jsonl", "config": {"root": str(sessions), "compression": "none"}},
    ]
    if query_sqlite:
        patch.append({"id": "session-query-sqlite", "config": {"path": ":memory:", "openAt": "first-search"}})
    patch.append({"insert": [{"name": plugin.as_uri()}]})
    descriptor = os.open(str(patch_file), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
        json.dump(patch, stream, separators=(",", ":"))
        stream.write("\n")
    version = json.loads(package_file.read_text(encoding="utf-8"))["version"]
    child_env = dict(environment)
    popen = popen_factory or subprocess.Popen
    opener_factory = opener_factory or urllib.request.build_opener
    process = popen(
        [str(node), str(entry), "--profile", "web", "--patch", str(patch_file),
         "--no-open", "--host", "127.0.0.1", "--port", "0"],
        cwd=str(working_directory), env=child_env, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True, start_new_session=True,
    )
    logs = {"lines": [], "lock": threading.Lock(), "ready": threading.Event(), "launch": None,
            "eof": threading.Event(), "reader_error": None}
    secrets = [value for key, value in child_env.items()
               if re.search(r"(?i)(token|secret|password|cookie|credential|api[_-]?key)", key)]

    def drain():
        try:
            for raw in iter(process.stdout.readline, ""):
                match = _LAUNCH.search(raw)
                if match and logs["launch"] is None:
                    logs["launch"] = match.group(1)
                    logs["ready"].set()
                safe = _redact(raw, secrets)
                with logs["lock"]:
                    logs["lines"].append(safe)
                    if len(logs["lines"]) > 500:
                        del logs["lines"][:-500]
        except Exception as error:  # retained as sanitized diagnostic metadata
            logs["reader_error"] = type(error).__name__
        finally:
            logs["eof"].set()

    reader = threading.Thread(target=drain, name="agentkit-dsh-output", daemon=True)
    reader.start()
    started = monotonic()
    try:
        readiness_deadline = monotonic() + readiness_timeout
        ready = False
        while monotonic() < readiness_deadline:
            if logs["ready"].wait(min(0.1, max(0.0, readiness_deadline - monotonic()))):
                ready = True
                break
            status = process.poll()
            if status is not None:
                break
        if not ready:
            status = process.poll()
            detail = "DSH readiness timeout (exit=" + repr(status) + ")"
            if logs["reader_error"]:
                detail += "; output reader=" + logs["reader_error"]
            raise RuntimeError(detail + ": " + _redact("".join(logs["lines"])[-5000:], secrets))
        launch = logs["launch"]
        launch_parts = urlsplit(launch)
        if launch_parts.scheme != "http" or launch_parts.hostname not in ("127.0.0.1", "localhost", "::1"):
            raise RuntimeError("DSH readiness URL was not loopback HTTP")

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *_args, **_kwargs):
                return None

        opener = opener_factory(NoRedirect)
        try:
            response = opener.open(launch, timeout=10)
        except urllib.error.HTTPError as error:
            response = error
        try:
            cookie_header = response.headers.get("Set-Cookie", "")
        finally:
            if hasattr(response, "close"):
                response.close()
        cookie = cookie_header.split(";", 1)[0]
        if not cookie or "=" not in cookie:
            raise RuntimeError("DSH launch cookie exchange failed: " + _redact("".join(logs["lines"])[-1000:], secrets + [launch, cookie_header]))
        secrets.append(cookie)
        # Connector requests use the stable origin URL; the exchanged token is
        # kept only in this returned in-memory fixture object.
        stable_url = urlunsplit((launch_parts.scheme, launch_parts.netloc, "/", "", ""))
        yield DshFixture(runtime, node, entry, owned_home, data, sessions, patch_file, plugin,
                         process, stable_url, cookie, version, monotonic() - started, logs)
    finally:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            deadline = monotonic() + shutdown_timeout
            while process.poll() is None and monotonic() < deadline:
                sleep(min(0.05, max(0.0, deadline - monotonic())))
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        try:
            process.wait(timeout=max(1, shutdown_timeout))
        except (subprocess.TimeoutExpired, TimeoutError):
            # Do not target any PID other than the group created above.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=max(1, shutdown_timeout))
        reader.join(timeout=max(1, shutdown_timeout))
