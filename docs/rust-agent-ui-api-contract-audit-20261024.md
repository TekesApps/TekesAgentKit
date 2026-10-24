# Agent UI public API and host contract audit — 2026-10-24

Scope: AgentKit #15, checked against the current Rust component source and the fixed public TekesUI
revision used by this branch. ClientKit and Tekes host files were read-only comparison inputs. No
Rust implementation, Swift/Kernel/SDK source, issue state, or UI dependency was changed by this
audit. The separate consumer was prepared without building by the generator, then compiled and run
by the main session using the shared target; those receipts are recorded separately below.

## Decision

The current source has public constructors for the three business containers and a second in-repo
consumer, but it does **not** yet provide a published AgentKit UI package. AgentKit v0.3.8 is the
published baseline; the UI workspace remains `0.0.0` / `publish = false` and depends on shared
contracts/presentation through repository paths. A product outside this checkout cannot reproduce
the current UI contract from v0.3.8 alone. Treat this branch's component API as an experimental
development contract until a reviewed immutable AgentKit release includes the required UI and shared
presentation changes.

The current code names the reusable composition `RunPort`. `RunSurface` remains a protocol/control
test harness; `ComponentDemo` is a host example. Neither is the reusable Run API. The three public
constructors belong to `tekes-agent-components-ui`; the direct consumer fixture deliberately imports
that package alone and constructs all three components.

## Version and dependency closure

| Input | Audited value | Meaning |
| --- | --- | --- |
| AgentKit checkout | `58521a1bc581ef14c3c63c9a7bc698bbd01d9ab3` | Source baseline when the fixture was prepared. |
| Published AgentKit baseline | `v0.3.8` | Does not contain the unpublished UI workspace API. |
| AgentKit UI packages | `0.0.0`, `publish = false` | Development-only; host path override must be explicit. |
| TekesUI | `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c` | Exact public Git revision in the UI manifest and lock. |
| TekesUI reviewed closure | 8 locked crates / 243 reviewed inputs | Manifest digest `3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c`. |
| AgentKit UI lock | `ui/Cargo.lock`, SHA-256 `8790cacd3395460c518f71240052a0bde27a25dbe7e18acda12753c53653d9d1` | Pinned branch dependency graph; standalone consumer will get its own lock. |

The 8 locked TekesUI crates are `tekes-ui-controls`, `tekes-ui-design-export`, `tekes-ui-motion`,
`tekes-ui-platform-macos`, `tekes-ui-render`, `tekes-ui-scene`, `tekes-ui-text`, and
`tekes-ui-tokens`, all at version `0.1.0` from the same full Git revision. The direct consumer graph
may be a subset; every TekesUI package it resolves must still point at that exact revision.

TekesUI #48 and #64 were read on 2026-10-24 and remain open. #48's current public status separates
the latest SDK main revision from this consumer's fixed `50ec49f` pin. The pinned closure does not
include the `module-runtime` / `app-protocol` ProcessContainer delivery required to claim same- and
cross-process acceptance. Keep the existing `ContentHost` path; consume ProcessContainer only after
the required public API is present in an exact reviewed revision. This audit does not upgrade the
pin based on the existence of a newer SDK main commit.

## Public component contract

| Public type | Host supplies | Component returns / owns |
| --- | --- | --- |
| `RunPort` | One validated `RunScope`; exclusive component namespaces and row range; the captured `RunSnapshot`, composer affordances/control gate, paging facts, and actionable availability on each update. | Composer + Transcript + Indicators composition, layout/display presentation, original source/scope fences, prepared update, and typed `RunPortIntent` values. Construct with `RunPort::standard(namespace, rows, scope)` or explicit `RunPort::new(...)`; install via `into_host()`. A mounted instance cannot be retargeted. |
| `Sidebar` | Versioned `SidebarInput` from one host inventory authority, including restored/loading/error distinction, canonical session addresses, activity/pin/action projections, selected address, plus explicit preference/search publications. | Session navigation and row presentation, paging/reveal state, typed select/action, page, preference, search, and visibility intents. Construct with `Sidebar::new(namespace, labels)`; update from the host inventory; install via `into_host()`. It has no private inventory or preference store. |
| `SidePanel` | Monotonic `SidePanelInput` with parent scope, `projection_ready`, ordered tabs, selected tab, the existing `SideChatSnapshot`, and known/unknown SideChat capability. | Tab navigation/content slot, mount/content tokens, typed select/close/create intents, retained UI content within the configured bound, and source fences. Construct with `SidePanel::new(namespace, labels, retained_parent_limit)`; update from the host projection; install via `into_host()`. It does not own task, child, or SideChat controllers. |

The host drains TekesUI actions immediately and passes them to each component's public intent/page
consumers. It dispatches each captured intent through the original endpoint binding, controller,
preference store, or navigation owner, checking its captured scope, pointer, request token, and
publication before applying asynchronous results. It must not resolve an old gesture through the
newly selected session. The standalone sample demonstrates construction and `ContentHost` mounting;
its independent compile/run result is recorded below; construction does not prove rendering or a
ProcessContainer integration.

## Ownership and platform responsibilities

| Layer | Responsibility |
| --- | --- |
| AgentKit components/presentation | Agent-specific states and layout; Run/transcript/composer/indicator presentation; task, child, SideChat, Summary and sidebar models; pointer/source ownership; stale-frame/request fences; typed business intents. |
| TekesUI | Generic controls/tokens/layout/rendering; text surface, first responder, selection, undo and marked-text composition; shared scrolling, reading/reveal transactions and generic process/container primitives. |
| Host | Window and whole-app layout/navigation; endpoint/runtime subscriptions; product copy/theme inputs; model, tool, file/path, install/credential and system-permission adapters; platform event forwarding; executing the original typed intents; geometry publication only after a successful current render. |

IME ownership stays in the original TekesUI editor. AgentKit checks the captured composer/pane
preflight before sending or changing destinations and must not simulate commits or re-create text
input. TekesUI owns reading anchors and scroll compensation; AgentKit publishes the matching
snapshot/page/reveal transaction, and the host forwards it without estimating anchors from row count
or pixels. The host owns window geometry and only publishes geometry after successful rendering.

The pinned SDK currently exposes the same-process `ContentHost` route used by these components. It
does not satisfy the full ProcessContainer contract. Same-process and cross-process behavior, remote
keyboard/IME/accessibility forwarding, lifecycle, and native interaction remain separately
unverified. Keep these gates explicit and do not copy those generic facilities into AgentKit.

## ClientKit comparison and evidence identity

Read-only ClientKit references confirm the product ownership split used here: embedded Runs keep
their pane source identity and stop with view removal; task identity is resolved from its live
projection and a missing task removes the old pane; task-to-run session selection is a shared
projection rather than a host-created fallback. The Tekes host reference keeps host event wiring
separate from AgentKit presentation.

The reference repositories contained unrelated WIP and were not modified. Current reference commits
and source hashes are preserved in
[`rust-agent-ui-api-contract-audit-20261024.json`](rust-agent-ui-api-contract-audit-20261024.json).
They are comparison provenance, not build evidence for Swift packages or the Tekes host.

## Clean public-consumer fixture

The generator creates a separate package with its own `[workspace]`, an explicit path to
`ui/components`, the public constructors, and a seeded copy of the reviewed UI lock. Its preparation
receipt records AgentKit HEAD and a SHA-256 over all 128 Rust/manifests/lock/pin inputs for
components, contract, presentation, and session; the source-package worktree status was clean and
matched the recorded HEAD. Preparation itself does not invoke Cargo or create a target.

Fixture: `.runtime-fixtures/issue15-contract-20261024/external-consumer-58521a1-63d36cd67c/`

The main session copied this sample into a separately reserved verification folder, resolved its
lock offline, checked `cargo metadata`, verified the exact SDK sources against the 243 reviewed
inputs, and executed the binary using the shared target. `cargo run --locked --offline` exited 0.
The resulting graph contained five local packages (external root, components, contract,
presentation, session) and six TekesUI packages (controls, design-export, motion, scene, text,
tokens); no demo or consumer-check package was present. The independent receipt records 131 actual
source/Cargo inputs and fingerprint
`b6300987b478db6a3b63bee88abd2c2676b06a2706475913c0a7ce451827d589`; after execution each input
was rechecked unchanged. The archived executable SHA-256 is
`fc812429a2912043b9dc4c5f197e8bcc79738c20145e4b04da4fc071a7842718` (15.85 MB).

Run evidence: `.runtime-fixtures/issue-batch-20261024/public-consumer-{inputs,run-receipt}.json`.
This proves independent compile and constructor/ContentHost execution against the fixed dependency
closure. It does not prove a rendered/native window, ProcessContainer same-/cross-process behavior,
IME/accessibility, or complete ClientKit View parity.

The fixture generator is `scripts/check-public-agent-ui-consumer.py`; its three Python tests confirm
it prepares a direct external package without invoking Cargo and refuses to overwrite unrelated WIP and includes both workspace manifests in source provenance.

## Remaining work for #15

- Compare final host APIs against the eventual AgentKit release and update the explicit development
  override to that immutable release pin.
- Consume ProcessContainer only from a fixed public TekesUI version and separately verify its
  same-process and cross-process modes.
- Keep native/IME/accessibility and full ClientKit View acceptance under their own gates; this API
  audit does not close those requirements or PR #10.

Related tracking: [AgentKit #15](https://github.com/TekesApps/TekesAgentKit/issues/15), overall plan
[#14](https://github.com/TekesApps/TekesAgentKit/issues/14), and draft
[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10).
