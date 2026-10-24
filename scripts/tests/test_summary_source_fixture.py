import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from summary_source_fixture import SummarySourceEvidenceError, validate_witness


def witness():
    parent = "parent-1"
    child = "child-1"
    call = "call-1"
    return {
        "evidenceScope": "original-publication-and-same-memory-input",
        "serverTransactionalFreshnessProven": False,
        "projectionSucceeded": True,
        "snapshot": {
            "connectionEpoch": 4,
            "selection": {"destination": {"kind":"run", "scope": {"endpointId": "pi", "sessionId": parent, "threadId":parent}}},
            "transcript": {"isStale": False, "tools": [{"id": call, "name": "tekes_subagent",
                                        "result": {"source":{"callId":call}, "content":[{"type":"text", "text":f"Started child {child}"}]}, "resultPosition":{"kind":"journal", "sequence":2, "contentIndex":None}}]},
            "summary": {"scope": {"endpointId": "pi", "threadId": parent,
                                    "sessionId": parent},
                        "subagents": [{"id": call, "title": "Model child",
                                        "sessionId": child, "status": "completed"}]},
        },
        "authority": {
            "endpoint_id": "pi", "connection_epoch": 4, "connection":"Ready", "inventory_ready":True, "control_ready":True, "actionables_ready":True,
            "sessions": {
                parent: {"journal_ready": True, "entries": {"2": {"event": {
                    "seq": 2, "type": "tool/result", "data": {
                        "message": {"source": {"callId": call},
                                    "content": [{"type": "text", "text": f"Started child {child}"}]}}}}}},
                child: {"summary": {"sessionId": child, "status": "completed"}},
            },
        },
    }


class SummarySourceFixtureTests(unittest.TestCase):
    def test_accepts_same_frame_pi_source_and_derives_no_terminal_indicator(self):
        result = validate_witness(witness())
        self.assertEqual(result["childSessionId"], "child-1")
        self.assertIsNone(result["indicator"])
        self.assertFalse(result["serverTransactionalFreshnessProven"])

    def test_native_position_links_original_message_without_splitting_row_id(self):
        value = witness()
        tool = value["snapshot"]["transcript"]["tools"][0]
        tool["resultPosition"] = {"kind":"native", "entryId":"original", "entryOrdinal":0}
        source = value["authority"]["sessions"]["parent-1"]
        source["entries"] = {}
        source["native_conversation"] = {"entries":[{"id":"original", "message":tool["result"]}]}
        self.assertEqual(validate_witness(value)["sourceIdentity"]["entryId"], "original")
        source["native_conversation"]["entries"][0]["id"] = "different"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_missing_epochs_and_wrong_authority_and_stale_input(self):
        for field, bad in [("connection_epoch", None), ("endpoint_id", "dsh"), ("connection", "Connecting")]:
            value = witness()
            value["authority"][field] = bad
            with self.assertRaises(SummarySourceEvidenceError):
                validate_witness(value)
        value = witness()
        value["snapshot"]["transcript"]["isStale"] = True
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_idle_fallback_requires_original_nonrunning_nonblank_empty_child_inputs(self):
        value = witness()
        child = value["authority"]["sessions"]["child-1"]
        child["summary"] = {"sessionId":"child-1", "running":False, "blank":False}
        child.update(run_open=False, queue=[], entries={})
        value["snapshot"]["summary"]["subagents"][0]["status"] = "idle"
        result = validate_witness(value)
        self.assertIsNone(result["rawChildStatus"])
        self.assertIsNone(result["indicator"])
        child["run_open"] = True
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_degraded_projection(self):
        value = witness()
        value["projectionSucceeded"] = False
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_missing_summary(self):
        value = witness()
        value["snapshot"]["summary"] = None
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_wrong_endpoint_or_selection(self):
        value = witness()
        value["snapshot"]["summary"]["scope"]["endpointId"] = "dsh"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_epoch_mismatch(self):
        value = witness()
        value["authority"]["connection_epoch"] = 5
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_pointer_without_owned_source_entry(self):
        value = witness()
        value["snapshot"]["summary"]["subagents"][0]["sessionId"] = "foreign-child"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_child_status_from_a_different_frame(self):
        value = witness()
        value["authority"]["sessions"]["child-1"]["summary"]["status"] = "running"
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_missing_raw_parent_tool_source(self):
        value = witness()
        value["authority"]["sessions"]["parent-1"]["entries"] = {}
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)

    def test_rejects_server_freshness_overclaim(self):
        value = witness()
        value["serverTransactionalFreshnessProven"] = True
        with self.assertRaises(SummarySourceEvidenceError):
            validate_witness(value)


if __name__ == "__main__":
    unittest.main()
