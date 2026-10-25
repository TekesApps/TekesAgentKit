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
ARTIFACT_SHA_FIELDS = ("programSha256", "consumerSha256", "runnerSha256", "runtimeSha256")

_MODE_SECTIONS = ("Battery Power", "AC Power")


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


def _command_output(observation: Any, label: str) -> str:
    _need(isinstance(observation, dict) and observation.get("exitStatus") == 0,
          f"raw {label} observation is missing or failed")
    stdout = observation.get("stdout")
    _need(isinstance(stdout, str) and stdout.strip(), f"raw {label} stdout is missing")
    return stdout


def _stable_power_modes(stdout: str) -> dict[str, dict[str, str]]:
    modes: dict[str, dict[str, str]] = {}
    active: str | None = None
    for raw_line in stdout.splitlines():
        line = raw_line.strip().rstrip(":")
        if line in _MODE_SECTIONS:
            _need(line not in modes, "raw powerMode repeats a power profile section")
            active = line
            modes[active] = {}
            continue
        if not line:
            continue
        _need(active is not None, "raw powerMode setting precedes its section")
        match = re.fullmatch(r"(.+?)\s+([^\s]+)", line)
        _need(match is not None, "raw powerMode setting is malformed")
        key, value = match.groups()
        _need(key not in modes[active], "raw powerMode setting is duplicated")
        modes[active][key] = value
    _need(all(section in modes and modes[section] for section in _MODE_SECTIONS),
          "raw powerMode must contain nonempty Battery Power and AC Power profiles")
    return modes


def _stable_machine_identity(value: Any) -> dict[str, Any]:
    """Extract stable identity from the full raw macOS observation.

    Battery charge/remaining time and the observation clocks stay in the raw
    receipt but do not define a comparable power configuration. Current power
    source and both configured power profiles do define it.
    """
    _need(isinstance(value, dict), "raw machine identity is required")
    os_text = _command_output(value.get("os"), "sw_vers")
    hardware_text = _command_output(value.get("hardware"), "sysctl")
    power_text = _command_output(value.get("power"), "pmset -g batt")
    modes_text = _command_output(value.get("powerMode"), "pmset -g custom")

    os_values = {}
    for line in os_text.splitlines():
        match = re.fullmatch(r"(ProductName|ProductVersion|BuildVersion):\s*(.+)", line.strip())
        if match:
            os_values[match.group(1)] = match.group(2)
    _need(set(os_values) == {"ProductName", "ProductVersion", "BuildVersion"},
          "raw sw_vers output is incomplete")

    hardware_values = {}
    for line in hardware_text.splitlines():
        match = re.fullmatch(r"hw\.(model|memsize|ncpu):\s*(\S+)", line.strip())
        if match:
            hardware_values[match.group(1)] = match.group(2)
    _need(set(hardware_values) == {"model", "memsize", "ncpu"},
          "raw sysctl output is incomplete")

    source = re.search(r"^Now drawing from '([^']+)'\s*$", power_text, re.MULTILINE)
    _need(source is not None and source.group(1) in ("AC Power", "Battery Power", "UPS Power"),
          "raw pmset battery output has no recognized current power source")
    return {"os": os_values, "hardware": hardware_values,
            "powerSource": source.group(1), "powerModes": _stable_power_modes(modes_text)}


def _sample_machine_identity(sample: dict[str, Any]) -> dict[str, Any]:
    before = _stable_machine_identity(sample.get("machineIdentity"))
    after = _stable_machine_identity(sample.get("machineAfter"))
    _need(before == after, "machine power source/mode or stable hardware changed during sample")
    normalized = sample.get("stableMachineIdentity")
    _need(isinstance(normalized, dict), "normalized stable machine identity must be retained with raw observations")
    _need(normalized == before,
          "retained stable machine identity differs from normalization of raw before/after observations")
    return before


def _sample_contract(sample: dict[str, Any], expected_mode: str, role: str,
                     profile: dict[str, Any]) -> dict[str, Any]:
    _need(sample.get("mode") == expected_mode, "sample is in the wrong trajectory")
    _need(sample.get("status") == "success", "failed/not_run sample is retained and blocks acceptance")
    _need(sample.get("diagnosticOnly") is not True, "diagnostic-only evidence cannot become an official sample")
    _need(sample.get("official") is True, "sample is not explicitly marked official")
    _need(isinstance(sample.get("failureReason"), (str, type(None))), "failureReason must be explicit")
    _validate_provenance(sample, role, profile)
    _sample_machine_identity(sample)
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
    for event in events:
        if event.get("event") != "send":
            continue
        index = event.get("sessionIndex")
        _need(type(index) is int and index in (0, 1),
              "original send must name its stable sessionIndex")
        _need(event.get("scope", {}).get("sessionId") == session_ids[index],
              "original send sessionIndex does not match its public session pointer")
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
                  and reconnects[0].get("epochAdvanced") is True
                  and reconnects[0].get("iteration", iteration) == iteration,
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
        if endpoint == "pi":
            seed_root = _process_identity(restart.get("seedAgentKitRoot"), "Pi seed AgentKit root")
            _need(seed_root != _process_identity(cleanup.get("consumerRoot"), "measured AgentKit root")
                  and restart.get("seedOwnedGroupAbsent") is True,
                  "Pi cold seed AgentKit and its owned group must retire before measurement")
            children = restart.get("measuredPiRpcChildren")
            _need(isinstance(children, list) and len(children) == 2,
                  "Pi cold ownership must identify both measured RPC child processes")
            _need(len({_process_identity(child, "Pi cold RPC child") for child in children}) == 2,
                  "Pi cold must prove two distinct RPC child process groups")
            seed_children = restart.get("seedPiRpcChildren")
            _need(isinstance(seed_children, list) and len(seed_children) == 2,
                  "Pi cold must retain both original seeder RPC process identities")
            seed_set = {_process_identity(child, "Pi seed RPC child") for child in seed_children}
            measured_set = {_process_identity(child, "Pi measured RPC child") for child in children}
            _need(len(seed_set) == 2 and seed_set.isdisjoint(measured_set),
                  "Pi cold seeder and measured RPC process identities must be disjoint")
        else:
            seed_identity = _process_identity(restart.get("seedHarness"), "cold seed harness")
            measured_identity = _process_identity(restart.get("measuredHarness"), "cold measured harness")
            _need(seed_identity != measured_identity, "cold measured harness must be distinct from seed harness")
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
                boundaries = cleanup.get("piRpcLifecycleSendBoundaries")
                _need(isinstance(boundaries, list) and len(boundaries) == 20,
                      "Pi lifecycle requires two send-time OS process censuses for every measured round")
                boundary_by_round: dict[int, list[dict[str, Any]]] = defaultdict(list)
                reconnects_by_round = {}
                original_round = None
                for original_event in events:
                    if original_event.get('event') == 'iteration_started':
                        original_round = original_event.get('iteration')
                    elif original_event.get('event') == 'reconnect':
                        _need(original_event.get('iteration', original_round) == original_round,
                              'reconnect metadata contradicts its original iteration marker')
                        reconnects_by_round[original_round] = original_event
                for boundary in boundaries:
                    _need(isinstance(boundary, dict) and boundary.get("error") is None
                          and isinstance(boundary.get("osProcesses"), list),
                          "Pi lifecycle send-time process census is missing or errored")
                    raw_index = boundary.get("rawEventIndex")
                    _need(type(raw_index) is int and 0 <= raw_index < len(events),
                          "Pi lifecycle send census does not bind a raw consumer event index")
                    event = events[raw_index]
                    round_index = boundary.get("round")
                    session_index = boundary.get("sessionIndex")
                    _need(event.get("event") == "send" and event.get("round") == round_index
                          and event.get("sessionIndex") == session_index
                          and event.get("scope", {}).get("sessionId") == boundary.get("scopeSessionId"),
                          "Pi lifecycle OS census is not tied to the corresponding original send scope")
                    _need(round_index in range(1, 11) and session_index in (0, 1),
                          "Pi lifecycle census is outside the ten measured rounds or two original scopes")
                    reconnect = reconnects_by_round.get(round_index)
                    _need(isinstance(reconnect, dict) and reconnect.get("converged") is True
                          and reconnect.get("endpointGenerationReplaced") is True
                          and events.index(reconnect) < raw_index,
                          "Pi lifecycle send census does not follow the same round's completed reconnect")
                    reconnect_publication = reconnect.get("publicationMonotonicNs")
                    send_intent = boundary.get("originalIntentMonotonicNs")
                    _need(type(reconnect_publication) is int and type(send_intent) is int
                          and send_intent > reconnect_publication,
                          "Pi lifecycle send must begin after its endpoint replacement publication")
                    observed = {(item.get("pid"), item.get("startIdentity")): item
                                for item in boundary["osProcesses"] if isinstance(item, dict)}
                    node_ids = {identity for identity, item in observed.items()
                                if item.get("role") == "harness"
                                and item.get("executableName") == "node"}
                    reported = child_sets[round_index]
                    _need(len(node_ids) == 2 and node_ids == set(reported),
                          "Pi lifecycle send census does not observe exactly its reported RPC child processes")
                    _need(roots[round_index] in observed,
                          "Pi lifecycle send census belongs to a different AgentKit process")
                    boundary_by_round[round_index].append(boundary)
                _need(set(boundary_by_round) == set(range(1, 11))
                      and all(len(rows_for_round) == 2
                              and {row.get("sessionIndex") for row in rows_for_round} == {0, 1}
                              for rows_for_round in boundary_by_round.values()),
                      "each measured Pi lifecycle round must bind both original send boundaries")
                for round_index in range(1, 11):
                    new_ids = set(child_sets[round_index])
                    for boundary in boundary_by_round[round_index]:
                        observed_ids = {(item.get("pid"), item.get("startIdentity"))
                                        for item in boundary["osProcesses"] if isinstance(item, dict)}
                        _need(new_ids.issubset(observed_ids),
                              "new Pi RPC children are absent from the after-reconnect send census")
                        _need(not set(child_sets[round_index - 1]).intersection(observed_ids),
                              "retired Pi RPC children remain in the after-reconnect send census")
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
    for index in range(2):
        session_ids = [row["sample"]["sessionIds"][index] for row in rows]
        metrics = {key: [ns_ms(value) for row, session in zip(rows, session_ids)
                         for value in row["perSession"][session][source]]
                   for key, source in (("firstUsableSnapshotMs", "firstSnapshotNs"),
                                       ("sendToFirstAssistantContentMs", "firstContentNs"),
                                       ("historyPage20ToProjectionMs", "page20Ns"))}
        per_session[str(index)] = {
            "sessionIds": session_ids,
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
        if json_key(_sample_machine_identity(left["sample"])) != json_key(
                _sample_machine_identity(right["sample"])):
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
    failed_samples = []
    for sample in samples:
        _need(isinstance(sample, dict), "sample row must be an object")
        role, mode = sample.get("role"), sample.get("mode")
        endpoint = sample.get("endpoint")
        _need(endpoint in ENDPOINTS and role in ("baseline", "candidate"), "sample endpoint/role is invalid")
        _need(mode in CASE_COUNTS, "sample must use one frozen trajectory")
        case_index = sample.get("caseIndex", sample.get("iteration"))
        _need(type(case_index) is int and case_index >= 0, "sample needs a nonnegative caseIndex")
        key = (endpoint, role, mode, case_index)
        _need(key not in seen, "duplicate case row cannot be replaced")
        seen.add(key)
        if sample.get("status") != "success":
            failed_samples.append({"role": role, "mode": mode, "endpoint": endpoint,
                                   "iteration": sample.get("iteration"), "caseIndex": case_index,
                                   "status": sample.get("status"),
                                   "failureReason": sample.get("failureReason")})
            continue
        grouped[(endpoint, role, mode)].append(_sample_contract(sample, mode, role, profile))

    reports = {}
    missing_case_groups = []
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
                    missing_case_groups.append({
                        "endpoint": endpoint, "role": role, "mode": mode,
                        "expectedCaseIndices": expected_indices,
                        "actualCaseIndices": actual_indices,
                        "missingCaseIndices": [index for index in expected_indices
                                               if index not in actual_indices],
                        "unexpectedCaseIndices": [index for index in actual_indices
                                                  if index not in expected_indices],
                        "expectedCaseCount": required, "actualCaseCount": len(rows),
                    })
                    # Keep any otherwise-complete trajectories visible. An
                    # extra index never stands in for a missing frozen case.
                    continue
                signatures = {(row["sample"].get("seedDigest"), row["sample"].get("workloadDigest"),
                              json_key(_sample_machine_identity(row["sample"])),
                              row["sample"].get("runtimeSha256")) for row in rows}
                _need(len(signatures) == 1 and None not in next(iter(signatures)),
                      f"{endpoint} {role} {mode} case inputs/machine/runtime are not consistent")
                reports[f"{endpoint}/{role}:{mode}"] = _role_summary(rows, profile)
            baseline = grouped.get((endpoint, "baseline", mode))
            candidate = grouped.get((endpoint, "candidate", mode))
            if (f"{endpoint}/baseline:{mode}" not in reports
                    or f"{endpoint}/candidate:{mode}" not in reports):
                continue
            _need(_paired_inputs(baseline, candidate), f"{endpoint} {mode} baseline and candidate are not comparable")
            for metric in ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs",
                           "historyPage20ToProjectionMs", "explicitReconnectToConvergedMs",
                           "cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB"):
                statistic = "max" if metric in ("cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB") else "p95"
                b, c = baseline_metric = reports[f"{endpoint}/baseline:{mode}"][metric][statistic], reports[f"{endpoint}/candidate:{mode}"][metric][statistic]
                per_scope_ratios = []
                if metric in ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs",
                              "historyPage20ToProjectionMs"):
                    for index in range(2):
                        baseline_scope = reports[f"{endpoint}/baseline:{mode}"]["perSession"][str(index)][metric]
                        candidate_scope = reports[f"{endpoint}/candidate:{mode}"]["perSession"][str(index)][metric]
                        scope_b = baseline_scope["p95"]
                        scope_c = candidate_scope["p95"]
                        _need(isinstance(scope_b, (int, float)) and scope_b > 0
                              and isinstance(scope_c, (int, float)),
                              f"{endpoint} {mode} scope {index} {metric} lacks a comparable baseline")
                        scope_ratio = scope_c / scope_b
                        scope_limit = profile["thresholds"]["latencyRegressionRatioMax"]
                        candidate_scope["baselineP95"] = scope_b
                        candidate_scope["baselineComparisonStatistic"] = "p95"
                        candidate_scope["regressionRatio"] = scope_ratio
                        candidate_scope["regressionLimit"] = scope_limit
                        candidate_scope["regressionPass"] = scope_ratio <= scope_limit
                        per_scope_ratios.append(scope_ratio)
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
                report["regressionPass"] = (None if ratio is None else
                    all(scope_ratio <= report["regressionLimit"] for scope_ratio in per_scope_ratios)
                    if per_scope_ratios else ratio <= report["regressionLimit"])

    metric_names = ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs", "historyPage20ToProjectionMs",
                    "explicitReconnectToConvergedMs", "cleanupMs", "agentkitSampledPeakMiB", "harnessSampledPeakMiB")
    accepted = (set(covered_endpoints) == set(ENDPOINTS) and not missing_case_groups
        and not failed_samples and all(
        (reports[name][metric]["absolutePass"] is True
         or (metric == "explicitReconnectToConvergedMs" and name.endswith(":warm")
             and reports[name][metric]["absolutePass"] is None))
        and (reports[name][metric].get("regressionPass") is True
             or reports[name][metric].get("regressionPass") is None)
        for name in reports if "/candidate:" in name for metric in metric_names) and all(
        item[metric]["absolutePass"] is True and item[metric].get("regressionPass") is True
        for name, report in reports.items() if "/candidate:" in name
        for item in report.get("perSession", {}).values()
        for metric in ("firstUsableSnapshotMs", "sendToFirstAssistantContentMs",
                       "historyPage20ToProjectionMs")))
    failures = [] if accepted else [
        {"role": name, "metric": metric,
         "absolutePass": report[metric]["absolutePass"],
         "regressionPass": report[metric].get("regressionPass")}
        for name, report in reports.items() if "/candidate:" in name
        for metric in metric_names if report[metric]["absolutePass"] is False
        or report[metric].get("regressionPass") is False]
    endpoint_coverage_complete = set(covered_endpoints) == set(ENDPOINTS)
    case_group_coverage_complete = endpoint_coverage_complete and not missing_case_groups
    return {"officialSamples": len(samples), "diagnosticOnly": False,
            "endpoints": covered_endpoints, "endpointCoverageComplete": endpoint_coverage_complete,
            "caseGroupCoverageComplete": case_group_coverage_complete,
            "missingCaseGroups": missing_case_groups,
            "trajectories": reports, "comparableBaseline": case_group_coverage_complete,
            "ordinarySessionPerformanceAccepted": accepted, "performanceAccepted": False,
            "failures": failures, "familyAttachmentPerformanceCovered": False,
            "issue55Issue56Accepted": False,
            **({"failedSamples": failed_samples, "failedSample": failed_samples[0]}
               if failed_samples else {}),
            **({"retainedSampleRows": samples} if missing_case_groups or failed_samples else {}),
            "reason": ("failed/not_run samples are retained and prevent acceptance"
                       if failed_samples else
                       "one or more frozen case groups are incomplete"
                       if missing_case_groups else
                       "the frozen profile does not include family/attachment performance workloads")}
