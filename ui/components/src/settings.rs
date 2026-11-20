//! Normalized Harness settings view (`HarnessSettingsSurface`) with CAS revision display,
//! read-only warning, conflict notice, and strict secret masking (`••••••••`).

use crate::ResponsiveWidthMode;
use crate::ids::*;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, ComponentId, Content, ContentActionKind, ContentActionPayload, ContentActionRecord,
        ContentHost, ScrollContent, SegmentedControlContent, StackContent, TextContent,
    },
    extended::{Axis, Stack},
};
use tekes_ui_tokens::Spacing;

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
        let header = Content::new(
            SETTINGS_HEADER_ID,
            TextContent::new("Harness 策略设置 · 尚未加载"),
        );
        let banner = Content::new(SETTINGS_BANNER_ID, TextContent::new(""));

        let mut refresh_btn = Button::new("刷新策略");
        refresh_btn.variant = ButtonVariant::Quiet;
        let refresh = Content::new(SETTINGS_REFRESH_BTN_ID, refresh_btn);

        let mut apply_btn = Button::new("CAS 状态校验");
        apply_btn.button.state.disabled = true;
        let apply = Content::new(SETTINGS_APPLY_BTN_ID, apply_btn);

        let actions = Content::new(
            SETTINGS_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![refresh, apply],
            },
        );

        let fields_stack = Content::new(
            SETTINGS_FIELDS_STACK_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
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
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(SETTINGS_FIELDS_SCROLL_ID),
                children: vec![header, actions, banner, scroll],
            },
        )
    }

    pub fn sync(
        &mut self,
        host: &mut ContentHost,
        model: &HarnessSettingsViewModel,
        mode: ResponsiveWidthMode,
    ) {
        let mode_label = match mode {
            ResponsiveWidthMode::Compact => "Compact",
            ResponsiveWidthMode::Expanded => "Expanded",
        };
        let header_text = format!(
            "{} · CAS rev {}{} ({mode_label})",
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
            "当前 Harness 配置处于只读模式 (readOnly)，所有修改操作已禁用。".to_owned()
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
            let row_id = ComponentId(SETTINGS_FIELD_BASE + idx as u64);
            let label_id = ComponentId(SETTINGS_FIELD_BASE + 500 + idx as u64);
            let ctrl_id = ComponentId(SETTINGS_CONTROL_BASE + idx as u64);

            if field.is_secret {
                let row = Content::new(
                    row_id,
                    TextContent::new(format!("{}: •••••••• (只读掩码)", field.label)),
                );
                children.push(row);
                continue;
            }

            let label = Content::new(
                label_id,
                TextContent::new(format!("{} = {}", field.label, field.display_value)),
            );
            let field_enabled = field.writable && !model.read_only;
            let control = match &field.kind {
                SettingFieldKindModel::Enum(options) if !options.is_empty() => {
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
                    Content::new(ctrl_id, seg)
                }
                SettingFieldKindModel::Boolean => {
                    let current = field.display_value == "true";
                    let mut btn = Button::new(if current { "已开启" } else { "已关闭" });
                    btn.button.state.disabled = !field_enabled;
                    if field_enabled {
                        self.controls.push((
                            ctrl_id,
                            ControlTarget::Bool {
                                key: field.key.clone(),
                                current,
                            },
                        ));
                    }
                    Content::new(ctrl_id, btn)
                }
                SettingFieldKindModel::Integer => {
                    let current: i64 = field.display_value.parse().unwrap_or(0);
                    let mut btn = Button::new("+1");
                    btn.variant = ButtonVariant::Quiet;
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
                    Content::new(ctrl_id, btn)
                }
                _ => Content::new(
                    ctrl_id,
                    TextContent::new(if field.writable {
                        "可写".to_owned()
                    } else {
                        "只读".to_owned()
                    }),
                ),
            };
            children.push(Content::new(
                row_id,
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: Spacing::Tight.value(),
                    },
                    flexible_child: None,
                    children: vec![label, control],
                },
            ));
        }

        if children.is_empty() {
            children.push(Content::new(
                ComponentId(SETTINGS_FIELD_BASE),
                TextContent::new("正在读取 Harness 策略项..."),
            ));
        }
        replace_stack_children(host, SETTINGS_FIELDS_STACK_ID, children);
    }

    pub fn handle_actions(&self, records: &[ContentActionRecord]) -> Vec<HarnessSettingsIntent> {
        let mut intents = Vec::new();
        for rec in records {
            if rec.action.target == SETTINGS_REFRESH_BTN_ID
                && matches!(rec.action.kind, ContentActionKind::Activate)
            {
                intents.push(HarnessSettingsIntent::Refresh);
                continue;
            }
            if let Some((_, target)) = self
                .controls
                .iter()
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
                    (ControlTarget::Bool { key, current }, ContentActionKind::Activate, _) => {
                        intents.push(HarnessSettingsIntent::ToggleBool {
                            key: key.clone(),
                            next_value: !current,
                        });
                    }
                    (ControlTarget::IntStep { key, current }, ContentActionKind::Activate, _) => {
                        intents.push(HarnessSettingsIntent::IncrementInt {
                            key: key.clone(),
                            next_value: current + 1,
                        });
                    }
                    _ => {}
                }
            }
        }
        intents
    }
}
