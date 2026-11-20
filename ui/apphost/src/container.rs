//! Reusable `ContainerContent` adapter bridging AgentKit components with `tekes-ui-app-host`.

use crate::workbench::SharedAgentWorkbench;
use anyhow::Result;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tekes_agent_components_ui::{AgentContainerShell, ResponsiveWidthMode};
use tekes_ui_app_host::{ContainerContent, ContainerRenderContext, LocalContainer, navigation};
use tekes_ui_controls::composition::{ComponentId, Content, ContentActionRecord, ContentHost};
use tekes_ui_platform_macos::WindowWake;

/// Optional GPU resource hook for containers owning decoded images (inline images, artifact previews).
pub trait ResourceContextOps {
    fn prepare_resources(
        &mut self,
        _host: &mut ContentHost,
        _context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        Ok(())
    }

    fn finish_resources(
        &mut self,
        _host: &mut ContentHost,
        _context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        Ok(())
    }
}

/// Default no-op GPU resource handler for pure vector/text containers.
#[derive(Debug, Default)]
pub struct NoopResources;

impl ResourceContextOps for NoopResources {}

/// Per-container projection adapter invoked ONLY inside that container's own `ContainerContent` hooks.
pub trait AgentContainerAdapter {
    /// Update `host` from `workbench` when `workbench.revision()` changed or width mode changed.
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    );

    /// Handle typed UI actions drained exclusively from this container's own `ContentHost`.
    fn handle_actions(
        &mut self,
        _host: &mut ContentHost,
        _workbench: &SharedAgentWorkbench,
        _records: Vec<ContentActionRecord>,
    ) {
    }

    /// Handle AppHost toolbar/menu commands routed to this container.
    fn handle_command(
        &mut self,
        _host: &mut ContentHost,
        _workbench: &SharedAgentWorkbench,
        _command_id: u64,
    ) -> Result<()> {
        Ok(())
    }

    fn prepare_resources(
        &mut self,
        _host: &mut ContentHost,
        _context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        Ok(())
    }

    fn finish_resources(
        &mut self,
        _host: &mut ContentHost,
        _context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        Ok(())
    }
}

/// Manages one persistent `ContentHost` inside `AppHost` without rebuilding on `focus <-> others` swaps,
/// deferring background updates while hidden (`!active`) and keeping sibling containers unborrowed.
pub struct ManagedAgentContainer<A: AgentContainerAdapter + 'static> {
    host: ContentHost,
    workbench: SharedAgentWorkbench,
    adapter: Rc<RefCell<A>>,
    emits_navigation: bool,
    active_flag: Rc<Cell<bool>>,
    arranged_width: Rc<Cell<f32>>,
    width_mode: Rc<Cell<ResponsiveWidthMode>>,
    last_applied_revision: u64,
    last_applied_width_mode: ResponsiveWidthMode,
    render_sync_count: u64,
}

impl<A: AgentContainerAdapter + 'static> ManagedAgentContainer<A> {
    pub fn build_local(
        id: impl Into<String>,
        shell_id: ComponentId,
        root_child: Content,
        workbench: SharedAgentWorkbench,
        adapter: A,
        emits_navigation: bool,
    ) -> Result<LocalContainer> {
        let active_flag = Rc::new(Cell::new(true));
        let arranged_width = Rc::new(Cell::new(0.0));
        let width_mode = Rc::new(Cell::new(ResponsiveWidthMode::Expanded));
        let shell = AgentContainerShell::new(
            root_child,
            Rc::clone(&active_flag),
            Rc::clone(&arranged_width),
            Rc::clone(&width_mode),
        );
        let mut host = ContentHost::new(Content::new(shell_id, shell))
            .map_err(|err| anyhow::anyhow!("Agent ContentHost init failed: {err:?}"))?;
        host.set_action_events(true);

        let adapter_rc = Rc::new(RefCell::new(adapter));
        adapter_rc
            .borrow_mut()
            .sync_from_workbench(&mut host, &workbench, width_mode.get());
        let initial_rev = workbench.revision();

        let source = Self {
            host,
            workbench: workbench.clone(),
            adapter: Rc::clone(&adapter_rc),
            emits_navigation,
            active_flag,
            arranged_width,
            width_mode,
            last_applied_revision: initial_rev,
            last_applied_width_mode: ResponsiveWidthMode::Expanded,
            render_sync_count: 0,
        };

        let action_workbench = workbench.clone();
        let action_adapter = Rc::clone(&adapter_rc);
        let cmd_workbench = workbench;
        let cmd_adapter = adapter_rc;

        Ok(LocalContainer::from_source(id, source)
            .on_action(move |host, records| {
                action_adapter
                    .borrow_mut()
                    .handle_actions(host, &action_workbench, records);
            })
            .on_command(move |host, command| {
                cmd_adapter
                    .borrow_mut()
                    .handle_command(host, &cmd_workbench, command)
            }))
    }

    pub fn build_source(
        shell_id: ComponentId,
        root_child: Content,
        workbench: SharedAgentWorkbench,
        adapter: A,
        emits_navigation: bool,
    ) -> Result<Self> {
        let active_flag = Rc::new(Cell::new(true));
        let arranged_width = Rc::new(Cell::new(0.0));
        let width_mode = Rc::new(Cell::new(ResponsiveWidthMode::Expanded));
        let shell = AgentContainerShell::new(
            root_child,
            Rc::clone(&active_flag),
            Rc::clone(&arranged_width),
            Rc::clone(&width_mode),
        );
        let mut host = ContentHost::new(Content::new(shell_id, shell))
            .map_err(|err| anyhow::anyhow!("Agent ContentHost init failed: {err:?}"))?;
        host.set_action_events(true);
        let adapter_rc = Rc::new(RefCell::new(adapter));
        adapter_rc
            .borrow_mut()
            .sync_from_workbench(&mut host, &workbench, width_mode.get());
        let initial_rev = workbench.revision();
        Ok(Self {
            host,
            workbench,
            adapter: adapter_rc,
            emits_navigation,
            active_flag,
            arranged_width,
            width_mode,
            last_applied_revision: initial_rev,
            last_applied_width_mode: ResponsiveWidthMode::Expanded,
            render_sync_count: 0,
        })
    }

    pub fn render_sync_count(&self) -> u64 {
        self.render_sync_count
    }

    pub fn width_mode(&self) -> ResponsiveWidthMode {
        self.width_mode.get()
    }

    pub fn arranged_width(&self) -> f32 {
        self.arranged_width.get()
    }

    pub fn is_active(&self) -> bool {
        self.active_flag.get()
    }
}

impl<A: AgentContainerAdapter + 'static> ContainerContent for ManagedAgentContainer<A> {
    fn with_host(&mut self, visit: &mut dyn FnMut(&mut ContentHost)) {
        visit(&mut self.host);
    }

    fn prepare_for_render(&mut self) {
        let current_rev = self.workbench.sync_background_revision();
        let current_width_mode = self.width_mode.get();
        if !self.active_flag.get() {
            return;
        }
        if current_rev != self.last_applied_revision
            || current_width_mode != self.last_applied_width_mode
        {
            self.adapter.borrow_mut().sync_from_workbench(
                &mut self.host,
                &self.workbench,
                current_width_mode,
            );
            self.last_applied_revision = current_rev;
            self.last_applied_width_mode = current_width_mode;
            self.render_sync_count = self.render_sync_count.saturating_add(1);
        }
    }

    fn prepare_resources(&mut self, context: &mut ContainerRenderContext<'_>) -> Result<()> {
        self.adapter
            .borrow_mut()
            .prepare_resources(&mut self.host, context)
    }

    fn finish_resources(&mut self, context: &mut ContainerRenderContext<'_>) -> Result<()> {
        self.adapter
            .borrow_mut()
            .finish_resources(&mut self.host, context)
    }

    fn take_navigation_request(&mut self) -> Option<navigation::Entry> {
        if self.emits_navigation {
            self.workbench.pop_navigation()
        } else {
            None
        }
    }

    fn start_process(&mut self, wake: WindowWake) -> Result<()> {
        self.workbench.register_wake(wake);
        Ok(())
    }
}
