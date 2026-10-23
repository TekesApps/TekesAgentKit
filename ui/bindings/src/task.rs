//! Task row resolution retains the existing TaskPanelState generation fences.
use crate::{PaneAccess, PaneAuthority, RunBinding, RunPresentationAlias};
use std::sync::{Arc, Mutex, Weak};
use tekes_agent_components_ui::SidePanelTab;
use tekes_agent_contract::{Result, unsupported};
use tekes_agent_presentation::{AgentTask, RunScope, TaskLoadToken, TaskPanelState};
use tekes_agent_session::SessionRuntime;
use tokio::sync::watch;

struct MountedTask {
    projection: TaskPanelState,
    source_generation: u64,
    run: Option<Arc<RunBinding>>,
}
#[derive(Clone)]
pub(crate) struct TaskRunGuard {
    owner: Weak<Mutex<MountedTask>>,
    generation: u64,
    scope: RunScope,
}
impl TaskRunGuard {
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.scope == other.scope
            && Weak::ptr_eq(&self.owner, &other.owner)
    }
    pub(crate) fn live(&self) -> bool {
        self.owner.upgrade().is_some_and(|owner| {
            let state = owner.lock().expect("task pane lock");
            state.source_generation == self.generation
                && state.projection.resolved
                && state
                    .projection
                    .value
                    .as_ref()
                    .is_some_and(|task| task.scope == self.scope)
        })
    }
}

/// One task pane owns one existing TaskPanelState, rather than caching a task's
/// first session forever. Host reads commit explicitly; failures are not absence.
pub struct TaskPaneBinding {
    runtime: Arc<SessionRuntime>,
    parent_scope: Option<RunScope>,
    parent_authority: PaneAuthority,
    parent_presentation: Option<RunPresentationAlias>,
    state: Arc<Mutex<MountedTask>>,
    changes: watch::Sender<TaskPanelState>,
}
impl TaskPaneBinding {
    pub fn for_run(parent: &RunBinding) -> Self {
        let mut source = Self::new(parent.runtime.clone());
        source.parent_scope = Some(parent.scope().clone());
        source.parent_authority = parent.authority();
        source.parent_presentation = parent.presentation_alias();
        source
    }
    pub fn uses_run(&self, parent: &RunBinding) -> bool {
        Arc::ptr_eq(&self.runtime, &parent.runtime)
    }
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        let projection = TaskPanelState::default();
        Self {
            runtime,
            parent_scope: None,
            parent_authority: PaneAuthority::default(),
            parent_presentation: None,
            changes: watch::channel(projection.clone()).0,
            state: Arc::new(Mutex::new(MountedTask {
                projection,
                source_generation: 0,
                run: None,
            })),
        }
    }
    pub fn snapshot(&self) -> TaskPanelState {
        self.state
            .lock()
            .expect("task pane lock")
            .projection
            .clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<TaskPanelState> {
        self.changes.subscribe()
    }
    pub fn owns_run(&self, run: &Arc<RunBinding>) -> bool {
        self.state
            .lock()
            .expect("task pane lock")
            .run
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, run))
    }
    pub fn tab(&self, pending_title: impl Into<String>) -> Result<SidePanelTab> {
        SidePanelTab::task_from_state(&self.snapshot(), pending_title).map_err(unsupported)
    }
    pub fn begin(&self, parent: RunScope, task_id: impl Into<String>) -> Result<TaskLoadToken> {
        tekes_agent_contract::require(
            parent.endpoint_id == self.runtime.inspect(|state| state.endpoint_id.clone()),
            "task endpoint mismatch",
        )?;
        let mut state = self.state.lock().expect("task pane lock");
        let token = state.projection.begin(parent, task_id)?;
        Self::retire(&mut state);
        self.changes.send_replace(state.projection.clone());
        Ok(token)
    }
    fn retire(state: &mut MountedTask) {
        state.source_generation = state
            .source_generation
            .checked_add(1)
            .expect("task source generation exhausted");
        state.run = None;
    }
    /// Apply each host row observation with its original load token. Status/name
    /// updates preserve the exact run and draft; pointer/loss/error changes retire
    /// it immediately, including captured callbacks and A→B→A results.
    pub fn commit(
        &self,
        token: &TaskLoadToken,
        result: std::result::Result<Option<AgentTask>, String>,
    ) -> bool {
        let mut state = self.state.lock().expect("task pane lock");
        let previous = state
            .projection
            .value
            .as_ref()
            .map(|task| task.scope.clone());
        let old = state.projection.clone();
        if !state.projection.commit(token, result) {
            return false;
        }
        let current = state
            .projection
            .value
            .as_ref()
            .map(|task| task.scope.clone());
        if previous != current {
            Self::retire(&mut state);
        }
        if state.projection != old {
            self.changes.send_replace(state.projection.clone());
        }
        true
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().expect("task pane lock");
        state.projection.cancel();
        Self::retire(&mut state);
        self.changes.send_replace(state.projection.clone());
    }
    /// Loading/missing/error has no RunBinding. A slow history baseline cannot
    /// install after the task resolves to another source or another load starts.
    pub async fn open_resolved(&self) -> Result<Option<Arc<RunBinding>>> {
        self.open_using(None).await
    }
    /// Reuse an already-owned canonical Run for the exact authoritative task
    /// scope. This returns a separately guarded task role, never the main Arc.
    /// A different live presentation cannot replace an existing task mount;
    /// begin a new resolution explicitly before changing its source.
    pub async fn open_resolved_from(&self, target: &RunBinding) -> Result<Option<Arc<RunBinding>>> {
        self.open_using(Some(target)).await
    }
    async fn open_using(&self, target: Option<&RunBinding>) -> Result<Option<Arc<RunBinding>>> {
        let (task, generation, alias) = {
            let state = self.state.lock().expect("task pane lock");
            let Some(task) = state
                .projection
                .value
                .as_ref()
                .filter(|_| state.projection.resolved)
            else {
                return Ok(None);
            };
            tekes_agent_contract::require(
                self.parent_authority.access() != PaneAccess::Missing,
                "task parent pane retired",
            )?;
            let alias =
                if let Some(target) = target {
                    tekes_agent_contract::require(
                        self.uses_run(target) && target.scope() == &task.scope,
                        "task presentation source mismatch",
                    )?;
                    Some(target.presentation_alias().ok_or_else(|| {
                        unsupported("task source has no writable Run presentation")
                    })?)
                } else if self.parent_scope.as_ref() == Some(&task.scope) {
                    Some(self.parent_presentation.clone().ok_or_else(|| {
                        unsupported("task parent has no writable Run presentation")
                    })?)
                } else {
                    None
                };
            if let Some(run) = &state.run {
                tekes_agent_contract::require(
                    alias.as_ref().is_none_or(|alias| alias.matches(run)),
                    "task already owns another canonical presentation",
                )?;
                return Ok(Some(run.clone()));
            }
            (task.clone(), state.source_generation, alias)
        };
        let result = if let Some(alias) = &alias {
            alias.open().await
        } else {
            RunBinding::open_task(self.runtime.clone(), &task).await
        };
        let mut state = self.state.lock().expect("task pane lock");
        if state.source_generation != generation
            || state.projection.value.as_ref().map(|task| &task.scope) != Some(&task.scope)
        {
            return Ok(None);
        }
        tekes_agent_contract::require(
            self.parent_authority.access() != PaneAccess::Missing,
            "task parent pane retired during history read",
        )?;
        if let Some(run) = &state.run {
            tekes_agent_contract::require(
                alias.as_ref().is_none_or(|alias| alias.matches(run)),
                "task already owns another canonical presentation",
            )?;
            return Ok(Some(run.clone()));
        }
        // Fence failed baselines as well: an old source's transport error must
        // not replace the current pane's loading/error presentation.
        let mut binding = result?;
        binding.ancestors.inherit(&self.parent_authority);
        binding.task = Some(TaskRunGuard {
            owner: Arc::downgrade(&self.state),
            generation,
            scope: task.scope,
        });
        let binding = Arc::new(binding);
        state.run = Some(binding.clone());
        Ok(Some(binding))
    }
}
