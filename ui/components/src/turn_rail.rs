//! Agent turn identities and ClientKit rail placement. This module does not
//! implement input handling, scrolling, focus, animation or a second Run state.
use crate::{RunColumnLayout, RunScope};
use tekes_ui_controls::composition::ComponentId;
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{ControlSize, Spacing};

/// A turn in the installed Transcript, with its actual retained row identities.
/// This is a local navigation target, never endpoint or mutation authority.
#[derive(Clone, Debug)]
pub struct TranscriptTurnTarget {
    pub(crate) owner: std::sync::Arc<()>,
    pub(crate) scope: RunScope,
    pub(crate) publication: u64,
    pub(crate) epoch: u64,
    pub(crate) turn_id: String,
    pub(crate) preview: crate::TranscriptTurnPreview,
    pub(crate) status: String,
    pub(crate) rows: Vec<ComponentId>,
    pub(crate) visible: Option<bool>,
}
impl TranscriptTurnTarget {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn connection_epoch(&self) -> u64 {
        self.epoch
    }
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
    pub fn input(&self) -> &str {
        &self.preview.input
    }
    pub fn preview(&self) -> &crate::TranscriptTurnPreview {
        &self.preview
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    /// Ordered real Content IDs, including input/output/tool/unplaced/footer
    /// rows belonging to this turn. No synthetic rows or ordinal identities.
    pub fn row_ids(&self) -> &[ComponentId] {
        &self.rows
    }
    /// None until the installed publication has been arranged. Loaded history
    /// is not the same as visible history; a model update retires old geometry.
    pub fn is_visible(&self) -> Option<bool> {
        self.visible
    }
}

/// Pane-space rail and track placement from the corresponding ClientKit View.
/// The library must own the track's independent scrolling and reveal commands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnRailGeometry {
    pub rail: Rect,
    pub track: Rect,
    pub document_size: Size,
    pub centers_ticks: bool,
}
impl TurnRailGeometry {
    pub const ROW_HEIGHT: f32 = 10.;
    pub const RESTING_LENGTH: f32 = 6.;
    pub const MAXIMUM_LENGTH: f32 = 26.;
    /// 1x convenience for static proposals. Mounted hosts use their actual scale.
    pub fn minimum_regular_width() -> f32 {
        Self::minimum_regular_width_at_scale(1.)
    }
    pub fn minimum_regular_width_at_scale(backing_scale: f32) -> f32 {
        let gutter = focus_gutter(backing_scale);
        Spacing::Regular.value().max(gutter)
            + RunColumnLayout::rail_width().max(Spacing::Loose.value() + Self::MAXIMUM_LENGTH)
            + gutter
    }
    /// 1x convenience for static proposals; track is the SDK scroll viewport,
    /// document_size is the tick rows excluding the SDK's focus gutter.
    pub fn for_run(pane: Rect, column: RunColumnLayout, count: usize) -> Self {
        Self::for_run_at_scale(pane, column, count, 1.)
    }
    /// RunChrome uses the leading fraction, independently of full-width body
    /// mode and trailing fraction. Reserve the public SDK focus bleed using the
    /// actual host scale; its backing-pixel guard is not a product constant.
    pub fn for_run_at_scale(
        pane: Rect,
        column: RunColumnLayout,
        count: usize,
        backing_scale: f32,
    ) -> Self {
        let width = finite(pane.size.width);
        let height = finite(pane.size.height);
        let rail_column = match column {
            RunColumnLayout::Insets { leading, .. } => {
                RunColumnLayout::document_pane(leading, None)
            }
            _ => RunColumnLayout::default(),
        };
        let available = rail_column.geometry(width).leading.clamp(0., width);
        let gutter = focus_gutter(backing_scale);
        let regular = available >= Self::minimum_regular_width_at_scale(backing_scale);
        let x = if regular {
            Spacing::Regular.value().max(gutter) - gutter
        } else {
            0.
        };
        let document_width = if regular {
            RunColumnLayout::rail_width().max(Spacing::Loose.value() + Self::MAXIMUM_LENGTH)
        } else {
            (available - 2. * gutter).max(0.)
        };
        let track_width = if regular {
            document_width + 2. * gutter
        } else {
            available
        };
        let content_height = count as f32 * Self::ROW_HEIGHT;
        let visible_height = if count == 0 {
            0.
        } else {
            (content_height + 2. * gutter).min(height * 0.7)
        };
        Self {
            rail: Rect::new(pane.min_x(), pane.min_y(), available, height),
            track: Rect::new(
                pane.min_x() + x,
                pane.min_y() + (height - visible_height) / 2.,
                track_width,
                visible_height,
            ),
            document_size: Size::new(document_width, content_height),
            centers_ticks: !regular,
        }
    }
    /// Document-local ink bounds. This does not subtract or write scroll offset.
    pub fn tick_rect(&self, index: usize, requested_length: f32) -> Option<Rect> {
        if index as f32 * Self::ROW_HEIGHT >= self.document_size.height {
            return None;
        }
        let leading = if self.centers_ticks {
            0.
        } else {
            Spacing::Loose.value()
        };
        let length = finite(requested_length).min((self.document_size.width - leading).max(0.));
        let x = if self.centers_ticks {
            (self.document_size.width - length) / 2.
        } else {
            leading
        };
        let thickness = ControlSize::FocusBar.value();
        Some(Rect::new(
            x,
            index as f32 * Self::ROW_HEIGHT + (Self::ROW_HEIGHT - thickness) / 2.,
            length,
            thickness,
        ))
    }
}
fn focus_gutter(backing_scale: f32) -> f32 {
    let frame = Rect::new(0., 0., 1., 1.);
    let bounds =
        tekes_ui_controls::extended::ControlFrame::new(frame, None).focus_bounds(backing_scale);
    (frame.min_x() - bounds.min_x()).max(0.)
}
fn finite(value: f32) -> f32 {
    if value.is_finite() { value.max(0.) } else { 0. }
}

/// Local conversation navigation. The last turn uses the existing library
/// return-to-latest path, matching ClientKit's last-section behavior. Other
/// turns require START alignment of their first actual row. Hosts must report
/// unsupported until their fixed library supports that placement; Nearest and
/// Centered are not substitutes for the corresponding ClientKit View.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunTurnNavigationKind {
    TurnStart(ComponentId),
    Latest,
}
/// Host display fact for the main Run, published after a successful render.
/// This is bottom visibility, not the library's sticky following-latest intent.
#[derive(Clone, Copy, Debug)]
pub struct RunTurnNavigationContext {
    pub bottom_is_visible: bool,
}
/// Event-time main-thread history stop. Hosts own the navigation stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunTurnJump {
    pub(crate) scope: RunScope,
    pub(crate) from: Option<String>,
    pub(crate) to: String,
}
impl RunTurnJump {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn from_turn(&self) -> Option<&str> {
        self.from.as_deref()
    }
    pub fn to_turn(&self) -> &str {
        &self.to
    }
}
/// Same ordering as ClientKit's recordTurnJump then takePendingTurnReveal.
/// Callbacks do not grant endpoint authority or implement a second Run state.
pub trait RunTurnNavigationHost {
    fn record_turn_jump(&mut self, jump: &RunTurnJump);
    fn take_pending_turn_reveal(&mut self, scope: &RunScope) -> Option<String>;
}
/// An original, non-focusing gesture. Clones share one dispatch admission;
/// neither this value nor its row ID grants endpoint/mutation authority.
#[derive(Clone, Debug)]
pub struct RunTurnNavigation {
    pub(crate) jump: Option<RunTurnJump>,
    pub(crate) target: TranscriptTurnTarget,
    pub(crate) kind: RunTurnNavigationKind,
    pub(crate) generation: u64,
    pub(crate) lifetime: std::sync::Arc<()>,
    pub(crate) claimed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) viewport: ComponentId,
}
impl RunTurnNavigation {
    pub fn host_jump(&self) -> Option<&RunTurnJump> {
        self.jump.as_ref()
    }
    pub fn target(&self) -> &TranscriptTurnTarget {
        &self.target
    }
    pub fn kind(&self) -> RunTurnNavigationKind {
        self.kind
    }
    pub fn viewport(&self) -> ComponentId {
        self.viewport
    }
}
