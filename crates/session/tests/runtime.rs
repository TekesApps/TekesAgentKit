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
            StreamTarget::WorkspaceInventory => unreachable!(),
            StreamTarget::SessionInventory => {
                let baseline = json!({"type":"baseline","generation":"g","snapshot":{"items":[]}});
                if self.attempt == 0 {
                    return Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(
                        stream::once(async {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Err(Error::Disconnected)
                        }),
                    )));
                }
                baseline
            }
            StreamTarget::SessionControl => {
                json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
            }
            StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
            StreamTarget::SessionConversation { .. } => unreachable!(),
            StreamTarget::SessionJournal { address, .. } => {
                let snapshot = json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":0,"windowLimit":50,"entries":[{"event":{"type":"user/message","seq":0,"time":1,"data":{"text":"same identity"}}}],"hasMoreBefore":false}});
                if address.session_id == "lost" && self.attempt > 0 {
                    return Err(unsupported("session file missing"));
                }
                if address.session_id == "broken" {
                    return Ok(Box::pin(stream::once(async { Ok(snapshot) }).chain(
                        stream::once(async {
                            Err(Error::Protocol("invalid session delta".into()))
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

struct InteractiveConnector(Arc<tokio::sync::Notify>);
struct InteractiveEndpoint {
    endpoint: Endpoint,
    answer: Arc<tokio::sync::Notify>,
}
#[async_trait]
impl EndpointConnector for InteractiveConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let base = Connector {
            connects: Arc::new(AtomicUsize::new(1)),
            prompts: Arc::new(AtomicUsize::new(0)),
        };
        let host = base.connect().await?.host().clone();
        Ok(Arc::new(InteractiveEndpoint {
            endpoint: Endpoint {
                attempt: 1,
                host,
                prompts: base.prompts,
            },
            answer: self.0.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for InteractiveEndpoint {
    fn identity(&self) -> &str {
        "k"
    }
    fn host(&self) -> &HostDescription {
        &self.endpoint.host
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        if target == StreamTarget::Actionables {
            Ok(Box::pin(stream::iter(vec![Ok(json!({"type":"reset","generation":"g"})),Ok(json!({"type":"upsert","generation":"g","actionable":{"id":"q","sessionId":"s","revision":0,"kind":"question","payload":{}}}))]).chain(stream::pending())))
        } else {
            self.endpoint.follow(target).await
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }
    async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
        self.answer.notified().await;
        Ok(json!({"accepted":true}))
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        self.answer.notify_one();
        Ok(())
    }
    async fn close(&self) {}
}
#[tokio::test]
async fn pending_mutation_does_not_block_actionable_response() {
    let runtime = SessionRuntime::spawn(
        "k",
        Arc::new(InteractiveConnector(Arc::new(tokio::sync::Notify::new()))),
    );
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let mut changes = runtime.subscribe();
    while !changes.inspect(|s| s.actionables.contains_key("q")) {
        changes.changed().await.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::try_join!(runtime.prompt("s", "/question", "q1"), async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            runtime.respond("q", 0, json!({"value":"yes"})).await
        })
    })
    .await
    .unwrap()
    .unwrap();
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn registry_routes_same_session_id_only_with_explicit_endpoint_identity() {
    let mut registry = SessionEndpointRegistry::new();
    for name in ["native", "pi", "dsh"] {
        registry
            .register(
                name,
                Arc::new(FailingConnector {
                    attempts: Arc::new(AtomicUsize::new(0)),
                }),
            )
            .unwrap();
        let address = SessionAddress {
            endpoint_id: name.into(),
            session_id: "same-id".into(),
        };
        assert_eq!(
            registry
                .session(&address)
                .unwrap()
                .inspect(|s| s.endpoint_id.clone()),
            name
        );
    }
    assert!(
        registry
            .register(
                "pi",
                Arc::new(FailingConnector {
                    attempts: Arc::new(AtomicUsize::new(0))
                })
            )
            .is_err()
    );
    assert!(
        registry
            .session(&SessionAddress {
                endpoint_id: "missing".into(),
                session_id: "same-id".into()
            })
            .is_err()
    );
    registry.stop().await.unwrap();
}

struct Services;
#[async_trait]
impl HostServices for Services {
    fn methods(&self) -> std::collections::BTreeSet<String> {
        ["host.probe".to_owned()].into()
    }
    async fn call(&self, id: &str, method: &str, payload: Value, key: &str) -> Result<Value> {
        Ok(json!({"endpoint":id,"method":method,"payload":payload,"rpcId":key}))
    }
}
#[tokio::test]
async fn explicit_host_services_add_only_their_declared_authority() {
    let connector = Connector {
        connects: Arc::new(AtomicUsize::new(1)),
        prompts: Arc::new(AtomicUsize::new(0)),
    };
    let runtime = SessionRuntime::spawn(
        "k",
        Arc::new(HostedConnector {
            endpoint: Arc::new(connector),
            services: Arc::new(Services),
        }),
    );
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    assert!(runtime.capabilities().await.unwrap().supports("host.probe"));
    let result = runtime
        .call_optional("host.probe", json!({"input":7}), "stable")
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({"endpoint":"k","method":"host.probe","payload":{"input":7},"rpcId":"stable"})
    );
    assert!(
        runtime
            .call_optional("host.not-advertised", json!({}), "no")
            .await
            .is_err()
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn discarded_open_session_retires_its_stream_without_reconnecting_other_sessions() {
    let connector = Connector {
        connects: Arc::new(AtomicUsize::new(1)),
        prompts: Arc::new(AtomicUsize::new(0)),
    };
    let runtime = SessionRuntime::spawn("k", Arc::new(connector.clone()));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    runtime.open_session("scratch").await.unwrap();
    runtime
        .call("session.discard", json!({"sessionId":"scratch"}), "discard")
        .await
        .unwrap();
    assert!(!runtime.snapshot().sessions["scratch"].journal_ready);
    assert_eq!(runtime.snapshot().connection, ConnectionState::Ready);
    assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn missing_pointer_and_exhausted_session_stream_do_not_reconnect_healthy_sessions() {
    let connector = Connector {
        connects: Arc::new(AtomicUsize::new(0)),
        prompts: Arc::new(AtomicUsize::new(0)),
    };
    let runtime = SessionRuntime::spawn("k", Arc::new(connector.clone()));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    runtime.open_session("lost").await.unwrap();
    runtime.open_session("healthy").await.unwrap();
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if runtime.inspect(|s| {
                s.connection == ConnectionState::Ready
                    && s.sessions
                        .get("lost")
                        .is_some_and(|s| s.journal_error.is_some())
                    && s.sessions.get("healthy").is_some_and(|s| s.journal_ready)
            }) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
    let _ = runtime.open_session("broken").await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime.inspect(|s| {
                s.sessions
                    .get("broken")
                    .is_some_and(|s| s.journal_error.is_some() && !s.journal_ready)
            }) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(connector.connects.load(Ordering::SeqCst), 2);
    assert!(
        runtime.inspect(
            |s| s.connection == ConnectionState::Ready && s.sessions["healthy"].journal_ready
        )
    );
    // Explicit retry still reaches the failed pointer rather than treating it as already open.
    assert!(runtime.open_session("lost").await.is_err());
    runtime.stop().await.unwrap();
}

#[derive(Clone)]
struct NativeWindowConnector {
    requests: Arc<std::sync::Mutex<Vec<usize>>>,
    custom_entries: bool,
}
struct NativeWindowEndpoint {
    base: Endpoint,
    requests: Arc<std::sync::Mutex<Vec<usize>>>,
    custom_entries: bool,
    limit: AtomicUsize,
    frames: tokio::sync::broadcast::Sender<Value>,
}
fn native_window_frame(kind: &str, limit: usize, custom_entries: bool) -> Value {
    // Model Pi's behavior: the window limit counts messages, while custom
    // entries remain in the ancestry and are included alongside those messages.
    let count = limit.min(120);
    let first_message = 120 - count;
    let mut entries = Vec::new();
    for i in first_message..120 {
        let message_id = format!("entry-{i}");
        entries.push(json!({
            "id": message_id,
            "parentId": if i == 0 {
                Value::Null
            } else if custom_entries {
                json!(format!("custom-{}", i - 1))
            } else {
                json!(format!("entry-{}", i - 1))
            },
            "type": "message",
            "message": {"role":"user","content":format!("message {i}"),"timestamp":i}
        }));
        if custom_entries {
            entries.push(json!({
                "id": format!("custom-{i}"),
                "parentId": format!("entry-{i}"),
                "type": "custom",
                "customType": "test-metadata",
                "data": {"messageIndex":i}
            }));
        }
    }
    let leaf_id = if custom_entries {
        "custom-119"
    } else {
        "entry-119"
    };
    json!({"type":kind,"generation":"g","snapshot":{"sessionId":"s","generation":"g","leafId":leaf_id,"entries":entries,"hasMoreBefore":count<120,"isStreaming":false,"state":{}}})
}
#[async_trait]
impl EndpointConnector for NativeWindowConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let host=serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"native-window","version":"test"},"capabilities":["session-journal","actionable-sync","session-control-sync","session-inventory-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false})).unwrap();
        Ok(Arc::new(NativeWindowEndpoint {
            base: Endpoint {
                attempt: 1,
                host,
                prompts: Arc::new(AtomicUsize::new(0)),
            },
            requests: self.requests.clone(),
            custom_entries: self.custom_entries,
            limit: AtomicUsize::new(50),
            frames: tokio::sync::broadcast::channel(16).0,
        }))
    }
}
#[async_trait]
impl SessionEndpoint for NativeWindowEndpoint {
    fn identity(&self) -> &str {
        "native-window"
    }
    fn host(&self) -> &HostDescription {
        &self.base.host
    }
    fn content_kind(&self) -> SessionContentKind {
        SessionContentKind::NativeConversation
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        if let StreamTarget::SessionConversation { max_messages, .. } = target {
            self.requests.lock().unwrap().push(max_messages);
            self.limit.store(max_messages, Ordering::SeqCst);
            let receiver = self.frames.subscribe();
            let updates = stream::unfold(receiver, |mut receiver| async move {
                Some((
                    receiver.recv().await.map_err(|_| Error::Disconnected),
                    receiver,
                ))
            });
            let custom_entries = self.custom_entries;
            Ok(Box::pin(
                stream::once(async move {
                    Ok(native_window_frame(
                        "conversation",
                        max_messages,
                        custom_entries,
                    ))
                })
                .chain(updates),
            ))
        } else {
            self.base.follow(target).await
        }
    }
    async fn load_earlier_conversation(&self, _: &str, count: usize) -> Result<bool> {
        let previous = self.limit.fetch_add(count, Ordering::SeqCst);
        self.frames
            .send(native_window_frame(
                "conversation-update",
                previous + count,
                self.custom_entries,
            ))
            .unwrap();
        Ok(true)
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }
    async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        unreachable!()
    }
    async fn close(&self) {}
}
#[tokio::test]
async fn reconnect_refetches_the_loaded_native_window_instead_of_shrinking_it() {
    let requests = Arc::new(std::sync::Mutex::new(vec![]));
    let runtime = SessionRuntime::spawn(
        "native-window",
        Arc::new(NativeWindowConnector {
            requests: requests.clone(),
            custom_entries: false,
        }),
    );
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime.open_session("s").await.unwrap();
    assert!(runtime.load_earlier("s").await.unwrap());
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .len()),
        100
    );
    runtime.reconnect().await.unwrap();
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if changes.inspect(|s| s.connection_epoch != old_epoch && s.sessions["s"].journal_ready)
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 100]);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .len()),
        100
    );
    // Close/open uses add_journal too and must retain the same paged window.
    runtime.close_session("s").await.unwrap();
    runtime.open_session("s").await.unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 100, 100]);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .filter(|entry| entry["type"] == "message")
            .count()),
        100
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn reconnect_and_reopen_count_native_messages_with_custom_entries() {
    let requests = Arc::new(std::sync::Mutex::new(vec![]));
    let runtime = SessionRuntime::spawn(
        "native-window",
        Arc::new(NativeWindowConnector {
            requests: requests.clone(),
            custom_entries: true,
        }),
    );
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime.open_session("s").await.unwrap();
    let initial_ids: Vec<String> = (70..120)
        .flat_map(|i| [format!("entry-{i}"), format!("custom-{i}")])
        .collect();
    let initial = runtime.inspect(|s| s.sessions["s"].native_conversation.clone().unwrap());
    assert_eq!(initial.entries.len(), 100);
    assert_eq!(
        initial
            .entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        initial_ids.clone()
    );
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    let mut changes = runtime.subscribe();
    runtime.reconnect().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if changes.inspect(|s| s.connection_epoch != old_epoch && s.sessions["s"].journal_ready)
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 50]);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .clone()),
        initial.entries
    );
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()),
        initial_ids
    );
    runtime.close_session("s").await.unwrap();
    runtime.open_session("s").await.unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 50, 50]);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .clone()),
        initial.entries
    );
    assert!(runtime.load_earlier("s").await.unwrap());
    let paged_entries = runtime.inspect(|s| {
        s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .clone()
    });
    let expected_ids: Vec<String> = (20..120)
        .flat_map(|i| [format!("entry-{i}"), format!("custom-{i}")])
        .collect();
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .len()),
        200
    );
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    let mut changes = runtime.subscribe();
    runtime.reconnect().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if changes.inspect(|s| s.connection_epoch != old_epoch && s.sessions["s"].journal_ready)
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 50, 50, 100]);
    let snapshot = runtime.inspect(|s| s.sessions["s"].native_conversation.clone().unwrap());
    assert_eq!(snapshot.entries, paged_entries);
    assert_eq!(
        snapshot
            .entries
            .iter()
            .filter(|entry| entry["type"] == "message")
            .count(),
        100
    );
    assert_eq!(snapshot.entries.len(), 200);
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        expected_ids
    );
    runtime.close_session("s").await.unwrap();
    runtime.open_session("s").await.unwrap();
    assert_eq!(*requests.lock().unwrap(), vec![50, 50, 50, 100, 100]);
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .len()),
        200
    );
    assert_eq!(
        runtime.inspect(|s| s.sessions["s"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .clone()),
        snapshot.entries
    );
    runtime.stop().await.unwrap();
}

#[derive(Clone)]
struct ActiveRunConnector {
    frames: tokio::sync::broadcast::Sender<Value>,
    follows: Arc<AtomicUsize>,
}
struct ActiveRunEndpoint {
    host: HostDescription,
    frames: tokio::sync::broadcast::Sender<Value>,
    follows: Arc<AtomicUsize>,
}
fn active_run_frame(streaming: bool, event: Option<Value>) -> Value {
    if let Some(event) = event {
        return json!({"type":"native-event","sessionId":"active-run-session",
            "generation":"active-run-generation","event":event});
    }
    json!({
        "type": "conversation-update",
        "generation": "active-run-generation",
        "snapshot": {
            "sessionId": "active-run-session",
            "generation": "active-run-generation",
            "leafId": if streaming {"user-1"} else {"assistant-1"},
            "entries": if streaming {
                vec![json!({"id":"user-1","type":"message","message":{"role":"user","content":"prompt"}})]
            } else {
                vec![
                    json!({"id":"user-1","type":"message","message":{"role":"user","content":"prompt"}}),
                    json!({"id":"assistant-1","type":"message","message":{"role":"assistant","content":"complete"}})
                ]
            },
            "hasMoreBefore": false,
            "isStreaming": streaming,
            "state": {}
        }
    })
}
#[async_trait]
impl EndpointConnector for ActiveRunConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let host = serde_json::from_value(json!({
            "protocolVersion":4,
            "product":{"name":"active-run","version":"test"},
            "capabilities":["session-journal","actionable-sync","session-control-sync","session-inventory-sync"],
            "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
        })).unwrap();
        Ok(Arc::new(ActiveRunEndpoint {
            host,
            frames: self.frames.clone(),
            follows: self.follows.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for ActiveRunEndpoint {
    fn identity(&self) -> &str {
        "active-run"
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn content_kind(&self) -> SessionContentKind {
        SessionContentKind::NativeConversation
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        match target {
            StreamTarget::SessionInventory => Ok(Box::pin(stream::once(async {
                Ok(json!({"type":"baseline","generation":"g","snapshot":{"items":[]}}))
            }).chain(stream::pending()))),
            StreamTarget::SessionControl => Ok(Box::pin(stream::once(async {
                Ok(json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}}))
            }).chain(stream::pending()))),
            StreamTarget::Actionables => Ok(Box::pin(stream::once(async {
                Ok(json!({"type":"reset","generation":"g"}))
            }).chain(stream::pending()))),
            StreamTarget::SessionConversation { .. } => {
                self.follows.fetch_add(1, Ordering::SeqCst);
                let receiver = self.frames.subscribe();
                let updates = stream::unfold(receiver, |mut receiver| async move {
                    Some((receiver.recv().await.map_err(|_| Error::Disconnected), receiver))
                });
                Ok(Box::pin(stream::once(async {
                    { let mut baseline = active_run_frame(true, None); baseline["type"] = json!("conversation"); Ok(baseline) }
                }).chain(updates)))
            }
            StreamTarget::WorkspaceInventory | StreamTarget::SessionJournal { .. } => unreachable!(),
        }
    }
    async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
        unreachable!()
    }
    async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
        unreachable!()
    }
    async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn close(&self) {}
}

#[tokio::test]
async fn native_active_run_journal_survives_last_observer_detach_and_reopen() {
    let (frames, _) = tokio::sync::broadcast::channel(8);
    let follows = Arc::new(AtomicUsize::new(0));
    let runtime = Arc::new(SessionRuntime::spawn(
        "active-run",
        Arc::new(ActiveRunConnector {
            frames: frames.clone(),
            follows: follows.clone(),
        }),
    ));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();

    let observer = runtime.acquire_history("active-run-session").await.unwrap();
    assert_eq!(follows.load(Ordering::SeqCst), 1);
    let delta = |text: &str| {
        json!({
            "type":"message_update",
            "agentkitStreamId":"active-run-stream",
            "message":{"role":"assistant","content":text},
            "assistantMessageEvent":{"type":"text_delta"}
        })
    };
    frames
        .send(active_run_frame(true, Some(delta("prefix one"))))
        .unwrap();
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if changes.inspect(|s| s.sessions["active-run-session"].native_events.len() == 1) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();

    observer.release().await.unwrap();
    frames
        .send(active_run_frame(true, Some(delta("prefix two"))))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state = changes.inspect(|s| s.sessions["active-run-session"].clone());
            if state.native_events.len() == 2 {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        runtime.inspect(|s| {
            s.sessions["active-run-session"].native_events[1]["message"]["content"]
                .as_str()
                .map(str::to_owned)
        }),
        Some("prefix two".to_owned())
    );

    let reopened = runtime.acquire_history("active-run-session").await.unwrap();
    assert_eq!(
        follows.load(Ordering::SeqCst),
        1,
        "reopen reuses the live journal instead of resetting its projection"
    );
    assert_eq!(
        runtime.inspect(|s| s.sessions["active-run-session"].native_events.len()),
        2
    );

    frames.send(active_run_frame(false, None)).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if changes.inspect(|s| !s.sessions["active-run-session"].run_open) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(
        runtime.inspect(|s| s.sessions["active-run-session"].journal_ready),
        "the reopened observer still owns the completed transcript"
    );
    reopened.release().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if !changes.inspect(|s| s.sessions["active-run-session"].journal_ready) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(follows.load(Ordering::SeqCst), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn native_detached_completed_run_retires_its_journal() {
    let (frames, _) = tokio::sync::broadcast::channel(8);
    let follows = Arc::new(AtomicUsize::new(0));
    let runtime = Arc::new(SessionRuntime::spawn(
        "active-run",
        Arc::new(ActiveRunConnector {
            frames: frames.clone(),
            follows: follows.clone(),
        }),
    ));
    runtime.wait_ready(Duration::from_secs(2)).await.unwrap();
    let first = runtime.acquire_history("active-run-session").await.unwrap();
    let second = runtime.acquire_history("active-run-session").await.unwrap();
    first.release().await.unwrap();
    assert!(runtime.inspect(|s| s.sessions["active-run-session"].journal_ready));
    second.release().await.unwrap();
    assert!(runtime.inspect(|s| s.sessions["active-run-session"].journal_ready));
    frames.send(active_run_frame(false, None)).unwrap();
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if changes.inspect(|s| !s.sessions["active-run-session"].journal_ready) {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        runtime.inspect(|s| s.sessions["active-run-session"]
            .native_conversation
            .as_ref()
            .unwrap()
            .entries
            .len()),
        2
    );
    assert_eq!(follows.load(Ordering::SeqCst), 1);
    runtime.stop().await.unwrap();
}
