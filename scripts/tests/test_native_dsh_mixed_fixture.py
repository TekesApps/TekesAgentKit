import base64
import copy
import hashlib
import json
from pathlib import Path
import struct
import sys
import unittest
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_dsh_mixed_fixture as fixture


def chunk(kind, payload):
    return (struct.pack(">I", len(payload)) + kind + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xffffffff))


def png(rgb):
    pixels = b"".join(b"\0" + bytes(rgb) * 2 for _ in range(2))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def item(kind, session, role, text, attachments, *, final=False, turn="turn-1"):
    return {
        "id": role + "-" + session,
        "recordId": role + "-record-" + session,
        "reconciliationId": "rpc-" + session if role == "user" else None,
        "kind": role,
        "text": text,
        "attachments": copy.deepcopy(attachments),
        "contentBlocks": [],
        "isFinal": final,
        "isProvisional": False,
        "sourceSequence": 2 if role == "user" else 3,
        "sourcePosition": {"kind": "journal", "sequence": 2 if role == "user" else 3,
                           "contentIndex": None},
        "turnId": turn,
    }


def row_for(kind, index):
    session = kind + "-session-" + str(index)
    names = fixture.IMAGE_NAMES[index]
    data_a = png((235, 36, 24) if index == 0 else (24, 196, 42))
    data_b = png((18, 88, 228) if index == 0 else (212, 42, 174))
    image_docs = [
        {"type": "image", "mediaType": "image/png", "name": names[0],
         "data": base64.b64encode(data_a).decode(), "byteCount": len(data_a), "width": 2, "height": 2},
        {"type": "image", "mediaType": "image/png", "name": names[1],
         "data": base64.b64encode(data_b).decode(), "byteCount": len(data_b), "width": 2, "height": 2},
    ]
    ids = [fixture._image_id(data_a), fixture._image_id(data_b)]
    if kind == "native":
        attachments = [
            {"type": "image", "asset": "sha256-" + ids[0][7:], "mime": "image/png", "name": names[0]},
            {"type": "file", "attachment": {"attachmentId": "sha256:" + hashlib.sha256(fixture.FILE_BYTES).hexdigest(),
             "name": fixture.FILE_NAME, "bytes": len(fixture.FILE_BYTES)}, "mediaType": "text/plain"},
            {"type": "image", "asset": "sha256-" + ids[1][7:], "mime": "image/png", "name": names[1]},
        ]
    else:
        attachments = [
            {"type": "image", "attachment": {"attachmentId": ids[0], "mediaType": "image/png",
             "name": names[0], "bytes": len(data_a), "width": 2, "height": 2}},
            {"type": "file", "attachment": {"attachmentId": "sha256:" + hashlib.sha256(fixture.FILE_BYTES).hexdigest(),
             "name": fixture.FILE_NAME, "bytes": len(fixture.FILE_BYTES)}},
            {"type": "image", "attachment": {"attachmentId": ids[1], "mediaType": "image/png",
             "name": names[1], "bytes": len(data_b), "width": 2, "height": 2}},
        ]
    text = fixture.TEXT_PROMPT if index == 0 else ""
    user = item(kind, session, "user", text, attachments)
    assistant = item(kind, session, "assistant", fixture.EXPECTED_REPLY, [], final=True)
    upload = {"receiptId": "upload-receipt-" + session,
              "file": {"attachmentId": "sha256:" + hashlib.sha256(fixture.FILE_BYTES).hexdigest(),
                       "name": fixture.FILE_NAME, "bytes": len(fixture.FILE_BYTES)}}
    prepared_content = ([{"type": "text", "text": text}] if text else [])
    prepared_content += [{key: image_docs[0][key] for key in ("type", "mediaType", "name", "data")},
                         {"type": "file", "receiptId": upload["receiptId"]},
                         {key: image_docs[1][key] for key in ("type", "mediaType", "name", "data")}]
    reads = []
    for raw, image, data, attachment_id in zip((attachments[0], attachments[2]), image_docs,
                                                (data_a, data_b), ids):
        info = {"attachmentId": attachment_id, "mediaType": "image/png", "bytes": len(data),
                "width": 2, "height": 2}
        if kind == "dsh":
            info["name"] = image["name"]
        reads.append({"rawTranscriptAttachment": raw, "attachmentId": attachment_id,
                      "read": {"attachment": info, "data": image["data"]}})
    if kind == "native":
        file_read = {"supported": True, "read": {"attachment": {
            "attachmentId": upload["file"]["attachmentId"], "name": fixture.FILE_NAME,
            "bytes": len(fixture.FILE_BYTES), "mediaType": "text/plain"},
            "data": base64.b64encode(fixture.FILE_BYTES).decode()}}
    else:
        file_read = {"supported": False, "read": None}
    row = {"scope": {"endpointId": kind, "sessionId": session, "threadId": session},
           "rpcId": "rpc-" + session, "authoredText": text,
           "prepared": {"content": prepared_content, "displayText": text},
           "uploadReceipt": upload, "imageInputs": image_docs, "input": user,
           "assistant": assistant, "imageReads": reads, "fileRead": file_read}
    return row, (data_a, data_b)


def receipt_for(kind):
    rows = [row_for(kind, index)[0] for index in range(2)]
    recovered = [dict(copy.deepcopy(row), epochBefore=4, epochAfter=6) for row in rows]
    return {"status": "completed", "endpointKind": kind, "roundTrips": rows,
            "reconnect": {"epochBefore": 4, "epochAfter": 6, "recovered": recovered}}


def native_provider_body(row, image_data):
    content = []
    if row["authoredText"]:
        content.append({"type": "input_text", "text": row["authoredText"]})
    content += [
        {"type": "input_image", "image_url": "data:image/png;base64," + row["imageInputs"][0]["data"]},
        {"type": "input_file", "filename": fixture.FILE_NAME,
         "file_data": "data:text/plain;base64," + base64.b64encode(fixture.FILE_BYTES).decode()},
        {"type": "input_image", "image_url": "data:image/png;base64," + row["imageInputs"][1]["data"]},
    ]
    return {"input": [{"role": "user", "content": content}]}


class NativeDshMixedFixtureTests(unittest.TestCase):
    def test_selected_chinese_file_is_the_frozen_29_byte_fixture(self):
        self.assertEqual(len(fixture.FILE_BYTES), 29)
        self.assertEqual(fixture.FILE_NAME, "中文混合.txt")

    def test_accepts_native_and_dsh_receipts_with_original_reconnect_evidence(self):
        for kind in ("native", "dsh"):
            rows = receipt_for(kind)
            result = fixture.validate_probe_receipt(rows, kind,
                [row["scope"]["sessionId"] for row in rows["roundTrips"]])
            self.assertEqual(len(result["images"]), 2)

    def test_rejects_wrong_order_metadata_bytes_and_unread_file_capability(self):
        value = receipt_for("native")
        value["roundTrips"][0]["prepared"]["content"].reverse()
        with self.assertRaisesRegex(ValueError, "content order"):
            fixture.validate_probe_receipt(value, "native", ["native-session-0", "native-session-1"])
        value = receipt_for("dsh")
        value["roundTrips"][1]["fileRead"] = {"supported": True, "read": {}}
        with self.assertRaisesRegex(ValueError, "no generic file download"):
            fixture.validate_probe_receipt(value, "dsh", ["dsh-session-0", "dsh-session-1"])
        value = receipt_for("native")
        value["reconnect"]["recovered"][0]["input"]["sourceSequence"] += 1
        with self.assertRaisesRegex(ValueError, "complete public transcript item"):
            fixture.validate_probe_receipt(value, "native", ["native-session-0", "native-session-1"])

    def test_native_provider_requires_exact_responses_file_bytes_and_image_order(self):
        receipt = receipt_for("native")
        for index, row in enumerate(receipt["roundTrips"]):
            _, image_bytes = fixture.validate_probe_receipt(
                receipt, "native", ["native-session-0", "native-session-1"])["images"][index]
            observed = fixture.inspect_provider_request("native", native_provider_body(row, image_bytes),
                expected_text=row["authoredText"], images=row["imageInputs"], image_bytes=image_bytes,
                upload_receipt=row["uploadReceipt"], input_item=row["input"], dsh_home="/tmp/owned-dsh")
            self.assertEqual([image["byteCount"] for image in observed["images"]],
                             [len(data) for data in image_bytes])
        bad = native_provider_body(receipt["roundTrips"][0], (b"bad", b"bad"))
        with self.assertRaisesRegex(ValueError, "exact authored text"):
            fixture.inspect_provider_request("native", bad, expected_text=fixture.TEXT_PROMPT,
                images=receipt["roundTrips"][0]["imageInputs"], image_bytes=(b"bad", b"bad"),
                upload_receipt=receipt["roundTrips"][0]["uploadReceipt"],
                input_item=receipt["roundTrips"][0]["input"], dsh_home="/tmp/owned-dsh")

    def test_dsh_provider_requires_exact_handles_images_and_only_published_context(self):
        with __import__("tempfile").TemporaryDirectory() as home:
            receipt = receipt_for("dsh")
            bodies = []
            for index, row in enumerate(receipt["roundTrips"]):
                image_bytes = row_for("dsh", index)[1]
                attachments = row["input"]["attachments"]
                blocks = []
                if row["authoredText"]:
                    blocks.append({"type": "text", "text": row["authoredText"]})
                for image_index, image_position in enumerate((0, 2)):
                    blocks.append({"type": "text", "text": fixture._expected_image_handle(
                        row["imageInputs"][image_index], attachments[image_position]["attachment"], home)})
                    blocks.append({"type": "image_url", "image_url": {
                        "url": "data:image/png;base64," + row["imageInputs"][image_index]["data"]}})
                    if image_index == 0:
                        blocks.append({"type": "text", "text": fixture._expected_file_handle(
                            row["uploadReceipt"]["file"]["attachmentId"], home)})
                body = {"messages": [
                    {"role": "user", "content": blocks},
                    {"role": "user", "content": fixture.RUNTIME_CONTEXT_PREFIX + "snapshot"},
                    {"role": "user", "content": "Time sampled while preparing turn 1, step 2: 2026-10-25T10:00:00+08:00[Asia/Shanghai]\nBrowser time zone for this request: unavailable. Ask the user to clarify otherwise-unqualified dates and times.\nElapsed since the preceding step context: 0s."},
                ]}
                bodies.append(copy.deepcopy(body))
                self.assertEqual(fixture.classify_dsh_request(body), ("prompt", None))
                facts = fixture.inspect_provider_request("dsh", body, expected_text=row["authoredText"],
                    images=row["imageInputs"], image_bytes=image_bytes,
                    upload_receipt=row["uploadReceipt"], input_item=row["input"], dsh_home=home)
                self.assertEqual(facts["hostContextSuffixes"], ["runtime-context", "time-context"])
                bad_image = json.loads(json.dumps(body))
                next(block for block in bad_image["messages"][0]["content"]
                     if block["type"] == "image_url")["type"] = "image"
                with self.assertRaisesRegex(ValueError, "OpenAI image_url block"):
                    fixture.inspect_provider_request("dsh", bad_image, expected_text=row["authoredText"],
                        images=row["imageInputs"], image_bytes=image_bytes,
                        upload_receipt=row["uploadReceipt"], input_item=row["input"], dsh_home=home)
                if index == 1:
                    body["messages"].append({"role": "user", "content": "unexpected human input"})
                    with self.assertRaisesRegex(ValueError, "unknown, duplicate, or missing human input"):
                        fixture.inspect_provider_request("dsh", body, expected_text=row["authoredText"],
                            images=row["imageInputs"], image_bytes=image_bytes,
                            upload_receipt=row["uploadReceipt"], input_item=row["input"], dsh_home=home)

            # Run the complete observation/receipt validation pipeline; only OS decoding
            # is omitted here because the owned live gate performs real ImageIO reads.
            import importlib.util
            from unittest.mock import patch
            spec = importlib.util.spec_from_file_location("mixed_runner", Path(__file__).parents[1] / "test-real-native-dsh-mixed-files.py")
            runner = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(runner)
            provider = runner.MixedProvider("dsh")
            for body in bodies:
                provider.observe("/dsh/v1/chat/completions", body)
            with patch.object(runner, "record_imageio_evidence", side_effect=lambda observations, *_: observations):
                result = provider.validate(receipt, [entry["scope"]["sessionId"] for entry in receipt["roundTrips"]], home, Path(home) / "images")
            self.assertEqual(result["attemptCount"], 2)
            self.assertEqual([row["authoredText"] for row in result["requests"]], [fixture.TEXT_PROMPT, ""])
            self.assertEqual(result["retryCount"], 0)

    def test_rejects_durable_native_file_in_place_of_public_projection(self):
        receipt = receipt_for("native")
        receipt["roundTrips"][0]["input"]["attachments"][1] = {
            "type": "file", "asset": "sha256-" + hashlib.sha256(fixture.FILE_BYTES).hexdigest(),
            "mime": "text/plain", "name": fixture.FILE_NAME, "bytes": len(fixture.FILE_BYTES)}
        with self.assertRaisesRegex(ValueError, "project_content_blocks DTO"):
            fixture.validate_probe_receipt(receipt, "native", [entry["scope"]["sessionId"] for entry in receipt["roundTrips"]])

    def test_dsh_title_is_accepted_only_in_published_one_message_json_shape(self):
        body = {"messages": [
            {"role": "system", "content": fixture.TITLE_SYSTEM_PREFIX + "\nReturn a short title."},
            {"role": "user", "content": fixture.TITLE_PREFIX + json.dumps(
                [{"seq": 3, "text": fixture.TEXT_PROMPT}], ensure_ascii=False)},
        ]}
        self.assertEqual(fixture.classify_dsh_request(body), ("title", fixture.TEXT_PROMPT))
        duplicated = copy.deepcopy(body)
        duplicated["messages"].insert(1, {"role": "user", "content": "unexpected human"})
        with self.assertRaisesRegex(ValueError, "unknown or repeated human"):
            fixture.classify_dsh_request(duplicated)
        body["messages"][1]["content"] = fixture.TITLE_PREFIX + "not JSON"
        with self.assertRaisesRegex(ValueError, "malformed published JSON"):
            fixture.classify_dsh_request(body)


if __name__ == "__main__":
    unittest.main()
