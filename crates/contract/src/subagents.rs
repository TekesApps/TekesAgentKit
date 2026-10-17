//! Parent-owned child control. A child ID is never a parent session fallback.
use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentEntry {
    pub kind: String,
    pub id: String,
    pub mode: Option<String>,
    pub plane: Option<String>,
    pub has_children: Option<bool>,
    pub label: Option<String>,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalog {
    pub entries: Vec<SubagentEntry>,
    pub parent_available: bool,
}
impl SubagentCatalog {
    pub fn child(&self, id: &str) -> Option<&SubagentEntry> {
        self.entries
            .iter()
            .find(|e| e.kind == "child" && e.id == id)
    }
    pub fn require_continuable(&self, id: &str) -> Result<()> {
        require(self.parent_available, "subagent parent unavailable")?;
        require(
            self.child(id)
                .is_some_and(|e| e.mode.as_deref() == Some("continuable")),
            "subagent control unavailable",
        )
    }
}

/// Read-only bounded line address. Child endpoint sequences never share the parent's cursor.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundedChildHistoryRequest {
    pub parent_session_id: String,
    pub child_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_sequence: Option<i64>,
    pub max_messages: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_revision: Option<String>,
}
impl BoundedChildHistoryRequest {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.parent_session_id.is_empty()
                && !self.child_session_id.is_empty()
                && self.parent_session_id != self.child_session_id,
            "invalid bounded child address",
        )?;
        require(
            self.max_messages > 0
                && self.max_messages <= 200
                && self.through_sequence.is_none_or(|s| s >= -1)
                && self.before_sequence.is_none_or(|s| s >= 0)
                && (self.before_sequence.is_none() || self.through_sequence.is_some())
                && (self.history_revision.is_none() || self.through_sequence.is_some()),
            "invalid bounded child history cut",
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundedChildHistoryPage {
    pub parent_session_id: String,
    pub child_session_id: String,
    pub through_sequence: i64,
    pub history_revision: String,
    pub entries: Vec<HistoryEntry>,
    pub has_more_before: bool,
    pub run_open: bool,
    pub source_call: Option<Value>,
}
impl BoundedChildHistoryPage {
    pub fn validate(&self, request: &BoundedChildHistoryRequest) -> Result<()> {
        request.validate()?;
        require(
            self.parent_session_id == request.parent_session_id
                && self.child_session_id == request.child_session_id,
            "bounded history address mismatch",
        )?;
        require(
            self.through_sequence >= -1
                && request
                    .through_sequence
                    .is_none_or(|s| s == self.through_sequence)
                && request
                    .history_revision
                    .as_ref()
                    .is_none_or(|r| r == &self.history_revision),
            "bounded history cut/revision mismatch",
        )?;
        require(
            self.history_revision.len() == 64
                && self.history_revision.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid bounded history revision",
        )?;
        validate_entries(&self.entries, self.through_sequence)?;
        require(
            self.entries
                .iter()
                .all(|e| request.before_sequence.is_none_or(|b| e.event.seq < b))
                && (!self.entries.is_empty() || !self.has_more_before),
            "invalid bounded history page",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_pages_bind_owner_cut_revision_and_exclusive_frontier() {
        let mut request = BoundedChildHistoryRequest {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            through_sequence: Some(3),
            before_sequence: Some(3),
            max_messages: 50,
            history_revision: Some("a".repeat(64)),
        };
        let mut page = BoundedChildHistoryPage {
            parent_session_id: "parent".into(),
            child_session_id: "child".into(),
            through_sequence: 3,
            history_revision: "a".repeat(64),
            entries: vec![
                serde_json::from_value(serde_json::json!({"event":{
                "seq":2,"time":1,"type":"turn/start","data":{"turn":1}}}))
                .unwrap(),
            ],
            has_more_before: false,
            run_open: true,
            source_call: None,
        };
        page.validate(&request).unwrap();
        page.child_session_id = "parent".into();
        assert!(page.validate(&request).is_err());
        page.child_session_id = "child".into();
        page.history_revision = "b".repeat(64);
        assert!(page.validate(&request).is_err());
        page.history_revision = "a".repeat(64);
        request.before_sequence = Some(2);
        assert!(page.validate(&request).is_err());
        request.before_sequence = Some(3);
        page.through_sequence = 4;
        assert!(page.validate(&request).is_err());
        request.through_sequence = None;
        assert!(request.validate().is_err());
    }
}
