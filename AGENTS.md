# AgentKit ownership and scope

This is a Rust repository. Do not add Swift source, embedded Swift code, Swift package targets,
or build/test steps requiring a Swift toolchain. ClientKit is an external behavior/contract reference;
keep protocol fixtures and verification inside this repository language-neutral or Rust-native.

TekesAgentKit owns reusable Rust Agent components and presentation state. TekesClientKit remains
the owner of Swift Agent UI. Align behavior with ClientKit; compare the ClientKit implementation
and the Tekes host before changing cross-repo behavior. Preserve unrelated WIP in all checkouts.

SessionEndpoint contracts, capability semantics and fixture parity precede UI implementation.
Kernel, Pi and DSH adapters normalize into the same contract. Transport locality, harness identity
and cache policy are independent. No direct access to harness-private databases as a shortcut.

TekesUI owns generic controls, tokens, layout, text and rendering. AgentKit owns Run/transcript,
Composer, Agent settings, tasks/subagents/SideChat, Summary, health and Agent list state/components.
Hosts own windows, navigation, overall layout, product theme/copy, scenario inputs, installation,
credentials, paths and system permissions through explicit inputs/callbacks.

Kernel implementation changes are out of scope. Development overrides must be explicit; release
builds need immutable published pins. Do not silently depend on an active sibling worktree.

For functional cross-repo UI changes validate AgentKit, its host and a second public-API consumer.
Swift ownership changes additionally require Tekes/scripts/check-agent-ui-ownership.py against the
intended ClientKit root and the shared Swift package/host/second-consumer checks. A Rust port alone
does not make that Swift checker a Rust ownership test; add a suitable Rust dependency boundary check.

Report static, replay, integration, native UI, IME/accessibility and performance evidence separately.
Native/Pi/DSH implement independent headless paths. Run/Transcript/Composer/connection presentation
is implemented independently of rendering; Rust UI components remain experimental and full View parity is unproven. Preserve submission identity,
destination isolation, stale-read fencing and no-automatic-retry semantics when adding hosts.
Keep the capability matrix in docs/session-endpoints.md accurate. Do not claim full ClientKit or UI
parity from headless builds and tests.

# Issue-driven delivery

Track the complete Rust Agent UI scope in GitHub issue #14 and
`docs/agentkit-requirements-coverage.md` (machine-readable mapping:
`docs/agentkit-issue-coverage-20261025.json`). The older
`docs/rust-agent-ui-issue-plan.md` retains commit-scoped historical evidence.
Reuse an existing matching issue before creating one.
Keep #14 as the Epic and #15–#28 as feature trackers. Implementation children
must describe independently deliverable capabilities, not individual tests or
validation runs. Each implementation issue has an owner, scope, deliverables,
specific dependencies and closure criteria. Separate implementation, actual
protocol acceptance, desktop/IME, upstream SDK consumption and release ownership.
Dependencies block only the affected child requirements.
Use exactly one status: 待办, 进行中, 已验证待集成, 等待上游, 等待桌面, 已关闭.
Verified code in an unmerged PR stays 已验证待集成 and OPEN. Before closing,
verify its applicable acceptance and mainline integration; record original
commit/program/version, PR, actual checks, not_run scope and completion time.
Historical deliveries retain their original evidence identity; a later passing
binary or a new issue audit does not revalidate an older/newer release.
A small change follows issue → implementation → verification; complex work first updates its
design and splits linked issues. The main chat coordinates; new independent child sessions own
implementation and use subagents as needed. Do not contact ended sessions. Small changes may be
completed directly by the main chat.
Record completed implementation separately from acceptance and merge/release status.
Before closing an issue, backfill implementation commits, applicable validation, unverified scope
and PR links. Design changes update both the design and the issue. Do not auto-close every
workstream when a broad experimental PR merges; each issue has its own acceptance criteria.
