# Real multi-endpoint Sidebar acceptance (#22 / #29)

## Objective and boundary

Run one current AgentKit offscreen demo against simultaneously available, isolated Native, Pi and DSH runtimes. Validate the original shared inventory, query projection, pagination and row ownership from actual public protocol results. Existing sequential per-harness tests remain a baseline, not proof of one aggregate host. This is an acceptance tool/profile; no Agent UI state machine, SDK control, private harness database or user history parser is added.

The published TekesUI pin remains `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`. Strict #53 nested reading stays failed until its public fix is consumed and independently verified. This profile does not prove desktop visibility, physical IME, AX, ProcessContainer or complete Views parity, and does not enable existing single-endpoint proof/selftest flags for multi-endpoint mode.

## Shared fixture ownership

Extract only launch/preparation lifetimes from `test-real-session-endpoints.py`: Pi public models configuration, DSH web readiness/version-plugin/cookie exchange/cleanup, and Native bootstrap/readiness/cleanup. Preserve the old script's default sequence, operation assertions and capability checks. Do not import that executable script as a module or evaluate source snippets. Keep provider response behavior in an importable shared fixture implementation with explicit inputs, not a copied alternate protocol.

The new profile uses a closed subprocess environment with owned HOME/dataRoot/XDG/temp paths, synthetic credentials and loopback provider endpoints. Native/DSH servers are held concurrently by an ExitStack; Pi processes remain owned by the original Rust connector. Continuously drain DSH stdout, redact launch tokens/cookies, record actual product handshakes and executable/source hashes, and clean up only owned processes/groups. Use existing installed Kernel0.9.3, fixed Pi1.3.0 baseline and DSH0.2.1-rc.2; no install/update, Kernel/SDK changes or new target. All temporary files stay on the SSD.

## Public seeding and one-host receipt

Create and rename owned sessions through public endpoint APIs. Obtain actual IDs and canonical titles/cwd from public snapshots; generate a unique input marker and observe a real reply through each harness using the controlled provider. No synthetic session/history file writes. Each endpoint supplies at least two distinct actual IDs to the existing `endpoints` JSON configuration; use environment references for authentication, never serialize token values into that config or public receipts.

Add a narrow offscreen receipt option, independent of the existing single-endpoint probes. Serialize one already-applied Sidebar host packet together with its mounted component state: full-address source rows, per-endpoint observation/readiness/errors, original activity/name-corpus metadata, displayed rows, query generation/child phases/hits/unresolved, folder catalog, paging facts, mounted row IDs and available component-provided reveal target. Do not re-snapshot different runtimes while composing a receipt. Revision numbers alone do not prove authority; the call site must pass the packet it actually applied.

The external runner checks actual seed return values against that receipt. Every endpoint must be individually healthy and ready; aggregate loaded alone is insufficient. Display rows must come from the source inventory with the same address/title. Local-name and provider-history search evidence remain separate. Pagination requires actual inventory over the component page limit and an original SidebarPageRequest/load_more transition, with before/after rows and paging facts. An absent or unmounted target fails the corresponding limited check; do not infer desktop visibility from a PNG. If public runtimes cannot use identical session IDs, leave that collision case to the existing mixed fixture and state that limit.

## Staged verification

1. Existing protocol baselines and fixed-entrypoint/version checks. A new SSD-path regression already exposed a fixture-only whitespace parser error in the DSH working-directory probe; preserve the failed log and verify its correction before launcher extraction.
2. Mocked launcher lifecycle/isolation/redaction tests and old-profile equivalence checks; source/serial shared-target component tests for the receipt.
3. One-host baseline, unique query and real local pagination, using the same demo executable and fixed protocol inputs. Each stage fails nonzero for missing endpoints, replies, coherence or expected rows; save partial receipts rather than turn them into success.
4. Main review, exact source/program/input hashes, per-Issue backfill and new-head CI. UI remains experimental; no merge/tag while required gates fail.

## Implemented profile and discovered focus defect

The runner is `scripts/test-real-multi-sidebar.py`; all paths are explicit existing runtime/program inputs. It creates three public sessions per harness (nine total), rather than relying on a selected-row supplement to make six rows look like a positive page transition. The demo's new `--sidebar-delivery-receipt <new absolute JSON path>` remains separate from existing single-endpoint proof flags. One process records baseline → original page control semantic Press → completed query, using the exact retained host packet, actual accepted display projection and parent ContentHost node membership. An original `ComponentDemo::select(0)` event establishes the component-provided reveal target. The runner fails if that target is absent or unmounted, any endpoint is unhealthy, a row/title/cwd differs from actual seeds, original row IDs change, query hits remain unresolved, or paging/query changes the keyboard owner. It does not claim a desktop reveal or OS accessibility interaction.

The strict profile exposed a real consumer defect: the ordinary page button used the SDK default activation focus, leaving focus on the removed page control after the last page. The fix is the existing public `Content::preserving_focus()` attribute on AgentKit's page button. The SDK still owns pointer/semantic activation and explicit Focus/Tab eligibility; no event workaround, new state machine or TekesUI source change is needed. The red component test and failing real receipt are retained beside the corrected run.

The profile does not cover real Task-family navigation (#23), mixed archived/ephemeral/child classification producer completeness (#29), same-ID collisions across actual independent runtimes, remote failure injection, native scrolling/visibility, physical Chinese IME or AX. Those remain independent requirements. Final evidence is recorded in [the results](rust-agent-ui-real-multi-sidebar-results-20261024.md).
