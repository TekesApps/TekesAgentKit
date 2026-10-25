#!/usr/bin/env python3
"""Execute one pre-registered #55/#56 case and emit its original official row.

Registrations are consumed before a case directory or subprocess is created.
This driver never imports/promotes a completed diagnostic receipt. It starts a
new runner after checking every pinned input, then derives the row from that
run's public seed readback, unmodified events, process census and receipts.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import traceback

from performance_official_registration import validate_registration, sha256

ROOT = Path("/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261025/performance")


def _json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def _rows(path: Path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _digest(value) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                    ensure_ascii=True).encode("utf-8")).hexdigest()


def _scope(row: dict, endpoint: str):
    scope = row.get("scope")
    if (not isinstance(scope, dict) or scope.get("endpointId") != endpoint
            or not isinstance(scope.get("sessionId"), str)
            or scope.get("threadId") != scope.get("sessionId")):
        raise ValueError("official event has a missing or mismatched original RunScope")
    return scope["sessionId"]


def _event_jsonl(path: Path):
    return _rows(path)


def _ordered_seed_ids(case: Path) -> list[str]:
    """Read pointers from original seed commands, including Native calls."""
    command_path = case / "seed-commands.json"
    if not command_path.is_file():
        return []
    output_path = case / 'seed.stdout.jsonl'
    creates = [row.get('result', {}) for row in (_rows(output_path) if output_path.is_file() else [])
               if row.get('op') == 'create']
    create_index = 0
    ids = []
    for command in _json(command_path):
        if command.get("op") == "create":
            if create_index >= len(creates):
                return []  # A failed capture may not have reached its create reply.
            pointer = creates[create_index].get('sessionId')
            create_index += 1
        elif command.get("op") == "call" and command.get("method") == "session.create":
            pointer = command.get("args", {}).get("sessionId")
        else:
            continue
        if isinstance(pointer, str) and pointer not in ids:
            ids.append(pointer)
    if create_index != len(creates):
        raise ValueError('public create response count differs from original seed commands')
    return ids


def _source_anchor(position: dict) -> dict:
    anchor = {"kind": position.get("kind")}
    for key in ("sequence", "entryId", "entryOrdinal", "contentIndex"):
        if position.get(key) is not None:
            anchor[key] = position[key]
    return anchor


def _seed_proof(case: Path, ids: list[str], endpoint: str):
    """Revalidate complete public seed snapshots and retain their semantic input."""
    from performance_public_readback import normalize_public_session, _validate_tool_group, _raw_session
    from performance_provider_fixture import BODY_BYTES, read_source_bytes, expected_tool_call_id

    rows = _rows(case / "seed.stdout.jsonl")
    presentations = [row["result"] for row in rows if row.get("op") == "presentation"]
    snapshots = [row["result"] for row in rows if row.get("op") == "snapshot"]
    if len(presentations) != 2 or len(snapshots) != 2:
        raise ValueError("official seed must contain two original presentation and snapshot pairs")
    proofs = []
    ids_in_source_order = []
    for index, session_id in enumerate(ids):
        selected = None
        for presentation, snapshot in zip(presentations, snapshots):
            run_scope = presentation.get("run", {}).get("scope", {})
            if run_scope.get("sessionId") == session_id:
                if session_id in snapshot.get("sessions", {}):
                    selected = presentation, snapshot
                    break
        if selected is None:
            raise ValueError("seed readback does not cover the original session pointer")
        presentation, snapshot = selected
        raw = snapshot["sessions"][session_id]
        proof = normalize_public_session(
            presentation, raw,
            run_scope={"endpointId": endpoint, "threadId": session_id, "sessionId": session_id},
            session_index=index, phase="seed", body_count=100)
        transcript = presentation["transcript"]
        target_anchors = proof["sourceIdentity"]["bodyRows"]
        body_rows = []
        for item in transcript.get("items", []):
            position = item.get("sourcePosition")
            if isinstance(position, dict) and _source_anchor(position) in target_anchors:
                body_rows.append(item)
        texts = [item.get("text") for item in body_rows]
        if (len(texts) != 100 or any(not isinstance(text, str)
                                     or len(text.encode("utf-8")) != BODY_BYTES for text in texts)):
            raise ValueError("public seed did not retain the original 100 ordered 1024-byte bodies")
        tools = transcript.get("tools", [])
        raw_session = _raw_session(raw)
        tool_ids = []
        for turn, tool in enumerate(tools, 1):
            group = _validate_tool_group(raw_session, tool, turn,
                                         expected_tool_call_id(index, turn),
                                         next((item.get("turnId") for item in body_rows
                                               if item.get("kind") == "user"
                                               and item.get("turnId") == tool.get("turnId")), None))
            tool_ids.append(group["callId"])
        if len(tool_ids) != 10:
            raise ValueError("public seed must contain exactly ten original tool call IDs")
        source_bytes = hashlib.sha256(read_source_bytes()).hexdigest()
        normalized = {"bodyMessages": texts, "sourceBytesSha256": [source_bytes] * 10,
                      "callIds": tool_ids}
        normalized_digest = _digest(normalized)
        proofs.append({"bodyMessages": 100, "toolGroups": 10,
                       "bodyUtf8Bytes": [BODY_BYTES] * 100,
                       "normalizedContent": normalized,
                       "normalizedContentDigest": normalized_digest,
                       "publicReadback": proof})
        ids_in_source_order.append(session_id)
    seed_digest = hashlib.sha256("".join(row["normalizedContentDigest"] for row in proofs)
                                 .encode("utf-8")).hexdigest()
    return {"sessionIds": ids_in_source_order, "seedProof": {"sessions": proofs},
            "seedDigest": seed_digest}


def _process_receipt(case: Path, endpoint: str, mode: str):
    consumer = _json(case / "consumer-exit.json")
    retirement = _json(case / "owned-process-retirement.json")
    rss = _json(case / "last-rss-summary.json")
    fixture_rows = _json(case / "fixture-lifecycle.json")
    if retirement.get("status") != "verified" or retirement.get("ownedAgentKitOrHarnessProcessesRemaining"):
        raise ValueError("owned process retirement census did not pass")
    if (type(rss.get("maxObservedIntervalNs")) is not int
            or not 0 < rss["maxObservedIntervalNs"] <= 250_000_000):
        raise ValueError("50ms RSS sampling gap is missing or exceeded the 250ms bound")
    process = {"consumerRoot": {"pid": consumer.get("pid"),
                                "startIdentity": consumer.get("startIdentity")},
               "consumerExitStatus": consumer.get("exitStatus"),
               "ownedGroupAbsent": consumer.get("ownedGroupAbsent"),
               "cleanupElapsedMs": retirement["cleanupElapsedNs"] / 1_000_000,
               "sampledTreePeakRssKiB": rss.get("sampledTreePeakRssKiB"),
               "ownedHarnessExitStatus": 0}
    if mode == "cold":
        seed_consumer = _json(case / "seed-exit.json")
        seed_harness = next((row.get("process") for row in fixture_rows
                             if row.get("operation") == "seed_retire" and row.get("process")), None)
        measured_harness = next((row.get("process") for row in fixture_rows
                                 if row.get("operation") == "resume" and row.get("process")), None)
        if seed_harness is None:
            seed_harness = {"pid": seed_consumer.get("pid"),
                            "startIdentity": seed_consumer.get("startIdentity")}
        if measured_harness is None:
            measured_harness = {"pid": consumer.get("pid"),
                                "startIdentity": consumer.get("startIdentity")}
        process["coldHarnessRestart"] = {
            "seedExitStatus": seed_consumer.get("exitStatus"),
            "seedStateSealed": (case / "seed-public-readback.json").is_file()
                                and seed_consumer.get("exitStatus") == 0,
            "seedHarness": seed_harness, "measuredHarness": measured_harness}
        if endpoint == "pi":
            sidecars = _rows(case / "consumer-process-boundaries.jsonl")
            round_sends = [row for row in sidecars if row.get("event") == "send"
                           and row.get("round") == 0]
            children = []
            for row in round_sends:
                nodes = [item for item in row.get("osProcesses", [])
                         if item.get("executableName") == "node"]
                children.extend(nodes)
            unique = {(row.get("pid"), row.get("startIdentity")): row for row in children}
            if len(round_sends) != 2 or len(unique) != 2:
                raise ValueError("cold Pi sample lacks two measured Node RPC process identities")
            process["coldHarnessRestart"]["measuredPiRpcChildren"] = [
                {"pid": pid, "startIdentity": start} for pid, start in sorted(unique)]
            seed_boundaries = _rows(case / "seed-process-boundaries.jsonl")
            seed_snapshots = [row for row in seed_boundaries
                              if row.get("event") in ("seed_public_snapshot", "seed_public_readback")]
            seed_children = {}
            for boundary in seed_snapshots:
                for child in boundary.get("osProcesses", []):
                    if child.get("role") == "harness" and child.get("executableName") == "node":
                        seed_children[(child.get("pid"), child.get("startIdentity"))] = child
            if not seed_snapshots or len(seed_children) != 2:
                raise ValueError("cold Pi sample lacks two seeder Node RPC identities from raw sidecars")
            measured_set = set(unique)
            if measured_set & set(seed_children):
                raise ValueError("cold Pi seeder and measured Node RPC identities are not disjoint")
            process["coldHarnessRestart"]["seedPiRpcChildren"] = [
                {"pid": pid, "startIdentity": start} for pid, start in sorted(seed_children)]
            seed_root = next((child for boundary in seed_snapshots
                              for child in boundary.get("osProcesses", [])
                              if child.get("role") == "agentkit"), None)
            if not seed_root:
                raise ValueError("cold Pi sample lacks separately observed seeder AgentKit root")
            process["coldHarnessRestart"]["seedAgentKitRoot"] = {
                "pid": seed_root.get("pid"), "startIdentity": seed_root.get("startIdentity")}
            process["coldHarnessRestart"]["seedOwnedGroupAbsent"] = seed_consumer.get("ownedGroupAbsent")
            process["coldHarnessRestart"].pop("seedHarness", None)
            process["coldHarnessRestart"].pop("measuredHarness", None)
    if mode in ("warm", "reconnectLifecycle"):
        boundaries = _rows(case / "consumer-process-boundaries.jsonl")
        starts = [row for row in boundaries if row.get("event") == "iteration_started"]
        sends = [row for row in boundaries if row.get("event") == "send"]
        if len(starts) != 11 or len(sends) != 22:
            raise ValueError("warm identity proof requires all 11 rounds and 22 original send censuses")
        identities = []
        for iteration in range(11):
            round_sends = sorted([item for item in sends if item.get("round") == iteration],
                                 key=lambda item: item.get("sessionIndex", -1))
            if len(round_sends) != 2 or [x.get("sessionIndex") for x in round_sends] != [0, 1]:
                raise ValueError("warm process identities lack both original sends in a round")
            process_rows = round_sends[0].get("osProcesses", [])
            agent = next((x for x in process_rows if x.get("role") == "agentkit"), None)
            item = {"agentkit": {"pid": agent.get("pid"), "startIdentity": agent.get("startIdentity")}
                    if agent else None}
            if endpoint == "pi":
                nodes = [x for x in process_rows if x.get("role") == "harness"
                         and x.get("executableName") == "node"]
                if len(nodes) != 2:
                    raise ValueError("Pi warm send census must identify exactly two Node RPC children")
                item["piRpcChildren"] = [{"pid": x["pid"], "startIdentity": x["startIdentity"]}
                                         for x in nodes]
            else:
                harness = next((x for x in process_rows if x.get("role") == "harness"), None)
                if harness is None:
                    raise ValueError("external warm send census lacks its original harness")
                item["ownedHarness"] = {"pid": harness["pid"], "startIdentity": harness["startIdentity"]}
            identities.append(item)
        process["warmProcessIdentities"] = identities
        if endpoint == "pi" and mode == "warm":
            child_sets = [tuple((x["pid"], x["startIdentity"]) for x in row["piRpcChildren"])
                          for row in identities]
            if len(set(child_sets)) != 1:
                raise ValueError("steady Pi warm children changed process identity")
    if endpoint == "pi" and mode == "reconnectLifecycle":
        boundaries = _rows(case / "consumer-process-boundaries.jsonl")
        events = _event_jsonl(case / "consumer.stdout.jsonl")
        sends = [row for row in boundaries if row.get("event") == "send"]
        if len(sends) != 22 or len([row for row in sends if row.get("round") == 0]) != 2:
            raise ValueError("Pi lifecycle needs one excluded initialization and ten measured send pairs")
        by_round = {}
        for round_index in range(11):
            pair = sorted([row for row in sends if row.get("round") == round_index],
                          key=lambda row: row.get("sessionIndex", -1))
            if len(pair) != 2 or [row.get("sessionIndex") for row in pair] != [0, 1]:
                raise ValueError("Pi lifecycle send census misses an original scope")
            if any(row.get("error") is not None for row in pair):
                raise ValueError("Pi lifecycle send census contains an OS observation error")
            sets = []
            for boundary in pair:
                nodes = [row for row in boundary.get("osProcesses", [])
                         if row.get("role") == "harness" and row.get("executableName") == "node"]
                if len(nodes) != 2 or any(not row.get("startIdentity") for row in nodes):
                    raise ValueError("Pi lifecycle send must observe exactly two original RPC child identities")
                sets.append({(row["pid"], row["startIdentity"]) for row in nodes})
            if sets[0] != sets[1]:
                raise ValueError("same Pi lifecycle round did not retain the same RPC child set across sends")
            by_round[round_index] = (pair, sets[0])
        lifecycle_boundaries = []
        retirements = []
        for round_index in range(1, 11):
            pair, new_set = by_round[round_index]
            previous_set = by_round[round_index - 1][1]
            if new_set == previous_set:
                raise ValueError("Pi lifecycle reconnect did not replace the RPC child identities")
            for boundary in pair:
                observed = {(row.get("pid"), row.get("startIdentity"))
                            for row in boundary.get("osProcesses", [])}
                if previous_set & observed or not new_set.issubset(observed):
                    raise ValueError("Pi lifecycle send census does not prove old retirement and new child presence")
                event_index = boundary.get("rawEventIndex")
                if type(event_index) is not int or not 0 <= event_index < len(events):
                    raise ValueError("Pi lifecycle send census lacks its original event index")
                event = events[event_index]
                if (event.get("event") != "send" or event.get("round") != round_index
                        or event.get("sessionIndex") != boundary.get("sessionIndex")
                        or _scope(event, "pi") != event.get("scope", {}).get("sessionId")):
                    raise ValueError("Pi lifecycle send census does not align with its raw event")
                # Retain the complete original sidecar row so the reporter can
                # independently resolve rawEventIndex, scope and process ids.
                lifecycle_boundaries.append(dict(boundary, scopeSessionId=event["scope"]["sessionId"]))
            reconnect = next((event for event in events if event.get("event") == "reconnect"
                              and event.get("iteration") == round_index), None)
            if not isinstance(reconnect, dict) or reconnect.get("converged") is not True:
                raise ValueError("Pi lifecycle send was not preceded by same-round reconnect convergence")
            if events.index(reconnect) >= min(row["rawEventIndex"] for row in pair):
                raise ValueError("Pi lifecycle reconnect appears after its measured send")
            if reconnect.get("endpointGenerationReplaced") is not True:
                raise ValueError("Pi lifecycle reconnect did not replace the endpoint generation")
            publication = reconnect.get("publicationMonotonicNs")
            if type(publication) is not int or any(row.get("originalIntentMonotonicNs", 0) <= publication
                                                   for row in pair):
                raise ValueError("Pi lifecycle send intent did not follow the reconnect publication")
            retirements.append({"oldSetRetired": True, "newSetStarted": True,
                                "oldChildrenAbsentAtBothSendCensuses": True,
                                "newChildrenObservedAtBothSendCensuses": True,
                                "round": round_index,
                                "oldChildren": [{"pid": pid, "startIdentity": start}
                                                for pid, start in sorted(previous_set)],
                                "newChildren": [{"pid": pid, "startIdentity": start}
                                                for pid, start in sorted(new_set)],
                                "reconnectPublicationMonotonicNs": publication})
        process["piRpcLifecycleSendBoundaries"] = lifecycle_boundaries
        process["piRpcLifecycleRetirements"] = retirements
        # The report's 11 process snapshots consist of the excluded init
        # send plus each measured round's post-reconnect send child set.
        identities = process.get("warmProcessIdentities")
        if not isinstance(identities, list) or len(identities) != 11:
            raise ValueError("Pi lifecycle requires 11 original round process identities")
        for index in range(11):
            pair, children = by_round[index]
            identities[index]["piRpcChildren"] = [
                {"pid": pid, "startIdentity": start} for pid, start in sorted(children)]
        process["warmProcessIdentities"] = identities
    if endpoint != "pi" and fixture_rows:
        retirements = [row for row in fixture_rows if row.get("operation") in
                       ("seed_retire", "measurement_retire")]
        if any(row.get("exitStatus") != 0 for row in retirements):
            raise ValueError("owned external harness did not retire successfully")
        process["ownedHarnessExitStatus"] = 0
    return process


def _make_sample(case: Path, pins: dict, returncode: int, stdout: str, stderr: str):
    reg = pins["registration"]
    profile = pins["profile"]
    sample = {"sampleId": reg["sampleId"], "role": reg["role"],
              "mode": reg["mode"], "caseIndex": reg["caseIndex"],
              "iteration": reg["caseIndex"], "endpoint": reg["endpoint"],
              "official": True, "diagnosticOnly": False,
              "sourceCommit": reg["sourceCommit"],
              "programSha256": reg["programSha256"],
              "consumerSha256": reg["consumerSha256"],
              "runnerSha256": reg["runnerSha256"],
              "runtimeSha256": reg["runtimeSha256"],
              "runtimeIdentity": {"criticalFiles": reg["runtimeFiles"]},
              "workloadDigest": _digest(profile["dataset"]), "process": {}}
    input_path, after_path = case / "inputs.json", case / "machine-after.json"
    if input_path.is_file():
        before = _json(input_path).get("machineBefore")
        if isinstance(before, dict):
            sample["machineIdentity"] = before
    if after_path.is_file():
        sample["machineAfter"] = _json(after_path)
    machine_error = None
    if "machineIdentity" in sample and "machineAfter" in sample:
        try:
            from performance_sample_report import _stable_machine_identity
            stable_before = _stable_machine_identity(sample["machineIdentity"])
            stable_after = _stable_machine_identity(sample["machineAfter"])
            if stable_before != stable_after:
                machine_error = "stable hardware/OS/power identity changed during official sample"
            else:
                sample["stableMachineIdentity"] = stable_before
        except Exception as error:
            machine_error = "machine identity proof failed: " + str(error)
    events_path = case / "consumer.stdout.jsonl"
    events = _event_jsonl(events_path) if events_path.is_file() else []
    ids = _ordered_seed_ids(case)
    failure_ids = ids if len(ids) == 2 and len(set(ids)) == 2 else []
    receipt_path = case / "official-execution-receipt.json"
    if returncode != 0 or not receipt_path.is_file():
        sample.update({"status": "failed", "failureReason":
                       machine_error or stderr[-4000:] or stdout[-4000:] or
                       "runner failed before producing its receipt",
                       "events": events, "sessionIds": failure_ids,
                       "seedProof": {"sessions": []}, "seedDigest": "", "process": {}})
        return sample
    inner = _json(receipt_path)
    if (inner.get("official") is not True or inner.get("diagnosticOnly") is not False
            or inner.get("status") != "success" or inner.get("error") is not None):
        sample.update({"status": "failed", "failureReason": machine_error or
                       inner.get("failureReason") or inner.get("error") or
                       "registered official execution failed a required source/event check",
                       "events": events, "sessionIds": failure_ids,
                       "seedProof": {"sessions": []}, "seedDigest": "", "process": {}})
        return sample
    if machine_error:
        sample.update({"status": "failed", "failureReason": machine_error,
                       "events": events, "sessionIds": failure_ids,
                       "seedProof": {"sessions": []}, "seedDigest": "", "process": {}})
        return sample
    if len(ids) != 2 or len(set(ids)) != 2:
        raise ValueError("official run must preserve two distinct session pointers in seed order")
    started = [event for event in events if event.get("event") == "consumer_started"]
    if len(started) != 1 or started[0].get("sessionIds") != ids:
        raise ValueError("original consumer scopes differ from ordered public seed create pointers")
    seed = _seed_proof(case, ids, reg["endpoint"])
    if reg["mode"] == "cold" and reg["caseIndex"] >= 5:
        raise ValueError("cold case index exceeds the five fixed samples")
    for row in events:
        if row.get("event") in ("snapshot", "send", "history_page", "run_controller_history_page"):
            _scope(row, reg["endpoint"])
    sample.update({"status": "success", "failureReason": None, "sessionIds": ids,
                   "events": events, **seed})
    sample["process"] = _process_receipt(case, reg["endpoint"], reg["mode"])
    return sample


def execute(registration_path: Path) -> Path:
    # This entire preflight is read-only and deliberately precedes mkdir/Popen.
    pins = validate_registration(registration_path,
                                 driver_path=Path(sys.executable).resolve(strict=True),
                                 official_runner_path=Path(__file__),
                                 caller_role="outer")
    reg = pins["registration"]
    registered_sha = sha256(Path(registration_path))
    case = ROOT / reg["caseName"]
    if case.exists() or case.is_symlink():
        raise ValueError("registered case name already exists; samples are never replaced")
    commands = [str(pins["driver"]), str(pins["caseRunner"]),
                "--endpoint", reg["endpoint"], "--mode", reg["mode"],
                "--case-name", reg["caseName"],
                "--official-registration", str(Path(registration_path).resolve(strict=True)),
                "--program", str(pins["program"]),
                "--program-receipt", reg["programReceiptPath"],
                "--seeder", str(pins["seeder"]),
                "--seeder-receipt", reg["seederReceiptPath"],
                "--profile", str(pins["profilePath"])]
    for key in ("kernel-supervisor", "kernel-worker", "providers-config", "pi-root", "dsh-root"):
        value = reg["runtimeArguments"].get(key)
        if value is not None:
            commands += ["--" + key, value]
    # The inner runner creates the owned case after independently consuming
    # the same registration. No case files or process exist before this point.
    result = subprocess.run(commands, cwd=ROOT, capture_output=True, text=True,
                            timeout=1800, check=False)
    if not case.is_dir():
        failure_path = Path(registration_path).with_suffix('.execution-failure.json')
        with failure_path.open('x', encoding='utf-8') as stream:
            json.dump({'registrationSha256': sha256(Path(registration_path)),
                       'measurementStarted': False, 'exitStatus': result.returncode,
                       'stdout': result.stdout, 'stderr': result.stderr}, stream, indent=2)
            stream.write('\n')
        raise ValueError("registered runner returned without creating its owned evidence directory")
    registration_changed = sha256(Path(registration_path)) != registered_sha
    if registration_changed:
        (case / "registration-mutation-detected.json").write_text(json.dumps({
            "status": "failed", "reason": "registration contents changed while its sample ran",
            "originalSha256": registered_sha,
            "afterSha256": sha256(Path(registration_path))}, indent=2) + "\n")
    (case / "official-runner.stdout.log").write_text(result.stdout, encoding="utf-8")
    (case / "official-runner.stderr.log").write_text(result.stderr, encoding="utf-8")
    try:
        if registration_changed:
            raise ValueError("official registration changed during the measured sample")
        # Recheck every registered source/runtime/program pin after execution.
        validate_registration(registration_path,
                              driver_path=Path(sys.executable).resolve(strict=True),
                              official_runner_path=Path(__file__), caller_role="outer")
        row = _make_sample(case, pins, result.returncode, result.stdout, result.stderr)
        row["registrationSha256"] = registered_sha
        row["executionReceiptSha256"] = sha256(case / "official-execution-receipt.json") if (
            case / "official-execution-receipt.json").is_file() else None
        row["rawConsumerEventsSha256"] = sha256(case / "consumer.stdout.jsonl") if (
            case / "consumer.stdout.jsonl").is_file() else None
        (case / "official-sample.json").write_text(json.dumps(row, ensure_ascii=False,
                                                               indent=2) + "\n", encoding="utf-8")
    except Exception as error:
        (case / "official-sample-materialization-error.json").write_text(json.dumps({
            "official": True, "diagnosticOnly": False, "status": "failed",
            "sampleId": reg["sampleId"], "failureReason": type(error).__name__ + ": " + str(error),
            "registrationSha256": sha256(Path(registration_path)),
            "traceback": traceback.format_exc()}, ensure_ascii=False, indent=2) + "\n")
        raise
    return case / "official-sample.json"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registration", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(execute(args.registration.resolve(strict=True)))
        return 0
    except Exception as error:
        print(f"OFFICIAL_CASE_PREFLIGHT_OR_CAPTURE_FAILED: {type(error).__name__}: {error}",
              file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
