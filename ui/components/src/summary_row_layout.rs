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
    /// Original passive SDK label. Zero visible frame when metadata is absent.
    pub trailing: ComponentId,
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
    has_trailing: bool,
}
impl SummaryRowBody {
    pub fn new(title: Content, trailing: ComponentId) -> Self {
        Self {
            children: vec![
                title,
                Content::new(
                    trailing,
                    TextContent {
                        text: String::new(),
                        role: proposal::TextRole::UiCaption,
                    },
                )
                .with_label_layout(tekes_ui_text::LabelOptions::single_line(
                    tekes_ui_text::LabelOverflow::Clip,
                ))
                .expect("original single-line Summary metadata"),
            ],
            has_trailing: false,
        }
    }
    pub fn update_trailing(&mut self, value: Option<String>) {
        self.has_trailing = value.is_some();
        self.children[1]
            .component_mut::<TextContent>()
            .unwrap()
            .text = value.unwrap_or_default();
    }
    fn measurements(&mut self, text: &mut dyn ButtonTextRenderer, width: f32) -> [Size; 2] {
        let width = width.max(0.);
        let meta = if self.has_trailing {
            let mut size = self.children[1].measure(text, Size::new(width, 1e8)).size;
            size.width = size.width.min(width);
            size
        } else {
            Size::new(0., 0.)
        };
        let title_width = (width
            - meta.width
            - if self.has_trailing {
                Spacing::Regular.value()
            } else {
                0.
            })
        .max(0.);
        let mut title = self.children[0]
            .measure(text, Size::new(title_width, 1e8))
            .size;
        title.width = title_width;
        [title, meta]
    }
}
impl Component for SummaryRowBody {
    fn fixed_slots(&self) -> bool {
        true
    }
    fn validate(&self) -> Result<(), CompositionError> {
        if self.children.len() == 2 {
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
        let title = self.measurements(text, available.width - 2. * title_inset())[0];
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
        let [title, meta] = self.measurements(text, content.size.width);
        let top = frame.min_y() + (frame.size.height - title.height) / 2.;
        let title = Rect::new(content.min_x(), top, title.width, title.height);
        let meta = Rect::new(
            content.max_x() - meta.width,
            top + (body_line(text) - meta.height) / 2.,
            meta.width,
            meta.height,
        );
        for (node, rect) in self.children.iter_mut().zip([title, meta]) {
            node.arrange(text, rect, rect.intersection(&clip).unwrap_or_default());
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }
}
