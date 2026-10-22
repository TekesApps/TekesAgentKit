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

/// A flat display timeline derived from public presentation provenance. Unlike
/// event_order(), it can place a transient preview or move a committed response
/// to its frame's first call. It never rewrites protocol positions or identities.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptDisplayOrder<'a> {
    pub ordered: Vec<TranscriptOrderedEvent<'a>>,
    /// No known source or normalized presentation anchor. Remains explicit.
    pub unplaced: Vec<TranscriptOrderedEvent<'a>>,
}
impl<'a> TranscriptSection<'a> {
    pub fn display_order(
        &self,
        scope: &crate::RunScope,
        sources: &crate::TranscriptThinkingSources,
    ) -> Result<TranscriptDisplayOrder<'a>> {
        scope.validate()?;
        // Preserve all original collision/family checks before display sorting.
        let original = self.event_order()?;
        let Some(owner) = &sources.scope else {
            return Ok(TranscriptDisplayOrder {
                ordered: original.ordered,
                unplaced: original.unplaced,
            });
        };
        require(owner == scope, "foreign Transcript presentation sources")?;
        let mut known = vec![];
        let mut unplaced = vec![];
        for (index, event) in original
            .ordered
            .into_iter()
            .chain(original.unplaced)
            .enumerate()
        {
            let source = match event {
                TranscriptOrderedEvent::Message(item)
                    if item.kind == crate::TranscriptItemKind::Assistant =>
                {
                    sources.by_id.get(&item.id)
                }
                TranscriptOrderedEvent::ToolCall(tool) => sources.by_id.get(&tool.id),
                _ => None,
            };
            let order = if let Some(source) = source {
                require(
                    source.arrival_order >= 0,
                    "negative Transcript presentation order",
                )?;
                Some(source.arrival_order)
            } else {
                match event.position() {
                    Some(TranscriptSourcePosition::Journal { sequence, .. }) => Some(*sequence),
                    Some(TranscriptSourcePosition::Native { entry_ordinal, .. }) => {
                        Some(i64::try_from(*entry_ordinal).map_err(|_| {
                            tekes_agent_contract::unsupported("native display order exhausted")
                        })?)
                    }
                    None => None,
                }
            };
            if let Some(order) = order {
                let rank = match event {
                    TranscriptOrderedEvent::Message(item)
                        if item.kind == crate::TranscriptItemKind::User =>
                    {
                        0
                    }
                    TranscriptOrderedEvent::Message(_) => 1,
                    _ => 2,
                };
                known.push((order, rank, index, event));
            } else {
                unplaced.push(event);
            }
        }
        known.sort_by(|left, right| {
            (left.0, left.1).cmp(&(right.0, right.1)).then_with(|| {
                match (left.3, right.3) {
                    (TranscriptOrderedEvent::Message(a), TranscriptOrderedEvent::Message(b)) => {
                        a.id.cmp(&b.id)
                    }
                    // Same-record calls retain proven content/call order rather
                    // than alphabetical tool IDs. Results keep their own sink.
                    _ => left.2.cmp(&right.2),
                }
            })
        });
        Ok(TranscriptDisplayOrder {
            ordered: known.into_iter().map(|(_, _, _, event)| event).collect(),
            unplaced,
        })
    }
}
