//! Explicit display observations over an existing canonical publication.
use crate::TurnFootnoteLabels;
use tekes_agent_presentation::{
    ChildComposerAvailability, ConnectionPhase, RunPhase, RunSnapshot, TranscriptSnapshot,
    TranscriptTurn,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranscriptFootnoteTiming {
    /// Host display revision, independent of provider publication. Zero is the
    /// legacy no-clock input. Increase when a clock/explicit live owner changes.
    pub revision: u64,
    pub now_ms: Option<f64>,
    pub live_turn_id: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnFootnoteDisplay {
    pub revision: u64,
    pub now_ms: Option<f64>,
    pub is_live_owner: bool,
    /// Host-localized settle time, not an elapsed interval or raw status.
    pub finished_text: Option<String>,
}
fn candidate(snapshot: &TranscriptSnapshot) -> Option<&TranscriptTurn> {
    if snapshot.is_stale {
        return None;
    }
    snapshot.turns.iter().rev().find(|turn| {
        turn.status == "running"
            && turn.ended_at.is_none()
            && !snapshot.items.iter().any(|item| {
                item.turn_id == turn.id
                    && item.is_final
                    && item.is_visible
                    && !item.is_provisional
                    && item.kind == tekes_agent_presentation::TranscriptItemKind::Assistant
            })
    })
}
impl TranscriptFootnoteTiming {
    /// ClientKit's eligible last open turn policy, using one original Run frame.
    /// A retained running string alone never grants live presentation.
    pub fn for_run(snapshot: &RunSnapshot, revision: u64, now_ms: Option<f64>) -> Self {
        let can_present = snapshot.run.as_ref().is_some_and(|run| {
            run.connection.phase == ConnectionPhase::Connected
                && snapshot.connection.phase == ConnectionPhase::Connected
                && !snapshot.connection.history_is_stale
                && matches!(
                    run.phase,
                    RunPhase::Idle | RunPhase::Running | RunPhase::WaitingForInput
                )
                && matches!(
                    snapshot.child_availability,
                    None | Some(ChildComposerAvailability::Available)
                )
                && snapshot.selection.as_ref().is_some_and(|selection| {
                    selection.destination
                        == tekes_agent_presentation::ComposerDestination::run(run.scope.clone())
                })
                && (run.can_submit || run.can_cancel)
        });
        Self {
            revision,
            now_ms,
            live_turn_id: can_present
                .then(|| candidate(&snapshot.transcript))
                .flatten()
                .map(|t| t.id.clone()),
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
        Ok(())
    }
    pub(crate) fn display(
        &self,
        turn: &TranscriptTurn,
        labels: &TurnFootnoteLabels,
    ) -> TurnFootnoteDisplay {
        let is_live_owner = self.live_turn_id.as_deref() == Some(turn.id.as_str());
        TurnFootnoteDisplay {
            revision: self.revision,
            now_ms: self.now_ms,
            is_live_owner,
            finished_text: (!is_live_owner)
                .then_some(turn.ended_at)
                .flatten()
                .filter(|end| end.is_finite() && *end > 0.)
                .and_then(|end| (labels.finished_timestamp)(end, self.now_ms)),
        }
    }
}
impl TurnFootnoteDisplay {
    pub(crate) fn validate(&self, turn: &TranscriptTurn) -> Result<(), &'static str> {
        if self.now_ms.is_some_and(|n| !n.is_finite() || n <= 0.)
            || (self.revision == 0 && self.now_ms.is_some())
        {
            return Err("invalid footnote display clock");
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
