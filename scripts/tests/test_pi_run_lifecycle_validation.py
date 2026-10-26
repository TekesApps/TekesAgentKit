from __future__ import annotations

import sys
import hashlib
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pi_run_lifecycle_validation import (
    LifecycleEvidenceError, validate_cancel, validate_unknown_reconciliation,
)


def capture(phase: str, publication: int, reconciled: bool = False) -> dict:
    ticket = "opaque-rpc-ticket-01"
    call = {"id": ticket, "phase": phase, "reconciled": reconciled, "operation": "prompt"}
    prompt = "ENDPOINT_CANCEL_NATIVE_PROOF"
    session = {"native_conversation": {"entries": [
        {"id":"user-event", "type":"user/message", "data":{"text":prompt,"source":{"rpcId":ticket}}},
        {"id":"receipt-event", "type":"custom", "customType":"agentkit.inputReceipt.v1",
         "data":{"rpcId":ticket,"hash":hashlib.sha256(prompt.encode()).hexdigest(),"mode":"followUp"}}
    ]}}
    return {"publication": publication, "connectionEpoch": 2,
        "scope": {"endpointId": "pi", "sessionId": "session-1", "threadId": "session-1"},
        "submission": call, "draft": {"text": "preserved draft"},
        "sameMemoryAuthority": True, "projectionSucceeded": True,
        "serverTransactionalFreshnessProven": False,
        "transcript":{"items":[{"kind":"user","text":prompt,"isProvisional":False,"reconciliationId":ticket}]},
        "session":session,
        "authority":{"endpoint_id":"pi","connection_epoch":2,"connection":"Ready",
                     "sessions":{"session-1":session}}}


class UnknownValidation(unittest.TestCase):
    def setUp(self):
        self.before = capture("unknown", 10)
        self.before["session"]["native_conversation"]["entries"] = []
        self.before["authority"]["sessions"]["session-1"] = self.before["session"]
        self.after = capture("unknown", 12, True)
        _bind_raw_input(self.after, "AGENTKIT_PI_UNKNOWN_PROOF")
        self.after["secondDispatch"] = False
        self.after["manualAcknowledgement"] = False
        self.provider = {"requestCount": 1, "businessRequests": 1, "matchingBusinessRequests": 1,
                         "requestBodiesPersisted": False}
        self.gate = [{"event": "gate_ready"}, {"event": "mutation_forwarded"},
                     {"event": "released"}]

    def test_accepts_exact_auto_reconciliation_after_one_mutation(self):
        result = validate_unknown_reconciliation(self.before, self.after,
            ticket_id="opaque-rpc-ticket-01", expected_prompt="AGENTKIT_PI_UNKNOWN_PROOF",
            expected_draft="preserved draft",
            provider=self.provider, gate_events=self.gate)
        self.assertTrue(result["accepted"])
        self.assertFalse(result["serverTransactionalFreshnessProven"])

    def test_rejects_ticket_substitution_even_when_text_matches(self):
        self.after["submission"]["id"] = "other-ticket"
        with self.assertRaises(LifecycleEvidenceError):
            validate_unknown_reconciliation(self.before, self.after,
                ticket_id="opaque-rpc-ticket-01", expected_prompt="AGENTKIT_PI_UNKNOWN_PROOF",
                expected_draft="preserved draft",
                provider=self.provider, gate_events=self.gate)

    def test_rejects_duplicate_provider_mutation(self):
        self.provider["businessRequests"] = 2
        with self.assertRaises(LifecycleEvidenceError):
            validate_unknown_reconciliation(self.before, self.after,
                ticket_id="opaque-rpc-ticket-01", expected_prompt="AGENTKIT_PI_UNKNOWN_PROOF",
                expected_draft="preserved draft",
                provider=self.provider, gate_events=self.gate)

    def test_rejects_missing_original_native_receipt(self):
        self.after["session"]["native_conversation"]["entries"] = []
        self.after["authority"]["sessions"]["session-1"] = self.after["session"]
        with self.assertRaises(LifecycleEvidenceError):
            validate_unknown_reconciliation(self.before, self.after,
                ticket_id="opaque-rpc-ticket-01", expected_prompt="AGENTKIT_PI_UNKNOWN_PROOF",
                expected_draft="preserved draft",
                provider=self.provider, gate_events=self.gate)

    def test_rejects_ambiguous_raw_user_and_wrong_source_position(self):
        import copy
        for ambiguity in ('duplicate', 'wrong_position'):
            candidate = copy.deepcopy(self.after)
            if ambiguity == 'duplicate':
                candidate['session']['native_conversation']['entries'].append(
                    copy.deepcopy(candidate['session']['native_conversation']['entries'][0]))
            else:
                candidate['transcript']['items'][0]['sourcePosition']['entryId'] = 'different-user'
            with self.subTest(ambiguity=ambiguity), self.assertRaises(LifecycleEvidenceError):
                validate_unknown_reconciliation(self.before, candidate, ticket_id='opaque-rpc-ticket-01',
                    expected_prompt='AGENTKIT_PI_UNKNOWN_PROOF', expected_draft='preserved draft',
                    provider=self.provider, gate_events=self.gate)

    def test_rejects_gate_release_before_mutation_forwarding(self):
        self.gate.reverse()
        with self.assertRaises(LifecycleEvidenceError):
            validate_unknown_reconciliation(self.before, self.after,
                ticket_id="opaque-rpc-ticket-01", expected_prompt="AGENTKIT_PI_UNKNOWN_PROOF",
                expected_draft="preserved draft",
                provider=self.provider, gate_events=self.gate)


class CancelValidation(unittest.TestCase):
    def setUp(self):
        self.before = capture("dispatched", 10)
        self.before["run"] = {"canCancel": True}
        self.after = capture("dispatched", 12)
        self.after.update(ticketId="opaque-rpc-ticket-01", cancelCallCompleted=True,
            run={"canCancel": False}, session={"run_open": False},
            transcript={"turns": [{"id": "[\"pi\",\"session-1\",\"native-run:run-1\"]",
                                    "input": "ENDPOINT_CANCEL_NATIVE_PROOF", "status": "aborted"}],
                        "items": [{"kind":"user","text":"ENDPOINT_CANCEL_NATIVE_PROOF",
                                   "isProvisional":False,"reconciliationId":"opaque-rpc-ticket-01"}]})
        self.after["session"] = capture("dispatched", 12)["session"]
        self.after["session"]["run_open"] = False
        _bind_raw_input(self.after, "ENDPOINT_CANCEL_NATIVE_PROOF")
        self.after["session"]["native_conversation"]["entries"].append({"id":"started-run",
            "type":"custom", "customType":"agentkit.run.v1", "data":{"phase":"started","id":"run-1"}})
        self.after["session"]["native_conversation"]["entries"].append({"id":"aborted-run",
            "type":"custom", "customType":"agentkit.run.v1", "data":{"phase":"settled","aborted":True,"id":"run-1"}})
        self.after["transcript"]["turns"][0]["id"] = '["pi","session-1","native-run:run-1"]'
        self.after["authority"] = {"endpoint_id":"pi","connection_epoch":2,"connection":"Ready",
                                    "sessions":{"session-1":self.after["session"]}}
        self.provider = {"requestCount": 1, "matchingBusinessRequests": 1, "firstStreamFrameSent": True,
                         "connectionClosedAfterCancel": True, "requestBodiesPersisted": False}

    def test_accepts_original_public_aborted_turn_and_closed_held_provider(self):
        result = validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
            expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
            provider=self.provider)
        self.assertTrue(result["accepted"])

    def test_rejects_cancel_ack_without_original_aborted_source(self):
        self.after["transcript"]["turns"][0]["status"] = "completed"
        with self.assertRaises(LifecycleEvidenceError):
            validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
                expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
                provider=self.provider)

    def test_rejects_projected_aborted_status_without_raw_settlement(self):
        self.after["session"]["native_conversation"]["entries"] = [entry for entry in
            self.after["session"]["native_conversation"]["entries"]
            if entry.get("customType") != "agentkit.run.v1"]
        self.after["authority"]["sessions"]["session-1"] = self.after["session"]
        with self.assertRaises(LifecycleEvidenceError):
            validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
                expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
                provider=self.provider)

    def test_rejects_unrelated_historical_aborted_run(self):
        for entry in self.after["session"]["native_conversation"]["entries"]:
            if entry.get("customType") == "agentkit.run.v1":
                entry["data"]["id"] = "old-run"
        self.after["transcript"]["turns"][0]["id"] = '["pi","session-1","native-run:current-run"]'
        self.after["authority"]["sessions"]["session-1"] = self.after["session"]
        with self.assertRaises(LifecycleEvidenceError):
            validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
                expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
                provider=self.provider)

    def test_rejects_settlement_before_run_start(self):
        entries = self.after["session"]["native_conversation"]["entries"]
        start = next(entry for entry in entries if entry.get("data", {}).get("phase") == "started")
        settle = next(entry for entry in entries if entry.get("data", {}).get("phase") == "settled")
        entries.remove(start)
        entries.insert(entries.index(settle) + 1, start)
        self.after["authority"]["sessions"]["session-1"] = self.after["session"]
        with self.assertRaises(LifecycleEvidenceError):
            validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
                expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
                provider=self.provider)

    def test_rejects_provider_request_that_did_not_close_after_cancel(self):
        self.provider["connectionClosedAfterCancel"] = False
        with self.assertRaises(LifecycleEvidenceError):
            validate_cancel(self.before, self.after, ticket_id="opaque-rpc-ticket-01",
                expected_prompt="ENDPOINT_CANCEL_NATIVE_PROOF", expected_draft="preserved draft",
                provider=self.provider)


class SignalWireContract(unittest.TestCase):
    def test_signals_are_raw_consumer_bytes_and_never_overwrite(self):
        import importlib.util
        import tempfile
        spec = importlib.util.spec_from_file_location('pi_lifecycle_runner_test',
            Path(__file__).resolve().parents[1] / 'test-real-pi-run-lifecycle.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, expected in [('cancel', b'cancel\n'), ('release', b'release\n'), ('gate', b'')]:
                path = root / name
                module.write_signal(path, expected)
                self.assertEqual(path.read_bytes(), expected)
                with self.assertRaises(FileExistsError):
                    module.write_signal(path, b'changed')
                self.assertEqual(path.read_bytes(), expected)


if __name__ == "__main__":
    unittest.main()


def _bind_raw_input(sample: dict, prompt: str) -> None:
    sample["transcript"]["items"][0]["text"] = prompt
    entries = sample["session"]["native_conversation"]["entries"]
    receipt = next(entry for entry in entries if entry.get("customType") == "agentkit.inputReceipt.v1")
    receipt["data"]["hash"] = hashlib.sha256(prompt.encode()).hexdigest()
    receipt["data"]["timestamp"] = 1234.0
    receipt["data"]["mode"] = "followUp"
    sample["transcript"]["items"][0].update(recordId="user-event",
        sourcePosition={"kind":"native","entryId":"user-event","entryOrdinal":0})
    entries.insert(0, {"id":"user-event","type":"message",
        "message":{"role":"user","content":prompt,"timestamp":1234.0}})
