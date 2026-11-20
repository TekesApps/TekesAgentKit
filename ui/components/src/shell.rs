//! Transparent single-child shell observing AppHost window activity and arranged width
//! without rebuilding or cloning child `ContentHost` instances on slot swaps.
use crate::layout_mode::ResponsiveWidthMode;
use std::cell::Cell;
use std::rc::Rc;
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Scene, Size};

/// Wraps one container root `Content` to record effective activity and width changes.
#[derive(Debug)]
pub struct AgentContainerShell {
    children: Vec<Content>,
    active_flag: Rc<Cell<bool>>,
    arranged_width: Rc<Cell<f32>>,
    width_mode: Rc<Cell<ResponsiveWidthMode>>,
}

impl AgentContainerShell {
    pub fn new(
        child: Content,
        active_flag: Rc<Cell<bool>>,
        arranged_width: Rc<Cell<f32>>,
        width_mode: Rc<Cell<ResponsiveWidthMode>>,
    ) -> Self {
        Self {
            children: vec![child],
            active_flag,
            arranged_width,
            width_mode,
        }
    }

    pub fn child(&self) -> Option<&Content> {
        self.children.first()
    }

    pub fn child_mut(&mut self) -> Option<&mut Content> {
        self.children.first_mut()
    }

    pub fn is_window_active(&self) -> bool {
        self.active_flag.get()
    }

    pub fn width_mode(&self) -> ResponsiveWidthMode {
        self.width_mode.get()
    }
}

impl Component for AgentContainerShell {
    fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
        CurveSubtreeCapability::Recursive
    }

    fn has_stable_hooks(&self) -> bool {
        true
    }

    fn supports_prepared_lifecycle(&self) -> bool {
        true
    }

    fn fixed_slots(&self) -> bool {
        true
    }

    fn ignores_pointer_actions(&self) -> bool {
        true
    }

    fn host_reveal_passthrough(&self) -> bool {
        self.children.len() == 1
    }

    fn children(&self) -> &[Content] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }

    fn validate(&self) -> Result<(), CompositionError> {
        if self.children.len() != 1 {
            return Err(CompositionError::InvalidSlotCount);
        }
        Ok(())
    }

    fn window_active_changed(&mut self, active: bool) {
        self.active_flag.set(active);
    }

    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if available.width.is_finite() && available.width > 0.0 {
            self.width_mode
                .set(ResponsiveWidthMode::from_width(available.width));
        }
        self.children.first_mut().map_or_else(
            || MeasuredChild::new(Size::default()),
            |c| c.measure(text, available),
        )
    }

    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if frame.size.width.is_finite() && frame.size.width > 0.0 {
            self.arranged_width.set(frame.size.width);
            self.width_mode
                .set(ResponsiveWidthMode::from_width(frame.size.width));
        }
        if let Some(child) = self.children.first_mut() {
            child.arrange(text, frame, clip);
        }
    }

    fn paint(
        &mut self,
        _scene: &mut Scene,
        _text: &mut dyn ButtonTextRenderer,
        _frame: Rect,
        _clip: Rect,
        _theme: ContentTheme,
    ) {
    }
}
