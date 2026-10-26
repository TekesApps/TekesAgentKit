# SessionEndpoints + existing TekesWebUI delivery

Current execution scope, 2026-10-26. Epic #14 remains the complete long-term
requirement; this phase supersedes the native-first ordering in the historical
2026-10-25 stage freeze. Owners: main chat coordinates integration and serialized
Cargo; independent workers own bounded source changes. #37 owns service/consumer
integration, #55/#56 actual protocol and performance, #58 review/integration,
#59 immutable headless/Web delivery. No implementation issue is closed by this
priority change.

## Inputs and ownership

- AgentKit main: `1dcc52153ea45ba6ce73a135a8d8fe8d3406360c`.
- Shared candidate: PR #10 `2c8a2fb83c8df1834b087d23970b37a4b7f0d109`;
  product baseline `10263722b5fb128922c49a31b5942c41333cd6e2`.
- Pi read/cleanup candidate: PR #72 `3f5b2e5f4449297d6833f43fe366f5ea4e741d24`.
- Existing WebUI source: `2e61ba2dd69bee950ebaa494d1a578d9af55d006`, with
  pre-existing WIP; public contract archive `a8610a420bad72eaedcf706b30df87153a0f3e16`.
- Saved local entry references AgentKit 0.3.8, Kernel 0.9.3 and WebUI 0.1.23;
  on entry the host reported `running:false`. Configured paths are not a fresh
  browser/runtime acceptance. No credentials or private configuration are published.

Reuse WebUI's TypeScript authenticated HTTP/NDJSON host and existing launcher.
AgentKit owns adapters, SessionRuntime, business presentation and private JSONL
service. Do not create another listener, installer, model proxy or browser state
machine. Shared generated TypeScript DTOs and the runtime binary must be pinned
and verified separately. Existing native WIP remains excluded from builds.

## Integration order and finish conditions

1. Isolate the committed contract/session/three SEP/presentation/Web source
   closure from native `ui/`. Include required resources, generated DTOs, public
   consumer and applicable headless checks. Review the closure before freezing
   validation input. Preserve native checks on the native PR; do not delete or
   weaken shared gates.
2. Compare the existing WebUI against that closure; fix only demonstrated
   compatibility and core-use failures. Use its existing setup/start/stop/status/
   open entry. Protect pre-existing WIP and do not reinstall runtimes.
3. Reuse version-bound evidence. Complete missing/affected Native/Pi/DSH protocol,
   queue, attachment, history/reconnect, task/child and performance checks under
   #55/#56. A synthetic provider is a repeatable protocol fixture, not real-model
   acceptance. Retain failed samples and budgets.
4. Validate the actual browser consumer: connect, create/open/model, prompt and
   streamed reasoning/tools, cancel/queue, approval/questions, upload/history/
   pagination/reconnect, supported tasks/children, destination drafts. Verify
   browser close/reopen independently of host restart. After public fixtures pass,
   use one minimal existing real-model invocation, without logging credentials.
5. #58 exact-source review/CI/independent consumer, merge and main CI; #59 release
   an immutable version and point the existing WebUI at it. Only integrated,
   accepted capabilities count as delivered; publish exact versions and limits.

## Current failures and remaining work

Kernel #86 is OPEN; the latest public release checked on 2026-10-26 is 0.9.3.
Native busy input admission with an empty public queue remains a supported-path
failure. Kernel implementation is outside this repository's scope. Determine
whether an AgentKit-side public-protocol correction exists; otherwise report the
minimal upstream change/authorization needed. Do not label the path Unsupported.

The original Native lifecycle, Pi warm/history and DSH warm/cleanup relative
performance failures remain valid at their original source/program/observer
identities. The Pi read pipeline is a bounded candidate, not proof those budgets
are resolved. New tests cannot erase those failures or borrow old passing results.
Native Family's completed 21-case cohort remains bound to its original programs.

TekesUI #53 and other purely native rendering/reading/IME/AX gaps do not block an
actually isolated headless/Web closure. They continue to block their native scope.
No native desktop, physical IME or AX acceptance in this phase. Native components,
fixed snapshots, programs and evidence remain preserved. Files/artifact preview,
terminal/browser panels and command palette stay enhancement work unless the
existing WebUI core workflow demonstrates a concrete blocker. Full Epic #14
requirements are unchanged.

## Current execution

An SSD worktree `codex/headless-web-delivery` isolates the source closure without
editing the primary WIP. WebUI compatibility is checked against its current public
consumer, not rebuilt as a new application. Build outputs reuse the existing SSD
shared target; immutable batch sources are not edited during validation. Historical
evidence retains original commit/runtime identities. Final release/browser and
real-model acceptance are pending, not implied by this document.

## DSH host dependency boundary (#67)

AgentKit owns the minimal authenticated runtime-version adapter in
`integrations/dsh-runtime-probe`; see [installation and scope](dsh-runtime-probe.md).
The consumer uses `/api/agentkit/harness-version` and retains strict official
runtime evidence checks. Tekes App builds and the Tekes repository CI are not
AgentKit release gates. Existing optional workspace plugins retain their own
release/compatibility responsibilities; this adapter neither replaces their file
tools nor changes the user's profile. The new adapter must pass real Host
authentication and consumer acceptance before deployment; unit tests do not
establish that the current daily Host has loaded it.
