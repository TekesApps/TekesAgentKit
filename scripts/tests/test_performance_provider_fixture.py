"""Offline contract checks for the frozen performance provider fixture."""

import hashlib
import http.server
import json
from pathlib import Path
import sys
import unittest
import threading
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from performance_provider_fixture import (
    BODY_BYTES,
    RequestStats,
    body_bytes,
    body_text,
    content_digest_manifest,
    create_provider_handler,
    expected_tool_call_id,
    measure_commands,
    provider_frames,
    read_source_bytes,
    read_source_path,
    seed_commands,
    validate_history,
)
from native_dsh_mixed_fixture import TITLE_PREFIX, TITLE_SYSTEM_PREFIX


class PerformanceProviderFixtureTests(unittest.TestCase):
    def test_responses_server_chain_preserves_issued_call_and_allows_next_user(self):
        stats = RequestStats()
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
            create_provider_handler('/tmp/perf', stats, endpoint='native'))
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        tool = {'type': 'function', 'name': 'read', 'parameters': {
            'properties': {'path': {}}, 'required': ['path']}}
        def post(body):
            request = urllib.request.Request(f'http://127.0.0.1:{server.server_port}/v1/responses',
                data=json.dumps(body).encode(), headers={'Content-Type': 'application/json'})
            with urllib.request.urlopen(request, timeout=2) as response:
                return response.read().decode()
        try:
            first = post({'input': [{'role': 'user', 'content': body_text('seed', 0, 1, 'user')}],
                          'tools': [tool]})
            self.assertIn('function_call', first)
            final = post({'previous_response_id': 'akperf-seed-s0-t01', 'input': [
                {'type': 'function_call_output', 'call_id': expected_tool_call_id(0, 1),
                 'output': read_source_bytes().decode()}], 'tools': [tool]})
            self.assertIn('final_answer', final)
            next_turn = post({'previous_response_id': 'akperf-seed-s0-t01', 'input': [
                {'type': 'message', 'role': 'user', 'content': body_text('seed', 0, 2, 'user')}],
                'tools': [tool]})
            self.assertIn(expected_tool_call_id(0, 2), next_turn)
            self.assertEqual(stats.snapshot()['businessRequests'], 3)
            self.assertTrue(stats.snapshot()['requests'][1]['serverChainContextRestored'])
            self.assertFalse(stats.snapshot()['requests'][2]['serverChainContextRestored'])
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)

    def test_bodies_are_deterministic_utf8_exactly_1024_bytes_with_fixed_header(self):
        for phase, rounds in (("seed", 50), ("measure", 10)):
            for session in (0, 1):
                for turn in range(1, rounds + 1):
                    for role in ("user", "assistant"):
                        first = body_bytes(phase, session, turn, role)
                        self.assertEqual(len(first), BODY_BYTES)
                        self.assertEqual(first, body_bytes(phase, session, turn, role))
                        text = first.decode("utf-8")
                        self.assertTrue(text.startswith(f"AKPERF|{phase}|s{session}|t{turn:02d}|{role}|"))
                        self.assertEqual(text, body_text(phase, session, turn, role))
                        self.assertIn("性能固定正文", text)

    def test_seed_and_measure_commands_use_only_public_compose_and_settlement(self):
        commands = seed_commands("session-a", "endpoint-test", "model-test", 0)
        self.assertEqual(len(commands), 102)
        self.assertEqual(commands[0], {"op": "call", "sessionId": "session-a",
                                      "method": "session.selectModel",
                                      "args": {"provider": "endpoint-test", "model": "model-test"}})
        self.assertEqual(commands[1], {"op": "open", "sessionId": "session-a"})
        for turn in range(1, 51):
            compose, settle = commands[2 + (turn - 1) * 2:4 + (turn - 1) * 2]
            expected = body_text("seed", 0, turn, "user")
            self.assertEqual(compose, {"op": "compose", "sessionId": "session-a", "text": expected})
            self.assertEqual(settle, {"op": "waitSettled", "sessionId": "session-a", "inputText": expected})
        warm = measure_commands("session-b", 1, 10)
        self.assertEqual(warm[0]["text"], body_text("measure", 1, 10, "user"))
        self.assertEqual(warm[1]["inputText"], warm[0]["text"])
        self.assertEqual([item["op"] for item in warm], ["compose", "waitSettled"])
        initialization = measure_commands("session-b", 1, 0)
        self.assertTrue(initialization[0]["text"].startswith("AKPERF|measure|s1|t00|user|"))
        self.assertEqual(len(content_digest_manifest(1, "measure", 11, first_turn=0)), 22)
        self.assertEqual(read_source_path("/tmp/perf").name, "performance-read-proof.txt")
        self.assertIn(b"AKPERF_PUBLIC_READ_SOURCE|fixture-v1", read_source_bytes())

    def test_content_manifest_and_public_history_readback_validate_all_bodies_and_tool_pairs(self):
        manifest = content_digest_manifest(0)
        rows = []
        for turn in range(1, 51):
            rows.extend((
                {"role": "user", "text": body_text("seed", 0, turn, "user")},
                {"role": "assistant", "text": body_text("seed", 0, turn, "assistant"), "isFinal": True},
            ))
        tools = [{"turn": turn, "toolName": "read", "callId": expected_tool_call_id(0, turn),
                  "resultCallId": expected_tool_call_id(0, turn), "resultText": f"actual public read output {turn}"}
                 for turn in range(1, 11)]
        receipt = validate_history(rows, tools, 0)
        self.assertEqual(len(manifest), 100)
        self.assertEqual(receipt["bodyMessageCount"], 100)
        self.assertEqual(receipt["toolGroupCount"], 10)
        self.assertEqual(receipt["bodyRows"][0]["sha256"], hashlib.sha256(rows[0]["text"].encode()).hexdigest())
        self.assertEqual(receipt["toolGroups"][0]["resultSha256"], hashlib.sha256(b"actual public read output 1").hexdigest())
        serialized = json.dumps(receipt)
        self.assertNotIn(rows[0]["text"], serialized)
        self.assertNotIn(tools[0]["resultText"], serialized)

    def test_chat_tool_call_waits_for_matching_public_result_before_final_body(self):
        prompt = body_text("seed", 0, 1, "user")
        call_id = expected_tool_call_id(0, 1)
        schema = {"type": "function", "function": {"name": "read", "parameters": {
            "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}}
        body = {"messages": [{"role": "user", "content": prompt}], "tools": [schema]}
        frames, receipt, title = provider_frames(body, path_kind="chat", workspace="/tmp/perf", endpoint="pi")
        calls = [call for frame in frames for choice in frame.get("choices", [])
                 for call in choice.get("delta", {}).get("tool_calls", [])]
        self.assertFalse(title)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]["id"], call_id)
        self.assertEqual(json.loads(calls[0]["function"]["arguments"]), {
            "path": str(read_source_path("/tmp/perf"))})
        self.assertEqual(receipt["callIds"], [call_id])
        self.assertEqual(receipt["resultCallIds"], [])

        continuation = {"messages": [
            {"role": "user", "content": prompt},
            {"role": "assistant", "tool_calls": [{"id": call_id, "type": "function",
             "function": {"name": "read", "arguments": calls[0]["function"]["arguments"]}}]},
            {"role": "tool", "tool_call_id": call_id, "content": "real harness read result"},
        ], "tools": [schema]}
        final_frames, final_receipt, _ = provider_frames(continuation, path_kind="chat",
                                                         workspace="/tmp/perf", endpoint="pi")
        text = "".join(choice.get("delta", {}).get("content", "")
                       for frame in final_frames for choice in frame.get("choices", []))
        self.assertEqual(text, body_text("seed", 0, 1, "assistant"))
        self.assertEqual(len(text.encode("utf-8")), BODY_BYTES)
        self.assertEqual(final_receipt["callIds"], [])
        self.assertEqual(final_receipt["resultCallIds"], [call_id])

    def test_responses_tool_call_uses_advertised_schema_and_requires_original_output(self):
        prompt = body_text("seed", 1, 2, "user")
        call_id = expected_tool_call_id(1, 2)
        tool = {"type": "function", "name": "read", "parameters": {
            "type": "object", "properties": {"file_path": {"type": "string"},
                                                   "offset": {"type": "integer"},
                                                   "limit": {"type": "integer"}},
            "required": ["file_path", "offset", "limit"]}}
        body = {"input": [{"role": "user", "content": [{"type": "input_text", "text": prompt}]}],
                "tools": [tool]}
        frames, _, _ = provider_frames(body, path_kind="responses", workspace="/tmp/perf", endpoint="native")
        call = next(frame["item"] for frame in frames if frame.get("item", {}).get("type") == "function_call")
        self.assertEqual(call["call_id"], call_id)
        self.assertEqual(json.loads(frames[-2]["arguments"]), {
            "file_path": str(read_source_path("/tmp/perf")), "offset": 1, "limit": 4})
        continuation = {"input": [
            {"role": "user", "content": [{"type": "input_text", "text": prompt}]},
            {"type": "function_call", "call_id": call_id, "name": "read", "arguments": "{}"},
            {"type": "function_call_output", "call_id": call_id, "output": "public read bytes"},
        ], "tools": [tool]}
        final, receipt, _ = provider_frames(continuation, path_kind="responses", workspace="/tmp/perf", endpoint="native")
        self.assertEqual(final[-1]["response"]["output"][0]["content"][0]["text"],
                         body_text("seed", 1, 2, "assistant"))
        self.assertEqual(receipt["resultCallIds"], [call_id])
        missing = dict(continuation, input=continuation["input"][:-1])
        with self.assertRaisesRegex(ValueError, "no returned public tool result"):
            provider_frames(missing, path_kind="responses", workspace="/tmp/perf", endpoint="native")

    def test_editor_fallback_uses_only_advertised_view_command(self):
        prompt = body_text("seed", 0, 3, "user")
        body = {"messages": [{"role": "user", "content": prompt}], "tools": [{"type": "function",
            "function": {"name": "str_replace_editor", "parameters": {"type": "object",
                "properties": {"path": {"type": "string"}, "command": {"type": "string"}},
                "required": ["path", "command"]}}}]}
        frames, _, _ = provider_frames(body, path_kind="chat", workspace="/tmp/perf", endpoint="dsh")
        call = next(call for frame in frames for choice in frame.get("choices", [])
                    for call in choice.get("delta", {}).get("tool_calls", []))
        self.assertEqual(call["function"]["name"], "str_replace_editor")
        self.assertEqual(json.loads(call["function"]["arguments"]), {
            "path": str(read_source_path("/tmp/perf")), "command": "view"})

    def test_readback_rejects_missing_pages_wrong_content_and_bad_tool_pairs(self):
        rows = []
        for turn in range(1, 51):
            rows.extend(({"role": "user", "text": body_text("seed", 0, turn, "user")},
                         {"role": "assistant", "text": body_text("seed", 0, turn, "assistant"), "isFinal": True}))
        tools = [{"toolName": "read", "callId": expected_tool_call_id(0, turn),
                  "resultCallId": expected_tool_call_id(0, turn), "resultText": "read result"}
                 for turn in range(1, 11)]
        with self.assertRaisesRegex(ValueError, "row count"):
            validate_history(rows[:50], tools, 0)
        changed = list(rows)
        changed[0] = {"role": "user", "text": "not the canonical body"}
        with self.assertRaisesRegex(ValueError, "differs at row 0"):
            validate_history(changed, tools, 0)
        bad_tools = list(tools)
        bad_tools[0] = dict(bad_tools[0], resultCallId="unknown-call")
        with self.assertRaisesRegex(ValueError, "pair"):
            validate_history(rows, bad_tools, 0)

    def test_unknown_inputs_and_unadvertised_tools_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "unknown performance fixture input"):
            provider_frames({"messages": [{"role": "user", "content": "ordinary unknown prompt"}]},
                            path_kind="chat", workspace="/tmp/perf", endpoint="pi")
        body = {"messages": [{"role": "user", "content": body_text("seed", 0, 1, "user")}], "tools": []}
        with self.assertRaisesRegex(ValueError, "advertised public read tool"):
            provider_frames(body, path_kind="chat", workspace="/tmp/perf", endpoint="pi")
        with self.assertRaisesRegex(ValueError, "unsupported required argument"):
            provider_frames({"messages": [{"role": "user", "content": body_text("seed", 0, 1, "user")}],
                "tools": [{"function": {"name": "read", "parameters": {"properties": {"path": {}},
                "required": ["path", "unrecognized"]}}}]}, path_kind="chat", workspace="/tmp/perf", endpoint="pi")

    def test_dsh_title_is_classified_by_strict_published_request_shape(self):
        source = "ordinary public prompt"
        title_body = {"messages": [
            {"role": "system", "content": TITLE_SYSTEM_PREFIX + " Use a concise title."},
            {"role": "user", "content": TITLE_PREFIX + json.dumps([{"seq": 1, "text": source}])},
        ]}
        frames, receipt, is_title = provider_frames(title_body, path_kind="chat", workspace="/tmp/perf", endpoint="dsh")
        self.assertTrue(is_title)
        self.assertIn("Performance workload", "".join(
            choice.get("delta", {}).get("content", "")
            for frame in frames for choice in frame.get("choices", [])))
        stats = RequestStats()
        stats.record(receipt, title=is_title)
        snapshot = stats.snapshot()
        self.assertEqual(snapshot["titleRequests"], 1)
        self.assertEqual(snapshot["businessRequests"], 0)
        self.assertNotIn(source, json.dumps(snapshot))

        # A body that only mentions the title route is an ordinary prompt, not a title call.
        ordinary = {"messages": [{"role": "user", "content": body_text("seed", 0, 1, "user")}]}
        with self.assertRaisesRegex(ValueError, "advertised public read tool"):
            provider_frames(ordinary, path_kind="chat", workspace="/tmp/perf", endpoint="dsh")

    def test_handler_factory_and_stats_keep_only_digest_metadata(self):
        stats = RequestStats()
        handler = create_provider_handler("/tmp/perf", stats, endpoint="pi")
        self.assertTrue(issubclass(handler, __import__("http.server").server.BaseHTTPRequestHandler))
        secret = body_text("measure", 0, 1, "user")
        receipt = {"pathKind": "chat", "bodySha256": [hashlib.sha256(secret.encode()).hexdigest()],
                   "roles": ["user"], "callIds": [], "resultCallIds": []}
        stats.record(receipt)
        serialized = json.dumps(stats.snapshot())
        self.assertIn(hashlib.sha256(secret.encode()).hexdigest(), serialized)
        self.assertNotIn(secret, serialized)


if __name__ == "__main__":
    unittest.main()
