//! Source turn-output aggregation for local clipboard display, not response authority.
use std::collections::BTreeMap;
use tekes_agent_presentation::{
    TranscriptDocument, TranscriptItemKind, TranscriptSnapshot, TranscriptSourcePosition,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CopyProvenance {
    record_id: Option<String>,
    position: Option<TranscriptSourcePosition>,
    stream_id: Option<String>,
}

/// An immutable copy of the same turn text ClientKit projects: assistant text
/// joined in snapshot order, separately gated by the presence of a final answer.
/// Empty strings retain their separators. Reasoning/tool fields are not copied.
/// Hidden/provisional facts follow that source projection; they do not gain a
/// display row or feedback/execute permission. Live-owner visibility is separate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptTurnCopy {
    turn_id: String,
    item_ids: Vec<String>,
    pub(crate) provenance: Vec<CopyProvenance>,
    text: String,
}
impl TranscriptTurnCopy {
    /// Read one known turn from the original scoped snapshot. Invalid document
    /// identities or an unknown turn reject; no provider lookup or last-row fallback.
    pub fn read(
        snapshot: &TranscriptSnapshot,
        turn_id: &str,
    ) -> Result<Option<Self>, &'static str> {
        TranscriptDocument::read(snapshot).map_err(|_| "invalid transcript copy document")?;
        if !snapshot.turns.iter().any(|turn| turn.id == turn_id) {
            return Err("unknown transcript copy turn");
        }
        Ok(Self::catalog(snapshot).remove(turn_id))
    }
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
    pub fn item_ids(&self) -> &[String] {
        &self.item_ids
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Caller has validated the original TranscriptDocument. Group once per
    /// publication rather than scanning the full item list for every turn.
    pub(crate) fn catalog(snapshot: &TranscriptSnapshot) -> BTreeMap<String, Self> {
        let mut groups: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        for item in &snapshot.items {
            if item.kind == TranscriptItemKind::Assistant {
                groups.entry(&item.turn_id).or_default().push(item);
            }
        }
        snapshot
            .turns
            .iter()
            .filter_map(|turn| {
                let items = groups.remove(turn.id.as_str())?;
                if !items.iter().any(|item| item.is_final) {
                    return None;
                }
                let text = items
                    .iter()
                    .map(|item| item.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if text.is_empty() {
                    return None;
                }
                Some((
                    turn.id.clone(),
                    Self {
                        turn_id: turn.id.clone(),
                        item_ids: items.iter().map(|item| item.id.clone()).collect(),
                        provenance: items
                            .iter()
                            .map(|item| CopyProvenance {
                                record_id: item.record_id.clone(),
                                position: item.source_position.clone(),
                                stream_id: item.metadata["nativeStreamId"]
                                    .as_str()
                                    .map(str::to_owned),
                            })
                            .collect(),
                        text,
                    },
                ))
            })
            .collect()
    }
}
