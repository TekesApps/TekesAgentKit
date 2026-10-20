//! ClientKit-aligned headless SessionEndpoint contract. Extension payloads stay lossless JSON.
mod annotations;
mod commands;
pub use annotations::*;
mod thread_actions;
pub use commands::*;
pub use thread_actions::*;
mod capabilities;
mod registry;
pub use capabilities::*;
mod session;
use async_trait::async_trait;
use futures_util::stream::BoxStream;
pub use registry::{HARNESS_EXTENSION_EVENTS, REQUIRED_EVENTS, SURFACE_EVENTS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use session::*;
use std::collections::{BTreeMap, BTreeSet};

pub type Result<T> = std::result::Result<T, Error>;
pub type FrameStream = BoxStream<'static, Result<Value>>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("remote: {0}")]
    Remote(Value),
    #[error("request timed out; reconcile before retrying mutations")]
    Timeout,
    #[error("endpoint disconnected")]
    Disconnected,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Protocol(message.into()))
    }
}
pub fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Protocol(format!("missing string {key}")))
}
// JSONDecoder accepts integral numeric spellings (1, 1.0, 1e0) for Swift Int.
fn value_integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        let number = value.as_f64()?;
        (number.is_finite()
            && number.fract() == 0.0
            && number >= i64::MIN as f64
            && number < -(i64::MIN as f64))
            .then_some(number as i64)
    })
}
fn decode_i64<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<i64, D::Error> {
    value_integer(&Value::deserialize(deserializer)?)
        .ok_or_else(|| serde::de::Error::custom("expected exact signed integer"))
}
fn decode_usize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<usize, D::Error> {
    usize::try_from(decode_i64(deserializer)?).map_err(serde::de::Error::custom)
}
fn decode_optional_i64<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<i64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        Ok(None)
    } else {
        value_integer(&value)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expected optional exact integer"))
    }
}
pub fn integer(v: &Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(value_integer)
        .ok_or_else(|| Error::Protocol(format!("missing integer {key}")))
}
pub fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Protocol(format!("missing array {key}")))
}
pub fn boolean(v: &Value, key: &str) -> Result<bool> {
    v.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| Error::Protocol(format!("missing bool {key}")))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostDescription {
    #[serde(deserialize_with = "decode_i64")]
    pub protocol_version: i64,
    pub product: Product,
    pub capabilities: BTreeSet<String>,
    pub cwd: String,
    pub home: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(deserialize_with = "decode_i64")]
    pub attached_sessions: i64,
    pub can_open_path: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Product {
    pub name: String,
    pub version: String,
}
impl HostDescription {
    /// NativeSessionEndpoint requires v4, rather than the Pi/DSH v3 compatibility path.
    pub fn validate_kernel(&self) -> Result<()> {
        require(
            self.protocol_version == 4,
            "Kernel requires protocolVersion 4",
        )?;
        require(
            !self.product.name.is_empty() && !self.product.version.is_empty(),
            "empty product identity",
        )?;
        require(self.attached_sessions >= 0, "negative attachedSessions")?;
        let known = [
            "workspace-sync",
            "session-inventory-sync",
            "session-journal",
            "projected-stream",
            "session-control-sync",
            "actionable-sync",
            "models",
            "attachments",
            "management",
        ];
        require(
            self.capabilities
                .iter()
                .all(|cap| known.contains(&cap.as_str())),
            "unknown closed capability",
        )?;
        for cap in [
            "session-inventory-sync",
            "session-control-sync",
            "actionable-sync",
        ] {
            require(
                self.capabilities.contains(cap),
                &format!("missing capability {cap}"),
            )?;
        }
        // This milestone implements the native journal, not the optional projected-stream adapter.
        require(
            self.capabilities.contains("session-journal"),
            "native journal capability required",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct SessionAddress {
    pub endpoint_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRequest {
    pub session_id: String,
    #[serde(deserialize_with = "decode_i64")]
    pub through_sequence: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "decode_optional_i64")]
    pub before_sequence: Option<i64>,
    #[serde(deserialize_with = "decode_usize")]
    pub max_messages: usize,
}
impl HistoryRequest {
    pub fn validate(&self) -> Result<()> {
        require(
            self.through_sequence >= -1 && self.max_messages > 0,
            "invalid history request",
        )?;
        if let Some(before) = self.before_sequence {
            require(
                before >= -1 && before <= self.through_sequence,
                "beforeSequence beyond frozen cut",
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub event: SessionEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_model_only: Option<bool>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(deserialize_with = "decode_i64")]
    pub seq: i64,
    /// Unix milliseconds, matching the public SEP event wire contract.
    pub time: f64,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignorable: Option<bool>,
    // Keep unknown ignorable event metadata opaque, like ClientKit's custom decoder.
    #[serde(flatten)]
    pub metadata: BTreeMap<String, Value>,
}
impl SessionEvent {
    pub fn validate(&self) -> Result<()> {
        require(
            self.seq >= 0 && self.time.is_finite() && self.time >= 0.0,
            "invalid event envelope",
        )?;
        require(
            self.ignorable != Some(false),
            "ignorable must be true or absent",
        )?;
        let known = REQUIRED_EVENTS.contains(&self.kind.as_str())
            || HARNESS_EXTENSION_EVENTS.contains(&self.kind.as_str());
        require(
            known || self.ignorable == Some(true),
            "unknown required event",
        )?;
        if !known {
            return Ok(());
        }
        let sources = self
            .metadata
            .get("sourceEventSeqs")
            .filter(|v| !v.is_null());
        let op = self.metadata.get("surfaceOp").filter(|v| !v.is_null());
        require(
            SURFACE_EVENTS.contains(&self.kind.as_str()) || (sources.is_none() && op.is_none()),
            "surface metadata on non-surface event",
        )?;
        if let Some(sources) = sources {
            let sources = sources
                .as_array()
                .ok_or_else(|| Error::Protocol("invalid source sequences".into()))?;
            let mut seen = BTreeSet::new();
            for source in sources {
                let n = value_integer(source)
                    .ok_or_else(|| Error::Protocol("invalid source sequence".into()))?;
                require(
                    n >= 0 && n < self.seq && seen.insert(n),
                    "invalid or duplicate source sequence",
                )?;
            }
        }
        if let Some(op) = op {
            if op == "append" {
                return Ok(());
            }
            require(string(op, "op")? == "replace", "invalid surface operation")?;
            if op.get("startSeq").is_some() || op.get("endSeq").is_some() {
                require(
                    op.as_object().is_some_and(|o| o.len() == 3),
                    "mixed replacement bounds",
                )?;
                let (start, end) = (integer(op, "startSeq")?, integer(op, "endSeq")?);
                require(
                    start >= 0 && end >= 0 && start < self.seq && end < self.seq,
                    "invalid sequence replacement",
                )?;
            } else {
                let (start, end) = (integer(op, "start")?, integer(op, "end")?);
                require(start >= 0 && end >= start, "invalid surface replacement")?;
            }
        }
        Ok(())
    }
}
pub fn validate_entries(entries: &[HistoryEntry], cut: i64) -> Result<()> {
    let mut previous = -1;
    for entry in entries {
        entry.event.validate()?;
        require(
            entry.event.seq > previous && entry.event.seq <= cut,
            "nonascending history or entry beyond cut",
        )?;
        previous = entry.event.seq;
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JournalSnapshot {
    pub session_id: String,
    pub generation: String,
    #[serde(deserialize_with = "decode_i64")]
    pub through_sequence: i64,
    #[serde(deserialize_with = "decode_usize")]
    pub window_limit: usize,
    pub entries: Vec<HistoryEntry>,
    pub has_more_before: bool,
    pub projections: Option<ProjectionBlock>,
    pub history_revision: Option<String>,
}
impl JournalSnapshot {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.generation.is_empty() && self.through_sequence >= -1 && self.window_limit > 0,
            "invalid journal snapshot",
        )?;
        validate_entries(&self.entries, self.through_sequence)?;
        if let Some(p) = &self.projections {
            require(
                p.as_of_seq <= self.through_sequence,
                "projection beyond frozen cut",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub session_id: String,
    #[serde(deserialize_with = "decode_i64")]
    pub through_sequence: i64,
    pub entries: Vec<HistoryEntry>,
    pub has_more_before: bool,
}
impl HistoryPage {
    pub fn validate(&self, request: &HistoryRequest) -> Result<()> {
        request.validate()?;
        require(
            self.session_id == request.session_id
                && self.through_sequence == request.through_sequence,
            "history response identity/cut mismatch",
        )?;
        validate_entries(&self.entries, self.through_sequence)?;
        if let (Some(before), Some(last)) = (request.before_sequence, self.entries.last()) {
            require(last.event.seq < before, "history overlaps cursor")?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionBlock {
    #[serde(deserialize_with = "decode_i64")]
    pub as_of_seq: i64,
    pub values: BTreeMap<String, Value>,
}

/// Native legacy title projection normalization. Explicit null is a real clear, not absence.
pub fn normalize_title(value: &mut Value) {
    match value {
        Value::Array(a) => a.iter_mut().for_each(normalize_title),
        Value::Object(o) => {
            o.values_mut().for_each(normalize_title);
            if o.contains_key("asOfSeq")
                && let Some(values) = o.get_mut("values").and_then(Value::as_object_mut)
                && let Some(legacy) = values.remove("sessionTitle")
                && !values.contains_key("title")
            {
                if legacy.is_null() {
                    values.insert("title".into(), Value::Null);
                } else if let Some(title) = legacy.get("title") {
                    values.insert("title".into(), title.clone());
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StreamTarget {
    WorkspaceInventory,
    SessionInventory,
    SessionControl,
    Actionables,
    SessionConversation {
        address: JournalAddress,
        #[serde(rename = "maxMessages")]
        max_messages: usize,
    },
    SessionJournal {
        address: JournalAddress,
        #[serde(rename = "maxMessages")]
        max_messages: usize,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JournalAddress {
    pub session_id: String,
}

/// A logical stream validator. A fresh stream must begin with one baseline for its generation.
#[derive(Debug, Default, Clone)]
pub struct GenerationValidator {
    generation: Option<String>,
}
impl GenerationValidator {
    pub fn accept(&mut self, generation: &str, baseline: bool) -> Result<()> {
        require(!generation.is_empty(), "empty generation")?;
        match &self.generation {
            None => require(baseline, "delta before baseline")?,
            Some(previous) if previous == generation => require(!baseline, "duplicate baseline")?,
            Some(_) => require(baseline, "generation changed without baseline")?,
        }
        self.generation = Some(generation.into());
        Ok(())
    }
}

#[async_trait]
pub trait SessionEndpoint: Send + Sync {
    fn identity(&self) -> &str;
    fn host(&self) -> &HostDescription;
    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities::default()
    }
    fn content_kind(&self) -> SessionContentKind {
        SessionContentKind::Journal
    }
    fn supports_recovery(&self) -> bool {
        false
    }
    async fn load_earlier_conversation(&self, _session_id: &str, _count: usize) -> Result<bool> {
        Err(unsupported("native conversation pagination"))
    }
    async fn locator(&self, session_id: &str) -> Result<SessionLocator> {
        Ok(SessionLocator {
            address: SessionAddress {
                endpoint_id: self.identity().into(),
                session_id: session_id.into(),
            },
            harness: self.host().product.name.clone(),
            cwd: None,
            working_directory: None,
            native_session_file: None,
            native_session_id: Some(session_id.into()),
            history_source: "session-journal".into(),
        })
    }
    async fn observe_files(&self, _session_id: &str, _path: &str) -> Result<FrameStream> {
        Err(unsupported("file observation"))
    }
    async fn file_stat(&self, _session_id: &str, _path: &str) -> Result<SessionFileStat> {
        Err(unsupported("session file stat"))
    }
    async fn file_read_bytes(
        &self,
        _session_id: &str,
        _path: &str,
        _offset: u64,
        _length: usize,
    ) -> Result<SessionFileBytePage> {
        Err(unsupported("session file bytes"))
    }
    async fn file_read_text(
        &self,
        _session_id: &str,
        _path: &str,
        _offset: u64,
        _limit: usize,
    ) -> Result<SessionFileTextPage> {
        Err(unsupported("session file text"))
    }
    fn supports_events(&self) -> bool {
        false
    }
    async fn events(&self) -> Result<FrameStream> {
        Err(unsupported("endpoint events"))
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream>;
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage>;
    async fn respond(
        &self,
        actionable_id: &str,
        expected_revision: i64,
        outcome: Value,
    ) -> Result<()>;
    /// Exact unary envelope; callers retain rpc_id across uncertain mutation retries.
    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value>;
    async fn close(&self);
}
#[async_trait]
pub trait EndpointConnector: Send + Sync {
    async fn connect(&self) -> Result<std::sync::Arc<dyn SessionEndpoint>>;
}

mod subagents;
pub use subagents::*;

mod live_usage;
pub use live_usage::*;
