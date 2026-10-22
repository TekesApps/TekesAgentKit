//! Agent conversation-column rules from ClientKit's corresponding Views.
//! All placement/clipping and visual tokens remain TekesUI-owned.
use tekes_ui_controls::{
    extended::Axis,
    layout::{AlignmentOverride, ContainerLayout, Insets, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{ControlSize, Spacing, proposal};

/// One horizontal column shared by Transcript, Composer and Run controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RunColumnLayout {
    /// ClientKit's bounded reading column; the maximum defaults to 770 points.
    Reading { maximum_width: f32 },
    /// ClientKit's full-width branch, retaining rail clearance on both sides.
    FullWidth,
    /// ClientKit document-pane fractions. Nonfinite values become 0; finite
    /// values clamp to 0..=0.45 independently, including asymmetric margins.
    Insets { leading: f32, trailing: f32 },
}
/// Pane-local logical points. A too-small host retains the 320-point reading
/// column rather than silently shrinking it; hosts consume minimum_pane_width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunColumnGeometry {
    pub leading: f32,
    pub width: f32,
}
fn fraction(n: f32) -> f32 {
    if n.is_finite() { n.clamp(0., 0.45) } else { 0. }
}
impl Default for RunColumnLayout {
    fn default() -> Self {
        Self::Reading {
            maximum_width: Self::DEFAULT_MAXIMUM_WIDTH,
        }
    }
}
impl RunColumnLayout {
    pub const MINIMUM_CONTENT_WIDTH: f32 = 320.;
    pub const DEFAULT_MAXIMUM_WIDTH: f32 = 770.;
    /// Corresponding View input precedence: explicit leading fractions win over
    /// full width; a trailing fraction alone does not enable document mode.
    pub fn from_view_inputs(full_width: bool, leading: Option<f32>, trailing: Option<f32>) -> Self {
        if let Some(leading) = leading {
            Self::document_pane(leading, trailing)
        } else if full_width {
            Self::FullWidth
        } else {
            Self::default()
        }
    }
    /// Source input: omitting trailing uses the requested leading fraction.
    pub fn document_pane(leading: f32, trailing: Option<f32>) -> Self {
        Self::Insets {
            leading: fraction(leading),
            trailing: fraction(trailing.unwrap_or(leading)),
        }
    }

    /// Source rule: mark plus hit slop on both sides, using library tokens.
    pub fn rail_width() -> f32 {
        ControlSize::FocusBar.value() + 2. * Spacing::Loose.value()
    }
    pub fn horizontal_inset() -> f32 {
        Self::rail_width() + Spacing::Loose.value()
    }
    pub(crate) fn normalized(self) -> Result<Self, &'static str> {
        match self {
            Self::Reading { maximum_width }
                if !maximum_width.is_finite() || maximum_width < Self::MINIMUM_CONTENT_WIDTH =>
            {
                Err("invalid RunPort reading width")
            }
            Self::Insets { leading, trailing } => Ok(Self::Insets {
                leading: fraction(leading),
                trailing: fraction(trailing),
            }),
            _ => Ok(self),
        }
    }
    /// A host floor that makes the source's 320-point Composer contract usable.
    /// It does not change the full-width/fraction branches' geometry when the
    /// host supplies a smaller pane.
    pub(crate) fn minimum_pane_width(self) -> f32 {
        match self {
            Self::Insets { leading, trailing } => {
                Self::MINIMUM_CONTENT_WIDTH / (1. - leading - trailing)
            }
            _ => Self::MINIMUM_CONTENT_WIDTH + 2. * Self::horizontal_inset(),
        }
    }
    pub(crate) fn geometry(self, viewport_width: f32) -> RunColumnGeometry {
        let viewport = if viewport_width.is_finite() {
            viewport_width.max(0.)
        } else {
            0.
        };
        match self {
            Self::Reading { maximum_width } => {
                let width = (viewport - 2. * Self::horizontal_inset())
                    .max(Self::MINIMUM_CONTENT_WIDTH)
                    .min(maximum_width);
                RunColumnGeometry {
                    leading: Self::horizontal_inset().max((viewport - width) * 0.5),
                    width,
                }
            }
            Self::FullWidth => RunColumnGeometry {
                leading: Self::horizontal_inset(),
                width: (viewport - 2. * Self::horizontal_inset()).max(0.),
            },
            Self::Insets { leading, trailing } => RunColumnGeometry {
                leading: viewport * leading,
                width: (viewport - viewport * leading - viewport * trailing).max(0.),
            },
        }
    }
    pub(crate) fn frame(self, bounds: Rect) -> Rect {
        let normalized = ContainerLayout::new(bounds, Insets::ZERO, Insets::ZERO, None).content();
        let column = self.geometry(normalized.size.width);
        // Source minimum-width overflow is deliberate. A spacer expresses the
        // pane-local leading edge without asking ContainerLayout to consume
        // an inset larger than the pane. The library owns the actual placement.
        layout_measured_children(
            normalized,
            Axis::Horizontal,
            &[
                MeasuredChild::new(Size::new(column.leading, normalized.size.height)),
                MeasuredChild::new(Size::new(column.width, normalized.size.height)),
            ],
            0.,
            proposal::layout::ROW,
            AlignmentOverride {
                main_axis: Some(proposal::layout::MainAxisDistribution::Start),
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Leading),
            },
        )[1]
        .frame
    }
}
