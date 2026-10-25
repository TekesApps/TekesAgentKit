"""Validate a same-publication Native parent Task error witness."""
from __future__ import annotations

from typing import Any

from native_summary_transition_fixture import validate_parent_summary_sample


class NativeParentFailureError(ValueError):
    """Raised when a published failure is not sourced by its parent call."""


def validate_running_to_failed_parent(
    running: Any, failed: Any, *, approval_response: Any, expected_draft: str
) -> dict[str, Any]:
    """Prove one original parent Task moved doing -> error across two captures.

    Source validation delegates to the shared Native Summary transition
    validator, which preserves both source dialects. Native 0.9.3 is expected
    to use journal entries for the parent Task.
    """
    try:
        first = validate_parent_summary_sample(running)
        last = validate_parent_summary_sample(failed)
    except (ValueError, TypeError, KeyError) as error:
        raise NativeParentFailureError(str(error)) from error
    if first["status"] != "in_progress" or running.get("rawStatus") != "doing":
        raise NativeParentFailureError("first parent Task capture is not doing")
    if last["status"] != "error" or failed.get("rawStatus") != "error":
        raise NativeParentFailureError("second parent Task capture is not failed/error")
    if running.get("scope") != failed.get("scope"):
        raise NativeParentFailureError("parent Run scope changed across the transition")
    if running.get("connectionEpoch") != failed.get("connectionEpoch"):
        raise NativeParentFailureError("connection epoch changed across the transition")
    if (type(running.get("publication")) is not int or type(failed.get("publication")) is not int
            or failed["publication"] <= running["publication"]):
        raise NativeParentFailureError("failed publication does not follow the doing publication")
    identity_keys = ("sourceCallId", "sourcePosition", "taskId", "title", "rowId",
                     "viewInstance", "titleInstance")
    if any(first[key] != last[key] for key in identity_keys):
        raise NativeParentFailureError("parent Task or mounted Summary identity changed")
    for label, sample, checked in (("doing", running, first), ("failed", failed, last)):
        if sample.get("scope", {}).get("endpointId") != "native":
            raise NativeParentFailureError(f"{label} capture is not Native")
        draft = sample.get("draft")
        if not isinstance(draft, dict) or draft.get("text") != expected_draft or not expected_draft:
            raise NativeParentFailureError(f"{label} capture lost the parent draft")
        if checked["serverTransactionalFreshnessProven"] is not False:
            raise NativeParentFailureError("server freshness must remain unproven")
    response = _object(approval_response, "approval response")
    actionable = _object(failed.get("approval"), "captured per-call approval")
    witness = _object(running.get("witness"), "running witness")
    authority = _object(witness.get("authority"), "running authority")
    catalog = _object(authority.get("actionables"), "running actionable authority")
    authoritative_actionable = _object(actionable.get("actionable"), "captured original approval actionable")
    if catalog.get(actionable.get("actionableId")) != authoritative_actionable:
        raise NativeParentFailureError("captured approval differs from running authority actionable")
    payload = _object(authoritative_actionable.get("payload"), "original approval actionable payload")
    if (authoritative_actionable.get("id") != actionable.get("actionableId")
            or authoritative_actionable.get("sessionId") != running.get("scope", {}).get("sessionId")
            or authoritative_actionable.get("revision") != actionable.get("revision")
            or payload.get("approvalId") != actionable.get("approvalId")
            or payload.get("callId") != first["sourceCallId"]
            or payload.get("toolName") != "task"):
        raise NativeParentFailureError("approval actionable is not for this original parent Task call")
    failed_tool = _object(failed.get("tool"), "failed parent Transcript tool")
    result_error = _object(failed_tool.get("resultError"), "original failed parent result")
    if result_error.get("code") != "denied":
        raise NativeParentFailureError("original parent Task result is not the Kernel approval denial")
    if response.get("accepted") is not True or response.get("outcome") != "rejected":
        raise NativeParentFailureError("the matching approval was not rejected")
    if response.get("actionableId") != actionable.get("actionableId"):
        raise NativeParentFailureError("approval response does not match the captured actionable")
    if response.get("approvalId") != actionable.get("approvalId"):
        raise NativeParentFailureError("approval response does not match the per-call approval")
    if (type(response.get("expectedRevision")) is not int or response["expectedRevision"] <= 0
            or response.get("expectedRevision") != actionable.get("revision")):
        raise NativeParentFailureError("approval response revision is missing")
    return {
        "transition": "doing->failed",
        "endpoint": "native",
        "sourceCallId": first["sourceCallId"],
        "taskId": first["taskId"],
        "sameMountedSummaryIdentity": True,
        "parentDraftPreserved": True,
        "approvalRejected": True,
        "sourceDialect": first["sourcePosition"]["kind"],
        "serverTransactionalFreshnessProven": False,
    }


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise NativeParentFailureError(f"{label} must be an object")
    return value


def validate_parent_only_catalog(catalog: Any, parent: str) -> dict[str, Any]:
    """Require the public Native catalog to contain only the original parent."""
    value = _object(catalog, "public child catalog")
    entries = value.get("entries")
    if value.get("parentAvailable") is not True or not isinstance(entries, list) or len(entries) != 1:
        raise NativeParentFailureError("public catalog does not prove a parent-only Run")
    entry = _object(entries[0], "public parent catalog entry")
    if entry.get("kind") != "parent" or entry.get("id") != parent or entry.get("hasChildren") is not False:
        raise NativeParentFailureError("public catalog contains a child or a different parent")
    return {"parentSessionId": parent, "publicCatalogOnlyParent": True, "childCount": 0}
