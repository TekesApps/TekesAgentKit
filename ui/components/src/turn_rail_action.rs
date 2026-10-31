//! Agent tick decoration and immutable gesture capture. Original Button
//! retains pointer arm/cancel, keyboard, focus, AX, chrome and slot ownership.
use crate::{RunTurnNavigation, TurnRailGeometry};
use std::sync::{Arc, Mutex};
use tekes_ui_controls::{
    ButtonTextRenderer, composition::*, layout::MeasuredChild, paint::ButtonVariant,
};
use tekes_ui_scene::{Quad, Rect, Scene, Size};
#[derive(Debug)]
pub(crate) struct Tick {
    pub index: usize,
    pub centers: bool,
    pub turn_id: Option<String>,
    pub visible: bool,
    pub hover: Arc<Mutex<HoverVisual>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HoverVisual {
    pub displayed_position: Option<f32>,
    pub influence: f32,
    pub hovered_turn_id: Option<String>,
}
impl Component for Tick {
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(a.width, TurnRailGeometry::ROW_HEIGHT))
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        _: &mut dyn ButtonTextRenderer,
        f: Rect,
        _: Rect,
        t: ContentTheme,
    ) {
        let geometry = TurnRailGeometry {
            rail: f,
            track: f,
            document_size: Size::new(f.size.width, TurnRailGeometry::ROW_HEIGHT),
            centers_ticks: self.centers,
        };
        let hover = self
            .hover
            .lock()
            .expect("turn rail hover lock poisoned")
            .clone();
        let requested_length = tick_length(self.index, &hover);
        if let Some(ink) = geometry.tick_rect(0, requested_length) {
            let opacity = if hover.hovered_turn_id.is_some() {
                if hover.hovered_turn_id.as_deref() == self.turn_id.as_deref() {
                    0.8
                } else {
                    0.3
                }
            } else if self.visible {
                0.8
            } else {
                0.3
            };
            let color = t
                .foreground
                .unwrap_or_else(|| t.palette.color("primary-text").expect("library text role"));
            s.push_quad(Quad::new(
                ink.offset(f.min_x(), f.min_y()),
                color.with_alpha(color.a * opacity),
            ));
        }
    }
}
pub(crate) fn tick_length(index: usize, hover: &HoverVisual) -> f32 {
    let Some(position) = hover.displayed_position.filter(|_| hover.influence > 0.) else {
        return TurnRailGeometry::RESTING_LENGTH;
    };
    let distance = (index as f32 - position).abs();
    let lower = distance.floor() as usize;
    let progress = distance - lower as f32;
    let eased = progress * progress * (3. - 2. * progress);
    let lengths = [26., 20., 14., 8., TurnRailGeometry::RESTING_LENGTH];
    let lower_length = lengths
        .get(lower)
        .copied()
        .unwrap_or(TurnRailGeometry::RESTING_LENGTH);
    let upper_length = lengths
        .get(lower.saturating_add(1))
        .copied()
        .unwrap_or(TurnRailGeometry::RESTING_LENGTH);
    let hovered = lower_length + (upper_length - lower_length) * eased;
    TurnRailGeometry::RESTING_LENGTH
        + (hovered - TurnRailGeometry::RESTING_LENGTH) * hover.influence
}
#[derive(Debug)]
pub(crate) struct TurnRailAction {
    pub button: Button,
    pub prototype: Option<RunTurnNavigation>,
    pub pending: Vec<RunTurnNavigation>,
}
impl TurnRailAction {
    pub fn new(decoration: ComponentId, hover: Arc<Mutex<HoverVisual>>) -> Self {
        let mut button = Button::new(Content::new(
            decoration,
            Tick {
                index: 0,
                centers: false,
                turn_id: None,
                visible: false,
                hover,
            },
        ))
        .with_accessible_name("回合");
        button.variant = ButtonVariant::Quiet;
        button.button.frame_policy = tekes_ui_controls::ButtonFramePolicy::Exact;
        button.pointer_activation_phase = ButtonActivationPhase::Press;
        Self {
            button,
            prototype: None,
            pending: vec![],
        }
    }
    fn capture(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated
            && let Some(prototype) = &self.prototype
        {
            let mut gesture = prototype.clone();
            gesture.claimed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.pending.push(gesture);
        }
        response
    }
}
impl Component for TurnRailAction {
    fn focus_decoration_above_siblings(&self) -> bool {
        self.button.focus_decoration_above_siblings()
    }
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
    fn child_theme(&self, t: ContentTheme) -> ContentTheme {
        self.button.child_theme(t)
    }
    fn measure(&mut self, t: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        self.button.measure(t, a)
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        self.button.arrange(t, f, c)
    }
    fn paint(
        &mut self,
        s: &mut Scene,
        t: &mut dyn ButtonTextRenderer,
        f: Rect,
        c: Rect,
        p: ContentTheme,
    ) {
        self.button.paint(s, t, f, c, p)
    }
    fn event(&mut self, e: &ContentEvent) -> ContentResponse {
        let r = self.button.event(e);
        self.capture(r)
    }
    fn accessibility_action(
        &mut self,
        p: Option<usize>,
        a: &AccessibilityAction,
    ) -> ContentResponse {
        let r = self.button.accessibility_action(p, a);
        self.capture(r)
    }
    fn focusable(&self) -> bool {
        self.button.focusable()
    }
    fn set_focus(&mut self, f: bool, v: bool) {
        self.button.set_focus(f, v)
    }
    fn semantics(&self) -> ContentSemantics {
        self.button.semantics()
    }
}

// The shared wrapper delegates the retained SDK control.
#[cfg(test)]
mod forwarded_focus_tests {
    use super::*;

    #[test]
    fn actual_turn_rail_delegates_library_focus_decoration_state() {
        let mut action = TurnRailAction::new(ComponentId(999), Arc::new(Mutex::default()));
        action.button.button.frame_policy = tekes_ui_controls::ButtonFramePolicy::Exact;
        assert!(!action.focus_decoration_above_siblings());
        action.set_focus(true, true);
        assert!(action.button.focus_decoration_above_siblings());
        assert!(action.focus_decoration_above_siblings());
        action.button.button.state.disabled = true;
        assert!(!action.button.focus_decoration_above_siblings());
        assert!(!action.focus_decoration_above_siblings());
        action.button.button.state.disabled = false;
        action.set_focus(true, false);
        assert!(!action.button.focus_decoration_above_siblings());
        assert!(!action.focus_decoration_above_siblings());
        action.set_focus(false, false);
        assert!(!action.focus_decoration_above_siblings());
    }
}
