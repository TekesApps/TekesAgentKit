//! Independent public-service consumer checks for Web child views.
//!
//! These tests drive SharedAgent request/follow; the fixture only speaks the
//! SessionEndpoint protocol and does not reproduce the child-view state machine.

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_web::SharedAgent;
use tokio::sync::{Notify, mpsc};

#[derive(Clone)]
struct FixtureConnector {
    epoch: u64,
    control: Arc<FixtureControl>,
}
struct FixtureEndpoint {
    host: HostDescription,
    epoch: u64,
    control: Arc<FixtureControl>,
}
#[derive(Clone, Copy, Debug)]
enum CatalogMode {
    Continuable,
    OneShot,
    ParentUnavailable,
}
struct FixtureControl {
    hold_next_catalog: AtomicBool,
    catalog_entered: Notify,
    release_catalog: Notify,
    mutations: AtomicUsize,
    catalog_mode: std::sync::Mutex<CatalogMode>,
}
impl FixtureControl {
    fn new() -> Self {
        Self {
            hold_next_catalog: AtomicBool::new(false),
            catalog_entered: Notify::new(),
            release_catalog: Notify::new(),
            mutations: AtomicUsize::new(0),
            catalog_mode: std::sync::Mutex::new(CatalogMode::Continuable),
        }
    }
}

#[async_trait]
impl EndpointConnector for FixtureConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(FixtureEndpoint {
            epoch: self.epoch,
            control: self.control.clone(),
            host: serde_json::from_value(json!({
                "protocolVersion":4,"product":{"name":"family-fixture","version":"1"},
                "capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],
                "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
            }))?,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for FixtureEndpoint {
    fn identity(&self) -> &str {
        "fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&[
            "subagents.list",
            "subagents.history",
            "subagents.prompt",
            "subagents.cancel",
        ])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let frame = match target {
            StreamTarget::SessionInventory => {
                json!({"type":"baseline","generation":format!("inventory-{}",self.epoch),"snapshot":{"items":[{"sessionId":"parent","title":"Parent","updatedAt":1,"running":false,"blank":false}]}})
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":format!("control-{}",self.epoch),"snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => {
                json!({"type":"reset","generation":format!("actions-{}",self.epoch)})
            }
            StreamTarget::SessionJournal { address, .. } => {
                json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":format!("journal-{}",self.epoch),"throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
            }
            _ => return Err(unsupported("unexpected fixture follow target")),
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
    async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
        match method {
            "sessions.recover" => Ok(json!({})),
            "endpoint.capabilities" => {
                serde_json::to_value(self.capabilities()).map_err(Into::into)
            }
            "subagents.list" => {
                if self.control.hold_next_catalog.swap(false, Ordering::SeqCst) {
                    self.control.catalog_entered.notify_one();
                    tokio::time::timeout(
                        Duration::from_secs(2),
                        self.control.release_catalog.notified(),
                    )
                    .await
                    .map_err(|_| unsupported("fixture catalog release timed out"))?;
                }
                let mode = *self.control.catalog_mode.lock().expect("catalog mode lock");
                let (parent_available, child_mode) = match mode {
                    CatalogMode::Continuable => (true, "continuable"),
                    CatalogMode::OneShot => (true, "one-shot"),
                    CatalogMode::ParentUnavailable => (false, "continuable"),
                };
                Ok(
                    json!({"parentAvailable":parent_available,"entries":[{"kind":"child","id":"child","mode":child_mode}]}),
                )
            }
            "subagents.history" => {
                let earlier = payload["beforeSequence"].as_i64().is_some();
                let entries = if earlier {
                    vec![history_entry(0, "turn/start"), history_entry(1, "turn/end")]
                } else {
                    vec![history_entry(2, "turn/start"), history_entry(3, "turn/end")]
                };
                Ok(
                    json!({"parentSessionId":payload["parentSessionId"],"childSessionId":payload["childSessionId"],
                    "throughSequence":3,"historyRevision":"a".repeat(64),"entries":entries,
                    "hasMoreBefore":!earlier,"runOpen":true,"sourceCall":null}),
                )
            }
            "subagents.prompt" | "subagents.cancel" => {
                self.control.mutations.fetch_add(1, Ordering::SeqCst);
                Ok(json!({}))
            }
            _ => Err(unsupported("unexpected fixture call")),
        }
    }
    async fn close(&self) {}
}

fn history_entry(seq: i64, event_type: &str) -> Value {
    json!({"event":{"type":event_type,"seq":seq,"time":seq+1,"data":{"turn":1}}})
}
async fn setup_with_control(control: Arc<FixtureControl>) -> SharedAgent {
    let mut agent = SharedAgent::default();
    agent.set_session_url_scheme("tekes").unwrap();
    agent
        .register(
            "fixture".into(),
            "tekes".into(),
            "Fixture".into(),
            Arc::new(FixtureConnector { epoch: 1, control }),
        )
        .unwrap();
    agent
        .endpoint("fixture")
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(2))
        .await
        .unwrap();
    agent
}
async fn setup() -> SharedAgent {
    setup_with_control(Arc::new(FixtureControl::new())).await
}

#[tokio::test]
async fn family_follow_emits_the_initial_projection_and_releases_on_disconnect() {
    let agent = Arc::new(setup().await);
    let (tx, mut rx) = mpsc::channel(1);
    let observed = agent.clone();
    let follow = tokio::spawn(async move {
        observed
            .follow(
                &json!({"op":"follow","endpoint":"fixture","target":{"kind":"familyView","parentSessionId":"parent"}}),
                tx,
            )
            .await
    });

    let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame["endpointId"], "fixture");
    assert_eq!(frame["parentSessionId"], "parent");
    assert!(frame["summary"]["tasks"].is_array());
    assert!(frame["subagentCatalog"]["entries"].is_array());

    drop(rx);
    tokio::time::timeout(Duration::from_secs(2), follow)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn call(method: &str, payload: Value) -> Value {
    static NEXT_RPC_ID: AtomicUsize = AtomicUsize::new(0);
    json!({"op":"call","endpoint":"fixture","method":method,"rpcId":format!("consumer-test-{}",NEXT_RPC_ID.fetch_add(1,Ordering::Relaxed)),"payload":payload})
}
async fn open_child(agent: &SharedAgent) -> Value {
    agent
        .request(&call(
            "agent.child.open",
            json!({"parentSessionId":"parent","childId":"child"}),
        ))
        .await
        .unwrap()
}
fn address(handle: &Value) -> Value {
    json!({"handle":handle["handle"],"parentSessionId":"parent","childId":"child"})
}

#[tokio::test]
async fn independently_opened_child_views_keep_separate_history_and_follow_leases() {
    let agent = Arc::new(setup().await);
    let first = open_child(&agent).await;
    let second = open_child(&agent).await;
    assert_ne!(first["handle"], second["handle"]);
    assert_eq!(first["transcript"]["hasMoreBefore"], true);
    assert_eq!(second["transcript"]["hasMoreBefore"], true);

    let earlier = agent
        .request(&call("agent.child.earlier", address(&first)))
        .await
        .unwrap();
    assert_eq!(earlier["loaded"], true);
    assert_eq!(earlier["transcript"]["hasMoreBefore"], false);
    let (tx_a, mut rx_a) = mpsc::channel(4);
    let observed = agent.clone();
    let handle_a = first["handle"].clone();
    let follow_a = tokio::spawn(async move {
        observed.follow(&json!({"op":"follow","endpoint":"fixture","target":{"kind":"childView","handle":handle_a,"parentSessionId":"parent","childId":"child"}}),tx_a).await
    });
    let frame_a = tokio::time::timeout(Duration::from_secs(2), rx_a.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame_a["handle"], first["handle"]);
    assert_eq!(frame_a["transcript"]["hasMoreBefore"], false);
    drop(rx_a);
    tokio::time::timeout(Duration::from_secs(2), follow_a)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        agent
            .request(&call("agent.child.earlier", address(&first)))
            .await
            .is_err(),
        "follow termination must release its handle"
    );

    let (tx_b, mut rx_b) = mpsc::channel(4);
    let observed = agent.clone();
    let handle_b = second["handle"].clone();
    let follow_b = tokio::spawn(async move {
        observed.follow(&json!({"op":"follow","endpoint":"fixture","target":{"kind":"childView","handle":handle_b,"parentSessionId":"parent","childId":"child"}}),tx_b).await
    });
    let frame = tokio::time::timeout(Duration::from_secs(2), rx_b.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame["handle"], second["handle"]);
    assert_eq!(
        frame["transcript"]["hasMoreBefore"], true,
        "the second open must retain its own head page"
    );
    drop(rx_b);
    tokio::time::timeout(Duration::from_secs(2), follow_b)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn child_handle_rejects_wrong_address_and_stale_connection_epoch() {
    let agent = setup().await;
    let opened = open_child(&agent).await;
    let mut wrong_address = address(&opened);
    wrong_address["childId"] = json!("other-child");
    assert!(
        agent
            .request(&call("agent.child.earlier", wrong_address))
            .await
            .is_err()
    );

    agent
        .replace_connector(
            "fixture",
            Arc::new(FixtureConnector {
                epoch: 2,
                control: Arc::new(FixtureControl::new()),
            }),
        )
        .await
        .unwrap();
    let initial_epoch = opened["connectionEpoch"].as_u64().unwrap();
    let mut ready = false;
    for _ in 0..100 {
        if agent
            .endpoint("fixture")
            .unwrap()
            .runtime
            .snapshot()
            .connection_epoch
            > initial_epoch
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ready, "fixture reconnect should advance the runtime epoch");
    assert!(
        agent
            .request(&call("agent.child.earlier", address(&opened)))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn closing_child_view_while_catalog_read_is_pending_blocks_prompt_and_cancel_submission() {
    for method in ["agent.child.prompt", "agent.child.cancel"] {
        let control = Arc::new(FixtureControl::new());
        let agent = Arc::new(setup_with_control(control.clone()).await);
        let opened = open_child(&agent).await;
        let child_address = address(&opened);
        control.hold_next_catalog.store(true, Ordering::SeqCst);

        let mut payload = child_address.clone();
        if method == "agent.child.prompt" {
            payload["mode"] = json!("queue");
            payload["content"] = json!([]);
        }
        let requester = agent.clone();
        let request = tokio::spawn(async move { requester.request(&call(method, payload)).await });
        tokio::time::timeout(Duration::from_secs(2), control.catalog_entered.notified())
            .await
            .expect("catalog read should enter its bounded hold");

        let mut close = child_address;
        close["handle"] = opened["handle"].clone();
        agent
            .request(&call("agent.child.close", close))
            .await
            .expect("inactive child handle should close during catalog read");
        control.release_catalog.notify_one();

        let result = tokio::time::timeout(Duration::from_secs(2), request)
            .await
            .expect("child operation should leave the catalog hold")
            .expect("child request task should not panic");
        assert!(result.is_err(), "{method} must fail after revocation");
        assert_eq!(
            control.mutations.load(Ordering::SeqCst),
            0,
            "{method} must not submit a remote mutation after the handle closes"
        );
    }
}

#[tokio::test]
async fn one_shot_and_parent_unavailable_children_are_readable_but_not_controllable() {
    for mode in [CatalogMode::OneShot, CatalogMode::ParentUnavailable] {
        let control = Arc::new(FixtureControl::new());
        *control.catalog_mode.lock().expect("catalog mode lock") = mode;
        let agent = setup_with_control(control.clone()).await;
        let opened = open_child(&agent).await;
        assert_eq!(opened["transcript"]["hasMoreBefore"], true);
        match mode {
            CatalogMode::OneShot => assert_eq!(opened["availability"], "oneShot"),
            CatalogMode::ParentUnavailable => {
                assert_eq!(opened["availability"], "parentUnavailable")
            }
            CatalogMode::Continuable => unreachable!(),
        }

        for method in ["agent.child.prompt", "agent.child.cancel"] {
            let mut payload = address(&opened);
            if method == "agent.child.prompt" {
                payload["mode"] = json!("queue");
                payload["content"] = json!([]);
            }
            assert!(
                agent.request(&call(method, payload)).await.is_err(),
                "{method} must be rejected for {mode:?}"
            );
        }
        assert_eq!(
            control.mutations.load(Ordering::SeqCst),
            0,
            "read-only child availability must not reach the endpoint mutation methods"
        );
    }
}
