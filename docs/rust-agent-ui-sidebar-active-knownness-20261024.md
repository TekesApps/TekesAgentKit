# Sidebar active knownness and display membership (#22 / #29)

A provider result for a known active child/ephemeral session must not create a Sidebar row, but must not be reported as missing inventory either. The mixed public fixture-to-runtime-to-bindings-to-component regression first failed on this distinction: `unresolved` was recomputed from visible rows only.

The fix reuses `SidebarPublication.activity_at`, already documented as canonical activity before display transforms. The single binding now publishes every accepted active identity from the same validated inventory frame. The multi binding preserves each endpoint's full map, rejecting invalid endpoint partitions, empty identities and nonfinite times. Visible rows still require their original activity facts. Search uses visible rows plus the original active identities for knownness, while snippets, display rows, folder membership, pagination and reveal targets continue to use visible membership only.

This adds no controller, runtime read, classification inference, or new authority. A retained-name corpus includes archives and is deliberately not evidence of active inventory. Replacing an inventory replaces its activity facts; observer failure retains the coherent old packet and existing readiness rules. Legacy manually constructed publications without activity facts retain visible-row fallback. Multi projection also validates the fixed endpoint domain before partitioning facts.

The mixed regression covers explicit child and ephemeral exclusions, parent-only fork visibility, cross-endpoint equal session IDs, local/provider query union, folder catalog, bounded paging, selected-read exclusion, absent hidden-row reveal targets and preserved canonical history. It uses local fixture runtimes; it does not prove actual Native/Pi/DSH lifecycle behavior, bounded Native child authority, desktop reading anchors, physical IME or accessibility.

ClientKit remains the read-only contract reference. Its visible Sidebar predicate and durable tie order inform the shared Rust implementation; the Rust query-completeness mechanism is an implementation detail, not a claim of identical SQL lifecycle or full Unicode comparison semantics. See [identity ordering](rust-agent-ui-sidebar-identity-order-20261024.md) for the latter limit.

Final verification and commit receipt will be recorded in the issue backfill and the companion results file. TekesUI remains pinned to `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`; the strict nested-reading gate is unchanged.

Read-only reference checkout inspected at `c289bc9d33edcd83a782ac17927a8c65a027eccd`: `ClientSQLiteSessionStore.isMainThread`, `ClientSessionSidebar.sessionRows`, and `SessionEndpointStoreContract.SessionEndpointStorageIdentity.thread`. No upstream source was modified.

## Verified delivery

- UI workspace: 713 passed / 0 failed / 1 existing ignored (#49), 101 result groups.
- Presentation: 246 passed / 0 failed / 0 ignored.
- Focused bindings/demo: 50 passed / 0 failed / 0 ignored.
- Root and UI strict Clippy, formatting and ownership/dependency boundaries passed.
- Normal demo built without launching; independent public API consumer compiled and executed five scenario functions, including hidden-active knownness versus retained archive membership.
- Initial defect and the obsolete native-menu-order assertion both have retained failing logs; the final full UI rerun passed after correcting the implementation and the menu expectation.

[Machine receipt with log hashes](rust-agent-ui-sidebar-active-knownness-results-20261024.json). Logs and standalone consumer source/lock remain under `.runtime-fixtures/identity-membership-20261024/`, now on the SSD through the compatibility symlink. No new target was created, no dependency pin changed, no desktop/harness run was performed. The prior head CI's strict nested-reading failure remains a separate gate; new-head CI must be checked after push. Neither issue is closed by these fixture results.
