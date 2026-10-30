import io
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_owned_fixture as fixture


class FakeProcess:
    def __init__(self):
        reader, self.writer = os.pipe()
        self.stdin = io.BytesIO()
        self.stdout = os.fdopen(reader, "rb")
        self.stderr = io.BytesIO()
        self.returncode = None
        self.waits = []

    def wait(self, timeout):
        self.waits.append(timeout)
        self.returncode = 0
        return 0

    def poll(self):
        return self.returncode


class NativeReadinessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        for name in ("supervisor", "worker"):
            (self.root / name).touch()
        self.process = FakeProcess()
        self.thread = None
        self.addCleanup(self.release_pipe)
        self.done = threading.Event()
        self.errors = []
        self.urls = []

    def release_pipe(self):
        if self.process.writer is not None:
            os.close(self.process.writer)
            self.process.writer = None
        if self.thread is not None:
            self.thread.join(timeout=2)
            self.assertFalse(self.thread.is_alive(), "owned fixture thread did not retire")

    def start(self, timeout=0.1, on_ready=None):
        def run():
            try:
                with fixture.owned_native(
                    self.root / "supervisor", self.root / "worker",
                    {"providers": [{"id": "synthetic"}]}, "http://127.0.0.1:1",
                    self.root, {}, readiness_timeout=timeout,
                    popen_factory=lambda *args, **kwargs: self.process,
                ) as native:
                    self.urls.append(native["url"])
                    if on_ready is not None:
                        on_ready(native)
            except BaseException as error:
                self.errors.append(error)
            finally:
                self.done.set()

        self.thread = threading.Thread(target=run, daemon=True)
        self.thread.start()

    def assert_retired(self):
        self.assertTrue(self.done.wait(2), "fixture remained blocked past its readiness deadline")
        self.thread.join(timeout=2)
        self.assertEqual(self.process.waits, [15])
        self.assertTrue(self.process.stdin.closed)
        self.assertTrue(self.process.stdout.closed)
        self.assertTrue(self.process.stderr.closed)

    def test_partial_line_times_out_and_retires_owned_process(self):
        os.write(self.process.writer, b'{"url":')
        self.start()
        self.assert_retired()
        self.assertEqual(self.urls, [])
        self.assertEqual(len(self.errors), 1)
        self.assertIsInstance(self.errors[0], TimeoutError)
        self.assertEqual(str(self.errors[0]), "Native Kernel readiness timed out")

    def test_readiness_arrives_in_chunks(self):
        os.write(self.process.writer, b'{"url":')
        later = threading.Timer(
            0.05, os.write, args=(self.process.writer, b'"http://127.0.0.1:1234"}\n'),
        )
        later.start()
        self.addCleanup(later.join)
        self.start(timeout=1)
        self.assert_retired()
        self.assertEqual(self.errors, [])
        self.assertEqual(self.urls, ["http://127.0.0.1:1234"])

    def test_empty_eof_preserves_exit_error_and_cleanup(self):
        self.release_pipe()
        self.start()
        self.assert_retired()
        self.assertEqual(len(self.errors), 1)
        self.assertIsInstance(self.errors[0], RuntimeError)
        self.assertEqual(str(self.errors[0]), "Native Kernel exited before readiness")

    def test_valid_final_line_at_eof_remains_accepted(self):
        os.write(self.process.writer, b'{"url":"http://127.0.0.1:1234"}')
        self.release_pipe()
        self.start()
        self.assert_retired()
        self.assertEqual(self.errors, [])
        self.assertEqual(self.urls, ["http://127.0.0.1:1234"])

    def test_stdout_after_ready_is_left_for_the_caller(self):
        os.write(self.process.writer, b'{"url":"http://127.0.0.1:1234"}\nnext\n')
        remaining = []
        self.start(on_ready=lambda native: remaining.append(native["process"].stdout.readline()))
        self.assert_retired()
        self.assertEqual(self.errors, [])
        self.assertEqual(remaining, [b"next\n"])

    def test_malformed_line_preserves_validation_error_and_cleanup(self):
        os.write(self.process.writer, b'not-json\n')
        self.start()
        self.assert_retired()
        self.assertEqual(len(self.errors), 1)
        self.assertIsInstance(self.errors[0], RuntimeError)
        self.assertEqual(str(self.errors[0]), "Native Kernel returned invalid readiness data")

    def test_read_error_propagates_and_retires_owned_process(self):
        os.write(self.process.writer, b'{')
        failure = OSError("controlled pipe failure")
        with patch.object(fixture.os, "read", side_effect=failure):
            self.start()
            self.assert_retired()
        self.assertEqual(self.errors, [failure])

    def test_partial_reads_share_one_deadline(self):
        os.write(self.process.writer, b'{"')
        now = [0.0]
        waits = []

        def select_partial(readers, writers, errors, timeout):
            waits.append(timeout)
            now[0] += min(0.04, timeout)
            return (readers if len(waits) < 3 else [], [], [])

        with patch.object(fixture, "monotonic", side_effect=lambda: now[0]), \
                patch.object(fixture.select, "select", side_effect=select_partial):
            self.start()
            self.assert_retired()
        self.assertEqual(len(self.errors), 1)
        self.assertIsInstance(self.errors[0], TimeoutError)
        self.assertEqual(len(waits), 3)
        for actual, expected in zip(waits, [0.1, 0.06, 0.02]):
            self.assertAlmostEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()
