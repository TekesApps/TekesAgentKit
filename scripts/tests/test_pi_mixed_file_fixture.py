import base64
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pi_mixed_file_fixture import (
    FILE_BYTES,
    FILE_NAME,
    QUEUED_TEXT,
    READY_PREFIX,
    has_transcript_file_prompt,
    inspect_pi_user_message,
    validate_file_read,
    validate_queue_ready_lines,
)


PNG_A = b"\x89PNG\r\n\x1a\nfixture-a"
PNG_B = b"\x89PNG\r\n\x1a\nfixture-b"


def image(name, data):
    return {"type": "image", "mediaType": "image/png", "name": name,
            "data": base64.b64encode(data).decode("ascii")}


def ready_event():
    rpc = "pi-mixed-ticket-123"
    file_receipt = "pi-mixed-file-456"
    fact = {
        "id": "queue-item-789",
        "rpcId": rpc,
        "placement": "queued",
        "message": {"role": "user", "content": [
            {"type": "text", "text": QUEUED_TEXT},
            image("排队第一张.png", PNG_A),
            {"type": "file", "receiptId": file_receipt},
            image("排队第二张.png", PNG_B),
        ]},
    }
    fact["payload"] = {"sessionId": "session-b", "mode": "queue",
                        "content": json.loads(json.dumps(fact["message"]["content"]))}
    return {
        "status": "queued",
        "scope": {"endpointId": "pi", "threadId": "session-b", "sessionId": "session-b"},
        "ticketId": rpc,
        "rpcId": rpc,
        "mode": "queue",
        "snapshotRevision": 31,
        "connectionEpoch": 7,
        "queueItemId": fact["id"],
        "queueFact": fact,
    }


def lines_for(value):
    return [READY_PREFIX + json.dumps(value)]


def sync_payload(value):
    content = value["queueFact"]["message"]["content"]
    value["queueFact"]["payload"]["content"] = json.loads(json.dumps(content))


class PiMixedFileFixtureTests(unittest.TestCase):
    def validate(self, value=None, **kwargs):
        return validate_queue_ready_lines(
            lines_for(ready_event() if value is None else value),
            expected_session_id="session-b", gate_entered=True, **kwargs,
        )

    def test_accepts_public_queued_fact_with_scope_rpc_and_mixed_order(self):
        result = self.validate()
        self.assertEqual(result["rpcId"], "pi-mixed-ticket-123")
        self.assertEqual(result["fileReceiptId"], "pi-mixed-file-456")
        self.assertEqual([image["bytes"] for image in result["images"]],
                         [len(PNG_A), len(PNG_B)])
        self.assertEqual(result["gateEntered"], True)

    def test_mode_queue_without_snapshot_queue_fact_is_rejected(self):
        value = ready_event()
        value.pop("queueFact")
        with self.assertRaisesRegex(ValueError, "RunSnapshot queue fact"):
            self.validate(value)

    def test_queue_ready_is_rejected_until_real_provider_hold_gate_entered(self):
        with self.assertRaisesRegex(ValueError, "gate was not entered"):
            validate_queue_ready_lines(lines_for(ready_event()),
                                       expected_session_id="session-b", gate_entered=False)

    def test_wrong_scope_and_rpc_identity_are_rejected(self):
        value = ready_event()
        value["scope"]["sessionId"] = "other-session"
        with self.assertRaisesRegex(ValueError, "scope differs"):
            self.validate(value)
        value = ready_event()
        value["queueFact"]["rpcId"] = "another-ticket"
        with self.assertRaisesRegex(ValueError, "RPC or placement"):
            self.validate(value)

    def test_wrong_queue_payload_image_file_order_and_receipt_are_rejected(self):
        value = ready_event()
        value["queueFact"]["message"]["content"][2]["receiptId"] = "other-file"
        sync_payload(value)
        with self.assertRaisesRegex(ValueError, "file receipt differs"):
            self.validate(value, expected_file_receipt="pi-mixed-file-456")
        value = ready_event()
        value["queueFact"]["message"]["content"][3] = dict(
            value["queueFact"]["message"]["content"][1])
        sync_payload(value)
        with self.assertRaisesRegex(ValueError, "must be distinct"):
            self.validate(value)
        value = ready_event()
        value["queueFact"]["payload"]["content"][0]["text"] = "wrong payload"
        with self.assertRaisesRegex(ValueError, "payload does not match"):
            self.validate(value)

    def test_duplicate_or_malformed_ready_events_fail_closed(self):
        line = lines_for(ready_event())[0]
        with self.assertRaisesRegex(ValueError, "exactly one"):
            validate_queue_ready_lines([line, line], expected_session_id="session-b",
                                       gate_entered=True)
        with self.assertRaisesRegex(ValueError, "complete stdout line"):
            validate_queue_ready_lines(["prefix " + line], expected_session_id="session-b",
                                       gate_entered=True)

    def test_missing_revision_wrong_image_mime_and_bad_base64_are_rejected(self):
        value = ready_event()
        value["snapshotRevision"] = True
        with self.assertRaisesRegex(ValueError, "snapshot revision"):
            self.validate(value)
        value = ready_event()
        value["queueFact"]["message"]["content"][1]["mediaType"] = "image/jpeg"
        sync_payload(value)
        with self.assertRaisesRegex(ValueError, "image/png"):
            self.validate(value)
        value = ready_event()
        value["queueFact"]["message"]["content"][3]["data"] = "%%%"
        sync_payload(value)
        with self.assertRaisesRegex(ValueError, "malformed base64"):
            self.validate(value)

    def test_public_file_read_must_match_receipt_and_fixture_bytes(self):
        receipt = {"receiptId": "upload-receipt-id", "file": {"attachmentId": "attachment-id",
                   "name": FILE_NAME, "bytes": len(FILE_BYTES)}}
        read = {"attachment": {"attachmentId": "attachment-id", "name": FILE_NAME,
                               "mediaType": "text/plain; charset=utf-8",
                               "bytes": len(FILE_BYTES)},
                "data": base64.b64encode(FILE_BYTES).decode("ascii")}
        validate_file_read(read, receipt)
        read["data"] = base64.b64encode(b"wrong").decode("ascii")
        with self.assertRaisesRegex(ValueError, "differs"):
            validate_file_read(read, receipt)

    def test_transcript_text_keeps_marker_first_and_json_quoted_file_name(self):
        actual = 'PI_MIXED_DIRECT\n\nAttached file: "中文混合.txt" (path: "/owned/upload")'
        self.assertTrue(has_transcript_file_prompt(actual, "PI_MIXED_DIRECT"))
        self.assertFalse(has_transcript_file_prompt(
            'Attached file: "中文混合.txt" (path: "/owned/upload")\n\nPI_MIXED_DIRECT',
            "PI_MIXED_DIRECT"))

    def test_provider_inspects_only_latest_user_and_keeps_file_as_quoted_text(self):
        older = {"role": "user", "content": "PI_MIXED_DIRECT"}
        latest = {"role": "user", "content": [
            {"type": "text", "text": 'PI_MIXED_DIRECT\n\nAttached file: "中文混合.txt" (path: "/owned/upload")'},
            {"type": "image_url", "image_url": {"url":
                "data:image/png;base64," + base64.b64encode(PNG_A).decode("ascii")}},
            {"type": "image_url", "image_url": {"url":
                "data:image/png;base64," + base64.b64encode(PNG_B).decode("ascii")}},
        ]}
        result = inspect_pi_user_message({"messages": [older, latest]}, "PI_MIXED_DIRECT")
        self.assertEqual([item["sha256"] for item in result["images"]], [
            hashlib.sha256(PNG_A).hexdigest(),
            hashlib.sha256(PNG_B).hexdigest(),
        ])
        with self.assertRaisesRegex(ValueError, "latest user marker"):
            inspect_pi_user_message({"messages": [older, {"role": "user", "content": "unrelated"}]},
                                    "PI_MIXED_DIRECT")
        latest["content"].append({"type": "file", "file": {"name": FILE_NAME}})
        with self.assertRaisesRegex(ValueError, "provider file blocks"):
            inspect_pi_user_message({"messages": [latest]}, "PI_MIXED_DIRECT")


    def test_native_source_hash_images_and_receipt_provenance_cannot_be_substituted(self):
        import importlib.util
        import copy
        path = Path(__file__).resolve().parents[1] / "test-real-pi-mixed-files.py"
        spec = importlib.util.spec_from_file_location("mixed_runner_source_test", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        text = 'PI_MIXED_DIRECT\n\nAttached file: "中文混合.txt" (path: "/owned/upload")'
        images = [image("a.png", PNG_A), image("b.png", PNG_B)]
        files = [{"contentIndex": 2, "precedingImageCount": 1,
                  "file": {"attachmentId": "file-id", "name": FILE_NAME,
                           "bytes": len(FILE_BYTES)}}]
        source = {"nativeReceipt": {"id": "receipt-entry", "type": "custom",
                  "customType": "agentkit.inputReceipt.v1", "data": {
                  "timestamp": 123, "rpcId": "ticket", "files": copy.deepcopy(files),
                  "hash": hashlib.sha256(text.encode()).hexdigest()}},
                  "nativeMessage": {"id": "user-entry", "type": "message", "message": {
                  "timestamp": 123, "role": "user", "content": [
                  {"type": "text", "text": text}, *images]}}}
        item = {"recordId": "user-entry", "text": text, "attachments": images,
                "metadata": {"admittedFileAttachments": files,
                "admittedFileReceipt": {"entryId": "receipt-entry",
                "customType": "agentkit.inputReceipt.v1"}}}
        module.validate_native_source(source, "ticket", item)
        for field in ("hash", "timestamp", "files", "images", "provenance"):
            bad_source, bad_item = copy.deepcopy(source), copy.deepcopy(item)
            if field == "hash":
                bad_source["nativeReceipt"]["data"]["hash"] = "0" * 64
            elif field == "timestamp":
                bad_source["nativeReceipt"]["data"]["timestamp"] += 1
            elif field == "files":
                bad_source["nativeReceipt"]["data"]["files"][0]["file"]["bytes"] += 1
            elif field == "images":
                bad_item["attachments"].reverse()
            else:
                bad_item["metadata"]["admittedFileReceipt"]["entryId"] = "other-receipt"
            with self.subTest(field=field), self.assertRaises(ValueError):
                module.validate_native_source(bad_source, "ticket", bad_item)



if __name__ == "__main__":
    unittest.main()
