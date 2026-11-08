# Web task and child views

Implementation issue: #74 under Epic #14, associated with #23 and #37.

The existing Web HTTP host translates calls to the private AgentKit JSON-lines
service. The browser consumes these AgentKit operations; it does not interpret
tool arguments, infer child identity from text, or keep a second task state
machine.

`call("agent.familySnapshot", {sessionId}, rpcId)` returns shared
`SummaryFacts` for that exact endpoint/session and, when the endpoint advertises
`subagents.list`, a fresh `SubagentCatalog`. The response includes the runtime
connection epoch. Summary facts come from the normalized projection and its
transcript; child status and navigable session IDs appear only when the existing
presentation code validates them against that endpoint's retained store.

`call("agent.child.open", {parentSessionId, childId}, rpcId)` requires a fresh
catalog entry under that exact parent. Every open creates its own opaque `handle`,
`ChildRun`, source-bound pagination state and history lease. The server selects an
independent child session when its summary or initialized journal is present in
that endpoint's store; control-only queue/actionable rows do not establish history; otherwise it uses the endpoint's advertised bounded `subagents.history`
source. A child ID alone is never an endpoint or parent fallback.

Subscribe with follow target
`{kind:"childView", handle, parentSessionId, childId}`. The initial frame and
updates are `{handle, endpointId, parentSessionId, childId, connectionEpoch,
transcript, run}`. Both projections come from AgentKit; the browser must not
fabricate an idle phase, connection, queue or actionables. Bounded read-only
history disables submit/cancel without changing the public catalog capability.
A handle has at most one active observer. Two panes or browser
consumers must call `agent.child.open` separately and get independent history
state. When follow ends for any reason, an RAII guard removes that handle and
releases its history lease. Unsubscribed handles expire after five minutes and
the service bounds unobserved handles to 32. A pane must reopen after its handle
expires or its endpoint epoch changes; the new open reacquires public history.

`agent.child.earlier`, `agent.child.prompt`, `agent.child.cancel`, and
`agent.child.close` all require `{handle, parentSessionId, childId}`. Every action
checks the bound endpoint, exact parent/child pair and connection epoch. Child
control re-reads the public catalog; only a continuable child with an available
parent can receive a prompt or cancel. One-shot and parent-unavailable children
may be readable where the source allows, but remain read-only. Harnesses that
reject child files retain that explicit limitation. The caller retains its
`rpcId` on uncertain outcomes and does not retry automatically.

Closing a browser never stops the parent or child Harness process. A host restart
drops the in-memory child view handle and pagination state; reopening reacquires
it from the Harness public catalog/history. This is not a claim that each Harness
continues active work across every service restart.

Artifact/file preview is outside this contract: the service has no browser file
read route in this implementation.

### Observation closure and admission

A retired handle revokes its owner token before removal. Prompt/cancel validate
that token after capability/catalog reads and channel backpressure at local
admission; cancel additionally retains the connection epoch fence at actor
dispatch. Closing an observation releases its lease and rejects future admissions,
but never cancels an already admitted mutation. Its original RPC identity remains
valid for outcome reconciliation; callers must not automatically retry.
