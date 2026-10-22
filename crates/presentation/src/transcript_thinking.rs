//! Live-thinking display policy, independent of a renderer or preference store.
//! Facts describe source presentation carriers; they never grant session authority.
use std::collections::BTreeSet;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingCall<'a> {
    pub group_id: &'a str,
    pub arrival_order: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptThinkingNarration<'a> {
    pub group_id: &'a str,
    /// First narration arrival, including a group's explicit narration order.
    pub arrival_order: i64,
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
