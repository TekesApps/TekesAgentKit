use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tekes_agent_contract::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum ConnectionState {
    #[default]
    Connecting,
    Ready,
    Failed(String),
    Stopped,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VersionedValue {
    pub sequence: i64,
    pub value: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct LivePreview {
    pub text: String,
    pub reasoning: String,
}
/// A headless read model; journal entries remain lossless. UI surface folding is out of scope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionProjection {
    pub summary: Option<Value>,
    pub native_conversation: Option<NativeConversationSnapshot>,
    pub native_events: std::collections::VecDeque<Value>,
    pub agent_error: Option<String>,
    pub entries: BTreeMap<i64, HistoryEntry>,
    pub previews: BTreeMap<String, LivePreview>,
    pub projections: BTreeMap<String, VersionedValue>,
    pub queue: Vec<Value>,
    pub jobs: Vec<Value>,
    pub tail_sequence: i64,
    pub history_through_sequence: i64,
    pub history_base_sequence: i64,
    pub has_more_before: bool,
    pub history_revision: Option<String>,
    pub journal_ready: bool,
    pub run_open: bool,
    pub journal_error: Option<String>,
}
impl Default for SessionProjection {
    fn default() -> Self {
        Self {
            summary: None,
            native_conversation: None,
            native_events: std::collections::VecDeque::new(),
            agent_error: None,
            entries: BTreeMap::new(),
            previews: BTreeMap::new(),
            projections: BTreeMap::new(),
            queue: Vec::new(),
            jobs: Vec::new(),
            tail_sequence: -1,
            history_through_sequence: -1,
            history_base_sequence: -1,
            has_more_before: false,
            history_revision: None,
            journal_ready: false,
            run_open: false,
            journal_error: None,
        }
    }
}
impl SessionProjection {
    fn block(&mut self, block: ProjectionBlock) {
        for (key, value) in block.values {
            let old = self.projections.get(&key);
            if old.is_none_or(|v| block.as_of_seq >= v.sequence) {
                self.projections.insert(
                    key,
                    VersionedValue {
                        sequence: block.as_of_seq,
                        value,
                    },
                );
            }
        }
    }
    pub fn apply_snapshot(&mut self, snapshot: JournalSnapshot) -> Result<()> {
        snapshot.validate()?;
        let base = snapshot.entries.first().map(|e| e.event.seq).unwrap_or(-1);
        let preserve = snapshot.history_revision == self.history_revision
            && base >= 0
            && !self.entries.is_empty()
            && base <= self.tail_sequence + 1;
        if preserve {
            for entry in &snapshot.entries {
                if let Some(old) = self.entries.get(&entry.event.seq) {
                    require(old == entry, "conflicting snapshot duplicate")?;
                }
            }
        }
        let preserved_earlier = preserve && self.history_base_sequence < base;
        if !preserve {
            self.entries.clear();
        }
        for entry in snapshot.entries {
            self.entries.insert(entry.event.seq, entry);
        }
        self.history_base_sequence = if preserved_earlier {
            self.history_base_sequence
        } else {
            base
        };
        if !preserved_earlier {
            self.has_more_before = snapshot.has_more_before;
        }
        self.tail_sequence = snapshot.through_sequence;
        self.history_through_sequence = snapshot.through_sequence;
        self.history_revision = snapshot.history_revision;
        self.previews.clear();
        self.journal_ready = true;
        self.journal_error = None;
        if let Some(block) = snapshot.projections {
            self.block(block);
        }
        self.refold();
        Ok(())
    }
    fn refold(&mut self) {
        self.run_open = false;
        self.previews.clear();
        // Fold only headless execution/preview state. All durable event semantics remain in entries.
        let events: Vec<_> = self.entries.values().map(|e| e.event.clone()).collect();
        for event in events {
            self.fold(&event);
        }
    }
    fn fold(&mut self, event: &SessionEvent) {
        match event.kind.as_str() {
            "turn/start" => self.run_open = true,
            "turn/end" => {
                self.run_open = false;
                self.previews.clear();
            }
            "assistant/message" | "step/end" => {
                self.previews.remove(&preview_key(event));
            }
            "assistant/chunk" => {
                let p = self.previews.entry(preview_key(event)).or_default();
                if let Some(presentation) = event.data.get("presentation") {
                    p.text = presentation["text"].as_str().unwrap_or("").into();
                    p.reasoning = presentation["reasoning"].as_str().unwrap_or("").into();
                } else {
                    let chunk = &event.data["chunk"];
                    let kind = chunk["type"].as_str().unwrap_or("");
                    let delta = chunk["text"]
                        .as_str()
                        .or_else(|| chunk["delta"].as_str())
                        .unwrap_or("");
                    if kind.contains("reasoning") {
                        p.reasoning.push_str(delta);
                    } else if kind.contains("text") {
                        p.text.push_str(delta);
                    }
                }
            }
            _ => {}
        }
    }
    pub fn apply_live(&mut self, entry: HistoryEntry) -> Result<()> {
        entry.event.validate()?;
        require(self.journal_ready, "live event before snapshot")?;
        let seq = entry.event.seq;
        if seq <= self.tail_sequence {
            if let Some(old) = self.entries.get(&seq) {
                require(old == &entry, "conflicting live duplicate")?;
            }
            return Ok(());
        }
        require(seq == self.tail_sequence + 1, "journal sequence gap")?;
        self.fold(&entry.event);
        self.tail_sequence = seq;
        if self.history_base_sequence < 0 {
            self.history_base_sequence = seq;
        }
        self.entries.insert(seq, entry);
        Ok(())
    }
    pub fn apply_page(&mut self, page: HistoryPage, request: &HistoryRequest) -> Result<bool> {
        page.validate(request)?;
        require(
            self.journal_ready
                && self.history_through_sequence == request.through_sequence
                && request.before_sequence == Some(self.history_base_sequence),
            "stale page result",
        )?;
        if page.entries.is_empty() {
            self.has_more_before = false;
            return Ok(false);
        }
        require(
            page.entries.last().expect("nonempty").event.seq == self.history_base_sequence - 1,
            "older page not contiguous",
        )?;
        for entry in &page.entries {
            if let Some(old) = self.entries.get(&entry.event.seq) {
                require(old == entry, "conflicting history duplicate")?;
            }
        }
        self.history_base_sequence = page.entries[0].event.seq;
        self.has_more_before = page.has_more_before;
        for entry in page.entries {
            self.entries.insert(entry.event.seq, entry);
        }
        // Prepending history must not erase the current process-local preview.
        let previews = self.previews.clone();
        self.refold();
        self.previews = previews;
        Ok(true)
    }
}
fn preview_key(event: &SessionEvent) -> String {
    format!("{}:{}", event.data["turn"], event.data["step"])
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MemoryProjectionStore {
    pub endpoint_id: String,
    pub connection: ConnectionState,
    pub host: Option<HostDescription>,
    pub last_error: Option<String>,
    pub sessions: BTreeMap<String, SessionProjection>,
    pub actionables: BTreeMap<String, Value>,
    pub inventory_ready: bool,
    pub control_ready: bool,
    pub actionables_ready: bool,
    #[serde(skip)]
    validators: BTreeMap<String, GenerationValidator>,
}
impl MemoryProjectionStore {
    pub fn new(endpoint_id: impl Into<String>) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            ..Self::default()
        }
    }
    pub fn begin_connection(&mut self, host: HostDescription) {
        self.disconnect();
        self.host = Some(host);
        self.connection = ConnectionState::Connecting;
    }
    pub fn disconnect(&mut self) {
        self.validators.clear();
        self.actionables.clear();
        self.inventory_ready = false;
        self.control_ready = false;
        self.actionables_ready = false;
        for session in self.sessions.values_mut() {
            session.journal_ready = false;
            session.previews.clear();
            session.native_events.clear();
        }
        self.connection = ConnectionState::Connecting;
    }
    pub fn reset_journal(&mut self, session_id: &str) {
        self.validators.remove(&format!("journal:{session_id}"));
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.journal_ready = false;
            s.previews.clear();
            s.native_events.clear();
        }
    }
    pub fn ready(&self) -> bool {
        self.inventory_ready && self.control_ready && self.actionables_ready
    }
    pub fn session(&self, address: &SessionAddress) -> Option<&SessionProjection> {
        (address.endpoint_id == self.endpoint_id)
            .then(|| self.sessions.get(&address.session_id))
            .flatten()
    }
    pub fn apply(&mut self, stream: &str, mut frame: Value) -> Result<()> {
        if matches!(stream, "inventory" | "control") {
            normalize_title(&mut frame);
        }
        let kind = string(&frame, "type")?.to_owned();
        let generation = if kind == "snapshot" || kind == "conversation" {
            string(&frame["snapshot"], "generation")?
        } else {
            string(&frame, "generation")?
        };
        let baseline = matches!(
            kind.as_str(),
            "baseline" | "reset" | "snapshot" | "conversation"
        );
        let mut validator = self.validators.get(stream).cloned().unwrap_or_default();
        validator.accept(generation, baseline)?;
        match stream {
            "inventory" => self.inventory(&kind, &frame)?,
            "control" => self.control(&kind, &frame)?,
            "actionables" => self.actionable(&kind, &frame)?,
            _ if stream.starts_with("journal:") => self.journal(&stream[8..], &kind, frame)?,
            _ => return Err(Error::Protocol("unknown logical stream".into())),
        }
        self.validators.insert(stream.into(), validator);
        if self.ready() {
            self.connection = ConnectionState::Ready;
        }
        Ok(())
    }
    fn inventory(&mut self, kind: &str, frame: &Value) -> Result<()> {
        match kind {
            "baseline" => {
                let items = array(&frame["snapshot"], "items")?;
                let mut summaries = BTreeMap::new();
                for item in items {
                    let id = validate_summary(item)?;
                    require(
                        summaries.insert(id.to_owned(), item.clone()).is_none(),
                        "duplicate inventory identity",
                    )?;
                }
                // Retain readable history for removed/archived sessions, but retire inventory authority.
                for session in self.sessions.values_mut() {
                    session.summary = None;
                }
                for (id, value) in summaries {
                    self.upsert_summary(&id, value)?;
                }
                self.inventory_ready = true;
            }
            "upsert" => {
                let value = frame["session"].clone();
                let id = validate_summary(&value)?.to_owned();
                self.upsert_summary(&id, value)?;
            }
            "remove" => {
                if let Some(s) = self.sessions.get_mut(string(frame, "sessionId")?) {
                    s.summary = None;
                }
            }
            "status" => {
                let id = string(frame, "sessionId")?;
                let running = boolean(frame, "running")?;
                if let Some(s) = self.sessions.get_mut(id) {
                    if let Some(summary) = &mut s.summary {
                        summary["running"] = Value::Bool(running);
                    }
                    if !s.journal_ready {
                        s.run_open = running;
                    }
                }
            }
            "plane" => {
                let id = string(frame, "sessionId")?;
                require(frame["updatedAt"].is_number(), "missing updatedAt")?;
                if let Some(summary) = self.sessions.get_mut(id).and_then(|s| s.summary.as_mut()) {
                    summary["updatedAt"] = frame["updatedAt"].clone();
                }
            }
            "agent-error" => {
                self.sessions
                    .entry(string(frame, "sessionId")?.into())
                    .or_default()
                    .agent_error = Some(string(frame, "message")?.into());
            }
            _ => return Err(Error::Protocol("wrong inventory frame".into())),
        }
        Ok(())
    }
    fn upsert_summary(&mut self, id: &str, value: Value) -> Result<()> {
        let block = value
            .get("projections")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value::<ProjectionBlock>(v.clone()))
            .transpose()?;
        let session = self.sessions.entry(id.into()).or_default();
        if !session.journal_ready {
            session.run_open = value["running"].as_bool().unwrap_or(false);
        }
        session.summary = Some(value);
        if let Some(block) = block {
            session.block(block);
        }
        Ok(())
    }
    fn control(&mut self, kind: &str, frame: &Value) -> Result<()> {
        match kind {
            "baseline" => {
                let snapshot = &frame["snapshot"];
                // Parse everything before committing baseline replacement.
                let queues = states(array(snapshot, "queues")?, "items")?;
                let jobs = states(array(snapshot, "jobs")?, "jobs")?;
                let mut projections = BTreeMap::new();
                for value in array(snapshot, "projections")? {
                    let id = string(value, "sessionId")?.to_owned();
                    let block: ProjectionBlock =
                        serde_json::from_value(value["projections"].clone())?;
                    require(
                        projections.insert(id, block).is_none(),
                        "duplicate projection identity",
                    )?;
                }
                for session in self.sessions.values_mut() {
                    session.queue.clear();
                    session.jobs.clear();
                }
                for (id, items) in queues {
                    self.sessions.entry(id).or_default().queue = items;
                }
                for (id, jobs) in jobs {
                    self.sessions.entry(id).or_default().jobs = jobs;
                }
                for (id, block) in projections {
                    self.sessions.entry(id).or_default().block(block);
                }
                self.control_ready = true;
            }
            "queue" | "jobs" => {
                let state = &frame["state"];
                let id = string(state, "sessionId")?.to_owned();
                let items = array(state, if kind == "queue" { "items" } else { "jobs" })?.clone();
                let s = self.sessions.entry(id).or_default();
                if kind == "queue" {
                    s.queue = items;
                } else {
                    s.jobs = items;
                }
            }
            "projection" => {
                let state = &frame["state"];
                let id = string(state, "sessionId")?.to_owned();
                let block: ProjectionBlock = serde_json::from_value(state["projections"].clone())?;
                self.sessions.entry(id).or_default().block(block);
            }
            _ => return Err(Error::Protocol("wrong control frame".into())),
        }
        Ok(())
    }
    fn actionable(&mut self, kind: &str, frame: &Value) -> Result<()> {
        match kind {
            "reset" => {
                self.actionables.clear();
                self.actionables_ready = true;
            }
            "upsert" => {
                let value = &frame["actionable"];
                let id = string(value, "id")?;
                string(value, "sessionId")?;
                string(value, "kind")?;
                require(
                    value.get("payload").is_some() && integer(value, "revision")? >= 0,
                    "invalid actionable",
                )?;
                let revision = integer(value, "revision")?;
                if self
                    .actionables
                    .get(id)
                    .is_none_or(|old| integer(old, "revision").unwrap_or(-1) <= revision)
                {
                    self.actionables.insert(id.into(), value.clone());
                }
            }
            "resolved" => {
                let id = string(frame, "actionableId")?;
                let revision = integer(frame, "revision")?;
                require(revision >= 0, "invalid actionable revision")?;
                if self
                    .actionables
                    .get(id)
                    .is_some_and(|old| integer(old, "revision").unwrap_or(-1) <= revision)
                {
                    self.actionables.remove(id);
                }
            }
            _ => return Err(Error::Protocol("wrong actionable frame".into())),
        }
        Ok(())
    }
    fn journal(&mut self, id: &str, kind: &str, frame: Value) -> Result<()> {
        match kind {
            "conversation" => {
                let snapshot: NativeConversationSnapshot =
                    serde_json::from_value(frame["snapshot"].clone())?;
                snapshot.validate()?;
                require(snapshot.session_id == id, "conversation session mismatch")?;
                if let Some(generation) = frame.get("generation").and_then(Value::as_str) {
                    require(
                        snapshot.generation == generation,
                        "conversation generation mismatch",
                    )?;
                }
                let session = self.sessions.entry(id.into()).or_default();
                session.run_open = snapshot.is_streaming;
                session.native_conversation = Some(snapshot);
                session.journal_ready = true;
                session.journal_error = None;
                session.native_events.clear();
            }
            "conversation-update" => {
                let snapshot: NativeConversationSnapshot =
                    serde_json::from_value(frame["snapshot"].clone())?;
                snapshot.validate()?;
                require(snapshot.session_id == id, "conversation session mismatch")?;
                if let Some(generation) = frame.get("generation").and_then(Value::as_str) {
                    require(
                        snapshot.generation == generation,
                        "conversation generation mismatch",
                    )?;
                }
                let session = self.sessions.entry(id.into()).or_default();
                session.run_open = snapshot.is_streaming;
                session.native_conversation = Some(snapshot);
            }
            "native-event" => {
                require(
                    string(&frame, "sessionId")? == id,
                    "native event session mismatch",
                )?;
                let event = frame
                    .get("event")
                    .ok_or_else(|| Error::Protocol("missing native event".into()))?
                    .clone();
                let session = self.sessions.entry(id.into()).or_default();
                if session.native_events.len() >= 256 {
                    session.native_events.pop_front();
                }
                session.native_events.push_back(event);
            }
            "snapshot" => {
                let snapshot: JournalSnapshot = serde_json::from_value(frame["snapshot"].clone())?;
                require(snapshot.session_id == id, "snapshot session mismatch")?;
                self.sessions
                    .entry(id.into())
                    .or_default()
                    .apply_snapshot(snapshot)?;
            }
            "event" | "transient" => {
                require(
                    string(&frame, "sessionId")? == id,
                    "journal session mismatch",
                )?;
                if kind == "event" {
                    let entry: HistoryEntry = serde_json::from_value(frame["entry"].clone())?;
                    self.sessions
                        .entry(id.into())
                        .or_default()
                        .apply_live(entry)?;
                } else {
                    let event: SessionEvent = serde_json::from_value(frame["event"].clone())?;
                    event.validate()?;
                    require(event.kind == "assistant/chunk", "invalid transient event")?;
                    integer(&event.data, "turn")?;
                    integer(&event.data, "step")?;
                    self.sessions.entry(id.into()).or_default().fold(&event);
                }
            }
            "projection" => {
                let p = &frame["frame"];
                require(string(p, "sessionId")? == id, "projection session mismatch")?;
                let key = string(p, "key")?.to_owned();
                let seq = integer(p, "seq")?;
                let value = p
                    .get("value")
                    .ok_or_else(|| Error::Protocol("missing projection value".into()))?
                    .clone();
                self.sessions
                    .entry(id.into())
                    .or_default()
                    .block(ProjectionBlock {
                        as_of_seq: seq,
                        values: [(key, value)].into(),
                    });
            }
            _ => return Err(Error::Protocol("wrong journal frame".into())),
        }
        Ok(())
    }
}
fn states(values: &[Value], field: &str) -> Result<BTreeMap<String, Vec<Value>>> {
    let mut out = BTreeMap::new();
    for state in values {
        let id = string(state, "sessionId")?.to_owned();
        require(
            out.insert(id, array(state, field)?.clone()).is_none(),
            "duplicate control identity",
        )?;
    }
    Ok(out)
}
fn validate_summary(value: &Value) -> Result<&str> {
    boolean(value, "running")?;
    boolean(value, "blank")?;
    require(value["updatedAt"].is_number(), "missing summary updatedAt")?;
    if let Some(block) = value.get("projections").filter(|v| !v.is_null()) {
        let _: ProjectionBlock = serde_json::from_value(block.clone())?;
    }
    string(value, "sessionId")
}
