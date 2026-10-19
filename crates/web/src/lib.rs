//! Shared AgentKit service. Hosts own its authentication, listener and process lifetime.
//! A disconnected view drops only its history lease; it never stops a runtime or Harness.
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use tekes_agent_contract::*;
use tekes_agent_presentation::{RunPresentation, RunScope, TranscriptSnapshot};
use tekes_agent_session::{ConnectionState, MemoryProjectionStore, SessionRuntime};
use tokio::sync::mpsc;

fn active_inventory(state: &MemoryProjectionStore) -> Vec<Value> {
    state
        .sessions
        .iter()
        .filter(|(id, _)| !state.archived_session_ids.contains(*id))
        .filter_map(|(id, session)| {
            session
                .summary
                .as_ref()
                .map(|summary| json!({"sessionId":id,"summary":summary}))
        })
        .collect()
}

/// Typed public read projection; the durable original text is retained for other consumers.
pub fn present_annotations(transcript: &mut Value) {
    if let Some(items) = transcript.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if item["kind"] == "user"
                && let Some(presentation) = item["text"]
                    .as_str()
                    .and_then(tekes_agent_presentation::annotation_input_presentation)
            {
                item["inputPresentation"] =
                    serde_json::to_value(presentation).expect("annotation presentation");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tekes_agent_session::SessionProjection;

    struct Offline;
    #[async_trait]
    impl EndpointConnector for Offline {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Err(Error::Disconnected)
        }
    }
    #[tokio::test]
    async fn public_annotation_compilation_is_local_and_typed_not_a_harness_rpc() {
        let mut agent = SharedAgent::default();
        agent
            .register(
                "offline".into(),
                "tekes".into(),
                "Offline".into(),
                Arc::new(Offline),
            )
            .unwrap();
        let value = agent.request(&json!({"op":"call","endpoint":"offline","method":"composer.compileAnnotations","rpcId":"stable","payload":{"text":"body","annotations":[{"id":"a","source":"Reply","excerpt":"quoted","comment":"explain"}]}})).await.unwrap();
        let compiled: AnnotationCompilation = serde_json::from_value(value).unwrap();
        let input =
            tekes_agent_presentation::annotation_input_presentation(&compiled.text).unwrap();
        assert_eq!(input.text, "body");
        assert_eq!(input.annotations[0].excerpt, "quoted");
        let mut transcript = json!({"items":[{"id":"user","kind":"user","text":compiled.text},{"id":"assistant","kind":"assistant","text":compiled.text}]});
        present_annotations(&mut transcript);
        assert_eq!(transcript["items"][0]["inputPresentation"]["text"], "body");
        assert!(transcript["items"][1].get("inputPresentation").is_none());
        let mut absent = Value::Null;
        present_annotations(&mut absent);
        assert!(absent.is_null());
    }

    #[test]
    fn archived_workspace_authority_excludes_retained_summary_without_erasing_history() {
        let mut store = MemoryProjectionStore::new("dsh");
        let archived = SessionProjection {
            summary: Some(json!({"sessionId":"archived","title":"Retained"})),
            ..Default::default()
        };
        let active = SessionProjection {
            summary: Some(json!({"sessionId":"active"})),
            ..Default::default()
        };
        store.sessions.insert("archived".into(), archived);
        store.sessions.insert("active".into(), active);
        store.archived_session_ids.push("archived".into());
        assert_eq!(
            active_inventory(&store),
            vec![json!({"sessionId":"active","summary":{"sessionId":"active"}})]
        );
        assert!(store.sessions["archived"].summary.is_some());
        store.archived_session_ids.clear();
        assert_eq!(active_inventory(&store).len(), 2);
    }
}

struct TrackedConnector {
    inner: Arc<RwLock<Arc<dyn EndpointConnector>>>,
    endpoint: Arc<RwLock<Option<Arc<dyn SessionEndpoint>>>>,
}
#[async_trait]
impl EndpointConnector for TrackedConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let connector = self.inner.read().expect("connector lock").clone();
        let endpoint = connector.connect().await?;
        *self.endpoint.write().expect("endpoint lock") = Some(endpoint.clone());
        Ok(endpoint)
    }
}
pub struct SharedEndpoint {
    pub kind: String,
    pub name: String,
    pub runtime: Arc<SessionRuntime>,
    connector: Arc<RwLock<Arc<dyn EndpointConnector>>>,
    endpoint: Arc<RwLock<Option<Arc<dyn SessionEndpoint>>>>,
}
impl SharedEndpoint {
    fn connected(&self) -> Result<Arc<dyn SessionEndpoint>> {
        self.endpoint
            .read()
            .expect("endpoint lock")
            .clone()
            .ok_or(Error::Disconnected)
    }
    pub fn description(&self) -> Value {
        let state = self.runtime.snapshot();
        let mut description = json!({"id":state.endpoint_id,"kind":self.kind,"name":self.name,
            "connection":match &state.connection {ConnectionState::Connecting=>json!("connecting"),ConnectionState::Ready=>json!("ready"),ConnectionState::Failed(e)=>json!({"failed":e}),ConnectionState::Stopped=>json!("stopped")},"host":state.host,
            "capabilities":self.connected().ok().map(|e| e.capabilities())});
        if let Some(methods) = description["capabilities"]["methods"].as_array_mut() {
            methods.push(json!("composer.compileAnnotations"));
            methods.push(json!("thread.actions"));
            methods.push(json!("thread.performAction"));
        }
        description
    }
}
#[derive(Default)]
pub struct SharedAgent {
    endpoints: BTreeMap<String, Arc<SharedEndpoint>>,
    thread_action_host: Option<Arc<dyn ThreadActionHost>>,
    session_url_scheme: Option<String>,
    thread_preferences: Option<Arc<tekes_agent_presentation::ThreadPreferences>>,
    inventory_changes: InventoryChanges,
}
struct InventoryChanges(tokio::sync::watch::Sender<()>);
impl Default for InventoryChanges {
    fn default() -> Self {
        Self(tokio::sync::watch::channel(()).0)
    }
}
impl SharedAgent {
    /// Host adapters call this after an external inventory-affecting local change.
    /// It publishes existing authority again; it never edits a session or a pin.
    pub fn invalidate_thread_inventory(&self) {
        self.inventory_changes.0.send_replace(());
    }
    pub fn set_session_url_scheme(&mut self, scheme: &str) -> Result<()> {
        require(
            !scheme.is_empty()
                && scheme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')),
            "invalid host session URL scheme",
        )?;
        self.session_url_scheme = Some(scheme.into());
        Ok(())
    }
    pub fn open_thread_preferences(&mut self, path: &std::path::Path, scope: &str) -> Result<()> {
        let store = Arc::new(tekes_agent_presentation::ThreadPreferences::open(
            path, scope,
        )?);
        self.thread_action_host = Some(store.clone());
        self.thread_preferences = Some(store);
        self.invalidate_thread_inventory();
        Ok(())
    }
    fn inventory(&self, shared: &SharedEndpoint, state: &MemoryProjectionStore) -> Value {
        let sessions: Vec<Value> = active_inventory(state)
            .into_iter()
            .map(|mut row| {
                let id = row["sessionId"]
                    .as_str()
                    .expect("inventory identity")
                    .to_owned();
                if self
                    .thread_action_host
                    .as_ref()
                    .and_then(|host| host.pin_state(&state.endpoint_id, &id))
                    .is_some_and(|pin| pin.pinned)
                {
                    row["pin"] = json!(ThreadPinPresentation { pinned: true });
                }
                row["activity"] =
                    serde_json::to_value(tekes_agent_presentation::thread_activity(state, &id))
                        .expect("thread activity");
                row
            })
            .collect();
        json!({"description":shared.description(),"sessions":sessions})
    }
    pub fn install_thread_action_host(&mut self, host: Arc<dyn ThreadActionHost>) {
        self.thread_action_host = Some(host);
        self.invalidate_thread_inventory();
    }
    pub fn thread_actions(
        &self,
        shared: &SharedEndpoint,
        id: &str,
    ) -> Result<ThreadActionsPresentation> {
        let state = shared.runtime.snapshot();
        let session = state
            .sessions
            .get(id)
            .ok_or_else(|| unsupported("unknown thread"))?;
        let endpoint = shared.connected().ok();
        let capabilities = endpoint
            .as_ref()
            .map(|e| e.capabilities())
            .unwrap_or_default();
        let host_methods = self
            .thread_action_host
            .as_ref()
            .map(|h| h.methods())
            .unwrap_or_default();
        let pin = self
            .thread_action_host
            .as_ref()
            .and_then(|h| h.pin_state(&state.endpoint_id, id));
        let summary = session.summary.as_ref();
        let archived = state.archived_session_ids.iter().any(|s| s == id)
            || summary.is_some_and(|s| s["status"] == "archived");
        let running = session.run_open
            || !session.queue.is_empty()
            || summary.is_some_and(|s| {
                s["running"] == true
                    || matches!(
                        s["status"].as_str(),
                        Some("running" | "waitingForInput" | "cancelling")
                    )
            });
        Ok(tekes_agent_presentation::thread_actions(
            &tekes_agent_presentation::ThreadActionContext {
                endpoint: &state.endpoint_id,
                session_id: id,
                kind: &shared.kind,
                url_scheme: self.session_url_scheme.as_deref().unwrap_or("agent"),
                capabilities: &capabilities,
                ready: state.connection == ConnectionState::Ready && endpoint.is_some(),
                running,
                archived,
                pin: pin.as_ref(),
                host_methods: &host_methods,
            },
        ))
    }
    fn present_turn_actions(
        &self,
        shared: &SharedEndpoint,
        id: &str,
        transcript: &mut TranscriptSnapshot,
    ) -> Result<()> {
        let caps = shared
            .connected()
            .ok()
            .map(|e| e.capabilities())
            .unwrap_or_default();
        let fork = self
            .thread_actions(shared, id)?
            .items
            .into_iter()
            .find(|a| a.id == "fork");
        for turn in &mut transcript.turns {
            turn.footnote_actions.clear();
            if caps.can_fork_completed_turn
                && turn.fork_boundary.is_some()
                && !transcript.is_stale
                && !matches!(turn.status.as_str(), "running" | "waitingForInput")
                && let Some(mut action) = fork.clone()
            {
                action.id = format!("forkTurn:{}", turn.id);
                action.label = "从此回合分叉".into();
                turn.footnote_actions.push(action);
            }
        }
        Ok(())
    }
    async fn perform_thread_action(
        &self,
        shared: &SharedEndpoint,
        mut request: ThreadActionRequest,
        rpc_id: &str,
    ) -> Result<ThreadActionOutcome> {
        use tekes_agent_presentation::{ThreadActionCommand, thread_action_command};
        let fork_boundary = if let Some(id) = request.action_id.strip_prefix("forkTurn:") {
            let state = shared.runtime.snapshot();
            let session = state
                .sessions
                .get(&request.session_id)
                .ok_or_else(|| unsupported("unknown thread"))?;
            let mut transcript = TranscriptSnapshot::read(
                &RunScope::main(&state.endpoint_id, &request.session_id),
                session,
            )?;
            self.present_turn_actions(shared, &request.session_id, &mut transcript)?;
            let turn = transcript
                .turns
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| unsupported("unknown turn"))?;
            require(
                turn.footnote_actions
                    .iter()
                    .any(|a| a.id == request.action_id && a.enabled),
                "turn fork is unavailable",
            )?;
            let boundary = turn
                .fork_boundary
                .clone()
                .ok_or_else(|| unsupported("missing settled turn boundary"))?;
            request.action_id = "fork".into();
            Some(boundary)
        } else {
            None
        };

        if matches!(request.action_id.as_str(), "pin" | "unpin")
            && let Some(store) = &self.thread_preferences
            && store.replay(&shared.runtime.snapshot().endpoint_id, &request, rpc_id)?
        {
            return Ok(ThreadActionOutcome::default());
        }
        let presentation = self.thread_actions(shared, &request.session_id)?;
        let command = thread_action_command(&presentation, &request)?;
        let mut outcome = ThreadActionOutcome::default();
        match command {
            ThreadActionCommand::Rename(title) => {
                let result = shared
                    .runtime
                    .call(
                        "session.rename",
                        json!({"sessionId":request.session_id,"title":title}),
                        rpc_id,
                    )
                    .await?;
                outcome.renamed_title = Some(result["title"].as_str().unwrap_or(&title).into());
                outcome.rename_seq = result["seq"].as_i64();
            }
            ThreadActionCommand::Fork => {
                let can_rename = shared
                    .connected()?
                    .capabilities()
                    .supports("session.rename");
                let state = shared.runtime.snapshot();
                let summary = state
                    .sessions
                    .get(&request.session_id)
                    .and_then(|s| s.summary.as_ref());
                let title = summary
                    .and_then(|s| s["title"].as_str().or_else(|| s["name"].as_str()))
                    .unwrap_or("会话");
                let title = tekes_agent_presentation::increased_fork_title(title);
                let mut payload = fork_boundary.unwrap_or_else(|| json!({}));
                payload["sessionId"] = json!(request.session_id);
                payload["ephemeral"] = json!(false);
                let result = shared.runtime.call("session.fork", payload, rpc_id).await?;
                let child = string(&result, "sessionId")?.to_owned();
                outcome.navigation = Some(ThreadActionNavigation {
                    endpoint: presentation.endpoint,
                    session_id: child.clone(),
                });
                if result.get("queueCleanupWarning").is_some() {
                    outcome.warning = Some("分叉已创建，继承队列清理未完成，请检查新会话。".into());
                }
                if can_rename
                    && shared
                        .runtime
                        .call(
                            "session.rename",
                            json!({"sessionId":child,"title":title}),
                            &format!("{rpc_id}:title"),
                        )
                        .await
                        .is_err()
                {
                    outcome.warning = Some("分叉已创建，但新会话命名未完成。".into());
                }
            }
            ThreadActionCommand::Archive | ThreadActionCommand::Unarchive => {
                let archive = matches!(command, ThreadActionCommand::Archive);
                shared
                    .runtime
                    .call(
                        if archive {
                            "session.archive"
                        } else {
                            "session.unarchive"
                        },
                        json!({"sessionId":request.session_id}),
                        rpc_id,
                    )
                    .await?;
                outcome.clear_selection = archive;
            }
            ThreadActionCommand::Host => {
                let result = self
                    .thread_action_host
                    .as_ref()
                    .ok_or_else(|| unsupported("thread action host unavailable"))?
                    .perform(&presentation.endpoint, &request, rpc_id)
                    .await;
                self.invalidate_thread_inventory();
                return result;
            }
        }
        Ok(outcome)
    }
    pub fn register(
        &mut self,
        id: String,
        kind: String,
        name: String,
        connector: Arc<dyn EndpointConnector>,
    ) -> Result<()> {
        require(
            !self.endpoints.contains_key(&id),
            "duplicate endpoint identity",
        )?;
        let endpoint = Arc::new(RwLock::new(None));
        let connector = Arc::new(RwLock::new(connector));
        let runtime = Arc::new(SessionRuntime::spawn(
            &id,
            Arc::new(TrackedConnector {
                inner: connector.clone(),
                endpoint: endpoint.clone(),
            }),
        ));
        self.endpoints.insert(
            id,
            Arc::new(SharedEndpoint {
                kind,
                name,
                runtime,
                connector,
                endpoint,
            }),
        );
        Ok(())
    }
    pub fn endpoint(&self, id: &str) -> Result<Arc<SharedEndpoint>> {
        self.endpoints
            .get(id)
            .cloned()
            .ok_or_else(|| unsupported("unknown endpoint"))
    }
    /// The embedding host may refresh credentials without changing endpoint or session identity.
    pub async fn replace_connector(
        &self,
        id: &str,
        connector: Arc<dyn EndpointConnector>,
    ) -> Result<()> {
        let endpoint = self.endpoint(id)?;
        *endpoint.connector.write().expect("connector lock") = connector;
        *endpoint.endpoint.write().expect("endpoint lock") = None;
        endpoint.runtime.reconnect().await
    }
    pub fn descriptions(&self) -> Value {
        json!(
            self.endpoints
                .values()
                .map(|e| e.description())
                .collect::<Vec<_>>()
        )
    }
    pub async fn request(&self, request: &Value) -> Result<Value> {
        if request["op"] == "endpoints" {
            return Ok(self.descriptions());
        }
        let shared = self.endpoint(string(request, "endpoint")?)?;
        let runtime = &shared.runtime;
        match string(request, "op")? {
            "describe" => Ok(shared.description()),
            "snapshot" => Ok(serde_json::to_value(runtime.snapshot().as_ref())?),
            "call" => {
                if request["method"] == "thread.actions" {
                    return Ok(serde_json::to_value(self.thread_actions(
                        &shared,
                        string(&request["payload"], "sessionId")?,
                    )?)?);
                }
                if request["method"] == "thread.performAction" {
                    return Ok(serde_json::to_value(
                        self.perform_thread_action(
                            &shared,
                            serde_json::from_value(request["payload"].clone())?,
                            string(request, "rpcId")?,
                        )
                        .await?,
                    )?);
                }
                if request["method"] == "composer.compileAnnotations" {
                    let input: AnnotationInput =
                        serde_json::from_value(request["payload"].clone())?;
                    let text = tekes_agent_presentation::compile_annotation_input(
                        &input.text,
                        &input.annotations,
                    )?;
                    return Ok(serde_json::to_value(AnnotationCompilation { text })?);
                }
                runtime
                    .call(
                        string(request, "method")?,
                        request["payload"].clone(),
                        string(request, "rpcId")?,
                    )
                    .await
            }
            "history" => Ok(serde_json::to_value(
                shared
                    .connected()?
                    .history_page(serde_json::from_value(request["payload"].clone())?)
                    .await?,
            )?),
            "respond" => {
                runtime
                    .respond(
                        string(request, "actionableId")?,
                        integer(request, "expectedRevision")?,
                        request["outcome"].clone(),
                    )
                    .await?;
                Ok(json!({}))
            }
            "earlier" => {
                Ok(json!({"loaded":runtime.load_earlier(string(request,"sessionId")?).await?}))
            }
            _ => Err(unsupported("unknown service operation")),
        }
    }
    /// Each observer gets its own lease and generation. No browser owns execution lifetime.
    pub async fn follow(&self, request: &Value, output: mpsc::Sender<Value>) -> Result<()> {
        let shared = self.endpoint(string(request, "endpoint")?)?;
        if request["target"] == "view" || request["target"] == "state" {
            let runtime = &shared.runtime;
            let session = request["sessionId"].as_str();
            let _lease = if let Some(id) = session {
                Some(runtime.acquire_history(id).await?)
            } else {
                None
            };
            let mut observer = runtime.subscribe();
            let (_preferences_sender, mut preferences) = tokio::sync::watch::channel(0_u64);
            if let Some(store) = &self.thread_preferences {
                preferences = store.subscribe();
            }
            let mut local_changes = self.inventory_changes.0.subscribe();
            loop {
                let state = runtime.snapshot();
                let value = if let Some(id) = session {
                    let scope = RunScope::main(&state.endpoint_id, id);
                    let mut transcript = state
                        .sessions
                        .get(id)
                        .map(|s| TranscriptSnapshot::read(&scope, s))
                        .transpose()?;
                    if let Some(transcript) = &mut transcript {
                        self.present_turn_actions(&shared, id, transcript)?;
                    }
                    let mut transcript = serde_json::to_value(transcript)?;
                    present_annotations(&mut transcript);
                    json!({"endpoint":state.endpoint_id,"sessionId":id,"run":RunPresentation::read(&state,&scope,true),"transcript":transcript})
                } else {
                    self.inventory(&shared, &state)
                };
                if output.send(value).await.is_err() {
                    break;
                }
                tokio::select! { result = local_changes.changed() => {if result.is_err(){break}}, result = preferences.changed() => {if result.is_err(){break}}, result = observer.changed() => {if result.is_err(){break}}, _ = output.closed() => break }
            }
        } else {
            let mut stream = shared
                .connected()?
                .follow(serde_json::from_value(request["target"].clone())?)
                .await?;
            loop {
                tokio::select! {
                    frame = stream.next() => match frame {Some(frame)=> {if output.send(frame?).await.is_err(){break}}, None=>break},
                    _ = output.closed() => break,
                }
            }
        }
        Ok(())
    }
}
