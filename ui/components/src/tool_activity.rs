//! Cumulative tool facts at their proven call/result location; no endpoint access.
use crate::output_message::{OutputPart, PartFacts, Slot};
use crate::{OutputContentIds, OutputContentKind, OutputMessageLabels};
use tekes_agent_presentation::{RunScope, TranscriptToolCall};
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolActivityPlacement {
    Call,
    ResultOnly,
    Unplaced,
}
#[derive(Clone, Debug)]
pub struct ToolActivityLabels {
    pub call: String,
    pub result_only: String,
    pub unplaced: String,
    pub details: OutputMessageLabels,
}
impl Default for ToolActivityLabels {
    fn default() -> Self {
        Self {
            call: "工具调用".into(),
            result_only: "工具结果（调用位置未确认）".into(),
            unplaced: "工具（事件位置未确认）".into(),
            details: Default::default(),
        }
    }
}
#[derive(Debug)]
pub struct ToolActivity {
    root: ComponentId,
    scope: RunScope,
    publication: u64,
    epoch: u64,
    header_revision: u64,
    tool: Option<TranscriptToolCall>,
    placement: ToolActivityPlacement,
    labels: ToolActivityLabels,
    bounded_details: bool,
    next: u64,
    reveal_arranged: bool,
    children: Vec<Content>,
}
impl ToolActivity {
    pub fn new(namespace: u32, scope: RunScope, labels: ToolActivityLabels) -> Self {
        Self {
            root: ComponentId((namespace as u64) << 32),
            scope,
            publication: 0,
            epoch: 0,
            header_revision: 0,
            tool: None,
            placement: ToolActivityPlacement::Unplaced,
            labels,
            bounded_details: false,
            next: 19,
            reveal_arranged: false,
            children: vec![],
        }
    }
    /// Explicit experimental viewport, gated by TekesUI #53 before standard
    /// Transcript/RunPort adoption. Uses the same original tool/text owners.
    pub fn with_bounded_details(
        namespace: u32,
        scope: RunScope,
        labels: ToolActivityLabels,
    ) -> Self {
        let mut view = Self::new(namespace, scope, labels);
        view.bounded_details = true;
        view
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn content_ids(&self) -> Option<OutputContentIds> {
        self.children
            .first()
            .map(|c| c.component::<OutputPart>().unwrap().ids())
    }
    fn checked_facts(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tool: &TranscriptToolCall,
        placement: ToolActivityPlacement,
    ) -> Result<(PartFacts, u64), &'static str> {
        if scope != &self.scope {
            return Err("tool activity scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale tool activity publication");
        }
        if tool.id.is_empty() || self.tool.as_ref().is_some_and(|old| old.id != tool.id) {
            return Err("tool activity identity changed");
        }
        if publication == self.publication
            && (epoch != self.epoch
                || self.tool.as_ref() != Some(tool)
                || placement != self.placement)
        {
            return Err("conflicting tool activity publication");
        }
        if self
            .next
            .checked_add(32)
            .is_none_or(|value| value >= u32::MAX as u64)
        {
            return Err("tool detail namespace exhausted");
        }
        let header_revision =
            crate::output_message::tool_header_revision(self.labels.details, scope);
        if epoch == self.epoch && header_revision < self.header_revision {
            return Err("stale tool header revision");
        }
        let facts = self.facts(tool, placement);
        if header_revision
            != crate::output_message::tool_header_revision(self.labels.details, scope)
        {
            return Err("tool header inputs changed during preparation");
        }
        crate::output_message::validate_tool_details(&facts)?;
        if publication == self.publication
            && self.children.first().is_some_and(|node| {
                let part = node.component::<OutputPart>().unwrap();
                !(part.matches_facts(&facts)
                    || (header_revision > self.header_revision
                        && part.matches_header_update(&facts)))
            })
        {
            return Err("conflicting tool detail publication");
        }
        Ok((facts, header_revision))
    }
    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tool: &TranscriptToolCall,
        placement: ToolActivityPlacement,
    ) -> Result<(), &'static str> {
        self.checked_facts(scope, publication, epoch, tool, placement)
            .map(|_| ())
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tool: &TranscriptToolCall,
        placement: ToolActivityPlacement,
    ) -> Result<(), &'static str> {
        self.reveal_arranged = false;
        let (facts, header_revision) =
            self.checked_facts(scope, publication, epoch, tool, placement)?;
        if self.children.is_empty() {
            let id = ComponentId(self.root.0 + 16);
            self.children.push(Content::new(
                id,
                OutputPart::new(id, facts.clone(), self.labels.details, self.bounded_details),
            ));
        }
        let base = self.root.0;
        let next = &mut self.next;
        self.children[0]
            .component_mut::<OutputPart>()
            .unwrap()
            .update(publication, epoch, facts, &mut || {
                let id = ComponentId(base + *next);
                *next += 1;
                id
            });
        self.header_revision = header_revision;
        self.publication = publication;
        self.epoch = epoch;
        self.tool = Some(tool.clone());
        self.placement = placement;
        Ok(())
    }
    fn facts(&self, tool: &TranscriptToolCall, placement: ToolActivityPlacement) -> PartFacts {
        let (title, text, tool_header) =
            crate::output_message::tool_text(&self.scope, tool, self.labels.details);
        let prefix = match placement {
            ToolActivityPlacement::Call => &self.labels.call,
            ToolActivityPlacement::ResultOnly => &self.labels.result_only,
            ToolActivityPlacement::Unplaced => &self.labels.unplaced,
        };
        PartFacts {
            title: if tool_header.is_some() && placement == ToolActivityPlacement::Call {
                title
            } else {
                format!("{prefix} · {title}")
            },
            tool_header,
            tool: crate::output_message::tool_details_facts(&self.scope, tool, self.labels.details),
            slot: Slot::Tool(0, tool.id.clone()),
            kind: OutputContentKind::Tool,
            text,
        }
    }
    pub fn tool_detail_sections(&self) -> Vec<crate::ToolDetailSectionIds> {
        self.children.first().map_or_else(Vec::new, |node| {
            node.component::<OutputPart>()
                .unwrap()
                .tool_detail_sections()
        })
    }
    pub fn tool_header(&self) -> Option<&tekes_agent_presentation::TranscriptToolHeader> {
        self.children
            .first()?
            .component::<OutputPart>()?
            .tool_header()
    }
    pub fn copy_action_ids(&self) -> Vec<ComponentId> {
        self.children.first().map_or_else(Vec::new, |node| {
            node.component::<OutputPart>().unwrap().copy_ids()
        })
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<crate::TranscriptCopyIntent> {
        self.children.first().map_or_else(Vec::new, |node| {
            node.component::<OutputPart>()
                .unwrap()
                .copy_intents(&self.scope, actions)
        })
    }
    pub fn handle_actions(&mut self, actions: &[ContentAction]) -> bool {
        let Some(part) = self
            .children
            .first_mut()
            .and_then(|c| c.component_mut::<OutputPart>())
        else {
            return false;
        };
        let mut changed = false;
        let target = part.ids().disclosure;
        for _ in actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate && target == Some(a.target))
        {
            part.toggle();
            self.reveal_arranged = false;
            changed = true;
        }
        changed
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for ToolActivity {
    fn host_reveal_passthrough(&self) -> bool {
        self.reveal_arranged
    }

    fn lifecycle(&mut self, _: Lifecycle) {
        self.reveal_arranged = false;
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.reveal_arranged = false;
        let height = self
            .children
            .first_mut()
            .map_or(0., |child| child.measure(text, available).size.height);
        MeasuredChild::new(Size::new(available.width, height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.reveal_arranged = false;
        if let Some(child) = self.children.first_mut() {
            child.arrange(text, frame, clip);
        }
        self.reveal_arranged = !frame.is_empty() && !clip.is_empty();
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.call)
    }
}
