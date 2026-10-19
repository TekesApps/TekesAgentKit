use crate::RunScope;
use serde::{Deserialize, Serialize};
use tekes_agent_contract::{Result, require};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTask {
    pub id: String,
    pub scope: RunScope,
    pub name: String,
    pub status: String,
    pub kind: String,
}
impl AgentTask {
    pub fn independent_run(&self) -> Option<RunScope> {
        (self.scope.session_id != self.scope.thread_id).then(|| self.scope.clone())
    }
    pub fn canonical_status(&self) -> &str {
        match self.status.as_str() {
            "running" | "working" | "in_progress" => "in_progress",
            "done" | "completed" => "completed",
            "failed" | "error" => "failed",
            "interrupted" | "stopped" => "interrupted",
            _ => "pending",
        }
    }
}
/// Generation fences A→B→A as well as cancellation. Tab identity is a task, never a cached pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskLoadToken {
    scope: RunScope,
    task_id: String,
    generation: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskPanelState {
    current: Option<TaskLoadToken>,
    generation: u64,
    pub value: Option<AgentTask>,
    pub resolved: bool,
    pub error: Option<String>,
}
impl TaskPanelState {
    pub fn active_task_id(&self) -> Option<&str> {
        self.current.as_ref().map(|token| token.task_id.as_str())
    }
    pub fn parent_scope(&self) -> Option<&RunScope> {
        self.current.as_ref().map(|token| &token.scope)
    }
    pub fn begin(&mut self, parent: RunScope, task_id: impl Into<String>) -> Result<TaskLoadToken> {
        parent.validate()?;
        let task_id = task_id.into();
        require(!task_id.is_empty(), "empty task id")?;
        self.generation += 1;
        let token = TaskLoadToken {
            scope: parent,
            task_id,
            generation: self.generation,
        };
        self.current = Some(token.clone());
        self.value = None;
        self.resolved = false;
        self.error = None;
        Ok(token)
    }
    pub fn commit(
        &mut self,
        token: &TaskLoadToken,
        result: std::result::Result<Option<AgentTask>, String>,
    ) -> bool {
        if self.current.as_ref() != Some(token) {
            return false;
        }
        match result {
            Ok(value) => {
                if value.as_ref().is_some_and(|v| {
                    v.id != token.task_id
                        || v.scope.endpoint_id != token.scope.endpoint_id
                        || v.scope.thread_id != token.scope.thread_id
                }) {
                    return false;
                }
                self.value = value;
                self.resolved = true;
                self.error = None;
            }
            Err(error) => {
                self.value = None;
                self.error = Some(error);
                self.resolved = false;
            }
        }
        true
    }
    pub fn cancel(&mut self) {
        self.current = None;
        self.value = None;
        self.resolved = false;
        self.error = None;
    }
}
