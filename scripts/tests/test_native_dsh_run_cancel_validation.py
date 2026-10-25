import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_dsh_run_cancel_validation import LifecycleEvidenceError, validate_cancel


PROMPT = "AGENTKIT_NATIVE_DSH_CANCEL_PROOF_20261026"
DRAFT = "Keep the next draft."
TICKET = "original-ticket"


def provider():
    return {"requestCount": 1, "matchingBusinessRequests": 1, "firstStreamFrameSent": True,
        "connectionClosedAfterCancel": True, "requestBodiesPersisted": False,
        "providerErrorType": None, "unexpectedRequestCount": 0,
        "activeHandlers": 0, "handlerThreadsStopped": True, "heldHandlerTimedOut": False,
        "titleRequests": 0, "requests": [{"kind": "prompt", "markerPresent": True}]}


def capture(endpoint, *, settled=False, epoch=4, run_id="r1", ticket=TICKET):
    turn_key = "turn:3"
    turn_id = json.dumps([endpoint, "session-1", turn_key], separators=(",", ":"))
    phase = "aborted" if settled else "running"
    raw = {}
    if endpoint == "native":
        events = [
            {"seq": 1, "type": "turn/start", "data": {"turn": 3}},
            {"seq": 2, "type": "user/message", "data": {
                "content": [{"type": "text", "text": PROMPT}],
                "source": {"kind": "user", "rpcId": ticket}}},
        ]
        if settled:
            events.append({"seq": 3, "type": "turn/end", "data": {
                "turn": 3, "reason": "aborted"}})
        raw = {"entries": {str(event["seq"]): {"event": event} for event in events}}
    else:
        events = [
            {"seq": 1, "type": "turn/start", "data": {"turn": 3}},
            {"seq": 2, "type": "user/message", "data": {"turn": 3, "text": PROMPT,
                "source": {"kind": "user", "rpcId": ticket}}},
        ]
        if settled:
            events.append({"seq": 3, "type": "turn/end", "data": {
                "turn": 3, "reason": {"kind": "cancelled"}}})
        raw = {"entries": {str(event["seq"]): {"event": event} for event in events}}
    source_session = {"run_open": not settled, **raw}
    return {
        "publication": 10 if not settled else 11,
        "connectionEpoch": epoch,
        "scope": {"endpointId": endpoint, "sessionId": "session-1"},
        "run": {"canCancel": not settled},
        "transcript": {"turns": [{"id": turn_id, "input": PROMPT, "status": phase}]},
        "submission": {"id": ticket, "phase": "dispatched"},
        "draft": {"text": DRAFT},
        "authority": {"endpoint_id":endpoint,"connection_epoch":epoch,"connection":"Ready",
                      "sessions": {"session-1": source_session}},
        "session": source_session,
        "projectionSucceeded": True,
        "sameMemoryAuthority": True,
    }


class NativeDshCancelValidationTests(unittest.TestCase):
    def test_accepts_source_bound_native_aborted_receipt(self):
        before, after = capture("native"), capture("native", settled=True)
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        result = validate_cancel("native", before, after, ticket_id=TICKET,
            expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())
        self.assertTrue(result["accepted"])

    def test_accepts_source_bound_dsh_cancelled_terminal(self):
        before, after = capture("dsh"), capture("dsh", settled=True)
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        result = validate_cancel("dsh", before, after, ticket_id=TICKET,
            expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())
        self.assertTrue(result["accepted"])

    def test_rejects_ack_without_matching_native_raw_settlement(self):
        before, after = capture("native"), capture("native", settled=True)
        after["session"]["entries"]["3"]["event"]["data"]["reason"] = "completed"
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        with self.assertRaisesRegex(LifecycleEvidenceError, "Native journal lacks"):
            validate_cancel("native", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())

    def test_rejects_stale_abort_from_another_native_turn(self):
        before, after = capture("native"), capture("native", settled=True)
        after["session"]["entries"]["3"]["event"]["data"]["reason"] = "completed"
        old = {"seq": 0, "type": "turn/end", "data": {"turn": 2, "reason": "aborted"}}
        after["session"]["entries"]["0"] = {"event": old}
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        with self.assertRaisesRegex(LifecycleEvidenceError, "Native journal lacks"):
            validate_cancel("native", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())

    def test_rejects_dsh_terminal_for_an_unrelated_turn(self):
        before, after = capture("dsh"), capture("dsh", settled=True)
        after["session"]["entries"]["3"]["event"]["data"]["turn"] = 2
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        with self.assertRaisesRegex(LifecycleEvidenceError, "cancelled/aborted terminal"):
            validate_cancel("dsh", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())

    def test_rejects_provider_ack_without_connection_close_or_with_duplicate(self):
        before, after = capture("native"), capture("native", settled=True)
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        proof = provider()
        proof["connectionClosedAfterCancel"] = False
        with self.assertRaisesRegex(LifecycleEvidenceError, "held original request"):
            validate_cancel("native", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=proof)
        proof["connectionClosedAfterCancel"] = True
        proof["matchingBusinessRequests"] = 2
        proof["requestCount"] = 2
        proof["requests"].append({"kind": "prompt", "markerPresent": True})
        with self.assertRaisesRegex(LifecycleEvidenceError, "held original request"):
            validate_cancel("native", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=proof)

    def test_rejects_user_from_outside_original_turn_and_detached_source(self):
        import copy
        for mode in ('outside-turn', 'detached-source'):
            before, after = capture('native'), capture('native', settled=True)
            after.update(ticketId=TICKET, cancelCallCompleted=True)
            if mode == 'outside-turn':
                after['session']['entries']['1']['event']['data']['turn'] = 4
            else:
                after['authority']['sessions']['session-1'] = copy.deepcopy(after['session'])
                after['session']['entries']['2']['event']['data']['source']['rpcId'] = 'forged'
            with self.subTest(mode=mode), self.assertRaises(LifecycleEvidenceError):
                validate_cancel('native', before, after, ticket_id=TICKET,
                    expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())

    def test_rejects_epoch_change_and_draft_loss(self):
        before, after = capture("dsh"), capture("dsh", settled=True, epoch=5)
        after.update(ticketId=TICKET, cancelCallCompleted=True)
        with self.assertRaisesRegex(LifecycleEvidenceError, "connection epoch"):
            validate_cancel("dsh", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())
        after["connectionEpoch"] = before["connectionEpoch"]
        after["authority"]["connection_epoch"] = before["connectionEpoch"]
        after["draft"]["text"] = ""
        with self.assertRaisesRegex(LifecycleEvidenceError, "draft"):
            validate_cancel("dsh", before, after, ticket_id=TICKET,
                expected_prompt=PROMPT, expected_draft=DRAFT, provider=provider())


if __name__ == "__main__":
    unittest.main()
