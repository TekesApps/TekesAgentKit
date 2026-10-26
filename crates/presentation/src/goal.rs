//! Read-only facts from the public goal projection, never a goal driver.
use crate::{RunScope, TranscriptItemKind, TranscriptSnapshot};
use serde::{Deserialize, Serialize};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_session::MemoryProjectionStore;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "String", into = "String")]
pub enum GoalPhase {
    Active,
    Paused,
    Blocked,
    Complete,
    /// Future wire phases are preserved verbatim, without terminal placement.
    Other(String),
}
impl GoalPhase {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Paused | Self::Blocked | Self::Complete)
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
            Self::Other(value) => value,
        }
    }
}
impl From<String> for GoalPhase {
    fn from(value: String) -> Self {
        match value.as_str() {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "blocked" => Self::Blocked,
            "complete" => Self::Complete,
            _ => Self::Other(value),
        }
    }
}
impl From<GoalPhase> for String {
    fn from(value: GoalPhase) -> Self {
        value.as_str().into()
    }
}
/// Scope and CAS identity come from one canonical projection publication.
/// Current SEP goal projections do not supply source/completed turn IDs or
/// active elapsed duration. Do not synthesize those fields from text or clocks.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GoalFacts {
    pub scope: RunScope,
    pub sequence: i64,
    pub id: String,
    pub revision: u64,
    pub phase: GoalPhase,
}
impl GoalFacts {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            !self.id.trim().is_empty()
                && self.revision > 0
                && !self.phase.as_str().trim().is_empty(),
            "invalid goal identity",
        )
    }
    /// Pi publishes a flat GoalView; DSH publishes the public folded { goal }.
    /// A null projection/fold is authoritative absence. No private file reads.
    pub fn read(store: &MemoryProjectionStore, scope: &RunScope) -> Result<Option<Self>> {
        require(
            store.endpoint_id == scope.endpoint_id,
            "goal endpoint mismatch",
        )?;
        let Some(value) = store
            .sessions
            .get(&scope.session_id)
            .and_then(|s| s.projections.get("goal"))
        else {
            return Ok(None);
        };
        let raw = value.value.get("goal").unwrap_or(&value.value);
        if raw.is_null() {
            return Ok(None);
        }
        // Match ClientKit's public goal projection prerequisites, even though
        // the footer only consumes identity/phase. A partial object is not a
        // valid authoritative GoalView/fold merely because it names a phase.
        require(
            raw["objective"].as_str().is_some(),
            "invalid goal objective",
        )?;
        let created = if value.value.get("goal").is_some() {
            &value.value["createdAt"]
        } else {
            &raw["createdAt"]
        };
        require(
            created.as_f64().is_some_and(f64::is_finite),
            "invalid goal createdAt",
        )?;
        let facts = Self {
            scope: scope.clone(),
            sequence: value.sequence,
            id: raw["id"]
                .as_str()
                .ok_or_else(|| unsupported("invalid goal id"))?
                .into(),
            revision: raw["revision"]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or_else(|| unsupported("invalid goal revision"))?,
            phase: raw["phase"]
                .as_str()
                .ok_or_else(|| unsupported("invalid goal phase"))?
                .to_owned()
                .into(),
        };
        facts.validate()?;
        Ok(Some(facts))
    }
    /// ClientKit's terminal-goal fallback when no completed-turn ID exists:
    /// the last canonical turn with a durable visible final assistant answer.
    /// This is display placement only, never proof that its input created a goal.
    pub fn terminal_turn_id<'a>(&self, transcript: &'a TranscriptSnapshot) -> Option<&'a str> {
        if !self.phase.is_terminal() || transcript.is_stale {
            return None;
        }
        let finals: std::collections::BTreeSet<_> = transcript
            .items
            .iter()
            .filter(|item| {
                item.kind == TranscriptItemKind::Assistant
                    && item.is_final
                    && item.is_visible
                    && !item.is_provisional
            })
            .map(|item| item.turn_id.as_str())
            .collect();
        transcript
            .turns
            .iter()
            .rev()
            .find(|turn| finals.contains(turn.id.as_str()))
            .map(|turn| turn.id.as_str())
    }
}
