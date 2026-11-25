//! Responsive width breakpoints shared across AppHost-embeddable Agent components.

/// Breakpoint in logical points distinguishing narrow `others` drawers from wide `focus` slots.
pub const COMPACT_WIDTH_BREAKPOINT: f32 = 520.0;

/// Adaptive layout density derived solely from container allocation width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponsiveWidthMode {
    /// Narrow column (`< 520 pt`), typically hosted in `AppHost` `others` or compact sidebar slots.
    Compact,
    /// Wide workspace (`>= 520 pt`), typically hosted in `AppHost` `focus` or expanded drawers.
    Expanded,
}

impl ResponsiveWidthMode {
    pub fn from_width(width: f32) -> Self {
        if width.is_finite() && width > 0.0 && width < COMPACT_WIDTH_BREAKPOINT {
            Self::Compact
        } else {
            Self::Expanded
        }
    }

    pub fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }
}
