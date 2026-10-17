//! Read one JSON connection document on stdin. Credentials never belong in argv/log output.
use serde_json::{Value, json};
use std::{io::Read, sync::Arc, time::Duration};
use tekes_agent_contract::{Error, HistoryRequest, Result, SessionEndpoint, string};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_session::SessionRuntime;
#[tokio::main]
async fn main() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .take(1024 * 1024)
        .read_to_string(&mut input)
        .map_err(|_| Error::Transport("stdin unavailable".into()))?;
    let connection: Value = serde_json::from_str(&input)?;
    let connector = KernelConnector::new(
        "kernel-headless",
        string(&connection, "url")?,
        string(&connection, "bearer")?.into(),
    )?;
    let runtime = SessionRuntime::spawn("kernel-headless", Arc::new(connector.clone()));
    runtime.wait_ready(Duration::from_secs(15)).await?;
    println!(
        "KERNEL_V4_READY inventory={}",
        runtime.snapshot().sessions.len()
    );
    let cwd = string(&connection, "cwd")?;
    let session = runtime.create_session(cwd, "agentkit-smoke-create").await?;
    runtime.call("session.selectModel",json!({"sessionId":session,"provider":string(&connection,"provider")?,"model":string(&connection,"model")?}),"agentkit-smoke-model").await?;
    runtime.open_session(&session).await?;
    runtime
        .prompt(
            &session,
            "AgentKit headless acceptance",
            "agentkit-smoke-prompt",
        )
        .await?;
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            if snapshot.sessions[&session]
                .entries
                .values()
                .any(|e| e.event.kind == "turn/end")
            {
                break;
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|_| Error::Timeout)??;
    // An accepted operation retried with its original identity must not run twice.
    runtime
        .prompt(
            &session,
            "AgentKit headless acceptance",
            "agentkit-smoke-prompt",
        )
        .await?;
    runtime.reconnect().await?;
    runtime.open_session(&session).await?;
    let snapshot = runtime.snapshot();
    let state = &snapshot.sessions[&session];
    let users = state
        .entries
        .values()
        .filter(|e| e.event.kind == "user/message")
        .count();
    assert_eq!(users, 1, "idempotent prompt duplicated durable input");
    assert!(
        state
            .entries
            .values()
            .any(|e| e.event.kind == "assistant/message")
    );
    println!(
        "PROMPT_RECONCILED entries={} user_messages={} tail={}",
        state.entries.len(),
        users,
        state.tail_sequence
    );
    let direct = connector.connect_kernel().await?;
    let before = state
        .entries
        .keys()
        .nth(3)
        .copied()
        .unwrap_or(state.tail_sequence);
    let page = direct
        .history_page(HistoryRequest {
            session_id: session.clone(),
            through_sequence: state.history_through_sequence,
            before_sequence: Some(before),
            max_messages: 1,
        })
        .await?;
    assert!(!page.entries.is_empty());
    println!(
        "FROZEN_HISTORY_PAGE_OK entries={} cut={}",
        page.entries.len(),
        page.through_sequence
    );
    direct.close().await;
    runtime.load_earlier(&session).await?;
    runtime.close_session(&session).await?;
    runtime.open_session(&session).await?;
    runtime
        .prompt(
            &session,
            "AgentKit cancellation acceptance",
            "agentkit-cancel-prompt",
        )
        .await?;
    runtime.cancel(&session, "agentkit-smoke-cancel").await?;
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let settled = changes.inspect(|store| {
                store.sessions.get(&session).is_some_and(|state| {
                    state
                        .entries
                        .values()
                        .filter(|e| e.event.kind == "turn/end")
                        .count()
                        >= 2
                        && !state.run_open
                })
            });
            if settled {
                break;
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
        Ok::<_, Error>(())
    })
    .await
    .map_err(|_| Error::Timeout)??;
    println!("CANCEL_SETTLEMENT_OBSERVED");
    runtime.stop().await?;
    println!("RECONNECT_REOPEN_CANCEL_STOP_OK");
    Ok(())
}
