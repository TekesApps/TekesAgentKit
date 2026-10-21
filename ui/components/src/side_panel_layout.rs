//! Agent pane placement; normalization and insets belong to the SDK layout API.
use tekes_ui_controls::{
    ButtonTextRenderer, ContainerLayout, Insets,
    composition::{Component, Content, StackContent},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

/// Window/titlebar clearance supplied by the host in pane-local logical points.
/// Inline demo tabs remain diagnostic chrome, not a native window toolbar.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SidePanelLayout {
    pub content_top_inset: f32,
}
impl SidePanelLayout {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        if !self.content_top_inset.is_finite() || self.content_top_inset < 0. {
            return Err("invalid side-panel content top inset");
        }
        Ok(())
    }
    pub(crate) fn canvas(self, pane: Rect, clip: Rect) -> ContainerLayout {
        ContainerLayout::new(
            pane,
            Insets::ZERO,
            Insets::new(self.content_top_inset, 0., 0., 0.),
            Some(clip),
        )
    }
}
/// Last arranged geometry only; not a successful-render or reading authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidePanelLayoutGeometry {
    pub pane: Rect,
    /// Pane below the host's titlebar clearance, including inline demo chrome.
    pub canvas: Rect,
    /// Actual Agent content, after inline chrome and the regular card inset.
    pub body: Rect,
    pub visible_body: Option<Rect>,
}

#[derive(Debug)]
pub(crate) struct AgentPanelBody {
    pub stack: StackContent,
    pub framed: bool,
    pub layout: Option<ContainerLayout>,
}
impl AgentPanelBody {
    fn container(&self, frame: Rect, clip: Option<Rect>) -> ContainerLayout {
        let inset = if self.framed {
            Spacing::Regular.value()
        } else {
            0.
        };
        ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets::new(inset, inset, inset, inset),
            clip,
        )
    }
}
impl Component for AgentPanelBody {
    fn children(&self) -> &[Content] {
        &self.stack.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.stack.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let layout = self.container(Rect::new(0., 0., available.width, available.height), None);
        // A pane is a bounded viewport even when its selected content is short.
        // Measure the original subtree at its inner width, but reserve all of
        // the flexible slot rather than letting a short label collapse the card.
        self.stack.measure(text, layout.content().size);
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let layout = self.container(frame, Some(clip));
        self.stack.arrange(
            text,
            layout.content(),
            layout.visible_clip().unwrap_or_default(),
        );
        self.layout = Some(layout);
    }
}
