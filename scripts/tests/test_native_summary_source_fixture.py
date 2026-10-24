import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from native_summary_source_fixture import NativeSummarySourceError, validate_witness


def witness():
    parent = "native-parent-1"
    row_id = "opaque::native::task::call::id"
    arguments = {"task_name": "Inspect the source", "goal": "Read-only check"}
    return {
        "evidenceScope": "original-publication-and-same-memory-input",
        "serverTransactionalFreshnessProven": False,
        "projectionSucceeded": True,
        "snapshot": {
            "connectionEpoch": 7,
            "selection": {"destination": {"kind": "run", "scope": {
                "endpointId": "native", "threadId": parent, "sessionId": parent,
            }}},
            "transcript": {"isStale": False, "tools": [{
                "id": row_id, "name": "task", "arguments": arguments, "phase": "done",
                "sourcePosition": {"kind": "native", "entryId": "entry-opaque",
                                   "entryOrdinal": 0, "contentIndex": 0},
            }]},
            "summary": {"scope": {"endpointId": "native", "threadId": parent,
                                    "sessionId": parent},
                        "tasks": [{"id": row_id, "scope": {
                            "endpointId": "native", "threadId": parent, "sessionId": parent,
                        }, "name": arguments["task_name"], "status": "done", "kind": "task"}]},
        },
        "authority": {
            "endpoint_id": "native", "connection_epoch": 7, "connection": "Ready",
            "inventory_ready": True, "control_ready": True, "actionables_ready": True,
            "sessions": {parent: {"journal_ready": True, "native_conversation": {"entries": [{
                "id": "entry-opaque", "message": {"role": "assistant", "content": [{
                    "type": "toolCall", "id": "raw-call-id-do-not-split", "name": "task",
                    "arguments": arguments,
                }]},
            }]}}},
        },
    }


class NativeSummarySourceFixtureTests(unittest.TestCase):
    def test_journal_source_pairs_call_and_result_without_decoding_the_row_id(self):
        value = witness()
        tool = value["snapshot"]["transcript"]["tools"][0]
        tool["sourcePosition"] = {"kind":"journal", "sequence":4, "contentIndex":None}
        tool["resultPosition"] = {"kind":"journal", "sequence":5, "contentIndex":None}
        tool["result"] = {"source":{"callId":"raw-call"}, "content":[{"type":"text","text":"done"}]}
        projection = value["authority"]["sessions"]["native-parent-1"]
        projection["native_conversation"] = None
        projection["entries"] = {
            "4":{"event":{"seq":4,"type":"tool/call", "data":{"callId":"raw-call", "name":"task", "arguments":tool["arguments"]}}},
            "5":{"event":{"seq":5,"type":"tool/result", "data":{"message":copy.deepcopy(tool["result"])}}}}
        self.assertEqual(validate_witness(value)["sourceIdentity"]["sequence"],4)
        projection["entries"]["5"]["event"]["data"]["message"]["source"]["callId"] = "different"
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(value)

    def test_accepts_original_native_task_call_and_keeps_parent_task_passive(self):
        result = validate_witness(witness())
        self.assertTrue(result["passiveParentTask"])
        self.assertEqual(result["summaryRow"]["name"], "Inspect the source")
        self.assertEqual(result["sourceIdentity"], {
            "kind": "native", "entryId": "entry-opaque", "entryOrdinal": 0,
            "contentIndex": 0,
        })
        self.assertFalse(result["serverTransactionalFreshnessProven"])

    def test_rejects_degraded_stale_and_server_freshness_overclaim(self):
        for path, value in [
            (("projectionSucceeded",), False),
            (("snapshot", "transcript", "isStale"), True),
            (("serverTransactionalFreshnessProven",), True),
        ]:
            candidate = witness()
            target = candidate
            for part in path[:-1]:
                target = target[part]
            target[path[-1]] = value
            with self.assertRaises(NativeSummarySourceError):
                validate_witness(candidate)

    def test_rejects_wrong_endpoint_epoch_or_selection_scope(self):
        mutations = [
            lambda value: value["authority"].update(endpoint_id="pi"),
            lambda value: value["authority"].update(connection_epoch=8),
            lambda value: value["snapshot"]["selection"]["destination"]["scope"].update(sessionId="foreign"),
        ]
        for mutate in mutations:
            candidate = witness()
            mutate(candidate)
            with self.assertRaises(NativeSummarySourceError):
                validate_witness(candidate)

    def test_rejects_row_name_or_phase_that_disagrees_with_source(self):
        for field, bad in [("name", "invented title"), ("status", "running")]:
            candidate = witness()
            candidate["snapshot"]["summary"]["tasks"][0][field] = bad
            with self.assertRaises(NativeSummarySourceError):
                validate_witness(candidate)

    def test_rejects_native_position_that_does_not_name_original_tool_content(self):
        for field, bad in [("entryOrdinal", 1), ("entryId", "other"), ("contentIndex", 1)]:
            candidate = witness()
            candidate["snapshot"]["transcript"]["tools"][0]["sourcePosition"][field] = bad
            with self.assertRaises(NativeSummarySourceError):
                validate_witness(candidate)
        candidate = witness()
        candidate["authority"]["sessions"]["native-parent-1"]["native_conversation"]["entries"][0]["message"]["content"][0]["arguments"]["task_name"] = "different"
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(candidate)

    def test_rejects_independent_child_pointer_or_missing_source_position(self):
        candidate = witness()
        candidate["snapshot"]["summary"]["tasks"][0]["sessionId"] = "child"
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(candidate)
        candidate = witness()
        del candidate["snapshot"]["transcript"]["tools"][0]["sourcePosition"]
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(candidate)

    def test_rejects_duplicate_or_missing_task_matches(self):
        candidate = witness()
        candidate["snapshot"]["summary"]["tasks"].append(
            copy.deepcopy(candidate["snapshot"]["summary"]["tasks"][0]))
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(candidate)
        candidate = witness()
        candidate["snapshot"]["summary"]["tasks"] = []
        with self.assertRaises(NativeSummarySourceError):
            validate_witness(candidate)


if __name__ == "__main__":
    unittest.main()
