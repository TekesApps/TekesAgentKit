//! Dialect-owned presentation provenance. No clock, UI row index or synthetic first-seen ledger.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require, unsupported};
use tekes_agent_session::SessionProjection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptThinkingSource {
    /// Publication-local dialect order, never a persistent identity.
    pub arrival_order: i64,
    pub response_frame_id: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptThinkingSources {
    pub scope: Option<RunScope>,
    pub live_turn_id: Option<String>,
    pub by_id: BTreeMap<String, TranscriptThinkingSource>,
}
impl TranscriptThinkingSources {
    pub fn is_empty(&self) -> bool {
        self.scope.is_none() && self.live_turn_id.is_none() && self.by_id.is_empty()
    }
}
fn put(sources: &mut TranscriptThinkingSources, id: String, order: i64, frame: String) {
    sources.by_id.insert(
        id,
        TranscriptThinkingSource {
            arrival_order: order,
            response_frame_id: frame,
        },
    );
}
pub(crate) fn read(
    scope: &RunScope,
    session: &SessionProjection,
    transcript: &TranscriptSnapshot,
) -> Result<TranscriptThinkingSources> {
    let mut sources = TranscriptThinkingSources {
        scope: Some(scope.clone()),
        ..Default::default()
    };
    if let Some(native) = &session.native_conversation {
        let records: BTreeMap<_, _> = transcript
            .items
            .iter()
            .filter(|row| row.kind == TranscriptItemKind::Assistant)
            .filter_map(|row| row.record_id.as_deref().map(|record| (record, row)))
            .collect();
        for (index, entry) in native.entries.iter().enumerate() {
            let Some(record) = entry["id"].as_str() else {
                continue;
            };
            if entry["message"]["role"] != "assistant" {
                continue;
            }
            let Some(row) = records.get(record) else {
                continue;
            };
            let order = i64::try_from(index)
                .map_err(|_| unsupported("native presentation order exhausted"))?;
            let frame = crate::transcript::scoped(scope, &format!("native-frame:{record}"));
            put(&mut sources, row.id.clone(), order, frame.clone());
            if let Some(blocks) = &row.content_blocks {
                for block in blocks {
                    if let TranscriptBlockContent::ToolCall { tool_id } = &block.content {
                        put(&mut sources, tool_id.clone(), order, frame.clone());
                    }
                }
            }
        }
        // Pi's streaming presentation follows the current public ancestry.
        // This is not an index into the bounded native-event transport buffer.
        let order = i64::try_from(native.entries.len())
            .map_err(|_| unsupported("native presentation order exhausted"))?;
        for row in transcript
            .items
            .iter()
            .filter(|row| row.kind == TranscriptItemKind::Assistant && row.is_provisional)
        {
            let frame = row.metadata["nativeStreamId"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    crate::transcript::scoped(scope, &format!("native-preview:{}", row.id))
                });
            put(&mut sources, row.id.clone(), order, frame.clone());
            if let Some(blocks) = &row.content_blocks {
                for block in blocks {
                    if let TranscriptBlockContent::ToolCall { tool_id } = &block.content {
                        put(&mut sources, tool_id.clone(), order, frame.clone());
                    }
                }
            }
        }
    } else {
        let mut first_calls = BTreeMap::<String, i64>::new();
        for entry in session.entries.values() {
            let event = &entry.event;
            let (Some(turn), Some(step)) =
                (event.data["turn"].as_i64(), event.data["step"].as_i64())
            else {
                continue;
            };
            let frame = crate::transcript::scoped(scope, &format!("response:{turn}:{step}"));
            if event.kind == "assistant/message" {
                put(&mut sources, frame.clone(), event.seq, frame.clone());
            }
            let calls: Vec<_> = if event.kind == "tool/call" {
                event.data["callId"].as_str().into_iter().collect()
            } else if event.kind == "assistant/message" {
                event.data["message"]["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|block| block["type"] == "tool-call")
                    .filter_map(|block| block["id"].as_str())
                    .collect()
            } else {
                vec![]
            };
            for call in calls.into_iter().filter(|call| !call.is_empty()) {
                first_calls
                    .entry(frame.clone())
                    .and_modify(|seq| *seq = (*seq).min(event.seq))
                    .or_insert(event.seq);
                let id = crate::transcript::scoped(scope, &format!("tool:{call}"));
                if sources
                    .by_id
                    .get(&id)
                    .is_none_or(|old| old.response_frame_id != frame)
                {
                    put(&mut sources, id, event.seq, frame.clone());
                }
            }
        }
        for row in transcript
            .items
            .iter()
            .filter(|row| row.kind == TranscriptItemKind::Assistant && !row.is_provisional)
        {
            if let Some(source) = sources.by_id.get_mut(&row.id)
                && let Some(first) = first_calls.get(&source.response_frame_id)
            {
                source.arrival_order = source.arrival_order.min(*first);
            }
        }
        for (key, preview) in &session.previews {
            let Some(order) = preview.presentation_order else {
                continue;
            };
            let Some((turn, step)) = key.split_once(':') else {
                continue;
            };
            let frame = crate::transcript::scoped(scope, &format!("response:{turn}:{step}"));
            if transcript
                .items
                .iter()
                .any(|row| row.id == frame && row.is_provisional)
            {
                put(&mut sources, frame.clone(), order, frame);
            }
        }
    }
    if session.journal_ready && session.run_open {
        sources.live_turn_id = transcript
            .turns
            .iter()
            .rev()
            .find(|turn| turn.status == "running")
            .map(|turn| turn.id.clone())
            .or_else(|| {
                transcript
                    .items
                    .iter()
                    .rev()
                    .find(|row| row.is_provisional && row.kind == TranscriptItemKind::Assistant)
                    .map(|row| row.turn_id.clone())
            });
    }
    let mut loaded: BTreeSet<&str> = transcript
        .items
        .iter()
        .map(|row| row.id.as_str())
        .chain(transcript.tools.iter().map(|tool| tool.id.as_str()))
        .collect();
    for row in &transcript.items {
        for block in row.content_blocks.iter().flatten() {
            if let TranscriptBlockContent::ToolCall { tool_id } = &block.content {
                loaded.insert(tool_id);
            }
        }
    }
    sources.by_id.retain(|id, _| loaded.contains(id.as_str()));
    Ok(sources)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingDisplayInput {
    pub mode: TranscriptThinkingMode,
    pub display_revision: u64,
    pub elapsed_seconds: u64,
}
impl TranscriptSnapshot {
    /// Explicit captured context also supports read-only child publications;
    /// constructing a writable/main Run is neither needed nor permitted.
    pub fn thinking_frame(
        &self,
        scope: &RunScope,
        turn_id: &str,
        publication: u64,
        epoch: u64,
        input: TranscriptThinkingDisplayInput,
    ) -> Result<TranscriptThinkingFrame> {
        require(
            self.thinking_sources.scope.as_ref() == Some(scope),
            "thinking source scope unavailable",
        )?;
        let document = TranscriptDocument::read(self)?;
        let section = document
            .sections
            .iter()
            .find(|section| section.id == turn_id)
            .ok_or_else(|| unsupported("thinking turn unavailable"))?;
        let live = !self.is_stale && self.thinking_sources.live_turn_id.as_deref() == Some(turn_id);
        let mut items = vec![];
        let mut call_ids: BTreeSet<_> = section.tools.iter().map(|tool| tool.id.as_str()).collect();
        for row in &section.outputs {
            if row.item.kind != TranscriptItemKind::Assistant {
                continue;
            }
            let has_reasoning = !row.item.reasoning.is_empty() || row.item.content_blocks.iter().flatten().any(|block| matches!(&block.content, TranscriptBlockContent::Reasoning { text } if !text.is_empty()));
            let visible = !row.item.text.is_empty() || !row.item.attachments.is_empty()
                || row.item.content_blocks.iter().flatten().any(|block| matches!(&block.content, TranscriptBlockContent::Text { text } if !text.is_empty()) || matches!(&block.content, TranscriptBlockContent::Attachment { .. } | TranscriptBlockContent::Unknown { .. }));
            for block in row.item.content_blocks.iter().flatten() {
                if let TranscriptBlockContent::ToolCall { tool_id } = &block.content {
                    call_ids.insert(tool_id);
                }
            }
            if !has_reasoning && !visible {
                continue;
            }
            let source = self
                .thinking_sources
                .by_id
                .get(&row.item.id)
                .ok_or_else(|| unsupported("thinking message provenance unavailable"))?;
            items.push(TranscriptThinkingItem {
                id: &row.item.id,
                arrival_order: source.arrival_order,
                response_frame_id: Some(&source.response_frame_id),
                is_provisional: row.item.is_provisional,
                reasoning_has_ended: row.item.reasoning_ended,
                has_reasoning,
                has_visible_content: visible,
            });
        }
        let mut calls = vec![];
        for id in call_ids {
            if let Some(source) = self.thinking_sources.by_id.get(id) {
                calls.push(TranscriptThinkingCall {
                    group_id: &source.response_frame_id,
                    arrival_order: source.arrival_order,
                });
            } else if live {
                return Err(unsupported("live thinking call provenance unavailable"));
            }
        }
        TranscriptThinkingFrame::read(
            scope,
            turn_id,
            publication,
            epoch,
            input.display_revision,
            &TranscriptThinkingInput {
                mode: input.mode,
                is_settled: section.turn.is_some_and(|turn| turn.status != "running"),
                is_live_presentation_owner: live,
                has_final_output: section.outputs.iter().any(|row| {
                    row.item.is_final && row.item.is_visible && !row.item.is_provisional
                }),
                elapsed_seconds: input.elapsed_seconds,
                items: &items,
                calls: &calls,
                narrations: &[],
            },
        )
    }
}

impl RunSnapshot {
    /// Consume source and authority from this one immutable Run snapshot.
    pub fn thinking_frame(
        &self,
        turn_id: &str,
        mode: TranscriptThinkingMode,
        display_revision: u64,
        elapsed_seconds: u64,
    ) -> Result<TranscriptThinkingFrame> {
        let scope = &self
            .run
            .as_ref()
            .ok_or_else(|| unsupported("thinking Run unavailable"))?
            .scope;
        self.transcript.thinking_frame(
            scope,
            turn_id,
            self.revision,
            self.connection_epoch,
            TranscriptThinkingDisplayInput {
                mode,
                display_revision,
                elapsed_seconds,
            },
        )
    }
}
