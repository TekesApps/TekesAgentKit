//! Agent composer: canonical drafts arrive from presentation; editing belongs to TekesUI.
use crate::AgentText;
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
    can_send: bool,
    can_cancel: bool,
    affordances: ComposerAffordances,
    labels: ComposerLabels,
    pending: Vec<ComposerIntent>,
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
            can_send: false,
            can_cancel: false,
            affordances: ComposerAffordances::default(),
            labels: ComposerLabels::default(),
            pending: Vec::new(),
        }
    }
    pub fn ids(&self) -> ComposerIds {
        self.ids
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
    pub fn is_composing(&self) -> bool {
        self.editor().input.marked.is_some()
    }
    fn text(&self) -> &AgentText {
        self.body.children[0]
            .component::<AgentText>()
            .expect("composer editor")
    }
    fn text_mut(&mut self) -> &mut AgentText {
        self.body.children[0]
            .component_mut::<AgentText>()
            .expect("composer editor")
    }
    fn toolbar(&mut self) -> &mut StackContent {
        self.body.children[1]
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
            let mut previous = std::mem::replace(&mut self.body.children[0], editor);
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
        self.revision = snapshot.revision;
        self.draft = snapshot.draft.clone();
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
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
