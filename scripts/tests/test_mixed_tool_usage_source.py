import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mixed_tool_usage_source import validate


def journal_frame(*, turn=4, output=60, settlement=60, live=None):
    turn_id = '["endpoint-x","session-x","turn:%s"]' % turn
    ev1 = {"type": "assistant/message", "seq": 31, "time": 1.0,
           "data": {"turn": turn, "step": 0, "message": {"id": "assistant-a"},
                    "usage": {"inputTokens": 10, "outputTokens": 20}}}
    ev2 = {"type": "assistant/message", "seq": 50, "time": 2.0,
           "data": {"turn": turn, "step": 1, "message": {"id": "assistant-b"},
                    "usage": {"inputTokens": 20, "outputTokens": 40}}}
    start = {"type": "turn/start", "seq": 20, "time": 0.0, "data": {"turn": turn}}
    end = {"type": "turn/end", "seq": 90, "time": 3.0,
           "data": {"turn": turn, "usage": {"inputTokens": 30, "outputTokens": settlement},
                    "reason": "completed"}}
    canonical = {"id": turn_id, "status": "completed", "usage": {"inputTokens": 30, "outputTokens": output},
                 "reportedUsage": {"outputTokens": output, "isComplete": True}, "liveUsage": None}
    session = {"entries": {str(e["seq"]): {"event": e} for e in (start, ev1, ev2, end)}, "previews": {}}
    frame = {"revision": 29, "connectionEpoch": 3, "endpointId": "endpoint-x",
             "threadId": "thread-x", "sessionId": "session-x", "turnId": turn_id,
             "sourceKind": "journal", "projectionSucceeded": True,
             "authority": {"endpointId": "endpoint-x", "connectionEpoch": 3, "sessionKey": "session-x"},
             "snapshot": {"revision":29,"connectionEpoch": 3, "transcript": {"turns": [canonical], "items": []},
                          "selection":{"destination":{"kind":"run","scope":{"endpointId":"endpoint-x","threadId":"thread-x","sessionId":"session-x"}}}},
             "rawSession": session,
             "canonical": canonical,
             "rawTurn": {"sequence": 90, "kind": "turn/end", "turn": turn,
                         "usage": end["data"]["usage"], "event": end},
             "reportedSources": [
                 {"sequence": 31, "kind": "assistant/message", "turn": turn, "step": 0,
                  "recordId": "assistant-a", "usage": dict(ev1["data"]["usage"]), "outputKnown": True, "event": ev1},
                 {"sequence": 50, "kind": "assistant/message", "turn": turn, "step": 1,
                  "recordId": "assistant-b", "usage": dict(ev2["data"]["usage"]), "outputKnown": True, "event": ev2}],
             "liveSources": [], "canonicalAbsence": {"reportedUsage": False, "liveUsage": True},
             "turnStarted": True}
    if live is not None:
        preview = {"text": "", "reasoning": "", "reasoning_ended": False,
                   "output_tokens": live - 20, "presentation_order": 51}
        session["previews"] = {f"{turn}:0": preview}
        session["entries"] = {str(e["seq"]): {"event": e} for e in (start, ev1)}
        session["run_open"] = True
        canonical.update(status="running", usage={"inputTokens": 10, "outputTokens": 20},
                         reportedUsage={"outputTokens":20,"isComplete":True}, liveUsage={"outputTokens": live})
        frame["snapshot"]["transcript"]["turns"] = [canonical]
        frame["rawTurn"] = None
        frame["reportedSources"] = [frame["reportedSources"][0]]
        frame["liveSources"] = [{"previewKey": f"{turn}:0", "outputTokens": live - 20,
                                 "presentationOrder": 51, "preview": preview}]
        frame["canonicalAbsence"] = {"reportedUsage": False, "liveUsage": False}
    return frame


def native_frame():
    turn = '["pi-endpoint","pi-session","native-turn:user-1"]'
    entry = {"id": "assistant-1", "type": "message", "message": {"role": "assistant",
             "usage": {"input": 10, "output": 20, "cost": 0.001}}}
    canonical = {"id": turn, "status": "completed", "usage": {"inputTokens": 10, "outputTokens": 20},
                 "reportedUsage": None, "liveUsage": None}
    item = {"id": "item-1", "recordId": "assistant-1", "turnId": turn}
    return {"revision": 9, "connectionEpoch": 2, "endpointId": "pi-endpoint",
        "threadId": "pi-session", "sessionId": "pi-session", "turnId": turn,
        "sourceKind": "nativeConversation", "projectionSucceeded": True,
        "authority": {"endpointId": "pi-endpoint", "connectionEpoch": 2, "sessionKey": "pi-session"},
        "snapshot": {"revision":9,"connectionEpoch": 2, "transcript": {"turns": [canonical], "items": [item]},
                     "selection":{"destination":{"kind":"run","scope":{"endpointId":"pi-endpoint","threadId":"pi-session","sessionId":"pi-session"}}}},
        "rawSession": {"native_conversation": {"sessionId": "pi-session", "entries": [entry]},
                       "entries": {}, "previews": {}},
        "canonicalItems": [item], "canonical": canonical,
        "rawTurn": None, "reportedSources": [{"recordId": "assistant-1", "kind": "message",
            "usage": entry["message"]["usage"], "entry": entry}],
        "liveSources": [], "canonicalAbsence": {"reportedUsage": True, "liveUsage": True},
        "absenceContract": "docs/session-endpoints.md:37-38"}


class UsageSourceValidation(unittest.TestCase):
    def test_running_cumulative_usage_retains_durable_steps_without_transient_preview(self):
        live = journal_frame(live=20)
        live['liveSources'] = []
        live['rawSession']['previews'] = {}
        result = validate({'final':journal_frame(), 'liveObserved':live})
        self.assertTrue(result['live']['liveObserved'])
        self.assertFalse(result['live']['transientPreviewObserved'])
        self.assertTrue(result['live']['durableUsagePaired'])

    def test_original_composer_destination_fences_thread_identity(self):
        frame = journal_frame()
        frame['snapshot']['selection']['destination']['scope']['threadId'] = 'different-thread'
        with self.assertRaisesRegex(ValueError, 'Composer destination'):
            validate({'final':frame,'liveObserved':None})

    def test_journal_settlement_is_authoritative_over_per_response_fixture(self):
        result = validate({"final": journal_frame(output=50, settlement=50), "liveObserved": None})
        self.assertEqual(result["final"]["outputTokens"], 50)

    def test_native_pi_message_usage_is_paired_and_absent_fields_remain_absent(self):
        result = validate({"final": native_frame(), "liveObserved": None})
        self.assertFalse(result["final"]["reportedObserved"])
        self.assertEqual(result["final"]["outputTokens"], 20)

    def test_live_source_is_same_turn_and_not_inferred_from_final(self):
        result = validate({"final": journal_frame(), "liveObserved": journal_frame(live=35)})
        self.assertTrue(result["live"]["liveObserved"])
        self.assertEqual(validate({"final": journal_frame(), "liveObserved": None})["live"]["status"],
                         "not_observed")

    def test_rejects_wrong_authority_or_epoch(self):
        for key, value in (("endpointId", "other"), ("connectionEpoch", 4)):
            frame = journal_frame()
            frame["authority"][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "authority"):
                validate({"final": frame, "liveObserved": None})
        frame = journal_frame()
        frame["snapshot"]["connectionEpoch"] = 7
        with self.assertRaisesRegex(ValueError, "snapshot epoch"):
            validate({"final": frame, "liveObserved": None})

    def test_rejects_wrong_scoped_turn_prefix(self):
        frame = journal_frame()
        frame["turnId"] = '["other","session-x","turn:4"]'
        with self.assertRaisesRegex(ValueError, "scope prefix"):
            validate({"final": frame, "liveObserved": None})

    def test_rejects_tampered_raw_usage_summary(self):
        frame = journal_frame()
        frame["reportedSources"][0]["usage"]["outputTokens"] = 999
        with self.assertRaisesRegex(ValueError, "summary differs"):
            validate({"final": frame, "liveObserved": None})

    def test_rejects_wrong_raw_settlement_turn(self):
        frame = journal_frame()
        frame["rawTurn"]["event"]["data"]["turn"] = 99
        with self.assertRaisesRegex(ValueError, "raw settlement identity"):
            validate({"final": frame, "liveObserved": None})

    def test_rejects_foreign_or_duplicate_preview(self):
        foreign = journal_frame(live=35)
        foreign["liveSources"][0]["previewKey"] = "5:0"
        with self.assertRaisesRegex(ValueError, "keys differ"):
            validate({"final": journal_frame(), "liveObserved": foreign})
        duplicate = journal_frame(live=35)
        duplicate["liveSources"].append(dict(duplicate["liveSources"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate raw preview"):
            validate({"final": journal_frame(), "liveObserved": duplicate})
        wrong_epoch = journal_frame(live=35)
        wrong_epoch["connectionEpoch"] = 4
        wrong_epoch["authority"]["connectionEpoch"] = 4
        wrong_epoch["snapshot"]["connectionEpoch"] = 4
        with self.assertRaisesRegex(ValueError, "epoch"):
            validate({"final": journal_frame(), "liveObserved": wrong_epoch})

    def test_rejects_tampered_pi_usage_and_unpaired_canonical_record(self):
        frame = native_frame()
        frame["reportedSources"][0]["usage"] = {"input": 10, "output": 999}
        with self.assertRaisesRegex(ValueError, "exact original message entry"):
            validate({"final": frame, "liveObserved": None})
        frame = native_frame()
        frame["reportedSources"][0]["recordId"] = "foreign-message"
        with self.assertRaisesRegex(ValueError, "exact canonical item"):
            validate({"final": frame, "liveObserved": None})


if __name__ == "__main__":
    unittest.main()
