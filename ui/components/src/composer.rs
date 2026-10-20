//! Agent composer: canonical drafts arrive from presentation; editing belongs to TekesUI.
use crate::{
    AgentText, ComposerContext, ComposerContextAffordances, ComposerContextIntent,
    ComposerContextLabels,
};
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
}
impl ComposerIds {
    /// Eight contiguous IDs; separate namespaces allow multiple composers in one window.
    pub fn new(base: u64) -> Self {
        assert!(base <= u64::MAX - 7, "Composer ID namespace overflow");
        Self {
            root: ComponentId(base),
            editor: ComponentId(base + 1),
            toolbar: ComponentId(base + 2),
            send: ComponentId(base + 3),
            cancel: ComponentId(base + 4),
            attachment: ComponentId(base + 5),
            model: ComponentId(base + 6),
            preset: ComponentId(base + 7),
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
    context_id: Option<ComponentId>,
    retained_context: Option<Content>,
    context_affordances: ComposerContextAffordances,
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
                    Content::new(ids.editor, AgentText::new(false)),
                    Content::new(
                        ids.toolbar,
                        StackContent {
                            model: Stack {
                                axis: Axis::Horizontal,
                                gap: Spacing::Tight.value(),
                            },
                            flexible_child: None,
                            children: Vec::new(),
                        },
                    ),
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
            context_id: None,
            retained_context: None,
            context_affordances: Default::default(),
        }
    }
    pub fn ids(&self) -> ComposerIds {
        self.ids
    }
    /// Optional draft cards use a separately reserved namespace. The original
    /// eight Composer IDs and editor/undo instance remain unchanged.
    pub fn with_context(
        mut self,
        namespace: u32,
        labels: ComposerContextLabels,
    ) -> Result<Self, &'static str> {
        if self.context_id.is_some()
            || ((self.ids.root.0 >> 32)..=(self.ids.preset.0 >> 32)).contains(&(namespace as u64))
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
                self.body.children.insert(0, context);
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
    fn text(&self) -> &AgentText {
        self.body
            .children
            .iter()
            .find(|node| node.id() == self.ids.editor)
            .unwrap()
            .component::<AgentText>()
            .expect("composer editor")
    }
    fn text_mut(&mut self) -> &mut AgentText {
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.editor)
            .unwrap()
            .component_mut::<AgentText>()
            .expect("composer editor")
    }
    fn toolbar(&mut self) -> &mut StackContent {
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.toolbar)
            .unwrap()
            .component_mut::<StackContent>()
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
                .unwrap_or_else(|| Content::new(self.ids.editor, AgentText::new(false)));
            let mut previous = std::mem::replace(
                self.body
                    .children
                    .iter_mut()
                    .find(|node| node.id() == self.ids.editor)
                    .unwrap(),
                editor,
            );
            previous
                .component_mut::<AgentText>()
                .expect("composer editor")
                .set_focus(false, false);
            if let Some(old) = &self.destination {
                self.retained.insert(old.storage_key(), previous);
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
            return Err(error);
        }
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
        let toolbar = self.toolbar();
        let mut old = std::mem::take(&mut toolbar.children);
        for (id, label, visible) in entries.into_iter().chain([
            (ids.cancel, cancel_label, true),
            (ids.send, send_label, true),
        ]) {
            if !visible {
                continue;
            }
            let mut button = if let Some(index) = old.iter().position(|child| child.id() == id) {
                old.remove(index)
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
            toolbar.children.push(button);
        }
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
            if action.target == self.ids.cancel && self.can_cancel {
                if let ComposerDestination::Run { scope } = destination {
                    self.pending.push(ComposerIntent::Cancel { scope });
                }
            } else if !self.is_composing() {
                if action.target == self.ids.send
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
        let measured = self.body.measure(text, available);
        // Like ClientKit's resident controls, the outer Composer uses the
        // assigned conversation column; intrinsic toolbar width is not its frame.
        MeasuredChild::new(Size::new(available.width, measured.size.height))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if self.presented {
            self.body.arrange(text, frame, clip);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
