//! Turn-level output aside; no window, endpoint or independent Run state.
use crate::OutputUsage;
use crate::{TranscriptCopyIntent, TranscriptCopySource, TranscriptTurnCopy};
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
    pub display: crate::TurnFootnoteDisplay,
}
impl TurnFootnoteFacts {
    pub fn read(turn: &TranscriptTurn) -> Self {
        Self::read_with_display(turn, Default::default())
    }
    pub fn read_with_display(turn: &TranscriptTurn, display: crate::TurnFootnoteDisplay) -> Self {
        Self {
            turn_id: turn.id.clone(),
            status: turn.status.clone(),
            started_at: turn.started_at,
            ended_at: turn.ended_at,
            output_usage: OutputUsage::read_turn(turn),
            display,
        }
    }
    /// A settled interval converted from Unix milliseconds to seconds.
    /// Running elapsed time needs an explicit host clock.
    pub fn elapsed_seconds(&self) -> Option<f64> {
        self.started_at.zip(self.ended_at).and_then(|(start, end)| {
            let elapsed = (end - start) / 1000.;
            (start.is_finite()
                && start > 0.
                && end.is_finite()
                && elapsed.is_finite()
                && elapsed >= 0.)
                .then_some(elapsed)
        })
    }
    /// The live aside contains elapsed time and positive reported token count.
    /// A settled aside contains only the host's localized source end timestamp.
    pub fn text(&self) -> String {
        if !self.display.is_live_owner {
            return self.display.finished_text.clone().unwrap_or_default();
        }
        let mut text = self
            .started_at
            .zip(self.display.now_ms)
            .and_then(|(start, now)| {
                let elapsed = (now - start) / 1000.;
                (start.is_finite()
                    && start > 0.
                    && elapsed.is_finite()
                    && elapsed >= 0.
                    && elapsed < u64::MAX as f64)
                    .then(|| Self::elapsed_text(elapsed.floor() as u64))
            })
            .unwrap_or_default();
        if let Some(OutputUsage::Live { output_tokens }) = &self.output_usage
            && *output_tokens > 0
        {
            let count = if *output_tokens < 1000 {
                output_tokens.to_string()
            } else {
                format!("{:.1}k", *output_tokens as f64 / 1000.)
            };
            text.push_str(&format!("   {count} tokens"));
        }
        text
    }
    pub fn elapsed_text(seconds: u64) -> String {
        let mut parts = vec![];
        for (value, unit) in [
            (seconds / 86400, "d"),
            (seconds % 86400 / 3600, "h"),
            (seconds % 3600 / 60, "m"),
        ] {
            if value > 0 {
                parts.push(format!("{value}{unit}"));
            }
        }
        parts.push(format!("{}s", seconds % 60));
        parts.join(" ")
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
    /// Host-localized terminal state label; basic style remains SDK-owned.
    pub goal_state: fn(tekes_agent_presentation::GoalPhase) -> String,
    /// Calendar/locale adapter. Receives source Unix ms and the explicit display clock.
    pub finished_timestamp: fn(f64, Option<f64>) -> Option<String>,
}
impl Default for TurnFootnoteLabels {
    fn default() -> Self {
        Self {
            copy: "复制输出".into(),
            group: "回合信息".into(),
            pending: "正在处理回合操作".into(),
            completed: "操作已完成".into(),
            unknown: "操作结果未确认，请先核对；不会自动重试".into(),
            goal_state: |phase| format!("目标 {}", phase.as_str()),
            information: TurnFootnoteFacts::text,
            finished_timestamp: |_, _| None,
        }
    }
}
/// Read-only aside captions use SDK single-line labels. Hosts must provide
/// `TextSurfaceHost::shape_label`; complete text remains in the original nodes.
#[derive(Debug)]
pub struct TurnFootnote {
    feedback_namespace: Option<u32>,
    feedback: Option<Content>,
    feedback_shown: bool,
    shown: bool,
    display: crate::TurnFootnoteDisplay,
    information: Option<Content>,
    information_text: String,
    output_copy: Option<OutputCopy>,
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
            feedback_namespace: None,
            feedback: None,
            feedback_shown: false,
            shown: false,
            display: Default::default(),
            information: None,
            information_text: String::new(),
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
                children: vec![aside_information(ComponentId(root.0 + 1), "")],
            },
            actions: BTreeMap::new(),
            next: 16,
            labels,
            pending: None,
            observed: None,
            notice: String::new(),
        }
    }
    pub(crate) fn reserve_feedback_namespace(&mut self, namespace: u32) {
        self.feedback_namespace = Some(namespace);
    }
    pub fn feedback(&self) -> Option<&crate::TurnFeedback> {
        if !self.feedback_shown {
            return None;
        }
        self.rows
            .children
            .iter()
            .find_map(|node| node.component::<crate::TurnFeedback>())
    }
    pub fn feedback_mut(&mut self) -> Option<&mut crate::TurnFeedback> {
        if !self.feedback_shown {
            return None;
        }
        self.rows
            .children
            .iter_mut()
            .find_map(|node| node.component_mut::<crate::TurnFeedback>())
    }
    pub(crate) fn check_feedback(
        &self,
        target: Option<&tekes_agent_presentation::FeedbackTargetCapture>,
    ) -> Result<(), &'static str> {
        if let Some(control) = self.feedback()
            && control.is_composing()
            && control.target() != target
        {
            return Err("finish feedback composition before replacing its response");
        }
        Ok(())
    }
    pub(crate) fn configure_feedback(
        &mut self,
        target: Option<tekes_agent_presentation::FeedbackTargetCapture>,
        labels: Option<&crate::FeedbackLabels>,
    ) {
        let existing = self
            .rows
            .children
            .iter()
            .position(|node| node.component::<crate::TurnFeedback>().is_some())
            .map(|index| self.rows.children.remove(index))
            .or_else(|| self.feedback.take());
        self.feedback_shown = target.is_some() && labels.is_some();
        if existing.is_some() || self.feedback_shown {
            let labels = labels.cloned().unwrap_or_default();
            let mut node = existing.unwrap_or_else(|| {
                let control = crate::TurnFeedback::new(
                    self.feedback_namespace
                        .expect("reserved feedback namespace"),
                    labels.clone(),
                );
                Content::new(control.ids().root, control)
            });
            node.component_mut::<crate::TurnFeedback>()
                .unwrap()
                .set_aside_target(target, &labels);
            self.feedback = Some(node);
        }
        self.render_actions();
    }
    pub fn scope(&self) -> &RunScope {
        &self.scope
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn action_id(&self, id: &str) -> Option<ComponentId> {
        if !self.shown || self.display.is_live_owner {
            return None;
        }
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
        self.turn
            .as_ref()
            .map(|turn| TurnFootnoteFacts::read_with_display(turn, self.display.clone()))
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
        self.update_with_display(
            scope,
            publication,
            epoch,
            turn,
            available,
            output,
            &Default::default(),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update_with_display(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        output: Option<TranscriptOutput<'_>>,
        display: &crate::TurnFootnoteDisplay,
    ) -> Result<(), &'static str> {
        self.update_copy(
            scope,
            publication,
            epoch,
            turn,
            available,
            copy_facts(turn, output)?,
            display,
        )
    }
    /// Whole-turn display text captured from the original canonical snapshot.
    /// This does not change the response target or grant mutation capabilities.
    #[allow(clippy::too_many_arguments)]
    pub fn update_with_turn_copy(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        copy: Option<&TranscriptTurnCopy>,
        display: &crate::TurnFootnoteDisplay,
    ) -> Result<(), &'static str> {
        self.update_copy(
            scope,
            publication,
            epoch,
            turn,
            available,
            turn_copy_facts(turn, copy)?,
            display,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn update_copy(
        &mut self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        copy: Option<OutputCopy>,
        display: &crate::TurnFootnoteDisplay,
    ) -> Result<(), &'static str> {
        let shown = display.shows_aside(turn, copy.is_some());
        let visibility_changed =
            self.shown != shown || self.display.is_live_owner != display.is_live_owner;
        let rekey = self.check_update_with_copy(
            scope,
            publication,
            epoch,
            turn,
            available,
            copy.as_ref(),
            display,
        )? || visibility_changed;
        if copy != self.output_copy || epoch != self.epoch || visibility_changed {
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
        self.shown = shown;
        self.display = display.clone();
        self.information_text =
            (self.labels.information)(&TurnFootnoteFacts::read_with_display(turn, display.clone()));
        if rekey {
            self.rekey();
        }
        self.refresh();
        self.render_actions();
        Ok(())
    }
    pub fn goal_state_id(&self) -> Option<ComponentId> {
        (self.shown && !self.display.is_live_owner && self.display.goal_state.is_some())
            .then_some(ComponentId(self.root.0 + 3))
    }
    pub fn information_id(&self) -> Option<ComponentId> {
        (self.shown && !self.information_text.is_empty()).then_some(ComponentId(self.root.0 + 1))
    }
    pub fn copy_button_id(&self) -> Option<ComponentId> {
        if !self.shown || self.display.is_live_owner {
            return None;
        }
        self.copy_id
    }
    pub fn take_copy_intents(&self, actions: &[ContentAction]) -> Vec<TranscriptCopyIntent> {
        if !self.shown || self.display.is_live_owner {
            return vec![];
        }
        let Some(copy) = &self.output_copy else {
            return vec![];
        };
        if !actions
            .iter()
            .any(|a| Some(a.target) == self.copy_id && a.kind == ContentActionKind::Activate)
        {
            return vec![];
        }
        vec![TranscriptCopyIntent::from_source(
            self.scope.clone(),
            copy.source.clone(),
            copy.text.clone(),
        )]
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_update_with_turn_copy(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        copy: Option<&TranscriptTurnCopy>,
        display: &crate::TurnFootnoteDisplay,
    ) -> Result<bool, &'static str> {
        self.check_update_with_copy(
            scope,
            publication,
            epoch,
            turn,
            available,
            turn_copy_facts(turn, copy)?.as_ref(),
            display,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn check_update_with_copy(
        &self,
        scope: &RunScope,
        publication: u64,
        epoch: u64,
        turn: &TranscriptTurn,
        available: bool,
        copy: Option<&OutputCopy>,
        display: &crate::TurnFootnoteDisplay,
    ) -> Result<bool, &'static str> {
        display.validate(turn)?;
        if epoch == self.epoch
            && (display.revision < self.display.revision
                || (publication == self.publication
                    && display.revision == self.display.revision
                    && display != &self.display))
        {
            return Err("stale or conflicting footnote display");
        }
        let rekey = self.check_update(scope, publication, epoch, turn, available)?;
        if publication == self.publication && copy != self.output_copy.as_ref() {
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
        let mut previous = std::mem::take(&mut self.rows.children);
        if let Some(index) = previous
            .iter()
            .position(|node| node.component::<crate::TurnFeedback>().is_some())
        {
            self.feedback = Some(previous.remove(index));
        }
        let info_id = ComponentId(self.root.0 + 1);
        let mut information = previous
            .iter()
            .position(|node| node.id() == info_id)
            .map(|index| previous.remove(index))
            .or_else(|| self.information.take())
            .expect("retained original footnote information");
        information.component_mut::<TextContent>().unwrap().text = self.information_text.clone();
        if !self.shown {
            self.information = Some(information);
            return;
        }
        if self.display.is_live_owner {
            if self.information_text.is_empty() {
                self.information = Some(information);
            } else {
                self.rows.children.push(information);
            }
            return;
        }
        if let Some(id) = self.copy_id {
            let mut node = previous
                .iter()
                .position(|node| node.id() == id)
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| {
                    Content::new(id, ButtonContent::new(&self.labels.copy)).preserving_focus()
                });
            node.component_mut::<ButtonContent>().unwrap().label = self.labels.copy.clone();
            self.rows.children.push(node);
        }
        if self.feedback_shown
            && let Some(control) = self.feedback.take()
        {
            self.rows.children.push(control);
        }
        for (id, action) in &self.actions {
            let mut node = previous
                .iter()
                .position(|node| node.id() == *id)
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| {
                    Content::new(*id, ButtonContent::new(&action.label)).preserving_focus()
                });
            let button = node.component_mut::<ButtonContent>().unwrap();
            button.label = action.label.clone();
            button.button.state.disabled =
                !self.available || self.pending.is_some() || !action.enabled;
            self.rows.children.push(node);
            if let Some(reason) = &action.disabled_reason {
                self.rows
                    .children
                    .push(aside_information(ComponentId(id.0 + (1_u64 << 31)), reason));
            }
        }
        if self.information_text.is_empty() {
            self.information = Some(information);
        } else {
            self.rows.children.push(information);
        }
        if let Some(phase) = &self.display.goal_state {
            let id = ComponentId(self.root.0 + 3);
            let mut node = previous
                .iter()
                .position(|node| node.id() == id)
                .map(|index| previous.remove(index))
                .unwrap_or_else(|| aside_information(id, ""));
            node.component_mut::<TextContent>().unwrap().text =
                (self.labels.goal_state)(phase.clone());
            self.rows.children.push(node);
        }
        let notice = if self.pending.is_some() && self.observed.is_none() {
            &self.labels.pending
        } else {
            &self.notice
        };
        if !notice.is_empty() {
            self.rows
                .children
                .push(aside_information(ComponentId(self.root.0 + 2), notice));
        }
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<TurnFootnoteIntent> {
        self.refresh();
        if !self.shown || self.display.is_live_owner || !self.available || self.pending.is_some() {
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
#[derive(Clone, Debug, PartialEq, Eq)]
struct OutputCopy {
    source: TranscriptCopySource,
    provenance: Vec<crate::turn_output_copy::CopyProvenance>,
    text: String,
}
fn turn_copy_facts(
    turn: &TranscriptTurn,
    copy: Option<&TranscriptTurnCopy>,
) -> Result<Option<OutputCopy>, &'static str> {
    let Some(copy) = copy else {
        return Ok(None);
    };
    if copy.turn_id() != turn.id {
        return Err("turn output copy belongs to another turn");
    }
    Ok(Some(OutputCopy {
        source: TranscriptCopySource::TurnOutput {
            turn_id: copy.turn_id().into(),
            item_ids: copy.item_ids().into(),
        },
        text: copy.text().into(),
        provenance: copy.provenance.clone(),
    }))
}
fn copy_facts(
    turn: &TranscriptTurn,
    output: Option<TranscriptOutput<'_>>,
) -> Result<Option<OutputCopy>, &'static str> {
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
    Ok(Some(OutputCopy {
        source: TranscriptCopySource::Message(output.key.into()),
        provenance: vec![],
        text: output.item.text.clone(),
    }))
}
impl Component for TurnFootnote {
    fn active(&self) -> bool {
        self.shown && !self.rows.children.is_empty()
    }
    fn children(&self) -> &[Content] {
        &self.rows.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.rows.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.rows.model.axis = if self.feedback().is_some_and(|control| control.is_open()) {
            Axis::Vertical
        } else {
            Axis::Horizontal
        };
        let measured = self.rows.measure(text, available);
        MeasuredChild::new(Size::new(available.width, measured.size.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let horizontal = self.rows.model.axis == Axis::Horizontal;
        let mut remaining = frame.size.width;
        let sizes: Vec<_> = self
            .rows
            .children
            .iter_mut()
            .map(|child| {
                // ClientKit's output information uses the space after preceding
                // controls, not the 40% cap reserved for action labels (notes).
                let width = if horizontal && child.component::<TextContent>().is_some() {
                    remaining.max(1.)
                } else {
                    frame.size.width
                };
                let measured = child.measure(text, Size::new(width, frame.size.height));
                if horizontal {
                    remaining -= measured.size.width + Spacing::Tight.value();
                }
                measured
            })
            .collect();
        let placed = layout_measured_children(
            frame,
            self.rows.model.axis,
            &sizes,
            Spacing::Tight.value(),
            if self.rows.model.axis == Axis::Horizontal {
                proposal::layout::ROW
            } else {
                proposal::layout::COLUMN
            },
            AlignmentOverride {
                cross_axis: Some(if self.rows.model.axis == Axis::Horizontal {
                    proposal::layout::CrossAxisAlignment::Center
                } else {
                    proposal::layout::CrossAxisAlignment::Leading
                }),
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

fn aside_information(id: ComponentId, text: &str) -> Content {
    Content::new(
        id,
        TextContent {
            text: text.into(),
            role: proposal::TextRole::UiCaption,
        },
    )
    .with_label_layout(tekes_ui_text::LabelOptions::single_line(
        tekes_ui_text::LabelOverflow::TailEllipsis,
    ))
    .expect("original output aside information is an SDK text label")
}
