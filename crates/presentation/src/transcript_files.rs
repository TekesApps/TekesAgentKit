//! Validated optional file facts carried by native input receipts.
//!
//! These facts describe attachment display order relative to native images;
//! they do not establish the position of text or other native content blocks.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_FILES: usize = 8;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAdmittedFileInfo {
    pub attachment_id: String,
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAdmittedFile {
    pub content_index: usize,
    pub preceding_image_count: usize,
    pub file: TranscriptAdmittedFileInfo,
}

impl TranscriptAdmittedFile {
    /// Serializable display facts, explicitly separate from native content.
    /// This does not grant attachment read or mutation authority.
    pub fn display_metadata(&self, receipt: &Value) -> Value {
        serde_json::json!({
            "source": "inputReceipt",
            "receipt": receipt,
            "contentIndex": self.content_index,
            "precedingImageCount": self.preceding_image_count,
            "file": self.file,
        })
    }
}

/// An attachment-only display item. Message indices refer to the original
/// `TranscriptItem::attachments` vector and are never rewritten by receipt data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptAttachmentDisplayItem {
    MessageAttachment { attachment_index: usize },
    AdmittedFile { file_index: usize },
}

/// Receipt provenance retained alongside admitted file metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdmittedFileReceipt {
    pub entry_id: String,
    pub custom_type: String,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeInputReceipt {
    pub rpc_id: String,
    pub timestamp: f64,
    pub hash: String,
    pub entry_id: Option<String>,
    pub custom_type: String,
    files: Option<Value>,
}

impl NativeInputReceipt {
    pub(crate) fn from_entry(entry: &Value) -> Option<Self> {
        let custom_type = entry["customType"].as_str()?;
        if !matches!(
            custom_type,
            "agentkit.inputReceipt.v1" | "tekes.inputReceipt.v1"
        ) {
            return None;
        }
        let data = &entry["data"];
        let timestamp = data["timestamp"].as_f64()?;
        let hash = data["hash"].as_str()?;
        let rpc_id = data["rpcId"].as_str()?;
        let entry_id = entry["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        Some(Self {
            rpc_id: rpc_id.to_owned(),
            timestamp,
            hash: hash.to_owned(),
            entry_id,
            custom_type: custom_type.to_owned(),
            files: data.get("files").cloned(),
        })
    }

    pub(crate) fn key(&self) -> String {
        format!("{:x}:{hash}", self.timestamp.to_bits(), hash = self.hash)
    }

    pub(crate) fn admitted_files(
        &self,
        actual_image_count: usize,
    ) -> Option<(Vec<TranscriptAdmittedFile>, AdmittedFileReceipt)> {
        let raw = self.files.as_ref()?;
        let entry_id = self.entry_id.as_ref()?;
        let files = validate_files(raw, actual_image_count)?;
        if files.is_empty() {
            return None;
        }
        Some((
            files,
            AdmittedFileReceipt {
                entry_id: entry_id.clone(),
                custom_type: self.custom_type.clone(),
            },
        ))
    }
}

pub(crate) fn validate_files(
    value: &Value,
    actual_image_count: usize,
) -> Option<Vec<TranscriptAdmittedFile>> {
    let values = value.as_array()?;
    if values.len() > MAX_FILES {
        return None;
    }
    let mut out = Vec::with_capacity(values.len());
    let (mut previous_index, mut previous_images, mut total) = (None, 0usize, 0u64);
    for value in values {
        let content_index = usize::try_from(value["contentIndex"].as_u64()?).ok()?;
        let preceding_image_count = match value.get("precedingImageCount") {
            None => 0,
            Some(value) => usize::try_from(value.as_u64()?).ok()?,
        };
        if content_index as u128 > 9_007_199_254_740_991
            || preceding_image_count > MAX_FILES
            || preceding_image_count > actual_image_count
            || content_index < preceding_image_count
            || previous_index.is_some_and(|previous| content_index <= previous)
            || preceding_image_count < previous_images
        {
            return None;
        }
        let file = &value["file"];
        let attachment_id = file["attachmentId"].as_str()?;
        let name = file["name"].as_str()?;
        let bytes = file["bytes"].as_u64()?;
        if !is_standard_uuid(attachment_id)
            || name.is_empty()
            || matches!(name, "." | "..")
            || name.contains('/')
            || name.contains('\\')
            || name.chars().any(char::is_control)
            || bytes == 0
            || bytes > MAX_FILE_BYTES
        {
            return None;
        }
        total = total.checked_add(bytes)?;
        if total > MAX_TOTAL_BYTES {
            return None;
        }
        out.push(TranscriptAdmittedFile {
            content_index,
            preceding_image_count,
            file: TranscriptAdmittedFileInfo {
                attachment_id: attachment_id.to_owned(),
                name: name.to_owned(),
                bytes,
            },
        });
        previous_index = Some(content_index);
        previous_images = preceding_image_count;
    }
    Some(out)
}

fn is_standard_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

pub(crate) fn image_count(attachments: &[Value]) -> usize {
    attachments
        .iter()
        .filter(|attachment| attachment["type"] == "image")
        .count()
}

pub(crate) fn display_order(
    attachments: &[Value],
    files: &[TranscriptAdmittedFile],
) -> Vec<TranscriptAttachmentDisplayItem> {
    let mut slots = vec![Vec::<usize>::new(); attachments.len() + 1];
    for (file_index, file) in files.iter().enumerate() {
        let target_image_number = file.preceding_image_count + 1;
        let mut seen = 0;
        let next_image = attachments.iter().position(|attachment| {
            if attachment["type"] == "image" {
                seen += 1;
                seen == target_image_number
            } else {
                false
            }
        });
        let slot = next_image.unwrap_or_else(|| {
            attachments
                .iter()
                .rposition(|attachment| attachment["type"] == "image")
                .map_or(attachments.len(), |index| index + 1)
        });
        slots[slot].push(file_index);
    }
    let mut order = Vec::with_capacity(attachments.len() + files.len());
    for (index, slot) in slots.iter().enumerate().take(attachments.len() + 1) {
        order.extend(
            slot.iter()
                .copied()
                .map(|file_index| TranscriptAttachmentDisplayItem::AdmittedFile { file_index }),
        );
        if index < attachments.len() {
            order.push(TranscriptAttachmentDisplayItem::MessageAttachment {
                attachment_index: index,
            });
        }
    }
    order
}
