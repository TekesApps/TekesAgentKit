//! SharedAgent inventory/menu adaptation. No independent pin store or action policy.
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tekes_agent_components_ui::{SidebarInput, SidebarRow};
use tekes_agent_contract::{
    Result, SessionAddress, ThreadActionDefinition, ThreadActionOutcome, ThreadActionRequest,
    ThreadActionsPresentation, require, unsupported,
};
use tekes_agent_presentation::SidebarNameCorpus;
use tekes_agent_web::{SharedAgent, SharedEndpoint};
use tokio::sync::{mpsc, watch};

pub use tekes_agent_presentation::{SidebarObservation, SidebarObserverPhase};
use tekes_agent_presentation::{
    SidebarObserverEndpointInput, SidebarObserverKey, SidebarObserverSourceKey,
    SidebarObserverStatusInput,
};

/// Original immutable publication paired with its observer provenance. Hosts
/// cannot attach private binding authority to a caller-edited publication.
#[derive(Clone, Debug)]
pub struct SidebarObservationPacket<P> {
    publication: Arc<P>,
    status: SidebarObserverStatusInput,
}
impl<P> SidebarObservationPacket<P> {
    pub(crate) fn new(publication: Arc<P>, status: SidebarObserverStatusInput) -> Self {
        Self {
            publication,
            status,
        }
    }
    pub fn publication(&self) -> &Arc<P> {
        &self.publication
    }
    pub fn status(&self) -> &SidebarObserverStatusInput {
        &self.status
    }
}

#[derive(Debug)]
pub(crate) struct SidebarObserverOwner {
    _identity: u8,
}
/// One failed observation attempt on one binding. Not a mutation retry.
pub struct SidebarObserverRetry {
    owner: Arc<SidebarObserverOwner>,
    endpoint_id: String,
    generation: u64,
}
impl SidebarObserverRetry {
    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
struct ObservationState {
    sender: Option<watch::Sender<Arc<SidebarPublication>>>,
    generation: u64,
    failed: bool,
    inventory: InventoryRows,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl ObservationState {
    fn next_revision(&mut self) -> Result<u64> {
        let revision = self
            .sender
            .as_ref()
            .ok_or_else(|| unsupported("inventory publication exhausted"))?
            .borrow()
            .revision;
        match revision.checked_add(1) {
            Some(next) => Ok(next),
            None => {
                self.failed = true;
                self.sender = None;
                Err(unsupported("inventory revision exhausted"))
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarPublication {
    pub revision: u64,
    pub observation: SidebarObservation,
    pub rows: Vec<SidebarRow>,
    /// Canonical effective activity for the full inventory, before display transforms.
    pub activity_at: std::collections::BTreeMap<SessionAddress, f64>,
    /// Retained-name source captured from this exact inventory frame.
    pub name_corpus: Option<SidebarNameCorpus>,
    pub loaded: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarFolderCatalog {
    pub revision: u64,
    pub loaded: bool,
    pub error: Option<String>,
    pub folders: Vec<tekes_agent_components_ui::SidebarFolderOption>,
}
impl SidebarPublication {
    /// No RPC/refresh, no search membership and no dependency on mounted rows.
    pub fn folder_catalog(&self) -> std::result::Result<SidebarFolderCatalog, &'static str> {
        if self.revision == 0 {
            return Err("unversioned Sidebar catalog");
        }
        Ok(SidebarFolderCatalog {
            revision: self.revision,
            loaded: self.loaded,
            error: self.error.clone(),
            folders: if self.loaded {
                tekes_agent_components_ui::sidebar_folder_catalog(&self.rows, &self.activity_at)?
            } else {
                vec![]
            },
        })
    }

    /// Host navigation supplies selection; the inventory observer never selects a row.
    pub fn input(&self, selected: Option<SessionAddress>) -> SidebarInput {
        SidebarInput {
            folder_filter: None,
            search_help: None,
            revision: self.revision,
            loaded: self.loaded,
            error: self.error.clone(),
            rows: self.rows.clone(),
            selected,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarActionOutcome {
    /// The host executes the user's clipboard gesture using this fresh shared value.
    Copy {
        address: SessionAddress,
        text: String,
    },
    Mutation {
        address: SessionAddress,
        outcome: ThreadActionOutcome,
    },
}

/// Observes the same SharedAgent endpoint used by RunBinding. Drop cancels only
/// the inventory observer, never remote execution or the endpoint's runtime.
pub struct SidebarBinding {
    pub(crate) agent: Arc<SharedAgent>,
    pub(crate) endpoint: Arc<SharedEndpoint>,
    publications: watch::Receiver<Arc<SidebarPublication>>,
    observation: Arc<Mutex<ObservationState>>,
    observer_owner: Arc<SidebarObserverOwner>,
    executor: tokio::runtime::Handle,
}
impl Drop for SidebarBinding {
    fn drop(&mut self) {
        let mut state = self.observation.lock().expect("Sidebar observation lock");
        state.failed = true;
        state.sender = None;
        if let Some(task) = state.task.take() {
            task.abort();
        }
    }
}
impl SidebarBinding {
    pub fn turn_action_dispatcher(&self) -> crate::TurnActionDispatcher {
        crate::TurnActionDispatcher::new(self.agent.clone())
    }

    pub fn new(agent: Arc<SharedAgent>, endpoint_id: &str) -> Result<Self> {
        let endpoint = agent.endpoint(endpoint_id)?;
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| unsupported("SidebarBinding requires an async runtime"))?;
        let (sender, publications) = watch::channel(Arc::new(SidebarPublication {
            revision: 1,
            observation: SidebarObservation::default(),
            rows: vec![],
            activity_at: Default::default(),
            name_corpus: None,
            loaded: false,
            error: None,
        }));
        let observation = Arc::new(Mutex::new(ObservationState {
            sender: Some(sender),
            generation: 1,
            failed: false,
            inventory: InventoryRows::default(),
            task: None,
        }));
        let binding = Self {
            agent,
            endpoint,
            publications,
            observation,
            observer_owner: Arc::new(SidebarObserverOwner { _identity: 0 }),
            executor,
        };
        let task = binding.spawn_observer(1);
        binding
            .observation
            .lock()
            .expect("Sidebar observation lock")
            .task = Some(task);
        Ok(binding)
    }
    pub(crate) fn observer_owner(&self) -> Arc<SidebarObserverOwner> {
        self.observer_owner.clone()
    }
    pub fn observer_generation(&self) -> u64 {
        self.observation
            .lock()
            .expect("Sidebar observation lock")
            .generation
    }
    pub(crate) fn owns_observer(&self, owner: &Arc<SidebarObserverOwner>, generation: u64) -> bool {
        let state = self.observation.lock().expect("Sidebar observation lock");
        Arc::ptr_eq(owner, &self.observer_owner)
            && state.generation == generation
            && state.sender.is_some()
    }
    pub fn capture_observer_retry(&self) -> Result<SidebarObserverRetry> {
        let state = self.observation.lock().expect("Sidebar observation lock");
        require(
            state.failed && state.sender.is_some(),
            "Sidebar observer is not retryable",
        )?;
        Ok(SidebarObserverRetry {
            owner: self.observer_owner.clone(),
            endpoint_id: self.endpoint.runtime.inspect(|s| s.endpoint_id.clone()),
            generation: state.generation,
        })
    }
    /// Capture only the failed attempt represented by a previously displayed
    /// original packet. Never substitute the latest generation at click time.
    pub fn capture_observer_retry_for(
        &self,
        key: &SidebarObserverKey,
    ) -> Result<SidebarObserverRetry> {
        self.capture_observer_retry_for_owner(key, &self.observer_owner)
    }
    pub(crate) fn capture_observer_retry_for_owner<S: std::any::Any + Send + Sync>(
        &self,
        key: &SidebarObserverKey,
        source_owner: &Arc<S>,
    ) -> Result<SidebarObserverRetry> {
        let state = self.observation.lock().expect("Sidebar observation lock");
        let endpoint_id = self.endpoint.runtime.inspect(|s| s.endpoint_id.clone());
        require(
            state.failed
                && state.sender.is_some()
                && key.matches_owners(
                    source_owner,
                    &self.observer_owner,
                    &endpoint_id,
                    state.generation,
                ),
            "stale Sidebar observer display key",
        )?;
        Ok(SidebarObserverRetry {
            owner: self.observer_owner.clone(),
            endpoint_id,
            generation: state.generation,
        })
    }
    /// Read one authentic immutable publication, then derive both display and
    /// key from it. No externally supplied publication or generation is accepted.
    pub fn observer_snapshot(&self) -> Result<SidebarObservationPacket<SidebarPublication>> {
        let publication = self.snapshot();
        let endpoint_id = self.endpoint.runtime.inspect(|s| s.endpoint_id.clone());
        let status = SidebarObserverStatusInput {
            source: SidebarObserverSourceKey::from_owner(&self.observer_owner),
            revision: publication.revision,
            endpoints: vec![SidebarObserverEndpointInput {
                key: SidebarObserverKey::from_owners(
                    &self.observer_owner,
                    &self.observer_owner,
                    &endpoint_id,
                    publication.observation.generation,
                )?,
                revision: publication.revision,
                observation: publication.observation,
                error: publication.error.clone(),
            }],
        };
        status.validate()?;
        Ok(SidebarObservationPacket::new(publication, status))
    }
    /// Explicitly re-open only the existing state follow. No transport or RPC retry.
    pub fn restart_observer(&self, retry: SidebarObserverRetry) -> Result<()> {
        let mut state = self.observation.lock().expect("Sidebar observation lock");
        require(
            Arc::ptr_eq(&retry.owner, &self.observer_owner)
                && retry.endpoint_id == self.endpoint.runtime.inspect(|s| s.endpoint_id.clone())
                && retry.generation == state.generation
                && state.failed,
            "stale Sidebar observer retry",
        )?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unsupported("Sidebar observer generation exhausted"))?;
        let revision = state.next_revision()?;
        let mut last = (**state.sender.as_ref().expect("live sender").borrow()).clone();
        last.revision = revision;
        last.observation.generation = generation;
        last.observation.phase = SidebarObserverPhase::Restarting;
        for row in &mut last.rows {
            row.activity.is_stale = true;
        }
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.generation = generation;
        state.failed = false;
        state
            .sender
            .as_ref()
            .expect("live sender")
            .send_replace(Arc::new(last));
        // The task cannot publish until this same lock has been released.
        state.task = Some(self.spawn_observer(generation));
        Ok(())
    }
    fn spawn_observer(&self, generation: u64) -> tokio::task::JoinHandle<()> {
        let owner = self.agent.clone();
        let source = self.endpoint.clone();
        let state = self.observation.clone();
        self.executor.spawn(async move {
            let (output, mut frames) = mpsc::channel(8);
            let request = json!({"endpoint":source.runtime.inspect(|s| s.endpoint_id.clone()), "target":"state"});
            let follow = owner.follow(&request, output);
            tokio::pin!(follow);
            let result: Result<()> = loop {
                tokio::select! {
                    result = &mut follow => break result.and_then(|()| Err(unsupported("inventory observation ended"))),
                    frame = frames.recv() => {
                        let Some(frame) = frame else { break Err(unsupported("inventory channel ended")); };
                        match publish_inventory_frame(&state, &owner, &source, generation, &frame) {
                            Ok(true) => {}, Ok(false) => return, Err(error) => break Err(error),
                        }
                    }
                }
            };
            if let Err(error) = result { publish_observer_failure(&state, generation, error); }
        })
    }
    pub fn preferences(&self) -> Option<tekes_agent_presentation::SidebarPreferencesSnapshot> {
        self.agent.sidebar_preferences()
    }
    pub fn set_preferences(
        &self,
        change: &tekes_agent_presentation::SidebarPreferenceChange,
    ) -> Result<()> {
        self.agent.set_sidebar_preferences(change)
    }
    pub fn snapshot(&self) -> Arc<SidebarPublication> {
        self.publications.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarPublication>> {
        self.publications.clone()
    }
    pub fn runtime(&self) -> &Arc<tekes_agent_session::SessionRuntime> {
        &self.endpoint.runtime
    }

    /// Reuse the published shared dispatcher and the caller's stable RPC identity.
    /// Outcomes describe navigation/selection changes; this observer never applies them.
    pub async fn perform(
        &self,
        address: &SessionAddress,
        definition: &ThreadActionDefinition,
        title: Option<String>,
        rpc_id: &str,
    ) -> Result<SidebarActionOutcome> {
        require(
            address.endpoint_id
                == self
                    .endpoint
                    .runtime
                    .inspect(|state| state.endpoint_id.clone()),
            "sidebar endpoint mismatch",
        )?;
        require(
            definition.enabled,
            definition
                .disabled_reason
                .as_deref()
                .unwrap_or("thread action unavailable"),
        )?;
        if definition.interaction == "copy" {
            let menu = self
                .agent
                .thread_actions(&self.endpoint, &address.session_id)?;
            let current = menu
                .items
                .iter()
                .find(|item| item.id == definition.id)
                .ok_or_else(|| unsupported("thread action no longer exists"))?;
            require(
                current.enabled && current.interaction == "copy",
                "copy action changed",
            )?;
            let text = current
                .copy_text
                .clone()
                .ok_or_else(|| unsupported("missing shared copy value"))?;
            return Ok(SidebarActionOutcome::Copy {
                address: address.clone(),
                text,
            });
        }
        require(
            !rpc_id.trim().is_empty(),
            "thread mutation requires RPC identity",
        )?;
        let request = ThreadActionRequest {
            session_id: address.session_id.clone(),
            action_id: definition.id.clone(),
            title,
        };
        let outcome = serde_json::from_value(self.agent.request(&json!({"endpoint":address.endpoint_id,"op":"call","method":"thread.performAction","payload":request,"rpcId":rpc_id})).await?)?;
        Ok(SidebarActionOutcome::Mutation {
            address: address.clone(),
            outcome,
        })
    }
}
fn publish_inventory_frame(
    state: &Arc<Mutex<ObservationState>>,
    agent: &SharedAgent,
    endpoint: &SharedEndpoint,
    generation: u64,
    frame: &Value,
) -> Result<bool> {
    let (previous, mut candidate) = {
        let state = state.lock().expect("Sidebar observation lock");
        if state.generation != generation || state.failed || state.sender.is_none() {
            return Ok(false);
        }
        let previous = state.sender.as_ref().expect("live sender").borrow().clone();
        (previous, state.inventory.clone())
    };
    // Thread-action host callbacks may inspect this binding. Never invoke them
    // under its lifecycle lock; recheck authority before committing the frame.
    let rows = candidate.read(agent, endpoint, frame)?;
    let frame_inventory_ready = frame["inventoryReady"].as_bool().unwrap_or(false);
    let endpoint_id = endpoint.runtime.inspect(|store| store.endpoint_id.clone());
    let name_corpus = match frame.get("nameCorpus").filter(|value| !value.is_null()) {
        Some(value) => {
            let corpus: SidebarNameCorpus = serde_json::from_value(value.clone())?;
            corpus.validate()?;
            require(
                corpus.endpoint_id == endpoint_id,
                "Sidebar name corpus endpoint mismatch",
            )?;
            require(
                frame["connectionEpoch"].as_u64() == Some(corpus.connection_epoch),
                "Sidebar name corpus connection epoch mismatch",
            )?;
            require(
                corpus.inventory_ready == frame_inventory_ready,
                "Sidebar name corpus readiness mismatch",
            )?;
            Some(corpus)
        }
        // A newer inventory without a corpus is a legacy membership source,
        // not authority to combine new rows with an older ranking corpus.
        None => None,
    };
    let mut state = state.lock().expect("Sidebar observation lock");
    if state.generation != generation || state.failed || state.sender.is_none() {
        return Ok(false);
    }
    if previous.observation.phase == SidebarObserverPhase::Restarting
        && !frame["inventoryReady"].as_bool().unwrap_or(false)
    {
        return Ok(true);
    }
    let revision = state.next_revision()?;
    state.inventory = candidate;
    let loaded = previous.loaded || frame_inventory_ready;
    // Keep canonical activity for every active inventory item from this same
    // validated frame. Display membership may omit subagents and ephemeral
    // sessions, but search still needs to distinguish those known sessions
    // from genuinely missing inventory entries.
    let activity_at = inventory_activity_at(&endpoint_id, &state.inventory.activity);
    let observation = SidebarObservation {
        generation,
        phase: SidebarObserverPhase::Observing,
        converged: previous.observation.converged || loaded,
    };
    state
        .sender
        .as_ref()
        .expect("live sender")
        .send_replace(Arc::new(SidebarPublication {
            revision,
            observation,
            rows,
            activity_at,
            name_corpus,
            loaded,
            error: None,
        }));
    Ok(true)
}
fn publish_observer_failure(
    state: &Arc<Mutex<ObservationState>>,
    generation: u64,
    error: tekes_agent_contract::Error,
) {
    let mut state = state.lock().expect("Sidebar observation lock");
    if state.generation != generation || state.failed || state.sender.is_none() {
        return;
    }
    state.failed = true;
    let Ok(revision) = state.next_revision() else {
        return;
    };
    let mut last = (**state.sender.as_ref().expect("live sender").borrow()).clone();
    last.revision = revision;
    last.observation.phase = SidebarObserverPhase::Failed;
    last.observation.converged = true;
    last.error = Some(error.to_string());
    for row in &mut last.rows {
        row.activity.is_stale = true;
    }
    state
        .sender
        .as_ref()
        .expect("live sender")
        .send_replace(Arc::new(last));
}
/// Display-only activity metadata. Consecutive running updates retain the
/// original activity position, matching ClientKit's sidebar_activity_at rule.
#[derive(Clone, Default)]
struct InventoryRows {
    activity: std::collections::BTreeMap<String, (bool, f64)>,
}
fn inventory_activity_at(
    endpoint_id: &str,
    activity: &std::collections::BTreeMap<String, (bool, f64)>,
) -> std::collections::BTreeMap<SessionAddress, f64> {
    activity
        .iter()
        .map(|(session_id, (_, effective))| {
            (
                SessionAddress {
                    endpoint_id: endpoint_id.to_owned(),
                    session_id: session_id.clone(),
                },
                *effective,
            )
        })
        .collect()
}
impl InventoryRows {
    fn read(
        &mut self,
        agent: &SharedAgent,
        endpoint: &SharedEndpoint,
        frame: &Value,
    ) -> Result<Vec<SidebarRow>> {
        let endpoint_id = endpoint.runtime.inspect(|state| state.endpoint_id.clone());
        require(
            frame["description"]["id"] == endpoint_id,
            "inventory endpoint mismatch",
        )?;
        let items = frame["sessions"]
            .as_array()
            .ok_or_else(|| unsupported("invalid inventory rows"))?;
        let mut addresses = std::collections::BTreeSet::new();
        let mut activity = std::collections::BTreeMap::new();
        let mut rows = Vec::new();
        for item in items {
            let id = tekes_agent_contract::string(item, "sessionId")?;
            require(
                !id.is_empty() && addresses.insert(id),
                "duplicate or empty inventory identity",
            )?;
            let summary = &item["summary"];
            let updated = summary["updatedAt"]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| unsupported("missing inventory activity time"))?;
            let row_activity: tekes_agent_contract::ThreadActivity =
                serde_json::from_value(item["activity"].clone())?;
            let running = row_activity.status == "running";
            let effective = match self.activity.get(id) {
                Some((true, previous)) if running => *previous,
                _ => updated,
            };
            activity.insert(id.to_owned(), (running, effective));
            if sidebar_row_is_member(item) {
                rows.push(inventory_row(
                    &endpoint_id,
                    item,
                    agent.thread_actions(endpoint, id)?,
                )?);
            }
        }
        rows.sort_by(|a, b| {
            activity[&b.address.session_id]
                .1
                .total_cmp(&activity[&a.address.session_id].1)
                .then_with(|| a.address.session_id.cmp(&b.address.session_id))
        });
        // Commit only after every row and action source was accepted.
        self.activity = activity;
        Ok(rows)
    }
}
/// Sidebar membership uses only normalized facts explicitly present in the
/// public inventory item. Missing or unknown classification stays visible for
/// compatibility and is not evidence that a row is a main thread.
pub(crate) fn sidebar_row_is_member(item: &Value) -> bool {
    item["summary"]["origin"] != "subagent" && item["summary"]["ephemeral"] != true
}

pub(crate) fn inventory_row(
    endpoint: &str,
    item: &Value,
    actions: ThreadActionsPresentation,
) -> Result<SidebarRow> {
    let id = tekes_agent_contract::string(item, "sessionId")?;
    require(
        !id.is_empty() && actions.endpoint == endpoint && actions.session_id == id,
        "invalid Sidebar row source",
    )?;
    require(
        item["summary"]["updatedAt"]
            .as_f64()
            .is_some_and(|v| v.is_finite()),
        "missing inventory activity time",
    )?;
    Ok(SidebarRow {
        primary_folder_id: None,
        folder_path: item["summary"]["cwd"].as_str().map(str::to_owned),
        address: SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: id.into(),
        },
        title: inventory_title(item, id),
        activity: serde_json::from_value(item["activity"].clone())?,
        pin: item
            .get("pin")
            .filter(|v| !v.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()?,
        actions,
    })
}
fn inventory_title(item: &Value, _id: &str) -> String {
    let canonical = item.get("projections").and_then(|p| p.get("title"));
    tekes_agent_presentation::canonical_session_title(&item["summary"], canonical)
}

#[cfg(test)]
mod inventory_title_tests {
    use super::inventory_title;
    use serde_json::json;

    #[test]
    fn canonical_clear_uses_default_without_resurrecting_summary_or_session_id() {
        for cleared_title in [json!(null), json!(""), json!(" \t\n ")] {
            let item = json!({
                "summary": {"title": "old title", "name": "old name"},
                "projections": {"title": cleared_title}
            });
            assert_eq!(inventory_title(&item, "session-id"), "New conversation");
        }
    }

    #[test]
    fn canonical_clear_in_summary_projection_is_also_authoritative() {
        let item = json!({
            "summary": {
                "title": "old title",
                "projections": {"values": {"title": "  "}}
            }
        });
        assert_eq!(inventory_title(&item, "session-id"), "New conversation");
    }

    #[test]
    fn canonical_and_legacy_titles_are_trimmed_and_legacy_name_is_supported() {
        assert_eq!(
            inventory_title(&json!({"projections":{"title":"  explicit title  "}}), "id"),
            "explicit title"
        );
        assert_eq!(
            inventory_title(&json!({"summary":{"title":"  legacy title  "}}), "id"),
            "legacy title"
        );
        assert_eq!(
            inventory_title(
                &json!({"summary":{"title":"  ","name":"  legacy name  "}}),
                "id"
            ),
            "legacy name"
        );
    }

    #[test]
    fn unnamed_legacy_uses_default_but_explicit_id_title_is_valid() {
        assert_eq!(
            inventory_title(&json!({"summary":{}}), "session-id"),
            "New conversation"
        );
        assert_eq!(
            inventory_title(&json!({"projections":{"title":"session-id"}}), "session-id"),
            "session-id"
        );
    }
}

#[cfg(test)]
#[path = "sidebar_observer_tests.rs"]
mod observer_recovery_tests;

#[cfg(test)]
mod membership_tests {
    use super::{inventory_activity_at, sidebar_row_is_member};
    use serde_json::json;
    use std::collections::BTreeMap;
    use tekes_agent_contract::SessionAddress;

    #[test]
    fn membership_excludes_only_explicit_subagents_and_ephemeral_rows() {
        for (summary, expected) in [
            (json!({"origin":"main","ephemeral":false}), true),
            (json!({"origin":"subagent"}), false),
            (json!({"ephemeral":true}), false),
            (json!({"parentSessionId":"parent"}), true),
            (json!({}), true),
            (json!({"origin":null,"ephemeral":null}), true),
            (json!({"origin":"unknown","ephemeral":"true"}), true),
        ] {
            assert_eq!(
                sidebar_row_is_member(&json!({"summary":summary})),
                expected,
                "summary classification: {summary}"
            );
        }
    }

    #[test]
    fn activity_publication_keeps_known_sessions_outside_display_membership() {
        let activity = BTreeMap::from([
            ("main".to_owned(), (false, 2.0)),
            ("child".to_owned(), (true, 1.0)),
            ("ephemeral".to_owned(), (false, 0.5)),
        ]);
        let published = inventory_activity_at("endpoint-a", &activity);

        assert_eq!(published.len(), 3);
        assert_eq!(
            published[&SessionAddress {
                endpoint_id: "endpoint-a".into(),
                session_id: "child".into(),
            }],
            1.0
        );
        assert!(!published.contains_key(&SessionAddress {
            endpoint_id: "endpoint-b".into(),
            session_id: "child".into(),
        }));
    }
}
