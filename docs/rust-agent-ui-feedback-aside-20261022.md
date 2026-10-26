# Feedback aside display and auxiliary focus (experimental, 2026-10-22)

Exact public TekesUI remains `09b4bab44b6dfa65487250ea816b610b9df59095`.
ClientKit `TranscriptTurnFootnote.rawOutputNodes` applies one output-aside gate,
returns only the running line for a live owner, and emits the note entry only
for a committed rating. A committed note supplies the entry's text. Tekes
`ClientAgentHostAdapters` composes the shared Run and supplies host navigation;
no host/view/SDK/Kernel source is modified in this Rust port.

AgentKit now shares the same `TurnFootnoteDisplay::shows_aside` predicate between
TurnFootnote and Transcript's feedback mount. A proven final response remains a
canonical feedback target even if its empty content and absent fork catalog make
the output aside invisible; visibility does not change that authority. Image-only
output can show feedback without inventing prose copying. A live owner never
mounts rating controls. Existing target capture, binding/pointer/CAS checks,
request admission, no automatic retry and late-completion rules are unchanged.

When an existing feedback editor is composing, a publication that would hide the
aside is rejected before editing the content tree, even when the response itself
is unchanged. After explicit composition commit, the original publication path
may retire the control. Existing read-only and stale-binding gates still apply.

`TurnFeedback` no longer renders a disabled note placeholder for an unrated target.
A rated target shows its committed note or the configured `FeedbackLabels.edit_note`
when no note exists. An empty status adds no caption band. Unknown/failed/CAS refresh
states remain explicit; this change does not remove their recovery controls or
optimistically replace committed data. The existing original SDK note editor is
retained across state/authority updates.

Input-copy, output-copy, turn-action and feedback auxiliary buttons consume
TekesUI `Content::preserving_focus()`. Original SDK pointer/AX activation, capture,
paint and state are reused. Pointer/AX Press retains the existing keyboard/IME
owner; Tab and explicit accessibility Focus remain governed by the SDK policy.
No focus restoration, editor replacement, fabricated key event or new input engine
is added. An explicitly clicked note editor can still acquire focus normally.

The independent public-API consumer checks the empty-response gate, committed
note text, absence of an unrated note/empty caption, pointer/AX activation on both
copy lanes and feedback, and the original Composer pointer, focus and marked-text
baseline. Component tests additionally verify image/fork gating separately from
canonical final-response proof, and IME preflight for an otherwise unchanged
response losing its aside. Complete tests and program scopes are in adjacent JSON.

Real Native/Pi/DSH feedback probes use the existing isolated runner, local controlled
provider and fixed runtime roots. Kernel uses explicit installed 0.9.3 binaries
with `--kernel-ui-only`: no Kernel rebuild or lifecycle/archive acceptance is
inferred. Pi and DSH retain their explicit capability/runtime scopes. Native probe
assertions check the hidden unrated note, committed note label and original Composer
focus, text, directed selection and marked state through refresh/rating, as well as existing note IME barriers,
CAS mutation, session isolation, explicit reread and no prompt submission.
The first probe incorrectly required an opaque editor authority epoch to survive
an acknowledged controlled draft republication. Fixed SDK `text_input/external.rs`
explicitly advances that epoch; the final probe checks the same original instance,
focus and semantic input state instead. The independent no-republication consumer
still verifies exact marked baseline preservation. The failed program/log is kept;
Native receipts explicitly set `opaqueBaselinePreserved=false`.
The final frozen program passes Pi1.3.0, DSH0.2.1-rc.2 and Native0.9.3 native
feedback receipts. Pi/DSH use the preserved pre-seed runner; the only subsequent
runner change seeds an otherwise blank Native independent-UI fixture via public
prompt/waitSettled before the probe's before-snapshots. Native passes with that
fix and still prints NATIVE_LIFECYCLE_NOT_RUN. A DSH attempt detected unexpected
Composer input not present in the probe actions and stopped; its same-program
isolated retry passes. The original failure and Native blank-target timeout remain.
No root cause is asserted for the unexpected input, and no assertion was removed.

The business/independent-consumer UI489 suite passes. Subsequent Rust changes are
native-probe observations/capture only, covered by strict all-target Clippy/build
and the final frozen program. Tools31, fmt, boundaries and actual Cargo source
closure pass. The same final program passes the original-window time/aside,
reading, readonly paging, Sidebar reveal and standard send/stream/cancel/draft/
reconnect fixture modes plus system Pinyin. The three real screenshots were
inspected: Pi/DSH show the committed-note entry; Native records the save/editor
transition and is not claimed as a static committed-note screenshot. There is no
paired ClientKit visual acceptance.
Only completed receipts establish acceptance; physical keyboard/candidate-window,
VoiceOver and paired ClientKit visual parity are separate.

This does not complete the single-row output-aside composition/order with copy,
rating/fork and timestamp; feedback remains a separate content row. Full glyph,
hover, goal/memory accessories, source aggregate-output copying and remaining
same-name UI layout require further work. Existing upstream gaps stay in their
original issues; no new generic API gap is claimed for this change. UI stays
0.0.0/publish=false and PR #10 draft until the full goal audit/review permits release.

Builds reuse the existing UI shared target and existing headless target. No new
target or cache deletion is performed. Source/WIP, fixed inputs, logs/failures,
receipts, screenshots and acceptance executables are retained; released space is
0 bytes. Runtime/node/Kernel identity is checked before the real probes.
