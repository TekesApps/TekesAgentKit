# DSH session membership wire regression

This regression pins the DSH `0.2.1-rc.2` public producer contract to the
AgentKit DSH adapter's inventory frames. The official Git tree and source blobs were retrieved for
commit `687f282e8625c6b8d11ff58211a90524be2e4317`; Git blob SHA-1 was verified before archiving. The local source
is under `.runtime-fixtures/sidebar-host-20261024/dsh-pinned-public-source/`.

In that source, `packages/api/session-controller/src/types.ts` defines
`SessionSummary` with optional `origin: 'subagent'` and `parentSessionId`, and
does not define `ephemeral`. `packages/api/session-controller/src/list.ts`
includes `listFields(header)` in both `summaryFor` and `summarizeCold`; that
helper forwards `header.origin`, `header.parentSession`, and `header.cwd` when
present. The same package declares `api-session/added(summary)` as carrying a
current `SessionSummary`.

The Rust regression exercises the adapter's actual local HTTP plus Remote
WebSocket path. An RPC `session/list` response must arrive unchanged inside the
`SessionInventory` baseline, and an `api-session/added` event must arrive
unchanged as the `upsert.session` value. The fixture covers an explicitly
classified subagent, a parent-only fork, and an ordinary row without origin;
missing `origin` stays missing and is never inferred from `parentSessionId`.
It also asserts that the adapter does not synthesize an `ephemeral` field.

This test verifies serialization and routing through the Rust adapter only. It
does not run a DSH harness or prove runtime inventory completeness. The pinned
public summary contract has no `ephemeral` field, so this evidence makes no
claim that DSH reports ephemeral membership. DSH's `header.origin` is
presentation metadata; it is preserved as provided and is not evidence that a
session can be continued.


The coordinating chat also verified the supported `dsh-v0.2.0-rc.2` tag at
`639ed015397290b3745d163aafe02ffee4aa3f84`. Its `list.ts:107-156,331-339`
forwards the same classification in live and cold rows; `types.ts:177-188`
has the same origin/parent vocabulary, without the newer formatStatus field.
The regression includes that older actual summary shape, not a second mock
version label. Runtime version admission is covered by existing separate tests;
this new regression does not launch either released DSH program.

Coordinating verification: the complete DSH crate suite passed 14/0/0;
root-workspace all-target strict Clippy, formatting and dependency boundaries passed.
The test also preserves the documented sessionListMetadata projection, without
inventing title projection fields. No DSH program, model provider or native window
was launched. No adapter production behavior was changed in this batch.
