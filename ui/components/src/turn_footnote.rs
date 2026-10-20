//! Turn-level output aside; no window, endpoint or independent Run state.
use crate::OutputUsage;
use crate::{TranscriptCopyIntent, TranscriptMessageIdentity};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tekes_agent_contract::{ThreadActionDefinition, ThreadActionOutcome};
use tekes_agent_presentation::{RunScope, TranscriptItemKind, TranscriptOutput, TranscriptTurn};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::{AlignmentOverride, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::{Spacing, proposal};

#[derive(Clone, Debug, PartialEq)]
pub enum TurnActionResult {
    Completed(ThreadActionOutcome),
    Rejected(String),
    Unknown(String),
}
#[derive(Default, Debug)]
struct Receipt {
    claimed: bool,
    result: Option<TurnActionResult>,
}
/// An immutable original-turn gesture. Clones share one admission and completion.
#[derive(Clone, Debug)]
pub struct TurnFootnoteIntent {
    scope: RunScope,
    turn: TranscriptTurn,
    epoch: u64,
    publication: u64,
    action: ThreadActionDefinition,
    receipt: Arc<Mutex<Receipt>>,
}
impl TurnFootnoteIntent {
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn turn(&self) -> &TranscriptTurn {
        &self.turn
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn publication(&self) -> u64 {
        self.publication
    }
    pub fn action(&self) -> &ThreadActionDefinition {
        &self.action
    }
    /// A dispatcher claims once before preparing an RPC. Not an authority token:
    /// it must independently validate current scope, pointer, boundary and policy.
    pub fn claim_dispatch(&self) -> bool {
        let mut r = self.receipt.lock().unwrap();
        if r.claimed || r.result.is_some() {
            false
        } else {
            r.claimed = true;
            true
        }
    }
    pub fn complete(&self, result: TurnActionResult) -> bool {
        let mut r = self.receipt.lock().unwrap();
        if r.result.is_some() {
            false
        } else {
            r.result = Some(result);
            true
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TurnFootnoteFacts {
    pub turn_id: String,
    pub status: String,
    /// Unix milliseconds, matching the shared transcript; never a host clock.
    pub started_at: Option<f64>,
    pub ended_at: Option<f64>,
    pub output_usage: Option<OutputUsage>,
}
impl TurnFootnoteFacts {
    pub fn read(turn: &TranscriptTurn) -> Self {
        Self {
            turn_id: turn.id.clone(),
            status: turn.status.clone(),
            started_at: turn.started_at,
            ended_at: turn.ended_at,
            output_usage: OutputUsage::read_turn(turn),
        }
    }
    /// A settled interval converted from Unix milliseconds to seconds.
    /// Running elapsed time needs an explicit host clock.
    pub fn elapsed_seconds(&self) -> Option<f64> {
        self.started_at.zip(self.ended_at).and_then(|(start, end)| {
            let elapsed = (end - start) / 1000.;
            (start.is_finite() && end.is_finite() && elapsed.is_finite() && elapsed >= 0.)
                .then_some(elapsed)
        })
    }
    pub fn text(&self) -> String {
        let mut parts = vec![self.status.clone()];
        if let Some(usage) = &self.output_usage {
            parts.push(match usage {
                OutputUsage::Live { output_tokens } => {
                    format!("输出 {output_tokens} tokens · 实时报告")
                }
                OutputUsage::Reported {
                    output_tokens,
                    complete,
                } => format!(
                    "输出 {output_tokens} tokens{}",
                    if *complete { "" } else { " · 部分报告" }
                ),
            });
        }
        if let Some(duration) = self.elapsed_seconds() {
            parts.push(format!("{duration:.1}s"));
        }
        parts.join(" · ")
    }
}
#[derive(Clone, Debug)]
pub struct TurnFootnoteLabels {
    pub copy: String,
    pub group: String,
    pub pending: String,
    pub completed: String,
    pub unknown: String,
    pub information: fn(&TurnFootnoteFacts) -> String,
}
impl Default for TurnFootnoteLabels {
    fn default() -> Self {
        Self {
            copy: "复制输出".into(),
            group: "回合信息".into(),
            pending: "正在处理回合操作".into(),
            completed: "操作已完成".into(),
            unknown: "操作结果未确认，请先核对；不会自动重试".into(),
            information: TurnFootnoteFacts::text,
        }
    }
}
#[derive(Debug)]
pub struct TurnFootnote {
    output_copy: Option<(TranscriptMessageIdentity, String)>,
    copy_id: Option<ComponentId>,
    root: ComponentId,
    scope: RunScope,
    turn: Option<TranscriptTurn>,
    publication: u64,
    epoch: u64,
    available: bool,
    rows: StackContent,
    actions: BTreeMap<ComponentId, ThreadActionDefinition>,
    next: u64,
    labels: TurnFootnoteLabels,
    pending: Option<TurnFootnoteIntent>,
    observed: Option<TurnActionResult>,
    notice: String,
}
impl TurnFootnote {
    pub fn new(namespace: u32, scope: RunScope, labels: TurnFootnoteLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            output_copy: None,
            copy_id: None,
            root,
            scope,
            turn: None,
            publication: 0,
            epoch: 0,
            available: false,
            rows: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![Content::new(
                    ComponentId(root.0 + 1),
                    TextContent {
                        text: String::new(),
                        role: proposal::TextRole::UiCaption,
                    },
                )],
            },
            actions: BTreeMap::new(),
            next: 16,
            labels,
            pending: None,
            observed: None,
            notice: String::new(),
        }
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn action_id(&self, id: &str) -> Option<ComponentId> {
        self.actions
            .iter()
            .find_map(|(node, a)| (a.id == id).then_some(*node))
    }
    /// Host pointer/history availability gate; this cannot make a disabled
    /// shared definition executable. Losing a loaded target retains only its readout.
    pub fn set_available(&mut self, available: bool) {
        if self.available != available {
            self.available = available;
            self.rekey();
            self.render_actions();
        }
    }
    pub fn facts(&self) -> Option<TurnFootnoteFacts> {
        self.turn.as_ref().map(TurnFootnoteFacts::read)
    }
    pub fn update(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
    ) -> Result<(), &'static str> {
        self.update_with_output(scope, publication, epoch, turn, available, None)
    }
    /// Pass `section.final_output()` from the same canonical document frame.
    /// Copy remains local when shared mutations are unavailable. Missing or
    /// ambiguous targets must be None, never a last-message fallback.
    pub fn update_with_output(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        output: Option<TranscriptOutput<'_>>,
    ) -> Result<(), &'static str> {
        let rekey =
            self.check_update_with_output(scope, publication, epoch, turn, available, output)?;
        let copy = copy_facts(turn, output)?;
        if copy != self.output_copy || epoch != self.epoch {
            self.copy_id = copy.as_ref().map(|_| {
                let id = ComponentId(self.root.0 + self.next);
                self.next += 1;
                id
            });
        }
        self.output_copy = copy;
        self.turn = Some(turn.clone());
        self.publication = publication;
        self.epoch = epoch;
        self.available = available;
        self.rows.children[0]
            .component_mut::<TextContent>()
            .unwrap()
            .text = (self.labels.information)(&TurnFootnoteFacts::read(turn));
        if rekey {
            self.rekey();
        }
        self.refresh();
        self.render_actions();
        Ok(())
    }
    pub fn copy_button_id(&self) -> Option<ComponentId> {
        self.copy_id
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<TranscriptCopyIntent> {
        let Some((message, text)) = &self.output_copy else {
            return vec![];
        };
        if !actions
            .iter()
            .any(|a| Some(a.target) == self.copy_id && a.kind == ContentActionKind::Activate)
        {
            return vec![];
        }
        vec![TranscriptCopyIntent::new(
            self.scope.clone(),
            message.clone(),
            text.clone(),
        )]
    }
    pub(crate) fn check_update_with_output(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        output: Option<TranscriptOutput<'_>>,
    ) -> Result<bool, &'static str> {
        let rekey = self.check_update(scope, publication, epoch, turn, available)?;
        let copy = copy_facts(turn, output)?;
        if publication == self.publication && copy != self.output_copy {
            return Err("conflicting turn copy publication");
        }
        Ok(rekey)
    }
    pub(crate) fn has_unresolved_action(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn check_update(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
    ) -> Result<bool, &'static str> {
        if scope != &self.scope {
            return Err("turn footnote scope mismatch");
        }
        if publication == 0 || publication < self.publication || epoch < self.epoch {
            return Err("stale turn footnote publication");
        }
        if turn.id.is_empty() || self.turn.as_ref().is_some_and(|old| old.id != turn.id) {
            return Err("turn footnote identity changed");
        }
        if publication == self.publication
            && (self.epoch != epoch || self.turn.as_ref() != Some(turn))
        {
            return Err("conflicting turn footnote publication");
        }
        let mut ids = std::collections::BTreeSet::new();
        if turn
            .footnote_actions
            .iter()
            .any(|a| a.id.is_empty() || !ids.insert(&a.id))
        {
            return Err("invalid turn footnote action catalog");
        }
        let rekey = self.turn.as_ref().is_none_or(|old| {
            old.footnote_actions != turn.footnote_actions || old.fork_boundary != turn.fork_boundary
        }) || self.epoch != epoch
            || self.available != available;
        let count = turn.footnote_actions.len() as u64;
        if self
            .next
            .checked_add(count + 1)
            .is_none_or(|n| n >= (1_u64 << 31))
        {
            return Err("turn footnote namespace exhausted");
        }
        Ok(rekey)
    }
    fn rekey(&mut self) {
        self.actions.clear();
        for action in self
            .turn
            .as_ref()
            .into_iter()
            .flat_map(|turn| &turn.footnote_actions)
        {
            if self.next >= (1_u64 << 31) {
                self.notice = "footnote namespace exhausted".into();
                break;
            }
            let id = ComponentId(self.root.0 + self.next);
            self.next += 1;
            self.actions.insert(id, action.clone());
        }
    }
    /// Observe only this mount's operation receipt. Known completion/rejection
    /// permits another explicit gesture with new IDs; uncertain outcomes stay locked.
    pub fn refresh(&mut self) {
        let result = self
            .pending
            .as_ref()
            .and_then(|intent| intent.receipt.lock().unwrap().result.clone());
        if result == self.observed {
            return;
        }
        self.observed = result.clone();
        if let Some(result) = result {
            match result {
                TurnActionResult::Completed(outcome) => {
                    self.notice = outcome
                        .warning
                        .unwrap_or_else(|| self.labels.completed.clone());
                    self.pending = None;
                    self.observed = None;
                    self.rekey();
                }
                TurnActionResult::Rejected(error) => {
                    self.notice = error;
                    self.pending = None;
                    self.observed = None;
                    self.rekey();
                }
                TurnActionResult::Unknown(error) => {
                    self.notice = format!("{} · {error}", self.labels.unknown);
                }
            }
            self.render_actions();
        }
    }
    fn render_actions(&mut self) {
        // Information node is retained through token/status-only publications.
        let mut previous = self.rows.children.split_off(1);
        for (id, action) in &self.actions {
            let mut node = previous
                .iter()
                .position(|node| node.id() == *id)
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| Content::new(*id, ButtonContent::new(&action.label)));
            let button = node.component_mut::<ButtonContent>().unwrap();
            button.label = action.label.clone();
            button.button.state.disabled =
                !self.available || self.pending.is_some() || !action.enabled;
            self.rows.children.push(node);
            if let Some(reason) = &action.disabled_reason {
                self.rows.children.push(Content::new(
                    ComponentId(id.0 + (1_u64 << 31)),
                    TextContent {
                        text: reason.clone(),
                        role: proposal::TextRole::UiCaption,
                    },
                ));
            }
        }
        if let Some(id) = self.copy_id {
            let mut node = previous
                .iter()
                .position(|node| node.id() == id)
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| Content::new(id, ButtonContent::new(&self.labels.copy)));
            node.component_mut::<ButtonContent>().unwrap().label = self.labels.copy.clone();
            self.rows.children.push(node);
        }
        let notice = if self.pending.is_some() && self.observed.is_none() {
            &self.labels.pending
        } else {
            &self.notice
        };
        if !notice.is_empty() {
            self.rows.children.push(Content::new(
                ComponentId(self.root.0 + 2),
                TextContent {
                    text: notice.clone(),
                    role: proposal::TextRole::UiCaption,
                },
            ));
        }
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<TurnFootnoteIntent> {
        self.refresh();
        if !self.available || self.pending.is_some() {
            return vec![];
        }
        let Some(action) = actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
            .find_map(|a| self.actions.get(&a.target))
            .filter(|a| a.enabled)
            .cloned()
        else {
            return vec![];
        };
        let intent = TurnFootnoteIntent {
            scope: self.scope.clone(),
            turn: self.turn.clone().expect("catalog has turn"),
            epoch: self.epoch,
            publication: self.publication,
            action,
            receipt: Arc::new(Mutex::new(Receipt::default())),
        };
        self.pending = Some(intent.clone());
        self.notice.clear();
        self.render_actions();
        vec![intent]
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
fn copy_facts(
    turn: &TranscriptTurn,
    output: Option<TranscriptOutput<'_>>,
) -> Result<Option<(TranscriptMessageIdentity, String)>, &'static str> {
    let Some(output) = output else {
        return Ok(None);
    };
    if turn.status != "completed"
        || output.item.turn_id != turn.id
        || output.item.kind != TranscriptItemKind::Assistant
        || !output.item.is_final
        || !output.item.is_visible
        || output.item.is_provisional
        || output.item.text.is_empty()
    {
        return Err("invalid final output copy target");
    }
    Ok(Some((output.key.into(), output.item.text.clone())))
}
impl Component for TurnFootnote {
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        let measured = self.rows.measure(text, available);
        MeasuredChild::new(Size::new(available.width, measured.size.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let sizes: Vec<_> = self
            .rows
            .children
            .iter_mut()
            .map(|c| c.measure(text, frame.size))
            .collect();
        let placed = layout_measured_children(
            frame,
            Axis::Vertical,
            &sizes,
            Spacing::Tight.value(),
            proposal::layout::COLUMN,
            AlignmentOverride {
                cross_axis: Some(proposal::layout::CrossAxisAlignment::Leading),
                ..Default::default()
            },
        );
        for (child, position) in self.rows.children.iter_mut().zip(placed) {
            child.arrange(text, position.frame, clip);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
