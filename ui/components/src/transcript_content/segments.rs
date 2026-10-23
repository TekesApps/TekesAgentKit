//! Settled history is partitioned by actual input identities, after tool grouping.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptActivitySegmentEnd {
    BeforeInput(TranscriptMessageIdentity),
    Final,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TranscriptActivitySegmentKey {
    pub turn_id: String,
    pub end: TranscriptActivitySegmentEnd,
}
impl TranscriptContent {
    /// Select one installed, nonempty history segment. Scope belongs to this view;
    /// this key is neither a session pointer nor permission to read or mutate it.
    pub fn set_activity_segment_expanded(
        &mut self,
        key: &TranscriptActivitySegmentKey,
        expanded: bool,
    ) -> Result<(), &'static str> {
        if self.turn_planes(&key.turn_id)?.activity
            != crate::TranscriptActivityPresentation::BetweenInputs
            || !self
                .keys
                .contains(&TranscriptContentRowKey::ActivitySegment(key.clone()))
        {
            return Err("activity history segment is not installed");
        }
        if self.segment_expansion.get(key).copied().unwrap_or(false) == expanded {
            return Ok(());
        }
        let generation = self
            .activity_generation
            .checked_add(1)
            .ok_or("activity display generation exhausted")?;
        self.segment_expansion.insert(key.clone(), expanded);
        self.activity_generation = generation;
        self.controls_dirty = true;
        self.arranged_revision = None;
        self.turn_target_owner = std::sync::Arc::new(());
        Ok(())
    }
    pub(super) fn segment_history<'a>(
        &self,
        mut rows: Vec<Planned<'a>>,
        snapshot: &TranscriptSnapshot,
        document: &'a TranscriptDocument<'a>,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<Vec<Planned<'a>>, &'static str> {
        for section in &document.sections {
            let facts = section
                .planes(
                    &self.scope,
                    &snapshot.thinking_sources,
                    timing.live_turn_id.as_deref() == Some(section.id),
                )
                .map_err(|_| "invalid activity history source")?;
            if facts.activity != crate::TranscriptActivityPresentation::BetweenInputs {
                continue;
            }
            let turn = section.turn.ok_or("history segment has no settled turn")?;
            let Some(start) = rows
                .iter()
                .position(|row| planned_turn(row) == Some(section.id))
            else {
                continue;
            };
            let end = rows
                .iter()
                .rposition(|row| planned_turn(row) == Some(section.id))
                .unwrap();
            let block: Vec<_> = rows.drain(start..=end).collect();
            let mut result = vec![];
            let mut pending = vec![];
            let mut final_flushed = false;
            for row in block {
                if planned_turn(&row) != Some(section.id) {
                    return Err("foreign activity history row");
                }
                if matches!(&row.content, PlannedContent::Output(output, _) if initial_prompt(output))
                {
                    result.push(row);
                } else if let PlannedContent::Input(input) = &row.content {
                    self.flush_segment(
                        &mut result,
                        &mut pending,
                        turn,
                        TranscriptActivitySegmentEnd::BeforeInput(input.key.into()),
                    );
                    result.push(row);
                } else if matches!(row.content, PlannedContent::Footnote(..))
                    || (matches!(row.content, PlannedContent::Output(_, Some(_)))
                        && row.activity.is_none())
                {
                    if !final_flushed {
                        self.flush_segment(
                            &mut result,
                            &mut pending,
                            turn,
                            TranscriptActivitySegmentEnd::Final,
                        );
                        final_flushed = true;
                    }
                    result.push(row);
                } else {
                    pending.push(row);
                }
            }
            if !final_flushed {
                self.flush_segment(
                    &mut result,
                    &mut pending,
                    turn,
                    TranscriptActivitySegmentEnd::Final,
                );
            }
            if !pending.is_empty() {
                return Err("activity arrived after final segment");
            }
            rows.splice(start..start, result);
        }
        Ok(rows)
    }
    fn flush_segment<'a>(
        &self,
        result: &mut Vec<Planned<'a>>,
        pending: &mut Vec<Planned<'a>>,
        turn: &'a TranscriptTurn,
        end: TranscriptActivitySegmentEnd,
    ) {
        // Count semantic timeline entries, not text nodes, tool members/results,
        // hidden empty body controllers or the explicit unplaced diagnostic.
        let count = pending.iter().filter(|row| segment_entry(row)).count();
        let final_segment = end == TranscriptActivitySegmentEnd::Final;
        if count > 0 || final_segment {
            let key = TranscriptActivitySegmentKey {
                turn_id: turn.id.clone(),
                end,
            };
            let expanded = self.segment_expansion.get(&key).copied().unwrap_or(false);
            let text = if final_segment {
                (self.labels.activity_settled)(turn)
            } else {
                (self.labels.activity_previous)(count)
            };
            let (key, content) = if count > 0 {
                (
                    TranscriptContentRowKey::ActivitySegment(key),
                    PlannedContent::Disclosure { text, expanded },
                )
            } else {
                (
                    TranscriptContentRowKey::ActivitySegmentSummary(key),
                    PlannedContent::Label(text),
                )
            };
            result.push(Planned {
                key,
                alias: None,
                content,
                thinking: None,
                hidden: false,
                narration_frame: None,
                activity: None,
            });
            if !expanded {
                for row in pending.iter_mut() {
                    if segment_entry(row) {
                        row.hidden = true;
                    }
                }
            }
        }
        result.append(pending);
    }
}

fn segment_entry(row: &Planned<'_>) -> bool {
    if row.hidden || row.activity.is_none() {
        return false;
    }
    if let PlannedContent::Output(output, _) = &row.content {
        return match &output.item.content_blocks {
            Some(blocks) => blocks.iter().any(|block| match &block.content {
                TranscriptBlockContent::Text { text } => !text.is_empty(),
                TranscriptBlockContent::Attachment { .. }
                | TranscriptBlockContent::Unknown { .. } => true,
                _ => false,
            }),
            None => !output.item.text.is_empty() || !output.item.attachments.is_empty(),
        };
    }
    true
}
