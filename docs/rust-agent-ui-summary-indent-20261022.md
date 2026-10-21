# Summary hierarchy geometry (experimental, 2026-10-22)

TekesUI stays at exact public `09b4bab44b6dfa65487250ea816b610b9df59095`.
No SDK, Kernel, ClientKit or Tekes reference source is modified.

## Published seam and source rule

ClientKit `SummaryDefaultRowView.configure/labelWidth/layout` applies the provider's
`MetalListRow.indentLevel` to the title's leading edge and available wrapping width.
Trailing metadata remains anchored to the same row trailing edge. `ListMetrics`
uses a deliberately off-grid 14-point step scaled with typography. Tekes
`ClientAgentSummaryHostAdapter` composes the shared Summary and supplies product
repository/navigation data. AgentKit does the same through explicit host inputs.

`SummaryLabels::indent_level: SummaryIndentFormatter` is a captured read-only
`Arc<dyn Fn(&SummaryOwner, &str, &SummaryRow) -> u32 + Send + Sync>` for ordinary default rows.
It receives the validated original owner explicitly, without looking up a newer
selected workspace. It defaults to zero. A provider/host supplies known hierarchy; Kit does not derive
it from slashes, titles, status or arbitrary JSON. Usage rows and custom Content
keep their own layout contracts and do not call this formatter. Action rows use
the same default-row rule, including when placed in a host section.

The level is captured during preflight alongside title and trailing formatting.
Changing the returned level under the same Summary publication rejects before
modifying the graph. Hosts must advance `SummaryInput`/`SummaryContextInput.revision`
when their projected hierarchy changes. No new SessionEndpoint DTO or Agent state
machine is introduced. Struct-literal callers of unpublished `SummaryLabels` must
add this field or use Default. The demo's `new_with_summary_labels` demonstrates
constructor injection; normal demo behavior keeps default zero levels.

The title moves by level * 14 * the public text host's type_scale and wraps in the
remaining title width. Metadata is measured independently and retains its trailing
edge even when indentation consumes all title width. Generic text measurement,
selection, controls, hit processing, clipping, paint, styles and colors stay with
TekesUI. The original action body retains its full hit/AX frame, and passive titles
retain their original read-only SDK text owner, selection and complete source text.
Only this application hierarchy rule uses the source 14-point geometry; no icon,
font, color, generic layout or input engine is copied.

## Validation scope

Adjacent JSON records exact source/program/receipt hashes and final checks.
The second public-API consumer covers all four themes, three type scales, two
widths, 0/1/3 levels, narrow/zero remaining title width, stable metadata trailing
edge, same-publication rejection, original action/text owners and selected text,
and unchanged captured actions. Component/host builds retain the existing SDK
source closure and namespace/owner contracts.

The original native Summary context mode uses an explicit host level projection.
It publishes level 2 while collapsed, then level 1 while expanded in the same
owner, verifies the original row/text field, metadata right edge, source-scale
geometry and Composer focus/marked baseline, then exercises owner/disclosure and
empty/unavailable behavior. A fresh optional `AGENT_SUMMARY_INDENT_CAPTURE` PNG
records the original demo window. Native/fixture/CoreText evidence is distinct
from real-harness, physical input, VoiceOver or paired product visual acceptance.
Historical Native/Pi/DSH receipts retain their original executable scope.
The initial repository-divider regression received an IME end notification after
its verification-complete phase; the probe now stops observing at that explicit
completion point and leaves closing to the host. All publication/geometry/IME
assertions before completion remain. Another reading attempt lost window activation
mid-scenario and did not complete; its log remains distinct from an isolated retry.
The first two-argument formatter program is retained separately; final owner-aware
API/program receipts supersede it without rewriting the old evidence.

## Remaining foundation and release boundary

Full single-row Transcript aside is still incomplete. Fixed SDK IconButtonView
requires IconRenderer, which is not exposed to Component::paint through the public
ButtonTextRenderer seam. Existing [TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)
now also records the action-icon use case and independent-consumer acceptance in
[the consumer supplement](https://github.com/TekesApps/TekesUI/issues/47#issuecomment-6328923129).
This is one existing renderer capability gap, not a request for generic wrapping
or an SDK Agent component. No engine/glyph/button implementation is added here.

Summary leading/status accessories, semantic runs, readonly whole-row routing and
full product row/SidePanel/paired layout remain under the existing audit/issues.
UI remains 0.0.0/publish=false and PR #10 draft; no merge/tag or overall completion
is implied by this hierarchy increment.

Builds reuse `/tmp/agentkit-layout-text-292-target`. No new target/cache deletion
is performed; released space is 0 bytes. Source/WIP, fixed inputs, failure records,
logs, screenshots and acceptance executables remain; shared variants with current
or uncertain ownership are preserved.
