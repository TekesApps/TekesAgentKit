//! Real turn rail over original library scroll and CompoundButton controls.
//! No generic hit testing, scrolling, input or animation engine lives here.
use crate::turn_rail_action::{Tick, TurnRailAction};
use crate::{RunScope, RunTurnNavigation, RunTurnNavigationKind, TurnRailGeometry};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tekes_ui_controls::{ButtonTextRenderer, ScrollArea, composition::*, layout::MeasuredChild};
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
/// Mount through RunView::install_turn_rail so source identity and layout stay
/// shared. The current default pin reports unsupported rather than overlapping
/// minimum-height buttons or a hidden input mirror.
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
        }
    }
    pub fn set_labels(&mut self, labels: TurnRailLabels) {
        self.labels = labels;
        let format = self.labels.action;
        for (index, node) in self.rows_mut().iter_mut().enumerate() {
            let row = node.component_mut::<TurnRailAction>().unwrap();
            if let Some(prototype) = &row.prototype {
                row.button.action.label = format(index, prototype.target());
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
            .action
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
        let needed = order.iter().filter(|k| !self.ids.contains_key(*k)).count() as u64 * 2;
        let next = self
            .next
            .checked_add(needed)
            .filter(|v| *v <= self.end)
            .ok_or("rail ID namespace exhausted")?;
        let membership_changed = order != self.order;
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
                .unwrap_or_else(|| rail_action_content(id, tick));
            let action = content.component_mut::<TurnRailAction>().unwrap();
            action.button.action.label = (self.labels.action)(index, prototype.target());
            action.button.action.button.state.selected =
                prototype.target().is_visible() == Some(true);
            action.button.action.button.state.disabled = !self.gate.allows(prototype.kind());
            action.button.children_mut().unwrap()[0]
                .component_mut::<Tick>()
                .unwrap()
                .centers = geometry.centers_ticks;
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
            a.button.action.button.state.disabled =
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
                        .action
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
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}

fn rail_action_content(id: ComponentId, tick: ComponentId) -> Content {
    Content::new(id, TurnRailAction::new(tick)).preserving_focus()
}
