//! Harness-neutral session location, native conversation and read-only file contracts.
use crate::{Error, Result, SessionAddress, require};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionContentKind {
    #[default]
    Journal,
    NativeConversation,
}
/// A native file path is diagnostic evidence supplied by the harness/host, never a remote local-read fallback.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionLocator {
    pub address: SessionAddress,
    pub harness: String,
    /// Original project / session workspace root.
    pub cwd: Option<String>,
    /// Current execution directory when the harness publishes an authoritative projection.
    pub working_directory: Option<String>,
    pub native_session_file: Option<String>,
    pub native_session_id: Option<String>,
    pub history_source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeConversationSnapshot {
    pub session_id: String,
    pub generation: String,
    pub leaf_id: Option<String>,
    pub entries: Vec<Value>,
    pub has_more_before: bool,
    pub is_streaming: bool,
    pub state: Value,
}
impl NativeConversationSnapshot {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.session_id.is_empty() && !self.generation.is_empty(),
            "invalid conversation identity",
        )?;
        let mut ids = std::collections::BTreeSet::new();
        for entry in &self.entries {
            let id = crate::string(entry, "id")?;
            require(
                !id.is_empty() && ids.insert(id),
                "empty or duplicate native entry identity",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileStat {
    pub absolute_path: String,
    pub version: String,
    pub bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileBytePage {
    pub absolute_path: String,
    pub version: String,
    pub bytes: Option<u64>,
    pub offset: u64,
    pub data: String,
    pub eof: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileTextPage {
    pub absolute_path: String,
    pub version: String,
    pub bytes: Option<u64>,
    pub offset: u64,
    pub text: String,
    pub lines: usize,
    pub eof: bool,
}
pub fn unsupported(feature: &str) -> Error {
    Error::Unsupported(feature.to_owned())
}

impl SessionFileBytePage {
    pub fn validate(&self, offset: u64, length: usize) -> Result<()> {
        use base64::Engine;
        require(
            !self.absolute_path.is_empty() && !self.version.is_empty() && self.offset == offset,
            "file page identity/offset mismatch",
        )?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|_| Error::Protocol("invalid file byte encoding".into()))?;
        require(
            bytes.len() <= length && (!bytes.is_empty() || self.eof),
            "invalid file page progress",
        )?;
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error::Protocol("file offset overflow".into()))?;
        if let Some(total) = self.bytes {
            require(
                end <= total && self.eof == (end == total),
                "invalid file page total/eof",
            )?;
        }
        Ok(())
    }
}

impl SessionFileTextPage {
    pub fn validate(&self, offset: u64, limit: usize) -> Result<()> {
        require(
            !self.absolute_path.is_empty()
                && !self.version.is_empty()
                && offset >= 1
                && self.offset == offset,
            "text page identity/offset mismatch",
        )?;
        require(
            self.lines <= limit && (self.lines > 0 || self.eof),
            "invalid text page progress",
        )
    }
}

pub fn validate_file_watch(frame: &Value) -> Result<()> {
    match crate::string(frame, "kind")? {
        "ready" => Ok(()),
        "change" => {
            let change = &frame["change"];
            require(
                !crate::string(change, "absolutePath")?.is_empty(),
                "missing file change path",
            )?;
            if let Some(absent) = change.get("absent") {
                require(absent.is_boolean(), "invalid absence marker")?;
            }
            if change["absent"] != true {
                require(
                    !crate::string(change, "version")?.is_empty(),
                    "missing file change version",
                )?;
            }
            Ok(())
        }
        _ => Err(crate::Error::Protocol(
            "unknown file observation frame".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_page_rejects_nonprogress_offsets_and_invalid_totals() {
        let mut page = SessionFileBytePage {
            absolute_path: "/work/file".into(),
            version: "v".into(),
            bytes: Some(4),
            offset: 0,
            data: "YWJj".into(),
            eof: false,
        };
        page.validate(0, 3).unwrap();
        assert!(page.validate(1, 3).is_err());
        page.eof = true;
        assert!(page.validate(0, 3).is_err());
        page.eof = false;
        page.data.clear();
        assert!(page.validate(0, 3).is_err());
        page.data = "!".into();
        assert!(page.validate(0, 3).is_err());
    }
}
