//! Disjoint owners of original message nodes in the Transcript's flat reading tree.
use super::*;

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
    fn bit(self) -> u8 {
        1 << self.offset()
    }
}
#[derive(Debug)]
pub struct OutputMessageFragment {
    owner: ComponentId,
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
    pub fn id(&self) -> ComponentId {
        ComponentId(self.owner.0 + self.kind.offset())
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
        handle_part_actions(&mut self.rows.children, actions)
    }
}
impl Component for OutputMessageFragment {
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
        self.rows.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        arrange_content(
            &mut self.rows.children,
            text,
            frame,
            clip,
            Spacing::Regular.value(),
            false,
        );
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
        self.split_mask != 0
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
            parts(scope, output, section, labels)?,
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
        fragments: [Option<&OutputMessageFragment>; 2],
    ) -> Result<(), &'static str> {
        self.check_fragments(fragments)?;
        if self.split_mask == 0
            && let Some(thinking) = thinking
        {
            return self.check_update_with_thinking(publication, epoch, output, section, thinking);
        }
        let stamp = thinking
            .map(|frame| self.state.thinking_stamp(publication, epoch, output, frame))
            .transpose()?;
        let nodes = self.rows.children.iter().chain(
            fragments
                .into_iter()
                .flatten()
                .flat_map(|part| part.rows.children.iter()),
        );
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
    fn check_fragments(
        &self,
        fragments: [Option<&OutputMessageFragment>; 2],
    ) -> Result<(), &'static str> {
        let mut mask = 0;
        for part in fragments.into_iter().flatten() {
            if part.owner != self.id()
                || part.scope != self.state.scope
                || mask & part.kind.bit() != 0
            {
                return Err("foreign or duplicate output fragment");
            }
            mask |= part.kind.bit();
        }
        if mask != self.split_mask {
            return Err("missing original output fragments");
        }
        Ok(())
    }
    pub(crate) fn rejoin_fragments(
        &mut self,
        mut fragments: [Option<&mut OutputMessageFragment>; 2],
    ) -> Result<(), &'static str> {
        self.check_fragments([fragments[0].as_deref(), fragments[1].as_deref()])?;
        for fragment in fragments.iter_mut().flatten() {
            self.rows.children.append(&mut fragment.rows.children);
        }
        self.split_mask = 0;
        Ok(())
    }
    pub(crate) fn split_fragments(&mut self) -> Vec<Content> {
        assert_eq!(self.split_mask, 0, "rejoin before updating output");
        let mut fragments = [vec![], vec![]];
        let nodes = std::mem::take(&mut self.rows.children);
        for node in nodes {
            match kind(&node) {
                Some(OutputMessageFragmentKind::Reasoning) => fragments[0].push(node),
                Some(OutputMessageFragmentKind::Tools) => fragments[1].push(node),
                None => self.rows.children.push(node),
            }
        }
        fragments
            .into_iter()
            .zip([
                OutputMessageFragmentKind::Reasoning,
                OutputMessageFragmentKind::Tools,
            ])
            .filter_map(|(children, kind)| {
                if children.is_empty() {
                    return None;
                }
                self.split_mask |= kind.bit();
                let part = OutputMessageFragment {
                    owner: self.id(),
                    scope: self.state.scope.clone(),
                    kind,
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
                Some(Content::new(part.id(), part))
            })
            .collect()
    }
}
