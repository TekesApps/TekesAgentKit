# Performance measurement preflight — not acceptance

Owners #55/#56. No performance sample has been collected. The original frozen
profile and budgets remain unchanged; these are source-audit findings to resolve
before instrumented runs, not permission to weaken the workload.

Both f1daa0f and the candidate have public SessionRuntime/RunController send,
open, history, reconnect and bounded child-history APIs. Basic family capability
therefore has a comparable baseline; only a demonstrated new API/behavior may be
reported absolute-only. Prepared CLI binaries are not performance results.

The original source audit identified two ambiguities (resolved by the execution recipe below):

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


## Before-sample correction: steady warm and lifecycle are distinct

No samples were collected under the earlier combined recipe. Pi reconnect
closes its session RPC children by contract; same consumer does not prove same
harness PID. Keep genuine steady-warm10 after excluded initialization, and add
a separate reconnect-lifecycle10 with its own identical initial dataset/growth.
Cold5 and every absolute/relative latency/RSS/cleanup budget remain unchanged.
The impossible same-Pi-PID reconnect assumption is explicitly corrected, never
called passed or Unsupported. Lifecycle time includes actual RPC close/probe/
restart and pointer/history convergence.

Public close/open retains old journal records, so a mutable Runtime window
cannot serve ten nonempty page samples after exhausting a finite seed. Each
page20 probe must instead construct a fresh public in-memory projection from
an actual follow20 bootstrap and consume the actual next page20. This is neither
private cache trimming nor a page50 slice. Pi's follow window is shared within
its endpoint; public history close/open50 restores the Runtime view without
closing the cached RPC. PID/start identity must be observed, not assumed. The
JSON records the three finite trajectories and actual window/cache limitations;
consumer implementation must match them before any official sample is taken.

## Diagnostic tooling delivered; official samples remain zero

The same public-API example source SHA119a2744d73452f2e133bfaebd3a9bdb7ac7c07db3f62f28b9af1fb2784124e0
compiled and passed strict Clippy over baseline f1daa0f and product4274, in separate
immutable source inputs using the existing SSD shared target serially. Fixed-role
programs and prior executables remain outside Cargo target with exact receipts.
No primary-root WIP was used. Python426 checks passed with bundled Python3.12.14.

The [diagnostic binding](evidence-bindings-20261026/performance-preflight.json)
records one Native candidate cold plumbing case: two publicly seeded sessions,
each100 exact1024-byte user/final-assistant bodies and10 actual read groups; all
source anchors/call/result identities verified; seeder/harness retired and owned
state sealed without private DB parsing; fresh harness PID restored the same
dataset. Fresh SEP page20 returned20 append messages plus60 journal records per
session, original pointer/history reconnect converged, both sessions sent and
settled, and the separate RunController50 route added50 messages per session.
Original process groups exited and provider reported no rejected requests.

Native0.9.3's published `crates/endpoint/src/history.rs` at09508a9 counts
user/assistant append messages, retaining control/tool/source-group records.
The diagnostic's earlier assumption of20/50 journal records was incorrect; the
consumer now checks the requested message count and all exact source records.
No request size, threshold, iteration count or product code was reduced. Other
pilot errors concerned server-chain fixture context and public structured read
output/empty tool-step carriers; their failed receipts remain unchanged. An
independent source review confirmed the100-body/10-tool workload was retained.

This tool is deliberately diagnostic-only. Official paired cold5, steadywarm10,
reconnectLifecycle10, Pi/DSH diagnostic execution, observed same-PID warm proof,
family/attachment performance, percentile/RSS/regression and cleanup budget
acceptance remain not_run. There is no aggregate performance pass, no completed
#55/#56, and no basis for merge/tag. The runner rejects missing/duplicate events
and verifies program/source/runtime identities before and after its owned case.
