//! Agent conversation-column policy. Geometry/clipping/centering use TekesUI;
//! hosts choose the pane policy, never a second Agent component tree.
use tekes_ui_controls::{
    extended::Axis,
    layout::{AlignmentOverride, ContainerLayout, Insets, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

/// Shared horizontal boundary for Transcript, Composer and Run controls.
/// Width is a reading-layout constraint, not a private visual theme. Insets,
/// typography, radii and control appearance continue to use TekesUI tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RunColumnLayout {
    /// Center a bounded reading column. Default maximum follows ClientKit's
    /// conversation-column contract; hosts can request another reading width.
    Reading { maximum_width: f32 },
    /// Fill the pane after the library's semantic horizontal clearance.
    FullWidth,
    /// Pane-specific fractional margins, matching ClientKit's document-panel
    /// input. Each fraction must be finite and within 0..=0.45.
    Insets { leading: f32, trailing: f32 },
}
impl Default for RunColumnLayout {
    fn default() -> Self {
        Self::Reading {
            maximum_width: 770.,
        }
    }
}
impl RunColumnLayout {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        match self {
            Self::Reading { maximum_width }
                if !maximum_width.is_finite() || maximum_width <= 0. =>
            {
                Err("invalid RunView reading width")
            }
            Self::Insets { leading, trailing }
                if [leading, trailing]
                    .into_iter()
                    .any(|n| !n.is_finite() || !(0. ..=0.45).contains(&n)) =>
            {
                Err("invalid RunView column margins")
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn frame(self, bounds: Rect) -> Rect {
        let normalized = ContainerLayout::new(bounds, Insets::ZERO, Insets::ZERO, None).content();
        let (leading, trailing) = match self {
            Self::Insets { leading, trailing } => (
                normalized.size.width * leading,
                normalized.size.width * trailing,
            ),
            _ => (Spacing::Relaxed.value(), Spacing::Relaxed.value()),
        };
        let inner = ContainerLayout::new(
            normalized,
            Insets::ZERO,
            Insets::new(0., trailing, 0., leading),
            None,
        )
        .content();
        let width = match self {
            Self::Reading { maximum_width } => inner.size.width.min(maximum_width),
            _ => inner.size.width,
        };
        layout_measured_children(
            inner,
            Axis::Vertical,
            &[MeasuredChild::new(Size::new(width, inner.size.height))],
            0.,
            proposal::layout::COLUMN,
            AlignmentOverride {
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Center),
                ..Default::default()
            },
        )[0]
        .frame
    }
}
