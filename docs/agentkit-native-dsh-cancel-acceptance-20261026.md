# Native and DSH Run cancel acceptance, 2026-10-26

## Frozen slice

This closes only the missing Native and DSH cancel rows in #55. It consumes the
existing offscreen public `SessionRuntime` + `RunController` binary; it adds no
product code, features, UI, Kernel changes, or dependency changes.

| Input | Frozen identity |
|---|---|
| Product source base | `5b7c84361206b45ac381b10437097d3b76c322c1` |
| Runner/tool source | `7d773ea0d89eb2bc5a4228df6f6409a05644ec3c` |
| Public headless consumer | `/Volumes/HP P900/dev/TekesAgentKit/acceptance-tools/run-lifecycle/current/run_lifecycle_acceptance` |
| Consumer SHA-256 | `84608c2ab363c436a36b6efc367bba12baa921089fdc9edd63260466b3196678` |
| Native Kernel | `0.9.3`, source revision `09508a9083c24c7accefc99516739f279b85913e`; journal `turn/end` contract from `crates/endpoint/src/projection.rs` at that revision |
| DSH | `@deepseek-ai/dsh 0.2.1-rc.2`, ACP SDK `1.4.0`; installed Node, CLI entry, DSH package metadata, and ACP contract hashes are captured in each receipt |
| Provider | per-run synthetic loopback OpenAI-compatible stream; no real credentials or remote provider |

Invoke one endpoint at a time so the stage coordinator can serialize runtime
work. Use only the already-installed paths and an absent output directory under
the existing SSD project:

```sh
mkdir -p /Volumes/HP\ P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261026

python3 -B scripts/test-real-native-dsh-run-cancel.py \
  --endpoint native \
  --program /Volumes/HP\ P900/dev/TekesAgentKit/acceptance-tools/run-lifecycle/current/run_lifecycle_acceptance \
  --kernel-supervisor /Users/frankqin/dev/Tekes/.build/kernel-0.9.3-installed/BuiltInKernel/tekes-supervisor \
  --kernel-worker /Users/frankqin/dev/Tekes/.build/kernel-0.9.3-installed/BuiltInKernel/tekes-worker \
  --providers-config /Users/frankqin/dev/TekesAgentKit/.runtime-fixtures/acceptance-refresh-20261025/kernel-0.9.3-contract-root/fixtures/config/providers.canonical.json \
  --kernel-revision 09508a9083c24c7accefc99516739f279b85913e \
  --output /Volumes/HP\ P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261026/native-cancel

python3 -B scripts/test-real-native-dsh-run-cancel.py \
  --endpoint dsh \
  --program /Volumes/HP\ P900/dev/TekesAgentKit/acceptance-tools/run-lifecycle/current/run_lifecycle_acceptance \
  --dsh-root /Volumes/HP\ P900/dev/TekesAgentKit/.runtime-fixtures/dsh-0.2.1-rc.2-sidebar-20261019 \
  --output /Volumes/HP\ P900/dev/TekesAgentKit/.runtime-fixtures/stage-close-20261026/dsh-cancel
```

The fixed consumer is the above product base plus the acceptance example
overlay; its own SHA and the example source SHA are recorded. The parent
acceptance process must compare both source identities and runtime identities
before invoking either command. The runner refuses a different tool HEAD or
consumer hash, Kernel build, DSH package version, or output location. It does
not build, install, use Cargo, fetch dependencies, or invoke desktop/IME/AX
automation.

## Required live sequence and evidence

The consumer creates or selects one isolated session, prepares one marked
prompt, dispatches it through the public controller, and retains a new Composer
draft. The loopback provider accepts exactly one original business request,
sends its first SSE frame, and holds the response. The runner waits for both
`run_cancellable` and the provider's actual held request, then writes exactly
`b"cancel\n"` to the consumer signal file. The consumer itself calls the public
`RunController.cancel` callback. An acknowledgement by itself is insufficient.

Acceptance requires all of these facts:

- the same `RunScope`, connection epoch, ticket, and Transcript turn advance
  from dispatched/running to aborted;
- the public Run becomes non-cancellable and the public session closes its run;
- the post-prepare draft remains byte-for-byte equal to the supplied draft;
- Native's exact journal `turn:<n>` has one `turn/start`, the original
  `user/message` with `source.rpcId` equal to the submission ticket and exact
  prompt bytes, and one later `turn/end` with `reason: "aborted"`. The pinned
  Kernel projection creates that reason from settlement `{outcome:"interrupted",
  reason:"user_stop"}`; historical NativeConversation run receipts are not used
  as proof for this Kernel endpoint;
- DSH has one original-ticket journal user message in the exact `turn:<n>`,
  followed by that same turn's `turn/end` terminal. Cancellation terminals are
  restricted to `aborted` or `cancelled`. This reflects the DSH ACP public
  contract: `session/cancel` is a notification and the original `session/prompt`
  completes with `StopReason::Cancelled` (`@agentclientprotocol/sdk` 1.4.0
  `dist/acp.d.ts`, `Agent.cancel`); an unrelated old abort cannot satisfy it;
- exactly one marked business request reached the synthetic provider and its
  held socket closed after cancel. DSH's explicitly recognized first-prompt
  title request is counted and reported separately. Any other request is an
  error, and no request body is persisted;
- consumer, provider handlers, and owned harness fixture have been stopped
  before `acceptance.json` is written. Runtime hashes and version/build
  identity are captured and rechecked.

Each run retains its source inputs, stdout/stderr, provider observations,
process cleanup facts, strict validator result, and `acceptance.json` under the
new evidence directory. Failed evidence remains on disk and has
`accepted: false`; do not retry into that directory or delete it.

Focused offline checks:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests \
  -p 'test_native_dsh_run_cancel_validation.py' -v
python3 -B scripts/test-real-native-dsh-run-cancel.py --help
```

These checks exercise validator fixtures and CLI/import setup only. They do not
claim real Native or DSH protocol acceptance. The stage coordinator runs the
two live cases serially after freezing their exact source/runtime inputs.

## Main-coordinated Native evidence

`native-cancel-20261026-v3` accepted the frozen product `5b7c843` with the
acceptance-only durable-input sampling fix, consumer SHA
`9a6a919215dc30c51df3b639188acb4783fd26f164592d15145a7f9e24734447`.
Kernel0.9.3/09508a9, original journal turn/start→user/message rpcId→turn/end
aborted, same public running→aborted turn, original draft and one held provider
request all agree; consumer and supervisor exit0. Failed v1 sampled an echo
before the durable user row; v2's validator wrongly required Native user/message
to duplicate data.turn. Fixed Kernel inherits that turn from ordered journal
boundaries. Both negative receipts and their exact source inputs are retained.
No stage feature expansion, desktop/IME/AX, or Kernel edit occurred.

`dsh-cancel-20261026-v1` also passed on the same consumer SHA: original DSH
journal turn/start→user/message rpcId→turn/end `{kind:aborted,reason:{kind:user}}`,
same running→aborted turn, draft preserved, one held business request plus one
separately classified title request, consumer/DSH exit0 and all owned handlers
retired. Pi's prior program SHA84608c2 is preserved as reference-only archive;
its old acceptance is not assigned to this new SHA9a6a919 program.
