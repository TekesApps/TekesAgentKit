//! Journal-owned system prompt cards; model surface and human history differ.
use super::*;
use tekes_agent_contract::HistoryEntry;

#[derive(Clone, Debug, PartialEq)]
struct Node {
    turn: i64,
    step: i64,
    text: String,
    update: bool,
    seq: i64,
}
#[derive(Default)]
pub(super) struct PromptFold {
    first_seq: Option<i64>,
    uncertain: bool,
    nodes: Vec<(i64, Node)>,
    replacements: BTreeMap<i64, i64>,
    effective: Option<Node>,
    last_append: bool,
    turn: Option<i64>,
    step: Option<i64>,
    turn_start: Option<i64>,
    step_start: Option<i64>,
    previous: Option<(String, Option<i64>, Option<i64>)>,
}
impl PromptFold {
    fn anchor(
        &self,
        seq: i64,
        previous: Option<&(String, Option<i64>, Option<i64>)>,
        initial: bool,
    ) -> i64 {
        if self.step.is_none()
            || (previous.is_none() && !initial)
            || previous.is_some_and(|(_, turn, step)| *turn == self.turn && *step == self.step)
        {
            return seq;
        }
        if self.step == Some(1) {
            self.turn_start.or(self.step_start).unwrap_or(seq)
        } else {
            self.step_start.unwrap_or(seq)
        }
    }
    fn location(&mut self, event: &tekes_agent_contract::SessionEvent) {
        let turn = event.data["turn"].as_i64().filter(|v| *v >= 0);
        let step = event.data["step"].as_i64().filter(|v| *v >= 0);
        if event.kind == "turn/start" {
            self.turn = turn;
            self.step = None;
            self.turn_start = Some(event.seq);
            self.step_start = None;
        }
        if event.kind == "step/start" {
            if self.turn != turn {
                self.turn_start = None;
            }
            self.turn = turn;
            self.step = step;
            self.step_start = Some(event.seq);
        }
        if let Some(turn) = turn {
            if self.turn != Some(turn) {
                self.step = None;
                self.turn_start = None;
                self.step_start = None;
            }
            self.turn = Some(turn);
            if let Some(step) = step {
                if self.step != Some(step) {
                    self.step_start = None;
                }
                self.step = Some(step);
            }
        }
        if event.kind == "step/end" && self.turn == turn && self.step == step {
            self.step = None;
            self.step_start = None;
        }
        if event.kind == "turn/end" && self.turn == turn {
            self.turn = None;
            self.step = None;
            self.turn_start = None;
            self.step_start = None;
        }
    }
    pub(super) fn apply(
        &mut self,
        scope: &RunScope,
        entry: &HistoryEntry,
    ) -> Option<TranscriptItem> {
        let event = &entry.event;
        let op = &event
            .metadata
            .get("surfaceOp")
            .cloned()
            .unwrap_or(Value::Null);
        // Unknown ignorable metadata is opaque in the public event contract;
        // its similarly named JSON is not a decoded surface operation.
        let replacement = tekes_agent_contract::SURFACE_EVENTS.contains(&event.kind.as_str())
            && op.get("startSeq").is_some();
        if entry.is_model_only != Some(true) && !replacement {
            self.location(event);
        }
        let mut introduced = None;
        if event.kind == "system/message" || replacement {
            let first = *self.first_seq.get_or_insert(event.seq);
            self.last_append = event.kind == "system/message" && op == "append";
            let before = self
                .nodes
                .iter()
                .map(|(_, node)| node)
                .rev()
                .find(|node| !node.text.is_empty())
                .cloned();
            let mut position = event.seq;
            if replacement {
                let start = op["startSeq"].as_i64().unwrap();
                let end = op["endSeq"].as_i64().unwrap();
                self.uncertain |= (start < first && !self.replacements.contains_key(&start))
                    || (end < first && !self.replacements.contains_key(&end));
                if !self.uncertain {
                    position = self.replacements.get(&start).copied().unwrap_or(start);
                    let end = self.replacements.get(&end).copied().unwrap_or(end);
                    self.nodes.retain(|(p, _)| *p < position || *p > end);
                    self.replacements.retain(|_, p| *p < position || *p > end);
                    self.replacements.insert(event.seq, position);
                }
            }
            if self.uncertain {
                self.nodes.clear();
                self.replacements.clear();
                self.effective = None;
            } else {
                if event.kind == "system/message" {
                    let node = Node {
                        turn: event.data["turn"].as_i64().unwrap_or(0),
                        step: event.data["step"].as_i64().unwrap_or(0),
                        text: text_blocks(&event.data["message"]["content"], "text"),
                        update: self.last_append && before.is_some(),
                        seq: event.seq,
                    };
                    self.nodes.push((position, node.clone()));
                    self.nodes.sort_by_key(|(position, _)| *position);
                    introduced = Some(node);
                }
                let survivor = self
                    .nodes
                    .iter()
                    .map(|(_, node)| node)
                    .rev()
                    .find(|node| !node.text.is_empty())
                    .cloned();
                if survivor != before {
                    self.effective = if survivor == introduced {
                        introduced.clone()
                    } else {
                        Some(Node {
                            turn: survivor.as_ref().map_or(0, |n| n.turn),
                            step: survivor.as_ref().map_or(0, |n| n.step),
                            text: survivor
                                .as_ref()
                                .map_or_else(String::new, |n| n.text.clone()),
                            update: false,
                            seq: event.seq,
                        })
                    };
                }
            }
        }
        // A model rewrite updates the effective prompt but never publishes a
        // second human card or replaces an append-origin transcript record.
        if entry.is_model_only == Some(true) || replacement {
            return None;
        }
        let (text, update, anchor, turn, key) = if event.kind == "system/message" {
            // Visible system cards require the public turn coordinate. A
            // malformed/legacy event cannot invent a visible turn zero.
            event.data["turn"].as_i64()?;
            let node = introduced?;
            let anchor = if node.update {
                event.seq
            } else {
                self.anchor(event.seq, None, true)
            };
            (
                node.text,
                node.update,
                anchor,
                Some(node.turn),
                format!("surface:{}", event.seq),
            )
        } else if event.kind == "request/header" {
            let system = self.effective.as_ref();
            let text = system.map_or_else(String::new, |node| node.text.clone());
            let append_owns = system.is_some_and(|node| {
                self.last_append
                    && (node.update || self.previous.is_none())
                    && Some(node.turn) == self.turn
                    && Some(node.step) == self.step
            });
            let changed = self
                .previous
                .as_ref()
                .is_some_and(|(previous, _, _)| previous != &text)
                && !system.is_some_and(|node| node.update);
            let show = !append_owns
                && (self.previous.is_none()
                    || event.data["reason"] != "change"
                    || event.data["startsSeries"] == true
                    || changed);
            let anchor = self.anchor(
                event.seq,
                self.previous.as_ref(),
                event.data["reason"] == "initial",
            );
            self.previous = Some((text.clone(), self.turn, self.step));
            if !show {
                return None;
            }
            (
                text,
                false,
                anchor,
                self.turn,
                format!("request-prompt:{}", event.seq),
            )
        } else {
            return None;
        };
        if text.is_empty() {
            return None;
        }
        let turn_id = turn
            .map(|n| scoped(scope, &format!("turn:{n}")))
            .unwrap_or_default();
        let mut row = item(
            scope,
            &key,
            &turn_id,
            TranscriptItemKind::System,
            Some(event.seq),
            Some(event.time),
        );
        row.is_final = true;
        row.text = text;
        row.content_blocks = Some(vec![TranscriptContentBlock {
            index: 0,
            content: crate::TranscriptBlockContent::Text {
                text: row.text.clone(),
            },
        }]);
        row.metadata = json!({"systemPrompt":{"placement": if update { "update" } else if self.turn_start == Some(anchor) { "initial" } else { "inline" }, "anchorSequence":anchor}});
        Some(row)
    }
}
