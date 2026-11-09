//! Shared AgentKit service. Hosts own its authentication, listener and process lifetime.
//! A disconnected view drops only its history lease; it never stops a runtime or Harness.
mod catalog;
mod context_details;
mod family;
mod feedback;
mod goals;
mod harness_workspace;
mod presets;
mod queue;
mod recovery;
mod settings;
mod sidebar;
mod sidechat;
mod stdio;
pub mod thread_search;
mod workspace_registry;
mod workspace_service;
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{Value, json};
pub use sidebar::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};
pub use stdio::{HostRequest, HostResponse, abort_observation, forward_follow};
use tekes_agent_contract::*;
use tekes_agent_presentation::{RunPresentation, RunScope, TranscriptSnapshot};
use tekes_agent_session::{ConnectionState, MemoryProjectionStore, SessionRuntime};
use tokio::sync::mpsc;
pub use workspace_registry::WorkspaceRegistration;
pub use workspace_service::HTTPWorkspaceServiceEndpoint;

// Connection history belongs to this observation, never to another endpoint/view.
fn view_run_presentation(
    state: &MemoryProjectionStore,
    scope: &RunScope,
    was_connected: &mut bool,
) -> RunPresentation {
    *was_connected |= matches!(state.connection, ConnectionState::Ready);
    RunPresentation::read(state, scope, *was_connected)
}

fn inventory_summary(state: &MemoryProjectionStore, id: &str) -> Option<Value> {
    if state
        .archived_session_ids
        .iter()
        .any(|archived| archived == id)
    {
        return None;
    }
    if state.sessions.get(id)?.summary.as_ref()?["status"] == "archived" {
        return None;
    }
    retained_inventory_summary(state, id)
}
fn retained_inventory_summary(state: &MemoryProjectionStore, id: &str) -> Option<Value> {
    let session = state.sessions.get(id)?;
    let summary = session.summary.as_ref()?;
    let projections: BTreeMap<_, _> = session
        .projections
        .iter()
        .map(|(key, value)| (key, &value.value))
        .collect();
    Some(json!({"sessionId":id,"summary":summary,"projections":projections}))
}
fn active_inventory(state: &MemoryProjectionStore) -> Vec<Value> {
    state
        .sessions
        .keys()
        .filter_map(|id| inventory_summary(state, id))
        .collect()
}

/// Archived rows retain the same public summary and projections as active rows.
/// An archive identity without a retained public summary is not a fabricated row.
fn archived_inventory(state: &MemoryProjectionStore) -> Vec<Value> {
    state
        .sessions
        .iter()
        .filter(|(id, session)| {
            state.archived_session_ids.contains(id)
                || session
                    .summary
                    .as_ref()
                    .is_some_and(|s| s["status"] == "archived")
        })
        .filter_map(|(id, _)| retained_inventory_summary(state, id))
        .collect()
}

/// Raw archive facts from the exact store snapshot used by active_inventory.
/// Retained values do not become current confirmation while readiness is false.
fn inventory_archive_facts(state: &MemoryProjectionStore) -> Value {
    let statuses: BTreeMap<_, _> = state
        .sessions
        .iter()
        .filter_map(|(id, session)| {
            session
                .summary
                .as_ref()?
                .get("status")
                .map(|value| (id.clone(), value.clone()))
        })
        .collect();
    json!({"workspaceReady":state.workspace_ready,"archivedSessionIds":state.archived_session_ids,
        "retainedSummaryStatuses":statuses})
}

/// Builds the retained name corpus from the same canonical store snapshot as inventory rows.
/// Display membership, archive status, and session classifications do not filter this source.
pub fn retained_name_corpus(
    state: &MemoryProjectionStore,
    endpoint_domain: impl IntoIterator<Item = String>,
) -> Result<tekes_agent_presentation::SidebarNameCorpus> {
    let mut endpoint_domain: Vec<_> = endpoint_domain.into_iter().collect();
    endpoint_domain.sort();
    let mut documents = Vec::new();
    for (session_id, session) in &state.sessions {
        if let Some(summary) = session.summary.as_ref() {
            require(
                summary.is_object(),
                &format!("invalid retained session summary for {session_id}"),
            )?;
            let projected_title = session.projections.get("title").map(|value| &value.value);
            documents.push(tekes_agent_presentation::SidebarNameDocument {
                address: SessionAddress {
                    endpoint_id: state.endpoint_id.clone(),
                    session_id: session_id.clone(),
                },
                title: tekes_agent_presentation::canonical_session_title(summary, projected_title),
            });
        }
    }
    let corpus = tekes_agent_presentation::SidebarNameCorpus {
        endpoint_id: state.endpoint_id.clone(),
        connection_epoch: state.connection_epoch,
        inventory_ready: state.inventory_ready,
        endpoint_domain,
        documents,
    };
    corpus.validate()?;
    Ok(corpus)
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
            vec![json!({"sessionId":"active","summary":{"sessionId":"active"},"projections":{}})]
        );
        assert!(store.sessions["archived"].summary.is_some());
        store.archived_session_ids.clear();
        assert_eq!(active_inventory(&store).len(), 2);
    }

    #[test]
    fn inventory_publishes_reconciled_projection_without_mutating_older_summary() {
        let mut store = MemoryProjectionStore::new("pi");
        let summary = json!({"sessionId":"a","projections":{"asOfSeq":1,"values":{"title":"old"}}});
        let mut session = SessionProjection {
            summary: Some(summary.clone()),
            ..Default::default()
        };
        session.projections.insert(
            "title".into(),
            tekes_agent_session::VersionedValue {
                sequence: 2,
                value: json!("当前中文标题"),
            },
        );
        store.sessions.insert("a".into(), session);
        let rows = active_inventory(&store);
        assert_eq!(rows[0]["projections"]["title"], "当前中文标题");
        assert_eq!(rows[0]["summary"], summary);
        store
            .sessions
            .get_mut("a")
            .unwrap()
            .projections
            .get_mut("title")
            .unwrap()
            .value = Value::Null;
        assert_eq!(
            active_inventory(&store)[0]["projections"]["title"],
            Value::Null
        );
    }

    #[test]
    fn retained_name_corpus_keeps_hidden_names_without_changing_inventory_membership() {
        let mut store = MemoryProjectionStore::new("one");
        store.connection_epoch = 7;
        store.inventory_ready = true;
        store.archived_session_ids.push("archived".into());
        for (id, summary) in [
            (
                "visible",
                json!({"sessionId":"visible","title":"old visible"}),
            ),
            (
                "archived",
                json!({"sessionId":"archived","status":"archived","title":"Archive name"}),
            ),
            (
                "child",
                json!({"sessionId":"child","origin":"subagent","title":"Child name"}),
            ),
            (
                "ephemeral",
                json!({"sessionId":"ephemeral","ephemeral":true,"title":"Temporary name"}),
            ),
        ] {
            store.sessions.insert(
                id.into(),
                SessionProjection {
                    summary: Some(summary),
                    ..Default::default()
                },
            );
        }
        store
            .sessions
            .insert("no-summary".into(), SessionProjection::default());
        store
            .sessions
            .get_mut("visible")
            .unwrap()
            .projections
            .insert(
                "title".into(),
                tekes_agent_session::VersionedValue {
                    sequence: 1,
                    value: json!("Renamed"),
                },
            );

        let rows = active_inventory(&store);
        assert_eq!(rows.len(), 3, "existing API membership remains unchanged");
        let corpus = retained_name_corpus(&store, ["two".into(), "one".into()]).unwrap();
        assert_eq!(corpus.connection_epoch, 7);
        assert!(corpus.inventory_ready);
        assert_eq!(corpus.endpoint_domain, vec!["one", "two"]);
        assert_eq!(corpus.documents.len(), 4);
        let titles: BTreeMap<_, _> = corpus
            .documents
            .iter()
            .map(|d| (d.address.session_id.as_str(), d.title.as_str()))
            .collect();
        assert_eq!(titles["visible"], "Renamed");
        assert_eq!(titles["archived"], "Archive name");
        assert_eq!(titles["child"], "Child name");
        assert_eq!(titles["ephemeral"], "Temporary name");
        assert!(!titles.contains_key("no-summary"));
    }

    #[test]
    fn retained_corpus_tracks_canonical_clear_and_actual_deletion() {
        let mut store = MemoryProjectionStore::new("one");
        store.sessions.insert(
            "same-id".into(),
            SessionProjection {
                summary: Some(json!({"title":"legacy title","name":"legacy name"})),
                ..Default::default()
            },
        );
        let domain = ["one".to_owned()];
        assert_eq!(
            retained_name_corpus(&store, domain.clone())
                .unwrap()
                .documents[0]
                .title,
            "legacy title"
        );

        store
            .sessions
            .get_mut("same-id")
            .unwrap()
            .projections
            .insert(
                "title".into(),
                tekes_agent_session::VersionedValue {
                    sequence: 2,
                    value: Value::Null,
                },
            );
        assert_eq!(
            retained_name_corpus(&store, domain.clone())
                .unwrap()
                .documents[0]
                .title,
            "New conversation"
        );
        store.sessions.remove("same-id");
        assert!(
            retained_name_corpus(&store, domain)
                .unwrap()
                .documents
                .is_empty()
        );

        let other_endpoint = MemoryProjectionStore::new("two");
        let mut other_endpoint = other_endpoint;
        other_endpoint.sessions.insert(
            "same-id".into(),
            SessionProjection {
                summary: Some(json!({"title":"other endpoint"})),
                ..Default::default()
            },
        );
        let mut first_endpoint = MemoryProjectionStore::new("one");
        first_endpoint.sessions.insert(
            "same-id".into(),
            SessionProjection {
                summary: Some(json!({"title":"first endpoint"})),
                ..Default::default()
            },
        );
        let one = retained_name_corpus(&first_endpoint, ["one".into(), "two".into()]).unwrap();
        let two = retained_name_corpus(&other_endpoint, ["one".into(), "two".into()]).unwrap();
        assert!(
            one.validate().is_ok() && two.validate().is_ok(),
            "same session IDs are scoped by endpoint"
        );
        assert_ne!(one.documents[0].address, two.documents[0].address);
    }

    #[tokio::test]
    async fn public_inventory_frame_pairs_rows_and_retained_corpus_from_one_snapshot() {
        let mut agent = SharedAgent::default();
        agent
            .register("one".into(), "test".into(), "One".into(), Arc::new(Offline))
            .unwrap();
        agent
            .register("two".into(), "test".into(), "Two".into(), Arc::new(Offline))
            .unwrap();
        let shared = agent.endpoints.get("one").unwrap();

        let mut store = MemoryProjectionStore::new("one");
        store.connection_epoch = 9;
        store.inventory_ready = true;
        store.archived_session_ids.push("archived".into());
        store.sessions.insert(
            "visible".into(),
            SessionProjection {
                summary: Some(json!({"sessionId":"visible","title":"Visible"})),
                ..Default::default()
            },
        );
        store.sessions.insert(
            "archived".into(),
            SessionProjection {
                summary: Some(json!({"sessionId":"archived","title":"Hidden name"})),
                ..Default::default()
            },
        );

        let frame = agent.inventory(shared, &store).unwrap();
        assert_eq!(frame["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(frame["inventoryReady"], true);
        assert_eq!(frame["nameCorpus"]["endpointId"], "one");
        assert_eq!(frame["nameCorpus"]["connectionEpoch"], 9);
        assert_eq!(frame["nameCorpus"]["endpointDomain"], json!(["one", "two"]));
        assert_eq!(
            frame["nameCorpus"]["documents"].as_array().unwrap().len(),
            2
        );
        assert!(
            frame["nameCorpus"]["documents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|document| document["address"]["session_id"] == "archived"
                    && document["title"] == "Hidden name")
        );
    }

    #[test]
    fn retained_corpus_rejects_invalid_summaries_and_unregistered_source() {
        let mut store = MemoryProjectionStore::new("one");
        store.sessions.insert(
            "bad".into(),
            SessionProjection {
                summary: Some(Value::Null),
                ..Default::default()
            },
        );
        assert!(retained_name_corpus(&store, ["one".into()]).is_err());

        store.sessions.clear();
        assert!(retained_name_corpus(&store, ["two".into()]).is_err());
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
    pub(crate) side_chats: sidechat::SideChatOwner,
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
            "connectionEpoch":state.connection_epoch,"canReconnect":matches!(state.connection, ConnectionState::Failed(_)),
            "connection":match &state.connection {ConnectionState::Connecting=>json!("connecting"),ConnectionState::Ready=>json!("ready"),ConnectionState::Failed(e)=>json!({"failed":e}),ConnectionState::Stopped=>json!("stopped")},"host":state.host,
            "capabilities":self.connected().ok().map(|e| e.capabilities())});
        if let Some(methods) = description["capabilities"]["methods"].as_array_mut() {
            methods.push(json!("composer.compileAnnotations"));
            methods.push(json!("agent.context.read"));
            if self.connected().ok().is_some_and(|endpoint| {
                harness_workspace::supports_harness_workspace(&self.kind, &endpoint.capabilities())
            }) {
                methods.push(json!("agent.workspace.read"));
            }
            if self.connected().ok().is_some_and(|endpoint| {
                endpoint.capabilities().supports("session.presets.describe")
                    && endpoint.capabilities().supports("session.presets.resolve")
            }) {
                methods.extend([
                    json!("agent.preset.describe"),
                    json!("agent.preset.preview"),
                ]);
            }

            if self.connected().ok().is_some_and(|endpoint| {
                let caps = endpoint.capabilities();
                caps.supports_ephemeral_fork
                    && caps.supports("session.fork")
                    && caps.supports("session.discard")
                    && caps.supports("session.cancel")
            }) {
                methods.extend(
                    [
                        "agent.sideChat.state",
                        "agent.sideChat.create",
                        "agent.sideChat.close",
                        "agent.sideChat.reconcile",
                        "agent.sideChat.prompt",
                        "agent.sideChat.cancel",
                    ]
                    .into_iter()
                    .map(|method| json!(method)),
                );
            }

            if self
                .connected()
                .ok()
                .is_some_and(|endpoint| endpoint.capabilities().supports("thread.search"))
            {
                methods.push(json!("agent.threadSearch"));
            }

            if self.connected().ok().is_some_and(|endpoint| {
                endpoint.supports_recovery() && endpoint.capabilities().supports("sessions.recover")
            }) {
                methods.extend([json!("agent.recovery.read"), json!("agent.recovery.run")]);
            }
            for (upstream, public) in [
                ("tools/list", "agent.tools.list"),
                ("tools/resolve", "agent.tools.resolve"),
                ("resources/list", "agent.resources.list"),
                ("resources/read", "agent.resources.read"),
            ] {
                if self
                    .connected()
                    .ok()
                    .is_some_and(|endpoint| endpoint.capabilities().supports(upstream))
                {
                    methods.push(json!(public));
                }
            }

            let capabilities = self
                .connected()
                .ok()
                .map(|endpoint| endpoint.capabilities());
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("feedback.list"))
            {
                methods.push(json!("agent.feedback.read"));
                methods.push(json!("agent.feedback.action"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("settings.describe"))
            {
                methods.push(json!("agent.settings.read"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("settings.mutate"))
            {
                methods.push(json!("agent.settings.mutate"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("goals.get"))
            {
                methods.push(json!("agent.goal.read"));
                methods.push(json!("agent.goal.action"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("commands/list") || caps.supports("skills/list"))
            {
                methods.push(json!("agent.catalog"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("commands/list") && caps.supports("commands/run"))
            {
                methods.push(json!("agent.command.run"));
            }
            if capabilities
                .as_ref()
                .is_some_and(|caps| caps.supports("skills/list"))
            {
                methods.push(json!("agent.skills.compile"));
            }
            methods.push(json!("thread.actions"));
            methods.push(json!("thread.performAction"));
            methods.push(json!("agent.familySnapshot"));
            methods.push(json!("agent.child.open"));
            methods.push(json!("agent.child.earlier"));
            methods.push(json!("agent.child.prompt"));
            methods.push(json!("agent.child.cancel"));
            methods.push(json!("agent.child.close"));
            if self
                .connected()
                .is_ok_and(|endpoint| endpoint.capabilities().supports("session.updateQueue"))
            {
                methods.push(json!("agent.queue.remove"));
                methods.push(json!("agent.queue.edit"));
            }
        }
        description
    }
}
#[derive(Default)]
pub struct SharedAgent {
    endpoints: BTreeMap<String, Arc<SharedEndpoint>>,
    workspace_services: workspace_registry::WorkspaceServices,
    thread_action_host: Option<Arc<dyn ThreadActionHost>>,
    session_url_scheme: Option<String>,
    thread_preferences: Option<Arc<tekes_agent_presentation::ThreadPreferences>>,
    inventory_changes: InventoryChanges,
    family_views: std::sync::Mutex<BTreeMap<String, family::FamilyChildView>>,
}
struct ThreadActionInputs {
    capabilities: EndpointCapabilities,
    connected: bool,
    host_methods: BTreeSet<String>,
    pin: Option<ThreadPinState>,
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
    pub fn sidebar_preferences(
        &self,
    ) -> Option<tekes_agent_presentation::SidebarPreferencesSnapshot> {
        self.thread_preferences
            .as_ref()
            .map(|store| store.sidebar_preferences())
    }
    pub fn set_sidebar_preferences(
        &self,
        change: &tekes_agent_presentation::SidebarPreferenceChange,
    ) -> Result<()> {
        self.thread_preferences
            .as_ref()
            .ok_or_else(|| unsupported("Sidebar preference store not installed"))?
            .set_sidebar_preferences(change)
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
    fn inventory(&self, shared: &SharedEndpoint, state: &MemoryProjectionStore) -> Result<Value> {
        let sessions: Vec<Value> = active_inventory(state)
            .into_iter()
            .map(|row| self.decorate_inventory_row(state, row))
            .collect();
        let archived_sessions: Vec<Value> = archived_inventory(state)
            .into_iter()
            .map(|row| self.decorate_inventory_row(state, row))
            .collect();
        let name_corpus = retained_name_corpus(state, self.endpoints.keys().cloned())?;
        Ok(
            json!({"description":shared.description(),"sessions":sessions,"archivedSessions":archived_sessions,"inventoryReady":state.inventory_ready,"connectionEpoch":state.connection_epoch,"nameCorpus":name_corpus,"archiveFacts":inventory_archive_facts(state)}),
        )
    }
    fn decorate_inventory_row(&self, state: &MemoryProjectionStore, row: Value) -> Value {
        let id = row["sessionId"].as_str().expect("inventory identity");
        let pin = self
            .thread_action_host
            .as_ref()
            .and_then(|host| host.pin_state(&state.endpoint_id, id));
        self.decorate_inventory_row_with_pin(state, row, pin.as_ref())
    }
    fn decorate_inventory_row_with_pin(
        &self,
        state: &MemoryProjectionStore,
        mut row: Value,
        pin: Option<&ThreadPinState>,
    ) -> Value {
        let id = row["sessionId"]
            .as_str()
            .expect("inventory identity")
            .to_owned();
        if pin.is_some_and(|pin| pin.pinned) {
            row["pin"] = json!(ThreadPinPresentation { pinned: true });
        }
        row["activity"] =
            serde_json::to_value(tekes_agent_presentation::thread_activity(state, &id))
                .expect("thread activity");
        row
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
        let inputs = self.thread_action_inputs(shared, id);
        shared
            .runtime
            .inspect(|state| self.thread_actions_in_state(shared, id, state, &inputs))
    }
    // Host callbacks run before acquiring the projection read lock. An adapter
    // may inspect runtime facts; no external callback is invoked under it.
    fn thread_action_inputs(&self, shared: &SharedEndpoint, id: &str) -> ThreadActionInputs {
        let endpoint = shared.connected().ok();
        let endpoint_id = shared.runtime.inspect(|s| s.endpoint_id.clone());
        ThreadActionInputs {
            capabilities: endpoint
                .as_ref()
                .map(|e| e.capabilities())
                .unwrap_or_default(),
            connected: endpoint.is_some(),
            host_methods: self
                .thread_action_host
                .as_ref()
                .map(|h| h.methods())
                .unwrap_or_default(),
            pin: self
                .thread_action_host
                .as_ref()
                .and_then(|h| h.pin_state(&endpoint_id, id)),
        }
    }
    fn thread_actions_in_state(
        &self,
        shared: &SharedEndpoint,
        id: &str,
        state: &MemoryProjectionStore,
        inputs: &ThreadActionInputs,
    ) -> Result<ThreadActionsPresentation> {
        let session = state
            .sessions
            .get(id)
            .ok_or_else(|| unsupported("unknown thread"))?;
        let summary = session.summary.as_ref();
        let archived = state.archived_session_ids.iter().any(|s| s == id)
            || summary.is_some_and(|s| s["status"] == "archived");
        let running = tekes_agent_presentation::session_has_active_work(session);
        Ok(tekes_agent_presentation::thread_actions(
            &tekes_agent_presentation::ThreadActionContext {
                endpoint: &state.endpoint_id,
                session_id: id,
                kind: &shared.kind,
                url_scheme: self.session_url_scheme.as_deref().unwrap_or("agent"),
                capabilities: &inputs.capabilities,
                ready: state.connection == ConnectionState::Ready && inputs.connected,
                running,
                archived,
                pin: inputs.pin.as_ref(),
                host_methods: &inputs.host_methods,
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
        let state = shared.runtime.snapshot();
        let session = state
            .sessions
            .get(id)
            .ok_or_else(|| unsupported("unknown thread"))?;
        tekes_agent_presentation::present_turn_actions(
            &tekes_agent_presentation::TurnActionContext {
                capabilities: &caps,
                ready: state.connection == ConnectionState::Ready && shared.connected().is_ok(),
                running: tekes_agent_presentation::session_has_active_work(session),
            },
            transcript,
        );
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
                let session = state.sessions.get(&request.session_id);
                let title = tekes_agent_presentation::canonical_session_title(
                    session
                        .and_then(|s| s.summary.as_ref())
                        .unwrap_or(&Value::Null),
                    session
                        .and_then(|s| s.projections.get("title"))
                        .map(|p| &p.value),
                );
                let title = tekes_agent_presentation::increased_fork_title(&title);
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
                side_chats: sidechat::SideChatOwner::new(runtime.clone()),
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
        if matches!(
            request["op"].as_str(),
            Some("workspace.list" | "workspace.call")
        ) {
            return self.workspace_request(request).await;
        }
        if request["op"] == "endpoints" {
            return Ok(self.descriptions());
        }
        let shared = self.endpoint(string(request, "endpoint")?)?;
        let runtime = &shared.runtime;
        match string(request, "op")? {
            "describe" => Ok(shared.description()),
            "reconnect" => {
                runtime.reconnect().await?;
                Ok(shared.description())
            }
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
                if request["method"] == "agent.familySnapshot" {
                    return self
                        .family_snapshot(&shared, string(&request["payload"], "sessionId")?)
                        .await;
                }
                if request["method"] == "agent.child.open" {
                    return self.open_child(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.child.earlier" {
                    return self.child_earlier(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.child.prompt" {
                    return self
                        .child_prompt(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.child.cancel" {
                    return self
                        .child_cancel(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.child.close" {
                    return self.close_child(&shared, &request["payload"]).await;
                }

                if request["method"] == "agent.feedback.read" {
                    return self
                        .feedback_read(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.feedback.action" {
                    return self
                        .feedback_action(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.settings.read" {
                    return self.settings_read(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.settings.mutate" {
                    return self
                        .settings_mutate(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.recovery.read" {
                    return self.recovery_read(&shared, &request["payload"]);
                }
                if request["method"] == "agent.recovery.run" {
                    return self
                        .recovery_run(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.tools.list" {
                    return self
                        .tool_catalog(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.tools.resolve" {
                    return self
                        .resolve_catalog_tool(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.resources.list" {
                    return self
                        .skill_resource_page(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.resources.read" {
                    return self
                        .read_skill_resource(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.threadSearch" {
                    return self
                        .thread_search(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.sideChat.state" {
                    return self.side_chat_state(&shared, &request["payload"]);
                }
                if request["method"] == "agent.sideChat.create" {
                    return self
                        .side_chat_create(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.sideChat.close" {
                    return self
                        .side_chat_close(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.sideChat.prompt" {
                    return self
                        .side_chat_prompt(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.sideChat.cancel" {
                    return self
                        .side_chat_cancel(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.sideChat.reconcile" {
                    return self.side_chat_reconcile(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.preset.describe" {
                    return self.preset_describe(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.preset.preview" {
                    return self
                        .preset_preview(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.workspace.read" {
                    return self
                        .harness_workspace_read(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.context.read" {
                    return self
                        .context_details_read(&shared, &request["payload"])
                        .await;
                }
                if request["method"] == "agent.goal.read" {
                    return self.goal_read(&shared, &request["payload"]).await;
                }
                if request["method"] == "agent.goal.action" {
                    return self
                        .goal_action(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.catalog" {
                    return self
                        .resource_catalog(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.command.run" {
                    return self
                        .run_catalog_command(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.skills.compile" {
                    return self
                        .compile_catalog_skills(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
                }
                if request["method"] == "agent.queue.edit" {
                    return self
                        .edit_queued_input(&shared, &request["payload"], string(request, "rpcId")?)
                        .await;
                }
                if request["method"] == "agent.queue.remove" {
                    return self
                        .remove_queued_input(
                            &shared,
                            &request["payload"],
                            string(request, "rpcId")?,
                        )
                        .await;
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
                if request["method"] == "session.cancel" {
                    runtime.inspect(|state| {
                        let scope = RunScope::main(
                            &state.endpoint_id,
                            string(&request["payload"], "sessionId")?,
                        );
                        require(
                            RunPresentation::read(state, &scope, true).can_cancel,
                            "Run is not cancellable",
                        )
                    })?;
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
                let id = string(request, "actionableId")?;
                let revision = integer(request, "expectedRevision")?;
                runtime.inspect(|state| {
                    let actionable = state
                        .actionables
                        .get(id)
                        .ok_or_else(|| Error::Protocol("missing actionable".into()))?;
                    let scope =
                        RunScope::main(&state.endpoint_id, string(actionable, "sessionId")?);
                    require(
                        RunPresentation::read(state, &scope, true).can_respond(id, revision),
                        "actionable is unavailable or stale",
                    )
                })?;
                runtime
                    .respond(id, revision, request["outcome"].clone())
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
        if request["target"]["kind"] == "familyView" {
            return self.follow_family(&shared, request, output).await;
        }
        if request["target"]["kind"] == "childView" {
            return self.follow_child(&shared, request, output).await;
        }
        if request["target"] == "view" || request["target"] == "state" {
            let runtime = &shared.runtime;
            let session = request["sessionId"].as_str();
            let mut observer = runtime.subscribe();
            let history = async {
                if let Some(id) = session {
                    runtime.acquire_history_view(id).await.map(Some)
                } else {
                    Ok(None)
                }
            };
            tokio::pin!(history);
            let mut history_pending = true;
            let mut _lease = None;
            let (_preferences_sender, mut preferences) = tokio::sync::watch::channel(0_u64);
            if let Some(store) = &self.thread_preferences {
                preferences = store.subscribe();
            }
            let mut local_changes = self.inventory_changes.0.subscribe();
            let mut was_connected = false;
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
                    json!({"endpoint":state.endpoint_id,"sessionId":id,"run":view_run_presentation(&state,&scope,&mut was_connected),"transcript":transcript})
                } else {
                    self.inventory(&shared, &state)?
                };
                if output.send(value).await.is_err() {
                    break;
                }
                tokio::select! {
                    result = &mut history, if history_pending => {
                        _lease = result?;
                        history_pending = false;
                    },
                    result = local_changes.changed() => {if result.is_err(){break}},
                    result = preferences.changed() => {if result.is_err(){break}},
                    result = observer.changed() => {if result.is_err(){break}},
                    _ = output.closed() => break,
                }
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

#[cfg(test)]
mod archive_facts_tests {
    use super::*;
    #[test]
    fn archived_rows_require_retained_summaries_and_follow_both_archive_authorities() {
        let mut store = MemoryProjectionStore::new("fixture");
        store.archived_session_ids =
            vec!["workspace".into(), "missing".into(), "no-summary".into()];
        for (id, summary) in [
            ("active", json!({"sessionId":"active","status":"idle"})),
            (
                "workspace",
                json!({"sessionId":"workspace","title":"Kept title","status":"idle"}),
            ),
            (
                "summary",
                json!({"sessionId":"summary","status":"archived"}),
            ),
        ] {
            store.sessions.entry(id.into()).or_default().summary = Some(summary);
        }
        store.sessions.entry("no-summary".into()).or_default();
        let archived = archived_inventory(&store);
        assert_eq!(
            archived
                .iter()
                .map(|row| row["sessionId"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["summary", "workspace"]
        );
        assert_eq!(archived[1]["summary"]["title"], "Kept title");
        assert_eq!(active_inventory(&store).len(), 1);
        assert_eq!(
            store.archived_session_ids.len(),
            3,
            "read does not erase archive facts without summaries"
        );
        store.archived_session_ids.clear();
        store
            .sessions
            .get_mut("summary")
            .unwrap()
            .summary
            .as_mut()
            .unwrap()["status"] = json!("idle");
        assert!(archived_inventory(&store).is_empty());
        assert_eq!(active_inventory(&store).len(), 3);
    }

    #[derive(Clone)]
    struct ArchiveConnector {
        host: HostDescription,
        inventory: tokio::sync::broadcast::Sender<Value>,
        calls: Arc<std::sync::Mutex<Vec<(String, Value, String)>>>,
    }
    #[async_trait]
    impl EndpointConnector for ArchiveConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(self.clone()))
        }
    }
    #[async_trait]
    impl SessionEndpoint for ArchiveConnector {
        fn identity(&self) -> &str {
            "archive"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            let mut capabilities = EndpointCapabilities::methods(&["session.unarchive"]);
            capabilities.supports_unarchive_session = true;
            capabilities
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::stream;
            match target {
                StreamTarget::SessionInventory => {
                    let receiver = self.inventory.subscribe();
                    let baseline = json!({"type":"baseline","generation":"archive-g","snapshot":{"items":[
                        {"sessionId":"active","title":"Active","status":"idle","running":false,"blank":false,"updatedAt":1},
                        {"sessionId":"saved","title":"Retained","status":"archived","running":false,"blank":false,"updatedAt":1,"projections":{"asOfSeq":2,"values":{"title":"Current archived title"}}}
                    ]}});
                    Ok(Box::pin(stream::once(async { Ok(baseline) }).chain(stream::unfold(receiver, |mut receiver| async move {
                        receiver.recv().await.ok().map(|value| (Ok(value), receiver))
                    }))))
                }
                StreamTarget::SessionControl => Ok(Box::pin(stream::once(async { Ok(json!({"type":"baseline","generation":"control-g","snapshot":{"queues":[],"jobs":[],"projections":[]}})) }).chain(stream::pending()))),
                StreamTarget::Actionables => Ok(Box::pin(stream::once(async { Ok(json!({"type":"reset","generation":"actionables-g"})) }).chain(stream::pending()))),
                _ => Err(unsupported("archive fixture stream")),
            }
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("archive fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("archive fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
            self.calls
                .lock()
                .unwrap()
                .push((method.into(), payload.clone(), rpc_id.into()));
            require(
                method == "session.unarchive" && payload == json!({"sessionId":"saved"}),
                "unexpected archive fixture call",
            )?;
            self.inventory.send(json!({"type":"upsert","generation":"archive-g","session":{"sessionId":"saved","title":"Retained","status":"idle","running":false,"blank":false,"updatedAt":2,"projections":{"asOfSeq":2,"values":{"title":"Current archived title"}}}})).map_err(|_| Error::Disconnected)?;
            Ok(json!({}))
        }
        async fn close(&self) {}
    }
    struct ArchivePin;
    #[async_trait]
    impl ThreadActionHost for ArchivePin {
        fn methods(&self) -> BTreeSet<String> {
            BTreeSet::new()
        }
        fn pin_state(&self, endpoint: &str, session: &str) -> Option<ThreadPinState> {
            (endpoint == "archive" && session == "saved").then_some(ThreadPinState { pinned: true })
        }
        async fn perform(
            &self,
            _: &str,
            _: &ThreadActionRequest,
            _: &str,
        ) -> Result<ThreadActionOutcome> {
            Err(unsupported("archive fixture host action"))
        }
    }

    #[tokio::test]
    async fn public_inventory_consumer_restores_archived_row_with_its_metadata() {
        use std::time::Duration;
        let (inventory, _) = tokio::sync::broadcast::channel(4);
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let connector = ArchiveConnector {
            host: serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"archive fixture","version":"1"},"capabilities":["session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false})).unwrap(),
            inventory,
            calls: calls.clone(),
        };
        let mut agent = SharedAgent::default();
        agent.install_thread_action_host(Arc::new(ArchivePin));
        agent
            .register(
                "archive".into(),
                "test".into(),
                "Archive".into(),
                Arc::new(connector),
            )
            .unwrap();
        agent
            .endpoint("archive")
            .unwrap()
            .runtime
            .wait_ready(Duration::from_secs(2))
            .await
            .unwrap();
        let agent = Arc::new(agent);
        let observed = agent.clone();
        let (output, mut frames) = mpsc::channel(8);
        let follow = tokio::spawn(async move {
            observed
                .follow(
                    &json!({"op":"follow","endpoint":"archive","target":"state"}),
                    output,
                )
                .await
        });
        let before = tokio::time::timeout(Duration::from_secs(2), frames.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(before["sessions"][0]["sessionId"], "active");
        let saved = &before["archivedSessions"][0];
        assert_eq!(saved["sessionId"], "saved");
        assert_eq!(saved["summary"]["title"], "Retained");
        assert_eq!(saved["projections"]["title"], "Current archived title");
        assert_eq!(saved["pin"]["pinned"], true);
        assert_eq!(saved["activity"]["status"], "idle");
        assert_eq!(saved["activity"]["isStale"], false);
        let menu = agent.request(&json!({"op":"call","endpoint":"archive","method":"thread.actions","rpcId":"read","payload":{"sessionId":"saved"}})).await.unwrap();
        assert!(
            menu["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == "unarchive" && item["enabled"] == true)
        );
        let receipt = agent.request(&json!({"op":"call","endpoint":"archive","method":"thread.performAction","rpcId":"restore-once","payload":{"sessionId":"saved","actionId":"unarchive"}})).await.unwrap();
        assert_eq!(receipt["clearSelection"], false);
        let after = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let frame = frames.recv().await.unwrap();
                if frame["archivedSessions"] == json!([]) {
                    break frame;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(after["sessions"].as_array().unwrap().len(), 2);
        let restored = after["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["sessionId"] == "saved")
            .unwrap();
        assert_eq!(restored["pin"], saved["pin"]);
        assert_eq!(restored["projections"], saved["projections"]);
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[(
                "session.unarchive".into(),
                json!({"sessionId":"saved"}),
                "restore-once".into()
            )]
        );
        follow.abort();
        agent
            .endpoint("archive")
            .unwrap()
            .runtime
            .stop()
            .await
            .unwrap();
    }

    #[test]
    fn archive_facts_preserve_readiness_and_raw_status_without_promoting_missing() {
        let mut store = MemoryProjectionStore::new("fixture");
        store.archived_session_ids = vec!["retained".into()];
        for (id, summary) in [
            ("retained", json!({"status":"archived"})),
            ("missing", json!({})),
            ("null", json!({"status":null})),
            ("future", json!({"status":{"future":true}})),
        ] {
            store.sessions.entry(id.into()).or_default().summary = Some(summary);
        }
        let facts = inventory_archive_facts(&store);
        assert_eq!(facts["workspaceReady"], false);
        assert_eq!(facts["archivedSessionIds"], json!(["retained"]));
        assert!(facts["retainedSummaryStatuses"].get("missing").is_none());
        assert_eq!(facts["retainedSummaryStatuses"]["null"], Value::Null);
        assert_eq!(
            facts["retainedSummaryStatuses"]["future"],
            json!({"future":true})
        );
        store.workspace_ready = true;
        let confirmed = inventory_archive_facts(&store);
        assert_eq!(confirmed["workspaceReady"], true);
        assert_eq!(
            confirmed["retainedSummaryStatuses"],
            facts["retainedSummaryStatuses"]
        );
    }
}

#[cfg(test)]
mod view_connection_tests {
    use super::*;
    use tekes_agent_presentation::ConnectionPhase;

    #[test]
    fn view_only_reports_reconnecting_after_its_own_successful_connection() {
        let mut state = MemoryProjectionStore::new("fixture");
        let scope = RunScope::main("fixture", "session");
        let mut connected = false;
        state.connection = ConnectionState::Connecting;
        assert_eq!(
            view_run_presentation(&state, &scope, &mut connected)
                .connection
                .phase,
            ConnectionPhase::Connecting
        );
        state.connection = ConnectionState::Ready;
        assert_eq!(
            view_run_presentation(&state, &scope, &mut connected)
                .connection
                .phase,
            ConnectionPhase::Connected
        );
        state.connection = ConnectionState::Failed("offline".into());
        assert_eq!(
            view_run_presentation(&state, &scope, &mut connected)
                .connection
                .phase,
            ConnectionPhase::Failed
        );
        state.connection = ConnectionState::Connecting;
        assert_eq!(
            view_run_presentation(&state, &scope, &mut connected)
                .connection
                .phase,
            ConnectionPhase::Reconnecting
        );
        let mut new_observation = false;
        assert_eq!(
            view_run_presentation(&state, &scope, &mut new_observation)
                .connection
                .phase,
            ConnectionPhase::Connecting
        );
    }
}
