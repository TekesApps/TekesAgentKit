//! Read-only Agent message lanes. No text renderer, scroll state or endpoint access.
use crate::{
    TranscriptItem, TranscriptItemKind, TranscriptSnapshot, TranscriptToolCall, TranscriptTurn,
};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require};

/// Identity within the owning RunScope, independent of list position or text.
/// A submission keeps its message identity when an optimistic echo is reconciled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TranscriptMessageKey<'a> {
    Submission(&'a str),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranscriptOutput<'a> {
    pub key: TranscriptMessageKey<'a>,
    /// Includes existing kind, reasoning, reasoning_ended and final/provisional facts.
    pub item: &'a TranscriptItem,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSection<'a> {
    pub id: &'a str,
    /// Missing for clipped/orphan messages. Do not fabricate a turn or settlement.
    pub turn: Option<&'a TranscriptTurn>,
    pub inputs: Vec<TranscriptInput<'a>>,
    pub outputs: Vec<TranscriptOutput<'a>>,
    /// Existing call/result DTOs, in their source order. This separate lane does
    /// not invent chronological interleaving with messages (not present in the DTO).
    pub tools: Vec<&'a TranscriptToolCall>,
}
impl TranscriptSection<'_> {
    /// Exact shared turn facts: actions/usage/live usage are not recalculated.
    pub fn output_footnote(&self) -> Option<&TranscriptTurn> {
        self.turn
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptDocument<'a> {
    pub sections: Vec<TranscriptSection<'a>>,
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
        let mut indices = BTreeMap::new();
        for turn in &snapshot.turns {
            require(!turn.id.is_empty(), "empty Transcript turn identity")?;
            require(
                !indices.contains_key(turn.id.as_str()),
                "duplicate Transcript turn identity",
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
        for item in &snapshot.items {
            require(!item.id.is_empty(), "empty Transcript message identity")?;
            require(
                item_ids.insert(item.id.as_str()),
                "duplicate Transcript message identity",
            )?;
            if !item.is_visible {
                continue;
            }
            let key = if item.kind == TranscriptItemKind::User {
                match item.reconciliation_id.as_deref() {
                    Some(id) => {
                        require(!id.is_empty(), "empty Transcript submission identity")?;
                        TranscriptMessageKey::Submission(id)
                    }
                    None => TranscriptMessageKey::Item(&item.id),
                }
            } else {
                TranscriptMessageKey::Item(&item.id)
            };
            require(keys.insert(key), "ambiguous Transcript message identity")?;
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
