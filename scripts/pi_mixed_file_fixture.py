"""Strict queue-ready and Pi provider payload checks for the mixed-file probe."""

from __future__ import annotations

import base64
import binascii
import hashlib
import json
import re


READY_PREFIX = "AGENTKIT_PI_MIXED_QUEUE_READY "
DIRECT_TEXT = "PI_MIXED_DIRECT"
HOLD_TEXT = "PI_MIXED_QUEUE_HOLD"
QUEUED_TEXT = "PI_MIXED_QUEUED"
FILE_NAME = "中文混合.txt"
FILE_BYTES = b"AgentKit Pi mixed file proof\n"
EXPECTED_REPLY = "ENDPOINT_REPLY_OK"
FILE_PROMPT_PREFIX = 'Attached file: "' + FILE_NAME + '" (path: '


def _is_int(value: object, *, positive: bool = False) -> bool:
    return type(value) is int and (value > 0 if positive else value >= 0)


def _decode_png_part(part: object, label: str) -> dict:
    if not isinstance(part, dict) or part.get("type") != "image":
        raise ValueError(label + " must be an image block")
    if part.get("mediaType", part.get("mimeType")) != "image/png":
        raise ValueError(label + " must declare image/png")
    encoded = part.get("data")
    if not isinstance(encoded, str) or not encoded:
        raise ValueError(label + " is missing canonical base64 data")
    try:
        data = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError):
        raise ValueError(label + " has malformed base64") from None
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError(label + " bytes do not have a PNG signature")
    return {
        "type": "image",
        "mediaType": "image/png",
        "name": part.get("name"),
        "data": encoded,
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "_bytes": data,
    }


def validate_queue_ready_lines(lines, *, expected_session_id: str, gate_entered: bool,
                               expected_file_receipt: str | None = None,
                               expected_rpc_id: str | None = None) -> dict:
    """Validate the actual public queue fact before the provider hold is released.

    A `mode == "queue"` event is insufficient: this requires a matching queued fact in
    the event's RunSnapshot payload, the exact mixed content order, an entered hold gate,
    and nonempty revision/epoch witnesses.
    """
    if not gate_entered:
        raise ValueError("provider hold gate was not entered before queue-ready")
    if not isinstance(expected_session_id, str) or not expected_session_id:
        raise ValueError("expected queued session identity is missing")

    markers = []
    malformed = []
    for line in lines:
        if READY_PREFIX in line:
            if not line.startswith(READY_PREFIX):
                malformed.append(line)
            else:
                markers.append(line[len(READY_PREFIX):].strip())
    if malformed:
        raise ValueError("queue-ready marker was not a complete stdout line")
    if len(markers) != 1:
        raise ValueError("expected exactly one queue-ready stdout event")
    try:
        ready = json.loads(markers[0])
    except (TypeError, json.JSONDecodeError):
        raise ValueError("queue-ready event is not valid JSON") from None
    if not isinstance(ready, dict):
        raise ValueError("queue-ready event must be an object")
    if ready.get("status") != "queued" or ready.get("mode") != "queue":
        raise ValueError("queue-ready status or mode differs")

    expected_scope = {
        "endpointId": "pi",
        "threadId": expected_session_id,
        "sessionId": expected_session_id,
    }
    if ready.get("scope") != expected_scope:
        raise ValueError("queue-ready scope differs from the selected Pi session")
    rpc_id = ready.get("rpcId")
    if not isinstance(rpc_id, str) or not rpc_id or ready.get("ticketId") != rpc_id:
        raise ValueError("queue-ready ticket and RPC identities differ")
    if expected_rpc_id is not None and rpc_id != expected_rpc_id:
        raise ValueError("queue-ready RPC identity differs from the expected ticket")
    if not _is_int(ready.get("snapshotRevision"), positive=True):
        raise ValueError("queue-ready snapshot revision is missing")
    if not _is_int(ready.get("connectionEpoch")):
        raise ValueError("queue-ready connection epoch is missing")

    fact = ready.get("queueFact")
    if not isinstance(fact, dict):
        raise ValueError("queue-ready lacks a RunSnapshot queue fact")
    if fact.get("rpcId") != rpc_id or fact.get("placement") != "queued":
        raise ValueError("queue fact RPC or placement differs")
    fact_item_id = fact.get("id")
    if not isinstance(fact_item_id, str) or not fact_item_id:
        raise ValueError("public queue fact item identity is missing")
    message = fact.get("message")
    if not isinstance(message, dict) or message.get("role") != "user":
        raise ValueError("queue fact must contain a user message")
    content = message.get("content")
    if (not isinstance(content, list) or len(content) != 4
            or any(not isinstance(part, dict) for part in content)):
        raise ValueError("queue fact must preserve text,image,file,image order")
    payload = fact.get("payload")
    if (not isinstance(payload, dict) or payload.get("sessionId") != expected_session_id
            or payload.get("mode") != "queue" or payload.get("content") != content):
        raise ValueError("queue fact payload does not match its scoped queued message")
    if content[0].get("type") != "text" or content[0].get("text") != QUEUED_TEXT:
        raise ValueError("queue fact text marker differs")
    first = _decode_png_part(content[1], "queue fact first image")
    file = content[2]
    if not isinstance(file, dict) or file.get("type") != "file":
        raise ValueError("queue fact file is not at the second attachment position")
    receipt_id = file.get("receiptId")
    if not isinstance(receipt_id, str) or not receipt_id:
        raise ValueError("queue fact file receipt is missing")
    if expected_file_receipt is not None and receipt_id != expected_file_receipt:
        raise ValueError("queue fact file receipt differs from uploaded file")
    second = _decode_png_part(content[3], "queue fact second image")
    if first["sha256"] == second["sha256"]:
        raise ValueError("queue fact generated images must be distinct")
    queue_item_id = ready.get("queueItemId")
    if queue_item_id is not None and (not isinstance(queue_item_id, str)
                                      or fact_item_id != queue_item_id):
        raise ValueError("queue item identity differs from the public queue fact")

    return {
        "ready": ready,
        "scope": expected_scope,
        "rpcId": rpc_id,
        "queueFact": fact,
        "queueItemId": fact_item_id,
        "fileReceiptId": receipt_id,
        "images": [first, second],
        "gateEntered": True,
    }


def _image_from_chat_part(part: object) -> dict | None:
    if not isinstance(part, dict) or part.get("type") != "image_url":
        return None
    image_url = part.get("image_url")
    if not isinstance(image_url, dict):
        raise ValueError("Pi provider image_url block is malformed")
    uri = image_url.get("url")
    match = re.fullmatch(r"data:(image/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/]*={0,2})",
                         uri if isinstance(uri, str) else "")
    if not match:
        raise ValueError("Pi provider image is not a supported data URI")
    mime, encoded = match.groups()
    try:
        data = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError):
        raise ValueError("Pi provider image base64 is malformed") from None
    signatures = {
        "image/png": data.startswith(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg": data.startswith(b"\xff\xd8\xff"),
        "image/gif": data.startswith((b"GIF87a", b"GIF89a")),
        "image/webp": len(data) >= 12 and data[:4] == b"RIFF" and data[8:12] == b"WEBP",
    }
    if not data or not signatures[mime]:
        raise ValueError("Pi provider image MIME does not match its bytes")
    return {"mime": mime, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "_bytes": data}


def inspect_pi_user_message(body: object, marker: str) -> dict:
    """Inspect one outgoing OpenAI chat user message without retaining its request body."""
    if not isinstance(body, dict) or not isinstance(body.get("messages"), list):
        raise ValueError("Pi provider request is not an OpenAI chat message payload")
    messages = [row for row in body["messages"]
                if isinstance(row, dict) and row.get("role") == "user"]
    if not messages:
        raise ValueError("Pi provider request has no user messages")
    selected = messages[-1]
    content = selected.get("content")
    parts = content if isinstance(content, list) else ([{"type": "text", "text": content}]
                                                      if isinstance(content, str) else [])
    selected_text = "\n".join(part.get("text", "") for part in parts
                               if isinstance(part, dict) and part.get("type") == "text"
                               and isinstance(part.get("text"), str))
    if marker == HOLD_TEXT:
        expected_text = selected_text == HOLD_TEXT
    else:
        expected_text = has_transcript_file_prompt(selected_text, marker)
    if not expected_text:
        raise ValueError("Pi provider request does not contain the expected latest user marker")
    if any(not isinstance(part, dict) or part.get("type") not in ("text", "image_url")
           for part in parts):
        raise ValueError("Pi file attachments must remain text, not provider file blocks")
    images = [image for part in parts if (image := _image_from_chat_part(part)) is not None]
    return {"marker": marker, "text": selected_text, "images": images,
            "hasFilePrompt": FILE_PROMPT_PREFIX in selected_text}


def has_transcript_file_prompt(text: object, marker: str) -> bool:
    if not isinstance(text, str):
        return False
    prefix = marker + "\n\n" + FILE_PROMPT_PREFIX
    return text.startswith(prefix) and text.endswith(")") and len(text) > len(prefix) + 1


def validate_file_read(read: object, receipt: object) -> None:
    if not isinstance(receipt, dict) or not isinstance(read, dict):
        raise ValueError("public Pi file receipt/read is missing")
    receipt_file = receipt.get("file")
    expected_attachment = receipt_file.get("attachmentId") if isinstance(receipt_file, dict) else None
    try:
        expected_data = base64.b64encode(FILE_BYTES).decode("ascii")
    except Exception as error:  # pragma: no cover - defensive, constant is bytes
        raise ValueError("fixture encoding failed") from error
    attachment = read.get("attachment")
    if (not isinstance(expected_attachment, str) or not isinstance(receipt.get("receiptId"), str)
            or not receipt.get("receiptId") or not isinstance(attachment, dict)
            or attachment.get("attachmentId") != expected_attachment
            or attachment.get("name") != FILE_NAME
            or attachment.get("mediaType") != "text/plain; charset=utf-8"
            or attachment.get("bytes") != len(FILE_BYTES)
            or receipt_file.get("name") != FILE_NAME
            or receipt_file.get("bytes") != len(FILE_BYTES)
            or read.get("data") != expected_data):
        raise ValueError("public Pi fileAttachment read differs from uploaded file bytes")


def write_json_exclusive(path, value) -> None:
    """Write a JSON evidence receipt without replacing an earlier run's evidence."""
    with open(path, "x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2)
        stream.write("\n")
