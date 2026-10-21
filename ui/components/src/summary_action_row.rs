//! ClientKit Summary action rows activate without becoming first responder.
//! Original SDK button owns input, capture, paint and activation state.
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Scene, Size};

#[derive(Debug)]
pub struct SummaryActionRow {
    pub(crate) button: ButtonContent,
}
impl SummaryActionRow {
    pub(crate) fn new() -> Self {
        Self {
            button: ButtonContent::new(""),
        }
    }
    /// Original SDK control, for inspection; facts still update through Summary.
    pub fn button(&self) -> &ButtonContent {
        &self.button
    }
}
impl Component for SummaryActionRow {
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
