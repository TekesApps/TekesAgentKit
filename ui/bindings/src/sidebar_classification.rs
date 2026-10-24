//! Read-only facts carried by a canonical session summary.
//!
//! This deliberately does not infer whether a session belongs in a Sidebar.
//! Missing and unrecognized values stay distinguishable so callers can apply
//! compatibility policy without treating uncertainty as a confirmed fact.

use serde_json::Value;
use std::collections::BTreeMap;
use tekes_agent_contract::SessionAddress;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarClassificationFact<T> {
    Missing,
    Known(T),
    Unknown(Value),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarClassificationOrigin {
    Fork,
    Subagent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarClassificationFacts {
    pub origin: SidebarClassificationFact<SidebarClassificationOrigin>,
    pub parent_session_id: SidebarClassificationFact<String>,
    pub ephemeral: SidebarClassificationFact<bool>,
}

/// Classification facts and the original Sidebar membership result captured
/// from one canonical inventory item. `included` is descriptive metadata; it
/// grants no navigation or mutation authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarClassificationRecord {
    pub facts: SidebarClassificationFacts,
    pub included: bool,
}

/// Complete classification snapshot from one successful inventory frame.
/// Its source metadata deliberately remains unchanged when an observer later
/// republishes retained rows during restart or failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarClassificationPublication {
    pub endpoint_id: String,
    pub source_revision: u64,
    pub connection_epoch: u64,
    pub observer_generation: u64,
    pub inventory_ready: bool,
    /// Raw, same-store archive facts; absence is a legacy/unknown source.
    pub archive_facts: Option<Value>,
    pub records: BTreeMap<SessionAddress, SidebarClassificationRecord>,
}

impl SidebarClassificationFacts {
    /// Read the three classification fields from a canonical summary object.
    ///
    /// Only the currently established `fork` and `subagent` origin values are
    /// typed as known. Other strings, nulls, and values of the wrong type are
    /// retained verbatim as `Unknown`.
    pub fn from_summary(summary: &Value) -> Result<Self, &'static str> {
        let Some(summary) = summary.as_object() else {
            return Err("session summary must be an object");
        };

        let origin = match summary.get("origin") {
            None => SidebarClassificationFact::Missing,
            Some(Value::String(value)) if value == "fork" => {
                SidebarClassificationFact::Known(SidebarClassificationOrigin::Fork)
            }
            Some(Value::String(value)) if value == "subagent" => {
                SidebarClassificationFact::Known(SidebarClassificationOrigin::Subagent)
            }
            Some(value) => SidebarClassificationFact::Unknown(value.clone()),
        };

        let parent_session_id = match summary.get("parentSessionId") {
            None => SidebarClassificationFact::Missing,
            Some(Value::String(value)) if !value.trim().is_empty() => {
                SidebarClassificationFact::Known(value.clone())
            }
            Some(value) => SidebarClassificationFact::Unknown(value.clone()),
        };

        let ephemeral = match summary.get("ephemeral") {
            None => SidebarClassificationFact::Missing,
            Some(Value::Bool(value)) => SidebarClassificationFact::Known(*value),
            Some(value) => SidebarClassificationFact::Unknown(value.clone()),
        };

        Ok(Self {
            origin,
            parent_session_id,
            ephemeral,
        })
    }
}
