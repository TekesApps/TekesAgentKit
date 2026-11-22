//! Unified Tekes-aligned Composer & top Run setup toolbar:
//! - `RunSetupToolbar`: top configuration strip (`Harness`, `Approval`, `Model`, `Effort`)
//!   with hairline separator, automatically wrapping to 2 compact rows when `< 520px`.
//! - `ComposerSurface`: single-row pure-input capsule (`+` attach, frameless IME-safe editor,
//!   `↑`/`■` Send/Stop) with optional context chips & queued-prompt strip.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{card_panel, secondary_text};
use std::collections::BTreeMap;
use tekes_agent_presentation::{ComposerArtifact, ComposerDestination, RunScope, RunSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer, ButtonVariant, DraftRevision, ExternalSelection, ExternalUndo,
    TextSurfacePresentation,
    composition::{
        Button, Component, ComponentId, Content, ContentActionKind, ContentActionPayload,
        ContentActionRecord, ContentHost, ContentRole, ContentSemantics, ContentTheme,
        CurveSubtreeCapability, MenuContent, MenuEntry, SeparatorContent, StackContent,
        TextFieldContent, TextSubmitPolicy, TextViewport,
    },
    extended::Axis,
    layout::{AlignmentOverride, Insets, MeasuredChild, layout_measured_children},
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{
    Spacing, proposal,
    proposal::layout::{CrossAxisAlignment, MainAxisDistribution},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunSetupIntent {
    SelectHarness(usize),
    SelectPreset(usize),
    SelectModel(usize),
    SelectEffort(usize),
}

/// Top configuration strip (`Harness`, `Approval`, `Model`, `Effort`) mounted at the top of
/// `AgentRunContainer` above `TranscriptSurface` and `ComposerSurface`.
#[derive(Debug)]
pub struct RunSetupToolbar {
    harnesses: Vec<String>,
    presets: Vec<String>,
    models: Vec<String>,
    efforts: Vec<String>,
    last_mode: ResponsiveWidthMode,
    last_harness: Option<(usize, bool)>,
    last_preset_idx: Option<usize>,
    last_model_idx: Option<usize>,
    last_effort_idx: Option<usize>,
}

impl Default for RunSetupToolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl RunSetupToolbar {
    pub fn new() -> Self {
        Self {
            harnesses: Vec::new(),
            presets: Vec::new(),
            models: Vec::new(),
            efforts: Vec::new(),
            last_mode: ResponsiveWidthMode::Expanded,
            last_harness: None,
            last_preset_idx: None,
            last_model_idx: None,
            last_effort_idx: None,
        }
    }

    fn make_setup_menu(
        labels: &[String],
        selected_idx: usize,
        prefix: Option<&str>,
        locked: bool,
    ) -> MenuContent {
        let active = selected_idx.min(labels.len().saturating_sub(1));
        let prefix = prefix.unwrap_or("");
        let trigger_label = labels
            .get(active)
            .map(|l| {
                if locked {
                    format!("{prefix}{l}")
                } else {
                    format!("{prefix}{l} ▾")
                }
            })
            .unwrap_or_else(|| "Select ▾".to_owned());
        let mut menu = MenuContent::new(trigger_label, labels.to_vec());
        let entries: Vec<MenuEntry> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| MenuEntry::Item {
                value: label.clone(),
                label: label.clone(),
                checked: Some(i == active),
                disabled: locked,
            })
            .collect();
        if !entries.is_empty() {
            let _ = menu.set_entries(entries);
        }
        menu.set_trigger_variant(ButtonVariant::Panel);
        menu.set_trigger_transparent_chrome(true);
        menu
    }

    fn sync_menu_state(
        host: &mut ContentHost,
        id: ComponentId,
        labels: &[String],
        selected_idx: usize,
        prefix: Option<&str>,
        locked: bool,
    ) {
        let Some(menu) = host
            .get_mut(id)
            .and_then(|c| c.component_mut::<MenuContent>())
        else {
            return;
        };
        let active = selected_idx.min(labels.len().saturating_sub(1));
        let prefix = prefix.unwrap_or("");
        if let Some(label) = labels.get(active) {
            let desired = if locked {
                format!("{prefix}{label}")
            } else {
                format!("{prefix}{label} ▾")
            };
            if menu.trigger().label() != desired {
                menu.set_trigger_contents(desired);
            }
        }
        let entries: Vec<MenuEntry> = labels
            .iter()
            .enumerate()
            .map(|(i, label)| MenuEntry::Item {
                value: label.clone(),
                label: label.clone(),
                checked: Some(i == active),
                disabled: locked,
            })
            .collect();
        if !entries.is_empty() {
            let _ = menu.set_entries(entries);
        }
    }

    fn cache_labels(host: &ContentHost, id: ComponentId, cache: &mut Vec<String>) {
        if cache.is_empty()
            && let Some(m) = host.get(id).and_then(|c| c.component::<MenuContent>())
        {
            *cache = m.labels().to_vec();
        }
    }

    pub fn build_content(
        harnesses: Vec<String>,
        presets: Vec<String>,
        models: Vec<String>,
        efforts: Vec<String>,
    ) -> Content {
        let harness_menu = Self::make_setup_menu(&harnesses, 0, None, false);
        let preset_menu = Self::make_setup_menu(&presets, 0, None, false);
        let model_menu = Self::make_setup_menu(&models, 0, None, false);
        let effort_menu = Self::make_setup_menu(&efforts, 1, Some("Effort: "), false);

        let primary_row = Content::new(
            COMPOSER_PRIMARY_ROW_ID,
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    Content::new(COMPOSER_HARNESS_SELECT_ID, harness_menu),
                    Content::new(COMPOSER_PRESET_SELECT_ID, preset_menu),
                ],
            ),
        );

        let secondary_row = Content::new(
            COMPOSER_SECONDARY_ROW_ID,
            StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    Content::new(COMPOSER_MODEL_SELECT_ID, model_menu),
                    Content::new(COMPOSER_EFFORT_SELECT_ID, effort_menu),
                ],
            ),
        );

        let controls =
            StackContent::horizontal(Spacing::Tight.value(), vec![primary_row, secondary_row])
                .into_content_with_distribution(
                    COMPOSER_CONTROLS_STACK_ID,
                    MainAxisDistribution::SpaceBetween,
                );

        let sep = Content::new(
            RUN_SETUP_TOOLBAR_SEP_ID,
            SeparatorContent::new(Axis::Horizontal),
        );

        Content::new(
            RUN_SETUP_TOOLBAR_ROOT_ID,
            StackContent::vertical(Spacing::Tight.value(), vec![controls, sep]),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        selected_harness_idx: usize,
        selected_preset_idx: usize,
        selected_model_idx: usize,
        selected_effort_idx: usize,
        width_mode: ResponsiveWidthMode,
    ) {
        Self::cache_labels(host, COMPOSER_HARNESS_SELECT_ID, &mut self.harnesses);
        Self::cache_labels(host, COMPOSER_PRESET_SELECT_ID, &mut self.presets);
        Self::cache_labels(host, COMPOSER_MODEL_SELECT_ID, &mut self.models);
        Self::cache_labels(host, COMPOSER_EFFORT_SELECT_ID, &mut self.efforts);

        if self.last_mode != width_mode {
            if let Some(controls) = host
                .get_mut(COMPOSER_CONTROLS_STACK_ID)
                .and_then(|c| c.component_mut::<StackContent>())
            {
                match width_mode {
                    ResponsiveWidthMode::Compact => {
                        controls.set_axis(Axis::Vertical);
                        controls.set_gap(2.0);
                    }
                    ResponsiveWidthMode::Expanded => {
                        controls.set_axis(Axis::Horizontal);
                        controls.set_gap(Spacing::Tight.value());
                    }
                }
            }
            self.last_mode = width_mode;
        }

        let harness_locked = snapshot.run.is_some() && !snapshot.transcript.items.is_empty();
        if self.last_harness != Some((selected_harness_idx, harness_locked)) {
            self.last_harness = Some((selected_harness_idx, harness_locked));
            let harnesses = self.harnesses.clone();
            Self::sync_menu_state(
                host,
                COMPOSER_HARNESS_SELECT_ID,
                &harnesses,
                selected_harness_idx,
                None,
                harness_locked,
            );
        }

        if self.last_preset_idx != Some(selected_preset_idx) {
            self.last_preset_idx = Some(selected_preset_idx);
            let presets = self.presets.clone();
            Self::sync_menu_state(
                host,
                COMPOSER_PRESET_SELECT_ID,
                &presets,
                selected_preset_idx,
                None,
                false,
            );
        }

        if self.last_model_idx != Some(selected_model_idx) {
            self.last_model_idx = Some(selected_model_idx);
            let models = self.models.clone();
            Self::sync_menu_state(
                host,
                COMPOSER_MODEL_SELECT_ID,
                &models,
                selected_model_idx,
                None,
                false,
            );
        }

        if self.last_effort_idx != Some(selected_effort_idx) {
            self.last_effort_idx = Some(selected_effort_idx);
            let efforts = self.efforts.clone();
            Self::sync_menu_state(
                host,
                COMPOSER_EFFORT_SELECT_ID,
                &efforts,
                selected_effort_idx,
                Some("Effort: "),
                false,
            );
        }
    }

    pub fn handle_actions(
        &mut self,
        snapshot: &RunSnapshot,
        records: &[ContentActionRecord],
    ) -> Vec<RunSetupIntent> {
        let mut out = Vec::new();
        let harness_locked = snapshot.run.is_some() && !snapshot.transcript.items.is_empty();
        for record in records {
            if let Some(ContentActionPayload::MenuSelection(sel)) = &record.payload {
                match record.action.target {
                    id if id == COMPOSER_HARNESS_SELECT_ID && !harness_locked => {
                        out.push(RunSetupIntent::SelectHarness(sel.index()));
                    }
                    id if id == COMPOSER_PRESET_SELECT_ID => {
                        out.push(RunSetupIntent::SelectPreset(sel.index()));
                    }
                    id if id == COMPOSER_MODEL_SELECT_ID => {
                        out.push(RunSetupIntent::SelectModel(sel.index()));
                    }
                    id if id == COMPOSER_EFFORT_SELECT_ID => {
                        out.push(RunSetupIntent::SelectEffort(sel.index()));
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

/// Horizontal composer row that centers `+` and `↑`/`■` for single-line input and
/// bottom-aligns them with the last visible input line once text wraps/grows.
#[derive(Debug)]
struct ComposerInputRow {
    gap: f32,
    flexible_child: ComponentId,
    children: Vec<Content>,
    arranged: bool,
}

impl ComposerInputRow {
    fn new(gap: f32, flexible_child: ComponentId, children: Vec<Content>) -> Self {
        Self {
            gap,
            flexible_child,
            children,
            arranged: false,
        }
    }

    fn finite(v: f32) -> f32 {
        if v.is_finite() { v } else { 0.0 }
    }

    fn measurements(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        available: Size,
        intrinsic: bool,
    ) -> Vec<MeasuredChild> {
        let mut measured: Vec<_> = self
            .children
            .iter_mut()
            .map(|c| {
                if intrinsic {
                    c.measure_intrinsic(text, available)
                } else {
                    c.measure(text, available)
                }
            })
            .collect();
        if let Some(i) = self
            .children
            .iter()
            .position(|c| c.id() == self.flexible_child)
        {
            let fixed: f32 = measured
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != i)
                .map(|(_, m)| Self::finite(m.size.width).max(0.0))
                .sum();
            let gap_total =
                Self::finite(self.gap).max(0.0) * self.children.len().saturating_sub(1) as f32;
            let left = (available.width - fixed - gap_total).max(0.0);
            let constrained = Size::new(left, available.height);
            measured[i] = if intrinsic {
                self.children[i].measure_intrinsic(text, constrained)
            } else {
                self.children[i].measure(text, constrained)
            };
        }
        measured
    }

    fn measure_content(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        available: Size,
        intrinsic: bool,
    ) -> MeasuredChild {
        let measured = self.measurements(text, available, intrinsic);
        let mut main = 0.0_f32;
        let mut cross = 0.0_f32;
        for m in measured {
            main += Self::finite(m.size.width).clamp(0.0, 1e8);
            cross = cross.max(Self::finite(m.size.height).max(0.0));
        }
        main += Self::finite(self.gap).max(0.0) * self.children.len().saturating_sub(1) as f32;
        MeasuredChild::new(Size::new(
            if available.width.is_finite() {
                available.width.max(0.0)
            } else {
                main
            },
            cross,
        ))
    }
}

impl Component for ComposerInputRow {
    fn host_reveal_passthrough(&self) -> bool {
        self.arranged
    }

    fn curve_subtree_capability(&self) -> CurveSubtreeCapability {
        CurveSubtreeCapability::Recursive
    }

    fn has_stable_hooks(&self) -> bool {
        true
    }

    fn supports_prepared_lifecycle(&self) -> bool {
        true
    }

    fn ignores_pointer_actions(&self) -> bool {
        true
    }

    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, "")
    }

    fn children(&self) -> &[Content] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }

    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.measure_content(text, available, false)
    }

    fn measure_intrinsic(
        &mut self,
        text: &mut dyn ButtonTextRenderer,
        available: Size,
    ) -> MeasuredChild {
        self.measure_content(text, available, true)
    }

    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.arranged = frame.size.width.is_finite()
            && frame.size.height.is_finite()
            && frame.size.width > 0.0
            && frame.size.height > 0.0;
        let measured = self.measurements(text, frame.size, false);
        let flex_idx = self
            .children
            .iter()
            .position(|c| c.id() == self.flexible_child);
        let max_fixed_h = measured
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != flex_idx)
            .map(|(_, m)| m.size.height)
            .fold(0.0_f32, f32::max);
        let editor_h = flex_idx.map_or(0.0, |i| measured[i].size.height);
        let cross_axis = if editor_h > max_fixed_h + 4.0 {
            CrossAxisAlignment::Trailing
        } else {
            CrossAxisAlignment::Center
        };
        let placements = layout_measured_children(
            frame,
            Axis::Horizontal,
            &measured,
            self.gap,
            proposal::layout::ROW,
            AlignmentOverride {
                cross_axis: Some(cross_axis),
                ..AlignmentOverride::default()
            },
        );
        for (child, placed) in self.children.iter_mut().zip(placements) {
            child.arrange(text, placed.frame, clip);
        }
    }

    fn paint(
        &mut self,
        _: &mut Scene,
        _: &mut dyn ButtonTextRenderer,
        _: Rect,
        _: Rect,
        _: ContentTheme,
    ) {
    }
}

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
            active_queued: None,
            last_queued_sig: String::new(),
        }
    }

    fn make_editor() -> Content {
        let mut field = TextFieldContent::new("输入消息", "");
        field.set_surface_presentation(TextSurfacePresentation::Content);
        field.show_metadata = false;
        field.viewport = TextViewport::Grow {
            min: 28.0,
            max: 140.0,
        };
        field.submit_policy = TextSubmitPolicy::CommandEnter;
        field.set_placeholder(Some("Describe what you want Tekes to do...".to_owned()));
        Content::new(COMPOSER_EDITOR_ID, field)
    }

    pub fn build_content() -> Content {
        let queued_slot = Content::new(
            COMPOSER_QUEUED_SLOT_ID,
            StackContent::vertical(0.0, Vec::new()),
        );

        let chips_row = Content::new(
            COMPOSER_CHIPS_ROW_ID,
            StackContent::horizontal(Spacing::Tight.value(), Vec::new()),
        );

        let mut attach_btn = Button::new("+").with_accessible_name("Attach context");
        attach_btn.set_variant(ButtonVariant::Panel);
        attach_btn.set_transparent_chrome(true);

        let mut send_btn = Button::new("↑").with_accessible_name("Send");
        send_btn.set_variant(ButtonVariant::Primary);

        let input_row = Content::new(
            COMPOSER_INPUT_ROW_ID,
            ComposerInputRow::new(
                Spacing::Tight.value(),
                COMPOSER_EDITOR_ID,
                vec![
                    Content::new(COMPOSER_ATTACH_BTN_ID, attach_btn),
                    Self::make_editor(),
                    Content::new(COMPOSER_SEND_BTN_ID, send_btn),
                ],
            ),
        );

        let card_body = Content::new(
            COMPOSER_CARD_STACK_ID,
            StackContent::vertical(0.0, vec![chips_row, input_row]),
        );
        let card = card_panel(
            COMPOSER_CARD_ID,
            card_body,
            Insets::new(6.0, 10.0, 6.0, 10.0),
        );

        Content::new(
            COMPOSER_ROOT_ID,
            StackContent::vertical(0.0, vec![queued_slot, card]),
        )
    }

    pub fn composing(host: &ContentHost) -> bool {
        host.get(COMPOSER_EDITOR_ID)
            .and_then(|c| c.component::<TextFieldContent>())
            .is_some_and(TextFieldContent::composing)
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

        let has_queued = queued.is_some();
        if let Some(item) = queued {
            let preview = if item.text.chars().count() > 54 {
                let head: String = item.text.chars().take(54).collect();
                format!("{head}…")
            } else {
                item.text
            };
            let mut steer_btn = Button::new("Steer");
            steer_btn.set_variant(ButtonVariant::Panel);
            steer_btn.set_transparent_chrome(true);
            let mut remove_btn = Button::new("×").with_accessible_name("Remove queued prompt");
            remove_btn.set_variant(ButtonVariant::Panel);
            remove_btn.set_transparent_chrome(true);

            let actions = Content::new(
                ComponentId(421),
                StackContent::horizontal(
                    Spacing::Micro.value(),
                    vec![
                        Content::new(COMPOSER_QUEUED_STEER_BTN_ID, steer_btn),
                        Content::new(COMPOSER_QUEUED_REMOVE_BTN_ID, remove_btn),
                    ],
                ),
            );
            let row = StackContent::horizontal(
                Spacing::Tight.value(),
                vec![
                    Content::new(COMPOSER_QUEUED_TEXT_ID, secondary_text(preview)),
                    actions,
                ],
            )
            .with_flexible_child(COMPOSER_QUEUED_TEXT_ID)
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
        if let Some(root) = host
            .get_mut(COMPOSER_ROOT_ID)
            .and_then(|c| c.component_mut::<StackContent>())
        {
            root.set_gap(if has_queued {
                Spacing::Tight.value()
            } else {
                0.0
            });
        }
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        snapshot: &RunSnapshot,
        _width_mode: ResponsiveWidthMode,
    ) {
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
            let _ = host.insert(COMPOSER_INPUT_ROW_ID, next_editor);
            let _ = host.reorder(
                COMPOSER_INPUT_ROW_ID,
                &[
                    COMPOSER_ATTACH_BTN_ID,
                    COMPOSER_EDITOR_ID,
                    COMPOSER_SEND_BTN_ID,
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
            let baseline = field.input().external_baseline();
            self.synced_rev = self.synced_rev.saturating_add(1).max(1);
            let _ = field.apply_external_value(
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

        let composing_now = Self::composing(host);
        let can_cancel = snapshot.run.as_ref().is_some_and(|r| r.can_cancel);
        if let Some(send) = host
            .get_mut(COMPOSER_SEND_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            if can_cancel {
                if send.label() != "■" {
                    send.set_contents("■");
                    send.set_accessible_name(Some("Stop".to_owned()));
                }
                send.set_disabled(false);
            } else {
                if send.label() != "↑" {
                    send.set_contents("↑");
                    send.set_accessible_name(Some("Send".to_owned()));
                }
                send.set_disabled(!snapshot.can_submit || composing_now);
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
            btn.set_variant(ButtonVariant::Panel);
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
            btn.set_variant(ButtonVariant::Panel);
            children.push(Content::new(id, btn));
        }

        let has_chips = !children.is_empty();
        replace_stack_children(host, COMPOSER_CHIPS_ROW_ID, children);
        if let Some(card_stack) = host
            .get_mut(COMPOSER_CARD_STACK_ID)
            .and_then(|c| c.component_mut::<StackContent>())
        {
            card_stack.set_gap(if has_chips {
                Spacing::Tight.value()
            } else {
                0.0
            });
        }
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
