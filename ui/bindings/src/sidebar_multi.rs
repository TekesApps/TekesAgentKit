//! Fixed-set multi-endpoint projection over the existing Sidebar observers.
//! This module merges publications and routes captured requests; it owns no
//! endpoint transport, search state, or second Sidebar mutation policy.
use crate::{
    SidebarActionOutcome, SidebarBinding, SidebarObservation, SidebarObserverPhase,
    SidebarObserverRetry, SidebarPublication,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tekes_agent_components_ui::{SidebarFolderOption, SidebarInput, SidebarRow};
use tekes_agent_contract::{Result, SessionAddress, ThreadActionDefinition, require, unsupported};
use tekes_agent_presentation::SidebarNameCorpus;
use tekes_agent_web::SharedAgent;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

/// Non-zero-sized allocation used only for binding-instance identity.
#[derive(Debug)]
pub(crate) struct SidebarMultiOwner {
    _identity: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarEndpointStatus {
    pub endpoint_id: String,
    /// Revision from this endpoint's original SidebarBinding. Never compared
    /// with another endpoint's revision.
    pub revision: u64,
    /// True only after this endpoint published its own initial inventory.
    pub loaded: bool,
    /// Observer generation and lifecycle for this endpoint's own binding.
    pub observation: SidebarObservation,
    /// True when rows are retained while this endpoint's observer is not
    /// currently observing.
    pub stale: bool,
    /// True only after this endpoint's observer enters its terminal phase.
    pub terminal_error: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarMultiPublication {
    pub revision: u64,
    pub rows: Vec<SidebarRow>,
    pub activity_at: BTreeMap<SessionAddress, f64>,
    /// Retained-name corpus from each original child endpoint publication.
    pub name_corpora: BTreeMap<String, SidebarNameCorpus>,
    /// Initial aggregation has converged when every endpoint published once or
    /// reached a terminal observer error; inspect endpoint status for health.
    pub loaded: bool,
    /// Deterministic endpoint-qualified summary. Per-endpoint detail is in
    /// `endpoints` and is never collapsed into this field.
    pub error: Option<String>,
    pub endpoints: BTreeMap<String, SidebarEndpointStatus>,
}
impl SidebarMultiPublication {
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
    pub fn folder_catalog(&self) -> std::result::Result<crate::SidebarFolderCatalog, &'static str> {
        if self.revision == 0 {
            return Err("unversioned multi-endpoint Sidebar catalog");
        }
        Ok(crate::SidebarFolderCatalog {
            revision: self.revision,
            loaded: self.loaded,
            error: self.error.clone(),
            folders: if self.loaded {
                tekes_agent_components_ui::sidebar_folder_catalog(&self.rows, &self.activity_at)?
            } else {
                Vec::<SidebarFolderOption>::new()
            },
        })
    }
}

/// One-shot action request captured from a specific aggregate instance and
/// address. It cannot be replayed against a replacement aggregate binding.
pub struct SidebarMultiActionTicket {
    owner: Arc<SidebarMultiOwner>,
    address: SessionAddress,
    definition: ThreadActionDefinition,
    observer_generation: u64,
}
impl SidebarMultiActionTicket {
    pub fn address(&self) -> &SessionAddress {
        &self.address
    }
    pub fn definition(&self) -> &ThreadActionDefinition {
        &self.definition
    }
}

/// One-shot recovery request fixed to one child endpoint and aggregate owner.
/// Aggregate revisions do not invalidate it; the child token fences recovery
/// against replacement of that endpoint observer.
pub struct SidebarMultiObserverRetry {
    owner: Arc<SidebarMultiOwner>,
    inner: SidebarObserverRetry,
}

struct EndpointUpdate {
    index: usize,
    publication: Arc<SidebarPublication>,
}

/// Inventory aggregation with an endpoint set fixed for this instance.
/// Existing `SidebarBinding` users retain their single-endpoint API unchanged.
pub struct SidebarMultiEndpointBinding {
    agent: Arc<SharedAgent>,
    bindings: Vec<SidebarBinding>,
    indices: BTreeMap<String, usize>,
    endpoint_ids: Vec<String>,
    owner: Arc<SidebarMultiOwner>,
    publications: watch::Receiver<Arc<SidebarMultiPublication>>,
    observer: JoinHandle<()>,
    bridges: Vec<JoinHandle<()>>,
}
impl Drop for SidebarMultiEndpointBinding {
    fn drop(&mut self) {
        self.observer.abort();
        for bridge in &self.bridges {
            bridge.abort();
        }
    }
}
impl SidebarMultiEndpointBinding {
    pub fn new<I, S>(agent: Arc<SharedAgent>, endpoint_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| unsupported("SidebarMultiEndpointBinding requires an async runtime"))?;
        let endpoint_ids: Vec<String> = endpoint_ids
            .into_iter()
            .map(|id| id.as_ref().to_owned())
            .collect();
        require(!endpoint_ids.is_empty(), "Sidebar endpoint set is empty")?;
        let mut indices = BTreeMap::new();
        for (index, endpoint_id) in endpoint_ids.iter().enumerate() {
            require(
                !endpoint_id.trim().is_empty(),
                "Sidebar endpoint ID is empty",
            )?;
            require(
                indices.insert(endpoint_id.clone(), index).is_none(),
                "duplicate Sidebar endpoint ID",
            )?;
            let endpoint = agent.endpoint(endpoint_id)?;
            require(
                endpoint
                    .runtime
                    .inspect(|state| state.endpoint_id == *endpoint_id),
                "Sidebar endpoint identity mismatch",
            )?;
        }

        let mut bindings = Vec::with_capacity(endpoint_ids.len());
        let mut initial = Vec::with_capacity(endpoint_ids.len());
        let (updates_tx, mut updates_rx) =
            mpsc::channel::<EndpointUpdate>(endpoint_ids.len().saturating_mul(4).clamp(8, 128));
        let mut bridges = Vec::with_capacity(endpoint_ids.len());
        for (index, endpoint_id) in endpoint_ids.iter().enumerate() {
            let binding = SidebarBinding::new(agent.clone(), endpoint_id)?;
            let mut receiver = binding.subscribe();
            initial.push(receiver.borrow_and_update().clone());
            let sender = updates_tx.clone();
            bridges.push(executor.spawn(async move {
                while receiver.changed().await.is_ok() {
                    let publication = receiver.borrow_and_update().clone();
                    if sender
                        .send(EndpointUpdate { index, publication })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }));
            bindings.push(binding);
        }
        drop(updates_tx);

        let owner = Arc::new(SidebarMultiOwner { _identity: 0 });
        let initial_publication = Arc::new(aggregate_publication(1, &endpoint_ids, &initial)?);
        let (sender, publications) = watch::channel(initial_publication);
        let observed_endpoint_ids = endpoint_ids.clone();
        let task = executor.spawn(async move {
            let mut latest = initial;
            let mut revision = 1_u64;
            while let Some(update) = updates_rx.recv().await {
                let Some(next_revision) = revision.checked_add(1) else {
                    // Closing the watch channel is the terminal, observable
                    // overflow signal; never publish a repeated revision.
                    break;
                };
                let Some(slot) = latest.get_mut(update.index) else {
                    break;
                };
                *slot = update.publication;
                let next =
                    match aggregate_publication(next_revision, &observed_endpoint_ids, &latest) {
                        Ok(value) => value,
                        Err(_) => break,
                    };
                revision = next_revision;
                sender.send_replace(Arc::new(next));
            }
        });

        Ok(Self {
            agent,
            bindings,
            indices,
            endpoint_ids,
            owner,
            publications,
            observer: task,
            bridges,
        })
    }

    pub fn endpoint_ids(&self) -> &[String] {
        &self.endpoint_ids
    }
    pub fn snapshot(&self) -> Arc<SidebarMultiPublication> {
        self.publications.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarMultiPublication>> {
        self.publications.clone()
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
    pub(crate) fn binding_for(&self, endpoint_id: &str) -> Option<&SidebarBinding> {
        self.indices
            .get(endpoint_id)
            .and_then(|index| self.bindings.get(*index))
    }
    pub(crate) fn owns(&self, owner: &Arc<SidebarMultiOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
    pub(crate) fn owner(&self) -> Arc<SidebarMultiOwner> {
        self.owner.clone()
    }

    /// Capture recovery for one endpoint in this fixed set. The resulting
    /// token remains bound to that endpoint even as other partitions advance.
    pub fn capture_observer_retry(&self, endpoint_id: &str) -> Result<SidebarMultiObserverRetry> {
        let binding = self
            .binding_for(endpoint_id)
            .ok_or_else(|| unsupported("Sidebar endpoint is outside the fixed set"))?;
        Ok(SidebarMultiObserverRetry {
            owner: self.owner.clone(),
            inner: binding.capture_observer_retry()?,
        })
    }

    /// Admit only the exact original aggregate/child attempt displayed by the
    /// UI. Unrelated endpoint publications do not invalidate this key.
    pub fn capture_observer_retry_for(
        &self,
        key: &tekes_agent_presentation::SidebarObserverKey,
    ) -> Result<SidebarMultiObserverRetry> {
        let binding = self
            .binding_for(key.endpoint_id())
            .ok_or_else(|| unsupported("Sidebar endpoint is outside the fixed set"))?;
        Ok(SidebarMultiObserverRetry {
            owner: self.owner.clone(),
            inner: binding.capture_observer_retry_for_owner(key, &self.owner)?,
        })
    }
    /// The aggregate publication already retains each original child's status.
    /// Derive keys from that single immutable publication and fixed owners;
    /// never join it with newer child snapshots or the flattened host facade.
    pub fn observer_snapshot(
        &self,
    ) -> Result<crate::SidebarObservationPacket<SidebarMultiPublication>> {
        use tekes_agent_presentation::{
            SidebarObserverEndpointInput, SidebarObserverKey, SidebarObserverSourceKey,
            SidebarObserverStatusInput,
        };
        let publication = self.snapshot();
        let endpoints = self
            .endpoint_ids
            .iter()
            .map(|endpoint_id| {
                let child = publication
                    .endpoints
                    .get(endpoint_id)
                    .ok_or_else(|| unsupported("missing original Sidebar endpoint status"))?;
                let binding = self
                    .binding_for(endpoint_id)
                    .ok_or_else(|| unsupported("missing original Sidebar endpoint binding"))?;
                Ok(SidebarObserverEndpointInput {
                    key: SidebarObserverKey::from_owners(
                        &self.owner,
                        &binding.observer_owner(),
                        endpoint_id,
                        child.observation.generation,
                    )?,
                    revision: child.revision,
                    observation: child.observation,
                    error: child.error.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let status = SidebarObserverStatusInput {
            source: SidebarObserverSourceKey::from_owner(&self.owner),
            revision: publication.revision,
            endpoints,
        };
        status.validate()?;
        Ok(crate::SidebarObservationPacket::new(publication, status))
    }
    /// Restart only the endpoint named by the captured child token.
    pub fn restart_observer(&self, retry: SidebarMultiObserverRetry) -> Result<()> {
        require(
            self.owns(&retry.owner),
            "Sidebar observer retry belongs to a replaced aggregate binding",
        )?;
        let endpoint_id = retry.inner.endpoint_id();
        let binding = self
            .binding_for(endpoint_id)
            .ok_or_else(|| unsupported("Sidebar endpoint is outside the fixed set"))?;
        binding.restart_observer(retry.inner)
    }

    /// Capture a currently advertised action from this aggregate publication.
    /// Stale endpoint rows and forged or changed action definitions are rejected.
    pub fn capture_action(
        &self,
        address: &SessionAddress,
        definition: &ThreadActionDefinition,
    ) -> Result<SidebarMultiActionTicket> {
        let binding = self
            .binding_for(&address.endpoint_id)
            .ok_or_else(|| unsupported("Sidebar endpoint is outside the fixed set"))?;
        let publication = binding.snapshot();
        validate_action_publication(&publication, address, definition)?;
        require(
            binding.owns_observer(
                &binding.observer_owner(),
                publication.observation.generation,
            ),
            "Sidebar observer is retired",
        )?;
        Ok(SidebarMultiActionTicket {
            owner: self.owner.clone(),
            address: address.clone(),
            definition: definition.clone(),
            observer_generation: publication.observation.generation,
        })
    }

    /// Consume a captured action through its original endpoint binding. Both
    /// owner identity and the endpoint's live shared action definition are
    /// checked again immediately before dispatch.
    pub async fn perform_action(
        &self,
        ticket: SidebarMultiActionTicket,
        title: Option<String>,
        rpc_id: &str,
    ) -> Result<SidebarActionOutcome> {
        require(
            self.owns(&ticket.owner),
            "Sidebar action belongs to a replaced aggregate binding",
        )?;
        let binding = self
            .binding_for(&ticket.address.endpoint_id)
            .ok_or_else(|| unsupported("Sidebar endpoint is outside the fixed set"))?;
        let publication = binding.snapshot();
        require(
            publication.observation.generation == ticket.observer_generation
                && binding.owns_observer(&binding.observer_owner(), ticket.observer_generation),
            "Sidebar action belongs to an older observer generation",
        )?;
        // A ticket is tied to the aggregate instance, but its endpoint may
        // have advanced or failed since capture. Recheck the original child
        // publication before consulting SharedAgent's live action catalog.
        validate_action_publication(&publication, &ticket.address, &ticket.definition)?;
        let current = self
            .agent
            .thread_actions(&binding.endpoint, &ticket.address.session_id)?;
        require(
            current.endpoint == ticket.address.endpoint_id
                && current.session_id == ticket.address.session_id,
            "Sidebar action source changed",
        )?;
        let current_definition = current
            .items
            .iter()
            .find(|item| item.id == ticket.definition.id)
            .ok_or_else(|| unsupported("thread action no longer exists"))?;
        require(
            current_definition.enabled && current_definition == &ticket.definition,
            "thread action changed",
        )?;
        binding
            .perform(&ticket.address, &ticket.definition, title, rpc_id)
            .await
    }
}

fn validate_action_publication(
    publication: &SidebarPublication,
    address: &SessionAddress,
    definition: &ThreadActionDefinition,
) -> Result<()> {
    require(
        publication.loaded && publication.error.is_none(),
        "Sidebar endpoint inventory is not current",
    )?;
    let row = publication
        .rows
        .iter()
        .find(|row| row.address == *address)
        .ok_or_else(|| unsupported("Sidebar action row is no longer present"))?;
    require(!row.activity.is_stale, "Sidebar action row is stale")?;
    require(
        definition.enabled
            && row.actions.endpoint == address.endpoint_id
            && row.actions.session_id == address.session_id
            && row
                .actions
                .items
                .iter()
                .any(|current| current == definition),
        "Sidebar action definition is no longer current",
    )
}

fn aggregate_publication(
    revision: u64,
    endpoint_ids: &[String],
    sources: &[Arc<SidebarPublication>],
) -> Result<SidebarMultiPublication> {
    require(
        revision > 0,
        "unversioned multi-endpoint Sidebar publication",
    )?;
    require(
        endpoint_ids.len() == sources.len(),
        "Sidebar source count differs from fixed endpoint set",
    )?;
    let mut rows = Vec::new();
    let mut activity_at = BTreeMap::new();
    let mut addresses = BTreeSet::new();
    let mut endpoints = BTreeMap::new();
    let mut name_corpora = BTreeMap::new();
    for (endpoint_id, source) in endpoint_ids.iter().zip(sources) {
        if let Some(corpus) = &source.name_corpus {
            require(
                corpus.endpoint_id == *endpoint_id,
                "Sidebar name corpus escaped its endpoint partition",
            )?;
            corpus.validate()?;
            name_corpora.insert(endpoint_id.clone(), corpus.clone());
        }
        let observer_stale = source.observation.phase != SidebarObserverPhase::Observing;
        for row in &source.rows {
            require(
                row.address.endpoint_id == *endpoint_id,
                "Sidebar row escaped its endpoint partition",
            )?;
            require(
                addresses.insert(row.address.clone()),
                "duplicate full Sidebar address across endpoint sources",
            )?;
            let activity = source
                .activity_at
                .get(&row.address)
                .copied()
                .ok_or_else(|| unsupported("Sidebar activity missing for address"))?;
            require(activity.is_finite(), "invalid Sidebar activity value")?;
            activity_at.insert(row.address.clone(), activity);
            let mut row = row.clone();
            if observer_stale {
                row.activity.is_stale = true;
            }
            rows.push(row);
        }
        let terminal_error = source.observation.phase == SidebarObserverPhase::Failed;
        endpoints.insert(
            endpoint_id.clone(),
            SidebarEndpointStatus {
                endpoint_id: endpoint_id.clone(),
                revision: source.revision,
                loaded: source.loaded,
                observation: source.observation,
                stale: observer_stale && !source.rows.is_empty(),
                terminal_error,
                error: source.error.clone(),
            },
        );
    }
    rows.sort_by(|a, b| {
        activity_at[&b.address]
            .total_cmp(&activity_at[&a.address])
            .then_with(|| a.address.cmp(&b.address))
    });
    let loaded = endpoints
        .iter()
        .all(|(_, status)| status.loaded || status.observation.converged);
    let errors: Vec<_> = endpoints
        .iter()
        .filter_map(|(id, status)| status.error.as_ref().map(|error| format!("{id}: {error}")))
        .collect();
    Ok(SidebarMultiPublication {
        revision,
        rows,
        activity_at,
        name_corpora,
        loaded,
        error: (!errors.is_empty()).then(|| errors.join("; ")),
        endpoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tekes_agent_contract::{ThreadActionDefinition, ThreadActionsPresentation, ThreadActivity};

    fn row(endpoint: &str, session: &str, stale: bool) -> SidebarRow {
        SidebarRow {
            primary_folder_id: None,
            folder_path: Some("/fixture".into()),
            address: SessionAddress {
                endpoint_id: endpoint.into(),
                session_id: session.into(),
            },
            title: format!("{endpoint}:{session}"),
            activity: ThreadActivity {
                status: "idle".into(),
                is_stale: stale,
            },
            pin: None,
            actions: ThreadActionsPresentation {
                endpoint: endpoint.into(),
                session_id: session.into(),
                label: "Actions".into(),
                items: vec![ThreadActionDefinition {
                    id: "copySessionId".into(),
                    label: "Copy ID".into(),
                    icon: "copy".into(),
                    interaction: "copy".into(),
                    enabled: true,
                    disabled_reason: None,
                    copy_text: Some(session.into()),
                }],
            },
        }
    }
    fn publication(
        revision: u64,
        row: Option<SidebarRow>,
        loaded: bool,
        error: Option<&str>,
        activity: f64,
    ) -> Arc<SidebarPublication> {
        let phase = if error.is_some() {
            SidebarObserverPhase::Failed
        } else {
            SidebarObserverPhase::Observing
        };
        let mut rows = Vec::new();
        let mut activity_at = BTreeMap::new();
        if let Some(row) = row {
            activity_at.insert(row.address.clone(), activity);
            rows.push(row);
        }
        Arc::new(SidebarPublication {
            revision,
            rows,
            activity_at,
            name_corpus: None,
            loaded,
            error: error.map(str::to_owned),
            observation: SidebarObservation {
                generation: 1,
                phase,
                converged: loaded || error.is_some(),
            },
        })
    }

    fn restarting_publication(
        revision: u64,
        row: SidebarRow,
        activity: f64,
    ) -> Arc<SidebarPublication> {
        let mut activity_at = BTreeMap::new();
        activity_at.insert(row.address.clone(), activity);
        Arc::new(SidebarPublication {
            revision,
            rows: vec![row],
            activity_at,
            name_corpus: None,
            loaded: false,
            error: Some("previous observer failed".into()),
            observation: SidebarObservation {
                generation: 2,
                phase: SidebarObserverPhase::Restarting,
                converged: true,
            },
        })
    }

    #[test]
    fn terminal_partial_failure_keeps_partition_rows_and_marks_only_loaded_failure_stale() {
        let ids = vec!["a".to_owned(), "b".to_owned()];
        let merged = aggregate_publication(
            7,
            &ids,
            &[
                publication(3, Some(row("a", "same", false)), true, None, 2.0),
                publication(5, None, false, Some("inventory failed"), 0.0),
            ],
        )
        .unwrap();
        assert!(
            merged.loaded,
            "terminal error settles first-load convergence"
        );
        assert_eq!(merged.rows.len(), 1);
        assert_eq!(merged.rows[0].address.endpoint_id, "a");
        assert!(merged.endpoints["a"].loaded);
        assert!(!merged.endpoints["a"].stale);
        assert!(!merged.endpoints["b"].loaded);
        assert!(!merged.endpoints["b"].stale);
        assert!(merged.endpoints["b"].terminal_error);

        let retained = aggregate_publication(
            8,
            &ids,
            &[
                publication(3, Some(row("a", "same", false)), true, None, 2.0),
                publication(
                    6,
                    Some(row("b", "same", true)),
                    true,
                    Some("disconnected"),
                    1.0,
                ),
            ],
        )
        .unwrap();
        assert_eq!(retained.rows.len(), 2);
        assert!(retained.rows[1].activity.is_stale);
        assert!(retained.endpoints["b"].loaded);
        assert!(retained.endpoints["b"].stale);
    }

    #[test]
    fn merged_order_is_stable_for_equal_activity_and_keeps_same_session_ids() {
        let ids = vec!["endpoint-b".to_owned(), "endpoint-a".to_owned()];
        let merged = aggregate_publication(
            1,
            &ids,
            &[
                publication(4, Some(row("endpoint-b", "same", false)), true, None, 1.0),
                publication(9, Some(row("endpoint-a", "same", false)), true, None, 1.0),
            ],
        )
        .unwrap();
        assert_eq!(merged.rows.len(), 2);
        assert_eq!(merged.rows[0].address.endpoint_id, "endpoint-a");
        assert_eq!(merged.rows[1].address.endpoint_id, "endpoint-b");
    }

    #[test]
    fn restarting_unloaded_partition_keeps_rows_and_convergence_without_losing_healthy_partition() {
        let ids = vec!["endpoint-b".to_owned(), "endpoint-a".to_owned()];
        let merged = aggregate_publication(
            4,
            &ids,
            &[
                restarting_publication(7, row("endpoint-b", "kept", false), 2.0),
                publication(
                    9,
                    Some(row("endpoint-a", "healthy", false)),
                    true,
                    None,
                    1.0,
                ),
            ],
        )
        .unwrap();

        assert!(
            merged.loaded,
            "restart preserves the initial convergence state"
        );
        assert_eq!(merged.rows.len(), 2);
        let restarting = &merged.endpoints["endpoint-b"];
        assert!(!restarting.loaded);
        assert!(restarting.observation.converged);
        assert_eq!(restarting.observation.generation, 2);
        assert_eq!(
            restarting.observation.phase,
            SidebarObserverPhase::Restarting
        );
        assert!(!restarting.terminal_error);
        assert!(restarting.stale);
        assert!(
            merged
                .rows
                .iter()
                .find(|r| r.address.endpoint_id == "endpoint-b")
                .unwrap()
                .activity
                .is_stale
        );
        assert!(!merged.endpoints["endpoint-a"].stale);
        assert!(
            merged
                .rows
                .iter()
                .any(|r| r.address.endpoint_id == "endpoint-a")
        );
    }
}
