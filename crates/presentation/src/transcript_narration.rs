//! Display-only selection from captured visible node text, never source inference.
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_contract::{Result, require};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptNarrationTimeline {
    Running,
    Settled,
}
#[derive(Clone, Copy, Debug)]
pub struct TranscriptNarrationItem<'a> {
    pub item_id: &'a str,
    pub frame_id: &'a str,
    /// Actual message nodes, excluding reasoning/tools/chrome. Keep boundaries:
    /// ClientKit sums each node's character count when ranking carriers.
    pub node_texts: &'a [String],
    pub canonical_narration: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TranscriptNarrationFallback<'a> {
    pub frame_id: &'a str,
    /// A separately captured group lane; never synthesize it from a sibling.
    pub node_texts: &'a [String],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptNarrationChoice {
    Empty,
    Item { item_id: String },
    GroupFallback { frame_id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptNarrationPolicy {
    choices: BTreeMap<String, TranscriptNarrationChoice>,
}
impl TranscriptNarrationPolicy {
    pub fn read(
        timeline: TranscriptNarrationTimeline,
        items: &[TranscriptNarrationItem<'_>],
        fallbacks: &[TranscriptNarrationFallback<'_>],
    ) -> Result<Self> {
        let mut ids = BTreeSet::new();
        let mut frames = BTreeSet::new();
        let mut frame_spellings = BTreeMap::new();
        for item in items {
            require(
                !item.item_id.is_empty() && !item.frame_id.is_empty(),
                "empty narration identity",
            )?;
            require(
                ids.insert(item.item_id.nfc().collect::<String>()),
                "duplicate narration item",
            )?;
            check_frame(&mut frame_spellings, item.frame_id)?;
            frames.insert(item.frame_id);
        }
        let mut groups = BTreeMap::new();
        for group in fallbacks {
            require(!group.frame_id.is_empty(), "empty narration fallback frame")?;
            require(
                groups.insert(group.frame_id, group.node_texts).is_none(),
                "duplicate narration fallback",
            )?;
            check_frame(&mut frame_spellings, group.frame_id)?;
            frames.insert(group.frame_id);
        }
        let mut choices = BTreeMap::new();
        for frame in frames {
            let carrier = items
                .iter()
                .filter(|item| item.frame_id == frame && !item.node_texts.is_empty())
                .max_by_key(|item| {
                    (
                        item.node_texts
                            .iter()
                            .map(|text| text.graphemes(true).count())
                            .sum::<usize>(),
                        item.canonical_narration,
                        item.item_id.nfc().collect::<String>(),
                    )
                });
            let fallback = groups.get(frame).copied().unwrap_or(&[]);
            let choice = match carrier {
                None if fallback.is_empty() => TranscriptNarrationChoice::Empty,
                None => TranscriptNarrationChoice::GroupFallback {
                    frame_id: frame.into(),
                },
                Some(item) => {
                    // Swift String equality/prefix are canonically equivalent.
                    // Preserve original source bytes; normalize only comparison.
                    let committed: String = item.node_texts.concat().nfc().collect();
                    let cumulative: String = fallback.concat().nfc().collect();
                    if timeline == TranscriptNarrationTimeline::Running
                        && !fallback.is_empty()
                        && cumulative != committed
                        && cumulative
                            .graphemes(true)
                            .take(committed.graphemes(true).count())
                            .eq(committed.graphemes(true))
                    {
                        TranscriptNarrationChoice::GroupFallback {
                            frame_id: frame.into(),
                        }
                    } else {
                        TranscriptNarrationChoice::Item {
                            item_id: item.item_id.into(),
                        }
                    }
                }
            };
            choices.insert(frame.into(), choice);
        }
        Ok(Self { choices })
    }
    pub fn choice_for(&self, frame_id: &str) -> Option<&TranscriptNarrationChoice> {
        self.choices.get(frame_id)
    }
}
fn check_frame(spellings: &mut BTreeMap<String, String>, frame: &str) -> Result<()> {
    let existing = spellings
        .entry(frame.nfc().collect())
        .or_insert_with(|| frame.into());
    require(
        existing.as_str() == frame,
        "canonically ambiguous narration frame",
    )
}
