# Performance measurement preflight — not acceptance

Owners #55/#56. No performance sample has been collected. The original frozen
profile and budgets remain unchanged; these are source-audit findings to resolve
before instrumented runs, not permission to weaken the workload.

Both f1daa0f and the candidate have public SessionRuntime/RunController send,
open, history, reconnect and bounded child-history APIs. Basic family capability
therefore has a comparable baseline; only a demonstrated new API/behavior may be
reported absolute-only. Prepared CLI binaries are not performance results.

Two preflight ambiguities remain:

1. Both public RunController/SessionRuntime history paths request 50 messages;
   the frozen measurement profile requests 20. Public Endpoint follow/history
   supports real page20 and existing memory-projection ingestion, so that route
   may measure SEP→projection page20. It must not be called RunController page20,
   or implemented by slicing a page50 response. The actual RunController route
   retains its own required behavior/evidence.
2. Before dataset production, distinguish 100 body-text messages (not 100 turn
   pairs) and the additional 10 tool call/result groups. Warm and cold send
   mutations must have identical preregistered datasets on both revisions;
   readonly history/reconnect data cannot silently drift as sends append rows.
   A concrete public-API reset/fork/retirement or deterministic growth plan is
   still required; no private history/DB rewriting is allowed.

Measurement must start at the original public intent call with subscriptions
already active and stop at its matching server-source publication, excluding
optimistic echoes. Reconnect requires same-pointer inventory/history convergence,
not transport-ready alone. Record all owned harness processes at 50ms intervals,
including workers/children and missed-short-process sampling limits; predefine
whether budgets use simultaneous tree totals. Keep cold5 and warm10 separate,
nearest-rank p95, all failures and full runtime/machine/power identities. No
post-result budget adjustment or absent-baseline claim for supported old APIs.

This plan is source-level only. No benchmark, latency/RSS pass, desktop/IME/AX
acceptance or completed #55/#56 is implied. The #67 blocking fix also needs its
own immutable candidate identity before final measurements.

## Execution semantics resolved before samples

The profile now explicitly treats100 as the initial body-message seed, not an
impossible public-history truncation after every warm send. Each of two ordinary
sessions starts with50 user/final-assistant pairs at1024 UTF-8 bytes each, plus10
actual read-tool groups. Cold5 starts fresh owned seeded datasets; warm excludes
one initialization iteration and then records initial body counts102,104,…120,
adding exactly one user/final assistant pair per session per iteration. Both
revisions use the identical trajectory; no hidden fork/archive inventory is used.
Family remains a separately disclosed one-parent/one-child topology and still has
a comparable baseline. Original thresholds, counts and failure retention remain.

The JSON `executionRecipe` freezes public seed readback, true SEP page20 versus
actual RunController page50, original-source assistant-first-frame timing, same
pointer/history reconnect convergence, simultaneous owned-tree RSS, and pairing
order. This is preregistration, not measurement or a narrowed acceptance claim.
No performance sample exists yet. Lack of a public cross-harness history truncate
API must not lead to private DB rewrites or silently incomparable warm datasets.
