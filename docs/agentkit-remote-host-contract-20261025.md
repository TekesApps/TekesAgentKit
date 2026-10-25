# AgentKitRemote / WebUI host contract (design for issue #37)

Status: implementation WIP, based on a read-only cross-repository audit on 2026-10-25. This document records current committed host code and separate working-tree candidates. It does not declare the uncommitted AgentKitRemote API published or a complete cross-host acceptance. The ClientKit adapter and Tekes descriptor reader are uncommitted WIP; the published ClientKit reference is `c289bc9d33edcd83a782ac17927a8c65a027eccd`. TekesWebUI's current checked-in host is `2e61ba2`; the HTTP/bridge implementation dates from `64a71dd`, while its runtime has additional WIP.

## Ownership and observed topology

| Concern | Owner / current evidence |
| --- | --- |
| Agent runtime, SessionEndpoint adapters, normalized presentation, operation semantics | AgentKit Rust (`tekes-agent-web`, `SharedAgent`, `SessionRuntime`) |
| Swift `AgentKitRemoteEndpoint`, HTTP decoding, runtime provider and endpoint integration | ClientKit; observed only in its uncommitted `Sources/Session/SessionEndpoint/AgentKit/` WIP |
| Listener binding, loopback HTTP routes, Bearer validation and NDJSON response streaming | TekesWebUI host (`host/http.ts`); committed host bridge code with current runtime WIP |
| Process supervision, descriptor creation/rotation, browser-origin policy and deployment | Embedding host/product; not owned by Rust `crates/web` |
| Swift `AgentKitRemoteEndpoint`, descriptor discovery and DSH Keychain refresh | ClientKit adapter plus Tekes product host; inspected in uncommitted WIP |

The term “remote” in the current ClientKit candidate does not mean arbitrary remote networking. Its initializer and refreshed connection both accept only plain HTTP at `127.0.0.1`, reject URL credentials/query/fragment and redirects, and use a bearer header. A future non-loopback or TLS transport would need a separate explicit security and credential contract; it must not be inferred from this adapter.

The intended topology is already present in two layers: ClientKit Remote calls the authenticated TekesWebUI loopback HTTP host; its host bridge multiplexes requests over the private AgentKit JSON-lines child process and maps stream output to NDJSON. The Rust `tekes-agent-web` binary reads one configuration line from stdin followed by multiplexed requests, and emits `{id,value}`, `{id,error}`, and `{id,done:true}` lines. Transport `id` is distinct from mutation `rpcId`. `crates/web` is not an HTTP listener and does not own Bearer credentials, descriptor rotation, DSH Keychain access or process lifetime. Static source confirms each layer exists; it does not yet prove that the current WebUI runtime and ClientKit candidate interoperate end to end against the current AgentKit process. Do not create a second listener in Rust.

## ClientKit WIP wire mapping

Every HTTP request is a JSON `POST` to `<connection.url>/api`, with `Authorization: Bearer <credential>` and JSON content type. `endpoint` is injected from the stable `SessionEndpointIdentity.id` into each request. The ClientKit HTTP envelope is separate from the child-process JSONL envelope; TekesWebUI maps between them. Streaming is newline-delimited JSON over the response body, capped at 16 MiB per line; it is neither SSE nor WebSocket. Empty lines are ignored and cancellation of the consuming stream cancels its URLSession task.

| ClientKit operation | Wire fields / Rust service correspondence |
| --- | --- |
| `describeHost()` | `{op:"describe", endpoint}`. ClientKit polls up to 30 times at 500 ms while host/connection is not ready; once ready it refreshes advertised methods, archive/unarchive, edit-history and completed-turn-fork capabilities. Rust `SharedAgent::request(describe)` returns endpoint identity, connection, host and endpoint capabilities. |
| `call(method,payload,rpcID)` | `{op:"call",endpoint,method,payload,rpcId}` maps to `SharedAgent::request(call)`. Mutating calls must preserve the caller's `rpcId` after uncertain transport results; neither layer automatically replays them. |
| `historyPage` / `respondToActionable` | `{op:"history",payload}` and `{op:"respond",actionableId,expectedRevision,outcome}`. The Rust service delegates to the exact endpoint/runtime and preserves stale revision checks. |
| `follow` | `{op:"follow",target:{kind}}`; kinds include workspace inventory, session inventory/control, actionables and journal. The WebUI bridge forwards one host-generated transport `id` through JSONL; Rust leases observations and streams frames. |
| `followJournal` | Follow target `session-journal` with session address and bounded `maxMessages`; stable session identity remains the original endpoint/session, not a display row. |
| projected transcript | `{op:"follow",target:"view",sessionId}`. Rust `SharedAgent::follow(view)` publishes normalized `run` and `transcript`; it holds a per-view history lease. The ClientKit adapter maps this to projected frames and maintains its own observation generation. |
| `loadEarlierProjectedStream` | `{op:"earlier",sessionId}` returns `{loaded}`; pagination remains an explicit call and retains its frozen-cut/source semantics. |
| cancel observation | HTTP disconnect cancels the matching host bridge observation, which sends `cancelObservation` with the same child-process transport ID. Rust only aborts IDs registered as observations; cancelling an ID must not abort a unary operation. |
| endpoint lifecycle | ClientKit `close()` cancels its observations and sends no service-stop command. Rust view lease release never stops the runtime or Harness. Only the embedding host owns process/runtime stop. |

`composer.compileAnnotations` is a pure AgentKit-side call and must not be forwarded to a Harness. Mutations such as prompt, cancel, queue updates and thread actions use the existing public call contract and stable RPC identity. A lost response is **unknown outcome**, not evidence of failure: present reconciliation/retry affordances from the established state and do not automatically submit again.

## Connection descriptor and security boundaries

The ClientKit/Tekes candidate reads `TEKES_AGENTKIT_CONNECTION`, or defaults to `~/.agents/client/agent-web/app-connection.json`. An explicitly selected missing file reports the host unavailable; a missing default file means no provider is installed. The descriptor currently has `format:1`, URL, bearer and endpoint records (stable ID, kind, display name, optional capabilities), a 64 KiB limit, regular-file/current-UID/private-mode checks, and a host-supplied refresh callback. This is uncommitted candidate behavior, not a published descriptor contract.

The Tekes host alone reads the DSH credential from its Keychain and sends it to the configured local daemon's private `host.attachDSH` operation. The daemon uses that credential only to configure the DSH harness connector. Browser credentials must never reach this operation. The host descriptor bearer and DSH cookie have distinct audiences and authorities. Never log either secret. The service process protocol must remain private to its authenticated embedding host.

The current ClientKit loopback HTTP client rejects redirects and disables cookies/proxies; the WebUI host binds its server to `127.0.0.1` and validates bearer requests. These are separate client/host controls. Descriptor ACLs, secret rotation, origin/CSRF policy and process sandboxing belong to the embedding host. Their complete deployment and adversarial security behavior remains unverified; `crates/web` must not log credentials or become a public listener.

## Capability, readiness, errors and reconnection

Endpoint identity and Harness kind survive the adapter: the endpoint's `kind` determines Tekes/DSH/Pi host presentation, while methods and policy booleans determine which actions are enabled. Advertised capabilities are not grants inferred from labels; host UI reads a fresh description before enabling an operation, and the underlying AgentKit endpoint rechecks authority when executing it. Missing endpoint/capability data stays unavailable/disabled. A connection in `connecting`, `failed` or `stopped` is not equivalent to `ready`; retained history may remain readable while new mutations are disabled.

Rust errors include protocol, transport, remote, timeout, disconnected and unsupported variants, while its current JSONL error envelope contains only a string. The WebUI bridge maps that string into the HTTP response; the current ClientKit decoder also reduces remote failures to strings and does not expose stable retryability or request phase. A future versioned error DTO must preserve compatibility and distinguish known rejection from uncertain mutation outcome without leaking credentials/provider details. Until both consumers understand it, keep existing string errors and original mutation `rpcId`; never infer retry safety from the transport `id`.

Stream EOF semantics also remain an explicit gap. The WebUI host currently ends successful long-lived follow responses with EOF. The current ClientKit candidate treats the child/HTTP `done:true` response as disconnected too, rather than a distinct normal completion. Do not describe `done` as a successful normal-stream terminal until both sides agree and tests prove it.

On transport loss, a stream consumer reacquires its observation and receives a fresh baseline/generation; it must not synthesize missing events or merge frames across generations. Selection, endpoint/session identity, history cursor, and request ID remain source-bound. Close/unmount releases observer/history lease only. No automatic mutation retry occurs on reconnection.

## Full Issue #37 delivery and acceptance plan

1. Freeze the two existing layers separately: AgentKit child JSONL DTOs and the WebUI HTTP/Bearer/NDJSON contract. Record the exact committed revisions and current WIP identities before changing either.
2. Publish Rust-side JSONL request/response DTOs and compatibility fixtures while preserving the config first line, operation names, legacy envelopes, endpoint IDs, Harness adapters, host ownership and distinct transport/mutation IDs.
3. Verify response ordering, bounded stream backpressure, concurrent observations, cancellation isolation, malformed/duplicate IDs, unary uncertain outcomes, history cut/page continuity, readiness/capability handling and DSH credential audience with fake endpoints.
4. Use the actual TekesWebUI host bridge/HTTP code as the second consumer against the current AgentKit child process and fake SEP. Cover bearer auth, NDJSON, reconnect baseline, stale revisions, cancellation of A without disrupting B, response loss after mutation admission and descriptor refresh. Existing WebUI tests using FakeBridge/fixture processes are not equivalent to this end-to-end receipt.
5. Keep structured error adoption, if needed, coordinated across Rust DTO, WebUI bridge, ClientKit decoder and independent consumer; preserve legacy strings during migration.
6. Record native window, browser-origin deployment, keychain, accessibility/IME and performance evidence only in their owning issues. This design has not run service, protocol, browser, native or security acceptance; it is not a passing test receipt.

## Current executable gaps

- Rust JSONL DTOs were not previously public or independently tested; the new issue #37 candidate addresses this private process boundary without changing the HTTP host.
- No current evidence connects the exact WebUI runtime WIP, ClientKit descriptor/decoder WIP and AgentKit binary in one real loopback test. Existing WebUI HTTP tests use FakeBridge; its pipe smoke test repeats empty endpoints.
- The Rust stream dispatcher must prove it drains accepted frames before terminal error/done and isolates observation cancellation from unary operations.
- HTTP error mapping still loses structured failure category and uncertain mutation phase; descriptor rotation/ACL security and browser-origin deployment need their own host evidence.
- EOF versus `done:true` normal-completion semantics are not aligned.
- Cross-process remote deployment, performance, native window, accessibility and IME acceptance remain separate scopes.

The current Rust changes are only a source candidate until main runs package tests and a real host-bridge/fake-SEP loopback acceptance. No HTTP server or external host source was changed in this work.
