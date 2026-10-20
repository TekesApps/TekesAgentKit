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
    pub(crate) input: String,
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
        &self.input
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
    pub fn minimum_regular_width() -> f32 {
        Spacing::Regular.value()
            + RunColumnLayout::rail_width().max(Spacing::Loose.value() + Self::MAXIMUM_LENGTH)
    }
    /// RunChrome uses the leading fraction, independently of full-width body
    /// mode and trailing fraction. No explicit fraction means default reading.
    pub fn for_run(pane: Rect, column: RunColumnLayout, count: usize) -> Self {
        let width = finite(pane.size.width);
        let height = finite(pane.size.height);
        let rail_column = match column {
            RunColumnLayout::Insets { leading, .. } => {
                RunColumnLayout::document_pane(leading, None)
            }
            _ => RunColumnLayout::default(),
        };
        let available = rail_column.geometry(width).leading.clamp(0., width);
        let regular = available >= Self::minimum_regular_width();
        let x = if regular {
            Spacing::Regular.value()
        } else {
            0.
        };
        let track_width = if regular {
            Self::minimum_regular_width() - x
        } else {
            available
        };
        let content_height = count as f32 * Self::ROW_HEIGHT;
        let visible_height = content_height.min(height * 0.7);
        Self {
            rail: Rect::new(pane.min_x(), pane.min_y(), available, height),
            track: Rect::new(
                pane.min_x() + x,
                pane.min_y() + (height - visible_height) / 2.,
                track_width,
                visible_height,
            ),
            document_size: Size::new(track_width, content_height),
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
/// An original, non-focusing gesture. Clones share one dispatch admission;
/// neither this value nor its row ID grants endpoint/mutation authority.
#[derive(Clone, Debug)]
pub struct RunTurnNavigation {
    pub(crate) target: TranscriptTurnTarget,
    pub(crate) kind: RunTurnNavigationKind,
    pub(crate) generation: u64,
    pub(crate) lifetime: std::sync::Arc<()>,
    pub(crate) claimed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) viewport: ComponentId,
}
impl RunTurnNavigation {
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
