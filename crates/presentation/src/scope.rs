use serde::{Deserialize, Serialize};
use tekes_agent_contract::{Result, SessionAddress, require};
/// Endpoint, thread and receiving session are independent identities. Never split encoded UI IDs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct RunScope {
    pub endpoint_id: String,
    pub thread_id: String,
    pub session_id: String,
}
impl RunScope {
    pub fn main(endpoint: impl Into<String>, session: impl Into<String>) -> Self {
        let session = session.into();
        Self {
            endpoint_id: endpoint.into(),
            thread_id: session.clone(),
            session_id: session,
        }
    }
    pub fn address(&self) -> SessionAddress {
        SessionAddress {
            endpoint_id: self.endpoint_id.clone(),
            session_id: self.session_id.clone(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        require(
            !self.endpoint_id.is_empty()
                && !self.thread_id.is_empty()
                && !self.session_id.is_empty(),
            "empty Run scope",
        )
    }
}
/// Draft identity excludes pane, file/diff target and visual presentation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ComposerDestination {
    Run {
        scope: RunScope,
    },
    NewThread {
        endpoint_id: String,
        workspace_id: String,
    },
}
impl ComposerDestination {
    pub fn run(scope: RunScope) -> Self {
        Self::Run { scope }
    }
    pub fn endpoint_id(&self) -> &str {
        match self {
            Self::Run { scope } => &scope.endpoint_id,
            Self::NewThread { endpoint_id, .. } => endpoint_id,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Run { scope } => scope.validate(),
            Self::NewThread {
                endpoint_id,
                workspace_id,
            } => require(
                !endpoint_id.is_empty() && !workspace_id.is_empty(),
                "empty draft scope",
            ),
        }
    }
    /// JSON framing prevents collisions when endpoint/native IDs contain colons or separators.
    pub fn storage_key(&self) -> String {
        serde_json::to_string(self).expect("string-only destination")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionToken {
    pub destination: ComposerDestination,
    pub generation: u64,
}
