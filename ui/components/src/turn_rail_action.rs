//! Agent tick decoration and immutable gesture capture. Original CompoundButton
//! retains pointer arm/cancel, keyboard, focus, AX, chrome and slot ownership.
use crate::{RunTurnNavigation, TurnRailGeometry};
use tekes_ui_controls::{
    ButtonTextRenderer, composition::*, layout::MeasuredChild, paint::ButtonVariant,
};
use tekes_ui_scene::{Quad, Rect, Scene, Size};
#[derive(Debug)]
pub(crate) struct Tick {
    pub centers: bool,
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
        if let Some(ink) = geometry.tick_rect(0, TurnRailGeometry::RESTING_LENGTH) {
            s.push_quad(Quad::new(
                ink.offset(f.min_x(), f.min_y()),
                t.foreground
                    .unwrap_or_else(|| t.palette.color("primary-text").expect("library text role")),
            ));
        }
    }
}
#[derive(Debug)]
pub(crate) struct TurnRailAction {
    pub button: CompoundButton,
    pub prototype: Option<RunTurnNavigation>,
    pub pending: Vec<RunTurnNavigation>,
}
impl TurnRailAction {
    pub fn new(decoration: ComponentId) -> Self {
        let mut button =
            CompoundButton::new("回合", Content::new(decoration, Tick { centers: false }))
                .expect("one passive Agent tick");
        button.action.variant = ButtonVariant::Quiet;
        // CANDIDATE_COMPACT_ACTION_CONFIGURATION
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
