use super::*;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};
use tekes_ui_scene::{Point, Scene};
use tekes_ui_tokens::proposal;

/// ClientKit Jump display policy only. The SDK retains all scrolling/animation.
#[derive(Debug, Default)]
pub struct RunJumpVisibility {
    near_bottom: Option<bool>,
    disclosures: BTreeSet<u64>,
    pending_inset: Option<f32>,
}
impl RunJumpVisibility {
    pub fn shows_button(&self) -> bool {
        self.near_bottom == Some(false) && self.disclosures.is_empty()
    }
    pub fn incoming_document(&mut self) {
        self.near_bottom = None;
        self.pending_inset = None;
    }
    pub fn update_geometry(&mut self, bottom_distance: f32, inset_bottom: f32) {
        if !bottom_distance.is_finite() || !inset_bottom.is_finite() {
            return;
        }
        if self
            .pending_inset
            .is_some_and(|pending| inset_bottom >= pending - 0.5)
        {
            self.pending_inset = None;
        }
        let near = bottom_distance <= 40.;
        if !near || self.pending_inset.is_none() {
            self.near_bottom = Some(near);
        }
    }
    pub fn disclosure(&mut self, id: u64, active: bool) {
        if active {
            self.disclosures.insert(id);
        } else {
            self.disclosures.remove(&id);
        }
    }
    pub fn cancel_pending_inset(&mut self) {
        self.pending_inset = None;
    }
    pub fn preserved_occlusion_change(
        &mut self,
        distance: Option<f32>,
        delta: f32,
        expected_inset: f32,
    ) {
        if !delta.is_finite() || !expected_inset.is_finite() {
            return;
        }
        if delta > 0.5 {
            self.pending_inset = Some(expected_inset.max(0.));
            self.near_bottom = Some(false);
        } else {
            self.pending_inset = None;
            if let Some(distance) = distance.filter(|d| d.is_finite()) {
                self.near_bottom = Some((distance + delta).max(0.) <= 40.);
            }
        }
    }
}
/// Event-time local navigation. Not endpoint or host-history authority.
#[derive(Debug, Clone)]
pub struct RunJumpLatest {
    root: ComponentId,
    viewport: ComponentId,
    generation: u64,
    lifetime: Arc<()>,
    claimed: Arc<AtomicBool>,
}
impl RunJumpLatest {
    pub fn root(&self) -> ComponentId {
        self.root
    }
    pub fn viewport(&self) -> ComponentId {
        self.viewport
    }
}
#[derive(Debug)]
struct JumpButton {
    button: Button,
    visible: bool,
    prototype: Option<RunJumpLatest>,
    pending: Vec<RunJumpLatest>,
    frame: Rect,
}
impl JumpButton {
    fn capture(&mut self, response: ContentResponse) -> ContentResponse {
        if response.activated
            && let Some(prototype) = &self.prototype
        {
            let mut intent = prototype.clone();
            intent.claimed = Arc::new(AtomicBool::new(false));
            self.pending.push(intent);
        }
        response
    }
}
impl Component for JumpButton {
    fn active(&self) -> bool {
        self.visible
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, _: Size) -> MeasuredChild {
        let side = proposal::metric("size", "size-icon-button").expect("Jump hit size token");
        MeasuredChild::new(Size::new(side, side))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.frame = frame;
        self.button.arrange(text, frame, clip);
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.button.paint(scene, text, frame, clip, theme);
    }
    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        let response = self.button.event(event);
        self.capture(response)
    }
    fn accessibility_action(
        &mut self,
        part: Option<usize>,
        action: &AccessibilityAction,
    ) -> ContentResponse {
        let response = self.button.accessibility_action(part, action);
        self.capture(response)
    }
    fn focusable(&self) -> bool {
        self.visible && self.button.focusable()
    }
    fn set_focus(&mut self, focused: bool, visible: bool) {
        self.button.set_focus(focused, visible);
    }
    fn focus_decoration_above_siblings(&self) -> bool {
        self.button.focus_decoration_above_siblings()
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Button, "回到最新")
    }
}
impl RunPort {
    /// Exclusive namespace, independent of rail and Transcript row pools.
    pub fn install_jump_to_latest(&mut self, namespace: u32) -> Result<(), &'static str> {
        if self.jump.is_some() {
            return Err("Run Jump already installed");
        }
        if self
            .transcript()
            .is_none_or(|t| !t.namespaces_disjoint(&[namespace]))
            || self
                .body
                .children
                .iter()
                .any(|n| n.id().0 >> 32 == u64::from(namespace))
            || self.ids.root.0 >> 32 == u64::from(namespace)
            || self
                .composer()
                .context()
                .is_some_and(|c| c.id().0 >> 32 == u64::from(namespace))
        {
            return Err("Run Jump namespace overlap");
        }
        let id = ComponentId(u64::from(namespace) << 32);
        let mut button = Button::new("↓");
        button.button.frame_policy = tekes_ui_controls::ButtonFramePolicy::Exact;
        button.pointer_activation_phase = ButtonActivationPhase::Press;
        self.body.children.push(
            Content::new(
                id,
                JumpButton {
                    button,
                    visible: false,
                    prototype: None,
                    pending: vec![],
                    frame: Rect::default(),
                },
            )
            .preserving_focus(),
        );
        self.jump = Some(id);
        Ok(())
    }
    pub fn jump_button_id(&self) -> Option<ComponentId> {
        self.jump
    }
    pub fn jump_visibility_mut(&mut self) -> &mut RunJumpVisibility {
        &mut self.jump_visibility
    }
    pub fn jump_is_visible(&self) -> bool {
        self.jump_control().is_some_and(|b| b.visible)
            && self.pane_presented
            && self.navigation_mounted
    }
    fn jump_control(&self) -> Option<&JumpButton> {
        self.body
            .children
            .iter()
            .find(|n| Some(n.id()) == self.jump)?
            .component()
    }
    pub(super) fn jump_permits(&self, id: ComponentId) -> bool {
        Some(id) == self.jump
            && self.jump_is_visible()
            && self
                .jump_control()
                .and_then(|b| b.prototype.as_ref())
                .is_some_and(|p| {
                    p.generation == self.publication_generation
                        && Arc::ptr_eq(&p.lifetime, &self.navigation_lifetime)
                })
    }
    pub(super) fn jump_at(&self, point: Point) -> bool {
        self.jump.is_some_and(|id| self.jump_permits(id))
            && self.jump_control().is_some_and(|b| b.frame.contains(point))
    }
    pub fn claim_jump_to_latest(&self, intent: &RunJumpLatest) -> Result<(), &'static str> {
        if intent.root != self.ids.root
            || intent.viewport != self.ids.viewport
            || intent.generation != self.publication_generation
            || !Arc::ptr_eq(&intent.lifetime, &self.navigation_lifetime)
            || !self.jump_is_visible()
        {
            return Err("stale or foreign Run Jump");
        }
        intent
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Run Jump already dispatched")?;
        Ok(())
    }
    pub(super) fn take_jump(&mut self) -> Vec<RunJumpLatest> {
        let Some(id) = self.jump else {
            return vec![];
        };
        std::mem::take(
            &mut self
                .body
                .children
                .iter_mut()
                .find(|n| n.id() == id)
                .unwrap()
                .component_mut::<JumpButton>()
                .unwrap()
                .pending,
        )
    }
    pub(super) fn arrange_jump(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        composer: Option<Rect>,
    ) {
        let Some(id) = self.jump else {
            return;
        };
        let geometry = self.geometry.unwrap();
        let viewport = self
            .body
            .children
            .iter()
            .find(|n| n.id() == self.ids.viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap();
        if let Some(document) = viewport.children().first() {
            self.jump_visibility.update_geometry(
                document.frame().max_y()
                    - geometry.document_bottom_clearance
                    - geometry.visible_document.max_y(),
                geometry.document_bottom_clearance,
            );
        }
        let visible = self.navigation_mounted
            && self
                .last
                .as_ref()
                .is_some_and(|s| !s.transcript.turns.is_empty())
            && self.jump_visibility.shows_button();
        let prototype = visible.then(|| RunJumpLatest {
            root: self.ids.root,
            viewport: self.ids.viewport,
            generation: self.publication_generation,
            lifetime: self.navigation_lifetime.clone(),
            claimed: Arc::new(AtomicBool::new(false)),
        });
        let side = proposal::metric("size", "size-icon-button").expect("Jump hit size token");
        // Pane centered, immediately above visual controls; regular padding is
        // part of Jump's host, not the document's loose occlusion clearance.
        let bottom = composer.map_or(frame.max_y(), |c| c.min_y());
        let rect = Rect::new(
            frame.min_x() + ((frame.size.width - side) / 2.).max(0.),
            bottom - Spacing::Regular.value() - side,
            side,
            side,
        );
        let node = self
            .body
            .children
            .iter_mut()
            .find(|n| n.id() == id)
            .unwrap();
        let button = node.component_mut::<JumpButton>().unwrap();
        button.visible = visible;
        button.prototype = prototype;
        node.arrange(text, rect, clip);
    }
}
