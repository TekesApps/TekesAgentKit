//! Agent Summary section composition. Buttons, Disclosure state, focus and
//! input handling remain in TekesUI; no generic accordion engine lives here.
use std::{cell::RefCell, rc::Rc};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Disclosure, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::Spacing;

pub(crate) type Expansion = Rc<RefCell<Disclosure>>;

#[derive(Debug)]
pub(crate) struct SummaryHeader {
    pub button: ButtonContent,
    pub expansion: Expansion,
}
impl SummaryHeader {
    fn activated(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated {
            self.expansion.borrow_mut().toggle();
        }
        response
    }
}
impl Component for SummaryHeader {
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        self.button.measure(text, size)
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.button.paint(scene, text, frame, clip, theme)
    }
    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        let result = self.button.event(event);
        self.activated(result)
    }
    fn accessibility_action(
        &mut self,
        part: Option<usize>,
        action: &AccessibilityAction,
    ) -> ContentResponse {
        let result = self.button.accessibility_action(part, action);
        self.activated(result)
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn set_focus(&mut self, focused: bool, visible: bool) {
        self.button.set_focus(focused, visible)
    }
    fn semantics(&self) -> ContentSemantics {
        let mut value = self.button.semantics();
        value.expanded = Some(self.expansion.borrow().open);
        value
    }
}

#[derive(Debug)]
pub(crate) struct SummaryRows {
    pub rows: StackContent,
    expansion: Expansion,
}
impl Component for SummaryRows {
    fn active(&self) -> bool {
        self.expansion.borrow().open
    }
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        if self.active() {
            self.rows.measure(text, size)
        } else {
            MeasuredChild::new(Size::default())
        }
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if self.active() {
            self.rows.arrange(text, frame, clip);
        }
    }
}

/// Actual last-arranged section frames. Hidden bodies have no frame; model
/// changes and a disclosure toggle invalidate this query until the next layout.
#[derive(Clone, Debug, PartialEq)]
pub struct SummarySectionLayout {
    pub section_id: String,
    pub header: Rect,
    pub body: Option<Rect>,
    pub expanded: bool,
}
#[derive(Debug)]
pub(crate) struct SummarySectionView {
    pub section: String,
    pub children: Vec<Content>,
    pub expansion: Expansion,
    pub layout: Option<SummarySectionLayout>,
}
impl SummarySectionView {
    pub fn new(
        section: String,
        header: ComponentId,
        body: ComponentId,
        expansion: Expansion,
    ) -> Self {
        Self {
            section,
            expansion: expansion.clone(),
            layout: None,
            children: vec![
                Content::new(
                    header,
                    SummaryHeader {
                        button: ButtonContent::new(""),
                        expansion: expansion.clone(),
                    },
                ),
                Content::new(
                    body,
                    SummaryRows {
                        rows: StackContent {
                            model: Stack {
                                axis: Axis::Vertical,
                                gap: 0.,
                            },
                            flexible_child: None,
                            children: vec![],
                        },
                        expansion,
                    },
                ),
            ],
        }
    }
    pub fn header_mut(&mut self) -> &mut SummaryHeader {
        self.children[0].component_mut::<SummaryHeader>().unwrap()
    }
    pub fn rows_mut(&mut self) -> &mut Vec<Content> {
        &mut self.children[1]
            .component_mut::<SummaryRows>()
            .unwrap()
            .rows
            .children
    }
    pub fn arranged(&self) -> Option<&SummarySectionLayout> {
        self.layout
            .as_ref()
            .filter(|layout| layout.expanded == self.expansion.borrow().open)
    }
}
impl Component for SummarySectionView {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let header = self.children[0].measure(text, available).size;
        let body = self.children[1].measure(text, available).size;
        MeasuredChild::new(Size::new(available.width, header.height + body.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let height = self.children[0].measure(text, frame.size).size.height;
        let header = Rect::new(frame.min_x(), frame.min_y(), frame.size.width, height);
        self.children[0].arrange(text, header, clip);
        let body = self.expansion.borrow().open.then(|| {
            Rect::new(
                frame.min_x(),
                header.max_y(),
                frame.size.width,
                (frame.size.height - height).max(0.),
            )
        });
        if let Some(body) = body {
            self.children[1].arrange(text, body, clip);
        }
        self.layout = Some(SummarySectionLayout {
            section_id: self.section.clone(),
            header,
            body,
            expanded: body.is_some(),
        });
    }
}

#[derive(Debug)]
pub(crate) struct SummaryDocument {
    pub list: ListContent,
}
impl Component for SummaryDocument {
    fn children(&self) -> &[Content] {
        &self.list.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.list.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let measured = self.list.measure(text, available);
        MeasuredChild::new(Size::new(
            available.width,
            measured.size.height + 2. * Spacing::Regular.value(),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let content = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets {
                top: Spacing::Regular.value(),
                bottom: Spacing::Regular.value(),
                ..Insets::ZERO
            },
            Some(clip),
        )
        .content();
        self.list.arrange(text, content, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        self.list.semantics()
    }
}
