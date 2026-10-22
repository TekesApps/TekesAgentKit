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
    tool: Option<TranscriptToolCall>,
    placement: ToolActivityPlacement,
    labels: ToolActivityLabels,
    bounded_details: bool,
    children: Vec<Content>,
}
impl ToolActivity {
    pub fn new(namespace: u32, scope: RunScope, labels: ToolActivityLabels) -> Self {
        Self {
            root: ComponentId((namespace as u64) << 32),
            scope,
            publication: 0,
            epoch: 0,
            tool: None,
            placement: ToolActivityPlacement::Unplaced,
            labels,
            bounded_details: false,
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
    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tool: &TranscriptToolCall,
        placement: ToolActivityPlacement,
    ) -> Result<(), &'static str> {
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
        Ok(())
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        tool: &TranscriptToolCall,
        placement: ToolActivityPlacement,
    ) -> Result<(), &'static str> {
        self.check_update(scope, publication, epoch, tool, placement)?;
        let (title, text) = (self.labels.details.tool)(tool);
        let prefix = match placement {
            ToolActivityPlacement::Call => &self.labels.call,
            ToolActivityPlacement::ResultOnly => &self.labels.result_only,
            ToolActivityPlacement::Unplaced => &self.labels.unplaced,
        };
        let facts = PartFacts {
            slot: Slot::Tool(0, tool.id.clone()),
            kind: OutputContentKind::Tool,
            title: format!("{prefix} · {title}"),
            text,
        };
        if self.children.is_empty() {
            let id = ComponentId(self.root.0 + 16);
            self.children.push(Content::new(
                id,
                OutputPart::new(id, facts.clone(), self.labels.details, self.bounded_details),
            ));
        }
        self.children[0]
            .component_mut::<OutputPart>()
            .unwrap()
            .update(publication, facts);
        self.publication = publication;
        self.epoch = epoch;
        self.tool = Some(tool.clone());
        self.placement = placement;
        Ok(())
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
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let height = self
            .children
            .first_mut()
            .map_or(0., |child| child.measure(text, available).size.height);
        MeasuredChild::new(Size::new(available.width, height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if let Some(child) = self.children.first_mut() {
            child.arrange(text, frame, clip);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.call)
    }
}
