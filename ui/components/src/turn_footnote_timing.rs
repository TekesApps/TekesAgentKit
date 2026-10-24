//! Explicit display observations over an existing canonical publication.
use crate::TurnFootnoteLabels;
use tekes_agent_presentation::{ConnectionPhase, RunSnapshot, TranscriptSnapshot, TranscriptTurn};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranscriptFootnoteTiming {
    /// Host display revision, independent of provider publication. Zero is the
    /// legacy no-clock input. Increase when a clock/explicit live owner changes.
    pub revision: u64,
    pub now_ms: Option<f64>,
    pub live_turn_id: Option<String>,
    /// Canonical facts from the same Run publication; not a separate goal store.
    pub goal: Option<tekes_agent_presentation::GoalFacts>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnFootnoteDisplay {
    pub revision: u64,
    pub now_ms: Option<f64>,
    pub is_live_owner: bool,
    /// Formal visible output exists (including image-only output). This does
    /// not grant a copy target, feedback target or execution capability.
    pub has_output: bool,
    /// Host-localized settle time, not an elapsed interval or raw status.
    pub finished_text: Option<String>,
    /// Terminal goal display only; no inferred origin marker or elapsed duration.
    pub goal_state: Option<tekes_agent_presentation::GoalPhase>,
}
fn candidate(snapshot: &TranscriptSnapshot) -> Option<&TranscriptTurn> {
    snapshot.latest_open_turn()
}

impl TranscriptFootnoteTiming {
    /// ClientKit's eligible last open turn policy, using one original Run frame.
    /// A retained running string alone never grants live presentation.
    pub fn for_run(snapshot: &RunSnapshot, revision: u64, now_ms: Option<f64>) -> Self {
        Self {
            goal: snapshot
                .goal
                .as_ref()
                .filter(|goal| {
                    !snapshot.transcript.is_stale
                        && snapshot.connection.phase == ConnectionPhase::Connected
                        && !snapshot.connection.history_is_stale
                        && snapshot.run.as_ref().is_some_and(|run| {
                            run.scope == goal.scope
                                && run.connection.phase == ConnectionPhase::Connected
                                && !run.connection.history_is_stale
                        })
                })
                .cloned(),
            revision,
            now_ms,
            live_turn_id: snapshot
                .live_presentation_turn()
                .map(|turn| turn.id.clone()),
        }
    }
    pub(crate) fn validate(&self, snapshot: &TranscriptSnapshot) -> Result<(), &'static str> {
        if self.now_ms.is_some_and(|n| !n.is_finite() || n <= 0.)
            || (self.revision == 0 && self.now_ms.is_some())
        {
            return Err("invalid footnote display clock");
        }
        if self
            .live_turn_id
            .as_deref()
            .is_some_and(|id| candidate(snapshot).is_none_or(|t| t.id != id))
        {
            return Err("invalid footnote live owner");
        }
        if let Some(goal) = &self.goal {
            goal.validate().map_err(|_| "invalid footnote goal")?;
        }
        Ok(())
    }
    pub(crate) fn display(
        &self,
        turn: &TranscriptTurn,
        labels: &TurnFootnoteLabels,
        has_output: bool,
        goal_turn_id: Option<&str>,
    ) -> TurnFootnoteDisplay {
        let is_live_owner = self.live_turn_id.as_deref() == Some(turn.id.as_str());
        TurnFootnoteDisplay {
            revision: self.revision,
            now_ms: self.now_ms,
            is_live_owner,
            goal_state: self
                .goal
                .as_ref()
                .filter(|_| goal_turn_id == Some(turn.id.as_str()))
                .map(|goal| goal.phase.clone()),
            has_output,
            finished_text: (!is_live_owner)
                .then_some(turn.ended_at)
                .flatten()
                .filter(|end| end.is_finite() && *end > 0.)
                .and_then(|end| (labels.finished_timestamp)(end, self.now_ms)),
        }
    }
}
impl TurnFootnoteDisplay {
    /// One source display gate for all output-aside contents. A validated copy
    /// argument can establish presence for the standalone legacy footer API.
    pub(crate) fn shows_aside(&self, turn: &TranscriptTurn, copy_output_present: bool) -> bool {
        self.is_live_owner
            || self.has_output
            || copy_output_present
            || (!matches!(turn.status.as_str(), "running" | "waitingForInput")
                && !turn.footnote_actions.is_empty())
    }
    pub(crate) fn validate(&self, turn: &TranscriptTurn) -> Result<(), &'static str> {
        if self.now_ms.is_some_and(|n| !n.is_finite() || n <= 0.)
            || (self.revision == 0 && self.now_ms.is_some())
        {
            return Err("invalid footnote display clock");
        }
        if self
            .goal_state
            .as_ref()
            .is_some_and(|phase| !phase.is_terminal())
        {
            return Err("nonterminal footnote goal");
        }
        if self.is_live_owner
            && (turn.status != "running" || turn.ended_at.is_some() || self.finished_text.is_some())
        {
            return Err("invalid footnote live owner");
        }
        if self.finished_text.is_some()
            && turn
                .ended_at
                .is_none_or(|end| !end.is_finite() || end <= 0.)
        {
            return Err("footnote settle label has no source timestamp");
        }
        Ok(())
    }
}
