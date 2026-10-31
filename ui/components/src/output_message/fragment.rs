//! Disjoint owners of original message nodes in the Transcript's flat reading tree.
use super::*;
/// A message-local original call identity. Duplicate references retain their
/// actual source block index; a sole call survives block-position changes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutputMessageToolFragmentKey {
    pub tool_id: String,
    pub block_index: Option<usize>,
}
fn tool_key(facts: &[PartFacts], first: &PartFacts) -> OutputMessageToolFragmentKey {
    let Slot::Tool(index, id) = &first.slot else {
        unreachable!("tool source slot");
    };
    OutputMessageToolFragmentKey {
        tool_id: id.clone(),
        block_index: (facts
            .iter()
            .filter(|part| matches!(&part.slot, Slot::Tool(_, other) if other == id))
            .count()
            > 1)
        .then_some(*index),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputMessageFragmentKind {
    Reasoning,
    Tools,
}
impl OutputMessageFragmentKind {
    pub(crate) fn offset(self) -> u64 {
        match self {
            Self::Reasoning => 1,
            Self::Tools => 2,
        }
    }
}
#[derive(Debug)]
pub struct OutputMessageFragment {
    id: ComponentId,
    owner: ComponentId,
    reveal_arranged: bool,
    tool_key: Option<OutputMessageToolFragmentKey>,
    scope: RunScope,
    kind: OutputMessageFragmentKind,
    labels: OutputMessageLabels,
    rows: StackContent,
}
fn kind(node: &Content) -> Option<OutputMessageFragmentKind> {
    if node.component::<crate::ToolRun>().is_some() {
        return Some(OutputMessageFragmentKind::Tools);
    }
    let facts = &node
        .component::<OutputPart>()
        .expect("original output part")
        .facts;
    if facts.kind == OutputContentKind::Tool {
        Some(OutputMessageFragmentKind::Tools)
    } else if facts.kind == OutputContentKind::Reasoning || facts.slot == Slot::ThinkingStatus {
        Some(OutputMessageFragmentKind::Reasoning)
    } else {
        None
    }
}
impl OutputMessageFragment {
    pub(crate) fn set_tools_flattened(&mut self, flattened: bool) {
        let mut changed = false;
        for node in &mut self.rows.children {
            if let Some(group) = node.component_mut::<crate::ToolRun>() {
                changed |= group.set_flattened(flattened);
            }
        }
        if changed {
            self.reveal_arranged = false;
        }
    }
    pub fn id(&self) -> ComponentId {
        self.id
    }
    pub(crate) fn owner(&self) -> ComponentId {
        self.owner
    }
    pub fn tool_key(&self) -> Option<&OutputMessageToolFragmentKey> {
        self.tool_key.as_ref()
    }
    pub fn kind(&self) -> OutputMessageFragmentKind {
        self.kind
    }
    pub fn content_ids(&self) -> Vec<OutputContentIds> {
        part_nodes(&self.rows.children)
            .into_iter()
            .map(|node| node.component::<OutputPart>().unwrap().ids)
            .collect()
    }
    pub fn tool_headers(&self) -> Vec<&tekes_agent_presentation::TranscriptToolHeader> {
        part_nodes(&self.rows.children)
            .into_iter()
            .filter_map(|node| node.component::<OutputPart>()?.tool_header())
            .collect()
    }
    pub fn tool_detail_sections(&self) -> Vec<crate::ToolDetailSectionIds> {
        part_nodes(&self.rows.children)
            .into_iter()
            .flat_map(|node| {
                node.component::<OutputPart>()
                    .unwrap()
                    .tool_detail_sections()
            })
            .collect()
    }
    fn visible_parts(&self) -> Vec<&Content> {
        visible_part_nodes(&self.rows.children)
    }
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        self.visible_parts()
            .into_iter()
            .flat_map(|node| node.component::<OutputPart>().unwrap().copy_ids())
            .collect()
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<crate::TranscriptCopyIntent> {
        self.visible_parts()
            .into_iter()
            .flat_map(|node| {
                node.component::<OutputPart>()
                    .unwrap()
                    .copy_intents(&self.scope, actions)
            })
            .collect()
    }
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        // Use the original control action implementation over these actual nodes.
        let changed = handle_part_actions(&mut self.rows.children, actions);
        if changed {
            self.reveal_arranged = false;
        }
        changed
    }
}
impl Component for OutputMessageFragment {
    fn host_reveal_passthrough(&self) -> bool {
        self.reveal_arranged
    }

    fn lifecycle(&mut self, _: Lifecycle) {
        self.reveal_arranged = false;
    }
    fn active(&self) -> bool {
        !self.rows.children.is_empty()
    }
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.reveal_arranged = false;
        self.rows.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.reveal_arranged = false;
        arrange_content(
            &mut self.rows.children,
            text,
            frame,
            clip,
            Spacing::Regular.value(),
            false,
        );
        self.reveal_arranged = !frame.is_empty() && !clip.is_empty();
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(
            ContentRole::Group,
            (self.labels.text)(OutputMessageText::Group),
        )
    }
}
impl OutputMessage {
    pub(crate) fn transcript_is_split(&self) -> bool {
        !self.split_fragments.is_empty()
    }
    pub(crate) fn transcript_tool_fragments(
        scope: &RunScope,
        output: &TranscriptOutput<'_>,
        section: &TranscriptSection<'_>,
        thinking: Option<&tekes_agent_presentation::TranscriptThinkingFrame>,
        labels: OutputMessageLabels,
    ) -> Result<Vec<(OutputMessageToolFragmentKey, Vec<String>)>, &'static str> {
        let stamp = thinking.map(|frame| ThinkingStamp {
            revision: frame.display_revision(),
            source_fingerprint: frame.source_fingerprint().into(),
            display: frame
                .display_for(&output.item.id)
                .unwrap_or(tekes_agent_presentation::TranscriptReasoningDisplay::Hidden),
        });
        let facts = thinking_parts(
            transcript_parts(scope, output, Some(section), labels)?,
            stamp.as_ref(),
            labels,
        );
        Ok(tool_part_runs(&facts, Some(section))?
            .into_iter()
            .filter(|range| facts[range.start].kind == OutputContentKind::Tool)
            .map(|range| {
                let key = tool_key(&facts, &facts[range.start]);
                let ids = facts[range]
                    .iter()
                    .map(|part| {
                        let Slot::Tool(_, id) = &part.slot else {
                            unreachable!();
                        };
                        id.clone()
                    })
                    .collect();
                (key, ids)
            })
            .collect())
    }
    pub(crate) fn transcript_presence(
        scope: &RunScope,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: Option<&tekes_agent_presentation::TranscriptThinkingFrame>,
        labels: OutputMessageLabels,
    ) -> Result<[bool; 3], &'static str> {
        let stamp = thinking.map(|frame| ThinkingStamp {
            revision: frame.display_revision(),
            source_fingerprint: frame.source_fingerprint().into(),
            display: frame
                .display_for(&output.item.id)
                .unwrap_or(tekes_agent_presentation::TranscriptReasoningDisplay::Hidden),
        });
        let facts = thinking_parts(
            transcript_parts(scope, output, section, labels)?,
            stamp.as_ref(),
            labels,
        );
        let mut present = [false; 3];
        for facts in facts {
            let index = if facts.kind == OutputContentKind::Tool {
                2
            } else if facts.kind == OutputContentKind::Reasoning
                || facts.slot == Slot::ThinkingStatus
            {
                1
            } else {
                0
            };
            present[index] = true;
        }
        Ok(present)
    }
    pub(crate) fn check_transcript_update(
        &self,
        publication: u64,
        epoch: u64,
        output: &TranscriptOutput<'_>,
        section: Option<&TranscriptSection<'_>>,
        thinking: Option<&tekes_agent_presentation::TranscriptThinkingFrame>,
        fragments: &[&OutputMessageFragment],
    ) -> Result<(), &'static str> {
        self.check_fragments(fragments)?;
        if self.split_fragments.is_empty()
            && let Some(thinking) = thinking
        {
            return self.check_update_with_thinking(publication, epoch, output, section, thinking);
        }
        let stamp = thinking
            .map(|frame| self.state.thinking_stamp(publication, epoch, output, frame))
            .transpose()?;
        let nodes = self
            .rows
            .children
            .iter()
            .chain(fragments.iter().flat_map(|part| part.rows.children.iter()));
        self.state.check_update_display(
            nodes,
            publication,
            epoch,
            output,
            section,
            stamp.as_ref(),
        )?;
        Ok(())
    }
    fn check_fragments(&self, fragments: &[&OutputMessageFragment]) -> Result<(), &'static str> {
        let mut seen = std::collections::BTreeSet::new();
        for part in fragments {
            if part.owner != self.id() || part.scope != self.state.scope || !seen.insert(part.id())
            {
                return Err("foreign or duplicate output fragment");
            }
            let children: Vec<_> = part.rows.children.iter().map(Content::id).collect();
            if self.split_fragments.get(&part.id()) != Some(&children) {
                return Err("missing or replaced original output fragment nodes");
            }
        }
        if seen.len() != self.split_fragments.len() {
            return Err("missing original output fragments");
        }
        Ok(())
    }
    pub(crate) fn rejoin_fragments(
        &mut self,
        fragments: Vec<&mut OutputMessageFragment>,
    ) -> Result<(), &'static str> {
        self.reveal_arranged = false;
        self.check_fragments(&fragments.iter().map(|part| &**part).collect::<Vec<_>>())?;
        for fragment in fragments {
            self.rows.children.append(&mut fragment.rows.children);
        }
        self.split_fragments.clear();
        Ok(())
    }
    pub(crate) fn split_fragments(
        &mut self,
        tools: &[(OutputMessageToolFragmentKey, ComponentId)],
    ) -> Vec<Content> {
        self.reveal_arranged = false;
        assert!(
            self.split_fragments.is_empty(),
            "rejoin before updating output"
        );
        let mut reasoning = vec![];
        let mut partitions = vec![];
        for node in std::mem::take(&mut self.rows.children) {
            match kind(&node) {
                Some(OutputMessageFragmentKind::Reasoning) => reasoning.push(node),
                Some(OutputMessageFragmentKind::Tools) => {
                    let first = part_nodes(std::slice::from_ref(&node))[0]
                        .component::<OutputPart>()
                        .unwrap();
                    let key = tool_key(&self.state.facts, &first.facts);
                    let id = tools
                        .iter()
                        .find(|(candidate, _)| candidate == &key)
                        .expect("preflighted original tool partition")
                        .1;
                    partitions.push((id, OutputMessageFragmentKind::Tools, Some(key), vec![node]));
                }
                None => self.rows.children.push(node),
            }
        }
        if !reasoning.is_empty() {
            partitions.insert(
                0,
                (
                    ComponentId(self.id().0 + OutputMessageFragmentKind::Reasoning.offset()),
                    OutputMessageFragmentKind::Reasoning,
                    None,
                    reasoning,
                ),
            );
        }
        partitions
            .into_iter()
            .map(|(id, kind, tool_key, children)| {
                assert!(
                    self.split_fragments
                        .insert(id, children.iter().map(Content::id).collect())
                        .is_none()
                );
                let part = OutputMessageFragment {
                    id,
                    owner: self.id(),
                    reveal_arranged: false,
                    scope: self.state.scope.clone(),
                    kind,
                    tool_key,
                    labels: self.state.labels,
                    rows: StackContent {
                        model: Stack {
                            axis: Axis::Vertical,
                            gap: Spacing::Regular.value(),
                        },
                        flexible_child: None,
                        children,
                    },
                };
                Content::new(id, part)
            })
            .collect()
    }
}
