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
The public TypeScript package `@tekesagentkit/annotations` exports generated
bindings, not a second implementation of the compiler. Regenerate with
`cargo run -p tekes-agent-contract --example export_annotations`; a contract test
checks the generated declarations. Public consumers must pin an immutable commit.

Old Web-only persisted input is decoded here as a compatibility read path. New
clients must not construct or parse annotation prompt envelopes themselves.
