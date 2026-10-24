# Agent UI tool report and narration source audit

Date: 2026-10-24  
Issue: [TekesAgentKit #19](https://github.com/TekesApps/TekesAgentKit/issues/19)  
PR: [TekesAgentKit #10](https://github.com/TekesApps/TekesAgentKit/pull/10)  
AgentKit source baseline: `58521a1bc581ef14c3c63c9a7bc698bbd01d9ab3`  
ClientKit reference: `c289bc9d33edcd83a782ac17927a8c65a027eccd`  
Tekes host reference: `8858ccd9d46590852ec7a0949aee289fc6798d70`

This is a static source-contract audit. It does not claim a live producer exists merely because a renderer has a value type or an endpoint transports opaque JSON. The parent session separately ran the shared presentation policy regressions: 26 passed, recorded at `.runtime-fixtures/issue-batch-20261024/source-policy-tests.log`. Those tests cover selection/formatting/narration policy only; they do not establish a live producer or real harness integration. No harness was started, no desktop/screenshot acceptance ran, and no build/test was run specifically for this document. ClientKit and Tekes had unrelated pre-existing WIP; only committed, unmodified paths were used as references. Source hashes and inspected paths are recorded in the adjacent JSON receipt.

## Result

No explicit public producer was found for `TranscriptToolReport` or a separately captured group-narration fallback in Native, Pi, DSH, or Tekes' host adapter. The currently available Tekes host producer is a **workspace-current diff snapshot** with a revision. It is the ClientKit-style fallback for a tool file row, not an account of what an individual tool call changed. It can be supplied through the existing `ToolHeaderAdapter` input callback; no duplicate API is needed.

The three endpoints carry tool result payloads, and some preserve metadata/details, but AgentKit deliberately retains those as opaque `result_meta`. A field named `didDo`, text in a result/stdout, or records in a private harness database are not accepted as an explicit report producer. Likewise, an assistant message's ordinary text is a valid message carrier; it does not establish a distinct group-fallback producer.

For all three harnesses, report and separate group-fallback support is **unknown / no producer found**, not “unsupported.” In contrast, DSH's refusal of native-conversation streaming is explicitly unsupported and must remain classified that way. Missing fields, explicit empty values, explicit tombstones, and declared unsupported capabilities are separate states.

The same distinction matters for host capability reporting. ClientKit DSH explicitly declares `canEditHistoricalInput == false`; no corresponding declaration was found on ClientKit Native or Pi, so those remain unknown in this audit. Do not convert an omitted capability into unsupported. AgentKit's current `EndpointCapabilities.can_edit_historical_input: bool` defaults to `false`, which cannot by itself represent unknown separately; that API-level limitation belongs with the #15 contract audit and is not changed by this #19 source-only work.

## Producer matrix

| Producer | Tool report | Workspace diff fallback | Independent group narration | Identity/update evidence |
| --- | --- | --- | --- | --- |
| Native SessionEndpoint | **Unknown; no public typed report field/producer.** Journal `tool/result` retains the source result and metadata; presentation keeps `result_meta` opaque. | **No endpoint producer found.** A host may supply a current workspace snapshot via the existing callback. | **Unknown; no separate public group-text lane.** Assistant message content is the canonical text source. | endpoint + session scope, turn/step, tool call ID, event sequence; stream generation exists. No report/fallback-specific revision or absence/tombstone contract. |
| Pi SessionEndpoint | **Unknown; no public typed report field/producer.** Completion `details` are retained as opaque result metadata. | **No endpoint producer found.** Host-only current snapshot is possible. | **Unknown; no separate public group-text lane.** ReadingTracker's stream ID fences partial assistant messages; it is not a narration-group identity. | endpoint + session scope, turn and tool-call IDs; journal order/entry identity; stream identity is specific to assistant message partials. No report/fallback-specific revision or tombstone. |
| DSH SessionEndpoint | **Unknown; no public typed report field/producer.** DSH tool-result `meta` is documented in code as tool-private/opaque; do not mine it for a report. | **No endpoint producer found.** Host-only current snapshot is possible. | **Unknown; no separate public group-text lane.** Packed assistant text/reasoning chunks are reconstructed as assistant blocks, not as an independent group fallback. | endpoint + session scope, turn/step, tool call ID, journal seq/time, assistant block index/revision. No report/fallback-specific revision or tombstone. |
| Tekes host | **No report callback/provider found in current host adapter.** | **Present:** `ClientRunHostActions.changeCounts` returns the host's displayed diff plus its revision. The result is workspace-current, scoped by the selected Run environment, and not historical per-call evidence. | **No separately captured group-text provider found.** | current diff revision is explicit. The host callback itself does not carry a tool report ID or narration frame ID. |
| ClientKit reference | Renderer accepts typed `TranscriptMetalCallReport`, but the tracked normal SessionEventProjector → SQLite bridge writes `callReportJSON: nil`; the transient feed also supplies `callReport: nil`. No producer assignment was found in this committed source closure. | Present as a host/UI input (`WorkspaceGitDiffSnapshot`) keyed by path with revision. It is explicitly a current-workspace fallback. | Normal SessionEventProjector group projection publishes frame identity/order/call membership and `narrationArrivalOrder`, while the direct SQLite and transient bridges set `narration: ""`. Actual assistant message nodes remain the canonical text carriers. No separately captured fallback producer was found in this closure. | group/call rows have endpoint/session/thread/turn IDs, per-call IDs, versions/event order; callback diff has a revision. Report-specific producer provenance and group-fallback presence/tombstone semantics are not in the normal bridge DTO. |

Capability example (not a report/narration producer): ClientKit DSH's `DSHHostEndpoint.canEditHistoricalInput` is explicitly false (`Sources/Session/SessionEndpoint/DSH/DSHHostEndpoint.swift:16`). Native and Pi do not declare that property in the inspected endpoint implementations, so classify them as unknown here. AgentKit's boolean default cannot prove upstream unsupported when the source omitted the fact.

## What is available to consume now

- `TranscriptToolReport` is an explicit value with open `verb`, optional `subject`, `resultCount`, `resultObject`, `note`, and a complete `addedLines` / `removedLines` pair. Shared presentation handles wording and own-report-over-workspace-count precedence.
- `ToolHeaderAdapter.inputs(scope, tool)` receives the `RunScope` and specific `TranscriptToolCall`; its `revision(scope)` marks one immutable host display snapshot. `RunPort::new(...).with_transcript(labels.with_tool_header(adapter))` already connects this for both embedded and standalone tool headers.
- `TranscriptNarrationFallback` accepts separately captured `frame_id` and visible node texts. `TranscriptNarrationPolicy` selects a group fallback only from those explicit input nodes and applies running-prefix/settled carrier rules; it never extracts a fallback from a sibling item.
- The present `RunScope` identifies endpoint, thread and session. Tool calls have a scoped call ID and turn ID; public source positions/stream generation provide source ordering where available. These are sufficient context for a host callback to look up one tool's explicit report, but the report object itself does not carry provenance fields. The callback must enforce exact scope + tool identity and snapshot revision.

The existing adapter is the supported host seam. This audit does not add another API, parse `result_meta`, change endpoint projections, or synthesize fixtures to represent real producer data.

## Contract needed before any producer is wired

Any future public source should provide, in one immutable publication:

1. **Authority:** endpoint ID, session ID, thread ID, producer/source identity and schema version.
2. **Target:** exact tool call ID and turn ID for a tool report; exact response frame/group ID for group narration. No path suffix, text equality, or matching tool name may substitute for an identity.
3. **Version:** connection/generation identity plus a monotonic source revision or durable event sequence, with defined replace/append/final semantics. A delayed old update must not overwrite a newer report or narration.
4. **Presence:** distinguish `unknown` (producer did not publish), `present` (including an intentionally empty payload if allowed), authoritative `absent`/tombstone (producer removed a prior value), and `unsupported` (endpoint explicitly declares the capability unavailable). A missing JSON key alone is not a tombstone.
5. **Tool report fields:** `verb`, optional `subject`, optional result count/object, note, and either both line counts or neither. A complete zero delta is valid. The report is descriptive; tool execution success/settlement still comes from the endpoint's canonical lifecycle fields.
6. **Group narration fields:** separately sourced ordered text nodes or structured visible-text blocks, frame ID, and revision/presence. Do not copy a canonical message into this slot. The presentation policy owns carrier selection and duplicate suppression.
7. **Workspace diff:** keep separate from per-call report. It must identify the current workspace/repository snapshot, unique file path resolution and revision. It may be used only as the presentation fallback for the current diff, never relabeled as “what this call changed.”

Until a producer publishes this contract, report and group-fallback paths stay unknown and empty in real endpoint integrations; ordinary tool rows, assistant messages, and the host's independent current diff continue to work. Real protocol fixtures and lifecycle/recovery integration remain open under #19. This source audit does not close #19.

## Evidence references

See [`rust-agent-ui-public-sources-20261024.json`](rust-agent-ui-public-sources-20261024.json) for the exact source hashes. Principal code locations:

- AgentKit Native, Pi and DSH endpoint adapters: `crates/endpoint-kernel/src/lib.rs`, `crates/endpoint-pi/src/lib.rs`, `crates/endpoint-pi/src/reading.rs`, `crates/endpoint-dsh/src/lib.rs`, and `crates/endpoint-dsh/src/history.rs`.
- AgentKit projection and explicit inputs: `crates/presentation/src/transcript.rs`, `crates/presentation/src/tool_report.rs`, `crates/presentation/src/tool_header.rs`, `crates/presentation/src/transcript_narration.rs`, `ui/components/src/output_message.rs`, and `ui/components/src/transcript_content.rs`.
- ClientKit public projected call/group fields: `Sources/Contract/SessionEndpoint/SessionProjectionValues.swift`; DSH group projection: `Sources/Session/SessionEndpoint/Client/SessionEventProjector.swift`; Pi result mapping: `Sources/Session/SessionEndpoint/PI/PITranscriptProjection.swift`.
- ClientKit normal persistence bridge: `Sources/Session/Sync/ClientSQLiteSessionStore.swift:818-853`; transient tool feed: `Sources/TranscriptCoordinator/TranscriptTransientToolFeed.swift:31-61`; typed mirror adaptation: `Sources/TranscriptCoordinator/TranscriptValues+Mirror.swift:47-94`.
- ClientKit host diff model / renderer: `Sources/TranscriptRendering/WorkspaceGitDiffSnapshot.swift`, `Sources/TranscriptRendering/TranscriptMetalAppBridges.swift:219-250`, `Sources/TranscriptRendering/TranscriptMetalActivity.swift:298-308`, and `Sources/TranscriptRendering/TranscriptToolVocabulary.swift`.
- Capability-state comparison: `Sources/Session/SessionEndpoint/DSH/DSHHostEndpoint.swift:16`, `Sources/Session/SessionEndpoint/Client/SessionEndpointRuntime.swift:127`, and AgentKit `crates/contract/src/capabilities.rs:8-16`.
- Tekes host: `Tekes/ClientAgentHostAdapters.swift:24-30` supplies only the displayed diff snapshot and revision; `Tekes/ClientAgentSummaryHostAdapter.swift:13-19` supplies summary artifact counts from the same workspace source.

