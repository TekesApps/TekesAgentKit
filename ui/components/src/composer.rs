//! Unified Tekes-aligned Composer surface: optional queued-prompt strip above a single
//! rounded `PanelContent` card containing context chips, frameless `TextSurfacePresentation::Content`
//! IME-safe editor, `+` attach, permission & model `MenuContent` pickers, and Send/Stop button.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{card_panel, secondary_text};
use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerArtifact, ComposerDestination, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonVariant, DraftRevision, ExternalSelection, ExternalUndo, TextSurfacePresentation,
    composition::{
        Button, Component, ComponentId, Content, ContentActionKind, ContentActionPayload,
        ContentActionRecord, ContentHost, MenuContent, MenuEntry, StackContent, TextFieldContent,
        TextSubmitPolicy, TextViewport,
    },
    extended::{Axis, Stack},
    layout::Insets,
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerIntent {
    EditText {
        destination: ComposerDestination,
        text: String,
    },
    Send {
        destination: ComposerDestination,
    },
    Cancel {
        scope: RunScope,
    },
    Reconnect,
    AttachDemoFile {
        destination: ComposerDestination,
    },
    RemoveArtifact {
        destination: ComposerDestination,
        artifact_id: String,
    },
    RemoveAnnotation {
        destination: ComposerDestination,
        annotation_id: String,
    },
    SelectModel(usize),
    SelectPreset(usize),
    RemoveQueued {
        item_id: String,
        session_id: String,
        epoch: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChipTarget {
    Artifact(String),
    Annotation(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueuedStripItem {
    id: String,
    session_id: String,
    epoch: u64,
    text: String,
}

#[derive(Debug)]
pub struct ComposerSurface {
    selected: Option<ComposerDestination>,
    editors: BTreeMap<String, Content>,
    committed_text: String,
    synced_rev: u64,
    chips_sig: String,
    chip_targets: BTreeMap<ComponentId, ChipTarget>,
    last_mode: ResponsiveWidthMode,
    models: Vec<String>,
    presets: Vec<String>,
    last_model_idx: usize,
    last_preset_idx: usize,
    active_queued: Option<QueuedStripItem>,
    last_queued_sig: String,
}

impl Default for ComposerSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposerSurface {
    pub fn new() -> Self {
        Self {
            selected: None,
            editors: BTreeMap::new(),
            committed_text: String::new(),
            synced_rev: 0,
            chips_sig: String::new(),
            chip_targets: BTreeMap::new(),
            last_mode: ResponsiveWidthMode::Expanded,
            models: Vec::new(),
            presets: Vec::new(),
            last_model_idx: usize::MAX,
            last_preset_idx: usize::MAX,
            active_queued: None,
            last_queued_sig: String::new(),
        }
    }

    fn make_editor() -> Content {
        let mut field = TextFieldContent::new("输入消息", "");
        field
            .surface
            .set_presentation(TextSurfacePresentation::Content);
        field.show_metadata = false;
        field.viewport = TextViewport::Grow {
            min: 40.0,
            max: 140.0,
        };
        field.submit_policy = TextSubmitPolicy::CommandEnter;
        field.set_placeholder(Some("Describe what you want Tekes to do...".to_owned()));
        Content::new(COMPOSER_EDITOR_ID, field)
    }

    fn make_menu(labels: &[String], selected_idx: usize) -> MenuContent {
        let active = selected_idx.min(labels.len().saturating_sub(1));
        let trigger_label = labels
            .get(active)
            .map(|l| format!("{l} ▾"))
            .unwrap_or_else(|| "Select ▾".to_owned());
        let mut menu = MenuContent::new(trigger_label, labels.to_vec());
        let entries: Vec<MenuEntry> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| MenuEntry::Item {
                value: label.clone(),
                label: label.clone(),
                checked: Some(i == active),
                disabled: false,
            })
            .collect();
        if !entries.is_empty() {
            let _ = menu.set_entries(entries);
        }
        menu.trigger.variant = ButtonVariant::Panel;
        menu.trigger.set_transparent_chrome(true);
        menu
    }

    pub fn build_content(models: Vec<String>, presets: Vec<String>) -> Content {
        let queued_slot = Content::new(
            COMPOSER_QUEUED_SLOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: 0.0,
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let chips_row = Content::new(
            COMPOSER_CHIPS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );

        let mut attach_btn = Button::new("+").with_accessible_name("Attach context");
        attach_btn.variant = ButtonVariant::Panel;
        attach_btn.set_transparent_chrome(true);

        let preset_menu = Self::make_menu(&presets, 0);
        let model_menu = Self::make_menu(&models, 0);

        let harness_label = Content::new(COMPOSER_HARNESS_LABEL_ID, secondary_text("Tekes"));

        let primary_row = Content::new(
            COMPOSER_PRIMARY_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(COMPOSER_ATTACH_BTN_ID, attach_btn),
                    Content::new(COMPOSER_PRESET_SELECT_ID, preset_menu),
                    harness_label,
                ],
            },
        );

        let mut send_btn = Button::new("↑").with_accessible_name("Send");
        send_btn.variant = ButtonVariant::Primary;

        let secondary_row = Content::new(
            COMPOSER_SECONDARY_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(COMPOSER_MODEL_SELECT_ID, model_menu),
                    Content::new(COMPOSER_SEND_BTN_ID, send_btn),
                ],
            },
        );

        let controls = StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: None,
            children: vec![primary_row, secondary_row],
        }
        .into_content_with_distribution(
            COMPOSER_CONTROLS_STACK_ID,
            MainAxisDistribution::SpaceBetween,
        );

        let card_body = Content::new(
            COMPOSER_CARD_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![chips_row, Self::make_editor(), controls],
            },
        );
        let card = card_panel(
            COMPOSER_CARD_ID,
            card_body,
            Insets::new(10.0, 14.0, 8.0, 14.0),
        );

        Content::new(
            COMPOSER_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![queued_slot, card],
            },
        )
    }

    pub fn composing(host: &ContentHost) -> bool {
        host.get(COMPOSER_EDITOR_ID)
            .and_then(|c| c.component::<TextFieldContent>())
            .is_some_and(TextFieldContent::composing)
    }

    fn sync_menu_state(
        host: &mut ContentHost,
        id: ComponentId,
        labels: &[String],
        selected_idx: usize,
    ) {
        let Some(menu) = host
            .get_mut(id)
            .and_then(|c| c.component_mut::<MenuContent>())
        else {
            return;
        };
        let active = selected_idx.min(labels.len().saturating_sub(1));
        if let Some(label) = labels.get(active) {
            let desired = format!("{label} ▾");
            if menu.trigger.label != desired {
                menu.trigger.set_contents(desired);
            }
        }
        let entries: Vec<MenuEntry> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| MenuEntry::Item {
                value: label.clone(),
                label: label.clone(),
                checked: Some(i == active),
                disabled: false,
            })
            .collect();
        if !entries.is_empty() {
            let _ = menu.set_entries(entries);
        }
    }

    fn sync_queued_strip(&mut self, host: &mut ContentHost, snapshot: &RunSnapshot) {
        let queued = snapshot.run.as_ref().and_then(|run| {
            run.queue.first().map(|raw| {
                let text = raw
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| raw.get("label").and_then(|v| v.as_str()))
                    .unwrap_or("Queued message")
                    .to_owned();
                let id = raw
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("queue-0")
                    .to_owned();
                QueuedStripItem {
                    id,
                    session_id: run.scope.session_id.clone(),
                    epoch: run.connection_epoch,
                    text,
                }
            })
        });

        let sig = queued
            .as_ref()
            .map(|q| format!("{}:{}:{}", q.id, q.epoch, q.text))
            .unwrap_or_default();
        if sig == self.last_queued_sig {
            return;
        }
        self.last_queued_sig = sig;
        self.active_queued = queued.clone();

        if let Some(item) = queued {
            let preview = if item.text.chars().count() > 54 {
                let head: String = item.text.chars().take(54).collect();
                format!("{head}…")
            } else {
                item.text
            };
            let mut steer_btn = Button::new("Steer");
            steer_btn.variant = ButtonVariant::Panel;
            steer_btn.set_transparent_chrome(true);
            let mut remove_btn = Button::new("×").with_accessible_name("Remove queued prompt");
            remove_btn.variant = ButtonVariant::Panel;
            remove_btn.set_transparent_chrome(true);

            let actions = Content::new(
                ComponentId(421),
                StackContent {
                    model: Stack {
                        axis: Axis::Horizontal,
                        gap: Spacing::Micro.value(),
                    },
                    flexible_child: None,
                    children: vec![
                        Content::new(COMPOSER_QUEUED_STEER_BTN_ID, steer_btn),
                        Content::new(COMPOSER_QUEUED_REMOVE_BTN_ID, remove_btn),
                    ],
                },
            );
            let row = StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(COMPOSER_QUEUED_TEXT_ID),
                children: vec![
                    Content::new(COMPOSER_QUEUED_TEXT_ID, secondary_text(preview)),
                    actions,
                ],
            }
            .into_content_with_distribution(
                COMPOSER_QUEUED_ROW_ID,
                MainAxisDistribution::SpaceBetween,
            );
            let panel = card_panel(
                COMPOSER_QUEUED_PANEL_ID,
                row,
                Insets::new(6.0, 12.0, 6.0, 12.0),
            );
            replace_stack_children(host, COMPOSER_QUEUED_SLOT_ID, vec![panel]);
        } else {
            replace_stack_children(host, COMPOSER_QUEUED_SLOT_ID, Vec::new());
        }
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        selected_model_idx: usize,
        selected_preset_idx: usize,
        width_mode: ResponsiveWidthMode,
    ) {
        if self.models.is_empty()
            && let Some(m) = host
                .get(COMPOSER_MODEL_SELECT_ID)
                .and_then(|c| c.component::<MenuContent>())
        {
            self.models = m.labels.clone();
        }
        if self.presets.is_empty()
            && let Some(p) = host
                .get(COMPOSER_PRESET_SELECT_ID)
                .and_then(|c| c.component::<MenuContent>())
        {
            self.presets = p.labels.clone();
        }

        if self.last_mode != width_mode {
            if let Some(controls) = host
                .get_mut(COMPOSER_CONTROLS_STACK_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                controls.model.axis = match width_mode {
                    ResponsiveWidthMode::Compact => Axis::Vertical,
                    ResponsiveWidthMode::Expanded => Axis::Horizontal,
                };
            }
            self.last_mode = width_mode;
        }

        self.sync_queued_strip(host, snapshot);

        let destination = snapshot.selection.as_ref().map(|s| s.destination.clone());
        let is_composing = Self::composing(host);
        if self.selected != destination && !is_composing {
            host.focus(None, false);
            if let Ok(old_editor) = host.remove(COMPOSER_EDITOR_ID)
                && let Some(old_dest) = &self.selected
            {
                self.editors.insert(old_dest.storage_key(), old_editor);
            }
            let next_key = destination
                .as_ref()
                .map(ComposerDestination::storage_key)
                .unwrap_or_default();
            let next_editor = self
                .editors
                .remove(&next_key)
                .unwrap_or_else(Self::make_editor);
            let _ = host.insert(COMPOSER_CARD_STACK_ID, next_editor);
            let _ = host.reorder(
                COMPOSER_CARD_STACK_ID,
                &[
                    COMPOSER_CHIPS_ROW_ID,
                    COMPOSER_EDITOR_ID,
                    COMPOSER_CONTROLS_STACK_ID,
                ],
            );
            self.selected = destination.clone();
            self.committed_text = snapshot.draft.text.clone();
        }

        if let Some(field) = host
            .get_mut(COMPOSER_EDITOR_ID)
            .and_then(|c| c.component_mut::<TextFieldContent>())
            && !field.composing()
            && field.text() != snapshot.draft.text
        {
            let baseline = field.surface.input().external_baseline();
            self.synced_rev = self.synced_rev.saturating_add(1).max(1);
            let _ = field.surface.apply_external_value(
                &baseline,
                &DraftRevision {
                    identity: destination
                        .as_ref()
                        .map(ComposerDestination::storage_key)
                        .unwrap_or_else(|| "none".to_owned()),
                    version: self.synced_rev,
                    payload: snapshot.draft.text.clone(),
                },
                ExternalSelection::Preserve,
                ExternalUndo::Transaction,
            );
            self.committed_text = snapshot.draft.text.clone();
        }

        self.sync_chips(host, snapshot);

        if self.last_model_idx != selected_model_idx {
            self.last_model_idx = selected_model_idx;
            let models = self.models.clone();
            Self::sync_menu_state(host, COMPOSER_MODEL_SELECT_ID, &models, selected_model_idx);
        }
        if self.last_preset_idx != selected_preset_idx {
            self.last_preset_idx = selected_preset_idx;
            let presets = self.presets.clone();
            Self::sync_menu_state(
                host,
                COMPOSER_PRESET_SELECT_ID,
                &presets,
                selected_preset_idx,
            );
        }

        let composing_now = Self::composing(host);
        let can_cancel = snapshot.run.as_ref().is_some_and(|r| r.can_cancel);
        if let Some(send) = host
            .get_mut(COMPOSER_SEND_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            if can_cancel {
                if send.label != "■" {
                    send.set_contents("■");
                    send.set_accessible_name(Some("Stop".to_owned()));
                }
                send.button.state.disabled = false;
            } else {
                if send.label != "↑" {
                    send.set_contents("↑");
                    send.set_accessible_name(Some("Send".to_owned()));
                }
                send.button.state.disabled = !snapshot.can_submit || composing_now;
            }
        }
    }

    fn sync_chips(&mut self, host: &mut ContentHost, snapshot: &RunSnapshot) {
        let mut sig = String::new();
        for art in &snapshot.draft.artifacts {
            sig.push_str("a:");
            sig.push_str(art.id());
            sig.push(';');
        }
        for ann in &snapshot.draft.annotations {
            sig.push_str("q:");
            sig.push_str(&ann.id);
            sig.push(';');
        }
        if sig == self.chips_sig {
            return;
        }
        self.chips_sig = sig;
        self.chip_targets.clear();

        let mut next_id = COMPOSER_CHIP_BASE;
        let mut children = Vec::new();
        for art in &snapshot.draft.artifacts {
            let label = match art {
                ComposerArtifact::Reference { path: Some(p), .. } => {
                    format!("{} ×", p.rsplit('/').next().unwrap_or(p))
                }
                ComposerArtifact::Attachment { name, .. } => format!("{name} ×"),
                ComposerArtifact::Command { name, .. } => format!("/{name} ×"),
                ComposerArtifact::Skill { name, .. } => format!("{name} ×"),
                ComposerArtifact::Reference { id, .. } => format!("{id} ×"),
            };
            let id = ComponentId(next_id);
            next_id += 1;
            self.chip_targets
                .insert(id, ChipTarget::Artifact(art.id().to_owned()));
            let mut btn = Button::new(label);
            btn.variant = ButtonVariant::Panel;
            children.push(Content::new(id, btn));
        }

        for ann in &snapshot.draft.annotations {
            let tag = if let Some(ref_val) = &ann.artifact_reference {
                let name = ref_val
                    .artifact_id
                    .rsplit('/')
                    .next()
                    .unwrap_or(&ref_val.artifact_id);
                format!("{name}@{} ×", ref_val.revision)
            } else {
                format!("{} ×", ann.source)
            };
            let id = ComponentId(next_id);
            next_id += 1;
            self.chip_targets
                .insert(id, ChipTarget::Annotation(ann.id.clone()));
            let mut btn = Button::new(tag);
            btn.variant = ButtonVariant::Panel;
            children.push(Content::new(id, btn));
        }

        replace_stack_children(host, COMPOSER_CHIPS_ROW_ID, children);
    }

    pub fn handle_actions(
        &mut self,
        host: &ContentHost,
        snapshot: &RunSnapshot,
        records: &[ContentActionRecord],
    ) -> Vec<ComposerIntent> {
        let mut out = Vec::new();
        let composing = Self::composing(host);
        if !composing
            && let Some(field) = host
                .get(COMPOSER_EDITOR_ID)
                .and_then(|c| c.component::<TextFieldContent>())
        {
            let text = field.text();
            if text != self.committed_text {
                self.committed_text = text.to_owned();
                if let Some(dest) = &self.selected {
                    out.push(ComposerIntent::EditText {
                        destination: dest.clone(),
                        text: self.committed_text.clone(),
                    });
                }
            }
        }

        let can_cancel = snapshot.run.as_ref().is_some_and(|r| r.can_cancel);

        for record in records {
            if let Some(chip) = self.chip_targets.get(&record.action.target)
                && matches!(record.action.kind, ContentActionKind::Activate)
                && let Some(dest) = &self.selected
            {
                match chip {
                    ChipTarget::Artifact(id) => out.push(ComposerIntent::RemoveArtifact {
                        destination: dest.clone(),
                        artifact_id: id.clone(),
                    }),
                    ChipTarget::Annotation(id) => out.push(ComposerIntent::RemoveAnnotation {
                        destination: dest.clone(),
                        annotation_id: id.clone(),
                    }),
                }
                continue;
            }
            if let Some(ContentActionPayload::MenuSelection(sel)) = &record.payload {
                match record.action.target {
                    id if id == COMPOSER_MODEL_SELECT_ID => {
                        out.push(ComposerIntent::SelectModel(sel.index()));
                    }
                    id if id == COMPOSER_PRESET_SELECT_ID => {
                        out.push(ComposerIntent::SelectPreset(sel.index()));
                    }
                    _ => {}
                }
                continue;
            }
            match record.action.target {
                id if id == COMPOSER_EDITOR_ID
                    && matches!(record.action.kind, ContentActionKind::Submit)
                    && !composing =>
                {
                    if let Some(dest) = &self.selected {
                        out.push(ComposerIntent::Send {
                            destination: dest.clone(),
                        });
                    }
                }
                id if id == COMPOSER_SEND_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if can_cancel {
                        if let Some(ComposerDestination::Run { scope }) =
                            snapshot.selection.as_ref().map(|s| &s.destination)
                        {
                            out.push(ComposerIntent::Cancel {
                                scope: scope.clone(),
                            });
                        }
                    } else if !composing && let Some(dest) = &self.selected {
                        out.push(ComposerIntent::Send {
                            destination: dest.clone(),
                        });
                    }
                }
                id if id == COMPOSER_CANCEL_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if let Some(ComposerDestination::Run { scope }) =
                        snapshot.selection.as_ref().map(|s| &s.destination)
                    {
                        out.push(ComposerIntent::Cancel {
                            scope: scope.clone(),
                        });
                    }
                }
                id if id == COMPOSER_QUEUED_REMOVE_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if let Some(item) = &self.active_queued {
                        out.push(ComposerIntent::RemoveQueued {
                            item_id: item.id.clone(),
                            session_id: item.session_id.clone(),
                            epoch: item.epoch,
                        });
                    }
                }
                id if id == COMPOSER_ATTACH_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    if let Some(dest) = &self.selected {
                        out.push(ComposerIntent::AttachDemoFile {
                            destination: dest.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        out
    }
}
