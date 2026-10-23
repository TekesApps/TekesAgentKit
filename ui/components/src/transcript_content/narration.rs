//! Whole-transcript composition over captured renderer facts and source order.
use super::*;
use crate::TranscriptItem;
use tekes_agent_presentation::{
    TranscriptNarrationChoice, TranscriptNarrationItem, TranscriptNarrationPolicy,
    TranscriptNarrationTimeline,
};

impl TranscriptContent {
    pub(super) fn arrange_narration<'a>(
        &self,
        rows: &mut [Planned<'a>],
        snapshot: &TranscriptSnapshot,
        document: &'a TranscriptDocument<'a>,
        timing: &crate::TranscriptFootnoteTiming,
    ) -> Result<(), &'static str> {
        let mut anchors: BTreeMap<TranscriptMessageIdentity, i64> = BTreeMap::new();
        for section in &document.sections {
            let facts = section
                .planes(
                    &self.scope,
                    &snapshot.thinking_sources,
                    timing.live_turn_id.as_deref() == Some(section.id),
                )
                .map_err(|_| "invalid narration plane source")?;
            let mut candidates = vec![];
            let mut unresolved = BTreeSet::new();
            for output in &section.outputs {
                if facts.items[&output.item.id].is_narration != Some(true) {
                    continue;
                }
                let source = &snapshot.thinking_sources.by_id[&output.item.id];
                let Some(nodes) = OutputMessage::narration_nodes(
                    &self.scope,
                    output,
                    section,
                    self.labels.output,
                )?
                else {
                    unresolved.insert(source.response_frame_id.clone());
                    continue;
                };
                candidates.push((output, source, nodes));
            }
            let inputs: Vec<_> = candidates
                .iter()
                .filter(|(_, source, _)| !unresolved.contains(&source.response_frame_id))
                .map(|(output, source, nodes)| TranscriptNarrationItem {
                    item_id: &output.item.id,
                    frame_id: &source.response_frame_id,
                    node_texts: nodes,
                    canonical_narration: false,
                })
                .collect();
            let timeline = if matches!(
                facts.activity,
                crate::TranscriptActivityPresentation::Settled { .. }
            ) {
                TranscriptNarrationTimeline::Settled
            } else {
                TranscriptNarrationTimeline::Running
            };
            // Current ClientKit journal producer intentionally supplies no copy
            // in the group lane. Do not invent a fallback from a sibling body.
            let policy = TranscriptNarrationPolicy::read(timeline, &inputs, &[])
                .map_err(|_| "invalid narration node source")?;
            for input in &inputs {
                let Some(TranscriptNarrationChoice::Item { item_id }) =
                    policy.choice_for(input.frame_id)
                else {
                    continue;
                };
                let earliest = candidates
                    .iter()
                    .filter(|(_, source, _)| source.response_frame_id == input.frame_id)
                    .map(|(_, source, _)| source.arrival_order)
                    .min()
                    .unwrap();
                for row in rows.iter_mut() {
                    if let PlannedContent::Output(output, Some(_)) = &row.content
                        && output.item.id == input.item_id
                    {
                        if output.item.id != *item_id {
                            row.hidden = true;
                        } else {
                            anchors.insert(output.key.into(), earliest);
                        }
                    }
                }
            }
        }
        // Split reasoning/text/tools are semantic entries. At a shared sink
        // the source order is reasoning, prose, calls, regardless of which
        // original message owns the selected reasoning and narration carriers.
        let owner_order: BTreeMap<TranscriptMessageIdentity, i64> = rows
            .iter()
            .filter_map(|row| match &row.content {
                PlannedContent::Output(output, _) => {
                    message_order(output.item, &snapshot.thinking_sources)
                        .map(|order| (output.key.into(), order))
                }
                _ => None,
            })
            .collect();
        let order = |row: &Planned<'_>| -> Option<(i64, u8)> {
            match &row.content {
                PlannedContent::Input(input) => {
                    raw_order(input.item.source_position.as_ref()).map(|order| (order, 1))
                }
                PlannedContent::Output(output, _) => anchors
                    .get(&TranscriptMessageIdentity::from(output.key))
                    .copied()
                    .or_else(|| message_order(output.item, &snapshot.thinking_sources))
                    .map(|order| (order, 1)),
                PlannedContent::Fragment(owner, kind, _) => {
                    owner_order.get(owner).copied().map(|order| {
                        (
                            order,
                            if *kind == OutputMessageFragmentKind::Reasoning {
                                0
                            } else {
                                2
                            },
                        )
                    })
                }
                PlannedContent::Tools(tools) => tools
                    .first()
                    .and_then(|(tool, _)| {
                        snapshot
                            .thinking_sources
                            .scope
                            .as_ref()
                            .and_then(|_| snapshot.thinking_sources.by_id.get(&tool.id))
                            .map(|source| source.arrival_order)
                            .or_else(|| {
                                raw_order(
                                    tool.source_position
                                        .as_ref()
                                        .or(tool.result_position.as_ref()),
                                )
                            })
                    })
                    .map(|order| (order, 2)),
                _ => None,
            }
        };
        let mut start = 0;
        while start < rows.len() {
            if order(&rows[start]).is_none() {
                start += 1;
                continue;
            }
            let turn = planned_turn(&rows[start]).map(str::to_owned);
            let mut end = start + 1;
            while end < rows.len()
                && order(&rows[end]).is_some()
                && planned_turn(&rows[end]) == turn.as_deref()
            {
                end += 1;
            }
            rows[start..end].sort_by_key(|row| order(row).unwrap());
            start = end;
        }
        Ok(())
    }
}
fn message_order(item: &TranscriptItem, sources: &crate::TranscriptThinkingSources) -> Option<i64> {
    item.system_prompt()
        .map(|source| source.anchor_sequence)
        .or_else(|| {
            sources
                .scope
                .as_ref()
                .filter(|_| item.kind == TranscriptItemKind::Assistant)
                .and_then(|_| sources.by_id.get(&item.id))
                .map(|source| source.arrival_order)
        })
        .or_else(|| raw_order(item.source_position.as_ref()))
}
fn raw_order(position: Option<&crate::TranscriptSourcePosition>) -> Option<i64> {
    match position? {
        crate::TranscriptSourcePosition::Journal { sequence, .. } => Some(*sequence),
        crate::TranscriptSourcePosition::Native { entry_ordinal, .. } => {
            i64::try_from(*entry_ordinal).ok()
        }
    }
}
