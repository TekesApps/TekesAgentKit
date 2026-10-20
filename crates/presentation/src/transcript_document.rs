//! Read-only Agent message lanes. No text renderer, scroll state or endpoint access.
use crate::{
    TranscriptItem, TranscriptItemKind, TranscriptSnapshot, TranscriptToolCall, TranscriptTurn,
};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require};

/// Key within the owning RunScope. Submission identifies a pending/journal input
/// or an explicit receipt association; NativeEntry survives loaded-window changes.
/// Item retains the existing legacy
/// or temporary timeline key and is not proof of a durable native reading target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptMessageKey<'a> {
    Submission(&'a str),
    /// Public durable NativeConversation entry identity. Native assistant
    /// timeline IDs use window-local timestamp occurrences and are not durable
    /// across prefix expansion. This key never uses entry ordinal or text.
    NativeEntry(&'a str),
    /// Process-local explicit assistant lifecycle identity, never durable authority.
    Stream(&'a str),
    /// Legacy/journal item or temporary native preview. A native preview without
    /// a public entry identity must not be aliased to a durable record by text,
    /// timestamp or a window-local occurrence counter.
    Item(&'a str),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TranscriptDelivery {
    Pending,
    Recorded { time: Option<f64> },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranscriptInput<'a> {
    pub key: TranscriptMessageKey<'a>,
    /// One whole message, including all text blocks and attachments; never split bubbles.
    pub item: &'a TranscriptItem,
    /// Render outside the bubble. Pending inputs never acquire a made-up timestamp.
    pub footnote: TranscriptDelivery,
}
impl<'a> TranscriptInput<'a> {
    /// Explicit shared receipt association for an optimistic Submission key.
    /// A NativeEntry remains canonical even if this receipt is discovered later.
    /// Keep the original RunScope/publication; this is not an authority token.
    pub fn reconciliation_key(&self) -> Option<TranscriptMessageKey<'a>> {
        self.item
            .reconciliation_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(TranscriptMessageKey::Submission)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranscriptOutput<'a> {
    pub key: TranscriptMessageKey<'a>,
    /// Includes existing kind, reasoning, reasoning_ended and final/provisional facts.
    pub item: &'a TranscriptItem,
}
impl<'a> TranscriptOutput<'a> {
    /// Explicit native receipt, not a text/timestamp/occurrence association.
    pub fn reconciliation_key(&self) -> Option<TranscriptMessageKey<'a>> {
        if self.item.kind != TranscriptItemKind::Assistant
            || self.item.is_provisional
            || !matches!(self.key, TranscriptMessageKey::NativeEntry(_))
        {
            return None;
        }
        self.item.metadata["nativeStreamId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(TranscriptMessageKey::Stream)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSection<'a> {
    pub id: &'a str,
    /// Missing for clipped/orphan messages. Do not fabricate a turn or settlement.
    pub turn: Option<&'a TranscriptTurn>,
    pub inputs: Vec<TranscriptInput<'a>>,
    pub outputs: Vec<TranscriptOutput<'a>>,
    /// Existing call/result DTOs. event_order() exposes proven event/container
    /// positions separately; this lane does not imply text-block layout.
    pub tools: Vec<&'a TranscriptToolCall>,
}
impl TranscriptSection<'_> {
    /// Exact shared turn facts: actions/usage/live usage are not recalculated.
    pub fn output_footnote(&self) -> Option<&TranscriptTurn> {
        self.turn
    }
    /// Final response selected by the existing normalized transcript policy.
    /// Ambiguous, clipped, live or hidden answers have no target;
    /// callers must not substitute the last text/tool/reasoning row.
    pub fn final_response(&self) -> Option<TranscriptOutput<'_>> {
        if self.turn?.status != "completed" {
            return None;
        }
        let mut candidates = self.outputs.iter().filter(|output| {
            output.item.kind == TranscriptItemKind::Assistant && output.item.is_final
        });
        let candidate = *candidates.next()?;
        if candidates.next().is_some()
            || !candidate.item.is_visible
            || candidate.item.is_provisional
        {
            return None;
        }
        Some(candidate)
    }
    /// Final prose suitable for copying; an image-only response can still be rated.
    pub fn final_output(&self) -> Option<TranscriptOutput<'_>> {
        self.final_response()
            .filter(|output| !output.item.text.is_empty())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptDocument<'a> {
    pub sections: Vec<TranscriptSection<'a>>,
    /// Session-wide notices have no turn. Keep them visible without inventing a
    /// turn, settlement, input bubble or per-turn footnote. Source order is retained.
    pub session_notices: Vec<TranscriptOutput<'a>>,
    pub has_more_before: bool,
    pub is_stale: bool,
    pub diagnostics: &'a [String],
}
impl<'a> TranscriptDocument<'a> {
    /// Organize an already-scoped canonical snapshot. Callers retain its RunScope
    /// and publication/epoch; this view neither validates authority nor mutates it.
    /// Stable keys are scoped to that authority, never global or row indexes.
    pub fn read(snapshot: &'a TranscriptSnapshot) -> Result<Self> {
        let mut sections = Vec::new();
        let mut session_notices = Vec::new();
        let mut indices = BTreeMap::new();
        for turn in &snapshot.turns {
            require(!turn.id.is_empty(), "empty Transcript turn identity")?;
            require(
                !indices.contains_key(turn.id.as_str()),
                "duplicate Transcript turn identity",
            )?;
            let mut actions = BTreeSet::new();
            require(
                turn.footnote_actions
                    .iter()
                    .all(|action| !action.id.is_empty() && actions.insert(action.id.as_str())),
                "invalid Transcript turn action catalog",
            )?;
            indices.insert(turn.id.as_str(), sections.len());
            sections.push(TranscriptSection {
                id: &turn.id,
                turn: Some(turn),
                inputs: vec![],
                outputs: vec![],
                tools: vec![],
            });
        }
        let mut keys = BTreeSet::new();
        let mut item_ids = BTreeSet::new();
        let mut native_entries = BTreeSet::new();
        let mut submissions = BTreeSet::new();
        let mut streams = BTreeSet::new();
        for item in &snapshot.items {
            require(!item.id.is_empty(), "empty Transcript message identity")?;
            require(
                item_ids.insert(item.id.as_str()),
                "duplicate Transcript message identity",
            )?;
            if !item.is_visible {
                continue;
            }
            if let Some(blocks) = &item.content_blocks {
                require(
                    blocks.windows(2).all(|pair| pair[0].index < pair[1].index),
                    "invalid Transcript content block order",
                )?;
            }
            let stream = item.metadata["nativeStreamId"].as_str();
            if let Some(stream) = stream {
                require(
                    item.kind == TranscriptItemKind::Assistant
                        && !stream.is_empty()
                        && streams.insert(stream),
                    "ambiguous assistant stream identity",
                )?;
            }
            let source_key = match &item.source_position {
                Some(crate::TranscriptSourcePosition::Native { entry_id, .. }) => {
                    require(
                        !entry_id.is_empty(),
                        "empty native Transcript entry identity",
                    )?;
                    require(
                        native_entries.insert(entry_id.as_str()),
                        "duplicate native Transcript entry identity",
                    )?;
                    TranscriptMessageKey::NativeEntry(entry_id)
                }
                _ => {
                    if item.is_provisional
                        && let Some(stream) = stream
                    {
                        TranscriptMessageKey::Stream(stream)
                    } else {
                        TranscriptMessageKey::Item(&item.id)
                    }
                }
            };
            let submission = if item.kind == TranscriptItemKind::User {
                match item.reconciliation_id.as_deref() {
                    Some(id) => {
                        require(!id.is_empty(), "empty Transcript submission identity")?;
                        require(
                            submissions.insert(id),
                            "ambiguous Transcript submission identity",
                        )?;
                        Some(TranscriptMessageKey::Submission(id))
                    }
                    None => None,
                }
            } else {
                None
            };
            let key = match source_key {
                TranscriptMessageKey::NativeEntry(_) => source_key,
                _ => submission.unwrap_or(source_key),
            };
            require(keys.insert(key), "ambiguous Transcript message identity")?;
            if item.kind == TranscriptItemKind::Notice && item.turn_id.is_empty() {
                session_notices.push(TranscriptOutput { key, item });
                continue;
            }
            let section = section(&mut sections, &mut indices, &item.turn_id)?;
            if item.kind == TranscriptItemKind::User {
                section.inputs.push(TranscriptInput {
                    key,
                    item,
                    footnote: if item.is_provisional {
                        TranscriptDelivery::Pending
                    } else {
                        TranscriptDelivery::Recorded { time: item.time }
                    },
                });
            } else {
                section.outputs.push(TranscriptOutput { key, item });
            }
        }
        let mut tool_ids = BTreeSet::new();
        for tool in &snapshot.tools {
            require(!tool.id.is_empty(), "empty Transcript tool identity")?;
            require(
                tool_ids.insert(tool.id.as_str()),
                "duplicate Transcript tool identity",
            )?;
            section(&mut sections, &mut indices, &tool.turn_id)?
                .tools
                .push(tool);
        }
        Ok(Self {
            sections,
            session_notices,
            has_more_before: snapshot.has_more_before,
            is_stale: snapshot.is_stale,
            diagnostics: &snapshot.diagnostics,
        })
    }
}
fn section<'a, 's>(
    sections: &'s mut Vec<TranscriptSection<'a>>,
    indices: &mut BTreeMap<&'a str, usize>,
    id: &'a str,
) -> Result<&'s mut TranscriptSection<'a>> {
    require(
        !id.is_empty(),
        "empty Transcript message/tool turn identity",
    )?;
    let index = *indices.entry(id).or_insert_with(|| {
        let index = sections.len();
        sections.push(TranscriptSection {
            id,
            turn: None,
            inputs: vec![],
            outputs: vec![],
            tools: vec![],
        });
        index
    });
    Ok(&mut sections[index])
}

impl<'a> TranscriptSection<'a> {
    /// Resolve only within this section, never by current selection or a different turn.
    pub fn tool_for_block(
        &self,
        block: &crate::TranscriptContentBlock,
    ) -> Option<&'a TranscriptToolCall> {
        let crate::TranscriptBlockContent::ToolCall { tool_id } = &block.content else {
            return None;
        };
        self.tools.iter().copied().find(|tool| &tool.id == tool_id)
    }
}
