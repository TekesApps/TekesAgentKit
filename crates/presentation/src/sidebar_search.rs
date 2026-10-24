//! Reader-owned query attempts over the public SessionRuntime. No Run/composer
//! state, native file lookup, local search index or host-owned query machine.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
/// Capture the source, endpoint epoch and query attempt, including ABA queries.
/// `owner` identifies one search source lifetime; generations are never recycled.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSearchKey {
    pub owner: String,
    pub endpoint_id: String,
    pub connection_epoch: u64,
    pub generation: u64,
    pub query: String,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SidebarSearchPhase {
    #[default]
    Idle,
    Searching,
    Complete {
        has_more: bool,
    },
    Failed {
        message: String,
        has_more: bool,
        retryable: bool,
    },
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSearchInput {
    /// Monotonic display publication, independent from inventory publications.
    pub revision: u64,
    /// None means search was cleared. No result or callback survives clearing.
    pub key: Option<SidebarSearchKey>,
    pub phase: SidebarSearchPhase,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SidebarSearchIntent {
    Retry { key: SidebarSearchKey },
}

use tekes_agent_contract::{
    Result, SessionAddress, SessionSearchHit, SessionSearchResults, require, unsupported,
};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tokio::{sync::watch, task::AbortHandle};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSearchMatch {
    pub address: SessionAddress,
    /// Provider-specific additions remain in `extra`.
    pub hit: SessionSearchHit,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSearchPublication {
    pub input: SidebarSearchInput,
    /// Exact host query, including surrounding whitespace. Empty/whitespace
    /// queries clear results without a remote read.
    pub query: String,
    /// Original provider order, deduplicated by identity; last snippet wins.
    pub hits: Vec<SidebarSearchMatch>,
    /// Never turn unknown inventory membership into an authoritative empty list.
    pub unresolved: Vec<SessionAddress>,
    pub result: Option<SessionSearchResults>,
}
/// Product copy is supplied by the host without moving query policy there.
#[derive(Clone, Debug)]
pub struct SidebarSearchMessages {
    pub unavailable: String,
    pub unsupported: String,
}
impl Default for SidebarSearchMessages {
    fn default() -> Self {
        Self {
            unavailable: "搜索连接尚未就绪".into(),
            unsupported: "此连接不支持会话内容搜索".into(),
        }
    }
}
struct State {
    publication: Arc<SidebarSearchPublication>,
    generation: u64,
    worker: Option<AbortHandle>,
    closed: bool,
    source_identity: Identity,
}
struct Inner {
    owner: String,
    messages: SidebarSearchMessages,
    runtime: Arc<SessionRuntime>,
    executor: tokio::runtime::Handle,
    state: Mutex<State>,
    publications: watch::Sender<Arc<SidebarSearchPublication>>,
}
/// One endpoint's search reader. Hosts pass product search text and observe
/// publications; debounce, epochs, retries and late-result rejection stay here.
pub struct SidebarSearchController {
    inner: Arc<Inner>,
    observer: tokio::task::JoinHandle<()>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
struct Identity {
    endpoint: String,
    epoch: u64,
    ready: bool,
    supported: bool,
}
fn identity(runtime: &SessionRuntime) -> Identity {
    runtime.inspect(|state| Identity {
        endpoint: state.endpoint_id.clone(),
        epoch: state.connection_epoch,
        ready: state.connection == ConnectionState::Ready,
        supported: state.endpoint_capabilities.supports("session.search"),
    })
}
impl SidebarSearchController {
    pub fn new(runtime: Arc<SessionRuntime>) -> Result<Self> {
        Self::new_with_messages(runtime, SidebarSearchMessages::default())
    }
    pub fn new_with_messages(
        runtime: Arc<SessionRuntime>,
        messages: SidebarSearchMessages,
    ) -> Result<Self> {
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| unsupported("SidebarSearchController requires an async runtime"))?;
        let initial = Arc::new(SidebarSearchPublication {
            input: SidebarSearchInput {
                revision: 1,
                key: None,
                phase: SidebarSearchPhase::Idle,
            },
            query: String::new(),
            hits: vec![],
            unresolved: vec![],
            result: None,
        });
        let (publications, _) = watch::channel(initial.clone());
        let inner = Arc::new(Inner {
            owner: uuid::Uuid::new_v4().to_string(),
            messages,
            runtime: runtime.clone(),
            executor: executor.clone(),
            state: Mutex::new(State {
                publication: initial,
                generation: 0,
                worker: None,
                closed: false,
                source_identity: identity(&runtime),
            }),
            publications,
        });
        let weak = Arc::downgrade(&inner);
        let mut changes = runtime.subscribe();
        let mut observed = identity(&runtime);
        let observer = executor.spawn(async move {
            loop {
                if changes.changed().await.is_err() {
                    break;
                }
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let current = identity(&inner.runtime);
                if current != observed {
                    observed = current;
                    let _ = Inner::begin(&inner, None, true, None);
                } else {
                    inner.reconcile_inventory();
                }
            }
        });
        Ok(Self { inner, observer })
    }
    pub fn owner(&self) -> &str {
        &self.inner.owner
    }
    pub fn snapshot(&self) -> Arc<SidebarSearchPublication> {
        self.inner.state.lock().unwrap().publication.clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarSearchPublication>> {
        self.inner.publications.subscribe()
    }
    /// An unchanged query does not issue a second request. A -> B -> A gets a
    /// new generation; clearing revokes the prior attempt immediately.
    pub fn set_query(&self, query: impl Into<String>) -> Result<()> {
        Inner::begin(&self.inner, Some(query.into()), false, None)
    }
    /// Reconcile the live connection, never a cached copy of the query. Repeated
    /// notifications for an already-applied identity do not restart a read.
    pub fn reconcile_connection(&self) -> Result<()> {
        Inner::begin(&self.inner, None, true, None)
    }
    pub fn retry(&self, intent: &SidebarSearchIntent) -> Result<()> {
        let SidebarSearchIntent::Retry { key } = intent;
        Inner::begin(&self.inner, Some(key.query.clone()), true, Some(key))
    }
}
impl Drop for SidebarSearchController {
    fn drop(&mut self) {
        self.observer.abort();
        let mut state = self.inner.state.lock().unwrap();
        state.closed = true;
        if let Some(worker) = state.worker.take() {
            worker.abort();
        }
        if let Some(revision) = state.publication.input.revision.checked_add(1) {
            let publication = Arc::new(SidebarSearchPublication {
                input: SidebarSearchInput {
                    revision,
                    key: None,
                    phase: SidebarSearchPhase::Idle,
                },
                query: String::new(),
                hits: vec![],
                unresolved: vec![],
                result: None,
            });
            state.publication = publication.clone();
            self.inner.publications.send_replace(publication);
        }
    }
}
impl Inner {
    fn begin(
        this: &Arc<Self>,
        query: Option<String>,
        force: bool,
        retry: Option<&SidebarSearchKey>,
    ) -> Result<()> {
        let mut state = this.state.lock().unwrap();
        let source = identity(&this.runtime);
        require(!state.closed, "search source closed")?;
        if query.is_none()
            && (state.source_identity == source || state.publication.query.trim().is_empty())
        {
            state.source_identity = source;
            return Ok(());
        }
        let query = query.unwrap_or_else(|| state.publication.query.clone());
        if let Some(key) = retry {
            require(
                state.publication.input.key.as_ref() == Some(key)
                    && matches!(
                        state.publication.input.phase,
                        SidebarSearchPhase::Failed {
                            retryable: true,
                            ..
                        }
                    )
                    && key.owner == this.owner
                    && key.endpoint_id == source.endpoint
                    && key.connection_epoch == source.epoch
                    && source.ready
                    && source.supported,
                "stale or unavailable search retry",
            )?;
        }
        if !force && state.publication.query == query && state.source_identity == source {
            return Ok(());
        }
        let revision = state
            .publication
            .input
            .revision
            .checked_add(1)
            .ok_or_else(|| unsupported("search publication exhausted"))?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unsupported("search attempt exhausted"))?;
        let key = (!query.trim().is_empty()).then(|| SidebarSearchKey {
            owner: this.owner.clone(),
            endpoint_id: source.endpoint.clone(),
            connection_epoch: source.epoch,
            generation,
            query: query.clone(),
        });
        let phase = if key.is_none() {
            SidebarSearchPhase::Idle
        } else if !source.ready {
            SidebarSearchPhase::Failed {
                message: this.messages.unavailable.clone(),
                has_more: false,
                retryable: false,
            }
        } else if !source.supported {
            SidebarSearchPhase::Failed {
                message: this.messages.unsupported.clone(),
                has_more: false,
                retryable: false,
            }
        } else {
            SidebarSearchPhase::Searching
        };
        if let Some(worker) = state.worker.take() {
            worker.abort();
        }
        state.generation = generation;
        state.source_identity = source;
        let run = phase == SidebarSearchPhase::Searching;
        let publication = Arc::new(SidebarSearchPublication {
            input: SidebarSearchInput {
                revision,
                key: key.clone(),
                phase,
            },
            query,
            hits: vec![],
            unresolved: vec![],
            result: None,
        });
        state.publication = publication.clone();
        this.publications.send_replace(publication);
        if run {
            let weak = Arc::downgrade(this);
            let key = key.unwrap();
            let runtime = this.runtime.clone();
            let worker = this.executor.spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if !Self::current(&weak, &key) {
                    return;
                }
                let rpc = format!("sidebar-search:{}:{}", key.owner, key.generation);
                let result = runtime
                    .search_sessions_at_epoch(key.connection_epoch, &key.query, &rpc)
                    .await;
                if let Some(inner) = weak.upgrade() {
                    inner.complete(&key, result);
                }
            });
            state.worker = Some(worker.abort_handle());
        }
        Ok(())
    }
    fn current(weak: &Weak<Self>, key: &SidebarSearchKey) -> bool {
        let Some(inner) = weak.upgrade() else {
            return false;
        };
        let state = inner.state.lock().unwrap();
        let source = identity(&inner.runtime);
        !state.closed
            && state.publication.input.key.as_ref() == Some(key)
            && source.ready
            && source.supported
            && source.epoch == key.connection_epoch
            && source.endpoint == key.endpoint_id
    }
    fn unresolved(&self, hits: &[SidebarSearchMatch]) -> Vec<SessionAddress> {
        self.runtime.inspect(|snapshot| {
            hits.iter()
                .filter(|item| {
                    snapshot
                        .archived_session_ids
                        .contains(&item.address.session_id)
                        || snapshot
                            .sessions
                            .get(&item.address.session_id)
                            .is_none_or(|session| session.summary.is_none())
                })
                .map(|item| item.address.clone())
                .collect()
        })
    }
    fn reconcile_inventory(&self) {
        let mut state = self.state.lock().unwrap();
        if state.closed || state.publication.result.is_none() {
            return;
        }
        let unresolved = self.unresolved(&state.publication.hits);
        if unresolved == state.publication.unresolved {
            return;
        }
        let Some(revision) = state.publication.input.revision.checked_add(1) else {
            return;
        };
        let mut value = (*state.publication).clone();
        value.input.revision = revision;
        value.unresolved = unresolved;
        let value = Arc::new(value);
        state.publication = value.clone();
        self.publications.send_replace(value);
    }
    fn complete(&self, key: &SidebarSearchKey, result: Result<SessionSearchResults>) {
        let mut state = self.state.lock().unwrap();
        let source = identity(&self.runtime);
        if state.closed
            || state.publication.input.key.as_ref() != Some(key)
            || source.epoch != key.connection_epoch
            || !source.ready
            || !source.supported
        {
            return;
        }
        let Some(revision) = state.publication.input.revision.checked_add(1) else {
            return;
        };
        let mut publication = (*state.publication).clone();
        publication.input.revision = revision;
        match result {
            Ok(result) => {
                let mut indices = BTreeMap::<String, usize>::new();
                for hit in &result.items {
                    if let Some(index) = indices.get(&hit.session_id) {
                        publication.hits[*index].hit = hit.clone();
                    } else {
                        indices.insert(hit.session_id.clone(), publication.hits.len());
                        publication.hits.push(SidebarSearchMatch {
                            address: SessionAddress {
                                endpoint_id: key.endpoint_id.clone(),
                                session_id: hit.session_id.clone(),
                            },
                            hit: hit.clone(),
                        });
                    }
                }
                publication.unresolved = self.unresolved(&publication.hits);
                publication.input.phase = SidebarSearchPhase::Complete {
                    has_more: result.has_more,
                };
                publication.result = Some(result);
            }
            Err(error) => {
                publication.input.phase = SidebarSearchPhase::Failed {
                    message: error.to_string(),
                    has_more: false,
                    retryable: true,
                }
            }
        }
        let publication = Arc::new(publication);
        state.worker = None;
        state.publication = publication.clone();
        self.publications.send_replace(publication);
    }
}
