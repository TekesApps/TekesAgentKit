import asyncio
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from performance_kernel_wire_probe import KernelWireProbe, PassiveObserver, parse_loopback_http_url


def masked_frame(opcode, payload, *, fin=True, mask=b'\x10\x20\x30\x40'):
    first = (0x80 if fin else 0) | opcode
    if len(payload) < 126:
        header = bytes((first, 0x80 | len(payload)))
    else:
        header = bytes((first, 0x80 | 126)) + len(payload).to_bytes(2, 'big')
    encoded = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
    return header + mask + encoded


class KernelWireProbeTests(unittest.IsolatedAsyncioTestCase):
    def test_only_explicit_loopback_http_urls_are_accepted(self):
        self.assertEqual(parse_loopback_http_url('http://127.0.0.1:1234/v1'), ('127.0.0.1', 1234))
        for url in ('https://127.0.0.1:1234', 'http://example.com:1234',
                    'http://127.0.0.1:0', 'http://user:pass@127.0.0.1:1234'):
            with self.subTest(url=url), self.assertRaises(ValueError):
                parse_loopback_http_url(url)

    def test_split_masked_fragment_is_decoded_without_unmasking_forward_bytes(self):
        events = []
        observer = PassiveObserver(events.append, 'client-to-kernel')
        observer.mode = 'websocket'
        message = json.dumps({'type': 'event', 'sessionId': 'session-secret',
                              'entry': {'event': {'type': 'user/message', 'data': {
                                  'text': 'must-not-be-recorded'}}}}, separators=(',', ':')).encode()
        split = 9
        frames = (masked_frame(1, message[:split], fin=False),
                  masked_frame(0, message[split:], fin=True, mask=b'\xaa\xbb\xcc\xdd'))
        wire = b''.join(frames)
        timestamp = 111
        for byte in wire:
            timestamp += 1
            observer.feed(bytes((byte,)), timestamp)
        rows = [row for row in events if row['kind'] == 'message']
        self.assertEqual(len(rows), 1)
        self.assertIn('user/message', rows[0]['fields']['labels'])
        self.assertEqual(rows[0]['fields']['sessionid'], hashlib.sha256(b'session-secret').hexdigest()[:16])
        self.assertEqual(rows[0]['startedMonotonicNs'], 126)
        self.assertNotIn('must-not-be-recorded', json.dumps(events))
        self.assertNotIn('session-secret', json.dumps(events))

    def test_split_content_length_json_request_records_only_safe_semantics(self):
        events = []
        observer = PassiveObserver(events.append, 'client-to-kernel')
        body = json.dumps({'method': 'session.prompt', 'rpcId': 'rpc-secret',
                           'params': {'sessionId': 'session-secret',
                                      'text': 'prompt-secret'}}, separators=(',', ':')).encode()
        message = (b'POST /v1/rpc HTTP/1.1\r\nAuthorization: Bearer header-secret\r\n'
                   + f'Content-Length: {len(body)}\r\nContent-Type: application/json\r\n\r\n'.encode()
                   + body)
        for index in range(0, len(message), 7):
            observer.feed(message[index:index + 7], 500 + index)
        rows = [row for row in events if row['kind'] == 'message']
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['fields']['label'], 'session.prompt')
        self.assertEqual(rows[0]['fields']['sessionid'], hashlib.sha256(b'session-secret').hexdigest()[:16])
        self.assertEqual(rows[0]['fields']['rpcid'], hashlib.sha256(b'rpc-secret').hexdigest()[:16])
        saved = json.dumps(events)
        for secret in ('header-secret', 'session-secret', 'rpc-secret', 'prompt-secret'):
            self.assertNotIn(secret, saved)

    async def test_proxy_forwards_handshake_and_masked_frames_byte_for_byte(self):
        request = (b'GET /v1/ws?session=route-secret HTTP/1.1\r\n'
                   b'Host: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
                   b'Authorization: Bearer credential-secret\r\n\r\n')
        body = (b'{"type":"event","sessionId":"session-secret","entry":{"event":'
                b'{"type":"user/message","data":{"text":"prompt-secret"}}}}')
        frames = (masked_frame(1, body[:11], fin=False),
                  masked_frame(0, body[11:], fin=True, mask=b'\xaa\xbb\xcc\xdd'))
        frame_bytes = b''.join(frames)
        seen = {"handshake": b"", "frames": b""}

        async def upstream(reader, writer):
            seen['handshake'] = await reader.readuntil(b'\r\n\r\n')
            writer.write(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n')
            await writer.drain()
            seen['frames'] = await reader.readexactly(len(frame_bytes))
            writer.write(seen['frames'])
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        server = await asyncio.start_server(upstream, '127.0.0.1', 0)
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory) / 'wire.jsonl'
            probe = KernelWireProbe(f'http://127.0.0.1:{server.sockets[0].getsockname()[1]}', evidence).start()
            try:
                reader, writer = await asyncio.open_connection('127.0.0.1', int(probe.url.rsplit(':', 1)[1]))
                for part in (request[:13], request[13:67], request[67:]):
                    writer.write(part); await writer.drain()
                response = await reader.readuntil(b'\r\n\r\n')
                self.assertIn(b'101 Switching Protocols', response)
                for part in (frame_bytes[:3], frame_bytes[3:17], frame_bytes[17:]):
                    writer.write(part); await writer.drain()
                echoed = await reader.readexactly(len(frame_bytes))
                self.assertEqual(echoed, frame_bytes)
                writer.close(); await writer.wait_closed()
                await asyncio.sleep(0.05)
            finally:
                probe.stop()
                server.close(); await server.wait_closed()

            self.assertEqual(seen['handshake'], request)
            self.assertEqual(seen['frames'], frame_bytes)
            records = [json.loads(line) for line in evidence.read_text().splitlines()]
            messages = [row for row in records if row['transport'] == 'websocket' and row['kind'] == 'message']
            self.assertEqual(len(messages), 2)  # Observer sees the byte-identical echo too.
            self.assertTrue(all('user/message' in row['fields']['labels'] for row in messages))
            saved = evidence.read_text()
            for secret in ('credential-secret', 'route-secret', 'session-secret', 'prompt-secret'):
                self.assertNotIn(secret, saved)


if __name__ == '__main__':
    unittest.main()
