# Native inventory membership wire regression

This regression covers the public Native SessionEndpoint v4 inventory path only. The frozen
Kernel v0.9.3 contract says `parentSessionId` is present for a completed `session.fork` and
`origin` is then the literal `fork`; both are omitted otherwise. `ephemeral` is present as
`true` when the session genesis carries the ephemeral flag and is omitted otherwise. An
inventory consumer must preserve these facts. It must not infer `origin: subagent` from a
parent pointer.

The regression in `crates/endpoint-kernel/tests/mux.rs` serves a v4 inventory baseline over a
mock WebSocket, receives it through `KernelConnector`/`NativeSessionEndpoint`, and applies the
decoded frame to `MemoryProjectionStore`. It checks an ephemeral fork, a durable fork whose
`ephemeral` field is omitted, and an ordinary session whose parent/origin/ephemeral fields are
omitted. This exercises the adapter wire and in-memory projection boundary without changing
Kernel or reaching into Kernel storage.

The fixture follows the Native producer inventory shape: session identity and operational
summary fields, with fork ancestry and the optional ephemeral genesis fact. `origin: fork` is
retained as supplied; the test explicitly rejects manufacturing `origin: subagent`.

This is a synthetic protocol regression. It does not run a Native harness, prove live producer
integration, classify DSH or Pi rows, or provide desktop/UI acceptance evidence. Related
membership follow-up remains tracked in AgentKit issue #29.

The coordinating chat verified the local annotated v0.9.3 tag peels to
`09508a9083c24c7accefc99516739f279b85913e` and inspected that exact commit
with `git show` (not the working tree): `spec/session-endpoint.md:1924-1945`
and `crates/supervisor/src/endpoint_carrier.rs:2620-2643`. The GitHub repository is public, and its annotated v0.9.3 tag API also resolves
to that same commit. Live installed-runtime identity remains a separate check.

Coordinating verification: the complete Native mux suite passed 6/0/0;
root-workspace all-target strict Clippy, formatting and dependency boundaries passed.
The fixed public tag and producer are source evidence; the new test is synthetic wire
and memory-projection evidence. No Native program or native window was launched.
