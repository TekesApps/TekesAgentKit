//! Public source positions, scoped to one projection; never use offsets as identity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TranscriptSourcePosition {
    Journal {
        sequence: i64,
        content_index: Option<usize>,
    },
    /// Ordinal is relative to the current loaded native snapshot. Paging can
    /// shift it; entry_id and message/tool IDs remain the identity authorities.
    Native {
        entry_id: String,
        entry_ordinal: usize,
        content_index: Option<usize>,
    },
}

use crate::{TranscriptItem, TranscriptSection, TranscriptToolCall};
use std::collections::BTreeMap;
use tekes_agent_contract::{Result, require};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TranscriptOrderedEvent<'a> {
    Message(&'a TranscriptItem),
    ToolCall(&'a TranscriptToolCall),
    ToolResult(&'a TranscriptToolCall),
}
impl<'a> TranscriptOrderedEvent<'a> {
    pub fn position(self) -> Option<&'a TranscriptSourcePosition> {
        match self {
            Self::Message(item) => item.source_position.as_ref(),
            Self::ToolCall(tool) => tool.source_position.as_ref(),
            Self::ToolResult(tool) => tool.result_position.as_ref(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptEventOrder<'a> {
    /// Event/container order, not Markdown or text-block layout. Tool calls inside
    /// an assistant message retain their content_index; the message is its container.
    pub ordered: Vec<TranscriptOrderedEvent<'a>>,
    /// Preview/echo/legacy or clipped records with no proven position. Never sort
    /// them by clock, text, phase or a fabricated sequence number.
    pub unplaced: Vec<TranscriptOrderedEvent<'a>>,
}
impl<'a> TranscriptSection<'a> {
    pub fn event_order(&self) -> Result<TranscriptEventOrder<'a>> {
        let mut events: Vec<_> = self
            .inputs
            .iter()
            .map(|row| TranscriptOrderedEvent::Message(row.item))
            .chain(
                self.outputs
                    .iter()
                    .map(|row| TranscriptOrderedEvent::Message(row.item)),
            )
            .collect();
        for tool in &self.tools {
            events.push(TranscriptOrderedEvent::ToolCall(tool));
            if tool.result.is_some() {
                events.push(TranscriptOrderedEvent::ToolResult(tool));
            }
        }
        let mut family = None;
        let mut native_entries = BTreeMap::new();
        let mut ordered = BTreeMap::new();
        let mut unplaced = Vec::new();
        for event in events {
            let Some(position) = event.position() else {
                unplaced.push(event);
                continue;
            };
            let (kind, ordinal, content_index) = match position {
                TranscriptSourcePosition::Journal {
                    sequence,
                    content_index,
                } => {
                    require(*sequence >= 0, "negative Transcript source sequence")?;
                    (0, *sequence as u64, *content_index)
                }
                TranscriptSourcePosition::Native {
                    entry_id,
                    entry_ordinal,
                    content_index,
                } => {
                    require(
                        !entry_id.is_empty(),
                        "empty native Transcript entry identity",
                    )?;
                    if let Some(previous) = native_entries.insert(*entry_ordinal, entry_id) {
                        require(
                            previous == entry_id,
                            "ambiguous native Transcript entry order",
                        )?;
                    }
                    (1, *entry_ordinal as u64, *content_index)
                }
            };
            require(
                family.is_none_or(|previous| previous == kind),
                "mixed Transcript source families",
            )?;
            family = Some(kind);
            require(
                ordered.insert((ordinal, content_index), event).is_none(),
                "ambiguous Transcript event position",
            )?;
        }
        Ok(TranscriptEventOrder {
            ordered: ordered.into_values().collect(),
            unplaced,
        })
    }
}
