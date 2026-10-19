//! Consumer-neutral thread menu contract. No renderer owns the action catalog.
use crate::{Result, annotations::binding};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

binding!(ThreadActionDefinition, ACTION_TYPESCRIPT, {
    id: String => "id": "string",
    label: String => "label": "string",
    icon: String => "icon": "string",
    interaction: String => "interaction": "'execute' | 'rename' | 'copy'",
    enabled: bool => "enabled": "boolean",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disabled_reason: Option<String> => "disabledReason?": "string",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    copy_text: Option<String> => "copyText?": "string",
});
binding!(ThreadActionsPresentation, ACTIONS_TYPESCRIPT, {
    endpoint: String => "endpoint": "string",
    session_id: String => "sessionId": "string",
    label: String => "label": "string",
    items: Vec<ThreadActionDefinition> => "items": "ThreadActionDefinition[]",
});
binding!(ThreadPinPresentation, PIN_TYPESCRIPT, {
    pinned: bool => "pinned": "boolean",
});
binding!(ThreadActivity, ACTIVITY_TYPESCRIPT, {
    status: String => "status": "'running' | 'queued' | 'waitingForInput' | 'completed' | 'notStarted' | 'failed' | 'cancelled' | 'idle' | 'unknown'",
    is_stale: bool => "isStale": "boolean",
});
binding!(ThreadActionRequest, ACTION_REQUEST_TYPESCRIPT, {
    session_id: String => "sessionId": "string",
    action_id: String => "actionId": "string",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String> => "title?": "string",
});
binding!(ThreadActionNavigation, NAVIGATION_TYPESCRIPT, {
    endpoint: String => "endpoint": "string",
    session_id: String => "sessionId": "string",
});
binding!(ThreadActionOutcome, OUTCOME_TYPESCRIPT, {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    navigation: Option<ThreadActionNavigation> => "navigation?": "ThreadActionNavigation",
    clear_selection: bool => "clearSelection": "boolean",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    renamed_title: Option<String> => "renamedTitle?": "string",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rename_seq: Option<i64> => "renameSeq?": "number",
    #[serde(default, skip_serializing_if = "Option::is_none")]
    warning: Option<String> => "warning?": "string",
});
impl Default for ThreadActionOutcome {
    fn default() -> Self {
        Self {
            navigation: None,
            clear_selection: false,
            renamed_title: None,
            rename_seq: None,
            warning: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPinState {
    pub pinned: bool,
}

/// Shared local-action authority. AgentKit supplies durable pin storage; hosts may adapt it. A consumer must not create an independent pin database.
#[async_trait]
pub trait ThreadActionHost: Send + Sync {
    fn methods(&self) -> BTreeSet<String>;
    fn pin_state(&self, endpoint: &str, session: &str) -> Option<ThreadPinState>;
    async fn perform(
        &self,
        endpoint: &str,
        request: &ThreadActionRequest,
        rpc_id: &str,
    ) -> Result<ThreadActionOutcome>;
}
pub fn thread_actions_typescript() -> String {
    [
        ACTION_TYPESCRIPT,
        ACTIONS_TYPESCRIPT,
        ACTION_REQUEST_TYPESCRIPT,
        PIN_TYPESCRIPT,
        ACTIVITY_TYPESCRIPT,
        NAVIGATION_TYPESCRIPT,
        OUTCOME_TYPESCRIPT,
    ]
    .concat()
}
