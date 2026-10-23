import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mixed_tool_provider import MARKER, frames


class MixedToolProviderTests(unittest.TestCase):
    def test_unrelated_requests_are_untouched(self):
        self.assertIsNone(frames({'messages': [{'role': 'user', 'content': 'ordinary'}]}, '/owned/proof.txt'))

    def test_chat_only_progresses_after_both_original_results_and_uses_view_only(self):
        body = {'messages': [{'role': 'user', 'content': MARKER}], 'tools': [{'function': {'name': 'str_replace_editor', 'parameters': {'properties': {'path': {'type': 'string'}}}}}]}
        first = frames(body, '/owned/proof.txt')
        calls = [x['choices'][0]['delta']['tool_calls'][0] for x in first if 'tool_calls' in x['choices'][0]['delta']]
        self.assertEqual([x['id'] for x in calls], ['mixed-a', 'mixed-b'])
        self.assertTrue(all('"command": "view"' in x['function']['arguments'] for x in calls))
        body['messages'].append({'role': 'tool', 'tool_call_id': 'mixed-a'})
        self.assertEqual(frames(body, '/owned/proof.txt'), first)
        body['messages'].append({'role': 'tool', 'tool_call_id': 'mixed-b'})
        self.assertEqual(frames(body, '/owned/proof.txt')[0]['choices'][0]['delta']['tool_calls'][0]['id'], 'mixed-c')
        body['messages'].append({'role': 'tool', 'tool_call_id': 'mixed-c'})
        self.assertIn('MIXED_FINAL_OWNED', frames(body, '/owned/proof.txt')[0]['choices'][0]['delta']['content'])

    def test_responses_uses_public_read_and_separate_reasoning_index(self):
        body = {'input': [{'role': 'user', 'content': MARKER}], 'tools': [{'type': 'function', 'name': 'read', 'parameters': {'properties': {'path': {'type': 'string'}}}}]}
        out = frames(body, '/owned/proof.txt', responses=True)
        calls = [x for x in out if x.get('item', {}).get('type') == 'function_call']
        self.assertEqual([x['item']['call_id'] for x in calls], ['mixed-a', 'mixed-b'])
        self.assertEqual([x['output_index'] for x in calls], [1, 2])
        body['input'] += [{'type': 'function_call_output', 'call_id': name} for name in ['mixed-a', 'mixed-b', 'mixed-c']]
        self.assertIn('MIXED_FINAL_OWNED', frames(body, '/owned/proof.txt', responses=True)[-1]['response']['output'][0]['content'][0]['text'])

    def test_dsh_read_uses_its_advertised_file_path(self):
        body = {'messages': [{'role': 'user', 'content': MARKER}], 'tools': [{'function': {
            'name': 'read', 'parameters': {'properties': {'file_path': {'type': 'string'}}}}}]}
        import json
        calls = [x['choices'][0]['delta']['tool_calls'][0] for x in frames(body, '/owned/proof.txt')
                 if 'tool_calls' in x['choices'][0]['delta']]
        self.assertEqual(json.loads(calls[0]['function']['arguments']), {'file_path': '/owned/proof.txt'})
        body['tools'][0]['function']['parameters'] = {}
        with self.assertRaisesRegex(ValueError, 'advertise its path'):
            frames(body, '/owned/proof.txt')

    def test_responses_followup_needs_no_repeated_user_marker(self):
        import json
        body = {'input': [{'type': 'function_call_output', 'call_id': call} for call in ['mixed-a', 'mixed-b']],
                'tools': [{'type': 'function', 'name': 'read', 'parameters': {
                    'properties': {'path': {}, 'offset': {}, 'limit': {}}, 'required': ['path', 'offset', 'limit']}}]}
        out = frames(body, '/owned/proof.txt', responses=True)
        call = out[0]
        self.assertEqual(call['output_index'], 0)
        self.assertEqual(call['item']['call_id'], 'mixed-c')
        self.assertEqual(json.loads(out[-1]['response']['output'][0]['arguments']),
                         {'path': '/owned/proof.txt', 'offset': 1, 'limit': 2})
        self.assertEqual(out[-1]['response']['output'][0]['call_id'], call['item']['call_id'])
        body['input'] = [{'type': 'function_call_output', 'call_id': 'mixed-c'}]
        self.assertIn('MIXED_FINAL_OWNED', frames(body, '/owned/proof.txt', responses=True)[-1]['response']['output'][0]['content'][0]['text'])

    def test_unadvertised_tool_is_never_invented(self):
        with self.assertRaisesRegex(ValueError, 'public read tool'):
            frames({'messages': [{'role': 'user', 'content': MARKER}]}, '/owned/proof.txt')
