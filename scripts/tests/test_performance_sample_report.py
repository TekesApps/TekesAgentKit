import hashlib
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from performance_sample_report import _stable_machine_identity, aggregate_batch, nearest_rank

PROFILE = {
    "format": 1,
    "resolvedProductCandidate": "candidate-sha",
    "baseline": "baseline-sha",
    "thresholds": {
        "latencyRegressionRatioMax": 1.2,
        "peakRssRegressionRatioMax": 1.15,
        "firstUsableSnapshotP95Ms": 10000,
        "sendToFirstPublicFrameP95Ms": 2000,
        "historyPageToProjectionP95Ms": 1000,
        "explicitReconnectToConvergedP95Ms": 5000,
        "agentkitPeakRssMiBMax": 512,
        "ownedHarnessPeakRssMiBMax": 2048,
        "ownedCleanupDeadlineMs": 10000,
    },
}


def seed_session(session_index):
    bodies = [f"{session_index}{index:03d}" + "x" * 1020 for index in range(100)]
    source_bytes = [hashlib.sha256(f"source-{session_index}-{index}".encode()).hexdigest()
                    for index in range(10)]
    call_ids = [f"call-{session_index}-{index}" for index in range(10)]
    normalized = {"bodyMessages": bodies, "sourceBytesSha256": source_bytes, "callIds": call_ids,
                  "casePath": f"/case/{session_index}", "opaqueIds": ["varying"]}
    digest_input = {key: normalized[key] for key in ("bodyMessages", "sourceBytesSha256", "callIds")}
    digest = hashlib.sha256(json.dumps(digest_input, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"bodyMessages": 100, "toolGroups": 10, "bodyUtf8Bytes": [1024] * 100,
            "normalizedContent": normalized, "normalizedContentDigest": digest}


def machine_observation(*, source="AC Power", percent=100, sleep=1, unix=1):
    def command(stdout, exit_status=0):
        return {"exitStatus": exit_status, "stdout": stdout, "stderr": ""}
    return {
        "unixNs": unix,
        "monotonicNs": unix * 1000,
        "loadAverage": [float(unix), 0, 0],
        "os": command("ProductName: macOS\nProductVersion: 26.6.2\nBuildVersion: 25G83\n"),
        "hardware": command("hw.model: Mac16,1\nhw.memsize: 34359738368\nhw.ncpu: 10\n"),
        "power": command(f"Now drawing from '{source}'\n"
                         f" -InternalBattery-0\t{percent}%; charged; 0:00 remaining present: true\n"),
        "powerMode": command(f"Battery Power:\n lowpowermode 0\n sleep {sleep}\n"
                             f"AC Power:\n lowpowermode 0\n sleep {sleep}\n"),
        "thermal": command("No thermal warning level has been recorded\n"),
    }


def install_machine_proof(row, *, before=None, after=None):
    before = before or machine_observation()
    after = after or machine_observation(unix=2)
    row["machineIdentity"] = before
    row["machineAfter"] = after
    row["stableMachineIdentity"] = _stable_machine_identity(before)


def sample(role, mode, case_index, endpoint="native", latency=100_000_000):
    sessions = [f"s{index}" for index in range(2)]
    def identity(name, pid):
        return {"pid": pid, "startIdentity": f"{name}-{role}-{endpoint}-{pid}"}
    def scope(session):
        return {"endpointId": endpoint, "threadId": session, "sessionId": session}
    events = [{"event": "consumer_started", "mode": mode, "sessionIds": sessions},
              {"event": "runtime_ready"}]
    if mode == "cold":
        for session in sessions:
            events.append({"event": "snapshot", "purpose": "cold_first_usable_original_projection",
                           "scope": scope(session), "usable": True, "latencyNs": latency,
                           "openLatencyNs": 123})
    else:
        for session in sessions:
            events.append({"event": "snapshot", "purpose": "initial_usable_original_projection",
                           "scope": scope(session), "usable": True, "latencyNs": latency,
                           "openLatencyNs": 123})
    rounds = 1 if mode == "cold" else 11
    for index in range(rounds):
        phase = "cold" if mode == "cold" else f"{mode}_initialization_excluded" if index == 0 else mode
        events.append({"event": "iteration_started", "mode": phase, "iteration": index,
                       "expectedBodyMessagesBeforePerSession": 100 if mode == "cold" else 100 + 2 * index})
        if mode == "cold":
            for session in sessions:
                events.append({"event": "snapshot", "purpose": "cold", "scope": scope(session),
                               "usable": True, "latencyNs": latency})
        if mode != "cold":
            for session in sessions:
                events.append({"event": "history_close_open_snapshot", "scope": scope(session),
                               "reason": "iteration", "usable": True, "latencyNs": latency + index})
        for session in sessions:
            page = {"event": "history_page", "scope": scope(session), "requestedMessages": 20,
                    "pageMethod": ("fresh SessionConversation(max_messages=20) follow + public "
                                   "load_earlier_conversation(20) + MemoryProjectionStore::apply"
                                   if endpoint == "pi" else
                                   "fresh SessionJournal(max_messages=20) follow + SEP history_page(20)"),
                    "priorIdentityRetained": True, "oldRecordContentAndOrderPreserved": True,
                    "latencyNs": latency}
            if endpoint == "pi":
                page["addedMessageRecordCount"] = 20
            else:
                page["actualAppendMessageCount"] = 20
            events.append(page)
        if endpoint == "pi":
            for session in sessions:
                events.append({"event": "history_close_open_snapshot", "scope": scope(session),
                               "reason": "restore_pi_runtime_window_50_after_page_probe",
                               "usable": True, "latencyNs": latency + index})
        if mode != "warm":
            events.append({"event": "reconnect", "converged": True, "epochAdvanced": True,
                           "endpointGenerationReplaced": True,
                           "iteration": index, "publicationMonotonicNs": 1_000 + index,
                           "latencyNs": latency})
        for index2, session in enumerate(sessions):
            events.append({"event": "send", "scope": scope(session), "sessionIndex": index2,
                           "intentMonotonicNs": 2_000 + index,
                           "round": index, "ticketId": f"ticket-{index2}-{index}",
                           "durableUser": {"record": "u"}, "firstMatchingAssistantContent": {"record": "a"},
                           "durableMatchingAssistantSettlement": {"record": "f"},
                           "assistantContentObservedLatencyNs": latency})
    for session in sessions:
        events.append({"event": "run_controller_history_page", "scope": scope(session),
                       "requestedMessages": 50, "actualAddedMessageCount": 50,
                       "priorIdentityRetained": True})
    events += [{"event": "cleanup_intent"}, {"event": "runtime_cleanup_completed"}]
    identities = []
    for round_index in range(11):
        row = {"agentkit": identity("agentkit", 100)}
        if endpoint == "pi":
            # Steady warm retains both RPC children; lifecycle replaces both
            # after each reconnect and records those transitions separately.
            base = 400 if mode != "reconnectLifecycle" else 400 + 2 * round_index
            row["piRpcChildren"] = [identity("pi0", base), identity("pi1", base + 1)]
        else:
            row["ownedHarness"] = identity("harness", 200)
        identities.append(row)
    restart = {"seedExitStatus": 0, "seedStateSealed": True,
               "seedHarness": identity("seed", 200 + case_index),
               "measuredHarness": identity("measured", 300 + case_index)}
    if endpoint == "pi":
        restart["measuredPiRpcChildren"] = [identity("pi0", 600), identity("pi1", 601)]
        restart["seedPiRpcChildren"] = [identity("seedpi0", 500), identity("seedpi1", 501)]
        restart["seedAgentKitRoot"] = identity("seedagent", 99)
        restart["seedOwnedGroupAbsent"] = True
    process = {"consumerExitStatus": 0, "ownedGroupAbsent": True,
               "consumerRoot": identity("consumer", 100), "ownedHarnessExitStatus": 0,
               "cleanupElapsedMs": 2, "coldHarnessRestart": restart if mode == "cold" else None,
               "warmProcessIdentities": identities if mode != "cold" else None,
               "piRpcLifecycleRetirements": ([{"oldSetRetired": True, "newSetStarted": True}
                                               for _ in range(10)] if endpoint == "pi" and mode == "reconnectLifecycle" else None),
               "sampledTreePeakRssKiB": {"agentkit": 1024, "harness": 2048}}
    if endpoint == "pi" and mode == "reconnectLifecycle":
        lifecycle_boundaries = []
        for round_index in range(1, 11):
            child_ids = identities[round_index]["piRpcChildren"]
            for event_index, event in enumerate(events):
                if event.get("event") != "send" or event.get("round") != round_index:
                    continue
                census = [{"pid": 100, "startIdentity": identity("agentkit", 100)["startIdentity"],
                           "role": "agentkit", "commandName": "performance_acceptance"}]
                census.extend({**child, "role": "harness", "commandName": "pi",
                               "executableName": "node"} for child in child_ids)
                lifecycle_boundaries.append({
                    "event": "send", "rawEventIndex": event_index,
                    "round": round_index, "sessionIndex": event["sessionIndex"],
                    "scopeSessionId": event["scope"]["sessionId"],
                    "originalIntentMonotonicNs": event["intentMonotonicNs"],
                    "monotonicNs": 20_000 + event_index,
                    "osProcesses": census, "error": None,
                })
        process["piRpcLifecycleSendBoundaries"] = lifecycle_boundaries
    role_commit = PROFILE["resolvedProductCandidate"] if role == "candidate" else PROFILE["baseline"]
    return {"role": role, "mode": mode, "iteration": case_index, "caseIndex": case_index,
            "status": "success", "official": True, "diagnosticOnly": False, "failureReason": None,
            "endpoint": endpoint, "sessionIds": sessions, "events": events,
            "seedProof": {"sessions": [seed_session(index) for index in range(2)]},
            "seedDigest": hashlib.sha256("".join(seed_session(index)["normalizedContentDigest"]
                                                    for index in range(2)).encode()).hexdigest(),
            "workloadDigest": "same-workload", "sourceCommit": role_commit,
            "programSha256": "1" * 64, "consumerSha256": "2" * 64,
            "runnerSha256": "3" * 64, "runtimeSha256": "4" * 64,
            "machineIdentity": machine_observation(unix=case_index + 1),
            "machineAfter": machine_observation(unix=case_index + 2),
            "stableMachineIdentity": _stable_machine_identity(machine_observation()),
            "runtimeIdentity": {"kernel": "fixed", "pi": "fixed", "dsh": "fixed"},
            "process": process}


def batch_factory():
    rows = []
    for endpoint in ("native", "pi", "dsh"):
        for mode, count in (("cold", 5), ("warm", 1), ("reconnectLifecycle", 1)):
            for index in range(count):
                rows.extend([sample("baseline", mode, index, endpoint),
                             sample("candidate", mode, index, endpoint, latency=105_000_000)])
    return {"profile": PROFILE, "officialSamples": len(rows), "diagnosticOnly": False,
            "sourceCandidate": "candidate-sha", "sourceBaseline": "baseline-sha", "samples": rows}


class PerformanceSampleReportTests(unittest.TestCase):
    def test_reconnect_without_redundant_iteration_uses_original_marker_segment(self):
        batch = batch_factory()
        for sample in batch['samples']:
            for event in sample['events']:
                if event.get('event') == 'reconnect':
                    event.pop('iteration', None)
        self.assertTrue(aggregate_batch(batch)['ordinarySessionPerformanceAccepted'])

    def test_pi_cold_rejects_reused_seed_rpc_even_when_rust_consumer_changes(self):
        batch = batch_factory()
        sample = next(row for row in batch["samples"]
                      if row["endpoint"] == "pi" and row["mode"] == "cold")
        restart = sample["process"]["coldHarnessRestart"]
        restart["seedPiRpcChildren"] = restart["measuredPiRpcChildren"]
        with self.assertRaisesRegex(ValueError, "must be disjoint"):
            aggregate_batch(batch)

    def test_nearest_rank_is_frozen_definition(self):
        self.assertEqual(nearest_rank([4, 1, 3, 2], .5), 2)
        self.assertEqual(nearest_rank(list(range(1, 21)), .95), 19)

    def test_case_counts_iterations_and_scope_metrics_are_distinct(self):
        batch = batch_factory()
        report = aggregate_batch(batch)
        self.assertEqual(report["officialSamples"], 42)
        self.assertTrue(report["ordinarySessionPerformanceAccepted"])
        self.assertFalse(report["familyAttachmentPerformanceCovered"])
        self.assertFalse(report["issue55Issue56Accepted"])
        warm = report["trajectories"]["native/candidate:warm"]
        self.assertEqual((warm["caseCount"], warm["measuredIterationCount"]), (1, 10))
        self.assertEqual(warm["perSession"]["0"]["sendToFirstAssistantContentMs"]["count"], 10)
        self.assertEqual(report["trajectories"]["native/candidate:cold"]["caseCount"], 5)
        self.assertEqual(report["trajectories"]["pi/candidate:reconnectLifecycle"]["perSession"]["1"]
                         ["historyPage20ToProjectionMs"]["count"], 10)

    def test_cold_cases_aggregate_by_stable_index_with_distinct_session_ids(self):
        batch = batch_factory()
        for row in batch["samples"]:
            if row["mode"] != "cold":
                continue
            old_ids = row["sessionIds"]
            new_ids = [f"{session}-cold-{row['caseIndex']}" for session in old_ids]
            row["sessionIds"] = new_ids
            for event in row["events"]:
                if event.get("event") == "consumer_started":
                    event["sessionIds"] = new_ids
                scope = event.get("scope")
                if isinstance(scope, dict):
                    index = old_ids.index(scope["sessionId"])
                    scope["sessionId"] = new_ids[index]
                    scope["threadId"] = new_ids[index]

        report = aggregate_batch(batch)
        cold = report["trajectories"]["native/candidate:cold"]
        self.assertEqual(cold["perSession"]["0"]["sessionIds"],
                         [f"s0-cold-{index}" for index in range(5)])
        self.assertEqual(cold["perSession"]["1"]["sessionIds"],
                         [f"s1-cold-{index}" for index in range(5)])
        self.assertEqual(cold["perSession"]["0"]["firstUsableSnapshotMs"]["count"], 5)
        self.assertEqual(cold["perSession"]["1"]["firstUsableSnapshotMs"]["count"], 5)

    def test_send_session_index_must_match_its_original_scope_pointer(self):
        batch = batch_factory()
        row = next(row for row in batch["samples"]
                   if row["role"] == "candidate" and row["mode"] == "cold")
        sends = [event for event in row["events"] if event.get("event") == "send"]
        sends[0]["sessionIndex"], sends[1]["sessionIndex"] = 1, 0
        with self.assertRaisesRegex(ValueError, "sessionIndex does not match"):
            aggregate_batch(batch)

    def test_relative_latency_limit_is_enforced_per_session_scope(self):
        batch = batch_factory()
        for row in batch["samples"]:
            if row["endpoint"] != "native" or row["mode"] != "cold":
                continue
            for event in row["events"]:
                if event.get("event") != "snapshot" or event.get("purpose") != "cold_first_usable_original_projection":
                    continue
                index = row["sessionIds"].index(event["scope"]["sessionId"])
                if row["role"] == "baseline":
                    event["latencyNs"] = 1_000_000 if index == 0 else 100_000_000
                else:
                    event["latencyNs"] = 90_000_000 if index == 0 else 90_000_000

        report = aggregate_batch(batch)
        trajectory = report["trajectories"]["native/candidate:cold"]
        # The old ratio of worst-scope p95s was 90/100 = 0.9 and hid
        # session 0's real 90/1 regression. Each scope now has its own gate.
        self.assertEqual(trajectory["firstUsableSnapshotMs"]["regressionRatio"], 0.9)
        self.assertEqual(trajectory["perSession"]["0"]["firstUsableSnapshotMs"]["regressionRatio"], 90)
        self.assertFalse(trajectory["perSession"]["0"]["firstUsableSnapshotMs"]["regressionPass"])
        self.assertTrue(trajectory["perSession"]["1"]["firstUsableSnapshotMs"]["regressionPass"])
        self.assertFalse(report["ordinarySessionPerformanceAccepted"])

    def test_diagnostic_only_never_becomes_official(self):
        report = aggregate_batch({"profile": PROFILE, "officialSamples": 0,
                                  "diagnosticOnly": True, "samples": []})
        self.assertFalse(report["performanceAccepted"])
        self.assertEqual(report["officialSamples"], 0)

    def test_missing_and_failed_cases_are_retained_not_replaced(self):
        value = batch_factory()
        value["samples"].pop()
        value["officialSamples"] = len(value["samples"])
        report = aggregate_batch(value)
        self.assertFalse(report["performanceAccepted"])
        self.assertEqual(report["retainedSampleRows"], value["samples"])
        value = batch_factory()
        value["samples"][-1].update(status="failed", failureReason="protocol failure")
        report = aggregate_batch(value)
        self.assertFalse(report["performanceAccepted"])
        self.assertEqual(report["retainedSampleRows"][-1]["failureReason"], "protocol failure")

    def test_exact_role_source_and_all_artifact_hashes_are_required(self):
        value = batch_factory()
        row = next(row for row in value["samples"] if row["role"] == "candidate")
        row["sourceCommit"] = "other-head"
        with self.assertRaisesRegex(ValueError, "sourceCommit"):
            aggregate_batch(value)
        for field in ("programSha256", "consumerSha256", "runnerSha256", "runtimeSha256"):
            value = batch_factory()
            next(row for row in value["samples"] if row["role"] == "candidate")[field] = "short"
            with self.assertRaisesRegex(ValueError, field):
                aggregate_batch(value)

    def test_seed_digest_recomputes_semantic_content_and_ignores_paths_opaque_ids(self):
        value = batch_factory()
        rows = value["samples"]
        rows[0]["seedProof"]["sessions"][0]["normalizedContent"]["casePath"] = "/elsewhere"
        rows[0]["seedProof"]["sessions"][0]["normalizedContent"]["opaqueIds"] = ["new"]
        aggregate_batch(value)
        value = batch_factory()
        rows = value["samples"]
        rows[0]["seedProof"]["sessions"][0]["normalizedContent"]["bodyMessages"][0] = "x" * 1024
        with self.assertRaisesRegex(ValueError, "digest"):
            aggregate_batch(value)

    def test_machine_transient_observations_are_not_identity(self):
        value = batch_factory()
        for row in value["samples"]:
            row["machineIdentity"]["unixNs"] += 500
            row["machineIdentity"]["loadAverage"] = [99]
            row["machineAfter"]["unixNs"] += 501
            row["machineAfter"]["loadAverage"] = [77]
        aggregate_batch(value)
        # Battery percentage/remaining time changes remain raw evidence, but
        # stable power source and active mode must stay fixed.
        row = value["samples"][-1]
        install_machine_proof(row, before=machine_observation(percent=99),
                              after=machine_observation(percent=98, unix=99))
        aggregate_batch(value)
        install_machine_proof(row, before=machine_observation(),
                              after=machine_observation(source="Battery Power", unix=99))
        with self.assertRaisesRegex(ValueError, "stable hardware changed"):
            aggregate_batch(value)

    def test_power_mode_and_raw_normalized_identity_are_required(self):
        value = batch_factory()
        row = value["samples"][-1]
        after = machine_observation(sleep=2, unix=99)
        install_machine_proof(row, after=after)
        with self.assertRaisesRegex(ValueError, "changed during sample"):
            aggregate_batch(value)

        value = batch_factory()
        row = value["samples"][-1]
        row.pop("stableMachineIdentity")
        with self.assertRaisesRegex(ValueError, "normalized stable machine identity"):
            aggregate_batch(value)

        value = batch_factory()
        row = value["samples"][-1]
        row["machineIdentity"]["powerMode"]["exitStatus"] = 1
        with self.assertRaisesRegex(ValueError, "missing or failed"):
            aggregate_batch(value)

    def test_snapshot_requires_original_latency_on_every_measured_iteration(self):
        value = batch_factory()
        row = next(row for row in value["samples"] if row["role"] == "candidate" and row["mode"] == "warm")
        events = row["events"]
        start = next(index for index, event in enumerate(events)
                     if event.get("event") == "iteration_started" and event.get("iteration") == 1)
        event = next(event for event in events[start:] if event["event"] == "history_close_open_snapshot"
                     and event["scope"]["sessionId"] == "s0" and event.get("reason") == "iteration")
        event.pop("latencyNs")
        with self.assertRaisesRegex(ValueError, "latencyNs"):
            aggregate_batch(value)

    def test_pi_lifecycle_replaces_children_but_steady_warm_keeps_them(self):
        value = batch_factory()
        lifecycle = next(row for row in value["samples"] if row["endpoint"] == "pi"
                         and row["role"] == "candidate" and row["mode"] == "reconnectLifecycle")
        lifecycle["process"]["warmProcessIdentities"][3]["piRpcChildren"] = \
            lifecycle["process"]["warmProcessIdentities"][2]["piRpcChildren"]
        with self.assertRaisesRegex(ValueError, "replace both RPC child identities"):
            aggregate_batch(value)
        value = batch_factory()
        warm = next(row for row in value["samples"] if row["endpoint"] == "pi"
                    and row["role"] == "candidate" and row["mode"] == "warm")
        warm["process"]["warmProcessIdentities"][4]["piRpcChildren"][0]["pid"] += 1
        with self.assertRaisesRegex(ValueError, "steady-warm Pi RPC"):
            aggregate_batch(value)

    def test_pi_cold_requires_two_rpc_child_process_identities(self):
        value = batch_factory()
        row = next(row for row in value["samples"] if row["endpoint"] == "pi"
                   and row["role"] == "candidate" and row["mode"] == "cold")
        row["process"]["coldHarnessRestart"].pop("measuredPiRpcChildren")
        with self.assertRaisesRegex(ValueError, "both measured RPC child"):
            aggregate_batch(value)

    def test_page20_uses_endpoint_specific_source_count_field(self):
        value = batch_factory()
        pi = next(row for row in value["samples"] if row["endpoint"] == "pi" and row["role"] == "candidate")
        page = next(event for event in pi["events"] if event["event"] == "history_page")
        page.pop("addedMessageRecordCount")
        page["actualAppendMessageCount"] = 20
        with self.assertRaisesRegex(ValueError, "Pi page20"):
            aggregate_batch(value)
        value = batch_factory()
        native = next(row for row in value["samples"] if row["endpoint"] == "native" and row["role"] == "candidate")
        page = next(event for event in native["events"] if event["event"] == "history_page")
        page["addedMessageRecordCount"] = 30
        page["actualAppendMessageCount"] = 19
        with self.assertRaisesRegex(ValueError, "semantic appends"):
            aggregate_batch(value)

    def test_missing_pi_lifecycle_retirement_trace_fails(self):
        value = batch_factory()
        row = next(row for row in value["samples"] if row["endpoint"] == "pi"
                   and row["role"] == "candidate" and row["mode"] == "reconnectLifecycle")
        row["process"]["piRpcLifecycleRetirements"] = []
        with self.assertRaisesRegex(ValueError, "retirement and new set"):
            aggregate_batch(value)

    def test_pi_lifecycle_requires_post_reconnect_send_time_process_censuses(self):
        value = batch_factory()
        row = next(row for row in value["samples"] if row["endpoint"] == "pi"
                   and row["role"] == "candidate" and row["mode"] == "reconnectLifecycle")
        del row["process"]["piRpcLifecycleSendBoundaries"]
        with self.assertRaisesRegex(ValueError, "send-time OS process censuses"):
            aggregate_batch(value)

        value = batch_factory()
        row = next(row for row in value["samples"] if row["endpoint"] == "pi"
                   and row["role"] == "candidate" and row["mode"] == "reconnectLifecycle")
        boundary = row["process"]["piRpcLifecycleSendBoundaries"][0]
        reconnect = next(event for event in row["events"]
                         if event.get("event") == "reconnect" and event.get("iteration") == boundary["round"])
        reconnect["publicationMonotonicNs"] = boundary["originalIntentMonotonicNs"]
        with self.assertRaisesRegex(ValueError, "after its endpoint replacement"):
            aggregate_batch(value)


if __name__ == "__main__":
    unittest.main()
