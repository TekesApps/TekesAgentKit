//! Harness-owned approval modes. Values are opaque; clients must use the advertised catalog.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub value: String,
    pub title: Option<String>,
    pub description: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionState {
    pub options: Vec<PermissionOption>,
    pub current_value: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPolicy {
    pub thread_level: Vec<PermissionOption>,
    pub server_level: PermissionState,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionModeState {
    pub mode: Option<String>,
}

pub fn permissions_typescript() -> &'static str {
    "export interface PermissionOption {\n  value: string;\n  title?: string | null;\n  description?: string | null;\n}\nexport interface PermissionState {\n  options: PermissionOption[];\n  currentValue: string | null;\n}\nexport interface PermissionPolicy {\n  threadLevel: PermissionOption[];\n  serverLevel: PermissionState;\n}\nexport interface PermissionModeState {\n  mode: string | null;\n}\n/** Optional shared question cancellation; submit verbatim through respond. */\nexport interface QuestionCancellationOutcome {\n  sessionId: string;\n  cancelQuestion: true;\n}\n"
}
