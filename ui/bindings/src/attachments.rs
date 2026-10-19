//! File bytes and system selection belong to the host. This adapter captures the
//! original run and performs one explicit, non-retryable upload operation.
use crate::RunBinding;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_contract::{AttachmentPolicy, MAX_UPLOAD_BYTES, Result, UploadReceipt, require};
use tekes_agent_presentation::{
    AttachmentFacts, AttachmentTransferPhase, AttachmentTransferSnapshot, ComposerArtifact,
    ComposerContextBaseline, ComposerDestination,
};

/// Immutable host-selected generic file. Images require a separate inline-image
/// policy/decoder path; a file with an image MIME type is still a generic file.
#[derive(Clone)]
pub struct AttachmentFile {
    name: String,
    media_type: String,
    bytes: Arc<[u8]>,
}
impl std::fmt::Debug for AttachmentFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachmentFile")
            .field("name", &self.name)
            .field("media_type", &self.media_type)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}
impl AttachmentFile {
    pub fn new(name: String, media_type: String, bytes: Arc<[u8]>) -> Result<Self> {
        require(
            !name.trim().is_empty()
                && name != "."
                && name != ".."
                && !name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\'),
            "attachment requires a basename",
        )?;
        require(
            !media_type.trim().is_empty() && !media_type.chars().any(char::is_control),
            "invalid attachment media type",
        )?;
        require(
            !bytes.is_empty() && bytes.len() <= MAX_UPLOAD_BYTES,
            "upload limit",
        )?;
        Ok(Self {
            name,
            media_type,
            bytes,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    pub fn byte_count(&self) -> u64 {
        self.bytes.len() as u64
    }
}

/// Retain this value on failure for explicit host reconciliation. RPC identities
/// are correlation only: Pi and DSH do not promise upload idempotency.
pub struct AttachmentSelection {
    binding: Arc<RunBinding>,
    baseline: ComposerContextBaseline,
    epoch: u64,
    request_id: String,
    claimed: AtomicBool,
    updates: tokio::sync::watch::Sender<Arc<AttachmentTransferSnapshot>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentOutcome {
    Attached,
    /// Known receipts are retained but were not written to a changed draft.
    Detached,
    /// A call may have reached the endpoint. Never automatically re-upload.
    Failed {
        uncertain: bool,
        message: String,
    },
}
#[derive(Clone, Debug)]
pub struct AttachmentReport {
    pub request_id: String,
    pub destination: ComposerDestination,
    pub outcome: AttachmentOutcome,
    pub receipts: Vec<UploadReceipt>,
}
impl RunBinding {
    pub fn supports_attachments(&self) -> bool {
        self.access() == crate::PaneAccess::Ready
            && self.current_run().is_some()
            && self.runtime.inspect(|s| {
                s.connection == tekes_agent_session::ConnectionState::Ready
                    && s.endpoint_capabilities
                        .methods
                        .contains("session.uploadFile")
                    && s.endpoint_capabilities
                        .methods
                        .contains("attachments.policy")
            })
    }
    /// Capture before opening the system picker, never after its async result.
    pub fn begin_attachments(
        self: &Arc<Self>,
        destination: &ComposerDestination,
    ) -> Result<AttachmentSelection> {
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        self.require_controls(destination)?;
        require(self.supports_attachments(), "file attachments unavailable")?;
        let snapshot = self.current_run().ok_or_else(|| {
            tekes_agent_contract::unsupported("pane retired while selecting files")
        })?;
        let baseline = ComposerContextBaseline::read(&snapshot)
            .ok_or_else(|| tekes_agent_contract::unsupported("composer selection missing"))?;
        require(
            &baseline.selection.destination == destination,
            "attachment destination mismatch",
        )?;
        require(
            self.runtime.inspect(|s| s.connection_epoch) == epoch,
            "attachment connection changed",
        )?;
        let request_id = uuid::Uuid::new_v4().to_string();
        let updates = tokio::sync::watch::channel(Arc::new(AttachmentTransferSnapshot {
            revision: 1,
            request_id: request_id.clone(),
            destination: destination.clone(),
            phase: AttachmentTransferPhase::Selecting,
        }))
        .0;
        Ok(AttachmentSelection {
            binding: self.clone(),
            baseline,
            epoch,
            request_id,
            claimed: AtomicBool::new(false),
            updates,
        })
    }
}
impl AttachmentSelection {
    pub fn snapshot(&self) -> Arc<AttachmentTransferSnapshot> {
        self.updates.borrow().clone()
    }
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<Arc<AttachmentTransferSnapshot>> {
        self.updates.subscribe()
    }
    fn publish(&self, phase: AttachmentTransferPhase) {
        self.updates.send_modify(|snapshot| {
            let mut next = (**snapshot).clone();
            next.revision = next
                .revision
                .checked_add(1)
                .expect("attachment revision exhausted");
            next.phase = phase;
            *snapshot = Arc::new(next);
        });
    }
    /// System picker cancellation, before any upload attempt. This does not
    /// cancel network IO or discard remote receipts.
    pub fn cancel_selection(&self) -> bool {
        if self.claimed.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.publish(AttachmentTransferPhase::Cancelled);
        true
    }
    pub fn fail_selection(&self, message: String) -> bool {
        if self.claimed.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.publish(AttachmentTransferPhase::Failed {
            completed: 0,
            uncertain: false,
            message,
        });
        true
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn destination(&self) -> &ComposerDestination {
        &self.baseline.selection.destination
    }
    fn current(&self) -> bool {
        if self.binding.require_controls(self.destination()).is_err()
            || !self.binding.supports_attachments()
            || self.binding.runtime.inspect(|s| s.connection_epoch) != self.epoch
        {
            return false;
        }
        let Some(snapshot) = self.binding.current_run() else {
            return false;
        };
        let Some(now) = ComposerContextBaseline::read(&snapshot) else {
            return false;
        };
        now.selection.destination == self.baseline.selection.destination
            && now.context_revision == self.baseline.context_revision
            && now.artifacts == self.baseline.artifacts
            && now.annotations == self.baseline.annotations
    }
    /// One attempt per capture, including preflight failure. The caller retains
    /// its immutable files and this capture; this API never silently retries.
    pub async fn upload(&self, files: &[AttachmentFile]) -> Result<AttachmentReport> {
        require(
            !self.claimed.swap(true, Ordering::AcqRel),
            "attachment operation already attempted",
        )?;
        let result = self.upload_once(files).await;
        let phase = match &result {
            Ok(report) => match &report.outcome {
                AttachmentOutcome::Attached => AttachmentTransferPhase::Attached {
                    count: report.receipts.len(),
                },
                AttachmentOutcome::Detached => AttachmentTransferPhase::Detached {
                    completed: report.receipts.len(),
                },
                AttachmentOutcome::Failed { uncertain, message } => {
                    AttachmentTransferPhase::Failed {
                        completed: report.receipts.len(),
                        uncertain: *uncertain,
                        message: message.clone(),
                    }
                }
            },
            Err(error) => AttachmentTransferPhase::Failed {
                completed: 0,
                uncertain: true,
                message: error.to_string(),
            },
        };
        self.publish(phase);
        result
    }
    async fn upload_once(&self, files: &[AttachmentFile]) -> Result<AttachmentReport> {
        let mut report = AttachmentReport {
            request_id: self.request_id.clone(),
            destination: self.destination().clone(),
            outcome: AttachmentOutcome::Detached,
            receipts: vec![],
        };
        if !self.current() {
            return Ok(report);
        }
        let policy = match self
            .binding
            .runtime
            .attachment_policy(Some(&self.binding.scope.session_id))
            .await
        {
            Ok(policy) => policy,
            Err(error) => {
                report.outcome = AttachmentOutcome::Failed {
                    uncertain: false,
                    message: error.to_string(),
                };
                return Ok(report);
            }
        };
        if !self.current() {
            return Ok(report);
        }
        if let Err(error) = validate_policy(&policy, &self.baseline.artifacts, files) {
            report.outcome = AttachmentOutcome::Failed {
                uncertain: false,
                message: error.to_string(),
            };
            return Ok(report);
        }
        let mut artifacts = self.baseline.artifacts.clone();
        for (index, file) in files.iter().enumerate() {
            if !self.current() {
                return Ok(report);
            }
            self.publish(AttachmentTransferPhase::Uploading {
                index: index + 1,
                total: files.len(),
            });
            let receipt = match self
                .binding
                .runtime
                .upload_bytes(
                    &self.binding.scope.session_id,
                    &file.name,
                    &file.media_type,
                    &file.bytes,
                    &format!("attachment:{}:{index}", self.request_id),
                )
                .await
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    report.outcome = AttachmentOutcome::Failed {
                        uncertain: true,
                        message: error.to_string(),
                    };
                    return Ok(report);
                }
            };
            // Preserve even a malformed receipt as reconciliation evidence.
            report.receipts.push(receipt.clone());
            if receipt.file.attachment_id.trim().is_empty() || receipt.file.name != file.name {
                report.outcome = AttachmentOutcome::Failed {
                    uncertain: true,
                    message: "upload receipt identity mismatch".into(),
                };
                return Ok(report);
            }
            artifacts.push(ComposerArtifact::Attachment {
                id: format!("attachment:{}:{index}", self.request_id),
                name: receipt.file.name,
                content: json!({"type":"file","receiptId":receipt.receipt_id}),
                facts: Some(AttachmentFacts {
                    byte_count: file.byte_count(),
                    media_type: Some(file.media_type.clone()),
                }),
            });
            if !self.current() {
                return Ok(report);
            }
        }
        // patch_context atomically fences Send/context ABA and preserves newer
        // text/settings; no replacement of an entire stale draft is allowed.
        if self.binding.require_controls(self.destination()).is_err() {
            return Ok(report);
        }
        match self
            .binding
            .controller
            .as_ref()
            .expect("captured writable controller")
            .patch_context_at_epoch(
                &self.baseline,
                artifacts,
                self.baseline.annotations.clone(),
                self.epoch,
            ) {
            Ok(true) => report.outcome = AttachmentOutcome::Attached,
            Ok(false) => {}
            Err(error) => {
                report.outcome = AttachmentOutcome::Failed {
                    uncertain: false,
                    message: error.to_string(),
                }
            }
        }
        Ok(report)
    }
}
fn validate_policy(
    policy: &AttachmentPolicy,
    existing: &[ComposerArtifact],
    files: &[AttachmentFile],
) -> Result<()> {
    require(!files.is_empty(), "no selected files")?;
    let existing: Vec<_> = existing
        .iter()
        .filter_map(|a| match a {
            ComposerArtifact::Attachment { facts, .. } => Some(facts),
            _ => None,
        })
        .collect();
    require(
        policy.max_attachments_per_message.is_none_or(|limit| {
            (existing.len() as u64).saturating_add(files.len() as u64) <= limit
        }),
        "attachment count limit",
    )?;
    for file in files {
        require(
            policy
                .max_attachment_bytes
                .is_none_or(|limit| file.byte_count() <= limit),
            "attachment size limit",
        )?;
    }
    if let Some(limit) = policy.max_message_attachment_bytes {
        let mut total = 0u64;
        for facts in existing {
            let facts = facts.as_ref().ok_or_else(|| {
                tekes_agent_contract::unsupported("existing attachment byte count unknown")
            })?;
            total = total.checked_add(facts.byte_count).ok_or_else(|| {
                tekes_agent_contract::unsupported("attachment byte count overflow")
            })?;
        }
        for file in files {
            total = total.checked_add(file.byte_count()).ok_or_else(|| {
                tekes_agent_contract::unsupported("attachment byte count overflow")
            })?;
        }
        require(total <= limit, "message attachment size limit")?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn file() -> AttachmentFile {
        AttachmentFile::new(
            "中文.txt".into(),
            "text/plain".into(),
            Arc::from(&b"abc"[..]),
        )
        .unwrap()
    }
    fn policy() -> AttachmentPolicy {
        AttachmentPolicy {
            inline_media_types: vec![],
            max_attachment_bytes: Some(3),
            max_attachments_per_message: Some(2),
            max_message_attachment_bytes: Some(6),
            max_image_pixels: None,
            max_image_dimension: None,
        }
    }
    #[test]
    fn generic_files_do_not_require_inline_media_support() {
        assert!(validate_policy(&policy(), &[], &[file()]).is_ok());
    }
    #[test]
    fn unknown_legacy_sizes_are_not_zero_and_limits_are_preflighted() {
        let mut artifact = ComposerArtifact::Attachment {
            id: "a".into(),
            name: "a".into(),
            content: json!({"type":"file","receiptId":"r"}),
            facts: None,
        };
        assert!(validate_policy(&policy(), &[artifact.clone()], &[file()]).is_err());
        if let ComposerArtifact::Attachment { facts, .. } = &mut artifact {
            *facts = Some(AttachmentFacts {
                byte_count: 3,
                media_type: None,
            });
        }
        assert!(validate_policy(&policy(), &[artifact.clone()], &[file()]).is_ok());
        assert!(validate_policy(&policy(), &[artifact], &[file(), file()]).is_err());
        let mut p = policy();
        p.max_attachment_bytes = Some(2);
        assert!(validate_policy(&p, &[], &[file()]).is_err());
        p.max_attachment_bytes = None;
        p.max_message_attachment_bytes = Some(2);
        assert!(validate_policy(&p, &[], &[file()]).is_err());
    }
    #[test]
    fn paths_empty_files_and_control_metadata_are_rejected() {
        for name in ["../a", "a/b", "a\\b", ".", "..", "\n"] {
            assert!(
                AttachmentFile::new(name.into(), "text/plain".into(), Arc::from(&b"x"[..]))
                    .is_err()
            );
        }
        assert!(AttachmentFile::new("a".into(), "text/plain".into(), Arc::from(&b""[..])).is_err());
        assert!(!format!("{:?}", file()).contains("abc"));
    }
}
