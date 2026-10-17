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
