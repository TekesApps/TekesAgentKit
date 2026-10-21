# Output aside visibility (experimental, 2026-10-22)

This follows the temporal footnote integration and keeps exact public TekesUI
`09b4bab44b6dfa65487250ea816b610b9df59095`. No SDK, Kernel, ClientKit or Tekes
source is modified; the implementation remains Rust-only.

ClientKit `TranscriptTurnFootnote.rawOutputNodes` requires a live presentation
owner, actual output, or a settled turn with completed-turn fork controls before
creating an output aside. A source end timestamp alone does not create a footer.
The same rule now controls `TurnFootnote`'s active contents. The known shared
completed-turn action catalog supplies fork controls independently of whether
those controls are currently enabled. Admission is still revalidated by existing
presentation/bindings; visibility grants no capability.

`TurnFootnoteDisplay::has_output` is an explicit display fact for standalone hosts.
The formal Transcript derives it from visible, final, nonprovisional assistant
content. Prose, attachments and visible unsupported content can establish output;
reasoning-only/tool-only blocks, hidden items, preview streams and timestamp-only
turns cannot. This flag is not a clipboard or feedback target. Existing typed
final-output-copy and feedback eligibility remain separate. Struct-literal users
of this unpublished 0.0.0 API must add the field (or use Default). Standalone
`update_with_output` still treats its validated output argument as output presence.

The original footer and information owner are privately retained while hidden.
No hidden caption, gesture or phantom active row is exposed. Showing it again
uses the same original SDK information node; a visibility/live-owner transition
retires captured action identities. Source facts remain readable through `facts`.
The existing SDK mixed-document reading transaction owns geometry/fallback;
AgentKit adds no offset, focus, editing or scrolling engine.

Read-only comparison includes ClientKit `TranscriptTurnFootnote.swift`,
`TranscriptMetalSnapshotBuilder.swift`, `TranscriptMetalAppBridges.swift`,
`TranscriptValues.swift`, and Tekes `ClientAgentHostAdapters.swift`. Tekes continues
to compose the shared Run and provide navigation/system adapters.

Adjacent JSON records final source/program hashes, tests and native receipts.
The complete UI486 suite covers the final business component changes; after adding
the opt-in native fixture/assertions, the final host tree passes targeted footer,
Transcript and independent-consumer tests plus strict all-target Clippy/build.
The frozen native program verifies the timestamp-only history negative case before
and after settling the live turn, original information/Composer owners and marked
baseline, reading/paging/Start/Latest, readonly paging and Sidebar reveal. The
original native fixture covers send/stream/cancel/drafts/reconnect; system Pinyin
passes on the same executable. Native screenshot was inspected. An initial reading
run lost native window activation before the focus assertion; the same executable
passes an isolated retry without source/assertion changes. Both logs are preserved.
Physical keyboard/candidate-window/VoiceOver and new real-harness acceptance remain
unverified by this stage.
The component matrix covers settled/failed/interrupted/open/waiting turns,
output-presence/fork-catalog combinations and disabled mutation admission.
Mixed Transcript tests cover prose, image-only, reasoning/tool-only, hidden,
preview and nonfinal cases. The independent public consumer verifies a timestamp-only
settlement hides its information node, then formal output restores the same node
and exact host-formatted timestamp without accepting stale plans.

Native evidence remains distinct from fixture/CoreText evidence and historical
real Native/Pi/DSH receipts. Full footnote feedback packing, rich content,
glyph/hover/goal-memory accessories and paired product visual parity are unfinished.
UI remains experimental and PR #10 draft; this does not complete the overall goal.
Builds reuse the existing shared target. No cache is deleted in this stage;
released space is 0 bytes. Source/WIP, fixed snapshots, logs/failures, screenshots
and acceptance executables are retained.
