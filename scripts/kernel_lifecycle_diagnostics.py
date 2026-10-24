"""Read-only, bounded sampling of open files beneath an isolated Kernel state root.

This records process and file-descriptor observations from macOS ``lsof``. It does
not prove that a lock is usable, and a missing sample does not prove that no holder
exists. The module never reads state-file contents or changes processes or locks.
"""

from __future__ import annotations

import json
import math
import os
import subprocess
import threading
import time
from contextlib import AbstractContextManager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, TextIO


_COMMAND = "lsof"
_FORMAT_VERSION = 1


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat()


def _resolved(path: os.PathLike[str] | str) -> Path:
    return Path(path).expanduser().resolve(strict=False)


def _is_within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def _validate_paths(
    state_root: os.PathLike[str] | str,
    output_dir: os.PathLike[str] | str,
) -> tuple[Path, Path]:
    state_arg = Path(state_root).expanduser()
    output_arg = Path(output_dir).expanduser()
    if not state_arg.is_absolute():
        raise ValueError("state_root must be an absolute path")
    if not output_arg.is_absolute():
        raise ValueError("output_dir must be an absolute path")

    root = state_arg.resolve(strict=True)
    if root == Path(root.anchor):
        raise ValueError("state_root cannot be a filesystem root")
    if not root.is_dir():
        raise ValueError("state_root must be a directory")

    output = _resolved(output_arg)
    if _is_within(output, root):
        raise ValueError("output_dir must be outside state_root")
    if output == Path(output.anchor):
        raise ValueError("output_dir cannot be a filesystem root")
    return root, output


def _text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return str(value)


class KernelLifecycleDiagnostics(AbstractContextManager["KernelLifecycleDiagnostics"]):
    """Periodically run a read-only ``lsof`` query and append bounded JSONL evidence.

    Construct with an absolute, isolated state root and a separate output directory.
    Call :meth:`start` explicitly, or use the object as a context manager. ``mark``
    adds a named phase record. ``stop`` performs a bounded join and returns metadata.

    The sample limit counts lsof attempts. When it is reached, one explicit
    ``truncated`` record is appended and sampling stops; this marker is in addition
    to the configured number of attempts.
    """

    def __init__(
        self,
        state_root: os.PathLike[str] | str,
        output_dir: os.PathLike[str] | str,
        *,
        interval: float = 0.25,
        timeout: float = 5.0,
        max_samples: int = 400,
        join_timeout: float | None = None,
        runner: Callable[..., Any] | None = None,
        clock: Callable[[], str] = _timestamp,
        monotonic: Callable[[], float] = time.monotonic,
        waiter: Callable[[threading.Event, float], bool] | None = None,
    ) -> None:
        self.state_root, self.output_dir = _validate_paths(state_root, output_dir)
        if not math.isfinite(interval) or interval <= 0:
            raise ValueError("interval must be finite and positive")
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("timeout must be finite and positive")
        if max_samples <= 0:
            raise ValueError("max_samples must be positive")
        self.interval = float(interval)
        self.timeout = float(timeout)
        self.max_samples = int(max_samples)
        self.join_timeout = float(join_timeout if join_timeout is not None else timeout + 1.0)
        if not math.isfinite(self.join_timeout) or self.join_timeout <= 0:
            raise ValueError("join_timeout must be finite and positive")

        self._runner = runner or subprocess.run
        self._clock = clock
        self._monotonic = monotonic
        self._waiter = waiter or (lambda event, seconds: event.wait(seconds))
        self._stop_event = threading.Event()
        self._write_lock = threading.Lock()
        self._thread: threading.Thread | None = None
        self._samples_file: TextIO | None = None
        self._started = False
        self._stopped = False
        self._sample_count = 0
        self._truncated = False
        self._errors: list[dict[str, str]] = []
        self._marks: list[dict[str, str]] = []
        self._started_at: str | None = None
        self._stopped_at: str | None = None
        self._metadata_path = self.output_dir / "metadata.json"
        self._samples_path = self.output_dir / "samples.jsonl"
        self._metadata_created = False

    def start(self) -> "KernelLifecycleDiagnostics":
        if self._started:
            raise RuntimeError("sampler has already been started")
        if self._stopped:
            raise RuntimeError("a stopped sampler cannot be restarted")

        self._started = True
        self._started_at = self._clock()
        try:
            self.output_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
            # Refuse to mix runs or overwrite evidence from a previous invocation.
            if any(self.output_dir.iterdir()):
                raise FileExistsError("output directory already contains sampler evidence")
            os.chmod(self.output_dir, 0o700)

            fd = os.open(self._samples_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            os.chmod(self._samples_path, 0o600)
            self._samples_file = os.fdopen(fd, "a", encoding="utf-8")
            self._write_metadata()
            self._thread = threading.Thread(
                target=self._sample_loop,
                name="kernel-lifecycle-lsof-sampler",
                daemon=True,
            )
            self._thread.start()
        except Exception as exc:
            # Diagnostics must not mask the lifecycle assertion being observed.
            self._errors.append(
                {"at": self._clock(), "kind": "start_error", "message": f"{type(exc).__name__}: {exc}"}
            )
            self._stopped = True
            self._stopped_at = self._clock()
            if self._samples_file is not None:
                try:
                    self._samples_file.close()
                except OSError as close_error:
                    self._errors.append(
                        {"at": self._clock(), "kind": "close_error", "message": str(close_error)}
                    )
                self._samples_file = None
        return self

    def __enter__(self) -> "KernelLifecycleDiagnostics":
        return self.start()

    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> bool:
        self.stop()
        return False

    def mark(self, label: str) -> None:
        """Append a phase marker without touching the state root."""
        if not isinstance(label, str) or not label.strip():
            raise ValueError("label must be a non-empty string")
        if len(label) > 200:
            raise ValueError("label must be at most 200 characters")
        with self._write_lock:
            if not self._started:
                raise RuntimeError("sampler has not been started")
            if self._stopped:
                if self._thread is not None:
                    raise RuntimeError("cannot mark a stopped sampler")
            marker = {"type": "mark", "at": self._clock(), "label": label}
            self._marks.append({"at": marker["at"], "label": label})
            if self._samples_file is not None:
                try:
                    self._write_record_locked(marker)
                    self._write_metadata_locked()
                except Exception as exc:
                    self._errors.append(
                        {"at": self._clock(), "kind": "write_error", "message": f"{type(exc).__name__}: {exc}"}
                    )

    def stop(self) -> dict[str, Any]:
        """Stop sampling, perform a bounded join, and return the final metadata."""
        if not self._started:
            return self._metadata()
        with self._write_lock:
            if self._stopped:
                return self._metadata()
            self._stopped = True
            self._stop_event.set()

        thread = self._thread
        if thread is not None:
            thread.join(self.join_timeout)
            if thread.is_alive():
                with self._write_lock:
                    self._errors.append(
                        {
                            "at": self._clock(),
                            "kind": "join_timeout",
                            "message": f"sampler thread remained alive after {self.join_timeout:g}s",
                        }
                    )

        with self._write_lock:
            self._stopped_at = self._clock()
            if self._samples_file is not None:
                try:
                    self._samples_file.flush()
                    os.fsync(self._samples_file.fileno())
                    self._samples_file.close()
                except Exception as exc:
                    self._errors.append(
                        {"at": self._clock(), "kind": "close_error", "message": f"{type(exc).__name__}: {exc}"}
                    )
                self._samples_file = None
            try:
                self._write_metadata_locked()
            except Exception as exc:
                self._errors.append(
                    {"at": self._clock(), "kind": "metadata_error", "message": f"{type(exc).__name__}: {exc}"}
                )
            return self._metadata()

    @property
    def metadata(self) -> dict[str, Any]:
        """Return the in-memory receipt, including errors when output was unavailable."""
        with self._write_lock:
            return self._metadata()

    def _sample_loop(self) -> None:
        while not self._stop_event.is_set():
            started_at = self._clock()
            started_monotonic = self._monotonic()
            record: dict[str, Any] = {
                "type": "sample",
                "started_at": started_at,
                "command": ["lsof", "-nP", "-Fpcflnt", "+D", str(self.state_root)],
                "returncode": None,
                "stdout": "",
                "stderr": "",
                "status": "error",
            }
            try:
                result = self._runner(
                    record["command"],
                    shell=False,
                    capture_output=True,
                    text=True,
                    timeout=self.timeout,
                    check=False,
                )
                record["returncode"] = result.returncode
                record["stdout"] = _text(result.stdout)
                record["stderr"] = _text(result.stderr)
                if result.returncode == 0:
                    record["status"] = "partial" if record["stderr"] else "ok"
                elif result.returncode == 1:
                    record["status"] = (
                        "partial" if record["stdout"] or record["stderr"] else "empty"
                    )
                else:
                    record["status"] = "error"
                if result.returncode not in (0, 1):
                    record["error"] = f"lsof exited with status {result.returncode}"
                elif record["status"] == "partial":
                    record["error"] = "lsof returned partial output or diagnostics"
            except subprocess.TimeoutExpired as exc:
                record["status"] = "timeout"
                record["stdout"] = _text(exc.stdout)
                record["stderr"] = _text(exc.stderr)
                record["error"] = str(exc)
            except Exception as exc:  # Preserve OS launch and injected runner errors.
                record["status"] = "error"
                record["error"] = f"{type(exc).__name__}: {exc}"

            record["finished_at"] = self._clock()
            record["elapsed_seconds"] = max(0.0, self._monotonic() - started_monotonic)
            with self._write_lock:
                # stop() first signals the thread, then joins it before closing
                # output. Preserve an already-running observation during that
                # join; only a timed-out join may leave output closed here.
                if self._samples_file is None:
                    break
                if self._sample_count >= self.max_samples:
                    try:
                        self._write_truncated_locked()
                    except Exception as exc:
                        self._errors.append(
                            {"at": self._clock(), "kind": "write_error", "message": f"{type(exc).__name__}: {exc}"}
                        )
                    break
                if record["status"] in ("timeout", "error", "partial"):
                    self._errors.append(
                        {
                            "at": record["finished_at"],
                            "kind": record["status"],
                            "message": record.get("error", record["status"]),
                        }
                    )
                try:
                    self._write_record_locked(record)
                    self._sample_count += 1
                    if self._sample_count >= self.max_samples:
                        self._write_truncated_locked()
                    self._write_metadata_locked()
                except Exception as exc:
                    self._errors.append(
                        {"at": self._clock(), "kind": "write_error", "message": f"{type(exc).__name__}: {exc}"}
                    )
                    self._stop_event.set()
                    break
                if self._truncated:
                    break

            elapsed = max(0.0, self._monotonic() - started_monotonic)
            self._waiter(self._stop_event, max(0.0, self.interval - elapsed))

    def _write_truncated_locked(self) -> None:
        if self._truncated:
            return
        self._truncated = True
        self._write_record_locked(
            {
                "type": "truncated",
                "at": self._clock(),
                "reason": "max_samples_reached",
                "max_samples": self.max_samples,
            }
        )
        self._stop_event.set()

    def _write_record_locked(self, record: dict[str, Any]) -> None:
        if self._samples_file is None:
            raise RuntimeError("samples output is not open")
        self._samples_file.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")
        self._samples_file.flush()

    def _metadata(self) -> dict[str, Any]:
        return {
            "format_version": _FORMAT_VERSION,
            "state_root": str(self.state_root),
            "output_dir": str(self.output_dir),
            "command": ["lsof", "-nP", "-Fpcflnt", "+D", str(self.state_root)],
            "interval_seconds": self.interval,
            "timeout_seconds": self.timeout,
            "join_timeout_seconds": self.join_timeout,
            "max_samples": self.max_samples,
            "sample_count": self._sample_count,
            "truncated": self._truncated,
            "started_at": self._started_at,
            "stopped_at": self._stopped_at,
            "marks": list(self._marks),
            "errors": list(self._errors),
            "lock_availability_proven": False,
            "missing_sample_proves_no_holder": False,
            "evidence_limit": "lsof observations do not prove lock availability; missing samples do not prove absence of a holder",
        }

    def _write_metadata(self) -> None:
        with self._write_lock:
            self._write_metadata_locked()

    def _write_metadata_locked(self) -> None:
        metadata = self._metadata()
        if not self._metadata_created:
            fd = os.open(
                self._metadata_path,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                0o600,
            )
            try:
                with os.fdopen(fd, "w", encoding="utf-8") as handle:
                    json.dump(metadata, handle, ensure_ascii=False, indent=2, sort_keys=True)
                    handle.write("\n")
                    handle.flush()
                    os.fsync(handle.fileno())
                os.chmod(self._metadata_path, 0o600)
                self._metadata_created = True
            except BaseException:
                try:
                    os.close(fd)
                except OSError:
                    pass
                raise
            return
        with self._metadata_path.open("r+", encoding="utf-8") as handle:
            handle.seek(0)
            handle.truncate()
            json.dump(metadata, handle, ensure_ascii=False, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(self._metadata_path, 0o600)

