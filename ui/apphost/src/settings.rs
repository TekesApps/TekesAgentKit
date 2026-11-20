//! `AgentSettingsOwner` (TekesUI native `SettingsOwner` with display preferences) and
//! `HarnessSettingsContainer` (epoch & CAS-fenced `settings.describe` / `settings.mutate`).

use crate::{
    AgentContainerAdapter, FOCUS_SWITCH_COMMAND, ManagedAgentContainer, SharedAgentWorkbench,
};
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tekes_agent_components_ui::{
    HarnessSettingsIntent, HarnessSettingsSurface, HarnessSettingsViewModel, ResponsiveWidthMode,
    SETTINGS_SHELL_ID, SettingFieldKindModel, SettingFieldModel,
};
use tekes_agent_ui_bindings::{HarnessSettingKind, HarnessSettingsBinding};
use tekes_ui_app_host::{
    ContainerPresentation, ContainerToolbarHost, ContainerToolbarPosition, LocalContainer,
    SettingsDisplayMode, SettingsOwner, SettingsWindow,
};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

pub const AGENT_SETTINGS_SCHEMA_TOML: &str = r#"
schema_version = 1
namespace = "agentkit.desktop"
provider = "agentkit.settings"

[[pages]]
id = "general"
label = "通用"

[[groups]]
id = "general"
page = "general"
label = "外观与排版"

[[groups.items]]
id = "appearance"
label = "外观"

[[groups.items.fields]]
id = "appearance"
path = ["general", "appearance"]
label = "外观"
description = "应用到所有 AppWindow。"
type = "enum"
default = "system"
options = [
  { value = "system", label = "跟随系统" },
  { value = "light", label = "浅色" },
  { value = "dark", label = "深色" },
]

[[groups.items.fields]]
id = "text-size"
path = ["general", "text_size_percent"]
label = "文字大小"
description = "文字缩放百分比。"
type = "integer"
default = 100
min = 50
max = 300
step = 10
suffix = "%"

[[groups]]
id = "agent-defaults"
page = "general"
label = "Agent 行为与 Diff 偏好"

[[groups.items]]
id = "defaults"
label = "默认展开与上下文"

[[groups.items.fields]]
id = "expand-thinking"
path = ["agent", "expand_thinking"]
label = "默认展开推理过程"
type = "boolean"
default = false

[[groups.items.fields]]
id = "diff-context-lines"
path = ["agent", "diff_context_lines"]
label = "Diff 上下文行数"
type = "integer"
default = 3
min = 0
max = 20
step = 1
"#;

/// Build an AppHost-compatible `SettingsOwner` bound to `"appearance"` & `"text-size"` display preferences.
pub fn build_agent_settings_owner(file: PathBuf) -> Result<SettingsOwner> {
    build_agent_settings_owner_with_mode(file, SettingsDisplayMode::Exclusive)
}

pub fn build_agent_settings_owner_with_mode(
    file: PathBuf,
    display_mode: SettingsDisplayMode,
) -> Result<SettingsOwner> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let window =
        SettingsWindow::from_toml(AGENT_SETTINGS_SCHEMA_TOML, file)?.display_mode(display_mode);
    SettingsOwner::new(window)?.with_display_preferences(
        "general/appearance/appearance",
        "general/appearance/text-size",
    )
}

struct HarnessSettingsAdapter {
    surface: HarnessSettingsSurface,
    initialized: bool,
}

impl HarnessSettingsAdapter {
    fn new() -> Self {
        Self {
            surface: HarnessSettingsSurface::new(),
            initialized: false,
        }
    }

    fn ensure_binding(
        &mut self,
        workbench: &SharedAgentWorkbench,
    ) -> Option<Arc<HarnessSettingsBinding>> {
        if let Some(existing) = workbench.harness_settings_binding() {
            return Some(existing);
        }
        if self.initialized {
            return None;
        }
        self.initialized = true;
        let run = workbench.run_binding()?;
        let wake: Arc<dyn tekes_agent_ui_bindings::UiWakeNotifier> = workbench.wake_hub();
        let binding =
            HarnessSettingsBinding::new(Arc::clone(run.runtime()), run.handle().clone(), wake);
        binding.refresh();
        workbench.attach_harness_settings_binding(Arc::clone(&binding));
        Some(binding)
    }

    fn build_model(binding: &HarnessSettingsBinding) -> HarnessSettingsViewModel {
        let snap = binding.snapshot();
        let fields = snap
            .fields
            .into_iter()
            .map(|f| SettingFieldModel {
                key: f.key,
                label: f.label,
                kind: match f.kind {
                    HarnessSettingKind::Enum(opts) => SettingFieldKindModel::Enum(opts),
                    HarnessSettingKind::Integer => SettingFieldKindModel::Integer,
                    HarnessSettingKind::Boolean => SettingFieldKindModel::Boolean,
                    HarnessSettingKind::String => SettingFieldKindModel::String,
                    HarnessSettingKind::Secret => SettingFieldKindModel::Secret,
                    HarnessSettingKind::ReadOnly => SettingFieldKindModel::ReadOnly,
                },
                display_value: f.display_value,
                writable: f.writable,
                is_secret: f.is_secret,
            })
            .collect();
        HarnessSettingsViewModel {
            loaded: snap.loaded,
            namespace_label: snap.namespace_label,
            revision: snap.revision,
            read_only: snap.read_only,
            fields,
            status: snap.last_status,
            error: snap.last_error,
            conflict_revision: snap.conflict_revision,
        }
    }
}

impl AgentContainerAdapter for HarnessSettingsAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(binding) = self.ensure_binding(workbench) else {
            return;
        };
        let model = Self::build_model(&binding);
        self.surface.sync(host, &model, width_mode);
    }

    fn handle_actions(
        &mut self,
        _host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(binding) = workbench.harness_settings_binding() else {
            return;
        };
        for intent in self.surface.handle_actions(&records) {
            match intent {
                HarnessSettingsIntent::Refresh => binding.refresh(),
                HarnessSettingsIntent::CycleEnum { key, next_value } => {
                    let mut patch = BTreeMap::new();
                    patch.insert(key, Value::String(next_value));
                    binding.apply_patch_async(patch);
                }
                HarnessSettingsIntent::ToggleBool { key, next_value } => {
                    let mut patch = BTreeMap::new();
                    patch.insert(key, Value::Bool(next_value));
                    binding.apply_patch_async(patch);
                }
                HarnessSettingsIntent::IncrementInt { key, next_value } => {
                    let mut patch = BTreeMap::new();
                    patch.insert(key, serde_json::json!(next_value));
                    binding.apply_patch_async(patch);
                }
            }
        }
    }
}

pub struct HarnessSettingsContainer;

impl HarnessSettingsContainer {
    pub fn build_tab(
        id: impl Into<String>,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let container = ManagedAgentContainer::build_local(
            id,
            SETTINGS_SHELL_ID,
            HarnessSettingsSurface::build_content(),
            workbench,
            HarnessSettingsAdapter::new(),
            false,
        )?;
        Ok(container
            .title("Harness 设置")
            .switcher_icon("tekes-settings")
            .presentation(ContainerPresentation::Tab)
            .toolbar_host(ContainerToolbarHost::Container {
                position: ContainerToolbarPosition::Top,
            })
            .toolbar(vec![ToolbarItem {
                id: FOCUS_SWITCH_COMMAND,
                title: "Focus 切换".into(),
                enabled: true,
                geometry_dependent: false,
            }])
            .focus_switch(FOCUS_SWITCH_COMMAND))
    }
}
