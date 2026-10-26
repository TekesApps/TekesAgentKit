# Native lifecycle holder diagnostics (#27)

## Question and evidence boundary

The fixed Kernel 0.9.3 lifecycle run previously returned `session-running` after settlement and rename. The ClientKit reference calls archive directly; AgentKit must not add an automatic stop, close, retry or private-store workaround to make that guard disappear.

Read-only review of the exact public `v0.9.3` (`09508a9083c24c7accefc99516739f279b85913e`) corrected an earlier hypothesis: the mux retains an endpoint subscription and journal-follow lease, not the `EndpointJournal` directory lock. That is not evidence of a self-held mux lock bug. Current Kernel WIP paths must not be cited as the fixed tag's implementation. The actual failed run lacks correlated holder sampling.

## Implementation slice

Add an opt-in diagnostic output argument to the existing real-endpoint script. It samples only the current script-owned isolated Kernel state root, around the unchanged native lifecycle batch. A standard-library helper records `lsof` file/lock observations, return codes, errors and monotonic/wall-clock timestamps in bounded JSONL, plus explicit phase marks. It reads no session/config file contents, changes no lock and terminates no process. Credentials remain confined to the script's existing stdin bootstrap; diagnostic argv contains paths only.

Sampling is observation, not a lock-acquisition test. A missing sample or holder is not evidence of lock availability. A `session-running` response remains a lifecycle failure, even if the diagnostic collection itself succeeds. Provider services are local controlled fixtures; no paid model or user HOME/history is used. The optional diagnostic path must not enable any UI flag, alter default assertions, create a new target, or run historical archived UI executables.

## Validation and next step

First verify the helper with mocked subprocess results (including empty/exit-1, timeout, failure, sample cap and shutdown), and run the existing script/provenance regressions. Before any isolated live run, verify the exact supervisor/worker build identity, current headless program, fixture provider configuration, owned processes/paths and output scope. Reuse the SSD shared target for any necessary current AgentKit client build. Kernel source and binaries remain read-only.

The follow-up may explain which process held which owned path during the failed archive. It cannot establish general Native lifecycle parity or justify a Kernel change without that evidence. Desktop, permissions, screenshots, IME/AX and UI acceptance remain unrun.

## Verified results

The final frozen script closure (10 local inputs) and current headless client were recorded before two fresh, isolated runs: one with sampling, one without. Both exited zero and passed the unchanged strict Native lifecycle batch, including scratch discard, rename, archive/unarchive; actual Task source-call, bounded child history, pagination/reconnect and wrong-parent rejection also passed. Exact supervisor/worker hashes match the earlier installed 0.9.3 evidence; the public-tag provider configuration hash also matches the historical fixture. These are current no-UI protocol/presentation results, not UI, three-harness or desktop acceptance.

The sampled run produced 11 observations, none truncated. `lsof` returned status 1 with file-reference output and no stderr; all observations are conservatively marked partial. It observed only the owned supervisor/worker beneath the owned root, but lock fields were blank. No lock mode, availability or causality was proved. The historical archive guard failure did not reproduce and remains preserved; these passing runs do not prove that earlier failure fixed or explain its timing. Two earlier exploratory runs are retained separately and are not substituted for the final frozen-source pair.

Python regression suite: 101 passed. This includes the sampler's mocked timeout/error/partial output, bounded limits, preservation of an observation already in flight at stop, refusal to overwrite prior evidence, and rejection of mixed/UI diagnostic profiles before starting a runtime. Runtime assertions were not relaxed; neither the ClientKit reference nor Kernel implementation was changed. No automatic retry/close/stop was added to archive.

The original fixture cleans up its own temporary runtime state. Postcheck found no owned Kernel processes; the pre-existing unrelated supervisor was still present and untouched. Current and previous shared-target headless executables, source snapshots, hashes and raw logs remain archived. No new target or additional build-cache deletion was performed.

[Machine receipt](rust-agent-ui-native-lifecycle-results-20261024.json). Local evidence: `.runtime-fixtures/native-lifecycle-diagnostics-20261024/` (SSD). Reproduction uses the `final-paired-execution.json` commands with a fresh, empty diagnostic directory; it must never run historical archived UI binaries or replace default lifecycle assertions.
