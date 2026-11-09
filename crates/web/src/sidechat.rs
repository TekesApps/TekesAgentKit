//! Web consumption of the existing shared SideChat owner. Handles only bind
//! browser callbacks to owner pointers; the controller owns mutation state.
use crate::*;
use tekes_agent_presentation::{SideChat, SideChatController, SideChatPhase};

#[derive(Clone)]
struct Pointer {
    parent: RunScope,
    scope: RunScope,
    epoch: u64,
}

pub(crate) struct SideChatOwner {
    controller: Arc<SideChatController>,
    handles: std::sync::Mutex<BTreeMap<String, Pointer>>,
}
impl SideChatOwner {
    pub(crate) fn new(runtime: Arc<SessionRuntime>) -> Self {
        Self {
            controller: Arc::new(SideChatController::new(runtime)),
            handles: std::sync::Mutex::new(BTreeMap::new()),
        }
    }
    fn record(&self, chat: &SideChat, epoch: u64) {
        let mut handles = self.handles.lock().expect("SideChat handles lock");
        if handles
            .values()
            .any(|pointer| pointer.scope == chat.scope && pointer.parent == chat.parent)
        {
            return;
        }
        handles.insert(
            uuid::Uuid::new_v4().to_string(),
            Pointer {
                parent: chat.parent.clone(),
                scope: chat.scope.clone(),
                epoch,
            },
        );
    }
    fn resolve(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        allow_closing: bool,
    ) -> Result<(String, SideChat, u64)> {
        let handle = string(payload, "handle")?;
        let parent_id = string(payload, "parentSessionId")?;
        let epoch = epoch(payload)?;
        let pointer = self
            .handles
            .lock()
            .expect("SideChat handles lock")
            .get(handle)
            .cloned()
            .ok_or_else(|| unsupported("SideChat handle is no longer owned"))?;
        require(
            pointer.parent.session_id == parent_id && pointer.epoch == epoch,
            "SideChat handle parent or connection changed",
        )?;
        shared.runtime.inspect(|state| {
            require(
                state.endpoint_id == pointer.scope.endpoint_id
                    && state.connection_epoch == epoch
                    && state.connection == ConnectionState::Ready,
                "SideChat connection changed; reconcile before continuing",
            )
        })?;
        let chat = self
            .controller
            .snapshot()
            .chats
            .get(&pointer.scope.session_id)
            .cloned()
            .ok_or_else(|| unsupported("SideChat is no longer owned"))?;
        require(
            chat.parent == pointer.parent && chat.scope == pointer.scope,
            "SideChat pointer changed",
        )?;
        require(
            chat.phase == SideChatPhase::Ready
                || allow_closing && chat.phase == SideChatPhase::Closing,
            "SideChat mutation is pending or unknown; reconcile first",
        )?;
        Ok((handle.to_owned(), chat, epoch))
    }
}

fn epoch(payload: &Value) -> Result<u64> {
    payload["connectionEpoch"]
        .as_u64()
        .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))
}
fn pointer_json(handle: &str, pointer: &Pointer, chat: &SideChat) -> Value {
    json!({"handle":handle,"parent":chat.parent,"scope":chat.scope,"title":chat.title,"phase":chat.phase,"error":chat.error,"connectionEpoch":pointer.epoch})
}

impl SharedAgent {
    pub(crate) fn side_chat_state(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let parent = RunScope::main(
            shared.runtime.snapshot().endpoint_id.clone(),
            string(payload, "parentSessionId")?,
        );
        parent.validate()?;
        let owner = &shared.side_chats;
        require(
            owner.controller.uses_runtime(&shared.runtime),
            "SideChat owner runtime mismatch",
        )?;
        let snapshot = owner.controller.snapshot();
        let current = shared.runtime.snapshot();
        // The controller records a successful fork before optional rename can
        // await. Rebuild its handle even if the original HTTP observer went away.
        for chat in snapshot.chats.values() {
            owner.record(chat, chat.connection_epoch);
        }
        let mut handles = owner.handles.lock().expect("SideChat handles lock");
        handles.retain(|_, pointer| {
            snapshot
                .chats
                .get(&pointer.scope.session_id)
                .is_some_and(|chat| chat.scope == pointer.scope && chat.parent == pointer.parent)
        });
        let chats: Vec<_> = handles
            .iter()
            .filter(|(_, pointer)| pointer.parent == parent)
            .filter_map(|(handle, pointer)| {
                snapshot
                    .chats
                    .get(&pointer.scope.session_id)
                    .map(|chat| pointer_json(handle, pointer, chat))
            })
            .collect();
        Ok(
            json!({"endpointId":current.endpoint_id,"parentSessionId":parent.session_id,"connectionEpoch":current.connection_epoch,
            "supported":owner.controller.support_snapshot() == Some(true),"creating":snapshot.is_creating_for(&parent),"unknownCreation":snapshot.unknown_creation_for(&parent),"chats":chats}),
        )
    }
    pub(crate) async fn side_chat_create(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let expected_epoch = epoch(payload)?;
        let parent = RunScope::main(
            shared.runtime.snapshot().endpoint_id.clone(),
            string(payload, "parentSessionId")?,
        );
        let title = shared.runtime.inspect(|state| {
            state
                .sessions
                .get(&parent.session_id)
                .and_then(|session| session.summary.as_ref())
                .and_then(|summary| {
                    summary["title"]
                        .as_str()
                        .or_else(|| summary["name"].as_str())
                })
                .map(str::to_owned)
        });
        let created = shared
            .side_chats
            .controller
            .create_at_epoch(parent, title.as_deref(), expected_epoch, rpc_id)
            .await?;
        shared.side_chats.record(&created, expected_epoch);
        self.side_chat_state(shared, payload)
    }
    pub(crate) async fn side_chat_close(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (handle, chat, epoch) = shared.side_chats.resolve(shared, payload, false)?;
        shared
            .side_chats
            .controller
            .close_owned_at_epoch(&chat, epoch, rpc_id)
            .await?;
        shared
            .side_chats
            .handles
            .lock()
            .expect("SideChat handles lock")
            .remove(&handle);
        let mut state = self.side_chat_state(shared, payload)?;
        state["removedHandles"] = json!([handle]);
        Ok(state)
    }
    pub(crate) async fn side_chat_reconcile(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let expected_epoch = epoch(payload)?;
        let parent_id = string(payload, "parentSessionId")?;
        shared
            .runtime
            .refresh_inventory_at_epoch(expected_epoch)
            .await?;
        let state = shared.runtime.snapshot();
        require(
            state.connection == ConnectionState::Ready
                && state.connection_epoch == expected_epoch
                && state.inventory_ready,
            "SideChat inventory is not current",
        )?;
        let snapshot = shared.side_chats.controller.snapshot();
        let mut removed = Vec::new();
        for chat in snapshot
            .chats
            .values()
            .filter(|chat| chat.parent.session_id == parent_id)
        {
            let exists = state
                .sessions
                .get(&chat.scope.session_id)
                .is_some_and(|session| session.summary.is_some());
            if shared.side_chats.controller.reconcile(chat, Ok(exists)) {
                let mut handles = shared
                    .side_chats
                    .handles
                    .lock()
                    .expect("SideChat handles lock");
                handles.retain(|handle, pointer| {
                    let keep = pointer.scope != chat.scope || pointer.parent != chat.parent;
                    if !keep {
                        removed.push(handle.clone());
                    }
                    keep
                });
            } else if exists {
                for pointer in shared
                    .side_chats
                    .handles
                    .lock()
                    .expect("SideChat handles lock")
                    .values_mut()
                    .filter(|pointer| pointer.scope == chat.scope && pointer.parent == chat.parent)
                {
                    pointer.epoch = expected_epoch;
                }
            }
        }
        let mut result = self.side_chat_state(shared, payload)?;
        result["removedHandles"] = json!(removed);
        Ok(result)
    }
    pub(crate) async fn side_chat_prompt(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (_, chat, epoch) = shared.side_chats.resolve(shared, payload, false)?;
        let request: PromptRequest = serde_json::from_value(json!({
            "sessionId":chat.scope.session_id,"mode":payload["mode"],"content":payload["content"],
            "clientTimeZone":payload.get("clientTimeZone")
        }))?;
        require(!request.content.is_empty(), "SideChat input is empty")?;
        let runtime = shared.runtime.clone();
        let validation_runtime = runtime.clone();
        let owner = shared.side_chats.controller.clone();
        let expected = chat.clone();
        runtime
            .call_optional_at_epoch_with_validation(
                epoch,
                "session.prompt",
                serde_json::to_value(request)?,
                rpc_id,
                move || {
                    require(
                        owner.snapshot().chats.get(&expected.scope.session_id) == Some(&expected),
                        "SideChat pointer changed before input admission",
                    )?;
                    validation_runtime.inspect(|state| {
                        require(
                            RunPresentation::read(state, &expected.scope, true).can_submit,
                            "SideChat cannot receive input",
                        )
                    })
                },
            )
            .await
    }
    pub(crate) async fn side_chat_cancel(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let (_, chat, epoch) = shared.side_chats.resolve(shared, payload, false)?;
        let runtime = shared.runtime.clone();
        let validation_runtime = runtime.clone();
        let owner = shared.side_chats.controller.clone();
        let expected = chat.clone();
        runtime
            .call_optional_at_epoch_with_validation(
                epoch,
                "session.cancel",
                json!({"sessionId":chat.scope.session_id}),
                rpc_id,
                move || {
                    require(
                        owner.snapshot().chats.get(&expected.scope.session_id) == Some(&expected),
                        "SideChat pointer changed before Stop admission",
                    )?;
                    validation_runtime.inspect(|state| {
                        require(
                            RunPresentation::read(state, &expected.scope, true).can_cancel,
                            "SideChat is not running",
                        )
                    })
                },
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;

    type Calls = Arc<Mutex<Vec<(String, Value)>>>;
    #[derive(Clone)]
    struct Fixture {
        host: HostDescription,
        calls: Calls,
        rows: Arc<Mutex<BTreeMap<String, Value>>>,
        inventory: tokio::sync::broadcast::Sender<Value>,
        journal: tokio::sync::broadcast::Sender<Value>,
        ephemeral: bool,
        running: Arc<AtomicBool>,
        fail_discard: Arc<AtomicBool>,
        malformed_fork: Arc<AtomicBool>,
    }
    fn summary(id: &str, ephemeral: bool) -> Value {
        json!({"sessionId":id,"title":id,"running":false,"blank":false,"updatedAt":1,"ephemeral":ephemeral})
    }
    #[async_trait]
    impl EndpointConnector for Fixture {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(self.clone()))
        }
    }
    #[async_trait]
    impl SessionEndpoint for Fixture {
        fn identity(&self) -> &str {
            "endpoint"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            let mut caps = EndpointCapabilities::methods(&[
                "session.fork",
                "session.discard",
                "session.cancel",
                "session.rename",
                "session.prompt",
            ]);
            caps.supports_ephemeral_fork = self.ephemeral;
            caps
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            match target {
                StreamTarget::SessionInventory => {
                    let rx = self.inventory.subscribe();
                    let rows:Vec<_> = self.rows.lock().unwrap().values().cloned().collect();
                    Ok(Box::pin(stream::once(async move {Ok(json!({"type":"baseline","generation":"g","snapshot":{"items":rows}}))}).chain(stream::unfold(rx,|mut rx|async move{rx.recv().await.ok().map(|value|(Ok(value),rx))}))))
                }
                StreamTarget::SessionJournal {address,..} => {
                    if address.session_id != "scratch" {
                        return Ok(Box::pin(stream::once(async move {Ok(json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}}))}).chain(stream::pending())));
                    }
                    let rx = self.journal.subscribe();
                    let entries = if address.session_id == "scratch" && self.running.load(Ordering::SeqCst) {
                        json!([{"event":{"type":"turn/start","seq":0,"time":1,"data":{"turn":1}}}])
                    } else {json!([])};
                    let seq = if entries.as_array().unwrap().is_empty(){-1}else{0};
                    Ok(Box::pin(stream::once(async move {Ok(json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":seq,"windowLimit":50,"entries":entries,"hasMoreBefore":false}}))}).chain(stream::unfold(rx,|mut rx|async move{rx.recv().await.ok().map(|value|(Ok(value),rx))}))))
                }
                StreamTarget::SessionControl => Ok(Box::pin(stream::once(async {Ok(json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}}))}).chain(stream::pending()))),
                StreamTarget::Actionables => Ok(Box::pin(stream::once(async {Ok(json!({"type":"reset","generation":"g"}))}).chain(stream::pending()))),
                _ => Err(unsupported("fixture target")),
            }
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
            self.calls
                .lock()
                .unwrap()
                .push((method.into(), payload.clone()));
            match method {
                "session.fork" => {
                    let row = summary("scratch", true);
                    self.rows
                        .lock()
                        .unwrap()
                        .insert("scratch".into(), row.clone());
                    let _ = self
                        .inventory
                        .send(json!({"type":"upsert","generation":"g","session":row}));
                    Ok(
                        json!({"sessionId":if self.malformed_fork.load(Ordering::SeqCst) {"parent"} else {"scratch"}}),
                    )
                }
                "session.cancel" => {
                    if self.running.swap(false, Ordering::SeqCst) {
                        let _=self.journal.send(json!({"type":"event","event":{"type":"turn/end","seq":1,"time":2,"data":{"turn":1,"reason":"aborted"}}}));
                    }
                    Ok(json!({"accepted":true}))
                }
                "session.discard" => {
                    if self.fail_discard.load(Ordering::SeqCst) {
                        return Err(Error::Timeout);
                    }
                    self.rows.lock().unwrap().remove("scratch");
                    let _ = self
                        .inventory
                        .send(json!({"type":"remove","generation":"g","sessionId":"scratch"}));
                    Ok(json!({"sessionId":"scratch"}))
                }
                _ => Ok(json!({"accepted":true})),
            }
        }
        async fn close(&self) {}
    }
    async fn setup(ephemeral: bool) -> (SharedAgent, Arc<SharedEndpoint>, Fixture) {
        let fixture=Fixture {
            host:serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"sidechat-fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false})).unwrap(),
            calls:Arc::new(Mutex::new(Vec::new())),rows:Arc::new(Mutex::new(BTreeMap::from([("parent".into(),summary("parent",false))]))),inventory:tokio::sync::broadcast::channel(16).0,journal:tokio::sync::broadcast::channel(16).0,
            ephemeral,running:Arc::new(AtomicBool::new(false)),fail_discard:Arc::new(AtomicBool::new(false)),malformed_fork:Arc::new(AtomicBool::new(false)),
        };
        let mut agent = SharedAgent::default();
        agent
            .register(
                "endpoint".into(),
                if ephemeral { "native" } else { "dsh" }.into(),
                "Fixture".into(),
                Arc::new(fixture.clone()),
            )
            .unwrap();
        let endpoint = agent.endpoint("endpoint").unwrap();
        endpoint
            .runtime
            .wait_ready(Duration::from_secs(2))
            .await
            .unwrap();
        endpoint.runtime.open_session("parent").await.unwrap();
        (agent, endpoint, fixture)
    }
    async fn create(agent: &SharedAgent, endpoint: &SharedEndpoint) -> Value {
        agent.request(&json!({"op":"call","endpoint":"endpoint","method":"agent.sideChat.create","rpcId":"fork-once","payload":{"parentSessionId":"parent","connectionEpoch":endpoint.runtime.snapshot().connection_epoch}})).await.unwrap()
    }
    fn payload(state: &Value) -> Value {
        json!({"parentSessionId":"parent","handle":state["chats"][0]["handle"],"connectionEpoch":state["chats"][0]["connectionEpoch"]})
    }

    #[tokio::test]
    async fn public_sidechat_owner_preserves_full_prompt_content_and_independent_history_leases() {
        let (agent, endpoint, fixture) = setup(true).await;
        let state = create(&agent, &endpoint).await;
        let first = endpoint
            .runtime
            .acquire_history_view("scratch")
            .await
            .unwrap();
        let second = endpoint
            .runtime
            .acquire_history_view("scratch")
            .await
            .unwrap();
        let mut request = payload(&state);
        request["mode"] = json!("queue");
        request["content"] = json!([{"type":"text","text":"body with compiled annotations"},{"type":"file","attachmentId":"owned-attachment"}]);
        request["clientTimeZone"] = json!("Asia/Shanghai");
        agent.request(&json!({"op":"call","endpoint":"endpoint","method":"agent.sideChat.prompt","rpcId":"send-once","payload":request})).await.unwrap();
        let calls = fixture.calls.lock().unwrap().clone();
        assert!(
            calls
                .iter()
                .any(|(method, request)| method == "session.fork"
                    && request["sessionId"] == "parent"
                    && request["ephemeral"] == true)
        );
        assert!(calls.iter().any(|(method,request)|method=="session.prompt" && request["sessionId"]=="scratch" && request["content"]==json!([{"type":"text","text":"body with compiled annotations"},{"type":"file","attachmentId":"owned-attachment"}]) && request["clientTimeZone"]=="Asia/Shanghai"));
        first.release().await.unwrap();
        assert!(endpoint.runtime.snapshot().sessions["scratch"].journal_ready);
        second.release().await.unwrap();
        assert!(
            !fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(method, _)| method == "session.cancel" || method == "session.discard")
        );
        assert_eq!(
            agent
                .side_chat_state(&endpoint, &json!({"parentSessionId":"parent"}))
                .unwrap()["chats"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        endpoint.runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_close_settles_running_child_before_discard_and_failed_discard_retains_pointer()
     {
        for fail in [false, true] {
            let (agent, endpoint, fixture) = setup(true).await;
            let state = create(&agent, &endpoint).await;
            fixture.running.store(true, Ordering::SeqCst);
            fixture.fail_discard.store(fail, Ordering::SeqCst);
            let result=agent.request(&json!({"op":"call","endpoint":"endpoint","method":"agent.sideChat.close","rpcId":"close-once","payload":payload(&state)})).await;
            let calls = fixture.calls.lock().unwrap().clone();
            let cancel = calls
                .iter()
                .position(|(method, _)| method == "session.cancel")
                .unwrap();
            let discard = calls
                .iter()
                .position(|(method, _)| method == "session.discard")
                .unwrap();
            assert!(cancel < discard);
            assert_eq!(
                calls
                    .iter()
                    .filter(|(method, _)| method == "session.discard")
                    .count(),
                1
            );
            assert!(!fixture.running.load(Ordering::SeqCst));
            if fail {
                assert!(matches!(result, Err(Error::Timeout)));
                let retained = agent
                    .side_chat_state(&endpoint, &json!({"parentSessionId":"parent"}))
                    .unwrap();
                assert_eq!(retained["chats"][0]["phase"], "unknown");
                assert_eq!(retained["chats"][0]["handle"], state["chats"][0]["handle"]);
            } else {
                let result = result.unwrap();
                assert_eq!(result["chats"], json!([]));
                assert_eq!(
                    result["removedHandles"],
                    json!([state["chats"][0]["handle"]])
                );
            }
            endpoint.runtime.stop().await.unwrap();
        }
    }

    #[tokio::test]
    async fn stale_epoch_foreign_parent_and_unsupported_harness_never_fork_or_discard() {
        let (agent, endpoint, fixture) = setup(true).await;
        let state = create(&agent, &endpoint).await;
        let mut foreign = payload(&state);
        foreign["parentSessionId"] = json!("other");
        assert!(
            agent
                .side_chat_close(&endpoint, &foreign, "foreign")
                .await
                .is_err()
        );
        endpoint.runtime.reconnect().await.unwrap();
        assert!(
            agent
                .side_chat_close(&endpoint, &payload(&state), "stale")
                .await
                .is_err()
        );
        assert!(
            agent
                .side_chat_create(
                    &endpoint,
                    &json!({"parentSessionId":"parent","connectionEpoch":state["connectionEpoch"]}),
                    "stale-create"
                )
                .await
                .is_err()
        );
        assert!(
            !fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(method, _)| method == "session.cancel" || method == "session.discard")
        );
        assert_eq!(
            fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "session.fork")
                .count(),
            1
        );
        endpoint.runtime.stop().await.unwrap();
        let (agent, endpoint, fixture) = setup(false).await;
        assert_eq!(
            agent
                .side_chat_state(&endpoint, &json!({"parentSessionId":"parent"}))
                .unwrap()["supported"],
            false
        );
        assert!(agent.side_chat_create(&endpoint,&json!({"parentSessionId":"parent","connectionEpoch":endpoint.runtime.snapshot().connection_epoch}),"dsh").await.is_err());
        assert!(fixture.calls.lock().unwrap().is_empty());
        endpoint.runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn reconcile_requires_fresh_inventory_and_clears_only_confirmed_absent_pointer() {
        let (agent, endpoint, fixture) = setup(true).await;
        let state = create(&agent, &endpoint).await;
        fixture.fail_discard.store(true, Ordering::SeqCst);
        let _ = agent
            .side_chat_close(&endpoint, &payload(&state), "unknown-close")
            .await;
        let request = json!({"parentSessionId":"parent","connectionEpoch":endpoint.runtime.snapshot().connection_epoch});
        let retained = agent
            .side_chat_reconcile(&endpoint, &request)
            .await
            .unwrap();
        assert_eq!(retained["chats"][0]["phase"], "ready");
        assert_eq!(retained["removedHandles"], json!([]));
        fixture.rows.lock().unwrap().remove("scratch");
        let absent = agent
            .side_chat_reconcile(&endpoint, &request)
            .await
            .unwrap();
        assert_eq!(absent["chats"], json!([]));
        assert_eq!(
            absent["removedHandles"],
            json!([state["chats"][0]["handle"]])
        );
        assert_eq!(
            fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "session.discard")
                .count(),
            1,
            "reconciliation never retries the mutation"
        );
        endpoint.runtime.stop().await.unwrap();
    }
    #[tokio::test]
    async fn malformed_fork_receipt_never_grants_discard_authority_over_the_parent() {
        let (agent, endpoint, fixture) = setup(true).await;
        fixture.malformed_fork.store(true, Ordering::SeqCst);
        let request = json!({"parentSessionId":"parent","connectionEpoch":endpoint.runtime.snapshot().connection_epoch});
        assert!(
            agent
                .side_chat_create(&endpoint, &request, "malformed-fork")
                .await
                .is_err()
        );
        let state = agent.side_chat_state(&endpoint, &request).unwrap();
        assert_eq!(state["chats"], json!([]));
        assert!(state["unknownCreation"].is_string());
        assert!(
            agent
                .side_chat_create(&endpoint, &request, "duplicate")
                .await
                .is_err()
        );
        assert_eq!(
            fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "session.fork")
                .count(),
            1
        );
        assert!(
            !fixture
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|(method, _)| method == "session.discard")
        );
        endpoint.runtime.stop().await.unwrap();
    }
}
