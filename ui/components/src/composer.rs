//! Agent composer: canonical drafts arrive from presentation; editing belongs to TekesUI.
use crate::composer_layout::{ComposerActions, ComposerInputBand, ComposerMeasurements};
use crate::{
    AgentText, ComposerContext, ComposerContextAffordances, ComposerContextIntent,
    ComposerContextLabels,
};
use crate::{ComposerLayoutGeometry, ComposerPrimaryActionKind};
use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerDestination, ComposerDraft, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

/// Reserve these IDs in the embedding ContentHost, including hidden affordances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposerIds {
    pub root: ComponentId,
    pub editor: ComponentId,
    pub toolbar: ComponentId,
    pub send: ComponentId,
    pub cancel: ComponentId,
    pub attachment: ComponentId,
    pub model: ComponentId,
    pub preset: ComponentId,
    /// Actual input capsule containing the original editor and input actions.
    /// Context cards and setup controls are outside this subtree. Hosts may
    /// bind library padding-focus behavior here when their fixed SDK supports
    /// it; editor replacement requires rebinding the original mounted editor.
    pub input_band: ComponentId,
}
impl ComposerIds {
    /// Nine contiguous IDs; separate namespaces allow multiple composers in one window.
    pub fn new(base: u64) -> Self {
        assert!(base <= u64::MAX - 8, "Composer ID namespace overflow");
        Self {
            root: ComponentId(base),
            editor: ComponentId(base + 1),
            toolbar: ComponentId(base + 2),
            send: ComponentId(base + 3),
            cancel: ComponentId(base + 4),
            attachment: ComponentId(base + 5),
            model: ComponentId(base + 6),
            preset: ComponentId(base + 7),
            input_band: ComponentId(base + 8),
        }
    }
}

/// Explicit host entry points. Never infer installation, filesystem or model authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposerAffordances {
    pub attachment: bool,
    pub model: bool,
    pub preset: bool,
}
/// Additional pane authority from existing presentation facts, e.g. SideChat closing/unknown.
/// This only restricts canonical Run capabilities; it cannot grant an unavailable operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposerControlGate {
    pub send: bool,
    pub cancel: bool,
    pub entries: bool,
}
impl ComposerControlGate {
    pub const OPEN: Self = Self {
        send: true,
        cancel: true,
        entries: true,
    };
    pub const CLOSED: Self = Self {
        send: false,
        cancel: false,
        entries: false,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerEntry {
    Attachment,
    Model,
    Preset,
}

/// Shared defaults; the embedding product supplies its own copy/localization.
#[derive(Clone, Debug)]
pub struct ComposerLabels {
    pub editor: String,
    pub group: String,
    pub send: String,
    pub cancel: String,
    pub attachment: String,
    pub model: String,
    pub preset: String,
}
impl Default for ComposerLabels {
    fn default() -> Self {
        Self {
            editor: "消息".into(),
            group: "消息编辑与发送".into(),
            send: "发送".into(),
            cancel: "停止".into(),
            attachment: "添加附件".into(),
            model: "模型".into(),
            preset: "预设".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComposerIntent {
    Edit {
        destination: ComposerDestination,
        text: String,
    },
    Send {
        destination: ComposerDestination,
    },
    Cancel {
        scope: RunScope,
    },
    OpenEntry {
        destination: ComposerDestination,
        entry: ComposerEntry,
    },
    Context(Box<ComposerContextIntent>),
}

/// Independently mountable component. Does not own a runtime or dispatch network operations.
/// Drain ContentHost actions immediately after event/AX delivery and before update().
#[derive(Debug)]
pub struct Composer {
    ids: ComposerIds,
    body: StackContent,
    retained: BTreeMap<String, Content>,
    destination: Option<ComposerDestination>,
    draft: ComposerDraft,
    committed: String,
    revision: u64,
    presented: bool,
    can_send: bool,
    can_cancel: bool,
    affordances: ComposerAffordances,
    labels: ComposerLabels,
    pending: Vec<ComposerIntent>,
    notice_id: Option<ComponentId>,
    notice_epoch: Option<u64>,
    context_id: Option<ComponentId>,
    retained_context: Option<Content>,
    context_affordances: ComposerContextAffordances,
    primary_kind: ComposerPrimaryActionKind,
    retained_actions: BTreeMap<ComponentId, Content>,
    geometry: Option<ComposerLayoutGeometry>,
}
impl Composer {
    pub fn new(ids: ComposerIds) -> Self {
        Self {
            ids,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(ids.input_band, ComposerInputBand::new(ids)),
                    Content::new(ids.toolbar, ComposerActions::new(ids)),
                ],
            },
            retained: BTreeMap::new(),
            destination: None,
            draft: ComposerDraft::default(),
            committed: String::new(),
            revision: 0,
            presented: true,
            can_send: false,
            can_cancel: false,
            affordances: ComposerAffordances::default(),
            labels: ComposerLabels::default(),
            pending: Vec::new(),
            notice_id: None,
            notice_epoch: None,
            context_id: None,
            retained_context: None,
            context_affordances: Default::default(),
            primary_kind: ComposerPrimaryActionKind::Send,
            retained_actions: BTreeMap::new(),
            geometry: None,
        }
    }
    pub fn ids(&self) -> ComposerIds {
        self.ids
    }
    pub fn primary_action_kind(&self) -> ComposerPrimaryActionKind {
        self.primary_kind
    }
    pub fn primary_action_id(&self) -> ComponentId {
        match self.primary_kind {
            ComposerPrimaryActionKind::Send => self.ids.send,
            ComposerPrimaryActionKind::Stop => self.ids.cancel,
        }
    }
    /// Retained geometry is not current after update or while archived.
    pub fn layout_geometry(&self) -> Option<&ComposerLayoutGeometry> {
        self.geometry.as_ref().filter(|g| {
            self.presented && g.publication == self.revision && g.destination == self.destination
        })
    }
    /// Optional status notice uses a separate namespace above the input card.
    /// Its native controls do not replace the original editor or Run state.
    pub fn install_notice(
        &mut self,
        namespace: u32,
        labels: crate::ComposerNoticeLabels,
    ) -> Result<(), &'static str> {
        if self.notice_id.is_some()
            || ((self.ids.root.0 >> 32)..=(self.ids.input_band.0 >> 32))
                .contains(&(namespace as u64))
            || self
                .context_id
                .is_some_and(|id| id.0 >> 32 == namespace as u64)
        {
            return Err("composer notice namespace overlaps");
        }
        let notice = crate::ComposerNotice::new(namespace, labels);
        self.notice_id = Some(notice.id());
        self.body
            .children
            .insert(0, Content::new(notice.id(), notice));
        self.geometry = None;
        Ok(())
    }
    pub fn notice(&self) -> Option<&crate::ComposerNotice> {
        self.body
            .children
            .iter()
            .find(|node| Some(node.id()) == self.notice_id)?
            .component()
    }
    pub fn notice_mut(&mut self) -> Option<&mut crate::ComposerNotice> {
        self.body
            .children
            .iter_mut()
            .find(|node| Some(node.id()) == self.notice_id)?
            .component_mut()
    }
    /// IME host policy for this exact original preserving-focus action. This
    /// query grants neither a refresh request nor endpoint authority.
    pub fn is_nonfocusing_notice_refresh_target(&self, target: ComponentId) -> bool {
        self.presented
            && self.layout_geometry().is_some_and(|g| g.notice.is_some())
            && self.notice().is_some_and(|notice| {
                notice.refresh_id() == target
                    && notice.displayed_input().is_some_and(|input| {
                        self.destination.as_ref() == Some(&input.destination)
                            && self.notice_epoch == Some(input.connection_epoch)
                            && input.enabled
                            && input
                                .notice
                                .as_ref()
                                .is_some_and(|state| state.can_refresh && !state.refreshing)
                    })
            })
    }
    pub fn is_nonfocusing_notice_refresh_at(&self, point: tekes_ui_scene::Point) -> bool {
        let Some(notice) = self.notice() else {
            return false;
        };
        self.is_nonfocusing_notice_refresh_target(notice.refresh_id())
            && notice
                .children()
                .iter()
                .find(|node| node.id() == notice.refresh_id())
                .and_then(Content::visible)
                .is_some_and(|frame| frame.contains(point))
    }
    pub fn update_notice(
        &mut self,
        input: &crate::ComposerNoticeInput,
    ) -> Result<(), &'static str> {
        if !self.presented
            || self.destination.as_ref() != Some(&input.destination)
            || self.notice_epoch != Some(input.connection_epoch)
        {
            return Err("Composer notice belongs to another destination or connection");
        }
        self.notice_mut()
            .ok_or("Composer has no notice slot")?
            .update(input)?;
        self.geometry = None;
        Ok(())
    }
    pub fn with_context(
        mut self,
        namespace: u32,
        labels: ComposerContextLabels,
    ) -> Result<Self, &'static str> {
        if self.context_id.is_some()
            || self
                .notice_id
                .is_some_and(|id| id.0 >> 32 == namespace as u64)
            || ((self.ids.root.0 >> 32)..=(self.ids.input_band.0 >> 32))
                .contains(&(namespace as u64))
        {
            return Err("composer context namespace overlaps");
        }
        let context = ComposerContext::new(namespace, labels);
        self.context_id = Some(context.id());
        self.retained_context = Some(Content::new(context.id(), context));
        Ok(self)
    }
    pub fn context(&self) -> Option<&ComposerContext> {
        self.body
            .children
            .iter()
            .find(|node| Some(node.id()) == self.context_id)
            .or(self.retained_context.as_ref())?
            .component::<ComposerContext>()
    }
    fn context_mut(&mut self) -> Option<&mut ComposerContext> {
        let id = self.context_id?;
        if let Some(index) = self.body.children.iter().position(|node| node.id() == id) {
            self.body.children[index].component_mut::<ComposerContext>()
        } else {
            self.retained_context
                .as_mut()?
                .component_mut::<ComposerContext>()
        }
    }
    fn place_context(&mut self) {
        let Some(id) = self.context_id else { return };
        let visible = self
            .context()
            .is_some_and(|context| !context.children().is_empty());
        if visible {
            if let Some(context) = self.retained_context.take() {
                let index = usize::from(self.notice_id.is_some());
                self.body.children.insert(index, context);
            }
        } else if let Some(index) = self.body.children.iter().position(|node| node.id() == id) {
            self.retained_context = Some(self.body.children.remove(index));
        }
    }
    pub fn set_context_affordances(&mut self, affordances: ComposerContextAffordances) {
        self.context_affordances = affordances;
    }
    pub fn set_labels(&mut self, labels: ComposerLabels) {
        self.text_mut().field.field.label = labels.editor.clone();
        for editor in self.retained.values_mut() {
            editor
                .component_mut::<AgentText>()
                .expect("retained composer editor")
                .field
                .field
                .label = labels.editor.clone();
        }
        self.labels = labels;
        self.rebuild_actions();
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.ids.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn destination(&self) -> Option<&ComposerDestination> {
        self.destination.as_ref()
    }
    pub fn editor(&self) -> &TextSurface {
        &self.text().field.surface
    }
    /// Visibility follows ClientKit acceptsInput, separately from transport or
    /// admission authority. Interrupted/unavailable runs retain their Composer.
    pub fn is_presented(&self) -> bool {
        self.presented
    }
    pub fn is_composing(&self) -> bool {
        self.presented && self.editor().input.marked.is_some()
    }
    fn band(&self) -> &ComposerInputBand {
        self.body
            .children
            .iter()
            .find(|n| n.id() == self.ids.input_band)
            .unwrap()
            .component::<ComposerInputBand>()
            .unwrap()
    }
    fn band_mut(&mut self) -> &mut ComposerInputBand {
        let id = self.ids.input_band;
        self.body
            .children
            .iter_mut()
            .find(|n| n.id() == id)
            .unwrap()
            .component_mut::<ComposerInputBand>()
            .unwrap()
    }
    fn text(&self) -> &AgentText {
        self.band()
            .editor()
            .component::<AgentText>()
            .expect("composer editor")
    }
    fn text_mut(&mut self) -> &mut AgentText {
        self.band_mut()
            .editor_mut()
            .component_mut::<AgentText>()
            .expect("composer editor")
    }
    fn toolbar(&mut self) -> &mut ComposerActions {
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.toolbar)
            .unwrap()
            .component_mut::<ComposerActions>()
            .expect("composer toolbar")
    }
    fn local_edit(&mut self) {
        if !self.is_composing() && self.editor().input.text != self.committed {
            self.committed = self.editor().input.text.clone();
            if let Some(destination) = &self.destination {
                self.pending.push(ComposerIntent::Edit {
                    destination: destination.clone(),
                    text: self.committed.clone(),
                });
            }
        }
    }
    /// A single RunController is the revision authority for this component's lifetime.
    pub fn update(
        &mut self,
        snapshot: &RunSnapshot,
        affordances: ComposerAffordances,
    ) -> Result<(), &'static str> {
        self.update_gated(snapshot, affordances, ComposerControlGate::OPEN)
    }
    /// Preflight the Agent policy for a transaction on an already mounted
    /// original destination. The caller validates returned arguments through
    /// TekesUI's public controlled-value check; no generic checks are copied.
    pub(crate) fn prepare_current_update(
        &self,
        snapshot: &RunSnapshot,
    ) -> Result<Option<crate::ComposerDraftUpdate>, &'static str> {
        if snapshot.revision == 0 || snapshot.revision < self.revision {
            return Err("stale or unversioned composer input");
        }
        let target = snapshot.selection.as_ref().map(|s| &s.destination);
        if self.destination.as_ref() != target || target.is_none() {
            return Err("initialize the original Composer before transactional updates");
        }
        if let Some(context) = self.context() {
            context.check_update(snapshot, self.context_affordances)?;
        }
        self.text().prepare_sync_draft(
            &self.destination.as_ref().unwrap().storage_key(),
            snapshot.revision,
            &snapshot.draft.text,
        )
    }
    pub fn update_gated(
        &mut self,
        snapshot: &RunSnapshot,
        affordances: ComposerAffordances,
        gate: ComposerControlGate,
    ) -> Result<(), &'static str> {
        if snapshot.revision == 0 || snapshot.revision < self.revision {
            return Err("stale or unversioned composer input");
        }
        let destination = snapshot
            .selection
            .as_ref()
            .map(|selection| selection.destination.clone());
        if destination != self.destination && self.is_composing() {
            return Err("finish composition before changing composer destination");
        }
        // A rejected card frame must not first synchronize the controlled draft
        // or enqueue local edits. Generic editor validation stays in TekesUI.
        if let Some(context) = self.context() {
            context.check_update(snapshot, self.context_affordances)?;
        }
        self.local_edit();
        if destination != self.destination {
            let key = destination
                .as_ref()
                .map(ComposerDestination::storage_key)
                .unwrap_or_default();
            let editor = self
                .retained
                .remove(&key)
                .unwrap_or_else(|| Content::new(self.ids.editor, AgentText::composer()));
            let mut previous = std::mem::replace(self.band_mut().editor_mut(), editor);
            previous
                .component_mut::<AgentText>()
                .expect("composer editor")
                .set_focus(false, false);
            if let Some(old) = &self.destination {
                self.retained.insert(old.storage_key(), previous);
            }
            if let Some(notice) = self.notice_mut() {
                notice.clear();
            }
            self.destination = destination;
            self.text_mut().field.field.label = self.labels.editor.clone();
            self.committed = snapshot.draft.text.clone();
        }
        let key = self
            .destination
            .as_ref()
            .map(ComposerDestination::storage_key)
            .unwrap_or_else(|| "unselected".into());
        if let Err(error) =
            self.text_mut()
                .sync_draft(&key, snapshot.revision, &snapshot.draft.text)
        {
            self.can_send = false;
            self.rebuild_actions();
            if let Some(notice) = self.notice_mut() {
                notice.clear();
            }
            self.notice_epoch = None;
            return Err(error);
        }
        if self.notice_epoch != Some(snapshot.connection_epoch)
            && let Some(notice) = self.notice_mut()
        {
            notice.clear();
        }
        self.notice_epoch = Some(snapshot.connection_epoch);
        self.presented = snapshot
            .run
            .as_ref()
            .is_none_or(|run| run.phase != tekes_agent_presentation::RunPhase::Archived);
        let gate = if self.presented {
            gate
        } else {
            ComposerControlGate::CLOSED
        };
        if !self.presented {
            self.pending.clear();
            if let Some(notice) = self.notice_mut() {
                notice.clear();
            }
        }
        self.revision = snapshot.revision;
        self.draft = snapshot.draft.clone();
        let context_editable = gate.entries && !self.is_composing();
        let context_affordances = self.context_affordances;
        if let Some(context) = self.context_mut() {
            context.update(snapshot, context_editable, context_affordances)?;
        }
        self.place_context();
        self.affordances = if gate.entries {
            affordances
        } else {
            ComposerAffordances::default()
        };
        // can_submit describes transport/admission authority. Content readiness
        // still comes from the shared compiler, including artifact-only input;
        // the controller rechecks actual command capabilities at admission.
        self.can_send = gate.send
            && self.destination.is_some()
            && snapshot.can_submit
            && snapshot.draft.compile(true).is_ok();
        self.can_cancel = gate.cancel && snapshot.run.as_ref().is_some_and(|run| run.can_cancel);
        let canonical_send_ready = snapshot.can_submit && snapshot.draft.compile(true).is_ok();
        self.primary_kind =
            if snapshot.run.as_ref().is_some_and(|run| run.can_cancel) && !canonical_send_ready {
                ComposerPrimaryActionKind::Stop
            } else {
                ComposerPrimaryActionKind::Send
            };
        self.geometry = None;
        if !self.is_composing() && self.editor().input.text == self.draft.text {
            self.committed = self.draft.text.clone();
        }
        self.rebuild_actions();
        Ok(())
    }
    fn rebuild_actions(&mut self) {
        let blocked = self.is_composing();
        if let Some(context) = self.context_mut() {
            context.set_blocked(blocked);
        }
        // Retain existing ButtonContent instances; painting/updates never synthesize activation.
        let ids = self.ids;
        let entries = [
            (
                ids.attachment,
                self.labels.attachment.clone(),
                self.affordances.attachment,
            ),
            (ids.model, self.labels.model.clone(), self.affordances.model),
            (
                ids.preset,
                self.labels.preset.clone(),
                self.affordances.preset,
            ),
        ];
        let cancel_label = self.labels.cancel.clone();
        let send_label = self.labels.send.clone();
        let composing = self.is_composing();
        let has_destination = self.destination.is_some();
        let send_enabled =
            self.can_send && !composing && self.editor().input.text == self.draft.text;
        let cancel_enabled = self.can_cancel;
        let primary = self.primary_action_id();
        let mut old = std::mem::take(&mut self.toolbar().children);
        // Keep the original editor in its fixed band slot; move only action
        // Content instances between mounted and retained business roles.
        let band = self.band_mut();
        let mut index = 0;
        while index < band.actions.children.len() {
            if band.actions.children[index].id() != ids.editor {
                old.push(band.actions.children.remove(index));
            } else {
                index += 1;
            }
        }
        let mut retained = std::mem::take(&mut self.retained_actions);
        let mut children = Vec::new();
        for (id, label, visible) in [
            (ids.cancel, cancel_label, primary == ids.cancel),
            (ids.send, send_label, primary == ids.send),
        ]
        .into_iter()
        .chain(entries)
        {
            if !visible {
                continue;
            }
            let mut button = if let Some(index) = old.iter().position(|child| child.id() == id) {
                old.remove(index)
            } else if let Some(button) = retained.remove(&id) {
                button
            } else {
                Content::new(id, ButtonContent::new(label.clone()))
            };
            button
                .component_mut::<ButtonContent>()
                .expect("composer action")
                .label = label;
            let enabled = if id == ids.send {
                send_enabled
            } else if id == ids.cancel {
                cancel_enabled
            } else {
                has_destination && !composing
            };
            button
                .component_mut::<ButtonContent>()
                .expect("composer action")
                .button
                .state
                .disabled = !enabled;
            children.push(button);
        }
        for mut node in old {
            node.component_mut::<ButtonContent>()
                .unwrap()
                .button
                .clear_layout();
            retained.insert(node.id(), node);
        }
        self.retained_actions = retained;
        let mut setup = vec![];
        let band = self.band_mut();
        band.actions.primary = primary;
        for child in children {
            if child.id() == primary || child.id() == ids.attachment {
                band.actions.children.push(child);
            } else {
                setup.push(child);
            }
        }
        let toolbar = self.toolbar();
        toolbar.primary = primary;
        toolbar.children = setup;
    }
    /// Typed intents retain the exact presented destination. Submit/Enter never sends.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<ComposerIntent> {
        if !self.presented {
            self.pending.clear();
            return Vec::new();
        }
        if !self.is_composing()
            && let Some(context) = self.context()
        {
            let intents: Vec<_> = context
                .take_intents(actions)
                .into_iter()
                .filter(|intent| match (&intent.item, intent.action) {
                    (_, crate::ComposerContextAction::Remove) => true,
                    (_, crate::ComposerContextAction::EditAnnotation) => {
                        self.context_affordances.edit_annotations
                    }
                    (
                        crate::ComposerContextItem::Artifact(artifact),
                        crate::ComposerContextAction::Open,
                    ) => {
                        self.context_affordances.preview_artifacts
                            || self.context_affordances.can_preview_file(artifact)
                    }
                    (
                        crate::ComposerContextItem::Annotation(_),
                        crate::ComposerContextAction::Open,
                    ) => self.context_affordances.reveal_annotations,
                })
                .map(|intent| ComposerIntent::Context(Box::new(intent)))
                .collect();
            self.pending.extend(intents);
        }
        self.local_edit();
        for action in actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
        {
            let Some(destination) = self.destination.clone() else {
                continue;
            };
            if action.target == self.ids.cancel
                && self.primary_kind == ComposerPrimaryActionKind::Stop
                && self.can_cancel
            {
                if let ComposerDestination::Run { scope } = destination {
                    self.pending.push(ComposerIntent::Cancel { scope });
                }
            } else if !self.is_composing() {
                if action.target == self.ids.send
                    && self.primary_kind == ComposerPrimaryActionKind::Send
                    && self.can_send
                    && self.editor().input.text == self.draft.text
                {
                    let baseline = self.editor().input.external_baseline();
                    self.text_mut().clear_baseline = Some(baseline);
                    self.pending.push(ComposerIntent::Send { destination });
                } else {
                    let entry =
                        if action.target == self.ids.attachment && self.affordances.attachment {
                            Some(ComposerEntry::Attachment)
                        } else if action.target == self.ids.model && self.affordances.model {
                            Some(ComposerEntry::Model)
                        } else if action.target == self.ids.preset && self.affordances.preset {
                            Some(ComposerEntry::Preset)
                        } else {
                            None
                        };
                    if let Some(entry) = entry {
                        self.pending
                            .push(ComposerIntent::OpenEntry { destination, entry });
                    }
                }
            }
        }
        self.rebuild_actions();
        std::mem::take(&mut self.pending)
    }
    /// Explicit lifecycle release, e.g. confirmed session discard. Never evict the mounted editor.
    pub fn forget(&mut self, destination: &ComposerDestination) -> Result<(), &'static str> {
        if self.destination.as_ref() == Some(destination) {
            return Err("detach the active destination before forgetting it");
        }
        self.retained.remove(&destination.storage_key());
        Ok(())
    }
    fn measurements(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        width: f32,
        height: f32,
    ) -> ComposerMeasurements {
        // The business cap is ten lines; shaping and scrolling stay in the
        // library's original editor across font-scale changes.
        let scale = text
            .text_surface_host()
            .map_or(1., |host| host.type_scale());
        let scale = if scale.is_finite() { scale.max(1.) } else { 1. };
        let max = self.editor().role.style().line_height * scale * 10.;
        let padding = Spacing::Snug.value();
        let inner = (width - padding * 2.).max(0.);
        let actions = self.band_mut().actions.group(text, inner, true);
        let setup = self.toolbar().group(text, inner, false);
        let notice = self
            .body
            .children
            .iter_mut()
            .find(|node| Some(node.id()) == self.notice_id)
            .map_or(Size::default(), |node| {
                node.measure(text, Size::new(width, 1e8)).size
            });
        let context = self
            .body
            .children
            .iter_mut()
            .find(|node| Some(node.id()) == self.context_id)
            .map_or(Size::default(), |node| {
                node.measure(text, Size::new(width, 1e8)).size
            });
        let overhead = (if notice.height > 0. {
            notice.height + Spacing::Snug.value()
        } else {
            0.
        }) + 2. * (Spacing::Snug.value() + Spacing::Regular.value())
            + if context.height > 0. {
                context.height + Spacing::Regular.value()
            } else {
                0.
            }
            + if setup.height > 0. {
                setup.height + Spacing::Snug.value()
            } else {
                0.
            };
        let max = max
            .min((height - overhead).max(0.))
            .max(self.editor().minimum_outer_height(scale));
        self.text_mut().field.viewport = TextViewport::Grow { min: 0., max };
        // Default library actions may be shorter than the single-line editor
        // envelope. Reserve their real trailing inset before shaping the input,
        // so a type-scale change cannot overlap the text and action targets.
        let minimum_envelope =
            self.editor().minimum_outer_height(scale) + 2. * Spacing::Regular.value();
        let trailing = padding + (minimum_envelope - actions.height).max(0.) / 2.;
        let editor_width =
            (width - Spacing::Loose.value() - trailing - actions.width - Spacing::Regular.value())
                .max(0.);
        let editor = self
            .band_mut()
            .editor_mut()
            .measure(text, Size::new(editor_width, 1e8))
            .size;
        ComposerMeasurements {
            notice,
            context,
            editor,
            actions,
            setup,
            row_height: (editor.height + Spacing::Regular.value() * 2.).max(actions.height),
        }
    }
}
impl Component for Composer {
    fn active(&self) -> bool {
        self.presented
    }
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if !self.presented {
            return MeasuredChild::new(Size::new(0., 0.));
        }
        let measured = self.measurements(text, available.width, available.height);
        MeasuredChild::new(Size::new(available.width, measured.height()))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if !self.presented {
            return;
        }
        let measured = self.measurements(text, frame.size.width, frame.size.height);
        // A tall context stack may exceed a small host. Preserve the resident
        // input/actions at the bottom; do not let cards push Send/Stop off-screen.
        let content_top = frame.min_y() - (measured.height() - frame.size.height).max(0.);
        let band = Rect::new(
            frame.min_x(),
            content_top + measured.notice_clearance() + measured.context_clearance(),
            frame.size.width,
            measured.band_height(),
        );
        let notice = if measured.notice.height > 0. {
            let rect = Rect::new(
                frame.min_x(),
                content_top,
                frame.size.width,
                measured.notice.height,
            );
            self.body
                .children
                .iter_mut()
                .find(|node| Some(node.id()) == self.notice_id)
                .unwrap()
                .arrange(text, rect, clip);
            Some(rect)
        } else {
            None
        };
        let context = if measured.context.height > 0. {
            let rect = Rect::new(
                frame.min_x(),
                content_top + measured.notice_clearance(),
                frame.size.width,
                measured.context.height,
            );
            self.body
                .children
                .iter_mut()
                .find(|node| Some(node.id()) == self.context_id)
                .unwrap()
                .arrange(text, rect, clip);
            Some(rect)
        } else {
            None
        };
        let envelope = measured.editor.height + 2. * Spacing::Regular.value();
        let row_center = band.min_y() + Spacing::Snug.value() + measured.row_height / 2.;
        let editor = Rect::new(
            band.min_x() + Spacing::Loose.value(),
            row_center - envelope / 2. + Spacing::Regular.value(),
            measured.editor.width,
            measured.editor.height,
        );
        let band_id = self.ids.input_band;
        let input = self.band_mut();
        input.editor_frame = editor;
        input.row_center = row_center;
        input.actions_size = measured.actions;
        self.body
            .children
            .iter_mut()
            .find(|n| n.id() == band_id)
            .unwrap()
            .arrange(text, band, clip);
        let setup = (measured.setup.height > 0.).then(|| {
            Rect::new(
                frame.min_x(),
                band.max_y() + Spacing::Snug.value(),
                frame.size.width,
                measured.setup.height,
            )
        });
        let toolbar_id = self.ids.toolbar;
        let toolbar = self.toolbar();
        toolbar.input_frame = Rect::default();
        toolbar.setup_frame = setup.unwrap_or_default();
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == toolbar_id)
            .unwrap()
            .arrange(text, frame, clip);
        let actions = &self.band().actions;
        let primary = actions
            .children
            .iter()
            .find(|node| node.id() == actions.primary)
            .map(Content::frame)
            .unwrap_or_default();
        let attachment = actions
            .children
            .iter()
            .find(|node| node.id() == actions.ids.attachment)
            .map(Content::frame);
        self.geometry = Some(ComposerLayoutGeometry {
            publication: self.revision,
            destination: self.destination.clone(),
            notice,
            context,
            input_band: band,
            editor,
            primary,
            attachment,
            setup,
        });
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}

#[cfg(test)]
mod input_band_tests {
    use super::*;

    #[test]
    fn input_capsule_contains_only_original_editor_and_input_actions() {
        let ids = ComposerIds::new(100);
        let mut view = Composer::new(ids);
        view.affordances = ComposerAffordances {
            attachment: true,
            model: true,
            preset: true,
        };
        view.rebuild_actions();
        assert_eq!(
            view.band()
                .children()
                .iter()
                .map(Content::id)
                .collect::<Vec<_>>(),
            vec![ids.editor, ids.send, ids.attachment]
        );
        assert_eq!(
            view.toolbar()
                .children()
                .iter()
                .map(Content::id)
                .collect::<Vec<_>>(),
            vec![ids.model, ids.preset]
        );
        assert!(view.band().fixed_slots());
        assert!(view.band().editor().component::<AgentText>().is_some());
        view.primary_kind = ComposerPrimaryActionKind::Stop;
        view.rebuild_actions();
        assert_eq!(
            view.band()
                .children()
                .iter()
                .map(Content::id)
                .collect::<Vec<_>>(),
            vec![ids.editor, ids.cancel, ids.attachment]
        );
        view.affordances = ComposerAffordances::default();
        view.rebuild_actions();
        assert_eq!(
            view.band()
                .children()
                .iter()
                .map(Content::id)
                .collect::<Vec<_>>(),
            vec![ids.editor, ids.cancel]
        );
        assert!(view.toolbar().children().is_empty());
    }
}
