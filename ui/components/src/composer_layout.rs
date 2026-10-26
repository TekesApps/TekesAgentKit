//! Agent content placement only; all controls and editing stay in TekesUI.
use crate::{AgentText, ComposerDestination, ComposerIds};
use tekes_ui_controls::feedback_view::SurfaceView;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::Axis,
    layout::{AlignmentOverride, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Spacing, Stroke, proposal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerPrimaryActionKind {
    Send,
    Stop,
}

/// Geometry of the installed Composer's last arrange. The setup row remains
/// an experimental inline host slot; it is not a native window toolbar.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerLayoutGeometry {
    pub publication: u64,
    pub destination: Option<ComposerDestination>,
    pub notice: Option<Rect>,
    pub context: Option<Rect>,
    pub input_band: Rect,
    pub editor: Rect,
    pub primary: Rect,
    pub attachment: Option<Rect>,
    pub setup: Option<Rect>,
}

/// The actual retained Agent action controls. This is not a generic button or
/// layout engine: role grouping and placements use the library layout helper.
#[derive(Debug)]
pub(crate) struct ComposerActions {
    pub children: Vec<Content>,
    pub ids: ComposerIds,
    pub primary: ComponentId,
    pub input_frame: Rect,
    pub setup_frame: Rect,
}
impl ComposerActions {
    pub fn new(ids: ComposerIds) -> Self {
        Self {
            children: vec![],
            ids,
            primary: ids.send,
            input_frame: Rect::default(),
            setup_frame: Rect::default(),
        }
    }
    pub fn group(&mut self, text: &mut dyn ButtonTextRenderer, width: f32, input: bool) -> Size {
        let mut size = Size::default();
        let mut count = 0usize;
        for node in &mut self.children {
            if node.id() == self.ids.editor
                || (node.id() == self.primary || node.id() == self.ids.attachment) != input
            {
                continue;
            }
            let measured = node.measure(text, Size::new(width, 1e8)).size;
            size.width += measured.width;
            size.height = size.height.max(measured.height);
            count += 1;
        }
        size.width += count.saturating_sub(1) as f32
            * if input {
                Stroke::Rule.value()
            } else {
                Spacing::Tight.value()
            };
        size
    }
}
impl Component for ComposerActions {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, _: Rect, clip: Rect) {
        for input in [true, false] {
            let bounds = if input {
                self.input_frame
            } else {
                self.setup_frame
            };
            let measured: Vec<_> = self
                .children
                .iter_mut()
                .filter(|node| {
                    node.id() != self.ids.editor
                        && (node.id() == self.primary || node.id() == self.ids.attachment) == input
                })
                .map(|node| node.measure(text, bounds.size))
                .collect();
            let placements = layout_measured_children(
                bounds,
                Axis::Horizontal,
                &measured,
                if input {
                    Stroke::Rule.value()
                } else {
                    Spacing::Tight.value()
                },
                proposal::layout::ROW,
                AlignmentOverride {
                    main_axis: Some(if input {
                        proposal::layout::MainAxisDistribution::End
                    } else {
                        proposal::layout::MainAxisDistribution::Start
                    }),
                    cross_axis: Some(proposal::layout::CrossAxisAlignment::Center),
                },
            );
            for (node, placement) in self
                .children
                .iter_mut()
                .filter(|node| {
                    node.id() != self.ids.editor
                        && (node.id() == self.primary || node.id() == self.ids.attachment) == input
                })
                .zip(placements)
            {
                node.arrange(text, placement.frame, clip);
            }
        }
    }
}

/// Actual input-band subtree. Only business placement and the original
/// library surface live here; focus, hit, caret and IME routing stay in TekesUI.
#[derive(Debug)]
pub(crate) struct ComposerInputBand {
    pub actions: ComposerActions,
    pub editor_frame: Rect,
    pub row_center: f32,
    pub actions_size: Size,
    surface: SurfaceView,
}
impl ComposerInputBand {
    pub fn new(ids: ComposerIds) -> Self {
        let mut actions = ComposerActions::new(ids);
        actions
            .children
            .push(Content::new(ids.editor, AgentText::composer()));
        Self {
            actions,
            editor_frame: Rect::default(),
            row_center: 0.,
            actions_size: Size::default(),
            surface: SurfaceView::default(),
        }
    }
    pub fn editor(&self) -> &Content {
        self.actions
            .children
            .iter()
            .find(|n| n.id() == self.actions.ids.editor)
            .unwrap()
    }
    pub fn editor_mut(&mut self) -> &mut Content {
        let id = self.actions.ids.editor;
        self.actions
            .children
            .iter_mut()
            .find(|n| n.id() == id)
            .unwrap()
    }
}
impl Component for ComposerInputBand {
    fn fixed_slots(&self) -> bool {
        true
    }
    fn children(&self) -> &[Content] {
        &self.actions.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.actions.children)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let editor_frame = self.editor_frame;
        self.editor_mut().arrange(text, editor_frame, clip);
        let editor = &self
            .editor()
            .component::<AgentText>()
            .unwrap()
            .field
            .surface;
        let line_height = editor.role.style().line_height * editor.type_scale;
        let center = self
            .row_center
            .max(editor.inner.max_y() - line_height / 2.)
            .min(frame.max_y() - self.actions_size.height / 2.);
        let trailing = (frame.max_y() - center - self.actions_size.height / 2.).max(0.);
        self.actions.input_frame = Rect::new(
            frame.max_x() - trailing - self.actions_size.width,
            center - self.actions_size.height / 2.,
            self.actions_size.width,
            self.actions_size.height,
        );
        self.actions.arrange(text, frame, clip);
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.surface.render_at_scale(
            scene,
            frame,
            Some(clip),
            theme.palette,
            true,
            text.backing_scale(),
        );
    }
}

pub(crate) struct ComposerMeasurements {
    pub notice: Size,
    pub context: Size,
    pub editor: Size,
    pub actions: Size,
    pub setup: Size,
    pub row_height: f32,
}
impl ComposerMeasurements {
    pub fn notice_clearance(&self) -> f32 {
        if self.notice.height > 0. {
            self.notice.height + Spacing::Snug.value()
        } else {
            0.
        }
    }
    pub fn context_clearance(&self) -> f32 {
        if self.context.height > 0. {
            self.context.height + Spacing::Regular.value()
        } else {
            0.
        }
    }
    pub fn band_height(&self) -> f32 {
        self.row_height + 2. * Spacing::Snug.value()
    }
    pub fn height(&self) -> f32 {
        self.notice_clearance()
            + self.context_clearance()
            + self.band_height()
            + if self.setup.height > 0. {
                Spacing::Snug.value() + self.setup.height
            } else {
                0.
            }
    }
}
