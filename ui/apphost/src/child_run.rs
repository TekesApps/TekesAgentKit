//! Multi-instance read-only `ChildRunContainer` (`ContainerMultiplicity::Multiple`)
//! projecting bounded/independent subagent traces without mounting a writable `Composer`.

use crate::{AgentContainerAdapter, ManagedAgentContainer, SharedAgentWorkbench};
use anyhow::Result;
use std::sync::Arc;
use tekes_agent_components_ui::{
    CHILD_RUN_SHELL_ID, ChildRunIntent, ChildRunSurface, ChildRunSurfaceStatus, ResponsiveWidthMode,
};
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_bindings::{CHILD_SUBAGENT_SESSION_ID, ChildRunBinding, ChildRunStatus};
use tekes_ui_app_host::{ContainerFactory, ContainerMultiplicity, LocalContainer};
use tekes_ui_controls::composition::{ContentActionRecord, ContentHost};

struct ChildRunAdapter {
    instance_id: String,
    surface: ChildRunSurface,
    initialized: bool,
}

impl ChildRunAdapter {
    fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            surface: ChildRunSurface::new(),
            initialized: false,
        }
    }

    fn ensure_binding(&mut self, workbench: &SharedAgentWorkbench) -> Option<Arc<ChildRunBinding>> {
        if let Some(existing) = workbench.child_run_binding(&self.instance_id) {
            return Some(existing);
        }
        if self.initialized {
            return None;
        }
        self.initialized = true;
        let run = workbench.run_binding()?;
        let wake: Arc<dyn tekes_agent_ui_bindings::UiWakeNotifier> = workbench.wake_hub();

        let (parent_scope, child_session_id) = if let Some(req) = workbench.pop_pending_child_run()
        {
            (req.scope, req.child_session_id)
        } else {
            let snap = run.run_snapshot();
            let parent = run
                .current_scope()
                .unwrap_or_else(|| RunScope::main(run.endpoint_id(), "session-alpha"));
            let child_id = run
                .summary_facts_for(&snap)
                .and_then(|facts| facts.subagents.into_iter().find_map(|s| s.session_id))
                .unwrap_or_else(|| CHILD_SUBAGENT_SESSION_ID.to_owned());
            (parent, child_id)
        };

        if let Ok(binding) = ChildRunBinding::open_for_child(
            Arc::clone(run.runtime()),
            parent_scope,
            child_session_id,
            run.handle().clone(),
            wake,
        ) {
            binding.refresh();
            workbench.attach_child_run_binding(&self.instance_id, Arc::clone(&binding));
            return Some(binding);
        }
        None
    }
}

impl AgentContainerAdapter for ChildRunAdapter {
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
        let status = match view.status {
            ChildRunStatus::Loading => ChildRunSurfaceStatus::Loading,
            ChildRunStatus::Ready => ChildRunSurfaceStatus::Ready,
            ChildRunStatus::Missing(reason) => ChildRunSurfaceStatus::Missing(reason),
        };
        self.surface.sync(
            host,
            &view.child_scope,
            &status,
            &view.snapshot,
            view.can_load_earlier,
            width_mode,
        );
    }

    fn handle_actions(
        &mut self,
        _host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(binding) = workbench.child_run_binding(&self.instance_id) else {
            return;
        };
        for intent in self.surface.handle_actions(&records) {
            match intent {
                ChildRunIntent::LoadEarlier => binding.load_earlier(),
                ChildRunIntent::Refresh => binding.refresh(),
            }
        }
    }
}

pub struct ChildRunContainer;

impl ChildRunContainer {
    pub fn build(
        instance_id: impl Into<String>,
        workbench: SharedAgentWorkbench,
    ) -> Result<LocalContainer> {
        let id = instance_id.into();
        let container = ManagedAgentContainer::build_local(
            id.clone(),
            CHILD_RUN_SHELL_ID,
            ChildRunSurface::build_content(),
            workbench,
            ChildRunAdapter::new(id),
            false,
        )?;
        Ok(container.title("Subagent").switcher_icon("tekes-job"))
    }

    /// Registers a `ContainerMultiplicity::Multiple` factory for dynamic read-only subagent trace tabs.
    /// Callers MUST NOT pre-declare `id` inside `OthersOptions.groups`.
    pub fn factory(id: impl Into<String>, workbench: SharedAgentWorkbench) -> ContainerFactory {
        let factory_id = id.into();
        ContainerFactory::local(
            factory_id,
            "Subagent",
            ContainerMultiplicity::Multiple,
            move |instance_id| Self::build(instance_id, workbench.clone()),
        )
        .switcher_icon("tekes-job")
    }
}
