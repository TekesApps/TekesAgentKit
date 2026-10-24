import sys
import json
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_probe_provider import native_probe_intent, directory_probe_path, MARKERS

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


class DirectoryProviderIntent(unittest.TestCase):
    def test_exact_path_survives_json_roundtrip_and_ssd_spaces(self):
        for path in ['/Volumes/HP P900/dev/项目/child', '/owned/quote"and\\slash', '/owned/trailing ']:
            with self.subTest(path=path):
                body = json.loads(json.dumps({'messages': [{'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY='+path}]}))
                self.assertEqual(directory_probe_path(body), path)

    def test_responses_parts_and_direct_input_preserve_the_path(self):
        path = '/Volumes/HP P900/child'
        self.assertEqual(directory_probe_path({'input': [{'role': 'user', 'content': [{'type': 'input_text', 'text': 'ENDPOINT_CHANGE_DIRECTORY='+path}]}]}), path)
        self.assertEqual(directory_probe_path({'input': 'ENDPOINT_CHANGE_DIRECTORY='+path}), path)

    def test_user_context_does_not_hide_latest_explicit_marker(self):
        body = {'messages': [{'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY=/old'}, {'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY=/new path'}, {'role': 'user', 'content': 'runtime-context'}]}
        self.assertEqual(directory_probe_path(body), '/new path')

    def test_non_user_quotations_and_unrelated_fields_are_not_requests(self):
        for role in ['assistant', 'tool', 'system']:
            self.assertIsNone(directory_probe_path({'messages': [{'role': role, 'content': 'ENDPOINT_CHANGE_DIRECTORY=/quoted'}]}))
        self.assertIsNone(directory_probe_path({'messages': [{'role': 'user', 'metadata': 'ENDPOINT_CHANGE_DIRECTORY=/quoted', 'content': 'normal'}]}))

    def test_invalid_latest_marker_does_not_replay_an_older_path(self):
        for suffix in ['', 'relative', '/bad\0path']:
            self.assertIsNone(directory_probe_path({'messages': [{'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY=/old'}, {'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY='+suffix}]}))
