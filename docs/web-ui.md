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

## Durable thread preferences (v0.3.3)

The embedding host supplies `threadPreferences: {path, scope}` in the private
JSON-lines startup configuration. `path` must be an absolute private file path;
`scope` is a stable host-selected account/workspace identity. Do not use a
per-launch ID. The local single-user Tekes host uses its canonical workspace
path inside its private runtime directory. Multi-user hosts must supply an
authenticated account identity as part of scope, not browser input.

AgentKit's public `ThreadPreferences` owns pin/unpin and global pin order across
endpoint/session identities. It writes a versioned file by atomic replacement
with mode 0600, serializes mutations under a lock, and refuses a second service
writer. Bad/unknown-version data fails startup without erasing preferences.
The store contains client preferences and RPC receipts only; it never mirrors
Harness transcripts or credentials. Receipts remain durable so an explicit
retry of a move reconciles without moving twice, including after restart.

`thread.actions` and `thread.performAction` are unchanged. State inventories
include optional `pin: ThreadPinPresentation` (`order`, zero-based) on each
active pinned session. The state stream wakes on preference changes so every
browser observes the same order without polling or local pin storage. Missing
or archived rows retain their saved identity; they are omitted by active
inventory and recover their saved position if they return. No configuration
means the consumer has not installed persistence and pin remains unavailable.

Run `cargo run -p tekes-agent-presentation --example thread_preferences` for
a second public-API consumer demonstrating durable storage without Web.
Mac ClientKit remains independent; no native state migration is included.
