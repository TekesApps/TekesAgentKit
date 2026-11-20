//! Multi-instance `SideChatContainer` (`ContainerMultiplicity::Multiple`) providing ephemeral
//! fork conversations with isolated draft / CJK `Preedit` state and `PaneAccess` gating.

use crate::{
    AgentContainerAdapter, FOCUS_SWITCH_COMMAND, ManagedAgentContainer, SharedAgentWorkbench,
};
use anyhow::Result;
use std::sync::Arc;
use tekes_agent_components_ui::{
    ResponsiveWidthMode, SIDECHAT_SHELL_ID, SideChatIntent, SideChatSurface, SideChatSurfaceAccess,
};
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_bindings::{SideChatAccess, SideChatBinding};
use tekes_ui_app_host::{
    ContainerFactory, ContainerMultiplicity, ContainerToolbarHost, ContainerToolbarPosition,
    LocalContainer,
};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct SideChatAdapter {
    instance_id: String,
    surface: SideChatSurface,
    initialized: bool,
}

impl SideChatAdapter {
    fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            surface: SideChatSurface::new(),
            initialized: false,
        }
    }

    fn ensure_binding(&mut self, workbench: &SharedAgentWorkbench) -> Option<Arc<SideChatBinding>> {
        if let Some(existing) = workbench.sidechat_binding(&self.instance_id) {
            return Some(existing);
        }
        if self.initialized {
            return None;
        }
        self.initialized = true;
        let run = workbench.run_binding()?;
        let wake: Arc<dyn tekes_agent_ui_bindings::UiWakeNotifier> = workbench.wake_hub();
        let binding = SideChatBinding::new(Arc::clone(run.runtime()), run.handle().clone(), wake);
        if let Some(req) = workbench.pop_pending_sidechat() {
            binding.create_for_parent(req.parent_scope, Some(&req.title));
        }
        workbench.attach_sidechat_binding(&self.instance_id, Arc::clone(&binding));
        Some(binding)
    }
}

impl AgentContainerAdapter for SideChatAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(binding) = self.ensure_binding(workbench) else {
            return;
        };
        let view = binding.view_snapshot();
        let access = match view.access {
            SideChatAccess::Available => SideChatSurfaceAccess::Available,
            SideChatAccess::Creating => SideChatSurfaceAccess::Creating,
            SideChatAccess::Closing => SideChatSurfaceAccess::Closing,
            SideChatAccess::Unsupported => SideChatSurfaceAccess::Unsupported,
            SideChatAccess::Disconnected => SideChatSurfaceAccess::Disconnected,
            SideChatAccess::Unknown(reason) => SideChatSurfaceAccess::Unknown(reason),
            SideChatAccess::Closed => SideChatSurfaceAccess::Closed,
        };
        let title = view.chat.as_ref().map(|c| c.title.as_str());
        let sid = view.chat.as_ref().map(|c| c.scope.session_id.as_str());
        self.surface.sync(
            host,
            title,
            sid,
            &access,
            view.run_snapshot.as_deref(),
            view.error.as_deref(),
            width_mode,
        );
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(binding) = workbench.sidechat_binding(&self.instance_id) else {
            return;
        };
        for intent in self.surface.handle_actions(host, &records) {
            match intent {
                SideChatIntent::CreateBranch => {
                    if let Some(run) = workbench.run_binding() {
                        let parent = run
                            .current_scope()
                            .unwrap_or_else(|| RunScope::main(run.endpoint_id(), "session-alpha"));
                        binding.create_for_parent(parent, Some("临时分支"));
                    }
                }
                SideChatIntent::CloseDiscard => {
                    binding.close_owned();
                }
                SideChatIntent::EditDraft(text) => {
                    binding.set_draft_text(text);
                }
                SideChatIntent::SendPrompt => {
                    let _ = binding.send_prompt();
                }
            }
        }
    }
}

pub struct SideChatContainer;

impl SideChatContainer {
    pub fn build(
        instance_id: impl Into<String>,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let id = instance_id.into();
        let container = ManagedAgentContainer::build_local(
            id.clone(),
            SIDECHAT_SHELL_ID,
            SideChatSurface::build_content(),
            workbench,
            SideChatAdapter::new(id),
            false,
        )?;
        Ok(container
            .title("SideChat 分支")
            .switcher_icon("tekes-side-chat")
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

    /// Registers a `ContainerMultiplicity::Multiple` factory for ephemeral side-chat panels.
    /// Callers MUST NOT pre-declare `id` inside `OthersOptions.groups`.
    pub fn factory(id: impl Into<String>, workbench: SharedAgentWorkbench) -> ContainerFactory {
        let factory_id = id.into();
        ContainerFactory::local(
            factory_id,
            "SideChat",
            ContainerMultiplicity::Multiple,
            move |instance_id| Self::build(instance_id, workbench.clone()),
        )
        .switcher_icon("tekes-side-chat")
    }
}
