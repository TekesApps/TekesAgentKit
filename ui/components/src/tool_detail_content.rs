//! Tool-owned sections composed entirely from original SDK text/stack/buttons.
use crate::{TranscriptCopyIntent, TranscriptCopySource};
use tekes_agent_presentation::{
    RunScope, TranscriptToolDetailKind, TranscriptToolDetailSection, TranscriptToolDetails,
};
use tekes_ui_controls::{
    ButtonTextRenderer, DraftRevision, ExternalSelection, ExternalUndo, ExternalValueResult,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolDetailSectionIds {
    pub kind: TranscriptToolDetailKind,
    pub root: ComponentId,
    pub header: ComponentId,
    pub title: ComponentId,
    pub copy: ComponentId,
    pub body: ComponentId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolDetailFacts {
    pub tool_id: String,
    pub turn_id: String,
    pub details: TranscriptToolDetails,
}
#[derive(Debug)]
struct Section {
    ids: ToolDetailSectionIds,
    facts: TranscriptToolDetailSection,
    epoch: u64,
    rows: StackContent,
}
impl Section {
    fn new(
        facts: &TranscriptToolDetailSection,
        alloc: &mut dyn FnMut() -> ComponentId,
        bounded: bool,
    ) -> Self {
        let ids = ToolDetailSectionIds {
            kind: facts.kind,
            root: alloc(),
            header: alloc(),
            title: alloc(),
            copy: alloc(),
            body: alloc(),
        };
        let mut field =
            TextFieldContent::read_only_content(&facts.title, "", proposal::TextRole::UiBody);
        if bounded {
            field.surface.scroll.set_scroll_policy(
                tekes_ui_controls::ScrollPolicy::Disabled,
                tekes_ui_controls::ScrollPolicy::Auto,
            );
            field.reading = Some(tekes_ui_controls::ReadingController::default());
        }
        Self {
            ids,
            facts: facts.clone(),
            epoch: 0,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(
                        ids.header,
                        StackContent {
                            model: Stack {
                                axis: Axis::Horizontal,
                                gap: Spacing::Tight.value(),
                            },
                            flexible_child: Some(ids.title),
                            children: vec![
                                Content::new(
                                    ids.title,
                                    TextContent {
                                        text: facts.title.clone(),
                                        role: proposal::TextRole::UiCaption,
                                    },
                                ),
                                Content::new(ids.copy, ButtonContent::new("")).preserving_focus(),
                            ],
                        },
                    ),
                    Content::new(ids.body, field),
                ],
            },
        }
    }
    fn update(
        &mut self,
        publication: u64,
        epoch: u64,
        facts: &TranscriptToolDetailSection,
        copy_label: &str,
        alloc: &mut dyn FnMut() -> ComponentId,
    ) {
        let header = self.rows.children[0]
            .component_mut::<StackContent>()
            .unwrap();
        header.children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = facts.title.clone();
        if self.epoch != epoch || self.facts.copy_text != facts.copy_text {
            self.ids.copy = alloc();
            header.children[1] =
                Content::new(self.ids.copy, ButtonContent::new(copy_label)).preserving_focus();
        }
        let button = header.children[1].component_mut::<ButtonContent>().unwrap();
        button.label = copy_label.into();
        button.button.state.disabled = facts.copy_text.is_empty();
        let field = self.rows.children[1]
            .component_mut::<TextFieldContent>()
            .unwrap();
        field.field.label = facts.title.clone();
        let baseline = field.surface.input.external_baseline();
        assert!(matches!(
            field.surface.apply_external_value(
                &baseline,
                &DraftRevision {
                    identity: format!("tool-detail:{}", self.ids.body.0),
                    version: publication,
                    payload: facts.text.clone(),
                },
                ExternalSelection::Preserve,
                ExternalUndo::Preserve
            ),
            ExternalValueResult::Applied { .. } | ExternalValueResult::Unchanged
        ));
        self.epoch = epoch;
        self.facts = facts.clone();
    }
}
impl Component for Section {
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
        self.rows.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.facts.title)
    }
}
#[derive(Debug)]
pub(crate) struct ToolDetailContent {
    rows: StackContent,
    facts: Option<ToolDetailFacts>,
    bounded: bool,
}
impl ToolDetailContent {
    pub fn new(bounded: bool) -> Self {
        Self {
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            facts: None,
            bounded,
        }
    }
    pub fn update(
        &mut self,
        publication: u64,
        epoch: u64,
        facts: &ToolDetailFacts,
        copy_label: &str,
        alloc: &mut dyn FnMut() -> ComponentId,
    ) {
        let mut old = std::mem::take(&mut self.rows.children);
        let same_owner = self
            .facts
            .as_ref()
            .is_some_and(|old| old.tool_id == facts.tool_id && old.turn_id == facts.turn_id);
        for section in &facts.details.sections {
            let mut content = if same_owner
                && let Some(index) = old
                    .iter()
                    .position(|node| node.component::<Section>().unwrap().ids.kind == section.kind)
            {
                old.remove(index)
            } else {
                let view = Section::new(section, alloc, self.bounded);
                Content::new(view.ids.root, view)
            };
            content.component_mut::<Section>().unwrap().update(
                publication,
                epoch,
                section,
                copy_label,
                alloc,
            );
            self.rows.children.push(content);
        }
        self.facts = Some(facts.clone());
    }
    pub fn ids(&self) -> Vec<ToolDetailSectionIds> {
        self.rows
            .children
            .iter()
            .map(|node| node.component::<Section>().unwrap().ids)
            .collect()
    }
    pub fn configure_viewports(&mut self, text: &mut dyn ButtonTextRenderer) {
        if !self.bounded {
            return;
        }
        let scale = text
            .text_surface_host()
            .map_or(1., |host| host.type_scale());
        for node in &mut self.rows.children {
            node.component_mut::<Section>().unwrap().rows.children[1]
                .component_mut::<TextFieldContent>()
                .unwrap()
                .viewport = TextViewport::Grow {
                min: 0.,
                max: 240. * scale,
            };
        }
    }
    pub fn copy_ids(&self) -> Vec<ComponentId> {
        self.rows
            .children
            .iter()
            .filter_map(|node| {
                let section = node.component::<Section>().unwrap();
                (!section.facts.copy_text.is_empty()).then_some(section.ids.copy)
            })
            .collect()
    }
    pub fn copy_intents(
        &self,
        scope: &RunScope,
        actions: &[ContentAction],
    ) -> Vec<TranscriptCopyIntent> {
        let Some(facts) = &self.facts else {
            return vec![];
        };
        self.rows
            .children
            .iter()
            .filter_map(|node| {
                let section = node.component::<Section>().unwrap();
                (!section.facts.copy_text.is_empty()
                    && actions.iter().any(|a| {
                        a.target == section.ids.copy && a.kind == ContentActionKind::Activate
                    }))
                .then(|| {
                    TranscriptCopyIntent::from_source(
                        scope.clone(),
                        TranscriptCopySource::ToolDetail {
                            tool_id: facts.tool_id.clone(),
                            turn_id: facts.turn_id.clone(),
                            section: section.ids.kind,
                        },
                        section.facts.copy_text.clone(),
                    )
                })
            })
            .collect()
    }
}
impl Component for ToolDetailContent {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.configure_viewports(text);
        self.rows.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.configure_viewports(text);
        self.rows.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }
}
