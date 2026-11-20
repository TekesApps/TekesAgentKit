//! Normalized Harness settings view (`HarnessSettingsSurface`) with CAS revision display,
//! read-only warning, conflict notice, native `SwitchContent` toggles, and strict secret masking (`••••••••`).

use crate::ResponsiveWidthMode;
use crate::ids::*;
use crate::ui_helpers::{heading_text, icon_button, label_text, secondary_text};
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, ScrollContent, SegmentedControlContent, StackContent, SwitchContent,
        TextContent,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::{Spacing, proposal::layout::MainAxisDistribution};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingFieldKindModel {
    Enum(Vec<String>),
    Integer,
    Boolean,
    String,
    Secret,
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingFieldModel {
    pub key: String,
    pub label: String,
    pub kind: SettingFieldKindModel,
    pub display_value: String,
    pub writable: bool,
    pub is_secret: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSettingsViewModel {
    pub loaded: bool,
    pub namespace_label: String,
    pub revision: u64,
    pub read_only: bool,
    pub fields: Vec<SettingFieldModel>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub conflict_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessSettingsIntent {
    Refresh,
    CycleEnum { key: String, next_value: String },
    ToggleBool { key: String, next_value: bool },
    IncrementInt { key: String, next_value: i64 },
}

#[derive(Clone, Debug)]
enum ControlTarget {
    Enum { key: String, options: Vec<String> },
    Bool { key: String, current: bool },
    IntStep { key: String, current: i64 },
}

#[derive(Debug, Default)]
pub struct HarnessSettingsSurface {
    last_sig: String,
    controls: Vec<(ComponentId, ControlTarget)>,
}

impl HarnessSettingsSurface {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_content() -> Content {
        let header = Content::new(SETTINGS_HEADER_ID, heading_text("Harness Settings"));
        let refresh = Content::new(
            SETTINGS_REFRESH_BTN_ID,
            icon_button(
                SETTINGS_REFRESH_ICON_ID,
                "tekes-refresh",
                "Refresh settings",
            ),
        );
        let header_row = StackContent {
            model: Stack {
                axis: Axis::Horizontal,
                gap: Spacing::Tight.value(),
            },
            flexible_child: Some(SETTINGS_HEADER_ID),
            children: vec![header, refresh],
        }
        .into_content_with_distribution(SETTINGS_HEADER_ROW_ID, MainAxisDistribution::SpaceBetween);

        let banner = Content::new(SETTINGS_BANNER_ID, secondary_text(""));

        let fields_stack = Content::new(
            SETTINGS_FIELDS_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
        );
        let scroll = Content::new(
            SETTINGS_FIELDS_SCROLL_ID,
            ScrollContent::new(ScrollArea::default(), fields_stack),
        );

        Content::new(
            SETTINGS_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: Some(SETTINGS_FIELDS_SCROLL_ID),
                children: vec![header_row, banner, scroll],
            },
        )
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        model: &HarnessSettingsViewModel,
        _mode: ResponsiveWidthMode,
    ) {
        let header_text = format!(
            "{} · CAS rev {}{}",
            model.namespace_label,
            model.revision,
            if model.read_only { " [只读]" } else { "" }
        );
        if let Some(node) = host.get_mut(SETTINGS_HEADER_ID)
            && let Some(txt) = node.component_mut::<TextContent>()
            && txt.text != header_text
        {
            txt.text = header_text;
        }

        let banner_text = if let Some(err) = &model.error {
            format!("异常 / 冲突: {err}")
        } else if model.read_only {
            "只读模式 · Read-only harness settings".to_owned()
        } else {
            model.status.clone().unwrap_or_default()
        };
        if let Some(node) = host.get_mut(SETTINGS_BANNER_ID)
            && let Some(txt) = node.component_mut::<TextContent>()
            && txt.text != banner_text
        {
            txt.text = banner_text;
        }

        let sig = format!("{model:?}");
        if self.last_sig == sig {
            return;
        }
        self.last_sig = sig;

        self.controls.clear();
        let mut children = Vec::new();
        for (idx, field) in model.fields.iter().enumerate() {
            let idx_u64 = idx as u64;
            let row_id = ComponentId(SETTINGS_FIELD_BASE + idx_u64);
            let label_id = ComponentId(SETTINGS_FIELD_BASE + 500 + idx_u64);
            let ctrl_id = ComponentId(SETTINGS_CONTROL_BASE + idx_u64);

            if field.is_secret {
                let row = StackContent {
                    model: Stack {
                        axis: Axis::Horizontal,
                        gap: Spacing::Tight.value(),
                    },
                    flexible_child: Some(label_id),
                    children: vec![
                        Content::new(label_id, label_text(&field.label)),
                        Content::new(ctrl_id, secondary_text("••••••••")),
                    ],
                }
                .into_content_with_distribution(row_id, MainAxisDistribution::SpaceBetween);
                children.push(row);
                continue;
            }

            let field_enabled = field.writable && !model.read_only;
            match &field.kind {
                SettingFieldKindModel::Boolean => {
                    let current = field.display_value == "true";
                    let mut sw = SwitchContent::new(&field.label, current);
                    sw.model.disabled = !field_enabled;
                    if field_enabled {
                        self.controls.push((
                            ctrl_id,
                            ControlTarget::Bool {
                                key: field.key.clone(),
                                current,
                            },
                        ));
                    }
                    children.push(Content::new(ctrl_id, sw));
                }
                SettingFieldKindModel::Enum(options) if !options.is_empty() => {
                    let label = Content::new(label_id, label_text(&field.label));
                    let mut seg = SegmentedControlContent::new(&field.label, options.clone());
                    let active_idx = options
                        .iter()
                        .position(|o| o == &field.display_value)
                        .unwrap_or(0);
                    let _ = seg.set_selected(active_idx);
                    if field_enabled {
                        self.controls.push((
                            ctrl_id,
                            ControlTarget::Enum {
                                key: field.key.clone(),
                                options: options.clone(),
                            },
                        ));
                    }
                    children.push(Content::new(
                        row_id,
                        StackContent {
                            model: Stack {
                                axis: Axis::Vertical,
                                gap: Spacing::Micro.value(),
                            },
                            flexible_child: None,
                            children: vec![label, Content::new(ctrl_id, seg)],
                        },
                    ));
                }
                SettingFieldKindModel::Integer => {
                    let current: i64 = field.display_value.parse().unwrap_or(0);
                    let label = Content::new(
                        label_id,
                        TextContent::new(format!("{}: {}", field.label, field.display_value)),
                    );
                    let mut btn = Button::new("+1");
                    btn.variant = ButtonVariant::Panel;
                    btn.button.state.disabled = !field_enabled;
                    if field_enabled {
                        self.controls.push((
                            ctrl_id,
                            ControlTarget::IntStep {
                                key: field.key.clone(),
                                current,
                            },
                        ));
                    }
                    let row = StackContent {
                        model: Stack {
                            axis: Axis::Horizontal,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: Some(label_id),
                        children: vec![label, Content::new(ctrl_id, btn)],
                    }
                    .into_content_with_distribution(row_id, MainAxisDistribution::SpaceBetween);
                    children.push(row);
                }
                _ => {
                    let row = StackContent {
                        model: Stack {
                            axis: Axis::Horizontal,
                            gap: Spacing::Tight.value(),
                        },
                        flexible_child: Some(label_id),
                        children: vec![
                            Content::new(label_id, label_text(&field.label)),
                            Content::new(ctrl_id, secondary_text(&field.display_value)),
                        ],
                    }
                    .into_content_with_distribution(row_id, MainAxisDistribution::SpaceBetween);
                    children.push(row);
                }
            }
        }

        if children.is_empty() {
            children.push(Content::new(
                ComponentId(SETTINGS_FIELD_BASE),
                secondary_text("Loading harness settings…"),
            ));
        }
        replace_stack_children(host, SETTINGS_FIELDS_STACK_ID, children);
    }

    pub fn handle_actions(
        &mut self,
        host: &ContentHost,
        records: &[ContentActionRecord],
    ) -> Vec<HarnessSettingsIntent> {
        let mut intents = Vec::new();
        let mut toggled_ids = Vec::new();

        for (id, target) in &mut self.controls {
            if let ControlTarget::Bool { key, current } = target
                && let Some(sw) = host.get(*id).and_then(|c| c.component::<SwitchContent>())
                && sw.model.checked != *current
            {
                *current = sw.model.checked;
                toggled_ids.push(*id);
                intents.push(HarnessSettingsIntent::ToggleBool {
                    key: key.clone(),
                    next_value: sw.model.checked,
                });
            }
        }

        for rec in records {
            if rec.action.target == SETTINGS_REFRESH_BTN_ID
                && matches!(rec.action.kind, ContentActionKind::Activate)
            {
                intents.push(HarnessSettingsIntent::Refresh);
                continue;
            }
            if let Some((_, target)) = self
                .controls
                .iter_mut()
                .find(|(id, _)| *id == rec.action.target)
            {
                match (target, &rec.action.kind, &rec.payload) {
                    (
                        ControlTarget::Enum { key, options },
                        _,
                        Some(ContentActionPayload::SegmentSelection(seg)),
                    ) => {
                        if let Some(opt) = options.get(seg.index()) {
                            intents.push(HarnessSettingsIntent::CycleEnum {
                                key: key.clone(),
                                next_value: opt.clone(),
                            });
                        }
                    }
                    (ControlTarget::Bool { key, current }, ContentActionKind::Activate, _)
                        if !toggled_ids.contains(&rec.action.target) =>
                    {
                        let next = !*current;
                        *current = next;
                        intents.push(HarnessSettingsIntent::ToggleBool {
                            key: key.clone(),
                            next_value: next,
                        });
                    }
                    (ControlTarget::IntStep { key, current }, ContentActionKind::Activate, _) => {
                        intents.push(HarnessSettingsIntent::IncrementInt {
                            key: key.clone(),
                            next_value: *current + 1,
                        });
                    }
                    _ => {}
                }
            }
        }
        intents
    }
}
