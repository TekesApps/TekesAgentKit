"""Exercise Summary transition failure gates over real loopback HTTP."""

from pathlib import Path
import queue
import sys
import threading
import unittest
import urllib.error
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from summary_transition_provider import HeldFailureGate, provider_with_failure_gates


class SummaryTransitionProviderTests(unittest.TestCase):
    def setUp(self):
        self.paths = []
        self.gate = HeldFailureGate("SUMMARY_TRANSITION_FAILURE", timeout_seconds=3)
        handler = provider_with_failure_gates(
            workspace=lambda: Path("/owned workspace"),
            requests=self.paths,
            gates={"child_failure": self.gate},
        )
        import http.server
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.server_thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.server_thread.start()

    def tearDown(self):
        self.gate.abort()
        self.server.shutdown()
        self.server.server_close()
        self.server_thread.join(timeout=2)

    def _url(self, path):
        return f"http://127.0.0.1:{self.server.server_port}{path}"

    @staticmethod
    def _post(url, body):
        request = urllib.request.Request(url, data=body,
                                         headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=5) as response:
                return response.status, response.read()
        except urllib.error.HTTPError as error:
            try:
                return error.code, error.read()
            finally:
                error.close()

    def _start_post(self, path, body):
        results = queue.Queue()

        def run():
            try:
                results.put(self._post(self._url(path), body))
            except BaseException as error:  # surface client-thread failures in the test
                results.put(error)

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        return thread, results

    def test_matching_request_waits_until_explicit_capture_release_then_returns_400(self):
        thread, results = self._start_post(
            "/v1/chat/completions",
            b'{"messages":[{"role":"user","content":"SUMMARY_TRANSITION_FAILURE"}]}',
        )
        self.assertTrue(self.gate.entered.wait(timeout=2), "matching request did not enter gate")
        with self.assertRaises(queue.Empty):
            results.get_nowait()
        self.assertTrue(thread.is_alive(), "held request returned before explicit release")
        self.assertEqual(self.paths, ["/v1/chat/completions"])
        self.assertEqual(self.gate.receipt(), {
            "requestCount": 1,
            "entered": True,
            "explicitlyReleased": False,
            "aborted": False,
            "timedOut": False,
        })

        self.gate.release_after_capture()
        result = results.get(timeout=2)
        self.assertFalse(isinstance(result, BaseException), repr(result))
        status, response_body = result
        self.assertEqual(status, 400)
        self.assertIn(b"SUMMARY_TRANSITION_FAILURE", response_body)
        self.assertIn(b"invalid_request_error", response_body)
        thread.join(timeout=2)
        self.assertEqual(self.gate.receipt(), {
            "requestCount": 1,
            "entered": True,
            "explicitlyReleased": True,
            "aborted": False,
            "timedOut": False,
        })

    def test_ordinary_request_is_replayed_to_original_sse_handler_once(self):
        status, response_body = self._post(
            self._url("/v1/chat/completions"),
            b'{"messages":[{"role":"user","content":"ordinary request"}]}',
        )
        self.assertEqual(status, 200)
        self.assertIn(b"data: {", response_body)
        self.assertIn(b"data: [DONE]", response_body)
        self.assertIn(b"ENDPOINT_", response_body)
        self.assertIn(b"REPLY_OK", response_body)
        self.assertEqual(self.paths, ["/v1/chat/completions"])
        self.assertFalse(self.gate.entered.is_set())
        self.assertEqual(self.gate.receipt()["requestCount"], 0)

    def test_abort_unblocks_request_without_counting_as_explicit_release(self):
        thread, results = self._start_post(
            "/v1/responses",
            b'{"input":[{"role":"user","content":"SUMMARY_TRANSITION_FAILURE"}]}',
        )
        self.assertTrue(self.gate.entered.wait(timeout=2))
        self.gate.abort()
        result = results.get(timeout=2)
        self.assertFalse(isinstance(result, BaseException), repr(result))
        self.assertEqual(result[0], 503)
        receipt = self.gate.receipt()
        self.assertTrue(receipt["aborted"])
        self.assertFalse(receipt["explicitlyReleased"])
        self.assertFalse(receipt["timedOut"])
        thread.join(timeout=2)

    def test_timeout_unblocks_request_without_counting_as_explicit_release(self):
        short_gate = HeldFailureGate("SHORT_TIMEOUT_MARKER", timeout_seconds=0.1)
        handler = provider_with_failure_gates(
            workspace=lambda: Path("/owned workspace"),
            requests=self.paths,
            gates={"short": short_gate},
        )
        self.server.shutdown()
        self.server.server_close()
        self.server_thread.join(timeout=2)
        self.server = __import__("http.server").server.ThreadingHTTPServer(
            ("127.0.0.1", 0), handler)
        self.server_thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.server_thread.start()
        thread, results = self._start_post(
            "/v1/responses",
            b'{"input":[{"role":"user","content":"SHORT_TIMEOUT_MARKER"}]}',
        )
        self.assertTrue(short_gate.entered.wait(timeout=2))
        result = results.get(timeout=2)
        self.assertFalse(isinstance(result, BaseException), repr(result))
        self.assertEqual(result[0], 504)
        receipt = short_gate.receipt()
        self.assertTrue(receipt["timedOut"])
        self.assertFalse(receipt["explicitlyReleased"])
        self.assertFalse(receipt["aborted"])
        thread.join(timeout=2)

    def test_release_before_entry_and_invalid_parameters_are_rejected(self):
        with self.assertRaises(RuntimeError):
            self.gate.release_after_capture()
        for marker, timeout in (("", 1), ("valid", 0), ("valid", 31), ("valid", True)):
            with self.subTest(marker=marker, timeout=timeout):
                with self.assertRaises(ValueError):
                    HeldFailureGate(marker, timeout_seconds=timeout)
        with self.assertRaises(ValueError):
            provider_with_failure_gates(
                workspace=lambda: Path("/owned"), requests=self.paths,
                gates={"one": self.gate, "two": HeldFailureGate(self.gate.marker)},
            )


if __name__ == "__main__":
    unittest.main()
