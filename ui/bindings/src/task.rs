//! Task row resolution retains the existing TaskPanelState generation fences.
use crate::RunBinding;
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
pub(crate) struct TaskRunGuard {
    owner: Weak<Mutex<MountedTask>>,
    generation: u64,
    scope: RunScope,
}
impl TaskRunGuard {
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
    state: Arc<Mutex<MountedTask>>,
    changes: watch::Sender<TaskPanelState>,
}
impl TaskPaneBinding {
    pub fn for_run(parent: &RunBinding) -> Self {
        Self::new(parent.runtime.clone())
    }
    pub fn uses_run(&self, parent: &RunBinding) -> bool {
        Arc::ptr_eq(&self.runtime, &parent.runtime)
    }
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        let projection = TaskPanelState::default();
        Self {
            runtime,
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
        let (task, generation) = {
            let state = self.state.lock().expect("task pane lock");
            if let Some(run) = &state.run {
                return Ok(Some(run.clone()));
            }
            let Some(task) = state
                .projection
                .value
                .as_ref()
                .filter(|_| state.projection.resolved)
            else {
                return Ok(None);
            };
            (task.clone(), state.source_generation)
        };
        let result = RunBinding::open_task(self.runtime.clone(), &task).await;
        let mut state = self.state.lock().expect("task pane lock");
        if state.source_generation != generation
            || state.projection.value.as_ref().map(|task| &task.scope) != Some(&task.scope)
        {
            return Ok(None);
        }
        if let Some(run) = &state.run {
            return Ok(Some(run.clone()));
        }
        // Fence failed baselines as well: an old source's transport error must
        // not replace the current pane's loading/error presentation.
        let mut binding = result?;
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
