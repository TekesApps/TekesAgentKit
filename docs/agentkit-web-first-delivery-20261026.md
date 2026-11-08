# SessionEndpoints + existing TekesWebUI delivery

[Epic #14](https://github.com/TekesApps/TekesAgentKit/issues/14) is the single live
delivery status entry, including current source/runtime pins, remaining failures
and the next batch. The entry versions below are historical inputs, not current
deployment or acceptance claims.

Current execution scope, 2026-10-26. Epic #14 remains the complete long-term
requirement; this phase supersedes the native-first ordering in the historical
2026-10-25 stage freeze. Owners: main chat coordinates integration and serialized
Cargo; independent workers own bounded source changes. #37 owns service/consumer
integration, #55/#56 actual protocol and performance, #58 review/integration,
#59 immutable headless/Web delivery. No implementation issue is closed by this
priority change.

## Per-endpoint delivery order (2026-10-27)

Deliver **Native → DSH → Pi**, each through SEP → shared presentation → public
Web service/contract → the existing TekesWebUI. Do not wait for unrelated
endpoint-specific failures before integrating an independently accepted endpoint.
All shared-code, contract, consumer, CI and applicable performance checks remain
required. Compiling all adapters is not evidence of accepting all three runtimes.

The current Native closure is unchanged between e9946e8 and product 375287c;
later product changes in that interval are Pi-only. Keep original Native evidence
identities and document this source relationship rather than relabeling programs.
Native Family's 21-case pass and earlier Native protocol/performance results are
reused only for their unchanged scope. Missing browser interactions, Native
browser-close continuation, restart boundaries and one existing real-model run
remain required, followed by exact delivery review, consumer, CI and release.

Browser-only detach during a running response must preserve the already received
streaming prefix in AgentKit shared runtime state, continue ingesting while there
is no observer, and restore the same live transcript on reattach. See
[observer lifetime](web-ui.md#observer-detach-during-an-active-run). This does not
extend the public Harness contract to make transient deltas durable across service
or source reconnects.

The existing host must allow explicit endpoint selection: Native-only startup
must not read Pi registry/runtime/provider credentials or DSH connection secrets,
and must not wait for disabled endpoints. Preserve the existing default and saved
configuration compatibility. This is host configuration, not a new business state
machine or a replacement launcher. Do not change provider settings or move data.
An enabled endpoint still has to satisfy its existing readiness and contracts.

Release notes must name the accepted endpoint scope and precise runtime/program
versions. Pi and DSH candidates and failures remain open until separately accepted;
merging shared code must not close their acceptance issues. Native TekesUI remains
paused and the complete Epic requirements are unchanged.

## Initial inputs and ownership (2026-10-26)

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

## Delivery gates and historical failures

The authorized Kernel queue (#86), read-only legacy recovery (#93), archive
notification (#95) and probe-wait (#97) fixes are released; the WebUI companion
candidate pins public Kernel 0.9.10. These are no longer open upstream blockers.
Further Kernel implementation remains outside this repository's scope unless
separately authorized. The daily installed version and exact acceptance identity
remain in Epic #14; a published fix does not establish deployment.

The original Native lifecycle, Pi warm/history and DSH warm/cleanup relative
performance failures remain valid at their original source/program/observer
identities. Later scoped conclusions and Pi reading optimizations are recorded
in Epic #14 and #55; no test or source change alone proves a budget resolved.
New evidence cannot erase old failures or borrow another program's passing result.
Native Family's completed 21-case cohort remains bound to its original programs.

TekesUI #53 and other purely native rendering/reading/IME/AX gaps do not block an
actually isolated headless/Web closure. They continue to block their native scope.
No native desktop, physical IME or AX acceptance in this phase. Native components,
fixed snapshots, programs and evidence remain preserved. Files/artifact preview,
terminal/browser panels and command palette stay enhancement work unless the
existing WebUI core workflow demonstrates a concrete blocker. Full Epic #14
requirements are unchanged.

## Execution model

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
