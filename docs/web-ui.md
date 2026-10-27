# Browser UI ownership

Browser UI has moved out of AgentKit to the standalone MIT React + TypeScript
project [TekesWebUI](https://github.com/TekesApps/TekesWebUI).

AgentKit retains the headless `crates/web` JSON-lines bridge, normalized SessionRuntime
and Harness adapters. Its binary name remains `tekes-agent-web` for compatibility.
Hosts own authentication, listener, credentials, permissions and process lifetime.
Swift reusable Agent views remain in ClientKit.

The shared annotation DTOs and TypeScript bindings are exported by the public
`@tekesagentkit/client-contract` package. `composer.compileAnnotations` compiles input
inside AgentKit; `follow(view)` projects `inputPresentation` for history rendering.
See [the service contract](../crates/web/README.md#annotation-interface).

## Observer detach during an active run

A browser closing releases its history observer; it does not cancel execution.
When the last observer leaves a ready journal whose shared projection reports an
active run, SessionRuntime retains that journal and continues folding live events.
A new observer reuses it, including text already streamed before the browser closed.
The journal's authoritative terminal state releases this temporary retention when
there are no observers; an attached observer keeps its normal lease. This lifetime
is serialized with frame processing and observer Open/Close in AgentKit, without
WebUI transcript caches or a second business state machine.

This is in-memory continuity within the same service and live Harness connection,
not durable storage of transient deltas. After service restart or a genuine source
reconnect, recovery uses the Harness's public history and running/outcome facts.
Uncommitted content absent from that history cannot be reconstructed or silently
resent. Existing history, provider configuration and user data are not rewritten.

## Durable pin relationships and thread activity (v0.3.4)

The embedding host supplies `threadPreferences: {path, scope}` in the private
JSON-lines startup configuration. `path` must be an absolute private file path;
`scope` is a stable host-selected account/workspace identity. Do not use a
per-launch ID. Multi-user hosts must include the authenticated account identity,
not a value accepted from browser input.

AgentKit persists only pin relationships keyed by endpoint/session identity and
RPC receipts for pin/unpin. The store is a set: serialization order has no display
meaning. Version 1 files migrate to version 2 while retaining every pin, dropping
legacy display order and move receipts. Pin/unpin remain idempotent across
restart. `movePinUp`/`movePinDown` are no longer service actions. Consumers own
sorting, grouping, labels and all associated preferences; none enter this store.

State inventories include optional `pin: ThreadPinPresentation` (`pinned: true`)
on pinned sessions and `activity: ThreadActivity` on every active session. Activity
contains `status` and `isStale`, computed from existing normalized inventory,
control/actionable and retained journal facts. It never opens all histories just
to infer completion. Idle is distinct from completed; unavailable facts remain
unknown. Explicit settled reasons preserve completed/failed/cancelled semantics.
Disconnected or incomplete source planes mark the facts stale. No status grouping
names, group order or view preferences are defined by AgentKit.

The state stream broadcasts pin changes and runtime updates to all consumers.
Missing or archived rows retain their pin relationship. File writes use atomic
replacement and mode 0600, with one writer lock. Malformed or unknown versions
fail explicitly without erasing the file. No transcript or credentials are stored.

Run `cargo run -p tekes-agent-presentation --example thread_preferences` for
a second public consumer of persistence. Mac ClientKit is unchanged.
