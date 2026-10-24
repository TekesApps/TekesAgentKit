"""Provider observation and conservative validation for inline image probes."""
from __future__ import annotations

import base64
import binascii
import hashlib
import re
import subprocess
import threading
from pathlib import Path


def _user_messages(body):
    history = body.get('input', body.get('messages'))
    if not isinstance(history, list):
        return []
    return [item for item in history if isinstance(item, dict) and item.get('role') == 'user']


def _parts(message):
    content = message.get('content')
    if isinstance(content, str):
        return [{'type': 'text', 'text': content}]
    return content if isinstance(content, list) else []


def _mime_for_bytes(data):
    if data.startswith(b'\x89PNG\r\n\x1a\n'):
        return 'image/png'
    if data.startswith(b'\xff\xd8\xff'):
        return 'image/jpeg'
    if data.startswith((b'GIF87a', b'GIF89a')):
        return 'image/gif'
    if (len(data) >= 12 and data[:4] == b'RIFF' and data[8:12] == b'WEBP'):
        return 'image/webp'
    return None


def _image_uri(part):
    if part.get('type') == 'image_url' and isinstance(part.get('image_url'), dict):
        uri = part['image_url'].get('url')
    elif part.get('type') == 'input_image':
        uri = part.get('image_url')
    else:
        return None
    if not isinstance(uri, str):
        raise ValueError('image part has no data URI')
    match = re.fullmatch(r'data:(image/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/]*={0,2})', uri)
    if not match:
        raise ValueError('provider image is not a supported base64 data URI')
    declared, payload = match.groups()
    try:
        data = base64.b64decode(payload, validate=True)
    except (binascii.Error, ValueError):
        raise ValueError('provider image base64 is malformed') from None
    actual = _mime_for_bytes(data)
    if not data or actual is None or actual != declared:
        raise ValueError('provider image MIME does not match image signature')
    return {'mime': declared, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
            '_data': data}


def inspect_user_request(body, *, dsh=False):
    """Return text/image order facts for the most recent actual user message."""
    messages = _user_messages(body)
    suffixes = []
    if dsh:
        # Published DSH 0.2.1-rc.2 projects these two host-generated context
        # events as user messages. Never skip arbitrary later human input.
        while messages and isinstance(messages[-1].get('content'), str):
            text = messages[-1]['content']
            if text.startswith('Current runtime context. This snapshot supersedes earlier runtime-context snapshots.\n\n'):
                suffixes.append('runtime-context')
            elif re.match(r'^Time sampled while preparing turn [0-9]+, step [0-9]+: ', text):
                suffixes.append('time-context')
            else:
                break
            messages.pop()
        if len(suffixes) != len(set(suffixes)) or len(suffixes) > 2:
            raise ValueError('unexpected repeated DSH context suffix')
    if not messages:
        raise ValueError('provider request has no user message')
    parts = _parts(messages[-1])
    observed, images, text_present, handles = [], [], False, 0
    for part in parts:
        if not isinstance(part, dict):
            continue
        if part.get('type') in ('text', 'input_text') and isinstance(part.get('text'), str):
            if dsh and re.match(r'^Image "中文图片\.png" \(sha256:c47dd9465c00e9a0c8b85e9ea58d3034a0d23b9cf926113602f3460752a4eb96\); request preview [0-9]+x[0-9]+px\.', part['text']):
                handles += 1
                continue
            text_present = text_present or bool(part['text'])
            observed.append('text')
            continue
        image = _image_uri(part)
        if image is not None:
            images.append(image)
            observed.append('image')
    if len(images) != 1:
        raise ValueError('each probe request must contain exactly one provider image')
    if dsh and handles != 1:
        raise ValueError('DSH request did not contain its exact published image handle')
    return {'hasText': text_present, 'contentOrder': observed, 'images': images,
            'imageHandleCount': handles, 'hostContextSuffixes': list(reversed(suffixes))}


def is_owned_dsh_title(body):
    history = body.get('messages', [])
    system = next((item.get('content') for item in history
                   if isinstance(item, dict) and item.get('role') == 'system'), None)
    users = _user_messages(body)
    prefix = 'Generate the session title from this JSON array of human messages:\n'
    if (not isinstance(system, str) or not system.startswith(
            'Create a concise title for an AI coding-assistant session from the supplied human messages.')
            or len(users) != 1 or not isinstance(users[0].get('content'), str)
            or not users[0]['content'].startswith(prefix)):
        return False
    import json
    try:
        framed = json.loads(users[0]['content'][len(prefix):])
    except ValueError:
        return False
    return (isinstance(framed, list) and len(framed) == 1
            and isinstance(framed[0], dict) and type(framed[0].get('seq')) is int
            and framed[0].get('text') == '请描述这张图片。')


def decode_image_with_imageio(data, mime, directory, label):
    """Preserve source bytes and use ImageIO to transcode them into PNG evidence."""
    if mime not in ('image/png', 'image/jpeg', 'image/gif', 'image/webp'):
        raise ValueError('unsupported image MIME')
    extension = {'image/png': '.png', 'image/jpeg': '.jpg',
                 'image/gif': '.gif', 'image/webp': '.webp'}[mime]
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / (label + '-provider-image' + extension)
    decoded_path = directory / (label + '-imageio-decoded.png')
    with path.open('xb') as stream:
        stream.write(data)
    converted = subprocess.run(['/usr/bin/sips', '-s', 'format', 'png', str(path),
                                '--out', str(decoded_path)], text=True,
                               capture_output=True, timeout=20, check=False)
    (directory / (label + '-imageio-convert.stdout')).write_text(converted.stdout, encoding='utf-8')
    (directory / (label + '-imageio-convert.stderr')).write_text(converted.stderr, encoding='utf-8')
    if converted.returncode != 0 or not decoded_path.is_file() or decoded_path.stat().st_size == 0:
        raise ValueError('ImageIO could not decode and transcode provider image')
    decoded = decoded_path.read_bytes()
    if not decoded.startswith(b'\x89PNG\r\n\x1a\n'):
        raise ValueError('ImageIO output is not a PNG image')
    metadata = subprocess.run(['/usr/bin/sips', '-g', 'format', '-g', 'pixelWidth',
        '-g', 'pixelHeight', str(decoded_path)], text=True, capture_output=True,
        timeout=15, check=False)
    (directory / (label + '-imageio-metadata.stdout')).write_text(metadata.stdout, encoding='utf-8')
    (directory / (label + '-imageio-metadata.stderr')).write_text(metadata.stderr, encoding='utf-8')
    width = re.search(r'pixelWidth:\s*(\d+)', metadata.stdout)
    height = re.search(r'pixelHeight:\s*(\d+)', metadata.stdout)
    if (metadata.returncode != 0 or not re.search(r'\sformat:\s*png\s*$', metadata.stdout, re.M | re.I)
            or not width or not height or int(width.group(1)) <= 0 or int(height.group(1)) <= 0):
        raise ValueError('ImageIO transcode output failed PNG metadata validation')
    return {'width': int(width.group(1)), 'height': int(height.group(1)),
            'providerImageEvidence': str(path), 'decodedPngEvidence': str(decoded_path),
            'decodedPngSha256': hashlib.sha256(decoded).hexdigest(),
            'imageIoTranscoded': True}


class InlineImageProvider:
    """A synthetic streaming provider that retains image metadata only."""
    def __init__(self, *, dsh=False):
        self.lock = threading.Lock()
        self.requests = []
        self.attempts = 0
        self.rejections = []
        self.dsh = dsh
        self.auxiliary = []

    def observe(self, body):
        with self.lock:
            self.attempts += 1
            if self.dsh and is_owned_dsh_title(body):
                if self.auxiliary:
                    raise ValueError('unexpected repeated DSH title generation')
                self.auxiliary.append({'kind': 'first-prompt-title'})
                return None
        facts = inspect_user_request(body, dsh=self.dsh)
        with self.lock:
            index = len(self.requests)
            if index >= 2:
                raise ValueError('unexpected extra provider request')
            expected_text = index == 0
            if facts['hasText'] is not expected_text:
                raise ValueError('provider request text/image scenario order mismatch')
            expected_order = ['text', 'image'] if expected_text else ['image']
            if facts['contentOrder'] != expected_order:
                raise ValueError('provider image content order does not match the requested scenario')
            row = {'requestIndex': index + 1, 'hasText': facts['hasText'],
                   'imageHandleCount': facts['imageHandleCount'],
                   'hostContextSuffixes': facts['hostContextSuffixes'],
                   'contentOrder': facts['contentOrder'], 'images': [
                       {key: image[key] for key in ('mime', 'bytes', 'sha256')}
                       for image in facts['images']]}
            self.requests.append(row)
        return index

    def receipt(self):
        with self.lock:
            if (self.attempts != 2 + len(self.auxiliary) or len(self.requests) != 2
                    or self.rejections):
                raise ValueError('provider did not receive exactly two image prompts')
            return {'requestCount': len(self.requests), 'requests': list(self.requests),
                    'wireRequestCount': self.attempts, 'auxiliaryRequests': list(self.auxiliary),
                    'providerBytesDifferBetweenPrompts': self.requests[0]['images'][0]['sha256'] !=
                    self.requests[1]['images'][0]['sha256'],
                    'requestBodiesPersisted': False}


def stream_response(path, body):
    """Return OpenAI-compatible successful completion frames."""
    if path.endswith('/responses'):
        value = {'type': 'response.completed', 'response': {'id': 'inline-image-probe',
            'status': 'completed', 'output': [{'type': 'message', 'role': 'assistant',
            'phase': 'final_answer', 'status': 'completed', 'content': [
                {'type': 'output_text', 'text': 'ENDPOINT_REPLY_OK'}]}]}}
        return [value]
    base = {'id': 'inline-image-probe', 'object': 'chat.completion.chunk',
            'created': 1, 'model': 'endpoint-test'}
    return [dict(base, choices=[{'index': 0, 'delta': {'role': 'assistant',
        'content': 'ENDPOINT_REPLY_OK'}, 'finish_reason': None}]),
        dict(base, choices=[{'index': 0, 'delta': {}, 'finish_reason': 'stop'}],
             usage={'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2})]
