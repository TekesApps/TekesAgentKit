//! Demo-host adapter that composes the public single- and multi-endpoint Sidebar bindings.
//! It only normalizes publications and owner identity; inventory/action policy remains in bindings.
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_components_ui::SidebarInput;
use tekes_agent_contract::{Result, SessionAddress, ThreadActionDefinition, unsupported};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    SidebarActionOutcome, SidebarBinding, SidebarMultiEndpointBinding, SidebarMultiPublication,
    SidebarMultiSelectedRead, SidebarPublication, SidebarSelectedRead, TurnActionDispatcher,
};
use tekes_agent_web::SharedAgent;
use tokio::{sync::watch, task::JoinHandle};

#[derive(Clone)]
enum Source {
    Single(Arc<SidebarBinding>),
    Multi(Arc<SidebarMultiEndpointBinding>),
}

pub struct SidebarHostPublication {
    owner: Arc<()>,
    publication: SidebarPublication,
    endpoint_count: usize,
    multi: Option<Arc<SidebarMultiPublication>>,
    observer: tekes_agent_presentation::SidebarObserverStatusInput,
    observed_error: Option<String>,
}
// Read-only field access preserves the host's publication API without exposing
// mutable source facts under an owner token. There is deliberately no DerefMut.
impl std::ops::Deref for SidebarHostPublication {
    type Target = SidebarPublication;
    fn deref(&self) -> &Self::Target {
        &self.publication
    }
}
impl SidebarHostPublication {
    pub fn as_publication(&self) -> SidebarPublication {
        self.publication.clone()
    }
    pub fn observer_status(&self) -> &tekes_agent_presentation::SidebarObserverStatusInput {
        &self.observer
    }
    /// Deduplicate only diagnostics from this exact original packet. A fixture
    /// display projection or a separate Sidebar input cannot hide a new error.
    pub fn inventory_error_for_display(&self) -> Option<String> {
        let errors: Vec<_> = self
            .observer
            .endpoints
            .iter()
            .filter(|e| e.error.is_some())
            .collect();
        let represented = self.publication.error == self.observed_error
            && !errors.is_empty()
            && errors.iter().all(|e| {
                e.observation.phase != tekes_agent_presentation::SidebarObserverPhase::Observing
            });
        if represented {
            None
        } else {
            self.publication.error.clone()
        }
    }
    pub fn endpoint_count(&self) -> usize {
        self.endpoint_count
    }
    /// Preserve per-endpoint inventory status for shared multi-search joins.
    pub fn as_multi_publication(&self) -> Option<SidebarMultiPublication> {
        let mut source = self.multi.as_ref()?.as_ref().clone();
        source.revision = self.revision;
        source.rows = self.rows.clone();
        source.activity_at = self.activity_at.clone();
        source.loaded = self.loaded;
        source.error = self.error.clone();
        Some(source)
    }
    /// Owned restore fixture only: status/revision may change, rows may be
    /// hidden or marked stale, but their original identities and actions cannot
    /// be minted or changed under this publication's owner token.
    pub fn with_restore_fixture_projection(&self, source: &SidebarPublication) -> Result<Self> {
        let mut seen = std::collections::BTreeSet::new();
        let valid = source.revision >= self.revision
            && source.observation == self.observation
            && source.activity_at == self.activity_at
            && source.name_corpus == self.name_corpus
            && source.classification == self.classification
            && source.rows.iter().all(|row| {
                seen.insert(row.address.clone())
                    && self.rows.iter().any(|original| {
                        let mut restricted = original.clone();
                        restricted.activity.is_stale |= row.activity.is_stale;
                        &restricted == row
                    })
            });
        if !valid {
            return Err(unsupported("restore fixture changed Sidebar source facts"));
        }
        Ok(Self {
            owner: self.owner.clone(),
            publication: source.clone(),
            endpoint_count: self.endpoint_count,
            multi: self.multi.clone(),
            observer: self.observer.clone(),
            observed_error: self.observed_error.clone(),
        })
    }
    pub fn folder_catalog(&self) -> Result<tekes_agent_ui_bindings::SidebarFolderCatalog> {
        let folders =
            tekes_agent_components_ui::sidebar_folder_catalog(&self.rows, &self.activity_at)
                .map_err(unsupported)?;
        Ok(tekes_agent_ui_bindings::SidebarFolderCatalog {
            revision: self.revision,
            loaded: self.loaded,
            error: self.error.clone(),
            folders,
        })
    }
    pub(crate) fn belongs_to(&self, source: &SidebarHostBinding) -> bool {
        Arc::ptr_eq(&self.owner, &source.owner)
    }
}

enum Read {
    Single(SidebarSelectedRead),
    Multi(SidebarMultiSelectedRead),
}
pub struct SidebarHostSelectedRead {
    owner: Arc<()>,
    read: Read,
}
impl SidebarHostSelectedRead {
    pub fn address(&self) -> &SessionAddress {
        match &self.read {
            Read::Single(read) => read.address(),
            Read::Multi(read) => read.address(),
        }
    }
    pub fn is_current(&self, source: &SidebarHostBinding) -> bool {
        Arc::ptr_eq(&self.owner, &source.owner)
            && match &self.read {
                Read::Single(read) => match &source.source {
                    Source::Single(binding) => read.is_current(binding),
                    Source::Multi(_) => false,
                },
                Read::Multi(read) => match &source.source {
                    Source::Multi(binding) => read.is_current(binding),
                    Source::Single(_) => false,
                },
            }
    }
    pub fn has_active_row(&self, source: &SidebarHostBinding) -> bool {
        Arc::ptr_eq(&self.owner, &source.owner)
            && match &self.read {
                Read::Single(read) => match &source.source {
                    Source::Single(binding) => read.has_active_row(binding),
                    Source::Multi(_) => false,
                },
                Read::Multi(read) => match &source.source {
                    Source::Multi(binding) => read.has_active_row(binding),
                    Source::Single(_) => false,
                },
            }
    }
    pub fn project_into(
        &self,
        source: &SidebarHostBinding,
        display: &mut SidebarInput,
        generation: u64,
    ) -> bool {
        if !Arc::ptr_eq(&self.owner, &source.owner) {
            return false;
        }
        match (&self.read, &source.source) {
            (Read::Single(read), Source::Single(binding)) => {
                read.project_into(binding, display, generation)
            }
            (Read::Multi(read), Source::Multi(binding)) => {
                read.project_into(binding, display, generation)
            }
            _ => false,
        }
    }
}

/// Host lifetime wrapper around the exact public binding instance.
/// Its watch carries an owner token, so delayed publications from an old wrapper
/// cannot overwrite a replacement whose local revision restarted at one.
pub struct SidebarHostBinding {
    source: Source,
    owner: Arc<()>,
    agent: Arc<SharedAgent>,
    runtimes: BTreeMap<String, Arc<SessionRuntime>>,
    publications: watch::Receiver<Arc<SidebarHostPublication>>,
    bridges: Vec<JoinHandle<()>>,
}
impl Drop for SidebarHostBinding {
    fn drop(&mut self) {
        for bridge in &self.bridges {
            bridge.abort();
        }
    }
}
impl SidebarHostBinding {
    pub fn new<I, S>(agent: Arc<SharedAgent>, endpoint_ids: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let endpoint_ids: Vec<String> = endpoint_ids
            .into_iter()
            .map(|id| id.as_ref().to_owned())
            .collect();
        let mut runtimes = BTreeMap::new();
        for endpoint_id in &endpoint_ids {
            let endpoint = agent.endpoint(endpoint_id)?;
            runtimes.insert(endpoint_id.clone(), endpoint.runtime.clone());
        }
        let source = if endpoint_ids.len() == 1 {
            Source::Single(Arc::new(SidebarBinding::new(
                agent.clone(),
                &endpoint_ids[0],
            )?))
        } else {
            Source::Multi(Arc::new(SidebarMultiEndpointBinding::new(
                agent.clone(),
                &endpoint_ids,
            )?))
        };
        Self::start(source, agent, runtimes)
    }
    fn start(
        source: Source,
        agent: Arc<SharedAgent>,
        runtimes: BTreeMap<String, Arc<SessionRuntime>>,
    ) -> Result<Self> {
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| unsupported("Sidebar host binding requires an async runtime"))?;
        let owner = Arc::new(());
        let (initial, receiver) = match &source {
            Source::Single(binding) => {
                let mut receiver = binding.subscribe();
                receiver.borrow_and_update();
                let initial = Self::from_single(&owner, &binding.observer_snapshot()?);
                (initial, WatchSource::Single(receiver))
            }
            Source::Multi(binding) => {
                let mut receiver = binding.subscribe();
                receiver.borrow_and_update();
                let initial = Self::from_multi(&owner, &binding.observer_snapshot()?);
                (initial, WatchSource::Multi(receiver))
            }
        };
        let (sender, publications) = watch::channel(Arc::new(initial));
        // Notifications only wake this bridge. Each forwarded value is one
        // authentic packet, never a publication joined with a second snapshot.
        let original = source.clone();
        let bridge_owner = owner.clone();
        let bridge = executor.spawn(async move {
            match (receiver, original) {
                (WatchSource::Single(mut receiver), Source::Single(binding)) => {
                    while receiver.changed().await.is_ok() {
                        receiver.borrow_and_update();
                        let Ok(packet) = binding.observer_snapshot() else {
                            break;
                        };
                        sender.send_replace(Arc::new(Self::from_single(&bridge_owner, &packet)));
                    }
                }
                (WatchSource::Multi(mut receiver), Source::Multi(binding)) => {
                    while receiver.changed().await.is_ok() {
                        receiver.borrow_and_update();
                        let Ok(packet) = binding.observer_snapshot() else {
                            break;
                        };
                        sender.send_replace(Arc::new(Self::from_multi(&bridge_owner, &packet)));
                    }
                }
                _ => unreachable!("Sidebar bridge source is fixed"),
            }
        });
        let bridges = vec![bridge];
        Ok(Self {
            source,
            owner,
            agent,
            runtimes,
            publications,
            bridges,
        })
    }
    fn from_single(
        owner: &Arc<()>,
        packet: &tekes_agent_ui_bindings::SidebarObservationPacket<SidebarPublication>,
    ) -> SidebarHostPublication {
        let source = packet.publication();
        SidebarHostPublication {
            owner: owner.clone(),
            publication: source.as_ref().clone(),
            endpoint_count: 1,
            multi: None,
            observer: packet.status().clone(),
            observed_error: source.error.clone(),
        }
    }
    fn from_multi(
        owner: &Arc<()>,
        packet: &tekes_agent_ui_bindings::SidebarObservationPacket<SidebarMultiPublication>,
    ) -> SidebarHostPublication {
        let source = packet.publication();
        SidebarHostPublication {
            owner: owner.clone(),
            publication: SidebarPublication {
                classification: None,
                // The facade has no single observer; recovery uses the retained
                // per-endpoint publication and opaque original source token.
                observation: Default::default(),
                revision: source.revision,
                loaded: source.loaded,
                error: source.error.clone(),
                rows: source.rows.clone(),
                activity_at: source.activity_at.clone(),
                name_corpus: None,
            },
            endpoint_count: source.endpoints.len(),
            multi: Some(source.clone()),
            observer: packet.status().clone(),
            observed_error: source.error.clone(),
        }
    }
    pub fn snapshot(&self) -> Arc<SidebarHostPublication> {
        self.publications.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarHostPublication>> {
        self.publications.clone()
    }
    /// This is observer-only recovery. The key is captured by the displayed
    /// packet; a click never substitutes current generation or default endpoint.
    pub fn restart_observer_for(
        &self,
        key: &tekes_agent_presentation::SidebarObserverKey,
    ) -> Result<()> {
        match &self.source {
            Source::Single(binding) => {
                let token = binding.capture_observer_retry_for(key)?;
                binding.restart_observer(token)
            }
            Source::Multi(binding) => {
                let token = binding.capture_observer_retry_for(key)?;
                binding.restart_observer(token)
            }
        }
    }
    pub fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
    }
    pub fn endpoint_ids(&self) -> Vec<String> {
        match &self.source {
            Source::Single(binding) => vec![binding.runtime().inspect(|s| s.endpoint_id.clone())],
            Source::Multi(binding) => binding.endpoint_ids().to_vec(),
        }
    }
    pub fn runtime_for(&self, endpoint_id: &str) -> Option<&Arc<SessionRuntime>> {
        self.runtimes.get(endpoint_id)
    }
    /// The demo knows the explicitly configured harness family. A parent's
    /// layout must never follow whichever endpoint happened to be listed first.
    pub fn child_history_layout(
        &self,
        endpoint_id: &str,
    ) -> Result<tekes_agent_presentation::ChildHistoryLayout> {
        use tekes_agent_presentation::ChildHistoryLayout;
        match self.agent.endpoint(endpoint_id)?.kind.as_str() {
            "native" => Ok(ChildHistoryLayout::SameSession),
            "pi" | "dsh" => Ok(ChildHistoryLayout::IndependentSessions),
            _ => Err(unsupported("unknown demo child history layout")),
        }
    }
    pub fn turn_action_dispatcher(&self) -> TurnActionDispatcher {
        TurnActionDispatcher::new(self.agent.clone())
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
    pub fn read_selected(
        &self,
        address: &SessionAddress,
        generation: u64,
    ) -> Result<SidebarHostSelectedRead> {
        let read = match &self.source {
            Source::Single(binding) => Read::Single(binding.read_selected(address, generation)?),
            Source::Multi(binding) => Read::Multi(binding.read_selected(address, generation)?),
        };
        Ok(SidebarHostSelectedRead {
            owner: self.owner.clone(),
            read,
        })
    }
    pub async fn perform_action(
        &self,
        address: &SessionAddress,
        definition: &ThreadActionDefinition,
        title: Option<String>,
        rpc_id: &str,
    ) -> Result<SidebarActionOutcome> {
        match &self.source {
            Source::Single(binding) => binding.perform(address, definition, title, rpc_id).await,
            Source::Multi(binding) => {
                let ticket = binding.capture_action(address, definition)?;
                binding.perform_action(ticket, title, rpc_id).await
            }
        }
    }
}

enum WatchSource {
    Single(watch::Receiver<Arc<SidebarPublication>>),
    Multi(watch::Receiver<Arc<SidebarMultiPublication>>),
}
