# Shared AgentKit Web service

`tekes-agent-web` is a reusable headless private JSON-lines service. It owns one
`SessionRuntime` per registered endpoint, and derives the browser/App transcript from
`tekes-agent-presentation`. Kernel, Pi and DSH remain separate Harnesses, with their
original endpoint/session IDs and public protocol adapters. It has no session database.

The embedding host owns authentication, listener, process supervision, settings,
credentials, execution roots and installed binaries. The binary accepts a configuration
line on stdin followed by multiplexed requests. It emits `{id,value}`, `{id,error}` and
`{id,done:true}` lines. It must never be exposed directly as a public unauthenticated pipe.
There is no listener, credential store or Kernel UI in this crate.

Operations: `endpoints`, `describe`, `snapshot`, `call` (requires caller-supplied `rpcId`),
`history`, `respond`, `earlier`, `follow`, `cancelObservation`. A `follow` targets `state`
(inventory/health), `view` (session/run/transcript), or a normalized `StreamTarget` object.
A host can refresh DSH authorization with the private `configureDSH` operation; its HTTP
wrapper must restrict this to its native-host authorization, never browser credentials.

Each view has its own history lease. Dropping an observation, changing selection, or
closing a browser releases only that observation. Runtime/Harness ownership stays in
the host until an explicit host stop. Requests are never automatically replayed after
uncertain mutations. Slow observers are disconnected and must request fresh baselines.

The browser application is the independent MIT TypeScript project
[TekesWebUI](https://github.com/TekesApps/TekesWebUI). No HTML, CSS, browser code or
browser tests live in this crate. Its historical binary name is retained for host
compatibility. Native Swift rendering lives in ClientKit's `AgentKitRemoteEndpoint`;
the Tekes App contributes host discovery and its Keychain authorization callback.

## Annotation interface

`call` method `composer.compileAnnotations` accepts the public `AnnotationInput`
DTO (`text`, `annotations`) and returns `AnnotationCompilation` (`text`). It is
pure compilation in AgentKit, never a Harness RPC or a model run. Connected
endpoint descriptions advertise the method in `capabilities.methods`.

Consumers pass the opaque compiled text to ordinary `session.prompt` alongside
their attachment receipts. Submission identity, acknowledgement and execution
continue to use the existing public session API. `follow(view)` user items expose
`inputPresentation: AnnotationInputPresentation` with authored text and annotations;
the original durable text remains in `item.text`. Assistant/model-produced text
is never interpreted as a user annotation envelope.

`ComposerAnnotation`, `TranscriptAnnotationSource`, `AnnotationInput`,
`AnnotationCompilation`, and `AnnotationInputPresentation` live in
`tekes-agent-contract`. Rust's `ComposerDraft` reuses this type and compiler.
The public TypeScript package `@tekesagentkit/client-contract` exports generated
bindings, not a second implementation of the compiler. Regenerate with
`cargo run -p tekes-agent-contract --example export_annotations`; a contract test
checks the generated declarations. Public consumers must pin an immutable commit.

Old Web-only persisted input is decoded here as a compatibility read path. New
clients must not construct or parse annotation prompt envelopes themselves.

## Thread action definitions

`call(thread.actions, {sessionId})` returns `ThreadActionsPresentation`, scoped to
an exact endpoint/session. The shared presentation module owns item IDs, labels,
ordering, semantic icon roles, interactions and disabled reasons. The baseline is
ClientKit's Mac sidebar: pin/unpin, optional rename, fork, copy session ID,
conditional pin-order actions, then archive or supported unarchive.

UI consumers render the definition. `interaction: rename` requests authored text;
`copy` carries an authoritative `copyText` for a user-initiated clipboard gesture;
`execute` invokes `call(thread.performAction, ThreadActionRequest)`. The command
is resolved against a **fresh** definition; arbitrary Harness method names cannot
be dispatched through this API. The caller retains its RPC identity after an
uncertain result and never retries automatically. Outcomes carry navigation,
selection clearing, rename receipts and optional partial-success warnings.

`ThreadActionHost` explicitly supplies shared pin state, ordering capability and
mutation callbacks. Without it pin is visible but disabled; clients must not
substitute local storage. The embedding host also supplies `sessionURLScheme`
(default `agent`). Tekes supplies `tekes`; DSH prefix stripping and Pi backend
identity match the Mac copy-session-ID behavior.

The generated public TypeScript package is now `@tekesagentkit/client-contract`;
it includes both annotations and thread-action DTOs. Existing immutable annotation
package pins remain valid at their original commits. Rust consumers use the same
`thread_actions` policy directly; see the presentation crate's `thread_actions`
example. The existing Swift sidebar still has its original native integration;
this service change does not claim that its entry point has been migrated.

### Transcript footnotes

Each turn may expose `footnoteActions` using the public `ThreadActionDefinition`
contract. A consumer renders the supplied labels/icons/availability and dispatches
its exact action ID through `thread.performAction`. Completed-turn Fork is distinct
from the sidebar's whole-session Fork: AgentKit resolves its identity to the
selected durable settlement boundary, never to the latest visible message or an
estimated sequence. Canonical journals use `turn/end` sequence; native histories
use the settled run marker's entry ID. Missing boundaries, stale journals, active
work and unavailable capabilities prevent execution. Current authority is rechecked
at click time. The existing outcome preserves navigation even if optional rename
or queue cleanup reports a warning; mutations are never automatically retried.

## Task and child presentation

The service exposes shared SummaryFacts and parent-owned child controls through
`agent.familySnapshot` and the `familyView` observation. Child content opens with
an opaque, per-observer handle; `childView` holds an independent history lease.
Read the [family contract](../../docs/web-family-contract.md) before consuming
these methods. Child IDs never become ordinary session fallbacks; one-shot,
parent-unavailable and bounded read-only sources retain their exact capabilities.

The public TypeScript contract also exports permission DTOs from Rust. The host
and browser render each Harness's actual catalog and confirmed effective mode;
values are not browser-enforced sandbox policies. A release must replace local
development package overrides with an immutable public commit and verify the
installed declaration bytes against that pin.
