use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_session::*;
#[derive(Clone)]
struct Connector {
    connects: Arc<AtomicUsize>,
    prompts: Arc<AtomicUsize>,
}
struct Endpoint {
    attempt: usize,
    host: HostDescription,
    prompts: Arc<AtomicUsize>,
}
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let attempt = self.connects.fetch_add(1, Ordering::SeqCst);
        let host=serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"Kernel","version":"test"},"capabilities":["session-journal","actionable-sync","session-control-sync","session-inventory-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false})).unwrap();
        Ok(Arc::new(Endpoint {
            attempt,
            host,
            prompts: self.prompts.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "k"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let value = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"g","snapshot":{"items":[]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => {
                let snapshot = json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":0,"windowLimit":50,"entries":[{"event":{"type":"user/message","seq":0,"time":1,"data":{"text":"same identity"}}}],"hasMoreBefore":false}});
                if self.attempt == 0 {
                    return Ok(Box::pin(stream::once(async { Ok(snapshot) }).chain(
                        stream::once(async {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                            Err(Error::Disconnected)
                        }),
                    )));
                }
                snapshot
            }
        };
        Ok(Box::pin(
            stream::once(async { Ok(value) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        panic!("stale actionable must not reach endpoint")
    }
    async fn call(&self, method: &str, _: Value, _: &str) -> Result<Value> {
        if method == "session.prompt" {
            self.prompts.fetch_add(1, Ordering::SeqCst);
            Err(Error::Timeout)
        } else {
            Ok(json!({}))
        }
    }
    async fn close(&self) {}
}
#[tokio::test]
async fn automatic_reconnect_restores_open_journal_without_retrying_mutation() {
    let connector = Connector {
        connects: Arc::new(AtomicUsize::new(0)),
        prompts: Arc::new(AtomicUsize::new(0)),
    };
    let runtime = SessionRuntime::spawn("k", Arc::new(connector.clone()));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    runtime.open_session("s").await.unwrap();
    assert!(runtime.prompt("s", "input", "stable").await.is_err());
    assert_eq!(connector.prompts.load(Ordering::SeqCst), 1);
    let mut watch = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = watch.borrow_and_update().clone();
            if connector.connects.load(Ordering::SeqCst) >= 2
                && snapshot.sessions.get("s").is_some_and(|s| s.journal_ready)
            {
                break;
            }
            watch.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(runtime.snapshot().sessions["s"].entries.len(), 1);
    assert!(runtime.respond("stale", 0, json!(true)).await.is_err());
    runtime.close_session("s").await.unwrap();
    assert!(!runtime.snapshot().sessions["s"].journal_ready);
    runtime.open_session("s").await.unwrap();
    runtime.stop().await.unwrap();
    assert_eq!(runtime.snapshot().connection, ConnectionState::Stopped);
}

struct FailingConnector {
    attempts: Arc<AtomicUsize>,
}
#[async_trait]
impl EndpointConnector for FailingConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Err(Error::Disconnected)
    }
}
#[tokio::test]
async fn recovery_is_bounded_and_stop_still_works_after_failure() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let runtime = SessionRuntime::spawn(
        "k",
        Arc::new(FailingConnector {
            attempts: attempts.clone(),
        }),
    );
    assert!(runtime.wait_ready(Duration::from_secs(5)).await.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 5);
    assert!(matches!(
        runtime.snapshot().connection,
        ConnectionState::Failed(_)
    ));
    runtime.stop().await.unwrap();
    assert_eq!(runtime.snapshot().connection, ConnectionState::Stopped);
}
