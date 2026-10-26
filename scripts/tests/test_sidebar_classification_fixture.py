"""Synthetic refusal tests for the public-only #29 classification evidence helper.

These fixtures exercise parser and validator boundaries only. They are not real
harness, UI, archive, or history acceptance evidence and use no runtime files.
"""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import sidebar_classification_fixture as classification


def _summary_for(role: str, parent: str) -> dict:
    if role == "parent":
        # Deliberately unknown values must stay unknown in the published facts.
        return {"origin": "future-origin", "parentSessionId": "  ", "ephemeral": None}
    if role == "child":
        return {"origin": "subagent", "parentSessionId": parent, "ephemeral": False}
    if role == "ephemeral":
        return {"origin": "fork", "parentSessionId": parent, "ephemeral": True}
    return {"origin": "fork", "parentSessionId": parent, "ephemeral": False}


def _synthetic_exchange(kind: str):
    native_source = {"cwd": "/synthetic/project", "provider": "loopback", "model": "fixture"}
    commands, samples = classification.classification_commands(
        kind,
        "supplied-parent",
        ["supplied-child"] if kind == "pi" else [],
        native_source=native_source if kind == "native" else None,
    )

    aliases: dict[str, str] = {}
    roles: dict[str, str] = {}
    parent_id = "classification-parent" if kind == "native" else "supplied-parent"
    for role, reference in samples:
        if reference in ("visible-fork", "archived-fork", "ephemeral"):
            continue
        roles[reference] = role
    outputs = [{"kind": kind, "ready": True}]
    snapshot_number = 0

    def resolve(value: str) -> str:
        return aliases.get(value, value)

    def all_states(phase: int) -> dict:
        state_sessions = {}
        archived_id = resolve("archived-fork")
        for role, reference in samples:
            session_id = resolve(reference)
            summary = _summary_for(role, parent_id)
            if kind == "dsh":
                summary.pop("ephemeral")
            archived = role == "archived-fork" and phase in (1, 3)
            state_sessions[session_id] = {
                # DSH may retain the raw summary while the same-frame workspace
                # archived-ID list removes the row from active inventory.
                "summary": summary if kind == "dsh" or not archived else None,
                "journal_ready": not (role == "archived-fork" and phase == 1),
                "history_through_sequence": 3,
                "history_revision": 7 + phase,
            }
        return {
            "endpoint_id": kind,
            "inventory_ready": True,
            "workspace_ready": kind == "dsh" and phase == 3,
            "archived_session_ids": [archived_id] if kind == "dsh" and phase == 3 else [],
            "sessions": state_sessions,
        }

    for command in commands:
        op = command["op"]
        result: dict
        method = command.get("method")
        session = resolve(command.get("sessionId", ""))
        if method == "session.fork":
            created = f"synthetic-{command['saveAs']}"
            aliases[command["saveAs"]] = created
            result = {"sessionId": created}
        elif op == "create":
            created = command.get("saveAs", "synthetic-parent")
            aliases[command.get("saveAs", "parent")] = created
            result = {"sessionId": created}
        elif method == "session.archive":
            result = {"archivedSessionIds": [session]}
        elif method == "session.unarchive":
            result = {"archivedSessionIds": []} if kind == "dsh" else {"sessionId": session}
        elif op == "presentation":
            role = roles.get(command.get("sessionId"))
            if role is None:
                role = next((sample_role for sample_role, reference in samples
                             if resolve(reference) == session), "visible-fork")
            result = {
                "summary": {"scope": {"endpointId": kind, "threadId": session, "sessionId": session}},
                "transcript": {"items": [{"id": f"{session}-item-1", "text": "stable synthetic history"}]},
            }
        elif op == "snapshot":
            result = all_states(snapshot_number)
            snapshot_number += 1
        else:
            result = {}
        outputs.append({"op": op, "result": result})
    parsed = classification.parse_classification_exchange(kind, commands, outputs, samples)
    return commands, outputs, samples, parsed


def _synthetic_delivery():
    parsed_by_endpoint = {kind: _synthetic_exchange(kind)[3] for kind in ("native", "pi", "dsh")}
    sources = []
    rows = []
    reads = []
    seeds = []
    for kind, evidence in parsed_by_endpoint.items():
        records = []
        seed_samples = []
        for sample in evidence["samples"]:
            key = (kind, sample["address"]["sessionId"])
            if sample["archived"] or (kind == "pi" and sample["role"] == "ephemeral"):
                reads.append({"address": sample["address"], "current": True,
                              "hasActiveRow": False, "hiddenRowNotSupplemented": True})
                seed_samples.append(sample)
                continue
            summary = sample["summaryAfter"] or {}
            facts = {
                field: classification._fact(summary, field)
                for field in ("origin", "parentSessionId", "ephemeral")
            }
            included = summary.get("origin") != "subagent" and summary.get("ephemeral") is not True
            record = {"address": sample["address"], **facts, "included": included}
            records.append(record)
            reads.append({"address": sample["address"], "current": True,
                          "hasActiveRow": included, "hiddenRowNotSupplemented": not included})
            if included:
                rows.append({"address": sample["address"]})
            seed_samples.append(sample)
        if kind == "pi":
            live_address = {"endpointId":"pi","sessionId":"synthetic-host-scratch"}
            records.append({"address":live_address,"included":False,
                "origin":{"state":"missing"},"parentSessionId":{"state":"known","value":"supplied-parent"},
                "ephemeral":{"state":"known","value":True}})
            reads.append({"address":live_address,"current":True,"hasActiveRow":False,"hiddenRowNotSupplemented":True})
        sources.append({"endpointId": kind, "sourceRevision": 9, "connectionEpoch": 2,
                        "observerGeneration": 4, "inventoryReady": True,
                        "archiveFacts": {"workspaceReady": False, "archivedSessionIds": []},
                        "records": records})
        seeds.append({"endpointId": kind, "samples": seed_samples})
    return {"classificationProof": {"inventoryPacket": {
        "source": {"classification": sources, "observer": [
            {"endpointId":kind,"loaded":True,"stale":False,"errorPresent":False,
             "observation":{"phase":"observing"}} for kind in ("native","pi","dsh")]},
        "display": {"rows": rows}},
        "independentSelectedReads": reads,
        "hostOwnedEphemeral":{"address":{"endpointId":"pi","sessionId":"synthetic-host-scratch"},
            "forkReply":{"sessionId":"synthetic-host-scratch"},
            "scope":{"endpointId":"pi","threadId":"synthetic-host-scratch","sessionId":"synthetic-host-scratch"},
            "beforeItems":1,"afterItems":1,"originalItemsPreserved":True,"previousInstanceRetired":True}}}, seeds


class PublicExchangeTests(unittest.TestCase):
    def test_mismatched_handshake_operation_and_count_are_refused(self):
        commands, outputs, _, _ = _synthetic_exchange("pi")
        wrong_kind = copy.deepcopy(outputs)
        wrong_kind[0]["kind"] = "dsh"
        with self.assertRaises(ValueError):
            classification.parse_classification_exchange("pi", commands, wrong_kind, [])

        wrong_op = copy.deepcopy(outputs)
        wrong_op[1]["op"] = "compose"
        with self.assertRaises(ValueError):
            classification.parse_classification_exchange("pi", commands, wrong_op, [])

        with self.assertRaises(ValueError):
            classification.parse_classification_exchange("pi", commands, outputs[:-1], [])

    def test_intermediate_snapshot_and_unarchive_ack_cannot_change_identity(self):
        commands, outputs, samples, _ = _synthetic_exchange("native")
        wrong = copy.deepcopy(outputs)
        snapshots = [row for row in wrong if row.get("op") == "snapshot"]
        snapshots[1]["result"]["endpoint_id"] = "pi"
        with self.assertRaises(ValueError):
            classification.parse_classification_exchange("native", commands, wrong, samples)
        wrong = copy.deepcopy(outputs)
        for command, row in zip(commands, wrong[1:]):
            if command.get("method") == "session.unarchive":
                row["result"]["sessionId"] = "foreign"
        with self.assertRaises(ValueError):
            classification.parse_classification_exchange("native", commands, wrong, samples)

    def test_stale_observer_and_missing_included_display_row_are_refused(self):
        delivery, seeds = _synthetic_delivery()
        delivery["classificationProof"]["inventoryPacket"]["source"]["observer"][0]["stale"] = True
        with self.assertRaises(AssertionError):
            classification.validate_classification_delivery(delivery, seeds)
        delivery, seeds = _synthetic_delivery()
        delivery["classificationProof"]["inventoryPacket"]["display"]["rows"].pop(0)
        with self.assertRaises(AssertionError):
            classification.validate_classification_delivery(delivery, seeds)

    def test_synthetic_baseline_preserves_unknowns_and_never_claims_real_acceptance(self):
        evidence = _synthetic_exchange("pi")[3]
        parent = next(row for row in evidence["samples"] if row["role"] == "parent")
        self.assertFalse(evidence["realUiAcceptance"])
        self.assertEqual(parent["summaryAfter"]["origin"], "future-origin")
        self.assertTrue(parent["history"]["originalItemsPreserved"])
        self.assertTrue(parent["history"]["unarchiveReconnectRestored"])
        dsh_archive = next(sample for sample in _synthetic_exchange("dsh")[3]["samples"]
                           if sample["role"] == "archived-fork")
        self.assertTrue(dsh_archive["archiveAuthority"]["positiveSnapshotFact"])
        self.assertIsNotNone(dsh_archive["summaryAfter"])

        delivery, seeds = _synthetic_delivery()
        result = classification.validate_classification_delivery(delivery, seeds)
        self.assertTrue(result["accepted"])
        self.assertFalse(result["nativeInteraction"])
        self.assertFalse(result["desktopVisibilityProven"])
        self.assertFalse(result["confirmedMainInferred"])

    def test_lost_or_rewritten_retained_history_is_refused(self):
        commands, outputs, samples, _ = _synthetic_exchange("pi")
        archive_wait = next(i for i, command in enumerate(commands)
                            if command["op"] == "waitInventoryMembership" and command.get("present") is False)
        presentation_index = next(i for i in range(archive_wait + 1, len(commands))
                                  if commands[i]["op"] == "presentation")
        lost = copy.deepcopy(outputs)
        lost[presentation_index + 1]["result"]["transcript"]["items"] = []
        with self.assertRaisesRegex(ValueError, "history lost or rewritten"):
            classification.parse_classification_exchange("pi", commands, lost, samples)

        rewritten = copy.deepcopy(outputs)
        rewritten[presentation_index + 1]["result"]["transcript"]["items"][0]["text"] = "rewritten"
        with self.assertRaisesRegex(ValueError, "history lost or rewritten"):
            classification.parse_classification_exchange("pi", commands, rewritten, samples)

    def test_wrong_public_archive_acknowledgement_is_refused(self):
        commands, outputs, samples, _ = _synthetic_exchange("native")
        archive_index = next(i for i, command in enumerate(commands)
                             if command.get("method") == "session.archive")
        mismatched = copy.deepcopy(outputs)
        mismatched[archive_index + 1]["result"]["archivedSessionIds"] = ["different-session"]
        with self.assertRaisesRegex(ValueError, "archive acknowledgements"):
            classification.parse_classification_exchange("native", commands, mismatched, samples)

    def test_delivery_rejects_cross_endpoint_records(self):
        delivery, seeds = _synthetic_delivery()
        source = delivery["classificationProof"]["inventoryPacket"]["source"]["classification"][0]
        source["records"][0]["address"]["endpointId"] = "pi"
        with self.assertRaisesRegex(AssertionError, "classification endpoint escaped"):
            classification.validate_classification_delivery(delivery, seeds)

    def test_delivery_rejects_unknown_values_promoted_to_known(self):
        delivery, seeds = _synthetic_delivery()
        source = next(row for row in delivery["classificationProof"]["inventoryPacket"]["source"]["classification"]
                      if row["endpointId"] == "pi")
        parent = next(row for row in source["records"] if row["address"]["sessionId"] == "supplied-parent")
        self.assertEqual(parent["origin"]["state"], "unknown")
        parent["origin"] = {"state": "known", "value": "future-origin"}
        with self.assertRaisesRegex(AssertionError, "raw classification facts changed"):
            classification.validate_classification_delivery(delivery, seeds)

    def test_hidden_row_resurrection_is_refused(self):
        delivery, seeds = _synthetic_delivery()
        proof = delivery["classificationProof"]
        child = next(sample for sample in next(seed for seed in seeds if seed["endpointId"] == "pi")["samples"]
                     if sample["role"] == "child")
        address = child["address"]
        proof["inventoryPacket"]["display"]["rows"].append({"address": address})
        read = next(row for row in proof["independentSelectedReads"] if row["address"] == address)
        read["hasActiveRow"] = True
        with self.assertRaises(AssertionError):
            classification.validate_classification_delivery(delivery, seeds)


if __name__ == "__main__":
    unittest.main()
