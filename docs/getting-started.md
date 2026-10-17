# Getting started

TekesAgentKit is a headless Rust client. You can build and test it without any harness, UI toolkit or model account. It does not start an Agent server or configure a provider for you.

## Build and test

Install Rust with rustup, clone the repository and run:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
```

The repository selects Rust 1.95.0. Python 3 is required only for repository checks and integration scripts. Windows has not been runtime-accepted; documented real-harness checks currently use macOS. Linux acceptance is reported by the public CI workflow separately from real-harness results.

The crates are not on crates.io. Use local paths for explicit development overrides, or pin a published Git milestone tag/full commit for an application. See the README for the workspace packages and Git dependency declarations. MIT terms are in the root LICENSE; separately installed harnesses are not relicensed by this project.

## Native / Kernel example

Your host must already run a compatible Kernel 0.9.0 supervisor, provision its bearer and provider configuration, and authorize the workspace path. Kernel v4 is required; the adapter does not accept a v3 Native handshake.

The following is a complete application entry point with `tokio`, `serde_json`, `uuid` and the contract/session/presentation/endpoint-kernel dependencies from this workspace. It sends one real prompt through your configured provider. All values come from the host environment rather than embedded credentials:

```rust,ignore
use std::{env, sync::Arc, time::Duration};
use serde_json::json;
use tekes_agent_contract::PromptMode;
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_presentation::{ComposerDestination, ComposerDraft, RunController, RunScope};
use tekes_agent_session::SessionRuntime;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let connector = KernelConnector::new(
        "native", &env::var("AGENTKIT_KERNEL_URL")?, env::var("AGENTKIT_KERNEL_BEARER")?,
    )?;
    let runtime = Arc::new(SessionRuntime::spawn("native", Arc::new(connector)));
    runtime.wait_ready(Duration::from_secs(30)).await?;
    let session = runtime.create_session(
        &env::var("AGENTKIT_WORKSPACE")?, &uuid::Uuid::new_v4().to_string(),
    ).await?;
    runtime.call("session.selectModel", json!({
        "sessionId": session,
        "provider": env::var("AGENTKIT_PROVIDER")?,
        "model": env::var("AGENTKIT_MODEL")?
    }), &uuid::Uuid::new_v4().to_string()).await?;

    let controller = RunController::new(runtime.clone())?;
    let scope = RunScope::main("native", session);
    controller.open(scope.clone()).await?;
    let destination = ComposerDestination::run(scope);
    controller.edit(&destination, ComposerDraft {
        text: "Describe this workspace briefly.".into(), ..Default::default()
    })?;
    let ticket = controller.prepare(&destination, PromptMode::Queue)?;
    // An optimistic input is already present here. Admission does not mean the Agent finished.
    controller.dispatch(ticket, None).await?;
    let mut changes = controller.subscribe();
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            if snapshot.transcript.items.iter().any(|item| item.is_final) { break; }
            changes.changed().await.map_err(|_| tekes_agent_contract::Error::Disconnected)?;
        }
        Ok::<_, tekes_agent_contract::Error>(())
    }).await??;
    runtime.stop().await?;
    Ok(())
}
```

A production consumer should also handle final error/abort turns and connection failure instead of waiting only for a final answer. Dropping a `RunController` stops its observation; it does not stop the runtime. Keep the controller/store stable to retain destination-owned in-memory drafts. The compiled [native_run example](../crates/headless/examples/native_run.rs) and headless CLI are consumers of the public API, not production launchers.

## Other connectors

Pi uses `PiConfiguration::new(absolute_node_path, absolute_data_root)`, with the installed CLI in `arguments`. Set `environment` explicitly: child processes use `env_clear`, so HOME, PATH and any host-authorized provider variables must be supplied. Additional extensions and external session-file roots are opt-in. AgentKit writes only its own data root and uses the public RPC/extension API. Adopt an existing file with `session.adopt`; a locator does not authorize reading an arbitrary path.

DSH uses `DshConfiguration::new(identity, url)` and optional explicit `(header, value)` authorization. Its host needs authenticated Remote HTTP/mux and the version bridge used by ClientKit: `/api/tekes/harness-version` must obtain `getDshRuntimeVersion` from the installed official runtime, not a caller-written version string. The integration script contains a minimal isolated bridge plugin. A stock server without that bridge is rejected rather than trusted by assumption.

All connectors return the common `SessionEndpoint` contract. The runtime's endpoint identity must match the connector identity. Session addresses always include endpoint and native session; never route solely by a globally assumed session string.

## Common operations

- `runtime.capabilities()` describes implemented optional methods and lifecycle flags. `call_optional` rejects unadvertised methods; `call` is the lossless public protocol escape hatch.
- `create`, `submit`, `fork`, `archive`, `unarchive`, `discard`, `update_queue`, `upload_bytes`, `attachment_policy`, `goal` provide typed common operations. A capability still depends on the configured host/service being available.
- `open_session` waits for a readable window. `load_earlier` preserves a frozen journal cut, or expands Pi's native ancestry window. `close_session` releases that window without deleting durable harness history.
- `locator`, scoped file reads and `observe_files` use endpoint authority. Remote paths are never silently treated as local files.
- `respond` requires the current actionable identity/revision. Event observation is not permission to answer.
- New-thread presentation needs explicit host `CreateSessionRequest` inputs at dispatch. Existing Run dispatch cannot create another session. Nonempty draft settings require `SubmissionPreparation`; host configuration is never guessed.

Use `inspect`/revision subscription for ordinary reads; `snapshot()` explicitly copies the backing read model. Presentation snapshots are immutable watch values. An unknown submission must be reconciled before another explicit send; neither reconnect nor a remounted surface retries it automatically.

## Offline replay and integration

`cargo run -p tekes-agent-session --example replay --locked` replays newline-delimited normalized frames; inspect its example source for the frame wrapper. `cargo run -p tekes-agent-presentation --example presentation_replay --locked` consumes one JSON document with `scope`, serialized `MemoryProjectionStore` under `authority`, and an optional `draft`. It performs no transport operation.

Use `scripts/test-real-session-endpoints.py` for all three installed runtime layouts and a synthetic provider. It covers session protocol and presentation admission/reconciliation; `scripts/test-real-kernel.py` adds cancellation settlement. Source baseline JSON documents contain historical absolute developer paths as provenance only; builds do not load those paths or require those checkouts.

## Troubleshooting

| Symptom | Check |
|---|---|
| Native handshake/version rejected | Kernel must advertise v4; check the actual supervisor binary, not only source checkout HEAD. |
| DSH version probe unavailable | Install/configure the official-runtime-backed host bridge and correct authentication. |
| Pi process launch/version failure | Check absolute Node/CLI paths, exact supported version, explicit HOME/PATH and writable owned data root. |
| Pi data root already owned | Reuse one connector/runtime owner per data root, or stop its previous owner before opening another. |
| Native Pi file missing/identity changed | Restore or deliberately adopt the intended native file. The adapter refuses to replace known history with a blank session. |
| Method Unsupported | Query capabilities; supply the required explicit host backend, or use another supported flow. DSH ephemeral fork/discard is intentionally unsupported. |
| Submission unknown | Reconcile the captured session and RPC ID; do not create an automatic retry loop. |
| Retained transcript but disabled input | Inspect endpoint connection, per-session journal_error, archived state and unresolved submission separately. |
| CI fails before any steps | Inspect workflow annotations for account/runner configuration. A zero-step failure is not a Rust test result. |
