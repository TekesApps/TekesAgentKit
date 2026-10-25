# Pi uncertain-prompt transport fixture (2026-10-25)

This script is a transport fault fixture for the existing #55 Pi submission contract. It does not implement an AgentKit state machine and does not by itself prove a Run ticket became Unknown or was reconciled.

## Frozen contract

The frozen AgentKit source is `5b7c84361206b45ac381b10437097d3b76c322c1`; Pi is 1.3.0 with Node 24.20.0. `crates/endpoint-pi/src/subagents.rs::direct_prompt` persists an uncertain receipt, sends the host `input.prepare` control, then issues the official Pi `prompt` request. `crates/endpoint-pi/src/rpc.rs::request` correlates by RPC id and uses its fixed 30-second timeout before forgetting the correlation. The gate never sets or shortens that timeout.

Pi RPC is newline-delimited JSON. This wrapper examines AgentKit-client-to-Pi stdin lines and arms only immediately before forwarding the one `type:"prompt"` line containing the configured unique marker. It writes that exact line unchanged. The preceding `/agentkit-control ...` input-prepare prompt and ordinary requests pass through unchanged. It drains Pi stdout after the gate into bounded memory without forwarding any byte. A release file created by the owning host flushes buffered response bytes in their original order, after which stdout passes through. It never replays a request or fabricates a response.

## Invocation

Supply the exact command vector used for the official Pi process after `--`; do not use a shell string. The endpoint appends its normal RPC arguments and companion extension to the configured executable/arguments. PiConnector's `--version` probe reaches this wrapper with `--version` in the command vector; the wrapper `exec`s that exact command before creating evidence files. RPC startup then uses the same receipt/release templates with `{pid}` replaced by the wrapper process ID, so reconnects retain separate receipts. The owner can derive that process ID from the child it launched.

```text
python3 scripts/pi_return_gate_fixture.py \
  --marker AGENTKIT_PI_UNKNOWN_GATE_20261025_7f23c9 \
  --receipt /owned/probe/pi-return-{pid}.jsonl \
  --release-file /owned/probe/pi-release-{pid} \
  --max-gate-bytes 4194304 \
  -- /absolute/path/to/node /absolute/path/to/pi-entry <original Pi arguments>
```

The owned host creates the empty release file only after its ordinary `session.prompt` call reaches the original 30-second timeout and its existing ticket reports uncertain. The host then performs its existing read-only state/history reconciliation using the same `rpcId`/receipt identity. Because every stdout response is held while gated, release is required before those public queries can receive responses. The owner must not prepare a second prompt or synthesize an acknowledgement. A release file must be a new, empty file; an existing path is an error.

The JSONL receipt stores only event names, prompt ordinal, byte counts, exit status and safe error class. It omits arguments, prompt content, response content and credentials. The gate buffer is capped (default 4 MiB, configurable); overflow terminates only the wrapper-owned child with bounded TERM/KILL cleanup and leaves a failure receipt. An unexpected second marked prompt also fails and cleans up. The Pi CLI and AgentKit companion are not patched.

## Local fixture checks and limits

The fake-process tests cover unchanged control preparation and pre-marker traffic, gate arming on client stdin before the marked line is delivered, withheld stdout responses, explicit release ordering, `--version` passthrough without consuming the RPC receipt path, per-child receipt naming, bounded overflow, duplicate marker, child exit while gated and pre-existing signal rejection.

These are transport tests only. No real Pi runtime was launched here. Later acceptance still must prove the original public RunController submission returns Unknown after the unchanged 30-second contract, its receipt keeps the original `rpcId` and payload identity, read-only reconciliation observes that original input without resubmitting it, and the provider business mutation count is exactly one. A synthetic model/provider response alone cannot prove those requirements.
