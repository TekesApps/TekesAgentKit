//! Read-only, original-session file bytes. Paths, staging and system viewers
//! remain explicit host capabilities. receiptId is never guessed as a read ID.
use crate::RunBinding;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use std::sync::Arc;
use tekes_agent_components_ui::{
    ComposerContextAction, ComposerContextIntent, ComposerContextItem,
};
use tekes_agent_contract::{MAX_UPLOAD_BYTES, Result, require, unsupported};
use tekes_agent_presentation::{ComposerArtifact, ComposerContextBaseline, ComposerDestination};

pub struct FilePreviewRequest {
    binding: Arc<RunBinding>,
    baseline: Arc<ComposerContextBaseline>,
    artifact_id: String,
    attachment_id: String,
    name: String,
    byte_count: u64,
    media_type: Option<String>,
    epoch: u64,
    request_id: String,
}
#[derive(Clone)]
pub struct FilePreview {
    request_id: String,
    name: String,
    media_type: Option<String>,
    bytes: Arc<[u8]>,
}
impl std::fmt::Debug for FilePreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePreview")
            .field("request_id", &self.request_id)
            .field("name", &self.name)
            .field("media_type", &self.media_type)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}
impl FilePreview {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl RunBinding {
    /// Read availability is independent of Send admission, but still belongs to
    /// a present, Ready pane and current public endpoint capabilities.
    pub fn supports_file_preview(&self) -> bool {
        self.access() == crate::PaneAccess::Ready
            && self.current_run().is_some()
            && self.runtime.inspect(|s| {
                s.connection == tekes_agent_session::ConnectionState::Ready
                    && s.endpoint_capabilities
                        .methods
                        .contains("session.fileAttachment")
            })
    }
    pub fn begin_file_preview(
        self: &Arc<Self>,
        intent: &ComposerContextIntent,
    ) -> Result<FilePreviewRequest> {
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        self.require_controls(&intent.baseline.selection.destination)?;
        require(
            self.supports_file_preview(),
            "generic file preview unavailable",
        )?;
        require(
            intent.action == ComposerContextAction::Open,
            "not a file preview action",
        )?;
        let snapshot = self
            .current_run()
            .ok_or_else(|| unsupported("file source retired"))?;
        require(
            intent.baseline.matches_snapshot(&snapshot),
            "file preview context changed",
        )?;
        let ComposerContextItem::Artifact(artifact) = &intent.item else {
            return Err(unsupported("not a file artifact"));
        };
        require(
            intent.baseline.artifacts.contains(artifact)
                && intent
                    .baseline
                    .artifacts
                    .iter()
                    .filter(|a| a.id() == artifact.id())
                    .count()
                    == 1,
            "file artifact changed or ambiguous",
        )?;
        let ComposerArtifact::Attachment {
            id,
            name,
            content,
            facts: Some(facts),
        } = artifact
        else {
            return Err(unsupported("file read identity unknown"));
        };
        require(
            content["type"] == "file",
            "inline image preview requires a separate adapter",
        )?;
        let attachment_id = facts
            .attachment_id
            .as_ref()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| unsupported("file read identity unknown"))?;
        require(valid_name(name), "file preview requires a basename")?;
        require(
            facts.byte_count <= MAX_UPLOAD_BYTES as u64,
            "file preview limit",
        )?;
        require(
            self.runtime.inspect(|s| s.connection_epoch) == epoch,
            "file preview connection changed",
        )?;
        Ok(FilePreviewRequest {
            binding: self.clone(),
            baseline: intent.baseline.clone(),
            artifact_id: id.clone(),
            attachment_id: attachment_id.clone(),
            name: name.clone(),
            byte_count: facts.byte_count,
            media_type: facts.media_type.clone(),
            epoch,
            request_id: uuid::Uuid::new_v4().to_string(),
        })
    }
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
}
impl FilePreviewRequest {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn destination(&self) -> &ComposerDestination {
        &self.baseline.selection.destination
    }
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Host navigation/IME fences are additional: check again immediately before
    /// presenting a viewer. This method never navigates or mutates a draft.
    pub fn is_current(&self) -> bool {
        self.binding.supports_file_preview()
            && self.binding.require_controls(self.destination()).is_ok()
            && self.binding.runtime.inspect(|s| s.connection_epoch) == self.epoch
            && self
                .binding
                .current_run()
                .is_some_and(|s| self.baseline.matches_snapshot(&s))
    }
    /// One public read per explicit call; no automatic retry, file IO or fallback.
    pub async fn read(&self) -> Result<FilePreview> {
        require(self.is_current(), "file preview capture stale")?;
        let params = json!({
            "sessionId": self.binding.scope.session_id,
            "attachmentId": self.attachment_id,
        });
        let value = self
            .binding
            .runtime
            .call(
                "session.fileAttachment",
                params,
                &format!("file-preview:{}", self.request_id),
            )
            .await?;
        require(
            self.is_current(),
            "file preview source changed while reading",
        )?;
        let reference = &value["attachment"];
        require(
            reference["attachmentId"] == self.attachment_id,
            "file preview identity mismatch",
        )?;
        require(
            reference["bytes"].as_u64() == Some(self.byte_count),
            "file preview byte count mismatch",
        )?;
        if !reference["name"].is_null() {
            require(
                reference["name"].as_str() == Some(&self.name),
                "file preview name mismatch",
            )?;
        }
        let encoded = value["data"]
            .as_str()
            .ok_or_else(|| unsupported("file preview data missing"))?;
        require(
            encoded.len() <= MAX_UPLOAD_BYTES.div_ceil(3) * 4,
            "file preview encoded limit",
        )?;
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| unsupported("invalid file preview base64"))?;
        require(
            bytes.len() as u64 == self.byte_count && bytes.len() <= MAX_UPLOAD_BYTES,
            "file preview decoded size mismatch",
        )?;
        let media_type = if reference["mediaType"].is_null() {
            self.media_type.clone()
        } else {
            Some(
                reference["mediaType"]
                    .as_str()
                    .ok_or_else(|| unsupported("invalid file preview media type"))?
                    .to_owned(),
            )
        };
        require(
            media_type.as_ref().is_none_or(|mime| {
                !mime.trim().is_empty() && mime.len() <= 256 && !mime.chars().any(char::is_control)
            }),
            "invalid file preview media type",
        )?;
        Ok(FilePreview {
            request_id: self.request_id.clone(),
            name: self.name.clone(),
            media_type,
            bytes: Arc::from(bytes),
        })
    }
}
