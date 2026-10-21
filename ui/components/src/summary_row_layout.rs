//! Agent Summary row geometry; text measurement/editing remains in the SDK.
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryDefaultRowIds {
    pub row: ComponentId,
    pub title: ComponentId,
}
pub(crate) fn title_inset() -> f32 {
    2. * Spacing::Regular.value()
}
pub(crate) fn body_line(text: &mut dyn ButtonTextRenderer) -> f32 {
    text.measure("Mg", proposal::TextRole::UiBody, 1e8).height
}
#[derive(Debug)]
pub(crate) struct SummaryRowBody {
    pub children: Vec<Content>,
}
impl SummaryRowBody {
    pub fn new(title: Content) -> Self {
        Self {
            children: vec![title],
        }
    }
}
impl Component for SummaryRowBody {
    fn fixed_slots(&self) -> bool {
        true
    }
    fn validate(&self) -> Result<(), CompositionError> {
        if self.children.len() == 1 {
            Ok(())
        } else {
            Err(CompositionError::InvalidSlotCount)
        }
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let title = self.children[0]
            .measure(
                text,
                Size::new((available.width - 2. * title_inset()).max(0.), 1e8),
            )
            .size;
        MeasuredChild::new(Size::new(
            available.width,
            title.height.max(body_line(text)) + 2. * Spacing::Regular.value(),
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let content = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(
                Spacing::Regular.value(),
                title_inset(),
                Spacing::Regular.value(),
                title_inset(),
            ),
            Some(clip),
        )
        .content();
        let title = self.children[0]
            .measure(text, Size::new(content.size.width, 1e8))
            .size;
        let title = Rect::new(
            content.min_x(),
            frame.min_y() + (frame.size.height - title.height) / 2.,
            content.size.width,
            title.height,
        );
        self.children[0].arrange(text, title, title.intersection(&clip).unwrap_or_default());
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }
}
