# AgentKit ownership and scope

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
Kernel/contract/session implement the initial headless path; Pi/DSH/presentation/components remain
scaffolding. Do not claim full ClientKit or UI parity from headless builds and tests.
