import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_probe_provider import native_probe_intent, MARKERS

class NativeProviderIntent(unittest.TestCase):
    def test_dsh_injected_user_context_does_not_hide_the_explicit_input(self):
        body={'messages':[{'role':'user','content':MARKERS[0]},{'role':'user','content':'runtime-context'},{'role':'user','content':'time-context'}]}
        self.assertEqual(native_probe_intent(body),MARKERS[0])
    def test_latest_recovery_outranks_a_historical_error(self):
        body={'messages':[{'role':'user','content':MARKERS[0]},{'role':'user','content':MARKERS[1]},{'role':'user','content':MARKERS[2]},{'role':'user','content':'runtime-context'}]}
        self.assertEqual(native_probe_intent(body),MARKERS[2])
    def test_assistant_and_tool_quotes_do_not_select_a_fault(self):
        self.assertIsNone(native_probe_intent({'messages':[{'role':'assistant','content':MARKERS[1]},{'role':'tool','content':MARKERS[0]}]}))
    def test_responses_message_list_and_direct_input_are_supported(self):
        self.assertEqual(native_probe_intent({'input':[{'role':'user','content':[{'type':'input_text','text':MARKERS[1]}]}]}),MARKERS[1])
        self.assertEqual(native_probe_intent({'input':MARKERS[2]}),MARKERS[2])
    def test_unrelated_fixture_requests_are_not_faulted(self):
        self.assertIsNone(native_probe_intent({'messages':[{'role':'user','content':'normal endpoint proof'}]}))
        self.assertIsNone(native_probe_intent({}))
