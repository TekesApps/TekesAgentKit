//! ClientKit Summary action rows activate without becoming first responder.
//! Original SDK CompoundButton owns input, capture, backing and activation.
use crate::summary_row_layout::SummaryRowBody;
use tekes_ui_controls::{
    ButtonFramePolicy, ButtonTextRenderer, composition::*, layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
#[derive(Debug)]
pub struct SummaryActionRow {
    button: CompoundButton,
}
impl SummaryActionRow {
    pub(crate) fn new(
        body: ComponentId,
        title: ComponentId,
        trailing: ComponentId,
        trailing_body: ComponentId,
    ) -> Self {
        let mut button = CompoundButton::new(
            "Summary row",
            Content::new(
                body,
                SummaryRowBody::new(
                    Content::new(title, TextContent::new("")),
                    trailing,
                    trailing_body,
                ),
            ),
        )
        .expect("one passive Summary title slot");
        button.action.button.frame_policy = ButtonFramePolicy::Exact;
        Self { button }
    }
    /// Original SDK action control, retained across status/text publications.
    pub fn button(&self) -> &ButtonContent {
        &self.button.action
    }
    pub(crate) fn update(&mut self, label: String, trailing: Option<String>, indent: u32) {
        self.button.action.label = label.clone();
        self.button.children_mut().unwrap()[0]
            .component_mut::<SummaryRowBody>()
            .unwrap()
            .update(trailing, indent);
        self.button.children_mut().unwrap()[0]
            .component_mut::<SummaryRowBody>()
            .unwrap()
            .children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = label;
    }
}
impl Component for SummaryActionRow {
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
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.button.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.button.arrange(text, frame, clip)
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
        self.button.event(event)
    }
    fn accessibility_action(
        &mut self,
        part: Option<usize>,
        action: &AccessibilityAction,
    ) -> ContentResponse {
        self.button.accessibility_action(part, action)
    }
    fn focusable(&self) -> bool {
        false
    }
    fn semantics(&self) -> ContentSemantics {
        self.button.semantics()
    }
}
