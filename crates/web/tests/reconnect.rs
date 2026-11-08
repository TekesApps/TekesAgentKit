use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tekes_agent_contract::*;
use tekes_agent_web::SharedAgent;

struct Fixture {
    available: AtomicBool,
    attempts: AtomicUsize,
    mutations: AtomicUsize,
}
struct Connector(Arc<Fixture>);
struct Endpoint(Arc<Fixture>, HostDescription);
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.0.attempts.fetch_add(1, Ordering::SeqCst);
        if !self.0.available.load(Ordering::SeqCst) {
            return Err(Error::Transport("controlled Harness unavailable".into()));
        }
        Ok(Arc::new(Endpoint(
            self.0.clone(),
            serde_json::from_value(json!({
                "protocolVersion":4,"product":{"name":"reconnect-fixture","version":"1"},
                "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                "cwd":"/work","home":"/home","attachedSessions":1,"canOpenPath":false
            }))?,
        )))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.1
    }
    async fn close(&self) {}
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let frame = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"i","snapshot":{"items":[{"sessionId":"existing","running":false,"blank":false,"updatedAt":1}]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"c","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"a"}),
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"j","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
            }
            _ => return Err(unsupported("unexpected fixture stream")),
        };
        Ok(Box::pin(
            stream::once(async { Ok(frame) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("fixture history page"))
    }
    async fn call(&self, method: &str, _: Value, _: &str) -> Result<Value> {
        if method == "endpoint.capabilities" {
            return Ok(serde_json::to_value(self.capabilities())?);
        }
        if method == "sessions.recover" {
            return Ok(json!({}));
        }
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
        Err(unsupported("no business replay is allowed"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        self.0.mutations.fetch_add(1, Ordering::SeqCst);
        Err(unsupported("no response replay is allowed"))
    }
}

#[tokio::test]
async fn public_explicit_reconnect_recovers_after_runtime_retries_exhaust_without_business_replay()
{
    let fixture = Arc::new(Fixture {
        available: AtomicBool::new(false),
        attempts: AtomicUsize::new(0),
        mutations: AtomicUsize::new(0),
    });
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "tekes".into(),
            "Fixture".into(),
            Arc::new(Connector(fixture.clone())),
        )
        .unwrap();
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.attempts.load(Ordering::SeqCst) < 5 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("existing finite retry policy should exhaust");
    let failed = agent
        .request(&json!({"op":"describe","endpoint":"fixture"}))
        .await
        .unwrap();
    assert!(
        failed["connection"]["failed"]
            .as_str()
            .unwrap()
            .contains("controlled Harness unavailable")
    );
    assert_eq!(failed["canReconnect"], true);
    fixture.available.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        fixture.attempts.load(Ordering::SeqCst),
        5,
        "restoring Harness alone must not restart exhausted retries"
    );
    let recovered = agent
        .request(&json!({"op":"reconnect","endpoint":"fixture"}))
        .await
        .unwrap();
    assert_eq!(recovered["connection"], "ready");
    assert_eq!(recovered["canReconnect"], false);
    assert_eq!(fixture.attempts.load(Ordering::SeqCst), 6);
    assert_eq!(fixture.mutations.load(Ordering::SeqCst), 0);
    assert!(runtime.snapshot().sessions.contains_key("existing"));
    runtime
        .acquire_history("existing")
        .await
        .unwrap()
        .release()
        .await
        .unwrap();
    runtime.stop().await.unwrap();
}
