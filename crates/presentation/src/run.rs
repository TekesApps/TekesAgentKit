use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tekes_agent_session::{ConnectionState, MemoryProjectionStore};

const HISTORY_ACCESS_KEY: &str = "historyAccess";
const READ_ONLY_HISTORY_MESSAGE: &str = "历史只读：旧会话未保存工具目录，不能继续运行或修改。";

/// Returns true when a session carries an explicit historical-access marker.
/// Any malformed or unknown marker fails closed as read-only. The server still
/// owns authorization; this only keeps shared controls from offering writes.
fn historical_read_only(session: &tekes_agent_session::SessionProjection) -> Option<String> {
    let projection = session.projections.get(HISTORY_ACCESS_KEY)?;
    let value = &projection.value;
    let digest = value.get("sourceDigest").and_then(Value::as_str);
    let digest_valid = digest.is_some_and(|digest| {
        digest.len() == 71
            && digest.starts_with("sha256-")
            && digest[7..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    });
    let expected = value.as_object().is_some_and(|object| {
        object.len() == 3
            && value["mode"] == "read-only"
            && value["reason"] == "legacy-tool-catalog-unavailable"
            && digest_valid
    });
    Some(if expected {
        READ_ONLY_HISTORY_MESSAGE.to_owned()
    } else {
        "历史访问标记无效；为保护会话，已禁用写操作。".to_owned()
    })
}

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
        let read_only_message = session.and_then(historical_read_only);
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
            _ => session_error.or(read_only_message.clone()),
        };
        let fresh = phase == ConnectionPhase::Connected
            && (!requires_session || session.is_some_and(|s| s.journal_ready));
        Self {
            can_submit: fresh && read_only_message.is_none(),
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
    #[serde(default)]
    pub connection_epoch: u64,
    pub phase: RunPhase,
    pub connection: ConnectionPresentation,
    pub can_submit: bool,
    pub can_cancel: bool,
    #[serde(default)]
    pub can_update_queue: bool,
    pub actionables: Vec<Value>,
    pub queue: Vec<Value>,
    pub jobs: Vec<Value>,
    pub error: Option<String>,
}
impl RunPresentation {
    /// Availability is projected from current authority; reply dispatch still
    /// validates the actionable revision against the live runtime.
    pub fn can_respond(&self, id: &str, revision: i64) -> bool {
        self.actionables.iter().any(|actionable| {
            actionable["id"] == id
                && actionable["revision"].as_i64() == Some(revision)
                && actionable["responseAvailable"] == true
        })
    }

    pub fn read(store: &MemoryProjectionStore, scope: &RunScope, was_connected: bool) -> Self {
        let connection =
            ConnectionPresentation::read(store, Some(&scope.session_id), was_connected);
        let session = store.sessions.get(&scope.session_id);
        let status = session
            .and_then(|s| s.summary.as_ref())
            .and_then(|s| s["status"].as_str());
        let archived =
            store.archived_session_ids.contains(&scope.session_id) || status == Some("archived");
        let historical_read_only = session.and_then(historical_read_only);
        let control_available = scope.endpoint_id == store.endpoint_id
            && session.is_some()
            && store.connection == ConnectionState::Ready
            && store.control_ready
            && store.inventory_ready
            && historical_read_only.is_none();
        let actionables_available = control_available && store.actionables_ready;
        let response_available = actionables_available && !archived;
        let actionables: Vec<_> = store
            .actionables
            .values()
            .filter(|v| v["sessionId"] == scope.session_id)
            .map(|source| {
                let mut actionable = source.clone();
                if let Some(object) = actionable.as_object_mut() {
                    // A harness extension cannot grant local reply availability.
                    object.insert("responseAvailable".into(), Value::Bool(response_available));
                    // This is adapter authority, not an extension supplied by a
                    // remote actionable. Unadvertised endpoints offer no cancel.
                    object.remove("cancelOutcome");
                    if !archived
                        && store.endpoint_capabilities.supports_question_cancellation
                        && source["kind"] == "question"
                    {
                        object.insert(
                            "cancelOutcome".into(),
                            serde_json::json!({"sessionId":scope.session_id,"cancelQuestion":true}),
                        );
                    }
                }
                actionable
            })
            .collect();
        let running = session.is_some_and(|session| {
            if session.journal_ready && session.journal_error.is_none() {
                session.run_open
            } else {
                // Inventory remains an independent live authority when history
                // cannot be read; a retained journal flag may already be stale.
                session
                    .summary
                    .as_ref()
                    .and_then(|summary| summary["running"].as_bool())
                    == Some(true)
            }
        });
        let history_available = connection.phase == ConnectionPhase::Connected
            && session.is_some_and(|session| session.journal_ready);
        let available = scope.endpoint_id == store.endpoint_id
            && (connection.can_submit || historical_read_only.is_some() && history_available);
        let phase = if archived {
            RunPhase::Archived
        } else if historical_read_only.is_some() {
            // The immutable history is readable, but stale historical control
            // facts must not advertise a live runnable/answerable turn.
            RunPhase::Unavailable
        } else if !available && !control_available {
            if session.is_none() {
                RunPhase::Loading
            } else {
                RunPhase::Unavailable
            }
        } else if status == Some("interrupted") {
            RunPhase::Interrupted
        } else if actionables_available && !actionables.is_empty() {
            RunPhase::WaitingForInput
        } else if running {
            RunPhase::Running
        } else {
            RunPhase::Idle
        };
        let can_submit = available && connection.can_submit && !archived;
        let can_update_queue = available
            && connection.can_submit
            && store.control_ready
            && store.endpoint_capabilities.supports("session.updateQueue")
            && historical_read_only.is_none()
            && !archived;
        Self {
            scope: scope.clone(),
            connection_epoch: store.connection_epoch,
            phase,
            connection,
            can_submit,
            can_cancel: control_available && running && !archived && status != Some("interrupted"),
            can_update_queue,
            actionables: if actionables_available {
                actionables
            } else {
                Vec::new()
            },
            queue: session
                .filter(|_| historical_read_only.is_none())
                .map(|s| s.queue.clone())
                .unwrap_or_default(),
            jobs: session
                .filter(|_| historical_read_only.is_none())
                .map(|s| s.jobs.clone())
                .unwrap_or_default(),
            error: session
                .and_then(|s| s.agent_error.clone())
                .or(historical_read_only),
        }
    }
}
