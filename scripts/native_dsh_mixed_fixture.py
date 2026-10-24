"""Strict public-receipt and provider checks for the Native/DSH mixed probe."""

from __future__ import annotations

import base64
import binascii
import hashlib
import json
from pathlib import Path
import re
import struct

from inline_image_live_fixture import decode_image_with_imageio


EXPECTED_REPLY = "ENDPOINT_REPLY_OK"
TEXT_PROMPT = "NATIVE_DSH_MIXED"
FILE_NAME = "中文混合.txt"
FILE_BYTES = b"AgentKit Pi mixed file proof\n"
IMAGE_NAMES = (
    ("第一张.png", "第二张.png"),
    ("空正文第一张.png", "空正文第二张.png"),
)
RUNTIME_CONTEXT_PREFIX = (
    "Current runtime context. This snapshot supersedes earlier runtime-context snapshots.\n\n"
)
TITLE_PREFIX = "Generate the session title from this JSON array of human messages:\n"
TITLE_SYSTEM_PREFIX = (
    "Create a concise title for an AI coding-assistant session from the supplied human messages."
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _object(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        _fail(label + " must be an object")
    return value


def _array(value: object, label: str) -> list:
    if not isinstance(value, list):
        _fail(label + " must be an array")
    return value


def _nonempty_string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        _fail(label + " must be a non-empty string")
    return value


def _base64(value: object, label: str) -> bytes:
    if not isinstance(value, str) or not value:
        _fail(label + " is missing base64 data")
    try:
        return base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError):
        _fail(label + " has malformed base64")


def _png_dimensions(data: bytes, label: str) -> tuple[int, int]:
    if len(data) < 24 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        _fail(label + " is not a PNG with an IHDR")
    width, height = struct.unpack(">II", data[16:24])
    if width != 2 or height != 2:
        _fail(label + " must be a 2x2 PNG")
    return width, height


def _image_input(value: object, index: int) -> tuple[dict, bytes]:
    item = _object(value, "imageInputs[" + str(index) + "]")
    if set(item) != {"type", "mediaType", "name", "data", "byteCount", "width", "height"}:
        _fail("imageInputs entry has an unexpected schema")
    if item.get("type") != "image" or item.get("mediaType") != "image/png":
        _fail("imageInputs must be PNG image blocks")
    data = _base64(item.get("data"), "imageInputs data")
    width, height = _png_dimensions(data, "imageInputs data")
    if item.get("byteCount") != len(data) or item.get("width") != width or item.get("height") != height:
        _fail("imageInputs byte count or dimensions differ from canonical PNG data")
    _nonempty_string(item.get("name"), "imageInputs name")
    return item, data


def _image_id(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def _same_identity(original: dict, recovered: dict, label: str) -> None:
    if original != recovered:
        _fail(label + " changed its complete public transcript item")


def _has_source_position(item: dict) -> bool:
    position = item.get("sourcePosition")
    sequence = item.get("sourceSequence")
    return isinstance(position, dict) or type(sequence) is int


def _validate_item_pair(kind: str, row: dict, expected_session: str, index: int) -> tuple[dict, dict]:
    scope = _object(row.get("scope"), "roundTrip scope")
    if (scope.get("endpointId") != kind or scope.get("sessionId") != expected_session
            or scope.get("threadId") != expected_session):
        _fail("roundTrip scope differs from the selected public session")
    rpc_id = _nonempty_string(row.get("rpcId"), "roundTrip rpcId")
    authored_text = TEXT_PROMPT if index == 0 else ""
    if row.get("authoredText") != authored_text:
        _fail("roundTrip authoredText differs from its scenario")
    user = _object(row.get("input"), "roundTrip input")
    assistant = _object(row.get("assistant"), "roundTrip assistant")
    if (user.get("kind") != "user" or user.get("text") != authored_text
            or user.get("isProvisional") is not False or user.get("isFinal") is not False
            or not _nonempty_string(user.get("id"), "input id")
            or not _nonempty_string(user.get("recordId"), "input recordId")
            or not _nonempty_string(user.get("turnId"), "input turnId")
            or user.get("reconciliationId") != rpc_id or not _has_source_position(user)):
        _fail("public user input is not the original sourced non-provisional RPC")
    if (assistant.get("kind") != "assistant" or assistant.get("text") != EXPECTED_REPLY
            or assistant.get("isFinal") is not True or assistant.get("isProvisional") is not False
            or assistant.get("turnId") != user.get("turnId")
            or not _nonempty_string(assistant.get("id"), "assistant id")
            or not _nonempty_string(assistant.get("recordId"), "assistant recordId")
            or not _has_source_position(assistant)):
        _fail("final assistant is not sourced to the original user turn")
    return user, assistant


def _validate_history(kind: str, user: dict, upload: dict, images: list[dict], image_bytes: list[bytes]) -> None:
    attachments = _array(user.get("attachments"), "input attachments")
    if len(attachments) != 3:
        _fail("public input attachments must retain image/file/image order")
    file_item = _object(attachments[1], "public file attachment")
    if file_item.get("type") != "file":
        _fail("public file attachment is not at index 1")
    file_ref = _object(upload.get("file"), "uploadReceipt.file")
    digest = hashlib.sha256(FILE_BYTES).hexdigest()
    if (file_ref.get("attachmentId") != "sha256:" + digest
            or file_ref.get("name") != FILE_NAME or file_ref.get("bytes") != len(FILE_BYTES)):
        _fail("public upload receipt differs from the exact 29-byte selected file")
    if kind == "native":
        if file_item != {"type": "file", "attachment": {
                "attachmentId": "sha256:" + digest, "name": FILE_NAME, "bytes": len(FILE_BYTES)},
                "mediaType": "text/plain"}:
            _fail("Native file history differs from the published project_content_blocks DTO")
    else:
        if file_item != {"type": "file", "attachment": {
                "attachmentId": "sha256:" + digest, "name": FILE_NAME, "bytes": len(FILE_BYTES)}}:
            _fail("DSH file history differs from the published descriptor format")
    for index, (raw_value, image, data) in enumerate(zip((attachments[0], attachments[2]), images, image_bytes)):
        raw = _object(raw_value, "public image attachment")
        if raw.get("type") != "image":
            _fail("public image attachment order differs")
        expected_id = _image_id(data)
        if kind == "native":
            if raw != {"type": "image", "asset": "sha256-" + expected_id[7:],
                       "mime": "image/png", "name": image["name"]}:
                _fail("Native image history differs from its exact public asset format")
        else:
            descriptor = _object(raw.get("attachment"), "DSH image descriptor")
            if (descriptor.get("attachmentId") != expected_id
                    or descriptor.get("mediaType") != "image/png"
                    or descriptor.get("name") != image["name"]
                    or descriptor.get("bytes") != len(data)
                    or descriptor.get("width") != 2 or descriptor.get("height") != 2
                    or set(raw) != {"type", "attachment"}):
                _fail("DSH image history differs from its public descriptor")


def _validate_image_reads(kind: str, row: dict, user: dict, images: list[dict], image_bytes: list[bytes]) -> None:
    reads = _array(row.get("imageReads"), "imageReads")
    attachments = user["attachments"]
    if len(reads) != 2:
        _fail("each original image must have one public session.attachment read")
    for index, evidence_value in enumerate(reads):
        evidence = _object(evidence_value, "imageReads entry")
        raw = attachments[index * 2]
        expected_id = _image_id(image_bytes[index])
        read = _object(evidence.get("read"), "public image read")
        descriptor = _object(read.get("attachment"), "public image read metadata")
        if (evidence.get("rawTranscriptAttachment") != raw
                or evidence.get("attachmentId") != expected_id
                or read.get("data") != images[index]["data"]
                or descriptor.get("attachmentId") != expected_id
                or descriptor.get("mediaType") != "image/png"
                or descriptor.get("bytes") != len(image_bytes[index])
                or descriptor.get("width") != 2 or descriptor.get("height") != 2
                or (kind == "dsh" and descriptor.get("name") != images[index]["name"])):
            _fail("public image read does not match original source, bytes, or metadata")


def _validate_file_read(kind: str, row: dict) -> None:
    value = _object(row.get("fileRead"), "fileRead")
    if kind == "dsh":
        if value != {"supported": False, "read": None}:
            _fail("DSH has no generic file download; fileRead must be explicitly unsupported")
        return
    if value.get("supported") is not True:
        _fail("Native public session.fileAttachment read is required")
    read = _object(value.get("read"), "Native fileAttachment read")
    info = _object(read.get("attachment"), "Native file read metadata")
    upload = row["uploadReceipt"]
    if (info.get("attachmentId") != upload["file"]["attachmentId"]
            or info.get("name") != FILE_NAME or info.get("bytes") != len(FILE_BYTES)
            or info.get("mediaType") != "text/plain"
            or read.get("data") != base64.b64encode(FILE_BYTES).decode("ascii")):
        _fail("Native public file read differs from the original uploaded bytes")


def validate_probe_receipt(receipt: object, kind: str, session_ids: list[str]) -> dict:
    """Validate the frozen public probe receipt and recovered source identities."""
    value = _object(receipt, "probe receipt")
    if value.get("status") != "completed" or value.get("endpointKind") != kind:
        _fail("probe receipt is not completed for the selected endpoint")
    rows = _array(value.get("roundTrips"), "roundTrips")
    if len(rows) != 2 or len(session_ids) != 2 or session_ids[0] == session_ids[1]:
        _fail("probe receipt must contain exactly two distinct public sessions")
    reconnect = _object(value.get("reconnect"), "reconnect")
    before, after = reconnect.get("epochBefore"), reconnect.get("epochAfter")
    if type(before) is not int or type(after) is not int or after <= before:
        _fail("runtime reconnect did not advance the public connection epoch")
    recovered = _array(reconnect.get("recovered"), "reconnect.recovered")
    if len(recovered) != 2:
        _fail("reconnect must recover both original round trips")
    validated_images = []
    rpc_ids = set()
    receipt_ids = set()
    for index, row_value in enumerate(rows):
        row = _object(row_value, "roundTrips entry")
        if set(row) != {"scope", "rpcId", "authoredText", "prepared", "uploadReceipt",
                        "imageInputs", "input", "assistant", "imageReads", "fileRead"}:
            _fail("roundTrip differs from the frozen receipt schema")
        rpc_id = _nonempty_string(row.get("rpcId"), "roundTrip rpcId")
        if rpc_id in rpc_ids:
            _fail("two public sessions reused a RunBinding RPC ID")
        rpc_ids.add(rpc_id)
        images = [_image_input(item, i) for i, item in enumerate(_array(row.get("imageInputs"), "imageInputs"))]
        if len(images) != 2:
            _fail("each round trip requires exactly two imageInputs")
        image_docs = [item for item, _ in images]
        image_bytes = [data for _, data in images]
        if image_bytes[0] == image_bytes[1] or _image_id(image_bytes[0]) == _image_id(image_bytes[1]):
            _fail("two source PNG images must be distinct")
        if [image["name"] for image in image_docs] != list(IMAGE_NAMES[index]):
            _fail("image names differ from the frozen scenario")
        if index == 0 and row.get("authoredText") != TEXT_PROMPT:
            _fail("first round trip must carry NATIVE_DSH_MIXED")
        if index == 1 and row.get("authoredText") != "":
            _fail("second round trip must have no authored text")
        upload = _object(row.get("uploadReceipt"), "uploadReceipt")
        if set(upload) != {"receiptId", "file"}:
            _fail("uploadReceipt must preserve its original nested public shape")
        _nonempty_string(upload.get("receiptId"), "uploadReceipt.receiptId")
        if upload["receiptId"] in receipt_ids:
            _fail("two independently created sessions reused a file upload receipt ID")
        receipt_ids.add(upload["receiptId"])
        file_receipt = _object(upload.get("file"), "uploadReceipt.file")
        if set(file_receipt) != {"attachmentId", "name", "bytes"}:
            _fail("uploadReceipt.file differs from the public file DTO")
        file_id = _nonempty_string(file_receipt.get("attachmentId"), "file attachmentId")
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", file_id):
            _fail("uploaded file attachment ID is not the public SHA-256 identity")
        prepared = _object(row.get("prepared"), "prepared")
        if set(prepared) != {"content", "displayText"} or prepared.get("displayText") != row["authoredText"]:
            _fail("prepared submission lost its exact displayText")
        content = _array(prepared.get("content"), "prepared.content")
        expected_types = ["text", "image", "file", "image"] if index == 0 else ["image", "file", "image"]
        if [part.get("type") if isinstance(part, dict) else None for part in content] != expected_types:
            _fail("prepared canonical content order differs from the scenario")
        offset = 1 if index == 0 else 0
        if index == 0 and content[0] != {"type": "text", "text": TEXT_PROMPT}:
            _fail("prepared canonical text block differs")
        if (content[offset] != {key: image_docs[0][key] for key in ("type", "mediaType", "name", "data")}
                or content[offset + 1] != {"type": "file", "receiptId": upload["receiptId"]}
                or content[offset + 2] != {key: image_docs[1][key] for key in ("type", "mediaType", "name", "data")}):
            _fail("prepared content did not preserve exact image bytes, file receipt, and order")
        user, assistant = _validate_item_pair(kind, row, session_ids[index], index)
        if user.get("attachments") != row["input"].get("attachments"):
            _fail("public input attachment list is missing")
        _validate_history(kind, user, upload, image_docs, image_bytes)
        _validate_image_reads(kind, row, user, image_docs, image_bytes)
        _validate_file_read(kind, row)
        recovered_row = _object(recovered[index], "recovered roundTrip")
        if set(recovered_row) != set(row) | {"epochBefore", "epochAfter"}:
            _fail("recovered round trip does not preserve the original schema and reconnect epochs")
        if recovered_row.get("epochBefore") != before or recovered_row.get("epochAfter") != after:
            _fail("recovered round trip epoch differs from runtime reconnect")
        if (recovered_row.get("scope") != row.get("scope")
                or recovered_row.get("rpcId") != row.get("rpcId")
                or recovered_row.get("authoredText") != row.get("authoredText")
                or recovered_row.get("prepared") != row.get("prepared")
                or recovered_row.get("uploadReceipt") != row.get("uploadReceipt")
                or recovered_row.get("imageInputs") != row.get("imageInputs")):
            _fail("reconnect changed original submission, upload, or canonical image evidence")
        recovered_user, recovered_assistant = _validate_item_pair(kind, recovered_row, session_ids[index], index)
        _same_identity(user, recovered_user, "reconnected original user input")
        _same_identity(assistant, recovered_assistant, "reconnected final assistant")
        _validate_history(kind, recovered_user, upload, image_docs, image_bytes)
        _validate_image_reads(kind, recovered_row, recovered_user, image_docs, image_bytes)
        _validate_file_read(kind, recovered_row)
        validated_images.append((image_docs, image_bytes))
    return {"receipt": value, "images": validated_images, "epochBefore": before, "epochAfter": after}


def _data_uri(value: object, label: str) -> tuple[str, bytes]:
    if not isinstance(value, str) or not value.startswith("data:") or ";base64," not in value:
        _fail(label + " must be a base64 data URI")
    mime, encoded = value[5:].split(";base64,", 1)
    if not mime:
        _fail(label + " has no MIME")
    return mime, _base64(encoded, label)


def _expected_file_handle(file_id: str, dsh_home: str) -> str:
    digest = file_id[7:]
    path = str(Path(dsh_home) / "attachments" / "v1" / "files" / digest[:2] / digest / FILE_NAME)
    qname = json.dumps(FILE_NAME, ensure_ascii=False, separators=(",", ":"))
    qpath = json.dumps(path, ensure_ascii=False, separators=(",", ":"))
    short = digest[:8]
    return (f"[File {qname} ({len(FILE_BYTES)} bytes, sha256:{short}): verbatim read-only copy saved at {qpath}. "
            "Read that path with your file tools when its contents are needed; copy it to a writable location before modifying it. "
            "When delegating file work, include this saved path in the delegation prompt; only subagents sharing this execution environment can read it.]")


def _expected_image_handle(image: dict, attachment: dict, dsh_home: str) -> str:
    attachment_id = _nonempty_string(attachment.get("attachmentId"), "DSH image attachmentId")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", attachment_id):
        _fail("DSH image descriptor has an invalid public attachment ID")
    digest = attachment_id[7:]
    path = str(Path(dsh_home) / "attachments" / "v1" / "objects" / digest[:2] / digest)
    name = json.dumps(image["name"], ensure_ascii=False, separators=(",", ":"))
    quoted_path = json.dumps(path, ensure_ascii=False, separators=(",", ":"))
    return (f"Image {name} ({attachment_id}); request preview 2x2px. "
            f"Normalized copy (read-only; may be resized or re-encoded): {quoted_path} (2x2px, image/png). "
            "Source dimensions, format, and byte size may differ. Copy to a writable path ending in .png before editing.")


def _user_messages(body: dict, field: str) -> list[dict]:
    history = _array(body.get(field), "provider " + field)
    return [_object(item, "provider history item") for item in history
            if isinstance(item, dict) and item.get("role") == "user"]


def _is_title_request(body: dict) -> tuple[bool, str | None]:
    messages = _array(body.get("messages"), "DSH messages")
    system = [item.get("content") for item in messages
              if isinstance(item, dict) and item.get("role") == "system"]
    user = _user_messages(body, "messages")
    if not user or not isinstance(user[-1].get("content"), str):
        return False, None
    content = user[-1]["content"]
    if not content.startswith(TITLE_PREFIX):
        return False, None
    if len(user) != 1:
        _fail("DSH title request contains unknown or repeated human messages")
    if not any(isinstance(value, str) and value.startswith(TITLE_SYSTEM_PREFIX) for value in system):
        _fail("DSH title route lacks its published first-prompt system format")
    try:
        framed = json.loads(content[len(TITLE_PREFIX):])
    except json.JSONDecodeError:
        _fail("DSH title request has malformed published JSON framing")
    if (not isinstance(framed, list) or len(framed) != 1 or not isinstance(framed[0], dict)
            or type(framed[0].get("seq")) is not int or framed[0]["seq"] <= 0
            or set(framed[0]) != {"seq", "text"} or not isinstance(framed[0].get("text"), str)):
        _fail("DSH title request does not contain exactly one sourced human message")
    return True, framed[0]["text"]


def strip_dsh_context_suffixes(messages: list[dict]) -> tuple[list[dict], list[str]]:
    remaining = list(messages)
    suffixes = []
    while remaining:
        content = remaining[-1].get("content")
        if not isinstance(content, str):
            break
        if content.startswith(RUNTIME_CONTEXT_PREFIX) and len(content) > len(RUNTIME_CONTEXT_PREFIX):
            suffix = "runtime-context"
        elif re.fullmatch(
                r"Time sampled while preparing turn [1-9][0-9]*, step [1-9][0-9]*: "
                r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}[+-][0-9]{2}:[0-9]{2}\[[A-Za-z0-9_/+.-]+\]\n"
                r"Browser time zone for this request: unavailable\. Ask the user to clarify otherwise-unqualified dates and times\.\n"
                r"Elapsed since the preceding (?:model-visible message|step context): "
                r"(?:unavailable|(?:[0-9]+d )?(?:[0-9]+h )?(?:[0-9]+m )?[0-9]+s)\.", content):
            suffix = "time-context"
        else:
            break
        if suffix in suffixes:
            _fail("DSH provider repeated a runtime or time context suffix")
        suffixes.append(suffix)
        remaining.pop()
    if len(suffixes) > 2:
        _fail("DSH provider has too many host context suffixes")
    return remaining, list(reversed(suffixes))


def inspect_provider_request(kind: str, body: object, *, expected_text: str,
                             images: list[dict], image_bytes: list[bytes], upload_receipt: dict,
                             input_item: dict, dsh_home: str) -> dict:
    """Verify one actual provider user message and return non-sensitive evidence."""
    request = _object(body, "provider request")
    file_id = upload_receipt["file"]["attachmentId"]
    if kind == "native":
        users = _user_messages(request, "input")
        if len(users) != 1:
            _fail("Native Responses request must contain exactly the one original human input")
        user = users[0]
        parts = _array(user.get("content"), "Native Responses user content")
        actual = []
        observed_images = []
        file_payloads = []
        for part_value in parts:
            part = _object(part_value, "Native Responses input block")
            kind_value = part.get("type")
            if kind_value == "input_text":
                actual.append(("text", part.get("text")))
            elif kind_value == "input_image":
                mime, data = _data_uri(part.get("image_url"), "Native input_image")
                actual.append(("image", data))
                observed_images.append((mime, data))
            elif kind_value == "input_file":
                mime, data = _data_uri(part.get("file_data"), "Native input_file.file_data")
                actual.append(("file", part.get("filename"), mime, data))
                file_payloads.append((part.get("filename"), mime, data))
            else:
                _fail("Native Responses user content contains an unknown block")
        expected = ([ ("text", expected_text)] if expected_text else [])
        expected += [("image", image_bytes[0]), ("file", FILE_NAME, "text/plain", FILE_BYTES),
                     ("image", image_bytes[1])]
        if actual != expected:
            _fail("Native Responses provider content differs from exact authored text, source images, or uploaded file")
        normalized = [{"mime": mime, "byteCount": len(data), "sha256": hashlib.sha256(data).hexdigest()}
                      for mime, data in observed_images]
        return {"kind": kind, "authoredText": expected_text, "contentOrder": [item[0] for item in actual],
                "images": normalized, "nativeFile": {"filename": FILE_NAME, "mime": "text/plain",
                    "byteCount": len(FILE_BYTES), "sha256": hashlib.sha256(FILE_BYTES).hexdigest()},
                "hostContextSuffixes": []}

    users = _user_messages(request, "messages")
    human_messages, context_suffixes = strip_dsh_context_suffixes(users)
    if len(human_messages) != 1:
        _fail("DSH request has unknown, duplicate, or missing human input")
    user = human_messages[0]
    content = user.get("content")
    if not isinstance(content, list):
        _fail("DSH mixed prompt must remain an ordered content-block list")
    expected_blocks = []
    if expected_text:
        expected_blocks.append({"type": "text", "text": expected_text})
    dsh_attachments = input_item.get("attachments")
    if not isinstance(dsh_attachments, list) or len(dsh_attachments) != 3:
        _fail("DSH source history lacks its exact image/file/image descriptors")
    for index, image_index in enumerate((0, 2)):
        descriptor = _object(dsh_attachments[image_index].get("attachment"), "DSH image descriptor")
        expected_blocks.append({"type": "text", "text": _expected_image_handle(images[index], descriptor, dsh_home)})
        expected_blocks.append({"type": "image_url", "image_url": {
            "url": "data:image/png;base64," + base64.b64encode(image_bytes[index]).decode("ascii")}})
        if index == 0:
            expected_blocks.append({"type": "text", "text": _expected_file_handle(file_id, dsh_home)})
    # DSH's published Pi AI serializer puts the durable image handle immediately before
    # every actual request image and replaces the file at its original content index.
    normalized = []
    observed = []
    for block in expected_blocks:
        if block.get("type") == "image_url":
            actual_block = content[len(observed)]
            if (not isinstance(actual_block, dict) or actual_block.get("type") != "image_url"
                    or set(actual_block) != {"type", "image_url"}
                    or not isinstance(actual_block.get("image_url"), dict)
                    or set(actual_block["image_url"]) != {"url"}):
                _fail("DSH actual OpenAI image_url block is missing at its published content position")
            mime, data = _data_uri(actual_block["image_url"].get("url"), "DSH provider image_url")
            expected_mime, expected_data = _data_uri(block["image_url"]["url"],
                                                       "canonical DSH source image")
            if mime != expected_mime or mime != "image/png" or data != expected_data:
                _fail("DSH provider image bytes or MIME differ from canonical source data")
            observed.append({"type": "image_url", "mime": mime, "data": actual_block["image_url"]["url"]})
            normalized.append({"mime": mime, "byteCount": len(data), "sha256": hashlib.sha256(data).hexdigest()})
        else:
            actual_block = content[len(observed)]
            if actual_block != block:
                _fail("DSH handle/text content differs from its exact published projection")
            observed.append(block)
    if len(content) != len(expected_blocks):
        _fail("DSH provider content contains unknown or duplicate blocks")
    expected_order = [block["type"] for block in expected_blocks]
    return {"kind": kind, "authoredText": expected_text, "contentOrder": expected_order,
            "images": normalized, "fileHandleVerified": True,
            "hostContextSuffixes": context_suffixes}


def classify_dsh_request(body: object) -> tuple[str, str | None]:
    """Classify exactly one published first-prompt title request or a prompt request."""
    request = _object(body, "DSH provider request")
    is_title, title_text = _is_title_request(request)
    return ("title", title_text) if is_title else ("prompt", None)


def record_imageio_evidence(observations: list[dict], image_data: list[bytes], directory) -> list[dict]:
    """Run the platform ImageIO tool on every provider image and retain converted pixels."""
    rows = []
    index = 0
    for request_index, observation in enumerate(observations, 1):
        request_row = dict(observation)
        decoded_rows = []
        for image_index, data in enumerate(image_data[index:index + len(observation["images"])], 1):
            mime = observation["images"][image_index - 1]["mime"]
            decoded = decode_image_with_imageio(data, mime, directory,
                "provider-request-" + str(request_index) + "-image-" + str(image_index))
            decoded_rows.append(decoded)
        index += len(observation["images"])
        request_row["images"] = [dict(image, **decoded)
                                  for image, decoded in zip(observation["images"], decoded_rows)]
        rows.append(request_row)
    if index != len(image_data):
        _fail("ImageIO did not process every original provider image byte sequence")
    return rows
