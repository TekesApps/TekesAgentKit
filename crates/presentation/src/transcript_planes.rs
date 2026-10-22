//! Read-only business planes over canonical messages, never a renderer or owner store.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptPlane {
    Activity,
    Output,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptItemPlanes {
    /// Reasoning always belongs to Activity, including a final assistant record.
    pub has_reasoning: bool,
    pub body: Option<TranscriptPlane>,
    /// None means no body/classification or unavailable response-frame provenance.
    /// The canonical message kind is never rewritten into a guessed narration kind.
    pub is_narration: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptTurnPlanes {
    pub turn_id: String,
    /// Explicit captured presentation ownership, not inferred from a running string.
    pub is_live_owner: bool,
    pub has_activity: bool,
    pub has_output: bool,
    pub items: BTreeMap<String, TranscriptItemPlanes>,
}
impl TranscriptSection<'_> {
    /// Caller supplies live ownership from the same captured Run/display context.
    /// Facts convey no submission, copy or history authority. Missing legacy frame
    /// metadata never reclassifies an assistant from text or timestamps.
    pub fn planes(
        &self,
        scope: &RunScope,
        sources: &TranscriptThinkingSources,
        is_live_owner: bool,
    ) -> Result<TranscriptTurnPlanes> {
        scope.validate()?;
        require(!self.id.is_empty(), "empty transcript plane turn")?;
        require(
            sources.scope.as_ref().is_none_or(|owner| owner == scope),
            "foreign transcript plane sources",
        )?;
        require(
            !is_live_owner
                || self
                    .turn
                    .is_some_and(|turn| turn.status == "running" && turn.ended_at.is_none()),
            "invalid live plane owner",
        )?;
        // A legacy/unowned catalog is not evidence for cross-record correlation.
        let source_for = |id: &str| sources.scope.as_ref().and_then(|_| sources.by_id.get(id));
        let mut tool_frames = BTreeSet::new();
        for tool in &self.tools {
            require(tool.turn_id == self.id, "foreign transcript plane tool")?;
            if let Some(source) = source_for(&tool.id) {
                require(
                    !source.response_frame_id.is_empty(),
                    "empty transcript plane frame",
                )?;
                tool_frames.insert(source.response_frame_id.as_str());
            }
        }
        let mut result = TranscriptTurnPlanes {
            turn_id: self.id.into(),
            is_live_owner,
            has_activity: !self.tools.is_empty(),
            has_output: false,
            items: BTreeMap::new(),
        };
        for output in &self.outputs {
            let row = output.item;
            require(row.turn_id == self.id, "foreign transcript plane message")?;
            let has_reasoning = !row.display_reasoning().trim().is_empty();
            let has_body = match &row.content_blocks {
                Some(blocks) => blocks.iter().any(|block| match &block.content {
                    TranscriptBlockContent::Text { text } => !text.is_empty(),
                    TranscriptBlockContent::Attachment { .. }
                    | TranscriptBlockContent::Unknown { .. } => true,
                    _ => false,
                }),
                None => !row.text.is_empty() || !row.attachments.is_empty(),
            };
            let body = has_body.then_some(
                if row.kind == TranscriptItemKind::Assistant && row.is_final && !row.is_provisional
                {
                    TranscriptPlane::Output
                } else {
                    TranscriptPlane::Activity
                },
            );
            let source = source_for(&row.id);
            if let Some(source) = source {
                require(
                    !source.response_frame_id.is_empty(),
                    "empty transcript plane frame",
                )?;
            }
            let is_narration = if has_body && row.kind == TranscriptItemKind::Assistant {
                if row.is_final {
                    Some(false)
                } else {
                    source.map(|source| tool_frames.contains(source.response_frame_id.as_str()))
                }
            } else {
                None
            };
            result.has_output |= body == Some(TranscriptPlane::Output);
            result.has_activity |=
                has_reasoning
                    || body == Some(TranscriptPlane::Activity)
                    || row.content_blocks.iter().flatten().any(|block| {
                        matches!(&block.content, TranscriptBlockContent::ToolCall { .. })
                    });
            require(
                result
                    .items
                    .insert(
                        row.id.clone(),
                        TranscriptItemPlanes {
                            has_reasoning,
                            body,
                            is_narration,
                        },
                    )
                    .is_none(),
                "duplicate transcript plane message",
            )?;
        }
        Ok(result)
    }
}

impl TranscriptSnapshot {
    /// The existing latest-open-turn rule, independent of UI clocks. A running
    /// string in older history never promotes an earlier turn into this slot.
    pub fn latest_open_turn(&self) -> Option<&TranscriptTurn> {
        if self.is_stale {
            return None;
        }
        self.turns
            .last()
            .filter(|turn| turn.status == "running" && turn.ended_at.is_none())
    }
}
impl RunSnapshot {
    /// Canonical live presentation eligibility shared by status and footnotes.
    /// This is display ownership, never a replacement for mutation admission.
    pub fn live_presentation_turn(&self) -> Option<&TranscriptTurn> {
        let can_present = self.run.as_ref().is_some_and(|run| {
            run.connection.phase == ConnectionPhase::Connected
                && self.connection.phase == ConnectionPhase::Connected
                && !self.connection.history_is_stale
                && matches!(
                    run.phase,
                    RunPhase::Idle | RunPhase::Running | RunPhase::WaitingForInput
                )
                && matches!(
                    self.child_availability,
                    None | Some(ChildComposerAvailability::Available)
                )
                && self.selection.as_ref().is_some_and(|selection| {
                    selection.destination == ComposerDestination::run(run.scope.clone())
                })
                && (run.can_submit || run.can_cancel)
        });
        can_present
            .then(|| self.transcript.latest_open_turn())
            .flatten()
    }
    /// Normal Run consumer entry: no host-derived live owner, new runtime read,
    /// parent publication borrowing or synthetic writable Run is needed.
    pub fn turn_planes(&self, turn_id: &str) -> Result<TranscriptTurnPlanes> {
        let scope = &self
            .run
            .as_ref()
            .ok_or_else(|| tekes_agent_contract::unsupported("plane Run unavailable"))?
            .scope;
        let document = TranscriptDocument::read(&self.transcript)?;
        let section = document
            .sections
            .iter()
            .find(|section| section.id == turn_id)
            .ok_or_else(|| tekes_agent_contract::unsupported("plane turn unavailable"))?;
        section.planes(
            scope,
            &self.transcript.thinking_sources,
            self.live_presentation_turn()
                .is_some_and(|turn| turn.id == turn_id),
        )
    }
}
