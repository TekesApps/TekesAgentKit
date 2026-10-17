use crate::RunScope;
use serde::{Deserialize, Serialize};
use tekes_agent_contract::SubagentCatalog;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChildComposerAvailability {
    Loading,
    Available,
    OneShot,
    ParentUnavailable,
    Unavailable,
}
impl ChildComposerAvailability {
    pub fn resolve(catalog: Option<&SubagentCatalog>, child: &str) -> Self {
        let Some(catalog) = catalog else {
            return Self::Loading;
        };
        match catalog.child(child).and_then(|e| e.mode.as_deref()) {
            Some("one-shot") => Self::OneShot,
            Some("continuable") if catalog.parent_available => Self::Available,
            Some("continuable") => Self::ParentUnavailable,
            _ => Self::Unavailable,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildCatalogToken {
    parent: RunScope,
    generation: u64,
}
/// Replace authority on every connection/selection change. Failed reads remove control authority.
#[derive(Clone, Debug, Default)]
pub struct ChildCatalogState {
    current: Option<ChildCatalogToken>,
    generation: u64,
    pub catalog: Option<SubagentCatalog>,
    pub error: Option<String>,
    pub refreshing: bool,
}
impl ChildCatalogState {
    pub fn begin(&mut self, parent: RunScope, connected: bool) -> Option<ChildCatalogToken> {
        self.generation += 1;
        self.catalog = None;
        self.error = None;
        self.refreshing = connected;
        self.current = connected.then_some(ChildCatalogToken {
            parent,
            generation: self.generation,
        });
        self.current.clone()
    }
    pub fn commit(
        &mut self,
        token: &ChildCatalogToken,
        result: std::result::Result<SubagentCatalog, String>,
    ) -> bool {
        if self.current.as_ref() != Some(token) {
            return false;
        }
        self.refreshing = false;
        match result {
            Ok(value) => {
                self.catalog = Some(value);
                self.error = None;
            }
            Err(error) => {
                self.catalog = None;
                self.error = Some(error);
            }
        }
        true
    }
    pub fn availability(&self, child: &str) -> ChildComposerAvailability {
        if self.current.is_none() || self.error.is_some() {
            return ChildComposerAvailability::Unavailable;
        }
        ChildComposerAvailability::resolve(self.catalog.as_ref(), child)
    }
}
/// Explicit adapter input, independent of control capability. Bounded history cannot be opened
/// as an ordinary session: a host must supply a child-line projection through ChildHistorySource.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChildHistoryLayout {
    SameSession,
    IndependentSessions,
}
#[async_trait::async_trait]
pub trait ChildHistorySource: Send + Sync {
    async fn read(
        &self,
        parent: &RunScope,
        child: &str,
    ) -> tekes_agent_contract::Result<crate::TranscriptSnapshot>;
}

/// One mounted child run owns a history lease. Same-session lines require an explicit projection
/// source; opening the parent session as if it were the child is deliberately impossible.
pub struct ChildRun {
    parent: RunScope,
    child: String,
    runtime: std::sync::Arc<tekes_agent_session::SessionRuntime>,
    source: Option<std::sync::Arc<dyn ChildHistorySource>>,
    _lease: tekes_agent_session::SessionHistoryLease,
}
impl ChildRun {
    pub async fn open(
        runtime: std::sync::Arc<tekes_agent_session::SessionRuntime>,
        parent: RunScope,
        child: impl Into<String>,
        layout: ChildHistoryLayout,
        source: Option<std::sync::Arc<dyn ChildHistorySource>>,
    ) -> tekes_agent_contract::Result<Self> {
        parent.validate()?;
        let child = child.into();
        tekes_agent_contract::require(
            !child.is_empty() && child != parent.session_id,
            "invalid child history identity",
        )?;
        tekes_agent_contract::require(
            runtime.inspect(|s| s.endpoint_id == parent.endpoint_id),
            "child endpoint mismatch",
        )?;
        if layout == ChildHistoryLayout::SameSession && source.is_none() {
            return Err(tekes_agent_contract::unsupported(
                "bounded child history needs a child-line projection source",
            ));
        }
        let session = if layout == ChildHistoryLayout::SameSession {
            &parent.session_id
        } else {
            &child
        };
        let lease = runtime.acquire_history(session).await?;
        Ok(Self {
            parent,
            child,
            runtime,
            source,
            _lease: lease,
        })
    }
    pub async fn transcript(&self) -> tekes_agent_contract::Result<crate::TranscriptSnapshot> {
        if let Some(source) = &self.source {
            return source.read(&self.parent, &self.child).await;
        }
        let scope = RunScope {
            endpoint_id: self.parent.endpoint_id.clone(),
            thread_id: self.parent.thread_id.clone(),
            session_id: self.child.clone(),
        };
        self.runtime.inspect(|s| {
            let session = s
                .sessions
                .get(&self.child)
                .ok_or(tekes_agent_contract::Error::Disconnected)?;
            let mut snapshot = crate::TranscriptSnapshot::read(&scope, session)?;
            snapshot.is_stale |=
                crate::ConnectionPresentation::read(s, Some(&self.child), false).history_is_stale;
            Ok(snapshot)
        })
    }
    pub async fn catalog(&self) -> tekes_agent_contract::Result<SubagentCatalog> {
        self.runtime.subagent_catalog(&self.parent.session_id).await
    }
    pub async fn prompt(
        &self,
        mode: tekes_agent_contract::PromptMode,
        content: Vec<serde_json::Value>,
        rpc_id: &str,
    ) -> tekes_agent_contract::Result<serde_json::Value> {
        self.runtime
            .prompt_child(&self.parent.session_id, &self.child, mode, content, rpc_id)
            .await
    }
    pub async fn cancel(&self, rpc_id: &str) -> tekes_agent_contract::Result<()> {
        self.runtime
            .cancel_child(&self.parent.session_id, &self.child, rpc_id)
            .await
    }
}
