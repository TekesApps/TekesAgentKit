use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tekes_agent_session::*;
use tokio::sync::{Semaphore, broadcast};
struct Plan {
    frame: Value,
    gate: Option<Arc<Semaphore>>,
}
#[derive(Default)]
struct State {
    inventory: VecDeque<Plan>,
    workspace: VecDeque<Plan>,
    inventory_senders: Vec<broadcast::Sender<Value>>,
    workspace_senders: Vec<broadcast::Sender<Value>>,
    rows: Vec<Value>,
    search: Value,
}
#[derive(Clone)]
struct Fixture {
    state: Arc<Mutex<State>>,
    workspace: bool,
    active: Arc<AtomicUsize>,
    connects: Arc<AtomicUsize>,
    journals: Arc<AtomicUsize>,
    mutations: Arc<AtomicUsize>,
}
fn row(id: &str) -> Value {
    json!({"sessionId":id,"title":id,"running":false,"blank":false,"updatedAt":1})
}
fn inventory(rows: Vec<Value>) -> Value {
    json!({"type":"baseline","generation":"g","snapshot":{"items":rows}})
}
fn workspace() -> Value {
    json!({"type":"baseline","generation":"g","snapshot":{"items":[],"archivedSessionIds":[],"pinnedSessionIds":[]}})
}
impl Fixture {
    fn new(workspace: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                rows: vec![row("s")],
                search: json!({"items":[],"hasMore":false}),
                ..Default::default()
            })),
            workspace,
            active: Default::default(),
            connects: Default::default(),
            journals: Default::default(),
            mutations: Default::default(),
        }
    }
    fn reads(&self) -> usize {
        self.state.lock().unwrap().inventory_senders.len()
    }
    async fn runtime(&self) -> Arc<SessionRuntime> {
        let r = Arc::new(SessionRuntime::spawn("fixture", Arc::new(self.clone())));
        r.wait_ready(Duration::from_secs(3)).await.unwrap();
        r
    }
}
struct Endpoint {
    fixture: Fixture,
    host: HostDescription,
}
#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.connects.fetch_add(1, Ordering::SeqCst);
        let mut capabilities = vec![
            "session-journal",
            "session-inventory-sync",
            "session-control-sync",
            "actionable-sync",
        ];
        if self.workspace {
            capabilities.push("workspace-sync");
        }
        Ok(Arc::new(Endpoint {
            fixture: self.clone(),
            host: serde_json::from_value(
                json!({"protocolVersion":4,"product":{"name":"owned inventory fixture","version":"1"},"capabilities":capabilities,"cwd":"/owned","home":"/owned","attachedSessions":0,"canOpenPath":false}),
            )?,
        }))
    }
}
struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
fn cursor(
    plan: Plan,
    receiver: broadcast::Receiver<Value>,
    active: Arc<AtomicUsize>,
) -> FrameStream {
    active.fetch_add(1, Ordering::SeqCst);
    let active = Active(active);
    let initial = stream::once(async move {
        if let Some(gate) = plan.gate {
            gate.acquire()
                .await
                .map_err(|_| Error::Disconnected)?
                .forget();
        }
        Ok(plan.frame)
    });
    let rest = stream::unfold((receiver, active), |(mut receiver, active)| async move {
        let next = receiver.recv().await.map_err(|_| Error::Disconnected);
        Some((next, (receiver, active)))
    });
    Box::pin(initial.chain(rest))
}
#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::methods(&["session.search", "test.barrier"])
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let mut state = self.fixture.state.lock().unwrap();
        let frame = match target {
            StreamTarget::SessionInventory => {
                let (sender, receiver) = broadcast::channel(32);
                state.inventory_senders.push(sender);
                let plan = state.inventory.pop_front().unwrap_or_else(|| Plan {
                    frame: inventory(state.rows.clone()),
                    gate: None,
                });
                return Ok(cursor(plan, receiver, self.fixture.active.clone()));
            }
            StreamTarget::WorkspaceInventory => {
                assert!(self.fixture.workspace);
                let (sender, receiver) = broadcast::channel(32);
                state.workspace_senders.push(sender);
                let plan = state.workspace.pop_front().unwrap_or(Plan {
                    frame: workspace(),
                    gate: None,
                });
                return Ok(cursor(plan, receiver, self.fixture.active.clone()));
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionJournal { address, .. } => {
                self.fixture.journals.fetch_add(1, Ordering::SeqCst);
                let snapshot = json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":0,"windowLimit":50,"entries":[{"event":{"seq":0,"type":"user/message","time":1,"data":{"text":"保留历史"}}}],"hasMoreBefore":false}});
                let preview = json!({"type":"transient","generation":"g","sessionId":address.session_id,"event":{"type":"assistant/chunk","seq":99,"time":1,"data":{"turn":1,"step":1,"chunk":{"type":"text-delta","delta":"保留流式预览"}}}});
                return Ok(Box::pin(
                    stream::iter(vec![Ok(snapshot), Ok(preview)]).chain(stream::pending()),
                ));
            }
            _ => return Err(unsupported("owned fixture target")),
        };
        Ok(Box::pin(
            stream::once(async { Ok(frame) }).chain(stream::pending()),
        ))
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        Err(unsupported("owned fixture paging"))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        Err(unsupported("owned fixture response"))
    }
    async fn call(&self, method: &str, _: Value, _: &str) -> Result<Value> {
        match method {
            "session.search" => Ok(self.fixture.state.lock().unwrap().search.clone()),
            "test.barrier" => Ok(Value::Null),
            _ => {
                self.fixture.mutations.fetch_add(1, Ordering::SeqCst);
                Err(unsupported("unexpected mutation"))
            }
        }
    }
    async fn close(&self) {}
}
async fn until(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !check() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn epoch(runtime: &SessionRuntime) -> u64 {
    runtime.inspect(|s| s.connection_epoch)
}
#[tokio::test]
async fn unknown_search_refreshes_once_and_new_deltas_preserve_original_history_and_epoch() {
    let f = Fixture::new(true);
    let r = f.runtime().await;
    r.open_session("s").await.unwrap();
    until(|| r.inspect(|s| !s.sessions["s"].previews.is_empty())).await;
    f.state.lock().unwrap().inventory_senders[0].send(json!({"type":"agent-error","generation":"g","sessionId":"s","message":"原发生时刻的执行失败"})).unwrap();
    until(|| r.inspect(|s| s.sessions["s"].agent_error.is_some())).await;
    let before = r.inspect(|s| serde_json::to_value(&s.sessions["s"]).unwrap());
    let e = epoch(&r);
    {
        let mut s = f.state.lock().unwrap();
        s.rows.push(row("new"));
        s.search = json!({"items":[{"sessionId":"new","snippet":"正文","provider":"extra"}],"hasMore":true});
    }
    let result = r
        .search_sessions_at_epoch(e, "query", "search-1")
        .await
        .unwrap();
    assert!(result.has_more);
    assert_eq!(result.items[0].extra["provider"], "extra");
    assert!(r.inspect(|s| s.sessions["new"].summary.is_some()));
    assert_eq!(epoch(&r), e);
    assert_eq!(
        r.inspect(|s| serde_json::to_value(&s.sessions["s"]).unwrap()),
        before
    );
    assert_eq!(f.reads(), 2);
    assert_eq!(f.state.lock().unwrap().workspace_senders.len(), 2);
    until(|| f.active.load(Ordering::SeqCst) == 2).await;
    f.state.lock().unwrap().inventory_senders[1]
        .send(json!({"type":"status","generation":"g","sessionId":"new","running":true}))
        .unwrap();
    until(|| r.inspect(|s| s.sessions["new"].summary.as_ref().unwrap()["running"] == true)).await;
    r.search_sessions_at_epoch(e, "query", "search-2")
        .await
        .unwrap();
    assert_eq!(f.reads(), 2);
    assert_eq!(f.journals.load(Ordering::SeqCst), 1);
    assert_eq!(f.connects.load(Ordering::SeqCst), 1);
    assert_eq!(f.mutations.load(Ordering::SeqCst), 0);
    r.stop().await.unwrap();
    until(|| f.active.load(Ordering::SeqCst) == 0).await;
}
#[tokio::test]
async fn invalid_pairs_do_not_partially_publish_or_retire_the_original_readers() {
    let f = Fixture::new(true);
    let r = f.runtime().await;
    let e = epoch(&r);
    for kind in [
        "malformed-projection",
        "generation",
        "first-delta",
        "empty-id",
        "duplicate-id",
        "workspace-shape",
    ] {
        let before = r.inspect(|s| serde_json::to_value(s).unwrap());
        let mut w = workspace();
        w["snapshot"]["pinnedSessionIds"] = json!(["s"]);
        let mut i = inventory(vec![row("new")]);
        match kind {
            "malformed-projection" => {
                i["snapshot"]["items"][0]["projections"] = json!({"asOfSeq":"bad","values":{}})
            }
            "generation" => w["generation"] = json!("other"),
            "empty-id" => i["snapshot"]["items"][0]["sessionId"] = json!(""),
            "duplicate-id" => i["snapshot"]["items"] = json!([row("new"), row("new")]),
            "workspace-shape" => w["snapshot"]["pinnedSessionIds"] = json!(["s", "s"]),
            _ => i["type"] = json!("upsert"),
        }
        {
            let mut s = f.state.lock().unwrap();
            s.workspace.push_back(Plan {
                frame: w,
                gate: None,
            });
            s.inventory.push_back(Plan {
                frame: i,
                gate: None,
            });
        }
        assert!(r.refresh_inventory_at_epoch(e).await.is_err());
        assert_eq!(
            r.inspect(|s| serde_json::to_value(s).unwrap()),
            before,
            "{kind}"
        );
        until(|| f.active.load(Ordering::SeqCst) == 2).await;
    }
    f.state.lock().unwrap().inventory_senders[0]
        .send(json!({"type":"status","generation":"g","sessionId":"s","running":true}))
        .unwrap();
    until(|| r.inspect(|s| s.sessions["s"].summary.as_ref().unwrap()["running"] == true)).await;
    assert_eq!(r.inspect(|s| s.connection.clone()), ConnectionState::Ready);
    r.stop().await.unwrap();
}
#[tokio::test]
async fn overlapping_refreshes_share_a_flight_and_wait_for_one_trailing_pass() {
    let f = Fixture::new(true);
    let r = f.runtime().await;
    let e = epoch(&r);
    let first = Arc::new(Semaphore::new(0));
    let second = Arc::new(Semaphore::new(0));
    {
        let mut s = f.state.lock().unwrap();
        s.inventory.push_back(Plan {
            frame: inventory(vec![row("s"), row("a")]),
            gate: Some(first.clone()),
        });
        s.inventory.push_back(Plan {
            frame: inventory(vec![row("s"), row("b")]),
            gate: Some(second.clone()),
        });
    }
    let one = {
        let r = r.clone();
        tokio::spawn(async move { r.refresh_inventory_at_epoch(e).await })
    };
    until(|| f.reads() == 2).await;
    let (sent, started) = tokio::sync::oneshot::channel();
    let two = {
        let r = r.clone();
        tokio::spawn(async move {
            let _ = sent.send(());
            r.refresh_inventory_at_epoch(e).await
        })
    };
    started.await.unwrap();
    r.call_at_epoch(e, "test.barrier", Value::Null, "barrier")
        .await
        .unwrap();
    first.add_permits(1);
    until(|| f.reads() == 3).await;
    assert!(!one.is_finished() && !two.is_finished());
    assert!(r.inspect(|s| s.sessions["a"].summary.is_some()));
    second.add_permits(1);
    one.await.unwrap().unwrap();
    two.await.unwrap().unwrap();
    assert_eq!(f.reads(), 3);
    assert!(r.inspect(|s| s.sessions["b"].summary.is_some() && s.sessions["a"].summary.is_none()));
    until(|| f.active.load(Ordering::SeqCst) == 2).await;
    r.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_retires_pending_refresh_and_new_epoch_can_refresh_without_reopening_other_work()
{
    let f = Fixture::new(true);
    let r = f.runtime().await;
    r.open_session("s").await.unwrap();
    let old = epoch(&r);
    {
        f.state.lock().unwrap().inventory.push_back(Plan {
            frame: inventory(vec![row("ghost")]),
            gate: Some(Arc::new(Semaphore::new(0))),
        });
    }
    let read = {
        let r = r.clone();
        tokio::spawn(async move { r.refresh_inventory_at_epoch(old).await })
    };
    until(|| f.reads() == 2).await;
    r.reconnect().await.unwrap();
    assert!(read.await.unwrap().is_err());
    let new = epoch(&r);
    assert_ne!(old, new);
    assert!(r.refresh_inventory_at_epoch(old).await.is_err());
    assert_eq!(f.reads(), 3);
    r.refresh_inventory_at_epoch(new).await.unwrap();
    assert_eq!(f.reads(), 4);
    assert!(r.inspect(|s| !s.sessions.contains_key("ghost")));
    assert_eq!(f.journals.load(Ordering::SeqCst), 2);
    until(|| f.active.load(Ordering::SeqCst) == 2).await;
    r.stop().await.unwrap();
}
#[tokio::test]
async fn session_only_unknown_hit_stays_unresolved_after_one_valid_refresh_without_fabricated_membership()
 {
    let f = Fixture::new(false);
    let r = f.runtime().await;
    f.state.lock().unwrap().search =
        json!({"items":[{"sessionId":"outside","snippet":"unknown"}],"hasMore":false});
    let result = r
        .search_sessions_at_epoch(epoch(&r), "query", "read")
        .await
        .unwrap();
    assert_eq!(result.items[0].session_id, "outside");
    assert_eq!(f.reads(), 2);
    assert!(f.state.lock().unwrap().workspace_senders.is_empty());
    assert!(r.inspect(|s| !s.sessions.contains_key("outside")));
    r.stop().await.unwrap();
}
