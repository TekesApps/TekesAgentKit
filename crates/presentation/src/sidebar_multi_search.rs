//! Fixed-set coordination for endpoint-owned sidebar search readers.
//!
//! This layer owns aggregate query identity and publication only. Each endpoint
//! keeps its existing `SidebarSearchController` for debounce, epoch fencing,
//! capability checks, and retry behavior.
use crate::{
    SidebarSearchController, SidebarSearchIntent, SidebarSearchKey, SidebarSearchMessages,
    SidebarSearchPhase, SidebarSearchPublication,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_session::SessionRuntime;
use tokio::{sync::watch, task::JoinHandle};

/// Identifies one aggregate query lifetime, including A → B → A transitions.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarMultiSearchKey {
    pub owner: String,
    pub generation: u64,
    pub query: String,
}

/// Aggregate display identity. Child readers retain their original keys.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarMultiSearchInput {
    pub owner: String,
    pub generation: u64,
    pub revision: u64,
    pub key: Option<SidebarMultiSearchKey>,
}

/// One runtime's unchanged single-endpoint publication.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarMultiSearchChild {
    pub endpoint_id: String,
    pub publication: SidebarSearchPublication,
}

/// Snapshot of the fixed endpoint set for the current aggregate query.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SidebarMultiSearchPublication {
    pub input: SidebarMultiSearchInput,
    pub query: String,
    pub children: Vec<SidebarMultiSearchChild>,
}

impl SidebarMultiSearchPublication {
    /// Structural source coherence for consumers. Endpoint authority and retry
    /// admission are still checked by the original live controllers.
    pub fn validate(&self) -> Result<()> {
        require(
            !self.input.owner.trim().is_empty()
                && self.input.revision > 0
                && !self.children.is_empty(),
            "invalid multi-search identity",
        )?;
        match &self.input.key {
            Some(key) => require(
                key.owner == self.input.owner
                    && key.generation > 0
                    && key.generation == self.input.generation
                    && key.query == self.query
                    && !key.query.trim().is_empty(),
                "invalid aggregate search key",
            )?,
            None => require(
                self.query.trim().is_empty(),
                "cleared aggregate retains query",
            )?,
        }
        let mut endpoints = BTreeSet::new();
        for child in &self.children {
            let source = &child.publication;
            require(
                !child.endpoint_id.trim().is_empty()
                    && endpoints.insert(&child.endpoint_id)
                    && source.input.revision > 0
                    && source.query == self.query,
                "invalid multi-search child identity",
            )?;
            if self.input.key.is_some() {
                let key = source
                    .input
                    .key
                    .as_ref()
                    .ok_or_else(|| unsupported("active child has no source key"))?;
                require(
                    !key.owner.trim().is_empty()
                        && key.endpoint_id == child.endpoint_id
                        && key.generation > 0
                        && key.query == self.query
                        && source.input.phase != SidebarSearchPhase::Idle,
                    "invalid multi-search child key",
                )?;
            } else {
                require(
                    source.input.key.is_none()
                        && source.input.phase == SidebarSearchPhase::Idle
                        && source.hits.is_empty()
                        && source.unresolved.is_empty()
                        && source.result.is_none(),
                    "cleared child retains results",
                )?;
            }
            let mut hits = BTreeSet::new();
            for hit in &source.hits {
                require(
                    hit.address.endpoint_id == child.endpoint_id
                        && !hit.address.session_id.is_empty()
                        && hit.address.session_id == hit.hit.session_id
                        && hits.insert(&hit.address),
                    "invalid multi-search child hit",
                )?;
            }
            require(
                source
                    .unresolved
                    .iter()
                    .all(|address| hits.contains(address)),
                "invalid unresolved search identity",
            )?;
        }
        Ok(())
    }
}

/// Retry is fenced by both the aggregate query and the original child attempt.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SidebarMultiSearchIntent {
    Retry {
        aggregate_key: SidebarMultiSearchKey,
        child_key: SidebarSearchKey,
    },
}

struct ChildReader {
    endpoint_id: String,
    reader: SidebarSearchController,
}

struct State {
    publication: Arc<SidebarMultiSearchPublication>,
    generation: u64,
    closed: bool,
}

struct Inner {
    owner: String,
    children: Vec<ChildReader>,
    state: Mutex<State>,
    publications: watch::Sender<Arc<SidebarMultiSearchPublication>>,
}

/// Coordinates a fixed, duplicate-free set of endpoint search readers.
///
/// Endpoint runtimes are consumed at construction so mutable child controllers
/// remain private to the coordinator.
pub struct SidebarMultiSearchController {
    inner: Arc<Inner>,
    observers: Vec<JoinHandle<()>>,
}

impl SidebarMultiSearchController {
    pub fn new(runtimes: Vec<Arc<SessionRuntime>>) -> Result<Self> {
        Self::new_with_messages(runtimes, SidebarSearchMessages::default())
    }

    pub fn new_with_messages(
        runtimes: Vec<Arc<SessionRuntime>>,
        messages: SidebarSearchMessages,
    ) -> Result<Self> {
        require(
            !runtimes.is_empty(),
            "multi-search requires at least one endpoint",
        )?;
        let mut endpoint_ids = BTreeSet::new();
        for runtime in &runtimes {
            let endpoint_id = runtime.inspect(|snapshot| snapshot.endpoint_id.clone());
            require(
                !endpoint_id.trim().is_empty() && endpoint_ids.insert(endpoint_id),
                "multi-search endpoint IDs must be nonempty and unique",
            )?;
        }

        let children = runtimes
            .into_iter()
            .map(|runtime| {
                let endpoint_id = runtime.inspect(|snapshot| snapshot.endpoint_id.clone());
                SidebarSearchController::new_with_messages(runtime, messages.clone()).map(
                    |reader| ChildReader {
                        endpoint_id,
                        reader,
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let owner = uuid::Uuid::new_v4().to_string();
        let initial = Arc::new(SidebarMultiSearchPublication {
            input: SidebarMultiSearchInput {
                owner: owner.clone(),
                generation: 0,
                revision: 1,
                key: None,
            },
            query: String::new(),
            children: children
                .iter()
                .map(|child| SidebarMultiSearchChild {
                    endpoint_id: child.endpoint_id.clone(),
                    publication: child.reader.snapshot().as_ref().clone(),
                })
                .collect(),
        });
        let (publications, _) = watch::channel(initial.clone());
        let inner = Arc::new(Inner {
            owner,
            children,
            state: Mutex::new(State {
                publication: initial,
                generation: 0,
                closed: false,
            }),
            publications,
        });

        let mut observers = Vec::with_capacity(inner.children.len());
        for child in &inner.children {
            let mut changes = child.reader.subscribe();
            let weak = Arc::downgrade(&inner);
            observers.push(tokio::spawn(async move {
                loop {
                    if changes.changed().await.is_err() {
                        break;
                    }
                    let Some(inner) = weak.upgrade() else {
                        break;
                    };
                    // A watch notification is only a wake-up. Always read the
                    // latest child snapshot so queued old-query payloads cannot
                    // bridge an ABA transition or resurrect a retired epoch.
                    Inner::refresh(&inner);
                }
            }));
        }

        Ok(Self { inner, observers })
    }

    pub fn owner(&self) -> &str {
        &self.inner.owner
    }

    pub fn snapshot(&self) -> Arc<SidebarMultiSearchPublication> {
        self.inner.state.lock().unwrap().publication.clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<SidebarMultiSearchPublication>> {
        self.inner.publications.subscribe()
    }

    /// The same query is a no-op. Empty or whitespace input clears all readers.
    pub fn set_query(&self, query: impl Into<String>) -> Result<()> {
        let query = query.into();
        let mut state = self.inner.state.lock().unwrap();
        require(!state.closed, "multi-search source closed")?;
        if state.publication.query == query {
            return Ok(());
        }
        let revision = state
            .publication
            .input
            .revision
            .checked_add(1)
            .ok_or_else(|| unsupported("multi-search publication exhausted"))?;
        let generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| unsupported("multi-search query generation exhausted"))?;

        // Keep observers behind the aggregate lock while child readers retire
        // their prior attempts and begin the new query.
        for child in &self.inner.children {
            child.reader.set_query(query.clone())?;
        }
        state.generation = generation;
        let key = (!query.trim().is_empty()).then(|| SidebarMultiSearchKey {
            owner: self.inner.owner.clone(),
            generation,
            query: query.clone(),
        });
        let children = self
            .inner
            .children
            .iter()
            .map(|child| SidebarMultiSearchChild {
                endpoint_id: child.endpoint_id.clone(),
                publication: child.reader.snapshot().as_ref().clone(),
            })
            .collect();
        Inner::publish(
            &self.inner,
            &mut state,
            SidebarMultiSearchPublication {
                input: SidebarMultiSearchInput {
                    owner: self.inner.owner.clone(),
                    generation,
                    revision,
                    key,
                },
                query,
                children,
            },
        );
        Ok(())
    }

    pub fn retry(&self, intent: &SidebarMultiSearchIntent) -> Result<()> {
        let SidebarMultiSearchIntent::Retry {
            aggregate_key,
            child_key,
        } = intent;
        let mut state = self.inner.state.lock().unwrap();
        require(!state.closed, "multi-search source closed")?;
        require(
            state.publication.input.key.as_ref() == Some(aggregate_key),
            "stale aggregate search retry",
        )?;
        require(
            child_key.query == aggregate_key.query,
            "stale child search retry",
        )?;
        let child = self
            .inner
            .children
            .iter()
            .find(|child| child.endpoint_id == child_key.endpoint_id)
            .ok_or_else(|| tekes_agent_contract::unsupported("unknown search endpoint"))?;
        let publication = child.reader.snapshot();
        require(
            publication.input.key.as_ref() == Some(child_key)
                && matches!(
                    publication.input.phase,
                    SidebarSearchPhase::Failed {
                        retryable: true,
                        ..
                    }
                ),
            "stale or unavailable child search retry",
        )?;
        child.reader.retry(&SidebarSearchIntent::Retry {
            key: child_key.clone(),
        })?;
        Inner::refresh_locked(&self.inner, &mut state);
        Ok(())
    }
}

impl Drop for SidebarMultiSearchController {
    fn drop(&mut self) {
        for observer in self.observers.drain(..) {
            observer.abort();
        }
        let mut state = self.inner.state.lock().unwrap();
        state.closed = true;
        if let (Some(revision), Some(generation)) = (
            state.publication.input.revision.checked_add(1),
            state.generation.checked_add(1),
        ) {
            state.generation = generation;
            for child in &self.inner.children {
                let _ = child.reader.set_query(String::new());
            }
            let publication = SidebarMultiSearchPublication {
                input: SidebarMultiSearchInput {
                    owner: self.inner.owner.clone(),
                    generation,
                    revision,
                    key: None,
                },
                query: String::new(),
                children: self
                    .inner
                    .children
                    .iter()
                    .map(|child| SidebarMultiSearchChild {
                        endpoint_id: child.endpoint_id.clone(),
                        publication: child.reader.snapshot().as_ref().clone(),
                    })
                    .collect(),
            };
            Inner::publish(&self.inner, &mut state, publication);
        }
    }
}

impl Inner {
    fn refresh(this: &Arc<Self>) {
        let mut state = this.state.lock().unwrap();
        Self::refresh_locked(this, &mut state);
    }

    fn refresh_locked(this: &Arc<Self>, state: &mut State) {
        if state.closed {
            return;
        }
        let query = &state.publication.query;
        let mut children = state.publication.children.clone();
        for child in &this.children {
            let latest = child.reader.snapshot();
            // Do not expose an old query as part of the new aggregate, even if
            // its watch event was already queued when the query changed.
            if latest.query == *query
                && let Some(slot) = children
                    .iter_mut()
                    .find(|slot| slot.endpoint_id == child.endpoint_id)
            {
                slot.publication = latest.as_ref().clone();
            }
        }
        if children == state.publication.children {
            return;
        }
        let Some(revision) = state.publication.input.revision.checked_add(1) else {
            return;
        };
        let publication = SidebarMultiSearchPublication {
            input: SidebarMultiSearchInput {
                owner: this.owner.clone(),
                generation: state.publication.input.generation,
                revision,
                key: state.publication.input.key.clone(),
            },
            query: state.publication.query.clone(),
            children,
        };
        Self::publish(this, state, publication);
    }

    fn publish(this: &Arc<Self>, state: &mut State, publication: SidebarMultiSearchPublication) {
        let publication = Arc::new(publication);
        state.publication = publication.clone();
        this.publications.send_replace(publication);
    }
}
