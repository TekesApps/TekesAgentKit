from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "performance_native_family_report.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("native_family_report", SCRIPT)
report = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(report)


def canon(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True,
                      separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def family_profile():
    return {
        "format": 1, "kind": "agentkit-native-family-profile", "issue": 56,
        "sourceProducts": {"baseline": "b" * 40, "candidate": "c" * 40},
        "runtime": {"kind": "native", "kernelVersion": "0.9.3", "source": "d" * 40},
        "paths": {
            "common50": {"roles": ["baseline", "candidate"]},
            "product20": {"roles": ["candidate"],
                          "noSameEntryBaselineReason": "new ChildRun20 entry has no f1 baseline"},
        },
        "seed": {"commentaryBodies": 61, "actualReadGroups": 61,
                 "proofText": "owned Native paged child proof\n",
                 "finalText": "NATIVE_FAMILY_SEED_COMPLETE",
                 "semanticCorpusSha256": ""},
        "thresholds": {"latencyRegressionRatioMax": 1.2, "peakRssRegressionRatioMax": 1.15,
                       "firstUsableSnapshotP95Ms": 10000, "sendToFirstPublicFrameP95Ms": 2000,
                       "historyPageToProjectionP95Ms": 1000,
                       "explicitReconnectToConvergedP95Ms": 5000, "agentkitPeakRssMiBMax": 512,
                       "ownedHarnessPeakRssMiBMax": 2048, "ownedCleanupDeadlineMs": 10000},
    }


def source_history():
    proof_text = "owned Native paged child proof\n"
    proof_sha = digest(proof_text.encode())
    entries = []
    seq = 1
    bodies = []
    reads = []
    for index in range(1, 62):
        marker = f"AKFAMILY|native|{index:02}|commentary|"
        body = marker + "x" * (1024 - len(marker.encode()))
        bodies.append(body)
        entries.append({"event": {"seq": seq, "type": "assistant/message",
                                  "data": {"message": {"role": "assistant", "content": body}}}})
        seq += 1
        call_id = f"opaque-read-call-{index}"
        entries.append({"event": {"seq": seq, "type": "tool/call",
                                  "data": {"name": "read", "callId": call_id,
                                           "arguments": {"path": "proof.txt"}}}})
        seq += 1
        payload = {"path": "proof.txt", "bytes": len(proof_text.encode()), "sha256": proof_sha,
                   "truncated": False, "total_lines": 1,
                   "lines": [{"line": 1, "text": proof_text.rstrip("\n"), "truncated": False}]}
        entries.append({"event": {"seq": seq, "type": "tool/result",
                                  "data": {"callId": call_id,
                                           "message": {"role": "user", "content": json.dumps(payload)}}}})
        seq += 1
        reads.append({"ordinal": index, "name": "read", "path": "proof.txt",
                      "bytes": len(proof_text.encode()), "sha256": proof_sha})
    entries.append({"event": {"seq": seq, "type": "assistant/message",
                              "data": {"message": {"role": "assistant", "content": "NATIVE_FAMILY_SEED_COMPLETE"},
                                       "sessionFinal": True}}})
    normalized = {"commentaryBodies": bodies, "readGroups": reads,
                  "finalText": "NATIVE_FAMILY_SEED_COMPLETE"}
    source = {"parentSessionId": "parent-original", "childSessionId": "child-original",
              "throughSequence": seq, "historyRevision": "rev-1", "hasMoreBefore": False,
              "runOpen": False, "sourceCall": {"taskId": "opaque-task-source"},
              "entries": entries}
    for entry in entries:
        entry["event"]["time"] = float(entry["event"]["seq"])
    return source, normalized


def sample_row(path, role, mode, index, multiplier=1.0, failed=False):
    rounds = report.MEASURED_ROUNDS[mode]
    latency = {
        "snapshot": {i: int(1_000_000 * multiplier) for i in rounds},
        "page20": {i: int(500_000 * multiplier) for i in rounds},
        "reconnect": {i: int(2_000_000 * multiplier) for i in rounds} if mode == "reconnectLifecycle" else {},
        "actual50Earlier": {i: 700_000 for i in rounds} if path == "common50" else {},
    }
    return {"sampleId": f"{path}-{role}-{mode}-{index}", "path": path, "role": role,
            "mode": mode, "caseIndex": index, "status": "failed" if failed else "success",
            "failureReason": "preserved original failure" if failed else None,
            "official": True, "diagnosticOnly": False, "semanticCorpusSha256": "e" * 64,
            "runtimeSha256": "f" * 64, "observerSha256": "1" * 64,
            "machineBefore": {"stableIdentity": {"model": "M"}},
            "protocol": {"latenciesNs": latency},
            "lifecycle": {"cleanupElapsedNs": 100_000_000,
                          "peaksKiB": {"agentkit": 100 * 1024, "harness": 300 * 1024}}}


class NativeFamilyPerformanceReportTests(unittest.TestCase):
    def setUp(self):
        self.profile = family_profile()
        source, normalized = source_history()
        self.normalized = normalized
        self.source = source
        self.profile["seed"]["semanticCorpusSha256"] = digest(canon(normalized))
        self.registration = {"official": True, "sampleId": "native-common50-baseline-cold-0",
                             "caseName": "native-common50-baseline-cold-0",
                             "endpoint": "native", "role": "baseline", "path": "common50",
                             "mode": "cold", "caseIndex": 0, "iterations": 1,
                             "sourceCommit": self.profile["sourceProducts"]["baseline"],
                             "parentSessionId": "parent-original", "childSessionId": "child-original"}

    def test_public_timestamp_typing_preserves_opaque_numbers_and_rejects_precision_loss(self):
        raw = {"event": {"seq": 7, "time": 1792974061536,
                          "data": {"opaqueNumber": 1}}}
        typed = report._public_history_entry(raw)
        self.assertIs(type(typed["event"]["time"]), float)
        self.assertIs(type(typed["event"]["data"]["opaqueNumber"]), int)
        self.assertIs(type(raw["event"]["time"]), int)
        for invalid in (2**53 + 1, float("nan"), float("inf"), True):
            with self.subTest(timestamp=invalid), self.assertRaises(ValueError):
                report._public_history_entry({"event": {"seq": 7, "time": invalid}})

    def test_round_metrics_survive_json_integer_key_serialization(self):
        samples = [{"protocol": {"latenciesNs": {"snapshot": {0: 1250000}}}}]
        decoded = json.loads(json.dumps(samples))
        self.assertEqual(report._values(samples, "snapshot", "cold"), [1.25])
        self.assertEqual(report._values(decoded, "snapshot", "cold"), [1.25])

    def test_raw_full200_rebuilds_semantic_corpus_and_preserves_opaque_ids(self):
        normalized, entries = report._validate_source_history(self.source, self.registration, self.profile)
        self.assertEqual(normalized, self.normalized)
        self.assertEqual(len(entries), 184)
        self.assertEqual(entries[1]["event"]["data"]["callId"], "opaque-read-call-1")

    def test_same_sequence_with_changed_content_and_wrong_page20_source_are_rejected(self):
        original = {"event": {"seq": 7, "type": "assistant/message",
                              "data": {"message": {"role": "assistant", "content": "old"}}}}
        changed = {"event": {"seq": 7, "type": "assistant/message",
                             "data": {"message": {"role": "assistant", "content": "new"}}}}
        with self.assertRaisesRegex(ValueError, "changed content"):
            report._union_pages([{"entries": [original, changed]}], [original])
        head_entries = self.source["entries"][-30:]
        frontier = report._raw_entry_sequence(head_entries[0])
        page = {"parentSessionId": "parent-original", "childSessionId": "child-original",
                "throughSequence": self.source["throughSequence"], "historyRevision": "rev-1",
                "sourceCall": self.source["sourceCall"], "runOpen": False,
                "hasMoreBefore": True, "entries": head_entries}
        call = {"event": "bounded_call", "connectionOrdinal": 1, "phase": "test",
                "intentMonotonicNs": 1, "returnMonotonicNs": 2, "latencyNs": 1,
                "passthroughUnmodified": True,
                "request": {"method": "subagents.history", "payload": {
                    "parentSessionId": "parent-original", "childSessionId": "child-original",
                    "maxMessages": 50, "beforeSequence": None}},
                "response": {"value": page}}
        with self.assertRaisesRegex(ValueError, "maxMessages"):
            report._request_payload(call, parent="parent-original", child="child-original",
                                     expected_size=20)
        call["request"]["payload"]["maxMessages"] = 20
        call["response"]["value"]["entries"] = head_entries
        with self.assertRaisesRegex(ValueError, "exactly one head and one Earlier"):
            report._validate_page_pair([call], parent="parent-original", child="child-original",
                                       source=self.source, page_size=20)
        bad_source = dict(self.source, childSessionId="wrong-child")
        with self.assertRaisesRegex(ValueError, "owner"):
            report._validate_source_history(bad_source, self.registration, self.profile)

    def test_wrong_role_or_api_path_is_rejected_from_raw_family_event_identity(self):
        event = {"path": "product20", "role": "baseline", "caseIndex": 0,
                 "parentScope": {"sessionId": "parent-original"}, "childId": "child-original"}
        with self.assertRaisesRegex(ValueError, "registration"):
            report._validate_event_identity(event, self.registration, "common50")
        event.update({"path": "common50", "role": "candidate", "caseIndex": 0})
        with self.assertRaisesRegex(ValueError, "registration"):
            report._validate_event_identity(event, self.registration, "common50")
        event.update({"path": "common50", "role": "baseline", "caseIndex": 0})
        report._validate_event_identity(event, self.registration, "common50")

    def test_fake_reconnect_old_epoch_and_parent_draft_pollution_are_rejected(self):
        event = {"path": "common50", "role": "baseline", "caseIndex": 0,
                 "parentScope": {"sessionId": "parent-original"}, "childId": "child-original",
                 "childRunInstanceRetained": True, "converged": True,
                 "endpointOrdinalBeforeReconnect": 4, "endpointOrdinalAfterReconnect": 4,
                 "connectionEpochBeforeReconnect": 9, "connectionEpochAfterReconnect": 10,
                 "parentDraft": "AKFAMILY_PARENT_DRAFT_KEEP",
                 "intentMonotonicNs": 10, "publicationMonotonicNs": 15, "latencyNs": 5}
        with self.assertRaisesRegex(ValueError, "new endpoint"):
            report._validate_reconnect_identity(event, "parent-original", "child-original")
        event["endpointOrdinalAfterReconnect"] = 5
        event["connectionEpochAfterReconnect"] = 9
        with self.assertRaisesRegex(ValueError, "new endpoint"):
            report._validate_reconnect_identity(event, "parent-original", "child-original")
        event["connectionEpochAfterReconnect"] = 10
        report._validate_reconnect_identity(event, "parent-original", "child-original")
        event["parentDraft"] = "polluted"
        with self.assertRaisesRegex(ValueError, "draft"):
            report._validate_reconnect_identity(event, "parent-original", "child-original")

    def test_product_transcript_must_equal_raw_projection_after_only_sourcecall_decoration(self):
        source_call = {"callId": "native-task-call", "name": "task",
                       "arguments": {"task": "owned child"}, "phase": "completed"}
        task_id = json.dumps(["native", "child-original", "task-source", "native-task-call"],
                             separators=(",", ":"))
        turn = {"id": task_id, "input": "", "status": "completed"}
        tool = {"id": task_id + ":call", "turnId": task_id, "name": "task",
                "arguments": {"task": "owned child"}, "phase": "completed"}
        raw = {"turns": [{"id": "source-message-turn"}], "items": [{"id": "source-item"}],
               "tools": [], "hasMoreBefore": True, "isStale": False, "diagnostics": []}
        actual = json.loads(json.dumps(raw))
        actual["turns"].insert(0, turn)
        actual["tools"].insert(0, tool)
        event = {"rawProjectionTranscript": raw,
                 "productSourceDecoration": {"sourceCall": source_call, "turnId": task_id,
                                             "toolId": task_id + ":call", "turn": turn, "tool": tool}}
        source = {"sourceCall": source_call}
        report._validate_product_projection(event, actual, source, "parent-original", "child-original")
        changed = json.loads(json.dumps(actual))
        changed["items"][0]["id"] = "altered-item"
        with self.assertRaisesRegex(ValueError, "differs from SessionProjection"):
            report._validate_product_projection(event, changed, source,
                                                "parent-original", "child-original")
        with self.assertRaisesRegex(ValueError, "source decoration"):
            report._validate_product_projection(event, actual,
                                                {"sourceCall": dict(source_call, callId="different")},
                                                "parent-original", "child-original")

    def test_refresh_or_prefix_validation_cannot_substitute_for_render_earlier(self):
        head_entries = self.source["entries"][-90:]
        earlier_entries = self.source["entries"][:-90]
        head = {"parentSessionId": "parent-original", "childSessionId": "child-original",
                "throughSequence": self.source["throughSequence"], "historyRevision": "rev-1",
                "sourceCall": self.source["sourceCall"], "runOpen": False,
                "hasMoreBefore": True, "entries": head_entries}
        earlier = {**head, "hasMoreBefore": False, "entries": earlier_entries}
        frontier = min(report._raw_entry_sequence(row) for row in head_entries)

        def call(size, phase, page, cursor=None):
            return {"event": "bounded_call", "connectionOrdinal": 1, "sourceReadOrdinal": 1,
                    "phase": phase, "intentMonotonicNs": 1, "returnMonotonicNs": 2,
                    "latencyNs": 1, "passthroughUnmodified": True,
                    "request": {"method": "subagents.history", "payload": {
                        "parentSessionId": "parent-original", "childSessionId": "child-original",
                        "maxMessages": size, "beforeSequence": cursor,
                        "throughSequence": self.source["throughSequence"] if cursor else None,
                        "historyRevision": "rev-1" if cursor else None}},
                    "response": {"value": page}}

        head_call = call(20, "childrun-open-and-first-snapshot", head)
        refresh = call(200, "product-childrun-refresh-after-earlier20", earlier, frontier)
        with self.assertRaisesRegex(ValueError, "exactly one true Earlier"):
            report._validate_actual_earlier([refresh], parent="parent-original", child="child-original",
                                            source=self.source, page_size=20, head=head,
                                            expected_phase="product-childrun-earlier20")
        render_earlier = call(20, "product-childrun-earlier20", earlier, frontier)
        pages = report._validate_actual_earlier([render_earlier, refresh], parent="parent-original",
                                                 child="child-original", source=self.source,
                                                 page_size=20, head=head,
                                                 expected_phase="product-childrun-earlier20")
        self.assertEqual(report._union_pages([head] + pages, self.source["entries"], must_exhaust=True),
                         self.source["entries"])
        self.assertEqual(report._dedupe_calls([head_call, head_call]), [head_call])
        changed_duplicate = dict(head_call, response={"value": dict(head, hasMoreBefore=False)})
        with self.assertRaisesRegex(ValueError, "sourceReadOrdinal changed"):
            report._dedupe_calls([head_call, changed_duplicate])

    def test_sep_projection_checks_exact_raw_union_without_sourceposition(self):
        head = {"entries": self.source["entries"][-90:], "hasMoreBefore": True}
        earlier = {"entries": self.source["entries"][:-90], "hasMoreBefore": False}
        union = self.source["entries"]
        snapshot = {"isStale": False, "hasMoreBefore": False,
                    "items": [{"id": "item-1", "text": "ordinary content"},
                              {"id": "item-2", "text": "NATIVE_FAMILY_SEED_COMPLETE"}]}
        event = {"projection": {"entries": {str(report._raw_entry_sequence(row)): row for row in union}},
                 "unionEntries": union, "transcript": snapshot}
        report._validate_sep_projection(event, [head, earlier], union)
        event["unionEntries"] = union[:-1]
        with self.assertRaisesRegex(ValueError, "event unionEntries"):
            report._validate_sep_projection(event, [head, earlier], union)

    def test_materializer_keeps_failed_case_and_never_promotes_diagnostic_receipt(self):
        with tempfile.TemporaryDirectory() as root:
            case = Path(root)
            (case / "inputs.json").write_text("{}\n")
            (case / "official-execution-receipt.json").write_text(json.dumps({
                "official": True, "diagnosticOnly": True, "status": "success",
                "sampleId": self.registration["sampleId"]}))
            row = report.materialize_sample(case, self.registration,
                                            {"format": 1, "thresholds": self.profile["thresholds"]},
                                            self.profile)
            self.assertEqual(row["status"], "failed")
            self.assertIs(row["diagnosticOnly"], True)
            self.assertIs(row["official"], False)
            self.assertIn("diagnostic evidence", row["failureReason"])

    def test_missing_or_failed_case_does_not_hide_available_group_metrics(self):
        samples = []
        for mode, count in report.CASE_COUNTS.items():
            for index in range(count):
                for role in ("baseline", "candidate"):
                    samples.append(sample_row("common50", role, mode, index))
                samples.append(sample_row("product20", "candidate", mode, index))
        # Retain an official failure in the expected slot; it is never replaced.
        failing_index = next(i for i, row in enumerate(samples)
                             if row["path"] == "product20" and row["mode"] == "cold" and row["caseIndex"] == 4)
        samples[failing_index] = sample_row("product20", "candidate", "cold", 4, failed=True)
        result = report.aggregate_batch({"familyProfile": self.profile,
                                         "baseProfile": {"format": 1}, "samples": samples})
        self.assertFalse(result["nativeFamilyPerformanceAccepted"])
        self.assertFalse(result["performanceAccepted"])
        self.assertFalse(result["issue56Accepted"])
        self.assertEqual(result["trajectories"]["common50/candidate:cold"]["sampleCount"], 5)
        self.assertTrue(result["failedCases"])
        missing = next(row for row in result["missingCaseGroups"]
                       if row["path"] == "product20" and row["mode"] == "cold")
        self.assertEqual(missing["missingCaseIndices"], [4])

    def test_common50_relative_regression_fails_and_product20_has_no_fake_baseline(self):
        samples = []
        for mode, count in report.CASE_COUNTS.items():
            for index in range(count):
                for role in ("baseline", "candidate"):
                    factor = 1.3 if role == "candidate" else 1.0
                    samples.append(sample_row("common50", role, mode, index, multiplier=factor))
                samples.append(sample_row("product20", "candidate", mode, index, multiplier=1.0))
        result = report.aggregate_batch({"familyProfile": self.profile,
                                         "baseProfile": {"format": 1}, "samples": samples})
        comparison = result["trajectories"]["common50/candidate:cold"]["relativeComparisons"]["snapshot"]
        self.assertGreater(comparison["ratio"], 1.2)
        self.assertFalse(comparison["pass"])
        product = result["trajectories"]["product20/candidate:cold"]
        self.assertFalse(product["hasComparableBaseline"])
        self.assertIn("new ChildRun20", product["noComparableBaselineReason"])
        self.assertFalse(result["nativeFamilyPerformanceAccepted"])

    def test_only_complete_21_case_matrix_can_pass_family_scope(self):
        samples = []
        for mode, count in report.CASE_COUNTS.items():
            for index in range(count):
                for role in ("baseline", "candidate"):
                    samples.append(sample_row("common50", role, mode, index))
                samples.append(sample_row("product20", "candidate", mode, index))
        result = report.aggregate_batch({"familyProfile": self.profile,
                                         "baseProfile": {"format": 1}, "samples": samples})
        self.assertTrue(result["allPreregisteredCasesPresent"])
        self.assertTrue(result["nativeFamilyPerformanceAccepted"])
        self.assertFalse(result["performanceAccepted"])
        self.assertFalse(result["issue56Accepted"])

    def test_raw_rss_cadence_and_conservative_cleanup_fail_closed(self):
        with tempfile.TemporaryDirectory() as root:
            case = Path(root)
            raw_events = [{"event": "iteration_started", "iteration": i, "mode": "warm",
                           "path": "common50", "role": "candidate", "caseIndex": 0,
                           "topology": "native-bounded-family", "monotonicNs": i + 1}
                          for i in range(11)]
            (case / "consumer.stdout.jsonl").write_text("\n".join(map(json.dumps, raw_events)) + "\n")
            (case / "consumer-process-boundaries.jsonl").write_text("\n".join(
                json.dumps({"event": "iteration_started", "iteration": i, "rawEventIndex": i,
                            "originalIntentMonotonicNs": i + 1, "monotonicNs": i + 2, "error": None,
                            "osProcesses": [{"role": "agentkit", "pid": 10, "startIdentity": "a"},
                                            {"role": "harness", "pid": 20, "startIdentity": "b"}]})
                for i in range(11)) + "\n")
            rss_rows = [{"monotonicNs": 1, "processes": [
                {"role": "agentkit", "rssKiB": 10}, {"role": "harness", "rssKiB": 20}],
                "treeRssKiB": {"agentkit": 10, "harness": 20, "provider": 0}},
                {"monotonicNs": 400_000_002, "processes": [
                {"role": "agentkit", "rssKiB": 10}, {"role": "harness", "rssKiB": 20}],
                 "treeRssKiB": {"agentkit": 10, "harness": 20, "provider": 0}}]
            (case / "measurement-rss.jsonl").write_text("\n".join(map(json.dumps, rss_rows)) + "\n")
            (case / "last-rss-summary.json").write_text(json.dumps({
                "sampleCount": 2, "maxObservedIntervalNs": 400_000_001,
                "requestedIntervalMs": 50,
                "sampledTreePeakRssKiB": {"agentkit": 10, "harness": 20, "provider": 0}}))
            (case / "owned-process-retirement.json").write_text(json.dumps({
                "status": "verified", "consumerRoot": {"pid": 10, "startIdentity": "a"},
                "cleanupIntentObservedMonotonicNs": 10, "runtimeCleanupCompletedMonotonicNs": 11,
                "cleanupIntentMonotonicNsLowerBound": 1, "allOwnedRootsExitedMonotonicNs": 12_000_000_002,
                "cleanupElapsedNs": 12_000_000_001,
                "cleanupDeadlineMs": 10}))
            (case / "fixture-lifecycle.json").write_text(json.dumps([
                {"operation": "start", "intentMonotonicNs": 1, "readyMonotonicNs": 2,
                 "completedMonotonicNs": 3,
                 "process": {"pid": 20, "startIdentity": "b", "role": "harness"}},
                {"operation": "seed_retire", "intentMonotonicNs": 4,
                 "completedMonotonicNs": 5, "exitStatus": 0},
                {"operation": "measurement_retire", "intentMonotonicNs": 6,
                 "completedMonotonicNs": 7, "exitStatus": 0}]))
            (case / "consumer-exit.json").write_text(json.dumps({
                "pid": 10, "startIdentity": "a", "exitStatus": 0, "ownedGroupAbsent": True}))
            errors = []
            result = report._validate_lifecycle(case, "warm", {"parentSessionId": "p", "childSessionId": "c",
                                                                  "path": "common50", "role": "candidate",
                                                                  "caseIndex": 0},
                                                {"consumerRoot": {"pid": 10, "startIdentity": "a"}}, errors)
            self.assertGreater(result["maxGapNs"], 250_000_000)
            self.assertTrue(any("RSS sample gap" in row for row in errors))
            self.assertTrue(any("cleanup" in row for row in errors))

    def test_duplicate_case_cannot_replace_a_failed_registration(self):
        row = sample_row("product20", "candidate", "warm", 0, failed=True)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            report.aggregate_batch({"familyProfile": self.profile, "baseProfile": {"format": 1},
                                    "samples": [row, row]})


if __name__ == "__main__":
    unittest.main()
