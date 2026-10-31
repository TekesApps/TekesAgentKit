//! Real turn rail over original library scroll and Button controls.
//! No generic hit testing, scrolling, input or animation engine lives here.
use crate::turn_rail_action::{HoverVisual, Tick, TurnRailAction};
use crate::{RunScope, RunTurnNavigation, RunTurnNavigationKind, TurnRailGeometry};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tekes_ui_controls::{
    ButtonTextRenderer, ControlMotionContext, ScrollArea, composition::*, layout::MeasuredChild,
};
use tekes_ui_motion::{Motion, MotionSpec, TimingCurve, state_transition::StateTransition};
use tekes_ui_scene::{Point, Rect, Size};
/// The exact published SDK provides original Exact/Press/preserving-focus APIs.
pub fn turn_rail_supported() -> bool {
    true
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnRailNavigationGate {
    pub turn_start: bool,
    pub latest: bool,
}
impl TurnRailNavigationGate {
    fn allows(self, kind: RunTurnNavigationKind) -> bool {
        match kind {
            RunTurnNavigationKind::TurnStart(_) => self.turn_start,
            RunTurnNavigationKind::Latest => self.latest,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnRailRevealReason {
    InitialLatest,
    FollowChanged,
}
#[derive(Clone, Debug)]
pub struct TurnRailReveal {
    owner: Arc<()>,
    token: Arc<()>,
    revision: u64,
    viewport: ComponentId,
    item: ComponentId,
    turn: String,
    reason: TurnRailRevealReason,
    claimed: Arc<AtomicBool>,
}
impl TurnRailReveal {
    pub fn viewport(&self) -> ComponentId {
        self.viewport
    }
    pub fn item(&self) -> ComponentId {
        self.item
    }
    pub fn reason(&self) -> TurnRailRevealReason {
        self.reason
    }
    pub fn document_revision(&self) -> u64 {
        self.revision
    }
}
#[derive(Debug)]
struct RailDocument {
    rows: Vec<Content>,
}
impl Component for RailDocument {
    fn children(&self) -> &[Content] {
        &self.rows
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        MeasuredChild::new(Size::new(
            a.width,
            self.rows.len() as f32 * TurnRailGeometry::ROW_HEIGHT,
        ))
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        for (i, row) in self.rows.iter_mut().enumerate() {
            row.arrange(
                t,
                Rect::new(
                    f.min_x(),
                    f.min_y() + i as f32 * TurnRailGeometry::ROW_HEIGHT,
                    f.size.width,
                    TurnRailGeometry::ROW_HEIGHT,
                ),
                c,
            );
        }
    }
}
#[derive(Clone, Debug)]
pub struct TurnRailLabels {
    pub name: String,
    pub action: fn(usize, &crate::TranscriptTurnTarget) -> String,
}
impl Default for TurnRailLabels {
    fn default() -> Self {
        Self {
            name: "回合导航".into(),
            action: |index, target| format!("回合 {} · {}", index + 1, target.input()),
        }
    }
}
/// Mount through RunPort::install_turn_rail so source identity and layout stay
/// shared. Original SDK controls retain pointer, focus and keyboard ownership.
#[derive(Debug)]
pub struct TurnRail {
    root: ComponentId,
    labels: TurnRailLabels,
    scope: RunScope,
    children: Vec<Content>,
    ids: BTreeMap<String, (ComponentId, ComponentId)>,
    order: Vec<String>,
    next: u64,
    end: u64,
    geometry: Option<TurnRailGeometry>,
    pending: Vec<RunTurnNavigation>,
    gate: TurnRailNavigationGate,
    owner: Arc<()>,
    revision: u64,
    follow: Option<String>,
    honored: bool,
    initialized: bool,
    reveal: Option<TurnRailReveal>,
    suspended: bool,
    source: Option<RailSource>,
    hover: Arc<Mutex<HoverVisual>>,
    hovered: bool,
    hover_target: f64,
    retarget_hover: bool,
    hover_motion: Option<StateTransition<f64>>,
}
#[derive(Debug)]
struct RailSource {
    scope: RunScope,
    epoch: u64,
    owner: Arc<()>,
}
impl RailSource {
    fn matches(&self, target: &crate::TranscriptTurnTarget) -> bool {
        self.scope == *target.scope()
            && self.epoch == target.connection_epoch()
            && Arc::ptr_eq(&self.owner, &target.owner)
    }
}
impl TurnRail {
    pub(crate) fn new(namespace: u32, scope: RunScope) -> Self {
        let base = (namespace as u64) << 32;
        let mut scroll = ScrollArea::default();
        scroll.set_scroll_policy(
            tekes_ui_controls::ScrollPolicy::Disabled,
            tekes_ui_controls::ScrollPolicy::Enabled,
        );
        scroll.set_scrollbar_visibility(false, false);
        Self {
            root: ComponentId(base),
            labels: Default::default(),
            scope,
            children: vec![Content::new(
                ComponentId(base + 1),
                ScrollContent::new(
                    scroll,
                    Content::new(ComponentId(base + 2), RailDocument { rows: vec![] }),
                ),
            )],
            ids: BTreeMap::new(),
            order: vec![],
            next: base + 16,
            end: base | u32::MAX as u64,
            geometry: None,
            pending: vec![],
            gate: Default::default(),
            owner: Arc::new(()),
            revision: 1,
            follow: None,
            honored: false,
            initialized: false,
            reveal: None,
            suspended: false,
            source: None,
            hover: Arc::new(Mutex::new(HoverVisual::default())),
            hovered: false,
            hover_target: 0.,
            retarget_hover: false,
            hover_motion: None,
        }
    }
    pub fn set_labels(&mut self, labels: TurnRailLabels) {
        self.labels = labels;
        let format = self.labels.action;
        for (index, node) in self.rows_mut().iter_mut().enumerate() {
            let row = node.component_mut::<TurnRailAction>().unwrap();
            if let Some(prototype) = &row.prototype {
                row.button
                    .set_accessible_name(Some(format(index, prototype.target())));
            }
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn viewport_id(&self) -> ComponentId {
        ComponentId(self.root.0 + 1)
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn document_revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn check_membership<'a>(
        &self,
        turns: impl Iterator<Item = &'a str>,
    ) -> Result<(), &'static str> {
        let order: Vec<_> = turns.map(str::to_owned).collect();
        let unique: BTreeSet<_> = order.iter().collect();
        if unique.len() != order.len() || order.iter().any(String::is_empty) {
            return Err("invalid rail membership");
        }
        let needed = order
            .iter()
            .filter(|key| !self.ids.contains_key(*key))
            .count() as u64
            * 2;
        self.next
            .checked_add(needed)
            .filter(|v| *v <= self.end)
            .ok_or("rail ID namespace exhausted")?;
        if order != self.order {
            self.revision
                .checked_add(1)
                .ok_or("rail revision exhausted")?;
        }
        Ok(())
    }
    pub(crate) fn clear_geometry(&mut self) {
        self.geometry = None;
    }
    pub(crate) fn invalidate_layout(&mut self) {
        self.geometry = None;
        self.clear_hover();
        self.owner = Arc::new(());
        self.reveal = None;
        self.suspended = false;
    }
    pub(crate) fn navigation_gate(&self) -> TurnRailNavigationGate {
        self.gate
    }
    pub(crate) fn prototype(&self, id: ComponentId) -> Option<&RunTurnNavigation> {
        self.rows()
            .iter()
            .find(|n| n.id() == id)?
            .component::<TurnRailAction>()?
            .prototype
            .as_ref()
    }
    pub(crate) fn action_at(&self, p: Point) -> Option<ComponentId> {
        self.rows()
            .iter()
            .find(|c| self.permits_id(c.id()) && c.visible().is_some_and(|r| r.contains(p)))
            .map(Content::id)
    }
    pub fn item_ids(&self) -> Vec<ComponentId> {
        self.order.iter().map(|key| self.ids[key].0).collect()
    }
    /// Captured data of the original rail item. Validate it through RunPort
    /// before reading a preview; this is neither overlay nor navigation authority.
    pub fn preview_target(&self, turn: &str) -> Option<crate::TranscriptTurnTarget> {
        self.prototype(self.row_id(turn)?)
            .map(|source| source.target().clone())
    }
    pub fn row_id(&self, turn: &str) -> Option<ComponentId> {
        self.ids.get(turn).map(|v| v.0)
    }
    /// Last library-resolved control frame; requires a successful current render
    /// before use as native publication evidence.
    pub fn action_layout(&self, turn: &str) -> Option<tekes_ui_controls::ButtonLayout> {
        let id = self.row_id(turn)?;
        self.rows()
            .iter()
            .find(|n| n.id() == id)?
            .component::<TurnRailAction>()?
            .button
            .button
            .layout()
    }
    pub fn layout_geometry(&self) -> Option<TurnRailGeometry> {
        self.geometry
    }
    fn rows(&self) -> &[Content] {
        &self.children[0]
            .component::<ScrollContent>()
            .unwrap()
            .children()[0]
            .component::<RailDocument>()
            .unwrap()
            .rows
    }
    fn rows_mut(&mut self) -> &mut Vec<Content> {
        &mut self.children[0]
            .component_mut::<ScrollContent>()
            .unwrap()
            .children_mut()
            .unwrap()[0]
            .component_mut::<RailDocument>()
            .unwrap()
            .rows
    }
    pub(crate) fn refresh(
        &mut self,
        prototypes: Vec<RunTurnNavigation>,
        geometry: TurnRailGeometry,
    ) -> Result<(), &'static str> {
        let mut unique = BTreeSet::new();
        for p in &prototypes {
            if p.target().scope() != &self.scope || !unique.insert(p.target().turn_id().to_owned())
            {
                return Err("foreign or duplicate rail turn");
            }
        }
        let order: Vec<_> = prototypes
            .iter()
            .map(|p| p.target().turn_id().to_owned())
            .collect();
        let next_source = prototypes.first().map(|p| RailSource {
            scope: p.target().scope().clone(),
            epoch: p.target().connection_epoch(),
            owner: p.target().owner.clone(),
        });
        if prototypes.iter().any(|p| {
            next_source
                .as_ref()
                .is_none_or(|source| !source.matches(p.target()))
        }) {
            return Err("mixed rail source");
        }
        let needed = order.iter().filter(|k| !self.ids.contains_key(*k)).count() as u64 * 2;
        let next = self
            .next
            .checked_add(needed)
            .filter(|v| *v <= self.end)
            .ok_or("rail ID namespace exhausted")?;
        let membership_changed = order != self.order;
        let source_changed = match (&self.source, &next_source) {
            (None, None) => false,
            (Some(current), Some(next)) => {
                current.scope != next.scope
                    || current.epoch != next.epoch
                    || !Arc::ptr_eq(&current.owner, &next.owner)
            }
            _ => true,
        };
        let revision = if membership_changed {
            self.revision
                .checked_add(1)
                .ok_or("rail revision exhausted")?
        } else {
            self.revision
        };
        let mut ids = self.ids.clone();
        ids.retain(|k, _| unique.contains(k));
        let mut assigned = self.next;
        for k in &order {
            ids.entry(k.clone()).or_insert_with(|| {
                let pair = (ComponentId(assigned), ComponentId(assigned + 1));
                assigned += 2;
                pair
            });
        }
        let old = std::mem::take(self.rows_mut());
        let mut old: BTreeMap<_, _> = old.into_iter().map(|c| (c.id(), c)).collect();
        let mut rows = vec![];
        for (index, prototype) in prototypes.into_iter().enumerate() {
            let (id, tick) = ids[prototype.target().turn_id()];
            let mut content = old
                .remove(&id)
                .unwrap_or_else(|| rail_action_content(id, tick, self.hover.clone()));
            let action = content.component_mut::<TurnRailAction>().unwrap();
            action
                .button
                .set_accessible_name(Some((self.labels.action)(index, prototype.target())));
            action.button.button.state.selected = prototype.target().is_visible() == Some(true);
            action.button.button.state.disabled = !self.gate.allows(prototype.kind());
            let tick = action.button.children_mut().unwrap()[0]
                .component_mut::<Tick>()
                .unwrap();
            tick.index = index;
            tick.centers = geometry.centers_ticks;
            tick.turn_id = Some(prototype.target().turn_id().to_owned());
            tick.visible = prototype.target().is_visible() == Some(true);
            action.prototype = Some(prototype);
            rows.push(content);
        }
        for (_, mut old) in old {
            self.pending
                .append(&mut old.component_mut::<TurnRailAction>().unwrap().pending);
        }
        *self.rows_mut() = rows;
        self.ids = ids;
        self.order = order;
        self.next = next;
        self.revision = revision;
        self.geometry = Some(geometry);
        if source_changed || membership_changed {
            self.clear_hover();
        }
        self.source = next_source;
        if membership_changed {
            self.reveal = None;
            self.suspended = false;
        }
        if self.order.is_empty() {
            self.initialized = false;
            self.honored = false;
        }
        Ok(())
    }
    pub(crate) fn set_gate(&mut self, gate: TurnRailNavigationGate) {
        self.gate = gate;
        for row in self.rows_mut() {
            let a = row.component_mut::<TurnRailAction>().unwrap();
            a.button.button.state.disabled =
                a.prototype.as_ref().is_none_or(|p| !gate.allows(p.kind()));
        }
    }
    pub fn take_navigation(&mut self) -> Vec<RunTurnNavigation> {
        let mut pending = std::mem::take(&mut self.pending);
        for row in self.rows_mut() {
            pending.append(&mut row.component_mut::<TurnRailAction>().unwrap().pending);
        }
        pending
    }
    pub(crate) fn permits_id(&self, id: ComponentId) -> bool {
        turn_rail_supported()
            && self.rows().iter().any(|c| {
                c.id() == id
                    && !c
                        .component::<TurnRailAction>()
                        .unwrap()
                        .button
                        .button
                        .state
                        .disabled
            })
    }
    /// Feed the shared business target, never pixel offsets. Same target does
    /// not rearm an already honored follow after a user scrolls the rail.
    pub fn set_follow_target(&mut self, turn: Option<String>) {
        if self.follow != turn {
            self.follow = turn;
            self.honored = false;
            self.reveal = None;
            self.suspended = false;
        }
    }
    /// Call only after a successful current frame. Initial mount reveals the
    /// actual last tick; later target changes use the source comfort zone and
    /// centered library reveal. Initial and follow never change Transcript.
    pub fn next_reveal(&mut self) -> Option<TurnRailReveal> {
        if !turn_rail_supported() || self.suspended {
            return None;
        }
        if let Some(pending) = &self.reveal {
            return Some(pending.clone());
        }
        let geometry = self.geometry?;
        if geometry.track.size.width <= 0. || geometry.track.size.height <= 0. {
            return None;
        }
        let (turn, reason) = if !self.initialized {
            (
                self.order.last()?.clone(),
                TurnRailRevealReason::InitialLatest,
            )
        } else {
            if self.honored {
                return None;
            }
            (self.follow.clone()?, TurnRailRevealReason::FollowChanged)
        };
        let item = self.row_id(&turn)?;
        if reason == TurnRailRevealReason::FollowChanged {
            let top = self.rows().iter().find(|c| c.id() == item)?.frame().min_y();
            let margin = 2. * TurnRailGeometry::ROW_HEIGHT;
            if top >= geometry.track.min_y() + margin && top <= geometry.track.max_y() - margin {
                self.honored = true;
                return None;
            }
        }
        let request = TurnRailReveal {
            owner: self.owner.clone(),
            token: Arc::new(()),
            revision: self.revision,
            viewport: self.viewport_id(),
            item,
            turn,
            reason,
            claimed: Arc::new(AtomicBool::new(false)),
        };
        self.reveal = Some(request.clone());
        Some(request)
    }
    pub fn claim_reveal(&self, request: &TurnRailReveal) -> Result<(), &'static str> {
        if !Arc::ptr_eq(&request.owner, &self.owner)
            || self.revision != request.revision
            || self
                .reveal
                .as_ref()
                .is_none_or(|r| !Arc::ptr_eq(&r.token, &request.token))
        {
            return Err("stale or foreign rail reveal");
        }
        request
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| "rail reveal already dispatched")
    }
    /// An accepted request is honored even when the library reports no motion.
    /// A rejected request is suspended until explicit retry or changed input.
    pub fn finish_reveal(&mut self, request: &TurnRailReveal, accepted: bool) -> bool {
        if !Arc::ptr_eq(&request.owner, &self.owner)
            || self.revision != request.revision
            || self
                .reveal
                .as_ref()
                .is_none_or(|r| !Arc::ptr_eq(&r.token, &request.token))
            || !request.claimed.load(Ordering::Acquire)
        {
            return false;
        }
        self.reveal = None;
        if accepted {
            self.initialized = true;
            if self.follow.as_deref() == Some(&request.turn) || self.follow.is_none() {
                self.honored = true;
            }
        } else {
            self.suspended = true;
        }
        true
    }
    pub fn retry_reveal(&mut self) {
        self.suspended = false;
        self.reveal = None;
    }
    fn clear_hover(&mut self) {
        self.hovered = false;
        self.hover_target = 0.;
        self.retarget_hover = false;
        self.hover_motion = None;
        *self.hover.lock().expect("turn rail hover lock poisoned") = HoverVisual::default();
    }
    fn update_hover(&mut self, position: Option<f32>) -> bool {
        let turn_id = position.and_then(|position| {
            let index = (position + 0.5).floor() as usize;
            self.order.get(index).cloned()
        });
        let next_hovered = turn_id.is_some();
        let mut visual = self.hover.lock().expect("turn rail hover lock poisoned");
        let displayed = position.or(visual.displayed_position);
        let changed = self.hovered != next_hovered
            || visual.displayed_position != displayed
            || visual.hovered_turn_id != turn_id;
        if self.hovered != next_hovered {
            self.hovered = next_hovered;
            self.hover_target = if next_hovered { 1. } else { 0. };
            self.retarget_hover = true;
        }
        visual.displayed_position = displayed;
        visual.hovered_turn_id = turn_id;
        changed
    }
    fn observe_pointer(&mut self, position: Option<Point>) -> bool {
        let Some(position) = position else {
            return self.update_hover(None);
        };
        let Some(geometry) = self.geometry else {
            return false;
        };
        if !geometry.track.contains(position) || self.rows().is_empty() {
            return self.update_hover(None);
        }
        // ScrollContent's arranged row frames include the SDK's current
        // document offset. Never reconstruct that offset from a second model.
        let first = self.rows()[0].frame();
        let row_position = (position.y - first.min_y()) / TurnRailGeometry::ROW_HEIGHT;
        if !(0. ..self.order.len() as f32).contains(&row_position) {
            return self.update_hover(None);
        }
        self.update_hover(Some(row_position - 0.5))
    }
    fn advance_hover(&mut self, text: &mut dyn ButtonTextRenderer) {
        let Some(ControlMotionContext {
            now,
            reduced_motion,
        }) = text.motion_context()
        else {
            self.hover_motion = None;
            self.retarget_hover = false;
            self.write_hover(self.hover_target as f32);
            return;
        };
        let mut value = self
            .hover
            .lock()
            .expect("turn rail hover lock poisoned")
            .influence as f64;
        if let Some(motion) = self.hover_motion.as_mut()
            && motion.is_reduced_motion() != reduced_motion
        {
            match motion.set_reduced_motion(reduced_motion, now) {
                Ok(frame) => value = frame.sample.value,
                Err(_) => {
                    self.hover_motion = None;
                    self.retarget_hover = false;
                    self.write_hover(self.hover_target as f32);
                    return;
                }
            }
        }
        if self.retarget_hover {
            let result = if let Some(motion) = self.hover_motion.as_mut() {
                motion.animate_to(self.hover_target, hover_motion(), now)
            } else {
                StateTransition::new(value, now, reduced_motion).and_then(|mut motion| {
                    let frame = motion.animate_to(self.hover_target, hover_motion(), now)?;
                    self.hover_motion = Some(motion);
                    Ok(frame)
                })
            };
            match result {
                Ok(frame) => {
                    value = frame.sample.value;
                    self.retarget_hover = false;
                }
                Err(_) => {
                    self.hover_motion = None;
                    self.retarget_hover = false;
                    self.write_hover(self.hover_target as f32);
                    return;
                }
            }
        }
        let mut needs_frame = false;
        if let Some(motion) = self.hover_motion.as_mut() {
            match motion.advance(now) {
                Ok(frame) => {
                    value = frame.sample.value;
                    needs_frame = frame.needs_frame;
                }
                Err(_) => {
                    self.hover_motion = None;
                    self.write_hover(self.hover_target as f32);
                    return;
                }
            }
        }
        self.write_hover(value as f32);
        if needs_frame {
            text.request_animation_frame();
        }
    }
    fn write_hover(&self, influence: f32) {
        let mut hover = self.hover.lock().expect("turn rail hover lock poisoned");
        hover.influence = influence.clamp(0., 1.);
        if !self.hovered && hover.influence <= 0. {
            hover.displayed_position = None;
        }
    }
}

fn hover_motion() -> Motion {
    Motion::new(MotionSpec::Tween {
        duration: Duration::from_millis(180),
        // Match the ClientKit 1 - (1 - t)^3 ease-out on its elapsed-time axis.
        curve: TimingCurve::cubic_bezier(1. / 3., 1., 2. / 3., 1.).expect("fixed ease-out curve"),
    })
}
impl Component for TurnRail {
    fn lifecycle(&mut self, event: Lifecycle) {
        if event == Lifecycle::Unmounted {
            self.invalidate_layout();
        }
    }
    fn active(&self) -> bool {
        turn_rail_supported() && !self.order.is_empty()
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, a: Size) -> MeasuredChild {
        MeasuredChild::new(a)
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, _: Rect, c: Rect) {
        if let Some(g) = self.geometry {
            self.children[0].arrange(t, g.track, g.track.intersection(&c).unwrap_or_default());
        }
    }
    fn paint(
        &mut self,
        _: &mut tekes_ui_scene::Scene,
        text: &mut dyn ButtonTextRenderer,
        _: Rect,
        _: Rect,
        _: ContentTheme,
    ) {
        self.advance_hover(text);
    }
    fn interactive(&self) -> bool {
        true
    }
    fn pointer_regions(&self, _: Rect, clip: Rect) -> Vec<Rect> {
        self.geometry
            .and_then(|geometry| geometry.track.intersection(&clip))
            .filter(|rect| !rect.is_empty())
            .into_iter()
            .collect()
    }
    fn pointer_event(&mut self, event: &PointerEvent) -> ContentResponse {
        let changed = match event.phase {
            PointerPhase::Move | PointerPhase::Enter => self.observe_pointer(Some(event.position)),
            PointerPhase::Exit => self.observe_pointer(None),
            _ => false,
        };
        ContentResponse {
            changed,
            ..Default::default()
        }
    }
    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        if let ContentEvent::PointerMove(position) = event {
            return ContentResponse {
                changed: self.observe_pointer(Some(*position)),
                ..Default::default()
            };
        }
        ContentResponse::default()
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}

fn rail_action_content(
    id: ComponentId,
    tick: ComponentId,
    hover: Arc<Mutex<HoverVisual>>,
) -> Content {
    Content::new(id, TurnRailAction::new(tick, hover)).preserving_focus()
}

#[cfg(test)]
mod hover_tests {
    use super::*;
    use crate::{TranscriptTurnPreview, TranscriptTurnTarget};
    use std::sync::Arc;
    use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer, ControlMotionContext};
    use tekes_ui_scene::Scene;
    use tekes_ui_tokens::{Rgba, Theme, proposal};

    #[derive(Default)]
    struct Text {
        motion: Option<ControlMotionContext>,
        requested_frames: usize,
    }
    impl ButtonTextRenderer for Text {
        fn measure(&mut self, text: &str, _: proposal::TextRole, _: f32) -> Size {
            Size::new(text.chars().count() as f32 * 6., 14.)
        }
        fn draw(
            &mut self,
            _: &mut Scene,
            _: &str,
            _: proposal::TextRole,
            _: Rect,
            _: ButtonTextAlign,
            _: Rgba,
        ) {
        }
        fn motion_context(&self) -> Option<ControlMotionContext> {
            self.motion
        }
        fn request_animation_frame(&mut self) {
            self.requested_frames += 1;
        }
    }

    fn source_targets(
        count: usize,
        publication: u64,
        epoch: u64,
        scope: RunScope,
        owner: &Arc<()>,
    ) -> Vec<RunTurnNavigation> {
        (0..count)
            .map(|index| {
                let turn_id = format!("turn-{index}");
                let target = TranscriptTurnTarget {
                    owner: owner.clone(),
                    scope: scope.clone(),
                    publication,
                    epoch,
                    turn_id,
                    preview: TranscriptTurnPreview {
                        input: format!("prompt {index}"),
                        output: String::new(),
                        has_final_answer: false,
                        is_settled: true,
                    },
                    status: "completed".into(),
                    rows: vec![ComponentId(900 + index as u64)],
                    visible: Some(index == 1),
                };
                RunTurnNavigation {
                    jump: None,
                    target,
                    kind: RunTurnNavigationKind::TurnStart(ComponentId(900 + index as u64)),
                    generation: publication,
                    lifetime: Arc::new(()),
                    claimed: Arc::new(AtomicBool::new(false)),
                    viewport: ComponentId((0x4700u64 << 32) + 1),
                }
            })
            .collect()
    }

    fn mounted(count: usize) -> (ContentHost, ComponentId, Arc<()>) {
        let scope = RunScope::main("hover-test", "session");
        let owner = Arc::new(());
        let mut rail = TurnRail::new(0x4700, scope.clone());
        rail.set_gate(TurnRailNavigationGate {
            turn_start: true,
            latest: true,
        });
        rail.refresh(
            source_targets(count, 1, 1, scope, &owner),
            TurnRailGeometry {
                rail: Rect::new(0., 0., 60., 100.),
                track: Rect::new(10., 10., 40., 80.),
                document_size: Size::new(40., count as f32 * TurnRailGeometry::ROW_HEIGHT),
                centers_ticks: false,
            },
        )
        .unwrap();
        let root = rail.id();
        (
            ContentHost::new(Content::new(root, rail)).unwrap(),
            root,
            owner,
        )
    }

    fn render(host: &mut ContentHost, text: &mut Text) {
        let bounds = Rect::new(0., 0., 100., 110.);
        host.render(
            &mut Scene::default(),
            text,
            bounds,
            bounds,
            Theme::LIGHT.into(),
        )
        .unwrap();
    }

    fn pointer(position: Point, phase: PointerPhase) -> ContentEvent {
        ContentEvent::Pointer(PointerEvent {
            position,
            phase,
            button: None,
            modifiers: Default::default(),
            click_count: 0,
        })
    }

    fn visual(host: &ContentHost, root: ComponentId) -> HoverVisual {
        host.get(root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .hover
            .lock()
            .unwrap()
            .clone()
    }

    #[test]
    fn pointer_uses_arranged_rows_half_row_ramp_and_sdk_scrolled_frames() {
        let (mut host, root, _) = mounted(24);
        let mut text = Text::default();
        render(&mut host, &mut text);
        let rail = host.get(root).unwrap().component::<TurnRail>().unwrap();
        let track = rail.layout_geometry().unwrap().track;
        let first = host.get(rail.item_ids()[0]).unwrap().frame();
        host.event(&ContentEvent::Scroll {
            position: track.center(),
            delta: Point::new(0., 35.),
            precise: true,
            begin: true,
            end: true,
            momentum: false,
            time: 0.,
        });
        render(&mut host, &mut text);
        let rail = host.get(root).unwrap().component::<TurnRail>().unwrap();
        let ids = rail.item_ids();
        let scrolled = host.get(ids[0]).unwrap().frame();
        assert_ne!(
            first.min_y(),
            scrolled.min_y(),
            "SDK wheel should move the original row frames"
        );

        let target_index = ids
            .iter()
            .position(|id| {
                host.get(*id)
                    .unwrap()
                    .visible()
                    .is_some_and(|r| r.contains(track.center()))
            })
            .expect("a scrolled original row should cross the viewport center");
        let row = host.get(ids[target_index]).unwrap().frame();
        let center = Point::new(row.center().x, row.center().y);
        let result = host.event(&pointer(center, PointerPhase::Move));
        assert!(!result.consumed && !result.capture_pointer);
        assert_eq!(
            visual(&host, root).hovered_turn_id.as_deref(),
            Some(format!("turn-{target_index}").as_str())
        );
        assert_eq!(
            visual(&host, root).displayed_position,
            Some(target_index as f32)
        );

        let half_row = Point::new(center.x, center.y + TurnRailGeometry::ROW_HEIGHT / 2.);
        host.event(&pointer(half_row, PointerPhase::Move));
        assert_eq!(
            visual(&host, root).displayed_position,
            Some(target_index as f32 + 0.5)
        );
        let ramp = HoverVisual {
            displayed_position: Some(3.5),
            influence: 1.,
            hovered_turn_id: Some("turn-3".into()),
        };
        assert_eq!(crate::turn_rail_action::tick_length(3, &ramp), 23.);
        assert_eq!(crate::turn_rail_action::tick_length(4, &ramp), 23.);
        assert_eq!(crate::turn_rail_action::tick_length(7, &ramp), 7.);
        assert_eq!(crate::turn_rail_action::tick_length(8, &ramp), 6.);
        host.event(&pointer(Point::new(90., 90.), PointerPhase::Move));
        assert!(visual(&host, root).hovered_turn_id.is_none());
        assert_eq!(
            visual(&host, root).displayed_position,
            Some(target_index as f32 + 0.5)
        );
    }

    #[test]
    fn motion_retargets_only_at_enter_leave_and_honors_clock_reduced_and_missing_clock() {
        let (mut host, root, _) = mounted(8);
        let mut text = Text {
            motion: Some(ControlMotionContext {
                now: Duration::ZERO,
                reduced_motion: false,
            }),
            requested_frames: 0,
        };
        render(&mut host, &mut text);
        let row = host
            .get(
                host.get(root)
                    .unwrap()
                    .component::<TurnRail>()
                    .unwrap()
                    .item_ids()[2],
            )
            .unwrap()
            .frame();
        host.event(&pointer(row.center(), PointerPhase::Move));
        render(&mut host, &mut text);
        assert_eq!(visual(&host, root).influence, 0.);
        assert!(text.requested_frames > 0);
        let generation = host
            .get(root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .hover_motion
            .as_ref()
            .unwrap()
            .generation();
        text.motion.as_mut().unwrap().now = Duration::from_millis(72);
        render(&mut host, &mut text);
        let halfway = visual(&host, root).influence;
        assert!(halfway > 0. && halfway < 1.);
        host.event(&pointer(
            Point::new(row.center().x, row.center().y + 1.),
            PointerPhase::Move,
        ));
        text.motion.as_mut().unwrap().now = Duration::from_millis(90);
        render(&mut host, &mut text);
        assert!(visual(&host, root).influence > halfway);
        assert_eq!(
            host.get(root)
                .unwrap()
                .component::<TurnRail>()
                .unwrap()
                .hover_motion
                .as_ref()
                .unwrap()
                .generation(),
            generation,
            "row movement must not restart the boundary fade",
        );

        host.event(&pointer(Point::new(99., 99.), PointerPhase::Exit));
        text.motion.as_mut().unwrap().now = Duration::from_millis(180);
        render(&mut host, &mut text);
        assert!(visual(&host, root).influence > 0.);
        text.motion.as_mut().unwrap().now = Duration::from_millis(360);
        render(&mut host, &mut text);
        assert_eq!(visual(&host, root).influence, 0.);
        assert!(visual(&host, root).displayed_position.is_none());

        let (mut reduced, reduced_root, _) = mounted(8);
        let mut reduced_text = Text {
            motion: Some(ControlMotionContext {
                now: Duration::from_secs(2),
                reduced_motion: true,
            }),
            requested_frames: 0,
        };
        render(&mut reduced, &mut reduced_text);
        let row = reduced
            .get(reduced_root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .item_ids()[2];
        let center = reduced.get(row).unwrap().frame().center();
        reduced.event(&pointer(center, PointerPhase::Move));
        render(&mut reduced, &mut reduced_text);
        assert_eq!(visual(&reduced, reduced_root).influence, 1.);
        assert_eq!(reduced_text.requested_frames, 0);
        reduced.event(&pointer(Point::new(99., 99.), PointerPhase::Exit));
        render(&mut reduced, &mut reduced_text);
        assert_eq!(visual(&reduced, reduced_root).influence, 0.);

        let (mut immediate, immediate_root, _) = mounted(8);
        let mut no_clock = Text::default();
        render(&mut immediate, &mut no_clock);
        let row = immediate
            .get(immediate_root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .item_ids()[2];
        let center = immediate.get(row).unwrap().frame().center();
        immediate.event(&pointer(center, PointerPhase::Move));
        render(&mut immediate, &mut no_clock);
        assert_eq!(visual(&immediate, immediate_root).influence, 1.);
        assert_eq!(no_clock.requested_frames, 0);
        immediate.event(&pointer(Point::new(99., 99.), PointerPhase::Exit));
        render(&mut immediate, &mut no_clock);
        assert_eq!(visual(&immediate, immediate_root).influence, 0.);
    }

    #[test]
    fn stable_source_publication_rebinds_target_without_restarting_hover_but_retirement_clears_it()
    {
        let (mut host, root, owner) = mounted(4);
        let mut text = Text {
            motion: Some(ControlMotionContext {
                now: Duration::ZERO,
                reduced_motion: false,
            }),
            requested_frames: 0,
        };
        render(&mut host, &mut text);
        let row = host
            .get(root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .item_ids()[1];
        let center = host.get(row).unwrap().frame().center();
        host.event(&pointer(center, PointerPhase::Move));
        render(&mut host, &mut text);
        text.motion.as_mut().unwrap().now = Duration::from_millis(60);
        render(&mut host, &mut text);
        let before = visual(&host, root);
        assert!(before.influence > 0. && before.influence < 1.);
        let generation = host
            .get(root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .hover_motion
            .as_ref()
            .unwrap()
            .generation();
        let scope = RunScope::main("hover-test", "session");
        host.get_mut(root)
            .unwrap()
            .component_mut::<TurnRail>()
            .unwrap()
            .refresh(
                source_targets(4, 2, 1, scope.clone(), &owner),
                TurnRailGeometry {
                    rail: Rect::new(0., 0., 60., 100.),
                    track: Rect::new(10., 10., 40., 80.),
                    document_size: Size::new(40., 80.),
                    centers_ticks: false,
                },
            )
            .unwrap();
        assert_eq!(visual(&host, root), before);
        assert_eq!(
            host.get(root)
                .unwrap()
                .component::<TurnRail>()
                .unwrap()
                .hover_motion
                .as_ref()
                .unwrap()
                .generation(),
            generation,
        );
        assert_eq!(
            host.get(root)
                .unwrap()
                .component::<TurnRail>()
                .unwrap()
                .preview_target("turn-1")
                .unwrap()
                .publication(),
            2,
        );

        let rail = host
            .get_mut(root)
            .unwrap()
            .component_mut::<TurnRail>()
            .unwrap();
        let mut reordered = source_targets(4, 3, 1, scope.clone(), &owner);
        reordered.reverse();
        rail.refresh(
            reordered,
            TurnRailGeometry {
                rail: Rect::new(0., 0., 60., 100.),
                track: Rect::new(10., 10., 40., 80.),
                document_size: Size::new(40., 80.),
                centers_ticks: false,
            },
        )
        .unwrap();
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_none());
        rail.update_hover(Some(1.));
        assert_eq!(
            rail.hover.lock().unwrap().hovered_turn_id.as_deref(),
            Some("turn-2")
        );
        let mut next_epoch = source_targets(4, 4, 2, scope.clone(), &owner);
        next_epoch.reverse();
        rail.refresh(
            next_epoch,
            TurnRailGeometry {
                rail: Rect::new(0., 0., 60., 100.),
                track: Rect::new(10., 10., 40., 80.),
                document_size: Size::new(40., 80.),
                centers_ticks: false,
            },
        )
        .unwrap();
        assert!(rail.hover_motion.is_none());
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_none());

        let rail = host
            .get_mut(root)
            .unwrap()
            .component_mut::<TurnRail>()
            .unwrap();
        let replacement_owner = Arc::new(());
        let mut replacement_controller = source_targets(4, 5, 2, scope, &replacement_owner);
        replacement_controller.reverse();
        rail.refresh(
            replacement_controller,
            TurnRailGeometry {
                rail: Rect::new(0., 0., 60., 100.),
                track: Rect::new(10., 10., 40., 80.),
                document_size: Size::new(40., 80.),
                centers_ticks: false,
            },
        )
        .unwrap();
        rail.update_hover(Some(1.));
        rail.clear_geometry();
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_some());
        rail.invalidate_layout();
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_none());
        rail.update_hover(Some(1.));
        rail.refresh(
            Vec::new(),
            TurnRailGeometry {
                rail: Rect::default(),
                track: Rect::default(),
                document_size: Size::default(),
                centers_ticks: false,
            },
        )
        .unwrap();
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_none());
        rail.update_hover(Some(1.));
        rail.lifecycle(Lifecycle::Unmounted);
        assert!(rail.hover.lock().unwrap().hovered_turn_id.is_none());
    }

    #[test]
    fn hover_preserves_original_button_focus_and_press_emits_one_navigation() {
        let (mut host, root, _) = mounted(5);
        let mut text = Text::default();
        render(&mut host, &mut text);
        let ids = host
            .get(root)
            .unwrap()
            .component::<TurnRail>()
            .unwrap()
            .item_ids();
        assert!(host.focus(Some(ids[0]), true));
        let focused = host.focused();
        let target = host.get(ids[2]).unwrap().frame();
        let event = host.event(&pointer(target.center(), PointerPhase::Move));
        assert!(!event.consumed && !event.capture_pointer);
        assert_eq!(host.focused(), focused);
        host.event(&ContentEvent::PointerDown(target.center()));
        host.event(&ContentEvent::PointerUp(target.center()));
        assert_eq!(host.focused(), focused);
        let actions = host
            .get_mut(root)
            .unwrap()
            .component_mut::<TurnRail>()
            .unwrap()
            .take_navigation();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].target().turn_id(), "turn-2");
    }
}
