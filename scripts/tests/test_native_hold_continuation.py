import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from native_dsh_mixed_fixture import BUSY_HOLD_TEXT, strip_provider_hold_request


class NativeHoldContinuationTests(unittest.TestCase):
    def setUp(self):
        self.held = {"input": [{"role": "user", "content": BUSY_HOLD_TEXT}]}
        self.current = {"previous_response_id": "owned-response", "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "current"}]}]}

    def test_only_fixture_bound_response_id_accepts_delta_transport(self):
        self.assertEqual(strip_provider_hold_request("native", self.current, self.held,
            held_response_id="owned-response"), self.current)
        for expected in (None, "other-response"):
            with self.assertRaises(ValueError):
                strip_provider_hold_request("native", self.current, self.held,
                    held_response_id=expected)

    def test_delta_transport_rejects_extra_history_or_an_unbound_hold_chain(self):
        for current, held in [
            (dict(self.current, input=self.held["input"] + self.current["input"]), self.held),
            (self.current, dict(self.held, previous_response_id="unknown")),
        ]:
            with self.assertRaises(ValueError):
                strip_provider_hold_request("native", current, held,
                    held_response_id="owned-response")


if __name__ == "__main__":
    unittest.main()
