use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_web::SharedAgent;

#[derive(Clone, Default)]
struct Connector {
    calls: Arc<Mutex<Vec<(String, Value, String)>>>,
    archive_disabled: Arc<AtomicBool>,
    rename_fails: Arc<AtomicBool>,
}
struct Endpoint {
    connector: Connector,
    host: HostDescription,
}
#[async_trait]
impl EndpointConnector for Connector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(Endpoint {
            connector: self.clone(),
            host: serde_json::from_value(
                json!({"protocolVersion":4,"product":{"name":"fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false}),
            )?,
        }))
    }
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "test"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps =
            EndpointCapabilities::methods(&["session.rename", "session.fork", "session.archive"]);
        caps.supports_archive_session = !self.connector.archive_disabled.load(Ordering::SeqCst);
        caps
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let frame = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"source","title":"Topic (9)","updatedAt":1,"running":false,"blank":false}]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
            }
            _ => return Err(unsupported("fixture target")),
        };
        Ok(Box::pin(
            stream::once(async { Ok(frame) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("fixture history"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Ok(())
    }
    async fn call(&self, method: &str, payload: Value, id: &str) -> Result<Value> {
        self.connector
            .calls
            .lock()
            .unwrap()
            .push((method.into(), payload.clone(), id.into()));
        match method {
            "session.fork" => Ok(json!({"sessionId":"child"})),
            "session.rename" if self.connector.rename_fails.load(Ordering::SeqCst) => {
                Err(Error::Timeout)
            }
            "session.rename" => Ok(json!({"title":payload["title"],"seq":7})),
            _ => Ok(json!({})),
        }
    }
    async fn close(&self) {}
}
async fn setup() -> (SharedAgent, Connector) {
    let connector = Connector::default();
    let mut agent = SharedAgent::default();
    agent.set_session_url_scheme("tekes").unwrap();
    agent
        .register(
            "test".into(),
            "tekes".into(),
            "Test".into(),
            Arc::new(connector.clone()),
        )
        .unwrap();
    agent
        .endpoint("test")
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(2))
        .await
        .unwrap();
    (agent, connector)
}
fn request(action: &str) -> Value {
    json!({"op":"call","endpoint":"test","method":"thread.performAction","rpcId":"stable","payload":{"sessionId":"source","actionId":action}})
}
#[tokio::test]
async fn commands_revalidate_current_policy_instead_of_trusting_an_open_menu() {
    let (agent, connector) = setup().await;
    let menu=agent.request(&json!({"op":"call","endpoint":"test","method":"thread.actions","rpcId":"read","payload":{"sessionId":"source"}})).await.unwrap();
    assert!(
        menu["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == "archive" && i["enabled"] == true)
    );
    connector.archive_disabled.store(true, Ordering::SeqCst);
    assert!(agent.request(&request("archive")).await.is_err());
    assert!(agent.request(&request("session.discard")).await.is_err());
    assert!(agent.request(&request("pin")).await.is_err());
    assert!(connector.calls.lock().unwrap().is_empty());
    let mut rename = request("rename");
    rename["payload"]["title"] = json!(" New title ");
    let result = agent.request(&rename).await.unwrap();
    assert_eq!(result["renamedTitle"], "New title");
    assert_eq!(
        connector.calls.lock().unwrap()[0],
        (
            "session.rename".into(),
            json!({"sessionId":"source","title":"New title"}),
            "stable".into()
        )
    );
}
#[tokio::test]
async fn fork_preserves_created_pointer_if_followup_rename_fails_and_never_retries() {
    let (agent, connector) = setup().await;
    connector.rename_fails.store(true, Ordering::SeqCst);
    let outcome = agent.request(&request("fork")).await.unwrap();
    assert_eq!(
        outcome["navigation"],
        json!({"endpoint":"test","sessionId":"child"})
    );
    assert!(outcome["warning"].is_string());
    let calls = connector.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "session.fork");
    assert_eq!(
        calls[1].1,
        json!({"sessionId":"child","title":"Topic (10)"})
    );
}
struct Host {
    calls: Mutex<Vec<String>>,
}
#[async_trait]
impl ThreadActionHost for Host {
    fn methods(&self) -> std::collections::BTreeSet<String> {
        ["thread.setPinned".into(), "thread.movePin".into()].into()
    }
    fn pin_position(&self, _: &str, _: &str) -> Option<ThreadPinPosition> {
        Some(ThreadPinPosition {
            pinned: true,
            can_move_up: false,
            can_move_down: true,
        })
    }
    async fn perform(
        &self,
        endpoint: &str,
        request: &ThreadActionRequest,
        id: &str,
    ) -> Result<ThreadActionOutcome> {
        self.calls.lock().unwrap().push(format!(
            "{endpoint}:{}:{}:{id}",
            request.session_id, request.action_id
        ));
        Ok(ThreadActionOutcome::default())
    }
}
#[tokio::test]
async fn pin_authority_is_an_explicit_shared_host_callback() {
    let (mut agent, connector) = setup().await;
    let host = Arc::new(Host {
        calls: Mutex::new(vec![]),
    });
    agent.install_thread_action_host(host.clone());
    let menu = agent
        .thread_actions(&agent.endpoint("test").unwrap(), "source")
        .unwrap();
    assert_eq!(menu.items[0].id, "unpin");
    assert!(menu.items[0].enabled);
    agent.request(&request("movePinDown")).await.unwrap();
    assert_eq!(
        *host.calls.lock().unwrap(),
        vec!["test:source:movePinDown:stable"]
    );
    assert!(connector.calls.lock().unwrap().is_empty());
}
