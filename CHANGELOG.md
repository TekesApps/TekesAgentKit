# Changelog

## Unreleased — experimental Rust Agent UI

- Correct journal tool settlement for all content error markers, signals, timeouts and public result views; preserve raw result metadata/error in optional backward-readable fields without changing turn settlement.
- Reusable Agent business components and original-binding host integration remain experimental; see the [delivery gates](docs/rust-agent-ui-delivery-status.md).
- Add the read-only `TranscriptToolCall::presentation_title` selection for formal producer views; embedded and independent tool disclosures consume the same facts and SDK single-line labels.
- Shared additions on this branch are not part of the immutable v0.3.8 contract, even while the workspace version field remains 0.3.8. UI packages remain 0.0.0 / publish=false; full tool rows/cards, native/IME and ProcessContainer acceptance remain open.

## 0.3.8 — 2026-10-19

- Preserve the original worker launch notice and derive recovery only from a later durable turn start in the same session, including restored history.
- Expose the same shared presentation status to Web hosts; validate replay through a second public-API consumer alongside workspace, renderer and boundary checks.

## 0.3.7 — 2026-10-19

- Expose completed-turn footnote actions through the shared session view and existing thread action contract.
- Resolve turn forks to durable journal/native settlement boundaries, revalidate capabilities at execution, and preserve navigation on follow-up warnings.
- Add selected-boundary, restored-history and capability-change regression coverage plus a second public-API consumer.
- Validation: 103 workspace tests, strict workspace Clippy, format and dependency-boundary checks; real public-API history reads across Kernel/Pi/DSH. Browser acceptance remains with the consumer.

## 0.3.0 — 2026-10-18

- Pi independent child creation, parent-owned list/prompt/read/cancel, durable request-key replay,
  official extension model tool, child history/reconnect and Summary creation semantics.
- Native bounded child history through an additive Kernel read-only API, frozen paging/revision,
  effective task-source projection and explicit unsupported behavior on older Kernel hosts.
- No UI, Swift, durable transcript Mirror or Kernel mutation API changes.

## 0.2.0 — 2026-10-18

- Headless task pointer state, parent-owned subagent catalog/control, consumer-owned child history leases.
- Ephemeral SideChat lifecycle with uncertain-mutation reconciliation and confirmed-discard draft cleanup.
- Scoped Summary facts/providers, authoritative todo/usage recovery and typed host navigation actions.
- DSH catalog projection routing and child delivery/admission normalization for the released protocol.
- No UI/TekesUI or Kernel implementation changes.
- Validation: 72 Rust tests, strict Clippy, format/dependency boundaries and Linux CI; real isolated Kernel 0.9.0 / Pi 1.3.0 / DSH 0.2.1-rc.2 and 0.2.0-rc.2 integration with synthetic providers.
- Scope: a headless milestone, not full harness feature parity. Pi native subagents require explicit host services; Native bounded child history requires a public child-line projection source; DSH has no ephemeral SideChat. UI, durable client cache and large-history performance acceptance remain future work.

## 0.1.1 — 2026-10-18

- Public repository with MIT License and inherited license metadata for every workspace package.
- Expanded README, Rust onboarding, headless JSON command guide and contributor instructions.
- Read-only GitHub Actions token permissions and a bounded CI timeout.
- No changes to Run/Transcript/Composer or SessionEndpoint behavior from the 0.1.0 milestone.

## 0.1.0 — 2026-10-18

Annotated headless milestone including merged PRs #1 and #2:

- Independent Native (Kernel 0.9.0/v4), Pi 1.3.0 and DSH 0.2.1-rc.2 adapters; DSH 0.2.0-rc.2 compatibility.
- Session pointers, native ancestry / frozen-cut history, reconnect, actionables, attachments and version-checked files.
- Memory projection and explicit optional/host capability boundaries, without a disk transcript Mirror.
- Renderer-free Run, Transcript, Composer and connection state, destination-owned drafts and receipt-based optimistic input reconciliation.
- 59 local tests and synthetic-provider integration evidence, including the three presentation paths.

No UI/TekesUI integration, durable client cache, production launcher or large-history performance acceptance is included. Earlier CI attempts could not start because of account billing/spending limits; see individual workflow results for subsequent Linux validation.
