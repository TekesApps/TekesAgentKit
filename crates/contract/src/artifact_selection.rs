//! Exact, host-resolved document artifact selections for annotation provenance.
//!
//! The host supplies all source snapshots; AgentKit performs exact value
//! validation without reading document storage.
use crate::{Result, require};
use serde::{Deserialize, Serialize};

/// Largest integer exactly representable by common JSON/JavaScript consumers.
pub const MAX_EXACT_SELECTION_OFFSET: u64 = 9_007_199_254_740_991;

pub const ARTIFACT_SELECTION_TYPESCRIPT: &str = "export interface Utf16SelectionRange {\n  location: number;\n  length: number;\n}\nexport interface ArtifactSelectionReference {\n  documentId: string;\n  artifactId: string;\n  artifactType: string;\n  fieldPath: string[];\n  revision: string;\n  digest: string;\n  range: Utf16SelectionRange;\n  quote: string;\n  contextBefore: string;\n  contextAfter: string;\n}\n";

/// A UTF-16 range, matching Cocoa and ClientKit selection coordinates.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Utf16SelectionRange {
    #[serde(deserialize_with = "decode_offset")]
    pub location: u64,
    #[serde(deserialize_with = "decode_offset")]
    pub length: u64,
}

// Match the existing ClientKit/AgentKit integer convention: 1, 1.0 and 1e0
// identify the same offset. Fractional, negative and boolean values do not.
fn decode_offset<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<u64, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    crate::value_integer(&value)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| serde::de::Error::custom("expected nonnegative exact UTF-16 integer"))
}

impl Utf16SelectionRange {
    pub fn validate(&self) -> Result<()> {
        require(
            self.location <= MAX_EXACT_SELECTION_OFFSET
                && self.length <= MAX_EXACT_SELECTION_OFFSET
                && self
                    .location
                    .checked_add(self.length)
                    .is_some_and(|end| end <= MAX_EXACT_SELECTION_OFFSET),
            "invalid UTF-16 selection range",
        )
    }
}

/// Immutable address and quote captured from one public document field/block.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSelectionReference {
    pub document_id: String,
    pub artifact_id: String,
    pub artifact_type: String,
    /// Stable host-defined path components; never display indexes.
    pub field_path: Vec<String>,
    pub revision: String,
    /// Opaque host-defined digest; capture and resolution use the same convention.
    pub digest: String,
    pub range: Utf16SelectionRange,
    pub quote: String,
    pub context_before: String,
    pub context_after: String,
}

/// Exact identity and optional immutable public snapshot content supplied by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtifactSelectionTarget<'a> {
    pub document_id: Option<&'a str>,
    pub artifact_id: Option<&'a str>,
    pub artifact_type: Option<&'a str>,
    pub field_path: Option<&'a [String]>,
    pub revision: Option<&'a str>,
    pub digest: Option<&'a str>,
    pub field_text: Option<&'a str>,
}

/// Resolution result; context is never used to rebind a stale range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactSelectionResolution {
    Exact,
    Stale,
    Unavailable,
}

impl ArtifactSelectionReference {
    /// Capture quote and surrounding context from an exact UTF-16 selection.
    /// Context limits are UTF-16 units and clamp to Unicode scalar boundaries.
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        document_id: String,
        artifact_id: String,
        artifact_type: String,
        field_path: Vec<String>,
        revision: String,
        digest: String,
        field_text: &str,
        range: Utf16SelectionRange,
        context_before_units: u64,
        context_after_units: u64,
    ) -> Result<Self> {
        validate_identity(
            &document_id,
            &artifact_id,
            &artifact_type,
            &field_path,
            &revision,
            &digest,
        )?;
        range.validate()?;
        let end = range.location + range.length;
        let start_byte = byte_offset_at_utf16(field_text, range.location)
            .ok_or_else(|| crate::Error::Protocol("selection splits UTF-16 scalar".into()))?;
        let end_byte = byte_offset_at_utf16(field_text, end)
            .ok_or_else(|| crate::Error::Protocol("selection splits UTF-16 scalar".into()))?;
        let before_floor = range.location.saturating_sub(context_before_units);
        let before_byte = byte_offset_ceil_utf16(field_text, before_floor)
            .ok_or_else(|| crate::Error::Protocol("invalid preceding context boundary".into()))?;
        let after_limit = end
            .saturating_add(context_after_units)
            .min(utf16_len(field_text));
        let after_byte = byte_offset_floor_utf16(field_text, after_limit)
            .ok_or_else(|| crate::Error::Protocol("invalid following context boundary".into()))?;
        let reference = Self {
            document_id,
            artifact_id,
            artifact_type,
            field_path,
            revision,
            digest,
            range,
            quote: field_text[start_byte..end_byte].into(),
            context_before: field_text[before_byte..start_byte].into(),
            context_after: field_text[end_byte..after_byte].into(),
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn validate(&self) -> Result<()> {
        validate_identity(
            &self.document_id,
            &self.artifact_id,
            &self.artifact_type,
            &self.field_path,
            &self.revision,
            &self.digest,
        )?;
        self.range.validate()?;
        require(
            self.range.length == utf16_len(&self.quote),
            "selection quote length does not match UTF-16 range",
        )
    }

    /// Resolve only against the supplied exact identity and snapshot.
    pub fn resolve(&self, target: &ArtifactSelectionTarget<'_>) -> ArtifactSelectionResolution {
        if self.validate().is_err() {
            return ArtifactSelectionResolution::Stale;
        }
        let (Some(document_id), Some(artifact_id), Some(artifact_type), Some(field_path)) = (
            target.document_id,
            target.artifact_id,
            target.artifact_type,
            target.field_path,
        ) else {
            return ArtifactSelectionResolution::Unavailable;
        };
        if self.document_id != document_id
            || self.artifact_id != artifact_id
            || self.artifact_type != artifact_type
            || self.field_path != field_path
        {
            return ArtifactSelectionResolution::Unavailable;
        }
        let (Some(revision), Some(digest), Some(field_text)) =
            (target.revision, target.digest, target.field_text)
        else {
            return ArtifactSelectionResolution::Unavailable;
        };
        if self.revision != revision || self.digest != digest {
            return ArtifactSelectionResolution::Stale;
        }
        if self.range.validate().is_err() {
            return ArtifactSelectionResolution::Stale;
        }
        let Some(end) = self.range.location.checked_add(self.range.length) else {
            return ArtifactSelectionResolution::Stale;
        };
        let (Some(start_byte), Some(end_byte)) = (
            byte_offset_at_utf16(field_text, self.range.location),
            byte_offset_at_utf16(field_text, end),
        ) else {
            return ArtifactSelectionResolution::Stale;
        };
        if &field_text[start_byte..end_byte] == self.quote.as_str() {
            ArtifactSelectionResolution::Exact
        } else {
            ArtifactSelectionResolution::Stale
        }
    }
}

fn validate_identity(
    document_id: &str,
    artifact_id: &str,
    artifact_type: &str,
    field_path: &[String],
    revision: &str,
    digest: &str,
) -> Result<()> {
    require(
        [document_id, artifact_id, artifact_type, revision, digest]
            .iter()
            .all(|part| !part.is_empty()),
        "artifact selection identity is empty",
    )?;
    require(
        !field_path.is_empty() && field_path.iter().all(|part| !part.is_empty()),
        "artifact selection field path is empty",
    )
}

fn utf16_len(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

/// Return the byte boundary for an exact UTF-16 offset; reject half-surrogates.
fn byte_offset_at_utf16(text: &str, offset: u64) -> Option<usize> {
    if offset == 0 {
        return Some(0);
    }
    let mut units = 0_u64;
    for (byte, scalar) in text.char_indices() {
        if units == offset {
            return Some(byte);
        }
        units += scalar.len_utf16() as u64;
        if units == offset {
            return Some(byte + scalar.len_utf8());
        }
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(text.len())
}

/// Clamp a desired offset toward the end, so preceding context never exceeds its limit.
fn byte_offset_ceil_utf16(text: &str, offset: u64) -> Option<usize> {
    if let Some(byte) = byte_offset_at_utf16(text, offset) {
        return Some(byte);
    }
    let mut units = 0_u64;
    for (byte, scalar) in text.char_indices() {
        units += scalar.len_utf16() as u64;
        if units > offset {
            return Some(byte + scalar.len_utf8());
        }
    }
    (units == offset).then_some(text.len())
}

/// Clamp a desired offset toward the beginning, so following context stays in bounds.
fn byte_offset_floor_utf16(text: &str, offset: u64) -> Option<usize> {
    if let Some(byte) = byte_offset_at_utf16(text, offset) {
        return Some(byte);
    }
    let mut units = 0_u64;
    for (byte, scalar) in text.char_indices() {
        let next = units + scalar.len_utf16() as u64;
        if next > offset {
            return Some(byte);
        }
        units = next;
    }
    (units == offset).then_some(text.len())
}
