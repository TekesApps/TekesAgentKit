"""Exercise the shared fixture through HTTP without any harness or paid provider."""
import http.server
import json
from pathlib import Path
import sys
import threading
import unittest
import urllib.error
import urllib.request
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from owned_endpoint_provider import provider_handler


class OwnedProvider(unittest.TestCase):
    def setUp(self):
        self.paths = []
        handler = provider_handler(workspace=lambda: Path('/owned work'), requests=self.paths)
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def request(self, path, body):
        request = urllib.request.Request(
            'http://127.0.0.1:'+str(self.server.server_port)+path,
            data=json.dumps(body).encode(), headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.read().decode()

    def test_chat_and_responses_share_the_original_controlled_reply(self):
        for path in ['/v1/chat/completions', '/v1/responses']:
            result = self.request(path, {'messages': [{'role': 'user', 'content': 'unique owned input'}]})
            frames = [json.loads(line[6:]) for line in result.splitlines()
                      if line.startswith('data: ') and line != 'data: [DONE]']
            self.assertTrue(frames)
            if path.endswith('/responses'):
                self.assertEqual(frames[-1]['response']['output'][0]['content'][0]['text'], 'ENDPOINT_REPLY_OK')
            else:
                self.assertEqual(''.join(frame['choices'][0]['delta'].get('content', '') for frame in frames), 'ENDPOINT_REPLY_OK')
        self.assertEqual(self.paths, ['/v1/chat/completions', '/v1/responses'])

    def test_working_directory_keeps_ssd_spaces_through_actual_sse(self):
        path = '/Volumes/HP P900/dev/项目/child'
        result = self.request('/v1/chat/completions', {'messages': [{'role': 'user', 'content': 'ENDPOINT_CHANGE_DIRECTORY='+path}]})
        first = json.loads(result.splitlines()[0][6:])
        call = first['choices'][0]['delta']['tool_calls'][0]
        self.assertEqual(call['function']['name'], 'working_directory')
        self.assertEqual(json.loads(call['function']['arguments']), {'cd': path})

    def test_explicit_error_status_remains_a_failure(self):
        with self.assertRaises(urllib.error.HTTPError) as caught:
            self.request('/v1/responses', {'input': [{'role': 'user', 'content': 'ENDPOINT_ERROR_NATIVE_PROOF'}]})
        self.assertEqual(caught.exception.code, 400)
        caught.exception.close()

    def test_profiles_do_not_share_request_storage(self):
        other = []
        handler = provider_handler(workspace=lambda: Path('/other'), requests=other)
        self.assertIsNot(handler, self.server.RequestHandlerClass)
        self.request('/v1/chat/completions', {'messages': []})
        self.assertEqual(other, [])
        self.assertEqual(self.paths, ['/v1/chat/completions'])
