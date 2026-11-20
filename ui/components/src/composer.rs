//! IME-safe multi-destination Composer surface with context chips (attachments & quoted
//! `ArtifactSelectionReference` spans), Model/Preset selectors, and responsive `Compact`/`Expanded` bar.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerArtifact, ComposerDestination, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonVariant, DraftRevision, ExternalSelection, ExternalUndo,
    composition::{
        Button, Component, ComponentId, Content, ContentActionKind, ContentActionPayload,
        ContentActionRecord, ContentHost, SelectContent, StackContent, TextContent,
        TextFieldContent, TextSubmitPolicy, TextViewport,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::Spacing;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChipTarget {
    Artifact(String),
    Annotation(String),
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
        }
    }

    fn make_editor() -> Content {
        let mut field = TextFieldContent::new("输入消息", "");
        field.show_metadata = false;
        field.viewport = TextViewport::Grow {
            min: 44.0,
            max: 180.0,
        };
        field.submit_policy = TextSubmitPolicy::CommandEnter;
        field.set_placeholder(Some(
            "输入指令… (Cmd+Enter 发送，Enter 换行；支持中文 IME 组合)".to_owned(),
        ));
        Content::new(COMPOSER_EDITOR_ID, field)
    }

    pub fn build_content(models: Vec<String>, presets: Vec<String>) -> Content {
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

        let attach_btn = Button::new("附带");
        let mut reconnect_btn = Button::new("重连");
        reconnect_btn.variant = ButtonVariant::Quiet;
        let mut cancel_btn = Button::new("停止");
        cancel_btn.variant = ButtonVariant::Quiet;
        let mut send_btn = Button::new("发送");
        send_btn.variant = ButtonVariant::Primary;

        let primary_row = Content::new(
            COMPOSER_PRIMARY_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(COMPOSER_MODEL_SELECT_ID, SelectContent::new("模型", models)),
                    Content::new(
                        COMPOSER_PRESET_SELECT_ID,
                        SelectContent::new("预设", presets),
                    ),
                    Content::new(COMPOSER_ATTACH_BTN_ID, attach_btn),
                ],
            },
        );

        let secondary_row = Content::new(
            COMPOSER_SECONDARY_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(COMPOSER_RECONNECT_BTN_ID, reconnect_btn),
                    Content::new(COMPOSER_CANCEL_BTN_ID, cancel_btn),
                    Content::new(COMPOSER_SEND_BTN_ID, send_btn),
                ],
            },
        );

        let controls = Content::new(
            COMPOSER_CONTROLS_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![primary_row, secondary_row],
            },
        );

        Content::new(
            COMPOSER_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![chips_row, Self::make_editor(), controls],
            },
        )
    }

    pub fn composing(host: &ContentHost) -> bool {
        host.get(COMPOSER_EDITOR_ID)
            .and_then(|c| c.component::<TextFieldContent>())
            .is_some_and(TextFieldContent::composing)
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        selected_model_idx: usize,
        selected_preset_idx: usize,
        width_mode: ResponsiveWidthMode,
    ) {
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
            let _ = host.insert(COMPOSER_ROOT_ID, next_editor);
            let _ = host.reorder(
                COMPOSER_ROOT_ID,
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

        if let Some(model_sel) = host
            .get_mut(COMPOSER_MODEL_SELECT_ID)
            .and_then(|c| c.component_mut::<SelectContent>())
        {
            model_sel.set_selected(selected_model_idx);
        }
        if let Some(preset_sel) = host
            .get_mut(COMPOSER_PRESET_SELECT_ID)
            .and_then(|c| c.component_mut::<SelectContent>())
        {
            preset_sel.set_selected(selected_preset_idx);
        }

        let composing_now = Self::composing(host);
        let can_cancel = snapshot.run.as_ref().is_some_and(|r| r.can_cancel);
        if let Some(send) = host
            .get_mut(COMPOSER_SEND_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            send.button.state.disabled = !snapshot.can_submit || composing_now || can_cancel;
        }
        if let Some(cancel) = host
            .get_mut(COMPOSER_CANCEL_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            cancel.button.state.disabled = !can_cancel;
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
                    format!("文件: {} ×", p.rsplit('/').next().unwrap_or(p))
                }
                ComposerArtifact::Attachment { name, .. } => format!("附件: {name} ×"),
                ComposerArtifact::Command { name, .. } => format!("/{name} ×"),
                ComposerArtifact::Skill { name, .. } => format!("技能: {name} ×"),
                ComposerArtifact::Reference { id, .. } => format!("引用: {id} ×"),
            };
            let id = ComponentId(next_id);
            next_id += 1;
            self.chip_targets
                .insert(id, ChipTarget::Artifact(art.id().to_owned()));
            let mut btn = Button::new(label);
            btn.variant = ButtonVariant::Quiet;
            children.push(Content::new(id, btn));
        }

        for ann in &snapshot.draft.annotations {
            let tag = if let Some(ref_val) = &ann.artifact_reference {
                let name = ref_val
                    .artifact_id
                    .rsplit('/')
                    .next()
                    .unwrap_or(&ref_val.artifact_id);
                format!("摘录: {name}@{} ×", ref_val.revision)
            } else {
                format!("引用: {} ×", ann.source)
            };
            let id = ComponentId(next_id);
            next_id += 1;
            self.chip_targets
                .insert(id, ChipTarget::Annotation(ann.id.clone()));
            let mut btn = Button::new(tag);
            btn.variant = ButtonVariant::Quiet;
            children.push(Content::new(id, btn));
        }

        if children.is_empty() {
            children.push(Content::new(
                ComponentId(next_id),
                TextContent::new("上下文：可直接引用工作区文件或代码选区"),
            ));
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
                    && matches!(record.action.kind, ContentActionKind::Activate)
                    && !composing =>
                {
                    if let Some(dest) = &self.selected {
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
                id if id == COMPOSER_RECONNECT_BTN_ID
                    && matches!(record.action.kind, ContentActionKind::Activate) =>
                {
                    out.push(ComposerIntent::Reconnect);
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
