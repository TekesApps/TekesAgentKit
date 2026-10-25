import sys
import hashlib
import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from performance_provider_fixture import body_text, expected_tool_call_id, read_source_bytes
from performance_public_readback import normalize_public_session


def _text_content(value):
    return [{"type": "text", "text": value}]


def _fixture(endpoint, index=0):
    session_id = f"perf-{endpoint}-s{index}"
    scope = {"endpointId": endpoint, "sessionId": session_id, "threadId": session_id}
    transcript_items = []
    transcript_tools = []
    if endpoint == "pi":
        entries = [{"id": "context", "message": {"role": "user", "content": _text_content("fixture context")}}]
        transcript_items.append({"kind": "system", "text": "fixture context", "isFinal": True,
                                 "isProvisional": False,
                                 "sourcePosition": {"kind": "native", "entryId": "context", "entryOrdinal": 0}})
        for turn in range(1, 51):
            turn_id = f"[{endpoint},{session_id},turn:{turn}]"
            for role in ("user", "assistant"):
                text = body_text("seed", index, turn, role)
                entry_id = f"body-{turn}-{role}"
                ordinal = len(entries)
                entries.append({"id": entry_id, "message": {"role": role, "content": _text_content(text)}})
                transcript_items.append({
                    "kind": role, "text": text, "isFinal": role == "assistant",
                    "isProvisional": False, "turnId": turn_id,
                    "sourcePosition": {"kind": "native", "entryId": entry_id,
                                        "entryOrdinal": ordinal, "contentIndex": 0},
                })
        for turn in range(1, 11):
            call_id = expected_tool_call_id(index, turn)
            call_entry_id, result_entry_id = f"call-{turn}", f"result-{turn}"
            call_ordinal = len(entries)
            entries.append({"id": call_entry_id, "message": {"role": "assistant", "content": [
                {"type": "toolCall", "id": call_id, "name": "read", "arguments": {"path": "proof.txt"}}
            ]}})
            result_ordinal = len(entries)
            result_text = read_source_bytes().decode("utf-8")
            entries.append({"id": result_entry_id, "message": {
                "role": "toolResult", "toolCallId": call_id, "toolName": "read",
                "content": _text_content(result_text),
            }})
            turn_id = f"[{endpoint},{session_id},turn:{turn}]"
            transcript_tools.append({
                "id": "opaque-presentation-id-must-not-be-decoded",
                "name": "read", "phase": "done", "turnId": turn_id,
                "sourcePosition": {"kind": "native", "entryId": call_entry_id,
                                    "entryOrdinal": call_ordinal, "contentIndex": 0},
                "resultPosition": {"kind": "native", "entryId": result_entry_id,
                                    "entryOrdinal": result_ordinal},
                "result": {"toolCallId": call_id, "content": _text_content(result_text)},
            })
        raw = {"native_conversation": {"sessionId": session_id, "entries": entries}}
    else:
        entries = {}

        def journal(seq, event_type, data):
            entries[str(seq)] = {"event": {"seq": seq, "type": event_type, "data": data}}

        seq = 0
        # Context rows are retained as source anchors, but do not count as bodies.
        journal(seq, "system/message", {"role": "system", "content": _text_content("fixture context")})
        transcript_items.append({"kind": "system", "text": "fixture context", "isFinal": True,
                                 "isProvisional": False,
                                 "sourcePosition": {"kind": "journal", "sequence": seq}})
        seq += 1
        for turn in range(1, 51):
            turn_id = f"[{endpoint},{session_id},turn:{turn}]"
            for role in ("user", "assistant"):
                text = body_text("seed", index, turn, role)
                if role == "user":
                    event_type, data = "user/message", {"role": role, "content": _text_content(text)}
                else:
                    event_type, data = "assistant/message", {"message": {
                        "role": role, "content": _text_content(text),
                    }}
                journal(seq, event_type, data)
                transcript_items.append({
                    "kind": role, "text": text, "isFinal": role == "assistant",
                    "isProvisional": False, "turnId": turn_id,
                    "sourcePosition": {"kind": "journal", "sequence": seq},
                })
                seq += 1
        for turn in range(1, 11):
            call_id = expected_tool_call_id(index, turn)
            turn_id = f"[{endpoint},{session_id},turn:{turn}]"
            if endpoint == "native":
                call_position = {"kind": "journal", "sequence": seq}
                journal(seq, "tool/call", {"callId": call_id, "name": "read", "turn": turn_id})
            else:
                call_position = {"kind": "journal", "sequence": seq, "contentIndex": 0}
                journal(seq, "assistant/message", {"message": {"role": "assistant", "content": [
                    {"type": "tool-call", "id": call_id, "name": "read", "arguments": {"path": "proof.txt"}},
                ]}, "turn": turn_id})
            seq += 1
            result_text = read_source_bytes().decode("utf-8")
            result_position = {"kind": "journal", "sequence": seq}
            journal(seq, "tool/result", {"message": {
                "role": "toolResult", "toolCallId": call_id, "toolName": "read",
                "content": _text_content(result_text),
            }, "turn": turn_id})
            seq += 1
            transcript_tools.append({
                "id": "opaque-presentation-id-must-not-be-decoded",
                "name": "read", "phase": "done", "turnId": turn_id,
                "sourcePosition": call_position, "resultPosition": result_position,
                "result": {"toolCallId": call_id, "content": _text_content(result_text)},
            })
        raw = {"entries": entries}

    snapshot = {
        "run": {"scope": scope},
        "transcript": {"hasMoreBefore": False, "items": transcript_items, "tools": transcript_tools},
    }
    return snapshot, raw, scope


class PublicReadbackTests(unittest.TestCase):
    def test_native_empty_tool_carrier_and_structured_source_result(self):
        snapshot, raw, scope = _fixture('native')
        turn = snapshot['transcript']['tools'][0]['turnId']
        raw['entries']['9999'] = {'event': {'seq': 9999, 'type': 'assistant/message',
            'data': {'message': {'role': 'assistant', 'content': []}}}}
        carrier = dict(kind='assistant', text='', isFinal=False, isProvisional=False,
            turnId=turn, sourcePosition={'kind': 'journal', 'sequence': 9999})
        snapshot['transcript']['items'].append(carrier)
        tool = snapshot['transcript']['tools'][0]
        message = raw['entries'][str(tool['resultPosition']['sequence'])]['event']['data']['message']
        message.pop('toolCallId', None)
        data = raw['entries'][str(tool['resultPosition']['sequence'])]['event']['data']
        call_id = data.pop('callId', expected_tool_call_id(0, 1))
        message['source'] = dict(kind='tool', callId=call_id)
        source = read_source_bytes()
        result = dict(bytes=len(source), sha256='sha256-'+hashlib.sha256(source).hexdigest(),
            truncated=False, total_lines=4, lines=[dict(line=i+1, text=text, truncated=False)
                  for i, text in enumerate(source.decode().splitlines())])
        message['content'] = _text_content(json.dumps(result))
        tool['result'] = message
        proof = normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)
        self.assertEqual(proof['validation']['bodyMessageCount'], 100)
        result['sha256'] = 'sha256-forged'
        message['content'] = _text_content(json.dumps(result))
        with self.assertRaisesRegex(ValueError, 'structured read result differs'):
            normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)

    def test_empty_assistant_without_same_turn_tool_is_rejected(self):
        snapshot, raw, scope = _fixture('native')
        row = snapshot['transcript']['items'][2]
        row.update(text='', isFinal=False)
        with self.assertRaisesRegex(ValueError, 'empty assistant'):
            normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)

    def test_native_dsh_and_pi_resolve_original_source_positions(self):
        for endpoint in ("native", "dsh", "pi"):
            for index in (0, 1):
                with self.subTest(endpoint=endpoint, session_index=index):
                    snapshot, raw, scope = _fixture(endpoint, index=index)
                    receipt = normalize_public_session(snapshot, raw, run_scope=scope, session_index=index)
                    validation = receipt["validation"]
                    self.assertEqual(validation["bodyMessageCount"], 100)
                    self.assertEqual(validation["toolGroupCount"], 10)
                    self.assertEqual(receipt["sourceIdentity"]["endpointId"], endpoint)
                    self.assertEqual(len(receipt["sourceIdentity"]["contextRows"]), 1)
                    self.assertEqual(receipt["validation"]["bodyRows"][0]["byteCount"], 1024)

    def test_wrong_scope_and_incomplete_paging_fail_closed(self):
        snapshot, raw, scope = _fixture("native")
        wrong_scope = dict(scope, sessionId="another-session")
        with self.assertRaisesRegex(ValueError, "RunSnapshot scope"):
            normalize_public_session(snapshot, raw, run_scope=wrong_scope, session_index=0)
        snapshot["transcript"]["hasMoreBefore"] = True
        with self.assertRaisesRegex(ValueError, "incomplete"):
            normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)

    def test_provisional_duplicate_old_and_unanchored_rows_are_rejected(self):
        cases = ("provisional", "duplicate", "old", "unanchored")
        for case in cases:
            with self.subTest(case=case):
                snapshot, raw, scope = _fixture("dsh")
                first = snapshot["transcript"]["items"][1]
                if case == "provisional":
                    first["isProvisional"] = True
                elif case == "duplicate":
                    snapshot["transcript"]["items"][3]["sourcePosition"] = dict(first["sourcePosition"])
                elif case == "old":
                    first["text"] = body_text("seed", 0, 50, "user")
                else:
                    first.pop("sourcePosition")
                with self.assertRaises(ValueError):
                    normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)

    def test_tool_result_must_be_source_anchored_and_include_public_read_bytes(self):
        snapshot, raw, scope = _fixture("pi")
        # A forged display tool result cannot replace the original Pi entry.
        raw["native_conversation"]["entries"][-1]["message"]["content"] = _text_content("forged")
        with self.assertRaisesRegex(ValueError, "omits deterministic source evidence"):
            normalize_public_session(snapshot, raw, run_scope=scope, session_index=0)

    def test_history_rows_override_still_requires_complete_public_page_and_sources(self):
        snapshot, raw, scope = _fixture("native")
        rows = {"hasMoreBefore": False, "items": snapshot["transcript"]["items"]}
        receipt = normalize_public_session(
            snapshot, raw, run_scope=scope, session_index=0, history_rows=rows,
        )
        self.assertEqual(receipt["validation"]["bodyMessageCount"], 100)
        rows["hasMoreBefore"] = True
        with self.assertRaisesRegex(ValueError, "hasMoreBefore"):
            normalize_public_session(
                snapshot, raw, run_scope=scope, session_index=0, history_rows=rows,
            )

    def test_post_seed_measure_phase_is_explicitly_unsupported(self):
        snapshot, raw, scope = _fixture("native")
        with self.assertRaisesRegex(ValueError, "not supported"):
            normalize_public_session(
                snapshot, raw, run_scope=scope, session_index=0, phase="measure", body_count=22,
            )


if __name__ == "__main__":
    unittest.main()
