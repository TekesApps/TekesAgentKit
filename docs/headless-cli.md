# Headless JSON consumer

Build with `cargo build -p tekes-agent-headless --locked`, then send **one JSON document** to stdin:

```sh
./target/debug/tekes-agent-headless < /secure/path/connection.json
```

This is an integration/debugging consumer, not an interactive shell or daemon. Input is limited to 16 MiB. Stdout is newline-delimited JSON: an initial ready/version record, then `{ "op": ..., "result": ... }` per successful operation. An unexpected error ends the batch with a nonzero exit. The runtime stops after the batch. Never commit a connection document containing credentials; command results and snapshots can contain session content.

## Connection configurations

Native:

```json
{
  "kind": "native", "identity": "native",
  "url": "http://127.0.0.1:8080/", "bearer": "HOST_PROVIDED_BEARER",
  "commands": [{ "op": "list" }]
}
```

DSH:

```json
{
  "kind": "dsh", "identity": "dsh",
  "url": "http://127.0.0.1:8080/", "cookie": "HOST_PROVIDED_SESSION_COOKIE",
  "commands": [{ "op": "list" }]
}
```

DSH accepts `bearer` instead of `cookie`; cookie takes precedence if both exist. The host must expose the official-runtime-backed version bridge described in [getting started](getting-started.md).

Pi:

```json
{
  "kind": "pi", "identity": "pi",
  "executable": "/absolute/pi-runtime/node/bin/node",
  "arguments": ["/absolute/pi-runtime/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js"],
  "dataRoot": "/absolute/owned-agentkit-data",
  "environment": { "HOME": "/absolute/host-authorized-home", "PATH": "/usr/bin:/bin" },
  "extensions": [], "sessionRoots": [],
  "commands": [{ "op": "list" }]
}
```

`identity` defaults to `kind`. Pi does not inherit the process environment; any additional provider variables must be explicit host inputs. Provider/model configuration and runtime installation remain host responsibilities. `sessionRoots` only authorizes explicit adoption of native session files; no global scan is performed.

## Batch example

After configuring a provider/model on the host, replace the `commands` array with:

```json
[
  { "op": "create", "cwd": "/absolute/project", "saveAs": "s" },
  { "op": "call", "sessionId": "s", "method": "session.selectModel",
    "args": { "provider": "HOST_PROVIDER_ID", "model": "HOST_MODEL_ID" } },
  { "op": "open", "sessionId": "s" },
  { "op": "compose", "sessionId": "s", "text": "Describe this workspace." },
  { "op": "waitSettled", "sessionId": "s" },
  { "op": "presentation", "sessionId": "s" },
  { "op": "call", "sessionId": "s", "method": "session.locator" },
  { "op": "reconnect" },
  { "op": "open", "sessionId": "s" },
  { "op": "snapshot" }
]
```

`compose` uses the public presentation API and verifies immediate echo before dispatch. For explicit idempotency/reconciliation tests, use `call` with a stable `rpcId` and a `session.prompt` payload; `compose` allocates its own captured submission ID.

## Commands

| op | Fields | Result |
|---|---|---|
| `list` | — | Current inventory summaries. |
| `create` | `cwd`, optional `saveAs` | New session ID; `saveAs` records a session alias. |
| `open` / `close` | `sessionId` | Attach/detach a content window. |
| `earlier` | `sessionId` | Whether earlier history was loaded. |
| `snapshot` | — | Full serialized memory read model. |
| `reconnect` | — | Reconnect and wait for baseline readiness; no mutation replay. |
| `compose` | `sessionId`, `text` | Text draft → optimistic echo → prompt admission. |
| `presentation` | `sessionId` | Selected RunSnapshot with transcript, composer and connection state. |
| `call` | `method`, optional `sessionId`, `args`, `rpcId`, `saveAs`, `savePointer` | Lossless unary result. |
| `callError` | Same input as `call` | Requires failure and returns an error description; unexpected success fails the batch. |
| `waitSettled` | `sessionId`, optional `minimumTurn` (default 1) | Wait up to 30 seconds for settlement. Pi's minimum counts assistant records; it is an integration helper, not a universal turn identity. |
| `observeReady` | `sessionId`, optional `path` (default `.`) | Wait up to 10 seconds for the file-observation ready frame; continuous observation uses the Rust API. |
| `waitGoal` | `sessionId`, `phase` | Poll the typed goal API for up to 30 seconds. |
| `promptAndAnswer` | `sessionId`, `text`, `outcome` | Concurrently submit and answer a matching scoped actionable; intended for integration tests. |

A command's top-level `sessionId` resolves a saved alias such as `s`; literal IDs also work. In `call.args`, any string starting with `@` resolves a saved variable recursively, including objects/arrays. `savePointer` selects a JSON Pointer from the unary result (default `/sessionId`); use the empty string to save the whole result. Example:

```json
{
  "op": "call", "sessionId": "s", "method": "goals.get",
  "saveAs": "goal", "savePointer": ""
}
```

This CLI cannot stage arbitrary user files or configure product settings by implication. Use explicit upload calls or the public Rust APIs and host preparation callback. Harness-specific optional methods and error behavior are listed in [session-endpoints.md](session-endpoints.md).
