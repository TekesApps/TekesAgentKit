//! Agent Summary section composition. Buttons, Disclosure state, focus and
//! input handling remain in TekesUI; no generic accordion engine lives here.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Disclosure, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
    layout_view::SeparatorView,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SectionHeaderKind {
    Summary,
    Sidebar,
}

pub(crate) type Expansion = Rc<RefCell<Disclosure>>;

#[derive(Debug)]
pub(crate) struct SummaryHeader {
    button: CompoundButton,
    pub expansion: Expansion,
    epoch: Rc<Cell<u64>>,
    kind: SectionHeaderKind,
    shows_divider: bool,
    divider: SeparatorView,
    divider_frame: Option<Rect>,
}
impl SummaryHeader {
    fn new(
        label: ComponentId,
        expansion: Expansion,
        epoch: Rc<Cell<u64>>,
        kind: SectionHeaderKind,
    ) -> Self {
        let label = Content::new(label, TextContent::new(""));
        let label = if kind == SectionHeaderKind::Summary {
            label
                .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                    tekes_ui_text::LabelOverflow::TailEllipsis,
                ))
                .expect("published Summary label policy on original TextContent")
        } else {
            label
        };
        Self {
            button: CompoundButton::new("Summary section", label)
                .expect("Summary header has one passive text slot"),
            expansion,
            epoch,
            kind,
            shows_divider: false,
            divider: SeparatorView::default(),
            divider_frame: None,
        }
    }
    fn rule_height() -> f32 {
        tekes_ui_tokens::proposal::metric("stroke", "stroke-rule")
            .expect("published separator rule")
    }
    fn divider_reserve(&self) -> f32 {
        if self.shows_divider {
            Self::rule_height() + Spacing::Regular.value()
        } else {
            0.
        }
    }
    pub fn set_repository_divider(&mut self, shown: bool) {
        self.shows_divider = self.kind == SectionHeaderKind::Summary && shown;
    }
    pub fn set_title(&mut self, title: String) {
        self.button.action.label = title.clone();
        self.button.children_mut().unwrap()[0]
            .component_mut::<TextContent>()
            .expect("Summary title")
            .text = title;
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.button.action.button.state.disabled = !enabled;
    }
    fn activated(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated && self.expansion.borrow_mut().toggle() {
            self.epoch.set(
                self.epoch
                    .get()
                    .checked_add(1)
                    .expect("Summary disclosure generation exhausted"),
            );
        }
        response
    }
}
impl Component for SummaryHeader {
    fn children(&self) -> &[Content] {
        self.button.children()
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.button.children_mut()
    }
    fn fixed_slots(&self) -> bool {
        self.button.fixed_slots()
    }
    fn validate(&self) -> Result<(), CompositionError> {
        self.button.validate()
    }
    fn child_theme(&self, theme: ContentTheme) -> ContentTheme {
        self.button.child_theme(theme)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        let mut measured = self.button.measure(text, size);
        measured.size.height += self.divider_reserve();
        measured
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        // The original button retains the full header hit/AX frame, including
        // the divider band, as ClientKit's header hitTest does.
        self.button.arrange(text, frame, clip);
        self.divider_frame = self.shows_divider.then(|| {
            let inset = crate::summary_row_layout::title_inset();
            Rect::new(
                frame.min_x() + inset,
                frame.min_y(),
                (frame.size.width - 2. * inset).max(0.),
                Self::rule_height(),
            )
        });
        if self.kind == SectionHeaderKind::Summary {
            let inset = crate::summary_row_layout::title_inset();
            let reserve = self.divider_reserve();
            let label = &mut self.button.children_mut().unwrap()[0];
            let width = (frame.size.width - 2. * inset).max(0.);
            let measured = label
                .measure(
                    text,
                    Size::new(width, (frame.size.height - reserve).max(0.)),
                )
                .size;
            let bounds = Rect::new(
                frame.min_x() + inset,
                frame.min_y() + reserve + (frame.size.height - reserve - measured.height) / 2.,
                width,
                measured.height,
            );
            label.arrange(text, bounds, bounds.intersection(&clip).unwrap_or_default());
        }
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.button.paint(scene, text, frame, clip, theme);
        if let Some(divider) = self.divider_frame {
            self.divider
                .render(scene, divider, Axis::Horizontal, Some(clip), theme.palette);
        }
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
        self.kind == SectionHeaderKind::Sidebar && self.button.focusable()
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
    /// Arranged logical SDK separator band, before ancestor clipping. None for
    /// first/non-repository sections. This is not an ink or pointer target.
    pub divider: Option<Rect>,
    pub body: Option<Rect>,
    pub expanded: bool,
}
#[derive(Debug)]
pub(crate) struct SummarySectionView {
    pub body_indent: f32,
    pub section: String,
    pub children: Vec<Content>,
    pub expansion: Expansion,
    pub layout: Option<SummarySectionLayout>,
    pub disclosure_epoch: Rc<Cell<u64>>,
}
impl SummarySectionView {
    pub fn new(
        section: String,
        header: ComponentId,
        label: ComponentId,
        body: ComponentId,
        expansion: Expansion,
        kind: SectionHeaderKind,
    ) -> Self {
        let epoch = Rc::new(Cell::new(0));
        Self {
            disclosure_epoch: epoch.clone(),
            section,
            body_indent: 0.,
            expansion: expansion.clone(),
            layout: None,
            children: vec![
                Content::new(
                    header,
                    SummaryHeader::new(label, expansion.clone(), epoch, kind),
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
    pub fn rows(&self) -> &[Content] {
        &self.children[1]
            .component::<SummaryRows>()
            .unwrap()
            .rows
            .children
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
        let body = self.children[1]
            .measure(
                text,
                Size::new(
                    (available.width - self.body_indent).max(0.),
                    available.height,
                ),
            )
            .size;
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
        let body = body.map(|body| {
            ContainerLayout::new(
                body,
                Insets::ZERO,
                Insets::new(0., 0., 0., self.body_indent),
                Some(clip),
            )
            .content()
        });
        if let Some(body) = body {
            self.children[1].arrange(text, body, clip);
        }
        self.layout = Some(SummarySectionLayout {
            section_id: self.section.clone(),
            header,
            divider: self.children[0]
                .component::<SummaryHeader>()
                .unwrap()
                .divider_frame,
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
