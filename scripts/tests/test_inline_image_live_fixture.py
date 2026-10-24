"""Focused tests for independent real-provider image observation."""
import base64
import copy
import hashlib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from inline_image_live_fixture import (InlineImageProvider, inspect_user_request,
    validate_public_image_read)

PNG = bytes([
    137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,2,0,0,0,144,119,83,222,
    0,0,0,12,73,68,65,84,120,156,99,96,96,96,0,0,0,4,0,1,246,23,56,85,0,0,0,0,73,69,78,68,174,66,96,130])
PNG_URI = 'data:image/png;base64,' + base64.b64encode(PNG).decode()


class InlineImageLiveFixtureTests(unittest.TestCase):
    def dsh_request(self, text=True):
        handle = ('Image "中文图片.png" (sha256:c47dd9465c00e9a0c8b85e9ea58d3034a0d23b9cf926113602f3460752a4eb96); '
                  'request preview 1x1px. It may be resized or re-encoded; source dimensions, format, and byte size may differ.')
        parts = ([{'type': 'text', 'text': '请描述这张图片。'}] if text else [])
        parts.extend([{'type': 'text', 'text': handle}, {'type': 'image_url', 'image_url': {'url': PNG_URI}}])
        return {'messages': [{'role': 'user', 'content': parts},
            {'role': 'user', 'content': 'Current runtime context. This snapshot supersedes earlier runtime-context snapshots.\n\nowned context'},
            {'role': 'user', 'content': 'Time sampled while preparing turn 1, step 1: owned time'}]}

    def test_dsh_public_context_suffixes_and_image_handle_preserve_input_shape(self):
        for text in [True, False]:
            facts = inspect_user_request(self.dsh_request(text), dsh=True)
            self.assertEqual(facts['hasText'], text)
            self.assertEqual(facts['contentOrder'], ['text', 'image'] if text else ['image'])
            self.assertEqual(facts['imageHandleCount'], 1)
            self.assertEqual(facts['hostContextSuffixes'], ['runtime-context', 'time-context'])

    def test_dsh_does_not_select_history_image_before_new_human_input(self):
        body = self.dsh_request()
        body['messages'].append({'role': 'user', 'content': 'new human message without an image'})
        with self.assertRaises(ValueError):
            inspect_user_request(body, dsh=True)

    def test_dsh_title_is_counted_as_an_explicit_auxiliary_request(self):
        provider = InlineImageProvider(dsh=True)
        title = {'messages': [
            {'role': 'system', 'content': 'Create a concise title for an AI coding-assistant session from the supplied human messages.'},
            {'role': 'user', 'content': 'Generate the session title from this JSON array of human messages:\n[{"seq":9,"text":"请描述这张图片。"}]'}]}
        self.assertIsNone(provider.observe(title))
        provider.observe(self.dsh_request(True))
        provider.observe(self.dsh_request(False))
        receipt = provider.receipt()
        self.assertEqual(receipt['requestCount'], 2)
        self.assertEqual(receipt['wireRequestCount'], 3)
        self.assertEqual(receipt['auxiliaryRequests'], [{'kind': 'first-prompt-title'}])
        with self.assertRaises(ValueError):
            provider.observe(title)
        with self.assertRaises(ValueError):
            provider.receipt()

    def test_arbitrary_text_request_is_not_treated_as_a_title(self):
        provider = InlineImageProvider(dsh=True)
        with self.assertRaises(ValueError):
            provider.observe({'messages': [{'role': 'user', 'content': 'Generate a title'}]})
        self.assertEqual(provider.auxiliary, [])

    def test_native_responses_image_and_text_order(self):
        facts = inspect_user_request({'input': [{'role': 'user', 'content': [
            {'type': 'input_text', 'text': '请描述图片'},
            {'type': 'input_image', 'image_url': PNG_URI}]}]})
        self.assertEqual(facts['contentOrder'], ['text', 'image'])
        self.assertEqual(facts['images'][0]['mime'], 'image/png')
        self.assertEqual(facts['images'][0]['bytes'], 69)

    def test_pi_chat_image_url_and_latest_user_only(self):
        facts = inspect_user_request({'messages': [
            {'role': 'user', 'content': [{'type': 'image_url', 'image_url': {'url': PNG_URI}}]},
            {'role': 'assistant', 'content': 'answer'},
            {'role': 'user', 'content': [{'type': 'text', 'text': 'text'},
                {'type': 'image_url', 'image_url': {'url': PNG_URI}}]}]})
        self.assertTrue(facts['hasText'])
        self.assertEqual(facts['contentOrder'], ['text', 'image'])
        self.assertEqual(facts['images'][0]['sha256'], __import__('hashlib').sha256(PNG).hexdigest())

    def test_provider_requires_two_distinct_current_user_prompt_shapes(self):
        provider = InlineImageProvider()
        provider.observe({'messages': [{'role': 'user', 'content': [
            {'type': 'text', 'text': 'describe'}, {'type': 'image_url', 'image_url': {'url': PNG_URI}}]}]})
        provider.observe({'messages': [{'role': 'user', 'content': [
            {'type': 'image_url', 'image_url': {'url': PNG_URI}}]}]})
        receipt = provider.receipt()
        self.assertEqual(receipt['requestCount'], 2)
        self.assertEqual([row['hasText'] for row in receipt['requests']], [True, False])
        self.assertFalse(receipt['providerBytesDifferBetweenPrompts'])
        self.assertFalse(receipt['requestBodiesPersisted'])

    def test_extra_retry_cannot_be_reported_as_two_successful_prompts(self):
        provider = InlineImageProvider()
        mixed = {'messages': [{'role': 'user', 'content': [
            {'type': 'text', 'text': 'describe'}, {'type': 'image_url', 'image_url': {'url': PNG_URI}}]}]}
        image = {'messages': [{'role': 'user', 'content': [
            {'type': 'image_url', 'image_url': {'url': PNG_URI}}]}]}
        provider.observe(mixed)
        provider.observe(image)
        with self.assertRaises(ValueError):
            provider.observe(image)
        with self.assertRaises(ValueError):
            provider.receipt()
        self.assertEqual(provider.attempts, 3)

    def test_rejects_wrong_mime_malformed_base64_and_missing_image(self):
        bad = [
            {'messages': [{'role': 'user', 'content': [{'type': 'image_url', 'image_url': {
                'url': PNG_URI.replace('image/png', 'image/jpeg')}}]}]},
            {'input': [{'role': 'user', 'content': [{'type': 'input_image',
                'image_url': 'data:image/png;base64,%%%'}]}]},
            {'messages': [{'role': 'user', 'content': 'text only'}]},
        ]
        for body in bad:
            with self.subTest(body=body), self.assertRaises(ValueError):
                inspect_user_request(body)

    def test_rejects_history_image_as_a_substitute_for_new_prompt_image(self):
        provider = InlineImageProvider()
        with self.assertRaises(ValueError):
            provider.observe({'messages': [{'role': 'user', 'content': 'previous text'},
                {'role': 'assistant', 'content': 'old image attached'},
                {'role': 'user', 'content': 'new prompt without attachment'}]})
        self.assertEqual(provider.requests, [])


    def public_read(self, kind='dsh'):
        digest = hashlib.sha256(PNG).hexdigest()
        attachment_id = digest if kind == 'pi' else 'sha256:' + digest
        info = {'attachmentId': attachment_id, 'mediaType': 'image/png',
                'bytes': len(PNG), 'width': 1, 'height': 1}
        if kind != 'native':
            info['name'] = '中文图片.png'
        raw = {'type': 'image', 'attachment': copy.deepcopy(info)} if kind == 'dsh' else {'type': 'image'}
        evidence = {'publicByteRead': True, 'attachmentId': attachment_id,
                    'rawTranscriptAttachment': copy.deepcopy(raw),
                    'read': {'attachment': info, 'data': base64.b64encode(PNG).decode()}}
        return evidence, raw

    def test_all_three_public_byte_read_dtos_keep_their_original_id_forms(self):
        for kind in ('native', 'pi', 'dsh'):
            evidence, raw = self.public_read(kind)
            validate_public_image_read(kind, evidence, raw, PNG, '中文图片.png')

    def test_dsh_unsupported_or_unobserved_byte_read_is_not_accepted(self):
        for flag in (False, None):
            evidence, raw = self.public_read()
            evidence['publicByteRead'] = flag
            with self.assertRaisesRegex(ValueError, 'source binding'):
                validate_public_image_read('dsh', evidence, raw, PNG, '中文图片.png')

    def test_dsh_wrong_bytes_mime_id_dimensions_and_name_fail(self):
        for field, wrong in (('attachmentId', 'other-id'), ('mediaType', 'image/jpeg'),
                             ('bytes', len(PNG) + 1), ('width', 2), ('height', 2),
                             ('name', 'other.png')):
            evidence, raw = self.public_read()
            evidence['read']['attachment'][field] = wrong
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_public_image_read('dsh', evidence, raw, PNG, '中文图片.png')
        evidence, raw = self.public_read()
        evidence['read']['data'] = base64.b64encode(b'wrong bytes').decode()
        with self.assertRaises(ValueError):
            validate_public_image_read('dsh', evidence, raw, PNG, '中文图片.png')

    def test_byte_read_cannot_substitute_another_raw_source_or_endpoint(self):
        evidence, raw = self.public_read()
        changed = copy.deepcopy(raw)
        changed['attachment']['attachmentId'] = 'another-source'
        with self.assertRaises(ValueError):
            validate_public_image_read('dsh', evidence, changed, PNG, '中文图片.png')
        with self.assertRaises(ValueError):
            validate_public_image_read('unknown', evidence, raw, PNG, '中文图片.png')



if __name__ == '__main__':
    unittest.main()
