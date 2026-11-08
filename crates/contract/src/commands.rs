//! Public non-UI requests. Harness-specific extension values remain lossless JSON.
use crate::*;
/// Public transport ceiling, independent of a harness's stricter attachment policy.
pub const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_preset: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreated {
    pub session_id: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptMode {
    Queue,
    Steer,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub session_id: String,
    pub mode: PromptMode,
    pub content: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_time_zone: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum QueueAction {
    Remove,
    Edit { content: Vec<Value> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    pub ephemeral: bool,
}
impl ForkRequest {
    pub fn validate(&self) -> Result<()> {
        require(
            !(self.session_id.is_empty() || self.at_seq.is_some() && self.entry_id.is_some()),
            "fork must select at most one boundary",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalReference {
    pub id: String,
    pub revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalView {
    pub id: String,
    pub revision: i64,
    pub objective: String,
    pub phase: String,
    pub max_goal_rounds: u64,
    pub rounds_started: u64,
    pub created_at: f64,
    pub updated_at: f64,
    pub activation: String,
    pub blocked_reason: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPolicy {
    pub inline_media_types: Vec<String>,
    pub max_attachment_bytes: Option<u64>,
    pub max_attachments_per_message: Option<u64>,
    pub max_message_attachment_bytes: Option<u64>,
    pub max_image_pixels: Option<u64>,
    pub max_image_dimension: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    pub attachment_id: String,
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadReceipt {
    pub receipt_id: String,
    pub file: UploadedFile,
}
