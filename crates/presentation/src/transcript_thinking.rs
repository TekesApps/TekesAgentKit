//! Live-thinking display policy, independent of a renderer or preference store.
//! Facts describe source presentation carriers; they never grant session authority.
use crate::RunScope;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TranscriptThinkingMode {
    /// Hide live reasoning prose, retaining only the newest eligible status.
    #[default]
    None,
    /// Keep reasoning disclosures, initially collapsed by the consuming view.
    Compact,
}
impl TranscriptThinkingMode {
    /// ClientKit preference compatibility. Hosts own storage and notifications.
    pub fn from_preference(value: &str) -> Self {
        match value {
            "compact" | "compact-thinking" => Self::Compact,
            _ => Self::None,
        }
    }
    pub fn preference_value(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Compact => "compact",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TranscriptThinkingItem<'a> {
    pub id: &'a str,
    /// First source arrival order, never a timestamp, loaded-window ordinal or
    /// order invented from the current vector. Paging must not reassign it.
    pub arrival_order: i64,
    pub response_frame_id: Option<&'a str>,
    pub is_provisional: bool,
    pub reasoning_has_ended: bool,
    pub has_reasoning: bool,
    /// Source-visible narration/output nodes on this carrier, not text inferred
    /// by the policy from tool output or raw reasoning prose.
    pub has_visible_content: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TranscriptThinkingCall<'a> {
    pub group_id: &'a str,
    pub arrival_order: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TranscriptThinkingNarration<'a> {
    pub group_id: &'a str,
    /// First narration arrival, including a group's explicit narration order.
    pub arrival_order: i64,
}

/// One message's display decision. It conveys no session mutation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptReasoningDisplay {
    Hidden,
    Placeholder { dots: u8 },
    Disclosure { active: bool },
}

/// Owned display-only frame, explicitly bound to the original Run publication.
/// Preference/elapsed changes advance display_revision, not Run authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingFrame {
    scope: RunScope,
    turn_id: String,
    publication: u64,
    epoch: u64,
    display_revision: u64,
    source_fingerprint: String,
    decisions: BTreeMap<String, TranscriptReasoningDisplay>,
}
impl TranscriptThinkingFrame {
    pub fn read(
        scope: &RunScope,
        turn_id: &str,
        publication: u64,
        epoch: u64,
        display_revision: u64,
        input: &TranscriptThinkingInput<'_>,
    ) -> Result<Self> {
        scope.validate()?;
        require(!turn_id.is_empty(), "empty thinking turn identity")?;
        require(
            publication > 0 && display_revision > 0,
            "invalid thinking frame version",
        )?;
        let policy = TranscriptThinkingPolicy::read(input)?;
        // Only mode and projected elapsed duration may vary independently of
        // the Run publication. Do not smuggle new source facts through a setting.
        let source = serde_json::to_vec(&(
            input.is_settled,
            input.is_live_presentation_owner,
            input.has_final_output,
            input.items,
            input.calls,
            input.narrations,
        ))?;
        let source_fingerprint = format!("{:x}", Sha256::digest(source));
        let disclosures: BTreeMap<_, _> = policy
            .disclosures
            .iter()
            .map(|row| (row.item_id, row.active))
            .collect();
        let decisions = input
            .items
            .iter()
            .filter(|item| item.has_reasoning)
            .map(|item| {
                let display = if let Some(&active) = disclosures.get(item.id) {
                    TranscriptReasoningDisplay::Disclosure { active }
                } else if let Some(status) = policy.placeholder.filter(|row| row.item_id == item.id)
                {
                    TranscriptReasoningDisplay::Placeholder { dots: status.dots }
                } else {
                    TranscriptReasoningDisplay::Hidden
                };
                (item.id.to_owned(), display)
            })
            .collect();
        Ok(Self {
            scope: scope.clone(),
            turn_id: turn_id.into(),
            publication,
            epoch,
            display_revision,
            source_fingerprint,
            decisions,
        })
    }
    pub fn matches_source(
        &self,
        scope: &RunScope,
        turn_id: &str,
        publication: u64,
        epoch: u64,
    ) -> bool {
        &self.scope == scope
            && self.turn_id == turn_id
            && self.publication == publication
            && self.epoch == epoch
    }
    pub fn display_revision(&self) -> u64 {
        self.display_revision
    }
    pub fn source_fingerprint(&self) -> &str {
        &self.source_fingerprint
    }
    pub fn display_for(&self, item_id: &str) -> Option<TranscriptReasoningDisplay> {
        self.decisions.get(item_id).copied()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TranscriptThinkingInput<'a> {
    pub mode: TranscriptThinkingMode,
    pub is_settled: bool,
    pub is_live_presentation_owner: bool,
    pub has_final_output: bool,
    /// Host-projected elapsed duration. The policy starts no timer and reads no clock.
    pub elapsed_seconds: u64,
    pub items: &'a [TranscriptThinkingItem<'a>],
    pub calls: &'a [TranscriptThinkingCall<'a>],
    pub narrations: &'a [TranscriptThinkingNarration<'a>],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptReasoningDisclosure<'a> {
    pub item_id: &'a str,
    pub active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingPlaceholder<'a> {
    pub item_id: &'a str,
    /// 1..=3, derived from elapsed duration. This is a passive status, never a
    /// disclosure/copy target for the hidden reasoning body.
    pub dots: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingPolicy<'a> {
    pub disclosures: Vec<TranscriptReasoningDisclosure<'a>>,
    pub placeholder: Option<TranscriptThinkingPlaceholder<'a>>,
}
impl<'a> TranscriptThinkingPolicy<'a> {
    /// Evaluate the current source facts without changing, discarding or copying
    /// retained reasoning. Source carrier grouping and flat timeline order remain
    /// the caller's existing presentation responsibilities.
    pub fn read(input: &TranscriptThinkingInput<'a>) -> Result<Self> {
        let mut ids = BTreeSet::new();
        for item in input.items {
            require(
                !item.id.is_empty() && ids.insert(item.id),
                "invalid thinking item identity",
            )?;
            require(
                item.response_frame_id != Some(""),
                "empty thinking frame identity",
            )?;
        }
        require(
            input.calls.iter().all(|call| !call.group_id.is_empty()),
            "empty thinking call group",
        )?;
        require(
            input.narrations.iter().all(|row| !row.group_id.is_empty()),
            "empty thinking narration group",
        )?;
        let call_frames: BTreeSet<_> = input.calls.iter().map(|call| call.group_id).collect();
        let live = input.is_live_presentation_owner && !input.has_final_output;
        let active = live
            .then(|| {
                input
                    .items
                    .iter()
                    .filter(|item| {
                        item.is_provisional
                            && !item.reasoning_has_ended
                            && item.has_reasoning
                            && !item.has_visible_content
                            && !item
                                .response_frame_id
                                .is_some_and(|frame| call_frames.contains(frame))
                    })
                    .max_by_key(|item| (item.arrival_order, item.id))
                    .map(|item| item.id)
            })
            .flatten();
        let disclosures = if input.mode == TranscriptThinkingMode::Compact
            || (input.is_settled && !input.is_live_presentation_owner)
        {
            input
                .items
                .iter()
                .filter(|item| item.has_reasoning)
                .map(|item| TranscriptReasoningDisclosure {
                    item_id: item.id,
                    active: active == Some(item.id),
                })
                .collect()
        } else {
            vec![]
        };
        let placeholder = if input.mode == TranscriptThinkingMode::None && live {
            input
                .items
                .iter()
                .filter(|item| item.has_reasoning)
                .max_by_key(|item| (item.arrival_order, item.id))
                .filter(|item| {
                    !item.has_visible_content
                        && !input.calls.iter().any(|call| {
                            call.arrival_order >= item.arrival_order
                                || Some(call.group_id) == item.response_frame_id
                        })
                        && !input.items.iter().any(|other| {
                            other.has_visible_content
                                && (other.arrival_order >= item.arrival_order
                                    || (item.response_frame_id.is_some()
                                        && item.response_frame_id == other.response_frame_id))
                        })
                        && !input.narrations.iter().any(|row| {
                            row.arrival_order >= item.arrival_order
                                || Some(row.group_id) == item.response_frame_id
                        })
                })
                .map(|item| TranscriptThinkingPlaceholder {
                    item_id: item.id,
                    dots: (input.elapsed_seconds % 3 + 1) as u8,
                })
        } else {
            None
        };
        Ok(Self {
            disclosures,
            placeholder,
        })
    }
}
