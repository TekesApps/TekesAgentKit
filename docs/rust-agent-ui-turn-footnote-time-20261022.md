# Transcript footnote display time (experimental, 2026-10-22)

This continues PR #10 on the existing exact public TekesUI revision
`09b4bab44b6dfa65487250ea816b610b9df59095` (peeled
`remediation-toolbar-checkpoint-20261021`). Public main and tags were rechecked;
there is no newer public main to upgrade to. #22/#28 reading/reveal APIs are
already consumed. Their open issue state is not a missing-API verdict.
No TekesUI, ClientKit, Tekes or Kernel source is modified.

## Source and ownership

Read-only references are ClientKit `TranscriptTurnFootnote.swift`,
`TurnEvaluationPresentation.swift`, `TranscriptMetalAppBridges.swift` and
`TranscriptMetalSnapshotBuilder.swift`, MetalTextKit `CompactCountFormat.swift`,
and Tekes `ClientAgentHostAdapters.swift`. Tekes composes the shared Run and owns
navigation, system capabilities and product adapters. AgentKit owns the display
rules over its existing presentation. Generic editor, TextContent, focus, reading,
scroll geometry and rendering stay with TekesUI. The sample remains Rust-only.

The live owner is the eligible last open, nonfinal turn in the same current Run.
Disconnected, stale, interrupted, archived and noncontinuable child destinations
cannot acquire live display from a retained running journal. Live output shows
elapsed whole seconds, omits zero day/hour/minute fields, always includes seconds,
and appends only positive provider live tokens using compact thousands and the
source's three-space separator. Unknown/epoch-zero start times are not invented
zero durations. A settled output shows the localized source end timestamp,
not raw status, diagnostic partial-report prose or elapsed duration. Live footnotes
hide copy/action controls and retire their old gestures; settlement restores
controls under new identities. Information retains its original SDK node.

Input and its footnote continue trailing alignment; machine output and its
footnote lead. This change does not claim complete feedback/control packing,
hover/glyph, goal/memory or full same-name ClientKit footnote layout parity.
Completed-final output copy retains the existing typed-copy policy; partial/failed
output copy differences remain work to audit.

## Public inputs and host integration

- `TranscriptFootnoteTiming { revision, now_ms, live_turn_id }` is an explicit
  display observation, independent of canonical provider revision. `for_run`
  derives eligibility from the existing RunSnapshot; it creates no Run state.
- `TurnFootnoteDisplay` carries the accepted per-turn observation and optional
  host-formatted settle timestamp. `TurnFootnoteLabels::finished_timestamp`
  receives source Unix milliseconds and optional display time. Its default
  returns None; standalone hosts must supply calendar/locale formatting.
- `TranscriptContent::{prepare_update_with_timing,update_with_timing}` include
  the display input in the existing preflight/apply and original SDK reading
  transaction. Stale or conflicting display revisions reject before graph edits,
  including when canonical model publication advances. Old prepared plans reject.
- `RunView::set_footnote_clock` stages a finite positive host Unix-ms observation
  before preparing the usual Run reading plan. It never edits controls directly.
  `has_live_footnote` lets the host schedule only an accepted, presented live owner.
- `ReadOnlyTranscript::set_footnote_clock` supplies calendar observation only;
  bounded readonly history never acquires a live owner.

The native sample uses a quantized one-second SystemTime clock and the existing
WindowWake. Its owned task is retired on settlement, hidden panes and close.
Calendar, locale and timezone formatting stays in `demo::footnote_host`, using
exact macOS `objc2-foundation = 0.3.2` already present in the lockfile closure.
No new package version is fetched. The adapter uses a forced 12-hour locale,
short clock for input, weekday within 0..<7 local calendar days, and numeric
month/day otherwise. Calendar-day comparison covers DST rather than assuming
24-hour days. It preserves other locale keywords. See Apple's
[hourCycle API](https://developer.apple.com/documentation/foundation/locale/hourcycle-swift.property)
and Unicode CLDR's [hour-cycle keyword definitions](https://cldr.unicode.org/translation/displaynames/locale-option-names-key).
Shared components import neither Foundation nor SystemTime. NonmacOS hosts supply
their own adapter.

New execution events in the owned demo fixture now use actual Unix milliseconds.
Historical replay timestamps remain unchanged; components do not clamp away bad
source timestamps. The runtime test checks prompt start/end timestamp bounds.

## Validation and evidence scope

Final source, exact executable hashes, receipt paths and source-reference hashes
are recorded in the adjacent JSON. UI workspace tests include the independent
public-API consumer, stale-clock/new-model fencing, retained information node,
Run reading transaction with original Composer focus/preedit, noncontinuable
eligibility, Foundation 12-hour locale and DST cutoff. Strict all-target Clippy,
format, source closure and dependency boundaries are checked.

Native modes exercise actual owned windows and public in-process SEP backhaul.
The new time mode advances display without domain publication, observes positive
and zero live-token reports, preserves the original information/Composer nodes
and marked baseline, settles to a calendar timestamp, and retires the wake.
Reading/readonly modes cover pagination and automatic target reveal with original
focus/preedit. The original owned-fixture native selftest covers send, streaming,
cancel, isolated drafts and reconnect. The real-endpoint stream-reading invocation
was rejected before execution because no explicit endpoint configuration was passed;
its log is retained and it is not counted as behavior acceptance. The real-endpoint
Run-controls mode was not rerun in this stage.
System Pinyin uses PID-targeted software key events with the input source unchanged.
These do not establish physical-keyboard/candidate-window, VoiceOver or paired
product visual acceptance. Historical real Native/Pi/DSH receipts keep their
original program scope; no new real-harness result is asserted here.

The first AgentKitFootnoteTime.app program/receipts are preserved as intermediate
because normal prompt fixture timestamps were corrected afterward. Final acceptance
uses a separately frozen executable, without overwriting that history. Full UI
remains 0.0.0/publish=false and PR #10 remains draft. No merge/tag is authorized
by these partial acceptance results.

Builds reuse `/tmp/agentkit-layout-text-292-target`. No new target or cache deletion
is performed in this stage; source/WIP, fixed SDK snapshots, logs, failure records,
screenshots and acceptance programs remain. Shared variants are retained because
independent consumers and other tasks may use them; released space is 0 bytes.
