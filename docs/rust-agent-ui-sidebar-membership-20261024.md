# Sidebar membership facts (2026-10-24)

Issue #29, linked to #22, addresses a confirmed Sidebar projection gap without
changing canonical inventory or endpoint protocols. ClientKit's main-thread
membership excludes a row when its normalized `origin` is `subagent` or its
`ephemeral` flag is true. A `parentSessionId` alone does not exclude a row:
forks can remain main conversations.

AgentKit applies one predicate to inventory row construction and bounded
selected-row reads: exclude only explicit `summary.origin == "subagent"` or
`summary.ephemeral == true`. Missing, null, differently typed, or other values
remain visible for compatibility and are not evidence that the row is a main
thread. The filter is a Sidebar presentation rule; canonical inventory and
history, Task, and SideChat sources remain intact. Complete `SessionAddress`
values keep same-ID sessions from different endpoints distinct.

The Pi endpoint's public normalized summary is produced in
`crates/endpoint-pi/src/lib.rs::summary`: it emits `origin` from the
subagent-parent fact and emits the row's `ephemeral` boolean. Its list path
uses that summary for each non-archived session. Native mux inventory and DSH
session/list classification coverage and missing-field semantics have not
been established by this change; no Native/DSH full-main parity is claimed.

The change adds a Rust unit predicate fixture for explicit main, subagent,
ephemeral, parent-only fork, and unclassified summaries, plus a demo integration
fixture for selected-read supplementation and identical session IDs on
separate endpoints. The implementation child ran rustfmt and `git diff --check`; the coordinating
chat then ran the complete UI workspace: 695 passed, zero failed, one existing
#49 ignore. Strict all-target UI Clippy, normal demo build, and the reused
standalone public component consumer also passed. The latter exercises unchanged
component public APIs; membership itself is proven by binding/unit and owned
fixture tests, including aggregate inventory and selected-read fencing.
These fixtures do not establish live endpoint behavior,
real harness compatibility, desktop/IME behavior, or Native/DSH classification
coverage.
