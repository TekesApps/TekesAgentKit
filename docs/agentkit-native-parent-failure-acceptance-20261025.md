# Native parent Task failure witness — 2026-10-25

This is one bounded missing real-protocol slice under #56. It is not a feature expansion or a full #56 acceptance. The required observation is a same parent Task changing from public `doing` to public `error`/canonical `failed` after a real per-call approval is rejected.

## Frozen inputs and scope

- AgentKit source: `5b7c84361206b45ac381b10437097d3b76c322c1`.
- Public consumer probe: `ui/consumer/examples/native_parent_approval.rs`, opt-in feature `native-parent-approval`; the runner requires its exact executable through `--probe-program` and never builds or selects a target path itself. The stage coordinator binds its digest to the frozen overlay build receipt.
- Native Kernel: 0.9.3, with explicit supervisor/worker paths and build identity checked before the bounded run. The frozen protocol source is `09508a9083c24c7accefc99516739f279b85913e`.
- One Native parent Run, one valid public `task` call held by `approvals.select` (`workspace-write`), and two `presentationSource` witnesses. No desktop, archive executable, child session, or unrelated provider is involved.

## Failure mechanism and evidence rule

The loopback provider returns a schema-valid `task` call with all required contract fields, including `output.mode: inline` and `output.path: ""`. Before the call, the host selects `workspace-write` approval so the Kernel holds the per-call approval. The consumer captures the parent Task in `doing`, then responds to the exact public actionable using its id and revision with `{approvalId, outcome:"rejected"}`. It captures the later error from the same opaque parent call. The original parent `tool/result` must be denied and paired to the original call; the public child catalog must show only the parent. A denial follow-up may make additional provider requests; each request and call count must be recorded, and no child call may be accepted as the parent's failure.

Acceptance requires the same successful original `presentationSource` publication and same-memory Native authority to show all of the following:

1. A selected parent Run with exact Native scope, ready Native source, fresh Transcript, and one Summary Task row in failed/error state.
2. A matching Transcript `task` tool with the schema-valid inline arguments and failed/error phase.
3. `sourcePosition` resolving to the original Native parent journal `tool/call` (or, where the source dialect is NativeConversation, its assistant `toolCall`), with exact original arguments and opaque call identity. Native 0.9.3 is known to publish the parent source in its journal; journal is the expected native source dialect here.
4. `resultPosition` resolving later in that same source dialect to the matching parent `tool/result` or `toolResult`, carrying the same opaque call ID and the public denial/error; the published Transcript result must equal that original result.
5. The Summary row, tool, parent Run, publication, and authority all agree on parent identity and connection epoch. Child navigation/session pointers and child Summary rows are rejected. A separate public child catalog must contain only that parent with `hasChildren=false`; a parent-only subscription is insufficient proof that no child was spawned.

Native Transcript derives the tool phase from the original `toolResult.isError`; a journal-only `resultError` field is not required and is not used as proof. Kernel flow documentation at the frozen source says “Child crash != parent crash”; this fixture does not synthesize parent state from a child-model or child-process error.

## Rejections and limits

The validator rejects a missing original result, mismatched call ID or source entry, wrong source dialect, wrong epoch or selected Run, non-error original result, invalid task arguments, mismatched Summary/tool state, and child result/pointer. JSON validation cannot prove that the remote service itself was transactionally fresh or that the entire producer witness was not fabricated; both limits remain explicit.

The runner writes to a new evidence directory and never replaces prior receipts. It records the probe executable, Kernel supervisor/worker and build description, provider snapshot, frozen source manifest, and Python inputs by SHA-256. The final receipt binds the command result, both phase witnesses, validation, provider observations, catalog, and owned-service exit evidence. It accepts only a clean public-consumer exit, Native 0.9.3 handshake, valid original Task call, explicit per-call approval rejection, validated `doing → failed` witness pair, parent-only catalog, and owned Kernel shutdown. It does not enlarge a timeout to turn a failure into a pass.

Acceptance also requires both captures to retain the same Run, connection epoch, call/task identity, mounted Summary view/row/title identity, and the nonempty parent draft. Only the approval response may occur between captures. `doing → failed` is accepted only if the first capture is before rejection and the later capture contains the same task and matching original denial result. The normal consumer example composes the existing public controller and SummaryView; it must not use the ComponentDemo or create derived UI state.

## Invocation shape

The stage coordinator supplies `--probe-program <frozen-overlay-executable>`, the frozen Kernel root and exact 0.9.3 supervisor/worker paths, and a new output path beneath the existing SSD fixture root. Do not execute this runner until the stage coordinator has frozen the shared-target build window and confirmed the explicit probe digest.

## Current implementation validation

The Python validator has focused synthetic positive/negative tests. No Native protocol run, Cargo command, desktop interaction, physical IME, accessibility, or performance validation is performed by implementing this fixture. Actual protocol status must be recorded by the stage coordinator after running the explicit invocation; an unrun script is not #56 evidence.


### Native approval-denied parent Task (bounded acceptance)

`native-parent-approval-v5/acceptance.json` binds Kernel 0.9.3 (`09508a9`) to
consumer SHA `e742cafae5ba3715ba3c3be16be8a3c1f849e1a34baff8fa4d9cbfbe638d5caf`,
built over frozen product source `5b7c843` with the new opt-in acceptance example.
The original parent journal call/result proves `doing → denied/error` in the same
Run/epoch, mounted Summary view/row/title identity and unchanged nonempty parent
draft. Public catalog contains only the parent. Two loopback provider requests
include the original task and its exact continuation/result; consumer and owned
Kernel exit 0. No child failure substitutes for the parent result. Server
transactional freshness is unproven; full #56, performance and desktop remain
unaccepted. Failed prior attempts are retained. v4's provider metadata overwrote
the follow-up marker-presence flag; v5 corrects the recorder and is authoritative.
This changes acceptance tooling only, not frozen product behavior or gate scope.
