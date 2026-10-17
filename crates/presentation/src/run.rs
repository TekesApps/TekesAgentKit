use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tekes_agent_session::{ConnectionState, MemoryProjectionStore};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionPhase {
    Connecting,
    Connected,
    Reconnecting,
    Degraded,
    Failed,
    Stopped,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPresentation {
    pub phase: ConnectionPhase,
    pub can_submit: bool,
    pub history_is_stale: bool,
    pub message: Option<String>,
}
impl ConnectionPresentation {
    pub fn read(store: &MemoryProjectionStore, session: Option<&str>, was_connected: bool) -> Self {
        let requires_session = session.is_some();
        let session = session.and_then(|id| store.sessions.get(id));
        let session_error = session.and_then(|s| s.journal_error.clone());
        let phase = match &store.connection {
            ConnectionState::Connecting if was_connected => ConnectionPhase::Reconnecting,
            ConnectionState::Connecting => ConnectionPhase::Connecting,
            ConnectionState::Ready if session_error.is_some() => ConnectionPhase::Degraded,
            ConnectionState::Ready => ConnectionPhase::Connected,
            ConnectionState::Failed(_) => ConnectionPhase::Failed,
            ConnectionState::Stopped => ConnectionPhase::Stopped,
        };
        let message = match &store.connection {
            ConnectionState::Failed(error) => Some(error.clone()),
            _ => session_error,
        };
        let fresh = phase == ConnectionPhase::Connected
            && (!requires_session || session.is_some_and(|s| s.journal_ready));
        Self {
            can_submit: fresh,
            history_is_stale: !fresh,
            message,
            phase,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RunPhase {
    Loading,
    Idle,
    Running,
    WaitingForInput,
    Interrupted,
    Archived,
    Unavailable,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunPresentation {
    pub scope: RunScope,
    pub phase: RunPhase,
    pub connection: ConnectionPresentation,
    pub can_submit: bool,
    pub can_cancel: bool,
    pub actionables: Vec<Value>,
    pub queue: Vec<Value>,
    pub jobs: Vec<Value>,
    pub error: Option<String>,
}
impl RunPresentation {
    pub fn read(store: &MemoryProjectionStore, scope: &RunScope, was_connected: bool) -> Self {
        let connection =
            ConnectionPresentation::read(store, Some(&scope.session_id), was_connected);
        let session = store.sessions.get(&scope.session_id);
        let status = session
            .and_then(|s| s.summary.as_ref())
            .and_then(|s| s["status"].as_str());
        let archived =
            store.archived_session_ids.contains(&scope.session_id) || status == Some("archived");
        let actionables: Vec<_> = store
            .actionables
            .values()
            .filter(|v| v["sessionId"] == scope.session_id)
            .cloned()
            .collect();
        let running = session.is_some_and(|s| s.run_open);
        let available = scope.endpoint_id == store.endpoint_id && connection.can_submit;
        let phase = if archived {
            RunPhase::Archived
        } else if !available {
            if session.is_none() {
                RunPhase::Loading
            } else {
                RunPhase::Unavailable
            }
        } else if status == Some("interrupted") {
            RunPhase::Interrupted
        } else if !actionables.is_empty() {
            RunPhase::WaitingForInput
        } else if running {
            RunPhase::Running
        } else {
            RunPhase::Idle
        };
        let can_submit = available && !archived;
        Self {
            scope: scope.clone(),
            phase,
            connection,
            can_submit,
            can_cancel: available && running && !archived && status != Some("interrupted"),
            actionables: if available { actionables } else { Vec::new() },
            queue: session.map(|s| s.queue.clone()).unwrap_or_default(),
            jobs: session.map(|s| s.jobs.clone()).unwrap_or_default(),
            error: session.and_then(|s| s.agent_error.clone()),
        }
    }
}
