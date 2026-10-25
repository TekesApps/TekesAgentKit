"""Offline, fail-closed aggregation for frozen #55/#56 performance batches."""
from __future__ import annotations

import hashlib
import json
import math
import re
from collections import defaultdict
from typing import Any


# A cold row is one independently seeded/restarted case. Warm and reconnect
# lifecycle rows are cases containing one excluded round plus ten measured rounds.
CASE_COUNTS = {"cold": 5, "warm": 1, "reconnectLifecycle": 1}
MODE_COUNTS = CASE_COUNTS  # Kept as a compatibility alias for callers.
MEASURED_ITERATIONS = {"cold": 1, "warm": 10, "reconnectLifecycle": 10}
ENDPOINTS = ("native", "pi", "dsh")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
STABLE_MACHINE_FIELDS = ("hardware", "os", "power")
ARTIFACT_SHA_FIELDS = ("programSha256", "consumerSha256", "runnerSha256", "runtimeSha256")


def nearest_rank(values: list[float], percentile: float) -> float:
    if not values:
        raise ValueError("cannot calculate a percentile without samples")
    if not 0 < percentile <= 1:
        raise ValueError("percentile must be in (0, 1]")
    ordered = sorted(values)
    return ordered[math.ceil(percentile * len(ordered)) - 1]


def _need(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _process_identity(value: Any, label: str) -> tuple[int, str]:
    _need(isinstance(value, dict), f"{label} process identity is missing")
    pid, start = value.get("pid"), value.get("startIdentity")
    _need(type(pid) is int and pid > 0 and isinstance(start, str) and start,
          f"{label} process identity needs a positive PID and stable startIdentity")
    return pid, start


def _events(sample: dict[str, Any]) -> list[dict[str, Any]]:
    events = sample.get("events")
    _need(isinstance(events, list) and all(isinstance(row, dict) for row in events),
          "sample events must be an array of original consumer event objects")
    return events


def json_key(value: Any) -> str | None:
    if value is None:
        return None
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def _scope_set(events: list[dict[str, Any]], name: str, endpoint: str) -> set[str]:
    scopes = [row.get("scope") for row in events if row.get("event") == name]
    result = set()
    for scope in scopes:
        _need(isinstance(scope, dict), f"{name} event is missing RunScope")
        _need(scope.get("endpointId") == endpoint, f"{name} endpoint differs from runtime identity")
        session = scope.get("sessionId")
        _need(isinstance(session, str) and session, f"{name} sessionId is missing")
        _need(scope.get("threadId") == session, f"{name} thread/session pointer mismatch")
        result.add(session)
    return result


def _seed_digest(row: dict[str, Any]) -> str:
    """Hash only normalized semantic seed content, never case paths/opaque IDs.

    `normalizedContent` has exactly three semantic inputs: body text, source byte
    digests, and protocol call IDs. `casePath`, timestamps and presentation IDs
    may coexist in the object but are intentionally not part of this digest.
    """
    content = row.get("normalizedContent")
    _need(isinstance(content, dict), "seed proof needs normalizedContent for digest verification")
    bodies = content.get("bodyMessages")
    source_bytes = content.get("sourceBytesSha256")
    call_ids = content.get("callIds")
    _need(isinstance(bodies, list) and len(bodies) == 100
          and all(isinstance(body, str) and len(body.encode("utf-8")) == 1024 for body in bodies),
          "normalized seed body messages must be 100 exact 1024-byte strings")
    _need(isinstance(source_bytes, list) and all(isinstance(item, str) and SHA256.fullmatch(item)
                                                  for item in source_bytes),
          "normalized seed source bytes need SHA-256 digests")
    _need(isinstance(call_ids, list) and len(call_ids) == 10
          and all(isinstance(value, str) and value for value in call_ids),
          "normalized seed must contain exactly ten semantic tool call IDs")
    canonical = {"bodyMessages": bodies, "sourceBytesSha256": source_bytes, "callIds": call_ids}
    return hashlib.sha256(json_key(canonical).encode("utf-8")).hexdigest()


def _validate_provenance(sample: dict[str, Any], role: str, profile: dict[str, Any]) -> None:
    expected = profile["resolvedProductCandidate"] if role == "candidate" else profile["baseline"]
    _need(sample.get("sourceCommit") == expected,
          f"{role} sourceCommit differs from the exact frozen role revision")
    for field in ARTIFACT_SHA_FIELDS:
        value = sample.get(field)
        _need(isinstance(value, str) and SHA256.fullmatch(value),
              f"sample {field} must be a complete SHA-256")


def _stable_machine_identity(value: Any) -> dict[str, Any]:
    _need(isinstance(value, dict), "machine identity is required")
    result = {field: value.get(field) for field in STABLE_MACHINE_FIELDS}
    _need(all(result[field] is not None for field in STABLE_MACHINE_FIELDS),
          "stable machine identity requires hardware, os and power fields")
    # unixNs, monotonicNs, loadAverage and thermal observations stay in source
    # rows for audit, but intentionally do not participate in equality checks.
    return result


def _sample_contract(sample: dict[str, Any], expected_mode: str, role: str,
                     profile: dict[str, Any]) -> dict[str, Any]:
    _need(sample.get("mode") == expected_mode, "sample is in the wrong trajectory")
    _need(sample.get("status") == "success", "failed/not_run sample is retained and blocks acceptance")
    _need(sample.get("diagnosticOnly") is not True, "diagnostic-only evidence cannot become an official sample")
    _need(sample.get("official") is True, "sample is not explicitly marked official")
    _need(isinstance(sample.get("failureReason"), (str, type(None))), "failureReason must be explicit")
    _validate_provenance(sample, role, profile)
    events = _events(sample)
    endpoint = sample.get("endpoint")
    _need(endpoint in ENDPOINTS, "sample must name its runtime endpoint")
    session_ids = sample.get("sessionIds")
    _need(isinstance(session_ids, list) and len(session_ids) == 2
          and all(isinstance(value, str) and value for value in session_ids)
          and len(set(session_ids)) == 2, "profile requires two distinct ordinary sessions")
    seed = sample.get("seedProof")
    _need(isinstance(seed, dict) and isinstance(seed.get("sessions"), list)
          and len(seed["sessions"]) == 2, "public seed readback proof is required")
    session_seed_digests = []
    for row in seed["sessions"]:
        _need(row.get("bodyMessages") == 100 and row.get("toolGroups") == 10,
              "each public seed must contain exactly 100 body messages and 10 tool groups")
        _need(row.get("bodyUtf8Bytes") == [1024] * 100,
              "each seeded body message must retain its exact 1024-byte profile")
        digest = _seed_digest(row)
        _need(row.get("normalizedContentDigest") == digest,
              "normalized seed digest does not match semantic body/source bytes/call IDs")
        session_seed_digests.append(digest)
    _need(sample.get("seedDigest") == hashlib.sha256("".join(session_seed_digests).encode()).hexdigest(),
          "sample seedDigest must bind both normalized session digests")

    started = [row for row in events if row.get("event") == "consumer_started"]
    _need(len(started) == 1 and started[0].get("mode") == expected_mode,
          "missing or duplicate consumer_started event")
    _need(started[0].get("sessionIds") == session_ids, "consumer pointers differ from sample")
    _need(sum(row.get("event") == "runtime_ready" for row in events) == 1,
          "missing or duplicate runtime_ready event")
    _need(sum(row.get("event") == "cleanup_intent" for row in events) == 1
          and sum(row.get("event") == "runtime_cleanup_completed" for row in events) == 1,
          "missing or duplicate runtime cleanup events")

    iters = [row for row in events if row.get("event") == "iteration_started"]
    expected_rounds = 1 if expected_mode == "cold" else 11
    _need(len(iters) == expected_rounds, "case iteration count is incomplete")
    if expected_mode == "cold":
        _need(iters[0].get("iteration") == 0
              and iters[0].get("expectedBodyMessagesBeforePerSession") == 100,
              "cold workload does not start at the preregistered 100 body messages")
        cold_snapshots = [row for row in events if row.get("event") == "snapshot"
                          and row.get("purpose") == "cold_first_usable_original_projection"]
        _need(len(cold_snapshots) == 2 and _scope_set(cold_snapshots, "snapshot", endpoint) == set(session_ids),
              "cold first snapshot must cover both original session scopes")
        _need(all(row.get("usable") is True and isinstance(row.get("latencyNs"), int)
                  and row["latencyNs"] >= 0 for row in cold_snapshots),
              "cold first snapshot requires original connect-to-projection latencyNs")
        first_snapshot_by_session = {row["scope"]["sessionId"]: float(row["latencyNs"])
                                     for row in cold_snapshots}
    else:
        for round_index, row in enumerate(iters):
            _need(row.get("iteration") == round_index, "iteration order is incomplete")
            _need(row.get("expectedBodyMessagesBeforePerSession") == 100 + 2 * round_index,
                  "warm/lifecycle body-message growth differs from preregistration")
        _need(iters[0].get("mode") == f"{expected_mode}_initialization_excluded",
              "excluded initialization iteration is missing")
        _need(all(row.get("mode") == expected_mode for row in iters[1:]),
              "measured iteration mode differs from preregistration")
        first_snapshot_by_session = {session: [] for session in session_ids}

    from performance_event_contract import validate_events
    validate_events(events, expected_mode, session_ids, endpoint)
    scopes = set(session_ids)
    measured = iters if expected_mode == "cold" else iters[1:]
    _need(len(measured) == MEASURED_ITERATIONS[expected_mode],
          "measured iteration count differs from the frozen case profile")
    per_session = {session: {"firstSnapshotNs": [], "firstContentNs": [], "page20Ns": []}
                   for session in session_ids}
    if expected_mode == "cold":
        for session in session_ids:
            per_session[session]["firstSnapshotNs"].append(first_snapshot_by_session[session])

    reconnect_ns = []
    for boundary in measured:
        iteration = boundary.get("iteration")
        start = events.index(boundary)
        later = [i for i, row in enumerate(events) if row.get("event") == "iteration_started" and i > start]
        rows = events[start:later[0] if later else len(events)]
        if expected_mode != "cold":
            snapshots = [row for row in rows if row.get("event") == "history_close_open_snapshot"
                         and row.get("reason") != "restore_pi_runtime_window_50_after_page_probe"]
            _need(len(snapshots) == 2 and _scope_set(snapshots, "history_close_open_snapshot", endpoint) == scopes,
                  "each measured warm/lifecycle iteration needs a fresh public close/open snapshot per scope")
            for row in snapshots:
                _need(row.get("usable") is True and isinstance(row.get("latencyNs"), int)
                      and row["latencyNs"] >= 0,
                      "close/open snapshot requires original history_close_open_snapshot latencyNs")
                per_session[row["scope"]["sessionId"]]["firstSnapshotNs"].append(float(row["latencyNs"]))
        sends = [row for row in rows if row.get("event") == "send"]
        _need(len(sends) == 2 and {row.get("sessionIndex") for row in sends} == {0, 1},
              "each measured iteration needs exactly one send per session")
        for row in sends:
            _need(row.get("round") == iteration and row.get("ticketId"),
                  "send lacks matching round/ticket identity")
            _need(row.get("durableUser") and row.get("firstMatchingAssistantContent")
                  and row.get("durableMatchingAssistantSettlement"),
                  "send lacks durable user, first assistant content, or final settlement")
            latency = row.get("assistantContentObservedLatencyNs")
            _need(isinstance(latency, (int, float)) and latency >= 0,
                  "send lacks original first-assistant-content latency")
            per_session[row["scope"]["sessionId"]]["firstContentNs"].append(float(latency))
        pages = [row for row in rows if row.get("event") == "history_page"]
        _need(len(pages) == 2 and _scope_set(pages, "history_page", endpoint) == scopes,
              "each measured iteration needs page20 for both session scopes")
        for row in pages:
            _need(row.get("requestedMessages") == 20
                  and row.get("priorIdentityRetained") is True
                  and row.get("oldRecordContentAndOrderPreserved") is True,
                  "page20 source/identity contract did not pass")
            method = row.get("pageMethod")
            if isinstance(method, str) and "SessionConversation" in method and "load_earlier_conversation(20)" in method:
                _need(row.get("addedMessageRecordCount") == 20,
                      "Pi page20 must prove exactly twenty added original message records")
            elif isinstance(method, str) and "SessionJournal" in method and "history_page(20)" in method:
                _need(row.get("actualAppendMessageCount") == 20,
                      "journal page20 must prove twenty semantic appends; full raw records may exceed twenty")
            else:
                raise ValueError("page20 must name its source-specific pageMethod")
            latency = row.get("latencyNs")
            _need(isinstance(latency, (int, float)) and latency >= 0,
                  "page20 lacks source-publication latency")
            per_session[row["scope"]["sessionId"]]["page20Ns"].append(float(latency))
        reconnects = [row for row in rows if row.get("event") == "reconnect"]
        if expected_mode == "warm":
            _need(not reconnects, "steady warm must not reconnect")
        else:
            _need(len(reconnects) == 1 and reconnects[0].get("converged") is True
                  and reconnects[0].get("epochAdvanced") is True,
                  "cold/lifecycle iteration lacks converged reconnect")
            latency = reconnects[0].get("latencyNs")
            _need(isinstance(latency, (int, float)) and latency >= 0,
                  "reconnect lacks original convergence latency")
            reconnect_ns.append(float(latency))

    for session, values in per_session.items():
        expected = MEASURED_ITERATIONS[expected_mode]
        for metric in ("firstContentNs", "page20Ns"):
            _need(len(values[metric]) == expected, f"{session} {metric} samples are incomplete")
        _need(len(values["firstSnapshotNs"]) == expected, f"{session} snapshot samples are incomplete")

    cleanup = sample.get("process")
    _need(isinstance(cleanup, dict), "owned process/cleanup receipt is required")
    _process_identity(cleanup.get("consumerRoot"), "consumer root")
    _need(cleanup.get("consumerExitStatus") == 0 and cleanup.get("ownedGroupAbsent") is True,
          "consumer process or process group did not retire cleanly")
    _need(cleanup.get("ownedHarnessExitStatus") == 0, "owned harness did not exit cleanly")
    if expected_mode == "cold":
        restart = cleanup.get("coldHarnessRestart")
        _need(isinstance(restart, dict) and restart.get("seedExitStatus") == 0
              and restart.get("seedStateSealed") is True,
              "cold case must prove sealed seed process exited before measured restart")
        seed_identity = _process_identity(restart.get("seedHarness"), "cold seed harness")
        measured_identity = _process_identity(restart.get("measuredHarness"), "cold measured harness")
        _need(seed_identity != measured_identity, "cold measured harness must be distinct from seed harness")
        if endpoint == "pi":
            children = restart.get("measuredPiRpcChildren")
            _need(isinstance(children, list) and len(children) == 2,
                  "Pi cold ownership must identify both measured RPC child processes")
            _need(len({_process_identity(child, "Pi cold RPC child") for child in children}) == 2,
                  "Pi cold must prove two distinct RPC child process groups")
    else:
        identities = cleanup.get("warmProcessIdentities")
        _need(isinstance(identities, list) and len(identities) == 11,
              "warm/lifecycle must retain excluded-init plus ten measured process identities")
        roots = [_process_identity(row.get("agentkit"), "warm AgentKit") for row in identities]
        _need(len(set(roots)) == 1, "warm/lifecycle AgentKit root PID/start identity changed")
        if endpoint == "pi":
            child_sets = []
            for row in identities:
                children = row.get("piRpcChildren")
                _need(isinstance(children, list) and len(children) == 2,
                      "Pi warm/lifecycle must retain both RPC child identities per round")
                child_sets.append(tuple(_process_identity(child, "Pi RPC child") for child in children))
            if expected_mode == "warm":
                _need(len(set(child_sets)) == 1, "steady-warm Pi RPC child PID/start identities changed")
            else:
                for old_set, new_set in zip(child_sets, child_sets[1:]):
                    _need(len(set(old_set)) == 2 and len(set(new_set)) == 2
                          and set(old_set).isdisjoint(new_set),
                          "Pi lifecycle must replace both RPC child identities on every reconnect")
                retirements = cleanup.get("piRpcLifecycleRetirements")
                _need(isinstance(retirements, list) and len(retirements) == 10
                      and all(row.get("oldSetRetired") is True and row.get("newSetStarted") is True
                              for row in retirements),
                      "Pi lifecycle must trace each old RPC set retirement and new set start")
        else:
            harnesses = [_process_identity(row.get("ownedHarness"), "warm owned harness")
                         for row in identities]
            _need(len(set(harnesses)) == 1, "warm/lifecycle owned harness PID/start identity changed")

    run_pages = [row for row in events if row.get("event") == "run_controller_history_page"]
    _need(len(run_pages) == 2 and _scope_set(run_pages, "run_controller_history_page", endpoint) == scopes
          and all(row.get("requestedMessages") == 50 and row.get("actualAddedMessageCount", 0) >= 50
                  and row.get("priorIdentityRetained") is True for row in run_pages),
          "separate original RunController fixed-50 page must be proven for both scopes")
    cleanup_ms = cleanup.get("cleanupElapsedMs")
    _need(isinstance(cleanup_ms, (int, float)) and cleanup_ms >= 0, "owned cleanup duration is missing")
    rss = cleanup.get("sampledTreePeakRssKiB")
    _need(isinstance(rss, dict), "sampled RSS summary is required")
    for key in ("agentkit", "harness"):
        _need(isinstance(rss.get(key), (int, float)) and rss[key] > 0,
              f"sampled {key} peak RSS is missing")

    return {"sample": sample, "perSession": per_session, "reconnectNs": reconnect_ns,
            "cleanupMs": float(cleanup_ms), "agentkitPeakMiB": float(rss["agentkit"]) / 1024,
            "harnessPeakMiB": float(rss["harness"]) / 1024,
            "iterationCount": len(measured), "caseCount": 1}


def _metric(values: list[float], threshold: float | None, *, limit_statistic: str = "p95") -> dict[str, Any]:
    if not values:
        return {"count": 0, "p50": None, "p95": None, "max": None,
                "absoluteLimit": threshold, "absolutePass": None}
    p95 = nearest_rank(values, .95)
    tested = max(values) if limit_statistic == "max" else p95
    passed = None if threshold is None else tested <= threshold
    return {"count": len(values), "p50": nearest_rank(values, .5), "p95": p95,
            "max": max(values), "absoluteLimit": threshold,
            "absoluteLimitStatistic": limit_statistic, "absoluteValue": tested,
            "absolutePass": passed}


def _role_summary(rows: list[dict[str, Any]], profile: dict[str, Any]) -> dict[str, Any]:
    limits = profile["thresholds"]
    ns_ms = lambda value: value / 1_000_000
    per_session = {}
    session_ids = list(rows[0]["perSession"])
    for index, session in enumerate(session_ids):
        metrics = {key: [ns_ms(value) for row in rows for value in row["perSession"][session][source]]
                   for key, source in (("firstUsableSnapshotMs", "firstSnapshotNs"),
                                       ("sendToFirstAssistantContentMs", "firstContentNs"),
                                       ("historyPage20ToProjectionMs", "page20Ns"))}
        per_session[str(index)] = {
            "sessionId": session,
            "firstUsableSnapshotMs": _metric(metrics["firstUsableSnapshotMs"], limits["firstUsableSnapshotP95Ms"]),
            "sendToFirstAssistantContentMs": _metric(metrics["sendToFirstAssistantContentMs"], limits["sendToFirstPublicFrameP95Ms"]),
            "historyPage20ToProjectionMs": _metric(metrics["historyPage20ToProjectionMs"], limits["historyPageToProjectionP95Ms"]),
        }
    reconnect = [ns_ms(value) for row in rows for value in row["reconnectNs"]]
    return {"caseCount": len(rows), "measuredIterationCount": sum(row["iterationCount"] for row in rows),
            "perSession": per_session,
            # Cross-scope worst values are used for the acceptance budget. The
            # distributions remain separate above so one scope cannot be hidden.
            "firstUsableSnapshotMs": _metric([v for item in per_session.values()
                                                for v in [item["firstUsableSnapshotMs"]["p95"]]],
                                             limits["firstUsableSnapshotP95Ms"]),
            "sendToFirstAssistantContentMs": _metric([v for item in per_session.values()
                                                        for v in [item["sendToFirstAssistantContentMs"]["p95"]]],
                                                     limits["sendToFirstPublicFrameP95Ms"]),
            "historyPage20ToProjectionMs": _metric([v for item in per_session.values()
                                                      for v in [item["historyPage20ToProjectionMs"]["p95"]]],
                                                   limits["historyPageToProjectionP95Ms"]),
            "explicitReconnectToConvergedMs": _metric(reconnect,
                limits["explicitReconnectToConvergedP95Ms"] if reconnect else None),
            "cleanupMs": _metric([row["cleanupMs"] for row in rows], limits["ownedCleanupDeadlineMs"], limit_statistic="max"),
            "agentkitSampledPeakMiB": _metric([row["agentkitPeakMiB"] for row in rows], limits["agentkitPeakRssMiBMax"], limit_statistic="max"),
            "harnessSampledPeakMiB": _metric([row["harnessPeakMiB"] for row in rows], limits["ownedHarnessPeakRssMiBMax"], limit_statistic="max")}


def _paired_inputs(baseline: list[dict[str, Any]], candidate: list[dict[str, Any]]) -> bool:
    fields = ("seedDigest", "workloadDigest", "runtimeSha256")
    for left, right in zip(sorted(baseline, key=lambda x: x["sample"].get("caseIndex", x["sample"].get("iteration"))),
                           sorted(candidate, key=lambda x: x["sample"].get("caseIndex", x["sample"].get("iteration")))):
        if any(json_key(left["sample"].get(field)) != json_key(right["sample"].get(field)) for field in fields):
            return False
        if json_key(_stable_machine_identity(left["sample"].get("machineIdentity"))) != json_key(
                _stable_machine_identity(right["sample"].get("machineIdentity"))):
            return False
    return True


def aggregate_batch(batch: dict[str, Any]) -> dict[str, Any]:
    """Aggregate a frozen official batch. Diagnostics and failures stay non-accepted."""
    profile = batch.get("profile")
    _need(isinstance(profile, dict) and profile.get("format") == 1,
          "the frozen format-1 performance profile is required")
    samples = batch.get("samples")
    _need(isinstance(samples, list), "samples must be explicit and retained in original order")
    expected_official = batch.get("officialSamples")
    if (batch.get("diagnosticOnly") is True or expected_official == 0
            or any(isinstance(row, dict) and row.get("diagnosticOnly") is True for row in samples)):
        return {"officialSamples": 0, "performanceAccepted": False, "diagnosticOnly": True,
                "retainedSampleRows": len(samples),
                "reason": "diagnosticOnly or officialSamples=0 evidence cannot enter acceptance"}
    if type(expected_official) is not int or expected_official != len(samples):
        return {"officialSamples": expected_official if type(expected_official) is int else 0,
                "performanceAccepted": False, "diagnosticOnly": False,
                "retainedSampleRows": samples,
                "reason": "officialSamples does not match retained rows; missing samples are not replaced"}
    _need(batch.get("sourceCandidate") == profile["resolvedProductCandidate"],
          "candidate source differs from frozen profile")
    _need(batch.get("sourceBaseline") == profile["baseline"], "baseline source differs from frozen profile")
    grouped: dict[tuple[str, str, str], list[dict[str, Any]]] = defaultdict(list)
    seen = set()
    for sample in samples:
        _need(isinstance(sample, dict), "sample row must be an object")
        if sample.get("status") != "success":
            return {"officialSamples": len(samples), "performanceAccepted": False,
                    "diagnosticOnly": False, "retainedSampleRows": samples,
                    "failedSample": {"role": sample.get("role"), "mode": sample.get("mode"),
                                     "iteration": sample.get("iteration"), "status": sample.get("status"),
                                     "failureReason": sample.get("failureReason")},
                    "reason": "failed/not_run sample is retained; acceptance stops without replacement"}
        role, mode = sample.get("role"), sample.get("mode")
        endpoint = sample.get("endpoint")
        _need(endpoint in ENDPOINTS and role in ("baseline", "candidate"), "sample endpoint/role is invalid")
        _need(mode in CASE_COUNTS, "sample must use one frozen trajectory")
        case_index = sample.get("caseIndex", sample.get("iteration"))
        _need(type(case_index) is int and case_index >= 0, "sample needs a nonnegative caseIndex")
        key = (endpoint, role, mode, case_index)
        _need(key not in seen, "duplicate case row cannot be replaced")
        seen.add(key)
        grouped[(endpoint, role, mode)].append(_sample_contract(sample, mode, role, profile))

    reports = {}
    covered_endpoints = sorted({sample["endpoint"] for sample in samples})
    for endpoint in ENDPOINTS:
        if endpoint not in covered_endpoints:
            continue
        for mode, required in CASE_COUNTS.items():
            for role in ("baseline", "candidate"):
                rows = sorted(grouped.get((endpoint, role, mode), []),
                              key=lambda row: row["sample"].get("caseIndex", row["sample"].get("iteration")))
                expected_indices = list(range(required)) if mode == "cold" else [0]
                actual_indices = [row["sample"].get("caseIndex", row["sample"].get("iteration")) for row in rows]
                if len(rows) != required or actual_indices != expected_indices:
                    return {"officialSamples": len(samples), "performanceAccepted": False,
                            "diagnosticOnly": False, "retainedSampleRows": samples,
                            "reason": f"{endpoint} {role} {mode} has wrong case count/index; cases are never substituted"}
                signatures = {(row["sample"].get("seedDigest"), row["sample"].get("workloadDigest"),
                              json_key(_stable_machine_identity(row["sample"].get("machineIdentity"))),
                              row["sample"].get("runtimeSha256")) for row in rows}
                _need(len(signatures) == 1 and None not in next(iter(signatures)),
                      f"{endpoint} {role} {mode} case inputs/machine/runtime are not consistent")
                reports[f"{endpoint}/{role}:{mode}"] = _role_summary(rows, profile)
            baseline = grouped[(endpoint, "baseline", mode)]
            candidate = grouped[(endpoint, "candidate", mode)]
            _need(_paired_inputs(baseline, candidate), f"{endpoint} {mode} baseline and candidate are not comparable")
            for metric in ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs",
                           "historyPage20ToProjectionMs", "explicitReconnectToConvergedMs",
                           "cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB"):
                statistic = "max" if metric in ("cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB") else "p95"
                b, c = baseline_metric = reports[f"{endpoint}/baseline:{mode}"][metric][statistic], reports[f"{endpoint}/candidate:{mode}"][metric][statistic]
                if b is None and c is None:
                    ratio = None
                else:
                    _need(isinstance(b, (int, float)) and b > 0 and isinstance(c, (int, float)),
                          f"{endpoint} {mode} {metric} lacks a comparable baseline")
                    ratio = c / b
                report = reports[f"{endpoint}/candidate:{mode}"][metric]
                report["baselineP95"] = b
                report["baselineComparisonStatistic"] = statistic
                report["regressionRatio"] = ratio
                report["regressionLimit"] = (profile["thresholds"]["peakRssRegressionRatioMax"]
                    if "MiB" in metric else profile["thresholds"]["latencyRegressionRatioMax"])
                report["regressionPass"] = None if ratio is None else ratio <= report["regressionLimit"]

    metric_names = ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs", "historyPage20ToProjectionMs",
                    "explicitReconnectToConvergedMs", "cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB")
    accepted = set(covered_endpoints) == set(ENDPOINTS) and all(
        (reports[name][metric]["absolutePass"] is True
         or (metric == "explicitReconnectToConvergedMs" and name.endswith(":warm")
             and reports[name][metric]["absolutePass"] is None))
        and (reports[name][metric].get("regressionPass") is True
             or reports[name][metric].get("regressionPass") is None)
        for name in reports if "/candidate:" in name for metric in metric_names)
    failures = [] if accepted else [
        {"role": name, "metric": metric,
         "absolutePass": report[metric]["absolutePass"],
         "regressionPass": report[metric].get("regressionPass")}
        for name, report in reports.items() if "/candidate:" in name
        for metric in metric_names if report[metric]["absolutePass"] is False
        or report[metric].get("regressionPass") is False]
    return {"officialSamples": len(samples), "diagnosticOnly": False,
            "endpoints": covered_endpoints, "endpointCoverageComplete": set(covered_endpoints) == set(ENDPOINTS),
            "trajectories": reports, "comparableBaseline": True,
            "ordinarySessionPerformanceAccepted": accepted, "performanceAccepted": False,
            "failures": failures, "familyAttachmentPerformanceCovered": False,
            "issue55Issue56Accepted": False,
            "reason": "the frozen profile does not include family/attachment performance workloads"}
