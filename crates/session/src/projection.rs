use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tekes_agent_contract::*;

#[cfg(test)]
thread_local! {
    static VALIDATE_BLOCK_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

type InventoryRows = BTreeMap<String, (Value, Option<ProjectionBlock>)>;
fn inventory_rows(frame: &Value) -> Result<InventoryRows> {
    let mut rows = BTreeMap::new();
    for item in array(&frame["snapshot"], "items")? {
        let id = validate_summary(item)?.to_owned();
        let block = item
            .get("projections")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value::<ProjectionBlock>(v.clone()))
            .transpose()?;
        require(
            rows.insert(id, (item.clone(), block)).is_none(),
            "duplicate inventory identity",
        )?;
    }
    Ok(rows)
}

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
    #[serde(default)]
    pub reasoning_ended: bool,
    /// Latest provider report for this step. None means unavailable, not zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Dialect presentation anchor, not the transient transport's seq. For
    /// transient chunks this is the last durable cursor + 1 at receipt time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_order: Option<i64>,
}
/// A headless read model; journal entries remain lossless. UI surface folding is out of scope.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionProjection {
    pub summary: Option<Value>,
    pub native_conversation: Option<NativeConversationSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_reading_identities: Vec<NativeMessageReadingIdentity>,
    pub native_events: std::collections::VecDeque<Value>,
    /// None permits legacy event-only snapshots. Idle is an explicit retirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_assistant: Option<crate::NativeAssistantProjection>,
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
            native_reading_identities: vec![],
            native_events: std::collections::VecDeque::new(),
            native_assistant: None,
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
    fn push_native_event(&mut self, event: Value) {
        if let Some(next) =
            crate::NativeAssistantProjection::receive(self.native_assistant.as_ref(), &event)
        {
            self.native_assistant = Some(next);
        }
        if self.native_events.len() >= 256 {
            self.native_events.pop_front();
        }
        self.native_events.push_back(event);
    }
    fn has_ordered_queue(&self, key: Option<&str>) -> bool {
        key.is_some_and(|key| self.projections.contains_key(key))
    }
    fn validate_block(&self, block: &ProjectionBlock, queue_key: Option<&str>) -> Result<()> {
        #[cfg(test)]
        VALIDATE_BLOCK_CALLS.with(|calls| calls.set(calls.get() + 1));
        // Validate every value before changing any projection.
        for (key, value) in &block.values {
            if Some(key.as_str()) == queue_key
                && let Some(old) = self.projections.get(key)
                && block.as_of_seq == old.sequence
            {
                require(
                    old.value == *value,
                    "conflicting projection at same sequence",
                )?;
            }
            if Some(key.as_str()) == queue_key {
                validate_queue_projection(value)?;
            }
        }
        Ok(())
    }
    fn block(&mut self, block: ProjectionBlock, queue_key: Option<&str>) -> Result<()> {
        self.validate_block(&block, queue_key)?;
        self.commit_block(block, queue_key);
        Ok(())
    }
    fn commit_block(&mut self, block: ProjectionBlock, queue_key: Option<&str>) {
        for (key, value) in block.values {
            let accepted = self.projections.get(&key).is_none_or(|old| {
                block.as_of_seq > old.sequence
                    || (Some(key.as_str()) != queue_key && block.as_of_seq == old.sequence)
            });
            if accepted {
                if Some(key.as_str()) == queue_key {
                    self.queue = value["items"]
                        .as_array()
                        .expect("validated queue items")
                        .clone();
                }
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
        self.apply_snapshot_with_queue(snapshot, None)
    }
    fn apply_snapshot_with_queue(
        &mut self,
        snapshot: JournalSnapshot,
        queue_key: Option<&str>,
    ) -> Result<()> {
        snapshot.validate()?;
        if let Some(block) = &snapshot.projections {
            self.validate_block(block, queue_key)?;
        }
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
            self.commit_block(block, queue_key);
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
        self.fold_at(event, event.seq);
    }
    fn fold_at(&mut self, event: &SessionEvent, presentation_order: i64) {
        match event.kind.as_str() {
            "turn/start" => self.run_open = true,
            "turn/end" => {
                self.run_open = false;
                self.previews.clear();
            }
            "assistant/message" | "assistant/attempt" | "step/end" => {
                self.previews.remove(&preview_key(event));
            }
            "assistant/chunk" => {
                if event.data["chunk"]["type"] == "usage" {
                    // Transient usage must not resurrect a closed turn or a sealed step.
                    let current = self
                        .entries
                        .values()
                        .rev()
                        .find(|e| e.event.kind == "turn/start");
                    let sealed = self.entries.values().any(|e| {
                        matches!(e.event.kind.as_str(), "assistant/message" | "step/end")
                            && e.event.data["turn"] == event.data["turn"]
                            && e.event.data["step"] == event.data["step"]
                    });
                    if !self.run_open
                        || sealed
                        || current.is_some_and(|e| e.event.data["turn"] != event.data["turn"])
                    {
                        return;
                    }
                    if let Some(count) = event.data["chunk"]["usage"]["outputTokens"].as_u64() {
                        self.previews
                            .entry(preview_key(event))
                            .or_default()
                            .output_tokens = Some(count);
                    }
                    return;
                }
                let p = self.previews.entry(preview_key(event)).or_default();
                p.presentation_order = Some(presentation_order);
                let kind = event.data["chunk"]["type"].as_str().unwrap_or("");
                if kind == "reasoning-end" || kind.starts_with("text") {
                    p.reasoning_ended = true;
                } else if kind.starts_with("reasoning") {
                    p.reasoning_ended = false;
                }
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
    pub connection_epoch: u64,
    pub connection: ConnectionState,
    pub host: Option<HostDescription>,
    pub last_error: Option<String>,
    pub sessions: BTreeMap<String, SessionProjection>,
    pub actionables: BTreeMap<String, Value>,
    pub endpoint_capabilities: EndpointCapabilities,
    pub endpoint_events: std::collections::VecDeque<Value>,
    pub inventory_ready: bool,
    pub workspaces: Vec<Value>,
    pub archived_session_ids: Vec<String>,
    pub pinned_session_ids: Vec<String>,
    pub workspace_ready: bool,
    pub control_ready: bool,
    pub actionables_ready: bool,
    /// Local adapter authorization. Remote projection keys never configure this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequenced_queue_projection_key: Option<String>,
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
    pub(crate) fn configure_sequenced_queue_projection(&mut self, key: Option<&str>) {
        if self.sequenced_queue_projection_key.as_deref() != key {
            // Retained opaque remote values cannot acquire local authority when
            // the connector changes its adapter configuration.
            for session in self.sessions.values_mut() {
                if let Some(old) = self.sequenced_queue_projection_key.as_deref() {
                    session.projections.remove(old);
                }
                if let Some(new) = key {
                    session.projections.remove(new);
                }
                session.queue.clear();
            }
        }
        self.sequenced_queue_projection_key = key.map(str::to_owned);
    }
    pub fn begin_connection(&mut self, host: HostDescription) {
        self.disconnect();
        self.host = Some(host);
        self.connection = ConnectionState::Connecting;
    }
    pub fn disconnect(&mut self) {
        self.connection_epoch = self.connection_epoch.wrapping_add(1);
        self.validators.clear();
        self.actionables.clear();
        self.endpoint_events.clear();
        self.inventory_ready = false;
        self.workspace_ready = false;
        self.control_ready = false;
        self.actionables_ready = false;
        for session in self.sessions.values_mut() {
            session.journal_ready = false;
            session.previews.clear();
            session.native_events.clear();
            session.native_assistant = None;
            session.native_reading_identities.clear();
        }
        self.connection = ConnectionState::Connecting;
    }
    pub fn reset_journal(&mut self, session_id: &str) {
        self.validators.remove(&format!("journal:{session_id}"));
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.journal_ready = false;
            s.previews.clear();
            s.native_events.clear();
            s.native_assistant = None;
            s.native_reading_identities.clear();
        }
    }
    pub fn ready(&self) -> bool {
        self.inventory_ready
            && self.control_ready
            && self.actionables_ready
            && (self.workspace_ready
                || !self
                    .host
                    .as_ref()
                    .is_some_and(|h| h.capabilities.contains("workspace-sync")))
    }
    pub fn session(&self, address: &SessionAddress) -> Option<&SessionProjection> {
        (address.endpoint_id == self.endpoint_id)
            .then(|| self.sessions.get(&address.session_id))
            .flatten()
    }
    /// Replace fresh reader baselines in one publication, without cloning any
    /// journal, preview or history. Parse every summary/projection before commit.
    pub(crate) fn replace_inventory_baselines(
        &mut self,
        workspace: Option<Value>,
        mut inventory: Value,
    ) -> Result<()> {
        require(
            inventory["type"] == "baseline",
            "inventory refresh requires baseline",
        )?;
        let generation = string(&inventory, "generation")?.to_owned();
        let mut validator = GenerationValidator::default();
        validator.accept(&generation, true)?;
        normalize_title(&mut inventory);
        let rows = inventory_rows(&inventory)?;
        self.validate_inventory_blocks(&rows)?;
        let expects_workspace = self
            .host
            .as_ref()
            .is_some_and(|h| h.capabilities.contains("workspace-sync"));
        require(
            workspace.is_some() == expects_workspace,
            "inventory refresh workspace capability mismatch",
        )?;
        let workspace = workspace
            .map(|frame| -> Result<Self> {
                require(
                    frame["type"] == "baseline" && frame["generation"] == generation,
                    "incoherent workspace/inventory refresh generation",
                )?;
                let mut staged = Self::new(self.endpoint_id.clone());
                staged.apply("workspace", frame)?;
                Ok(staged)
            })
            .transpose()?;
        if let Some(staged) = workspace {
            self.workspaces = staged.workspaces;
            self.archived_session_ids = staged.archived_session_ids;
            self.pinned_session_ids = staged.pinned_session_ids;
            self.workspace_ready = true;
            self.validators
                .insert("workspace".into(), validator.clone());
        }
        self.commit_inventory(rows)?;
        self.validators.insert("inventory".into(), validator);
        if self.ready() {
            self.connection = ConnectionState::Ready;
        }
        Ok(())
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
            "events" => match kind.as_str() {
                "baseline" => self.endpoint_events.clear(),
                "event" => {
                    let event = frame
                        .get("event")
                        .ok_or_else(|| Error::Protocol("missing endpoint event".into()))?
                        .clone();
                    if self.endpoint_events.len() >= 256 {
                        self.endpoint_events.pop_front();
                    }
                    self.endpoint_events.push_back(event);
                }
                _ => return Err(Error::Protocol("unexpected endpoint event frame".into())),
            },
            "workspace" => self.workspace(&kind, &frame)?,
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
    fn workspace(&mut self, kind: &str, frame: &Value) -> Result<()> {
        let ids = |value: &Value| -> Result<Vec<String>> {
            let mut seen = std::collections::BTreeSet::new();
            value
                .as_array()
                .ok_or_else(|| Error::Protocol("invalid workspace identity list".into()))?
                .iter()
                .map(|v| {
                    let id = v
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| Error::Protocol("invalid workspace identity".into()))?;
                    require(seen.insert(id), "duplicate workspace identity")?;
                    Ok(id.into())
                })
                .collect()
        };
        match kind {
            "baseline" | "replace" => {
                let snapshot = &frame["snapshot"];
                let items = array(snapshot, "items")?.clone();
                let mut seen = std::collections::BTreeSet::new();
                for item in &items {
                    require(
                        seen.insert(string(item, "workspaceId")?),
                        "duplicate workspace",
                    )?;
                    string(item, "path")?;
                }
                let archived = ids(&snapshot["archivedSessionIds"])?;
                let pinned = match snapshot.get("pinnedSessionIds") {
                    Some(v) => ids(v)?,
                    None => Vec::new(),
                };
                self.workspaces = items;
                self.archived_session_ids = archived;
                self.pinned_session_ids = pinned;
                self.workspace_ready = true;
            }
            "upsert" => {
                let value = frame["workspace"].clone();
                let id = string(&value, "workspaceId")?;
                if let Some(old) = self
                    .workspaces
                    .iter_mut()
                    .find(|old| old["workspaceId"] == id)
                {
                    *old = value;
                } else {
                    self.workspaces.push(value);
                }
            }
            "remove" => {
                let id = string(frame, "workspaceId")?;
                self.workspaces.retain(|w| w["workspaceId"] != id);
            }
            "reorder" => {
                let order = ids(&frame["workspaceIds"])?;
                require(
                    order.len() == self.workspaces.len()
                        && order
                            .iter()
                            .all(|id| self.workspaces.iter().any(|w| w["workspaceId"] == *id)),
                    "workspace reorder must be complete",
                )?;
                self.workspaces
                    .sort_by_key(|w| order.iter().position(|id| w["workspaceId"] == *id).unwrap());
            }
            "archived" => self.archived_session_ids = ids(&frame["sessionIds"])?,
            "pinned" => self.pinned_session_ids = ids(&frame["sessionIds"])?,
            _ => return Err(Error::Protocol("unknown workspace frame".into())),
        }
        Ok(())
    }
    fn inventory(&mut self, kind: &str, frame: &Value) -> Result<()> {
        match kind {
            "baseline" => {
                let rows = inventory_rows(frame)?;
                self.commit_inventory(rows)?;
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
                    // Operational errors belong to the preceding execution.
                    // Only an ordered live inventory start clears them; replaying
                    // turn/start from snapshots or pagination must not do so.
                    if running {
                        s.agent_error = None;
                    }
                    if let Some(summary) = &mut s.summary {
                        summary["running"] = Value::Bool(running);
                    }
                    if !s.journal_ready || s.journal_error.is_some() {
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
        self.commit_summary(id, value, block)?;
        Ok(())
    }
    fn commit_inventory(&mut self, rows: InventoryRows) -> Result<()> {
        self.validate_inventory_blocks(&rows)?;
        // Missing membership retires authority, never a reader's retained history.
        for session in self.sessions.values_mut() {
            session.summary = None;
        }
        for (id, (value, block)) in rows {
            self.commit_summary(&id, value, block)?;
        }
        self.inventory_ready = true;
        Ok(())
    }
    fn validate_inventory_blocks(&self, rows: &InventoryRows) -> Result<()> {
        let queue_key = self.sequenced_queue_projection_key.as_deref();
        for (id, (_, block)) in rows {
            if let Some(block) = block {
                if let Some(session) = self.sessions.get(id) {
                    session.validate_block(block, queue_key)?;
                } else {
                    SessionProjection::default().validate_block(block, queue_key)?;
                }
            }
        }
        Ok(())
    }
    fn commit_summary(
        &mut self,
        id: &str,
        value: Value,
        block: Option<ProjectionBlock>,
    ) -> Result<()> {
        let queue_key = self.sequenced_queue_projection_key.clone();
        if let Some(block) = &block {
            if let Some(session) = self.sessions.get(id) {
                session.validate_block(block, queue_key.as_deref())?;
            } else {
                SessionProjection::default().validate_block(block, queue_key.as_deref())?;
            }
        }
        let session = self.sessions.entry(id.into()).or_default();
        if !session.journal_ready || session.journal_error.is_some() {
            session.run_open = value["running"].as_bool().unwrap_or(false);
        }
        session.summary = Some(value);
        if let Some(block) = block {
            session.block(block, queue_key.as_deref())?;
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
                let queue_key = self.sequenced_queue_projection_key.as_deref();
                for (id, block) in &projections {
                    if let Some(session) = self.sessions.get(id) {
                        session.validate_block(block, queue_key)?;
                    } else {
                        SessionProjection::default().validate_block(block, queue_key)?;
                    }
                }
                for session in self.sessions.values_mut() {
                    if !session.has_ordered_queue(queue_key) {
                        session.queue.clear();
                    }
                    session.jobs.clear();
                }
                for (id, items) in queues {
                    let session = self.sessions.entry(id).or_default();
                    if !session.has_ordered_queue(queue_key) {
                        session.queue = items;
                    }
                }
                for (id, jobs) in jobs {
                    self.sessions.entry(id).or_default().jobs = jobs;
                }
                for (id, block) in projections {
                    self.sessions
                        .entry(id)
                        .or_default()
                        .block(block, queue_key)?;
                }
                self.control_ready = true;
            }
            "queue" | "jobs" => {
                let state = &frame["state"];
                let id = string(state, "sessionId")?.to_owned();
                let items = array(state, if kind == "queue" { "items" } else { "jobs" })?.clone();
                let s = self.sessions.entry(id).or_default();
                if kind == "queue" {
                    if !s.has_ordered_queue(self.sequenced_queue_projection_key.as_deref()) {
                        s.queue = items;
                    }
                } else {
                    s.jobs = items;
                }
            }
            "projection" => {
                let state = &frame["state"];
                let id = string(state, "sessionId")?.to_owned();
                let block: ProjectionBlock = serde_json::from_value(state["projections"].clone())?;
                self.sessions
                    .entry(id)
                    .or_default()
                    .block(block, self.sequenced_queue_projection_key.as_deref())?;
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
                let identities: Vec<NativeMessageReadingIdentity> = frame
                    .get("readingIdentities")
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()?
                    .unwrap_or_default();
                validate_native_reading_identities(&snapshot, &identities)?;
                let session = self.sessions.entry(id.into()).or_default();
                session.run_open = snapshot.is_streaming;
                session.native_reading_identities = identities;
                session.native_conversation = Some(snapshot);
                session.journal_ready = true;
                session.journal_error = None;
                session.native_events.clear();
                session.native_assistant = Some(crate::NativeAssistantProjection::Idle);
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
                let event = frame
                    .get("event")
                    .map(|event| {
                        require(
                            !string(event, "type")?.is_empty(),
                            "empty native event type",
                        )?;
                        Ok::<_, Error>(event.clone())
                    })
                    .transpose()?;
                let identities: Vec<NativeMessageReadingIdentity> = frame
                    .get("readingIdentities")
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()?
                    .unwrap_or_default();
                validate_native_reading_identities(&snapshot, &identities)?;
                let session = self.sessions.entry(id.into()).or_default();
                session.run_open = snapshot.is_streaming;
                session.native_reading_identities = identities;
                session.native_conversation = Some(snapshot);
                if let Some(event) = event {
                    session.push_native_event(event);
                }
                let committed = match &session.native_assistant {
                    Some(crate::NativeAssistantProjection::Message { event, .. }) => {
                        event["agentkitStreamId"].as_str().is_some_and(|stream| {
                            session
                                .native_reading_identities
                                .iter()
                                .any(|identity| identity.stream_id == stream)
                        })
                    }
                    _ => false,
                };
                if !session.run_open || committed {
                    session.native_assistant = Some(crate::NativeAssistantProjection::Idle);
                }
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
                session.push_native_event(event);
            }
            "snapshot" => {
                let snapshot: JournalSnapshot = serde_json::from_value(frame["snapshot"].clone())?;
                require(snapshot.session_id == id, "snapshot session mismatch")?;
                let queue_key = self.sequenced_queue_projection_key.clone();
                self.sessions
                    .entry(id.into())
                    .or_default()
                    .apply_snapshot_with_queue(snapshot, queue_key.as_deref())?;
            }
            "projection-block" => {
                require(
                    string(&frame, "sessionId")? == id,
                    "projection session mismatch",
                )?;
                let block: ProjectionBlock = serde_json::from_value(frame["projections"].clone())?;
                self.sessions
                    .entry(id.into())
                    .or_default()
                    .block(block, self.sequenced_queue_projection_key.as_deref())?;
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
                    let session = self.sessions.entry(id.into()).or_default();
                    let order = session.tail_sequence.checked_add(1).ok_or_else(|| {
                        Error::Protocol("preview presentation order exhausted".into())
                    })?;
                    session.fold_at(&event, order);
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
                self.sessions.entry(id.into()).or_default().block(
                    ProjectionBlock {
                        as_of_seq: seq,
                        values: [(key, value)].into(),
                    },
                    self.sequenced_queue_projection_key.as_deref(),
                )?;
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
    let id = string(value, "sessionId")?;
    require(!id.is_empty(), "empty inventory session identity")?;
    Ok(id)
}

fn validate_queue_projection(value: &Value) -> Result<()> {
    require(
        value.as_object().is_some_and(|object| object.len() == 1) && value["items"].is_array(),
        "invalid sequenced queue projection",
    )
}

#[cfg(test)]
mod queue_authority_tests {
    use super::*;
    use serde_json::json;

    fn snapshot_168(queue_items: &[&str], revision: &str, turn: u64) -> JournalSnapshot {
        JournalSnapshot {
            session_id: "s".into(),
            generation: "g".into(),
            through_sequence: 167,
            window_limit: 168,
            entries: (0..168)
                .map(|seq| HistoryEntry {
                    event: SessionEvent {
                        kind: "user/message".into(),
                        seq,
                        time: 1.0,
                        data: json!({"turn":turn,"step":1}),
                        ignorable: None,
                        metadata: BTreeMap::new(),
                    },
                    view: None,
                    is_model_only: None,
                })
                .collect(),
            has_more_before: false,
            projections: Some(ProjectionBlock {
                as_of_seq: 167,
                values: [(
                    SEQUENCED_QUEUE_PROJECTION_KEY.into(),
                    json!({"items":queue_items}),
                )]
                .into(),
            }),
            history_revision: Some(revision.into()),
        }
    }

    #[test]
    fn journal_snapshot_validates_projection_once_before_mutating_history() {
        let key = SEQUENCED_QUEUE_PROJECTION_KEY;
        let mut session = SessionProjection::default();
        VALIDATE_BLOCK_CALLS.with(|calls| calls.set(0));

        session
            .apply_snapshot_with_queue(snapshot_168(&["original"], "r1", 1), Some(key))
            .unwrap();

        VALIDATE_BLOCK_CALLS.with(|calls| assert_eq!(calls.get(), 1));
        assert_eq!(session.entries.len(), 168);
        assert_eq!(session.entries[&0].event.data, json!({"turn":1,"step":1}));
        assert_eq!(session.queue, vec![json!("original")]);

        VALIDATE_BLOCK_CALLS.with(|calls| calls.set(0));
        assert!(
            session
                .apply_snapshot_with_queue(snapshot_168(&["conflict"], "r2", 2), Some(key))
                .is_err()
        );
        VALIDATE_BLOCK_CALLS.with(|calls| assert_eq!(calls.get(), 1));
        assert_eq!(session.entries.len(), 168);
        assert_eq!(session.entries[&0].event.data, json!({"turn":1,"step":1}));
        assert_eq!(session.queue, vec![json!("original")]);
        assert_eq!(
            session.projections[key].value,
            json!({"items":["original"]})
        );
    }

    #[test]
    fn adapter_switch_never_promotes_retained_opaque_queue_and_same_adapter_reconnect_retains_cut()
    {
        let mut store = MemoryProjectionStore::new("fixed-connector");
        let session = store.sessions.entry("s".into()).or_default();
        session.projections.insert(
            SEQUENCED_QUEUE_PROJECTION_KEY.into(),
            VersionedValue {
                sequence: 99,
                value: json!({"items":["untrusted"]}),
            },
        );
        store.configure_sequenced_queue_projection(Some(SEQUENCED_QUEUE_PROJECTION_KEY));
        assert!(
            !store.sessions["s"]
                .projections
                .contains_key(SEQUENCED_QUEUE_PROJECTION_KEY)
        );
        store
            .sessions
            .get_mut("s")
            .unwrap()
            .block(
                ProjectionBlock {
                    as_of_seq: 20,
                    values: [(
                        SEQUENCED_QUEUE_PROJECTION_KEY.into(),
                        json!({"items":["current"]}),
                    )]
                    .into(),
                },
                Some(SEQUENCED_QUEUE_PROJECTION_KEY),
            )
            .unwrap();
        store.disconnect();
        store.configure_sequenced_queue_projection(Some(SEQUENCED_QUEUE_PROJECTION_KEY));
        assert_eq!(store.sessions["s"].queue, vec![json!("current")]);
        store.configure_sequenced_queue_projection(None);
        assert!(store.sessions["s"].queue.is_empty());
        assert!(
            !store.sessions["s"]
                .projections
                .contains_key(SEQUENCED_QUEUE_PROJECTION_KEY)
        );
    }
}
