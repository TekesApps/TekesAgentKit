from __future__ import annotations

import json
import io
from pathlib import Path
import tempfile
import unittest

from performance_native_family_seed import (
    COMMENTARY_BYTES, COMMENTARY_COUNT, PAGE_SIZE, commentary_body,
    diagnostic_candidate_receipt, earlier_history_request, family_history_commands,
    family_native_provider, native_family_seed_commands, validate_public_child_catalog,
    validate_public_family_groundtruth, validate_public_family_pages,
)


def _entry(seq: int, event_type: str, data: dict, *, append: bool = False) -> dict:
    event = {"type": event_type, "seq": seq, "time": seq + 1, "data": data}
    if append:
        event["surfaceOp"] = "append"
    return {"event": event}


def groundtruth() -> dict:
    entries = []
    seq = 0
    for index in range(1, COMMENTARY_COUNT + 1):
        seq += 1
        entries.append(_entry(seq, "assistant/message", {"message": {
            "role": "assistant", "content": [{"type": "text", "text": commentary_body(index)}]},
            "sessionFinal": False, "meta": {"debug": "not transcript text"}}, append=True))
        call_id = f"read-{index}"
        seq += 1
        entries.append(_entry(seq, "tool/call", {"name": "read", "callId": call_id,
                                                   "arguments": {"limit": 2, "offset": 1,
                                                                 "path": "proof.txt"}}))
        seq += 1
        result = {"artifact_version": 0, "bytes": 31, "limit": 2,
                  "lines": [{"line": 1, "text": "owned Native paged child proof",
                             "truncated": False}], "offset": 1, "path": "proof.txt",
                  "sha256": "sha256-e286561fa61d8bd46d81795c55634ac36757d566b853b88897aeb04710247e63",
                  "total_lines": 1, "truncated": False}
        entries.append(_entry(seq, "tool/result", {"message": {
            "role": "user", "source": {"kind": "tool", "callId": call_id},
            "content": [{"type": "text", "text": json.dumps(result, separators=(",", ":"))}]},
            "meta": {"kernel_outcome": "ok"}}, append=True))
    seq += 1
    entries.append(_entry(seq, "assistant/message", {"message": {
        "role": "assistant", "content": [{"type": "text", "text": "NATIVE_FAMILY_SEED_COMPLETE"}]},
        "sessionFinal": True, "meta": {"debug": "also not transcript text"}}, append=True))
    return {"parentSessionId": "parent", "childSessionId": "child",
            "throughSequence": seq, "historyRevision": "revision-1", "runOpen": False,
            "hasMoreBefore": False, "sourceCall": {"callId": "original-task",
                "name": "task", "phase": "completed", "arguments": {"goal": "proof"}},
            "entries": entries}


class NativeFamilySeedTests(unittest.TestCase):
    def test_commentary_bodies_are_exact_size_and_uniquely_marked(self):
        values = [commentary_body(index) for index in range(1, COMMENTARY_COUNT + 1)]
        self.assertEqual(len(set(values)), COMMENTARY_COUNT)
        self.assertTrue(all(len(item.encode("utf-8")) == COMMENTARY_BYTES for item in values))
        self.assertTrue(values[0].startswith("AKFAMILY|native|01|commentary|"))

    def test_native_seed_reuses_original_task_pointer_with_minimal_execution_scope(self):
        commands = native_family_seed_commands("parent", "/tmp/owned-workspace", "provider", "model")
        self.assertEqual([item["op"] for item in commands],
                         ["call", "call", "call", "open", "compose"])
        self.assertEqual(commands[-1]["text"], "ENDPOINT_NATIVE_CHILD")
        execution = commands[0]["args"]["execution"]
        self.assertEqual(execution["allowed_tools"], ["task", "read"])
        self.assertEqual(execution["read_roots"], [str(Path("/tmp/owned-workspace").resolve())])
        self.assertEqual(execution["writable_roots"], [])
        self.assertEqual(execution["toolchain_roots"], [])
        self.assertFalse(execution["network"])

    def test_public_catalog_resolves_one_bounded_child_and_rejects_slot(self):
        self.assertEqual(validate_public_child_catalog({"entries": [
            {"kind": "child", "id": "child", "mode": "bounded"}]}, "parent"), "child")
        with self.assertRaises(ValueError):
            validate_public_child_catalog({"entries": [
                {"kind": "child", "id": "parent", "mode": "bounded"}]}, "parent")

    def test_public_groundtruth_uses_native_message_wire_and_requires_all_groups_and_final(self):
        source = groundtruth()
        proof = validate_public_family_groundtruth(source,
            parent={"sessionId": "parent"}, child_id="child")
        self.assertEqual(proof["commentaryCount"], 61)
        self.assertEqual(proof["readGroupCount"], 61)
        self.assertTrue(proof["diagnosticOnly"])
        self.assertEqual(proof["officialSamples"], 0)
        self.assertFalse(proof["performanceAccepted"])
        self.assertEqual(proof["commentaryCount"], 61)
        source["entries"][0]["event"]["data"]["meta"]["debug"] = commentary_body(2)
        proof = validate_public_family_groundtruth(source,
            parent={"sessionId": "parent"}, child_id="child")
        self.assertEqual(proof["commentaryCount"], 61)
        source["entries"][0]["event"]["data"]["message"]["content"][0]["text"] = "tampered"
        with self.assertRaises(ValueError):
            validate_public_family_groundtruth(source,
                parent={"sessionId": "parent"}, child_id="child")
        source = groundtruth()
        source["entries"].pop(0)
        with self.assertRaises(ValueError):
            validate_public_family_groundtruth(source,
                parent={"sessionId": "parent"}, child_id="child")

    def test_pages_require_two_pages_same_cut_and_exact_source_entries(self):
        history = groundtruth()
        proof = validate_public_family_groundtruth(history,
            parent={"sessionId": "parent"}, child_id="child")
        entries = history["entries"]
        pages = [dict(history, entries=entries[-20:], hasMoreBefore=True),
                 dict(history, entries=entries[:1], hasMoreBefore=False)]
        requests = [dict(parentSessionId="parent", childSessionId="child", maxMessages=20),
                    earlier_history_request("parent", "child", proof["throughSequence"],
                                            proof["historyRevision"],
                                            min(item["event"]["seq"] for item in pages[0]["entries"]))]
        result = validate_public_family_pages(proof, pages, requests)
        self.assertEqual(result["pagePlanEntryCount"], 21)
        self.assertEqual(family_history_commands("parent", "child")[0]["args"]["maxMessages"], 200)
        self.assertEqual(family_history_commands("parent", "child")[1]["args"]["maxMessages"], PAGE_SIZE)
        wrong_cut = [dict(page) for page in pages]
        wrong_cut[1]["throughSequence"] -= 1
        with self.assertRaises(ValueError):
            validate_public_family_pages(proof, wrong_cut, requests)
        tampered = [dict(page) for page in pages]
        tampered[1] = dict(tampered[1], entries=[dict(entries[0], event=dict(
            entries[0]["event"], time=-1))])
        with self.assertRaises(ValueError):
            validate_public_family_pages(proof, tampered, requests)

    def test_page_expansion_may_exceed_twenty_and_eof_page_may_be_one_record(self):
        history = groundtruth()
        proof = validate_public_family_groundtruth(history,
            parent={"sessionId": "parent"}, child_id="child")
        entries = history["entries"]
        pages = [dict(history, entries=entries[1:], hasMoreBefore=True),
                 dict(history, entries=entries[:1], hasMoreBefore=False)]
        requests = [dict(parentSessionId="parent", childSessionId="child", maxMessages=20),
                    earlier_history_request("parent", "child", proof["throughSequence"],
                                            proof["historyRevision"],
                                            min(item["event"]["seq"] for item in pages[0]["entries"]))]
        self.assertGreater(len(pages[0]["entries"]), PAGE_SIZE)
        self.assertEqual(validate_public_family_pages(proof, pages, requests)["pagePlanEntryCount"],
                         len(entries))

    def test_candidate_receipt_is_diagnostic_and_contains_only_digests_for_bodies(self):
        state = {"stepReceipts": [{"step": index, "commentaryDigest": "a" * 64,
                                   "commentaryBytes": COMMENTARY_BYTES}
                                  for index in range(1, COMMENTARY_COUNT + 1)],
                 "finalResponseId": "native-family-final"}
        history = {"entries": [{"event": {"data": {"message": {"content": [
            {"type": "text", "text": "secret body"}]}}}}]}
        pages = {"pagePlan": [{"entries": [{"event": {"seq": 1}}]}]}
        receipt = diagnostic_candidate_receipt(parent_session_id="parent", child_session_id="child",
                                               provider_state=state, history=history, pages=pages)
        encoded = json.dumps(receipt)
        self.assertNotIn("secret body", encoded)
        self.assertTrue(receipt["diagnosticOnly"])
        self.assertEqual(receipt["officialSamples"], 0)
        self.assertFalse(receipt["performanceAccepted"])
        self.assertFalse(receipt["nativePaneAccepted"])

    def test_provider_chains_actual_read_output_and_response_identity(self):
        class FakeRequest:
            def __init__(self, body):
                self.body = json.dumps(body).encode()
                self.rfile = io.BytesIO(self.body)
                self.headers = {"Content-Length": str(len(self.body))}
                self.path = "/v1/responses"
                self.wfile = io.BytesIO()
                self.status = None

            def send_response(self, status):
                self.status = status

            def send_header(self, *_args):
                pass

            def end_headers(self):
                pass

        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            (workspace / "proof.txt").write_text("owned Native paged child proof\n", encoding="utf-8")
            requested_paths = []
            provider_type, state = family_native_provider(workspace=workspace, requests=requested_paths)
            def request(body):
                value = FakeRequest(body)
                value._send_sse = lambda events: provider_type._send_sse(value, events)
                return value
            first_body = {"input": [{"role": "user", "content": "CHILD_NATIVE_PROOF"}],
                "tools": [{"name": "read", "parameters": {"type": "object",
                    "properties": {"path": {"type": "string"}}, "required": ["path"]}}]}
            first = request(first_body)
            provider_type.do_POST(first)
            first_events = [json.loads(line[6:]) for line in first.wfile.getvalue().decode().splitlines()
                            if line.startswith("data: ")]
            self.assertEqual(first.status, 200)
            self.assertEqual(first_events[-1]["response"]["id"], "native-family-step-1")
            continuation = request({"previous_response_id": "native-family-step-1",
                "input": [{"type": "function_call_output", "call_id": "native-family-read-1",
                           "output": "owned Native paged child proof"}], "tools": first_body["tools"]})
            provider_type.do_POST(continuation)
            self.assertEqual(state["stepReceipts"][1]["priorReadResultObserved"], True)
            self.assertEqual(state["stepReceipts"][1]["step"], 2)
            broken = request({"previous_response_id": "wrong-response",
                "input": [{"type": "function_call_output", "call_id": "native-family-read-2",
                           "output": "owned Native paged child proof"}], "tools": first_body["tools"]})
            with self.assertRaisesRegex(ValueError, "previous_response_id chain"):
                provider_type.do_POST(broken)


if __name__ == "__main__":
    unittest.main()
