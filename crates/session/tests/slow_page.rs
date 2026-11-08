use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tekes_agent_contract::*;
use tekes_agent_session::*;
use tokio::sync::{Notify, Semaphore, broadcast};

#[derive(Clone)]
struct Fixture {
    entered: Arc<Notify>,
    release: Arc<Semaphore>,
    gate_first_page: bool,
    first_page: Arc<AtomicBool>,
    pages: Arc<AtomicUsize>,
    cancels: Arc<AtomicUsize>,
    replies: Arc<AtomicUsize>,
    journal: broadcast::Sender<Value>,
}

struct Endpoint {
    fixture: Fixture,
    host: HostDescription,
}

#[async_trait]
impl EndpointConnector for Fixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(Endpoint {
            fixture: self.clone(),
            host: serde_json::from_value(json!({
                "protocolVersion":4,
                "product":{"name":"slow-page-fixture","version":"1"},
                "capabilities":["session-journal","actionable-sync","session-control-sync","session-inventory-sync"],
                "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
            }))?,
        }))
    }
}

#[async_trait]
impl SessionEndpoint for Endpoint {
    fn identity(&self) -> &str {
        "slow-page-fixture"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }

    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        let (initial, updates): (Vec<Value>, FrameStream) = match target {
            StreamTarget::SessionInventory => (
                vec![
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"s","title":"session","running":false,"blank":false,"updatedAt":1}]}}),
                ],
                Box::pin(stream::pending()),
            ),
            StreamTarget::SessionControl => (
                vec![
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}}),
                ],
                Box::pin(stream::pending()),
            ),
            StreamTarget::Actionables => (
                vec![
                    json!({"type":"reset","generation":"g"}),
                    json!({"type":"upsert","generation":"g","actionable":{"id":"q","sessionId":"s","revision":7,"kind":"question","payload":{}}}),
                ],
                Box::pin(stream::pending()),
            ),
            StreamTarget::WorkspaceInventory => unreachable!(),
            StreamTarget::SessionConversation { .. } => unreachable!(),
            StreamTarget::SessionJournal { address, .. } => {
                assert_eq!(address.session_id, "s");
                let snapshot = json!({
                    "type":"snapshot",
                    "snapshot":{
                        "sessionId":"s","generation":"g","throughSequence":6,
                        "windowLimit":2,"entries":[entry(5),entry(6)],
                        "hasMoreBefore":true,"historyRevision":"rev-1"
                    }
                });
                let receiver = self.fixture.journal.subscribe();
                let updates = stream::unfold(receiver, |mut rx| async move {
                    let frame =
                        rx.recv()
                            .await
                            .map_err(|_| Error::Disconnected)
                            .and_then(|frame| {
                                if frame["fixtureRemoteError"] == true {
                                    Err(Error::Remote(json!({"code":"fixture-history-error"})))
                                } else {
                                    Ok(frame)
                                }
                            });
                    Some((frame, rx))
                });
                (vec![snapshot], Box::pin(updates))
            }
        };
        Ok(Box::pin(
            stream::iter(initial.into_iter().map(Ok)).chain(updates),
        ))
    }

    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.fixture.pages.fetch_add(1, Ordering::SeqCst);
        if self.fixture.gate_first_page && self.fixture.first_page.swap(false, Ordering::SeqCst) {
            self.fixture.entered.notify_one();
            self.fixture
                .release
                .acquire()
                .await
                .map_err(|_| Error::Disconnected)?
                .forget();
        }
        let before = request
            .before_sequence
            .expect("fixture requests earlier pages");
        let (entries, has_more_before) = if before == 5 {
            (vec![entry(3), entry(4)], true)
        } else if before == 3 {
            (vec![entry(1), entry(2)], false)
        } else {
            return Err(Error::Protocol("unexpected page cursor".into()));
        };
        Ok(HistoryPage {
            session_id: request.session_id,
            through_sequence: request.through_sequence,
            entries: entries
                .into_iter()
                .map(serde_json::from_value)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            has_more_before,
        })
    }

    async fn call(&self, method: &str, _: Value, _: &str) -> Result<Value> {
        if method == "session.cancel" {
            self.fixture.cancels.fetch_add(1, Ordering::SeqCst);
        }
        Ok(json!({"accepted":true}))
    }
    async fn respond(&self, _: &str, revision: i64, _: Value) -> Result<()> {
        assert_eq!(revision, 7);
        self.fixture.replies.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn close(&self) {}
}

fn entry(seq: i64) -> Value {
    json!({"event":{"type":"user/message","seq":seq,"time":seq as f64,"data":{"text":format!("message-{seq}")}}})
}

fn fixture(gate_first_page: bool) -> Fixture {
    let (journal, _) = broadcast::channel(16);
    Fixture {
        entered: Arc::new(Notify::new()),
        release: Arc::new(Semaphore::new(0)),
        gate_first_page,
        first_page: Arc::new(AtomicBool::new(true)),
        pages: Arc::new(AtomicUsize::new(0)),
        cancels: Arc::new(AtomicUsize::new(0)),
        replies: Arc::new(AtomicUsize::new(0)),
        journal,
    }
}

async fn ready_runtime(fixture: Fixture) -> Arc<SessionRuntime> {
    let runtime = Arc::new(SessionRuntime::spawn(
        "slow-page-fixture",
        Arc::new(fixture),
    ));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    runtime.open_session("s").await.unwrap();
    wait_for_journal(&runtime).await;
    runtime
}

async fn wait_for_journal(runtime: &SessionRuntime) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(|state| state.sessions.get("s").is_some_and(|s| s.journal_ready)) {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn slow_page_does_not_block_live_events_cancel_or_actionable_reply() {
    let fixture = fixture(true);
    let runtime = ready_runtime(fixture.clone()).await;
    let loading_runtime = runtime.clone();
    let loading = tokio::spawn(async move { loading_runtime.load_earlier("s").await });
    tokio::time::timeout(Duration::from_secs(2), fixture.entered.notified())
        .await
        .unwrap();

    fixture
        .journal
        .send(json!({"type":"event","generation":"g","sessionId":"s","entry":entry(7)}))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(|state| state.sessions["s"].tail_sequence == 7) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::try_join!(
            runtime.cancel("s", "cancel-1"),
            runtime.respond("q", 7, json!({"answer":"yes"}))
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(fixture.cancels.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.replies.load(Ordering::SeqCst), 1);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), runtime.load_earlier("s"))
            .await
            .unwrap()
            .is_err(),
        "same-session duplicate must be rejected"
    );

    fixture.release.add_permits(1);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), loading)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"].history_base_sequence),
        3
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"].tail_sequence),
        7
    );
    assert!(runtime.load_earlier("s").await.unwrap());
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"]
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>()),
        vec![1, 2, 3, 4, 5, 6, 7]
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_page_after_close_and_reopen_is_rejected_without_projection_pollution() {
    let fixture = fixture(true);
    let runtime = ready_runtime(fixture.clone()).await;
    let loading_runtime = runtime.clone();
    let loading = tokio::spawn(async move { loading_runtime.load_earlier("s").await });
    tokio::time::timeout(Duration::from_secs(2), fixture.entered.notified())
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), runtime.close_session("s"))
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), runtime.open_session("s"))
        .await
        .unwrap()
        .unwrap();
    wait_for_journal(&runtime).await;
    fixture.release.add_permits(1);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), loading)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"]
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>()),
        vec![5, 6]
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"].history_base_sequence),
        5
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn reconnect_cancels_old_page_without_merging_into_the_new_epoch() {
    let fixture = fixture(true);
    let runtime = ready_runtime(fixture.clone()).await;
    let epoch = runtime.inspect(|state| state.connection_epoch);
    let reader = runtime.clone();
    let loading = tokio::spawn(async move { reader.load_earlier("s").await });
    tokio::time::timeout(Duration::from_secs(2), fixture.entered.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), runtime.reconnect())
        .await
        .unwrap()
        .unwrap();
    wait_for_journal(&runtime).await;
    assert!(runtime.inspect(|state| state.connection_epoch > epoch));
    assert!(
        tokio::time::timeout(Duration::from_secs(2), loading)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    fixture.release.add_permits(1);
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"]
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>()),
        vec![5, 6]
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_page_from_failed_remote_reader_is_rejected() {
    let fixture = fixture(true);
    let runtime = ready_runtime(fixture.clone()).await;
    let loading_runtime = runtime.clone();
    let loading = tokio::spawn(async move { loading_runtime.load_earlier("s").await });
    tokio::time::timeout(Duration::from_secs(2), fixture.entered.notified())
        .await
        .unwrap();

    fixture
        .journal
        .send(json!({"fixtureRemoteError":true}))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(|state| state.sessions["s"].journal_error.is_some()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(runtime.inspect(|state| state.sessions["s"].journal_ready));

    fixture.release.add_permits(1);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), loading)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["s"]
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>()),
        vec![5, 6]
    );
    runtime.stop().await.unwrap();
}
