//! Read-only Sidebar observer status and opaque retry provenance.
//!
//! These values describe one coherent reader publication. They do not perform
//! retries or expose the binding owners that authorize them.
use std::{any::Any, collections::BTreeSet, fmt, sync::Arc};

use tekes_agent_contract::{Result, require};

/// Opaque identity for one host or aggregate Sidebar source lifetime.
///
/// Equality is allocation identity. The allocation is intentionally private
/// and is never included in formatting or serialization.
#[derive(Clone)]
pub struct SidebarObserverSourceKey {
    owner: Arc<dyn Any + Send + Sync>,
}

impl SidebarObserverSourceKey {
    /// Capture the identity of an existing source owner.
    pub fn from_owner<T: Any + Send + Sync>(owner: &Arc<T>) -> Self {
        Self {
            owner: owner.clone(),
        }
    }

    /// Check whether this key belongs to the exact same source allocation.
    pub fn matches_owner<T: Any + Send + Sync>(&self, owner: &Arc<T>) -> bool {
        let owner: Arc<dyn Any + Send + Sync> = owner.clone();
        Arc::ptr_eq(&self.owner, &owner)
    }
}

impl PartialEq for SidebarObserverSourceKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
    }
}

impl Eq for SidebarObserverSourceKey {}

impl fmt::Debug for SidebarObserverSourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SidebarObserverSourceKey { .. }")
    }
}

/// Opaque proof tying a displayed observer generation to its original source
/// and endpoint binding. It can be carried by an intent, but cannot authorize a
/// retry without matching the original private owner allocations.
#[derive(Clone)]
pub struct SidebarObserverKey {
    source: SidebarObserverSourceKey,
    child_owner: Arc<dyn Any + Send + Sync>,
    endpoint_id: String,
    generation: u64,
}

impl SidebarObserverKey {
    /// Capture a generation from the original aggregate/source and child
    /// binding owners. A public source key alone cannot mint retry generations.
    pub fn from_owners<S: Any + Send + Sync, C: Any + Send + Sync>(
        source_owner: &Arc<S>,
        child_owner: &Arc<C>,
        endpoint_id: &str,
        generation: u64,
    ) -> Result<Self> {
        require(
            !endpoint_id.trim().is_empty() && generation > 0,
            "invalid sidebar observer retry identity",
        )?;
        Ok(Self {
            source: SidebarObserverSourceKey::from_owner(source_owner),
            child_owner: child_owner.clone(),
            endpoint_id: endpoint_id.to_owned(),
            generation,
        })
    }

    pub fn source(&self) -> &SidebarObserverSourceKey {
        &self.source
    }

    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Check the complete original source, child, endpoint and generation
    /// tuple against the binding owners held by the caller.
    pub fn matches_owners<S: Any + Send + Sync, C: Any + Send + Sync>(
        &self,
        source_owner: &Arc<S>,
        child_owner: &Arc<C>,
        endpoint_id: &str,
        generation: u64,
    ) -> bool {
        let child_owner: Arc<dyn Any + Send + Sync> = child_owner.clone();
        self.source.matches_owner(source_owner)
            && Arc::ptr_eq(&self.child_owner, &child_owner)
            && self.endpoint_id == endpoint_id
            && self.generation == generation
    }
}

impl PartialEq for SidebarObserverKey {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && Arc::ptr_eq(&self.child_owner, &other.child_owner)
            && self.endpoint_id == other.endpoint_id
            && self.generation == other.generation
    }
}

impl Eq for SidebarObserverKey {}

impl fmt::Debug for SidebarObserverKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SidebarObserverKey")
            .field("source", &self.source)
            .field("endpoint_id", &self.endpoint_id)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarObserverPhase {
    Observing,
    Restarting,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SidebarObservation {
    pub generation: u64,
    pub phase: SidebarObserverPhase,
    pub converged: bool,
}

impl Default for SidebarObservation {
    fn default() -> Self {
        Self {
            generation: 1,
            phase: SidebarObserverPhase::Observing,
            converged: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarObserverEndpointInput {
    pub key: SidebarObserverKey,
    pub revision: u64,
    pub observation: SidebarObservation,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarObserverStatusInput {
    pub source: SidebarObserverSourceKey,
    pub revision: u64,
    pub endpoints: Vec<SidebarObserverEndpointInput>,
}

impl SidebarObserverStatusInput {
    /// Validate that all endpoint statuses belong to this source and describe
    /// unique, nonempty endpoints with a coherent nonzero revision/generation.
    pub fn validate(&self) -> Result<()> {
        require(
            self.revision > 0 && !self.endpoints.is_empty(),
            "invalid sidebar observer status identity",
        )?;
        let mut endpoint_ids = BTreeSet::new();
        for endpoint in &self.endpoints {
            require(
                !endpoint.key.endpoint_id.trim().is_empty()
                    && endpoint_ids.insert(endpoint.key.endpoint_id.as_str())
                    && endpoint.revision > 0
                    && endpoint.key.generation == endpoint.observation.generation
                    && endpoint.key.source == self.source,
                "incoherent sidebar observer endpoint status",
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarObserverIntent {
    Retry { key: SidebarObserverKey },
}
