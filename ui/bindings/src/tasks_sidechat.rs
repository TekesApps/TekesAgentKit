//! UI bindings for queued prompts / background tasks (`TasksQueueContainer`),
//! read-only bounded `ChildRun` traces, and isolated-draft `SideChat` panels.

use crate::UiWakeNotifier;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tekes_agent_contract::{PromptMode, Result};
use tekes_agent_presentation::{
    ComposerDestination, RunController, RunScope, RunSnapshot, SideChat, SideChatController,
    SideChatPhase, SummaryFacts,
};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tokio::runtime::Handle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedPromptItem {
    pub item_id: String,
    pub session_id: String,
    pub text: String,
    pub connection_epoch: u64,
    pub can_mutate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskRowEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub canonical_status: String,
    pub kind: String,
    pub parent_scope: Option<RunScope>,
    pub child_session_id: Option<String>,
}

fn extract_queue_item_text(item: &Value) -> String {
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        return text.to_owned();
    }
    if let Some(parts) = item.get("content").and_then(Value::as_array) {
        let joined: Vec<&str> = parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect();
        if !joined.is_empty() {
            return joined.join(" ");
        }
    }
    item.get("label")
        .and_then(Value::as_str)
        .unwrap_or("(排队消息)")
        .to_owned()
}

pub fn project_queued_prompts(snapshot: &RunSnapshot) -> Vec<QueuedPromptItem> {
    let Some(run) = snapshot.run.as_ref() else {
        return Vec::new();
    };
    let can_mutate = run.can_update_queue
        && matches!(
            snapshot.connection.phase,
            tekes_agent_presentation::ConnectionPhase::Connected
        );
    run.queue
        .iter()
        .enumerate()
        .map(|(idx, raw)| QueuedPromptItem {
            item_id: raw["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("queue-{idx}")),
            session_id: run.scope.session_id.clone(),
            text: extract_queue_item_text(raw),
            connection_epoch: run.connection_epoch,
            can_mutate,
        })
        .collect()
}

fn canonicalize_task_status(raw: &str) -> String {
    match raw {
        "running" | "working" | "in_progress" => "in_progress".to_owned(),
        "done" | "completed" => "completed".to_owned(),
        "failed" | "error" => "failed".to_owned(),
        "interrupted" | "stopped" => "interrupted".to_owned(),
        _ => "pending".to_owned(),
    }
}

pub fn project_tasks(
    snapshot: &RunSnapshot,
    summary_facts: Option<&SummaryFacts>,
) -> Vec<TaskRowEntry> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let parent_scope = snapshot.run.as_ref().map(|r| r.scope.clone());

    if let Some(run) = snapshot.run.as_ref() {
        for (idx, job) in run.jobs.iter().enumerate() {
            let id = job["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("job-{idx}"));
            let child_session_id = job["childSessionId"]
                .as_str()
                .or_else(|| {
                    job["sessionId"]
                        .as_str()
                        .filter(|sid| *sid != run.scope.session_id)
                })
                .map(str::to_owned);
            let title = job["name"]
                .as_str()
                .or_else(|| job["title"].as_str())
                .unwrap_or(&id)
                .to_owned();
            let status = job["status"].as_str().unwrap_or("pending").to_owned();
            let kind = job["kind"].as_str().unwrap_or("job").to_owned();
            seen.insert(id.clone());
            rows.push(TaskRowEntry {
                id,
                title,
                canonical_status: canonicalize_task_status(&status),
                status,
                kind,
                parent_scope: Some(run.scope.clone()),
                child_session_id,
            });
        }
    }

    if let Some(facts) = summary_facts {
        for sub in &facts.subagents {
            let child_session_id = sub.session_id.clone();
            if child_session_id.as_ref().is_some_and(|cid| {
                rows.iter()
                    .any(|r| r.child_session_id.as_deref() == Some(cid))
            }) {
                continue;
            }
            if !seen.insert(sub.id.clone()) {
                continue;
            }
            let status = if sub.status.is_empty() {
                "pending".to_owned()
            } else {
                sub.status.clone()
            };
            rows.push(TaskRowEntry {
                id: sub.id.clone(),
                title: sub.title.clone(),
                canonical_status: canonicalize_task_status(&status),
                status,
                kind: "subagent".to_owned(),
                parent_scope: Some(facts.scope.clone()),
                child_session_id,
            });
        }
        for task in &facts.tasks {
            if !seen.insert(task.id.clone()) {
                continue;
            }
            rows.push(TaskRowEntry {
                id: task.id.clone(),
                title: task.name.clone(),
                canonical_status: task.canonical_status().to_owned(),
                status: task.status.clone(),
                kind: task.kind.clone(),
                parent_scope: parent_scope.clone(),
                child_session_id: task.independent_run().map(|s| s.session_id),
            });
        }
    }

    rows
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChildRunStatus {
    Loading,
    Ready,
    Missing(String),
}

#[derive(Clone, Debug)]
pub struct ChildRunViewSnapshot {
    pub parent_scope: RunScope,
    pub child_scope: RunScope,
    pub status: ChildRunStatus,
    pub snapshot: Arc<RunSnapshot>,
    pub can_load_earlier: bool,
    pub revision: u64,
}

struct ChildRunInternal {
    opened_once: bool,
    missing_reason: Option<String>,
    loading: bool,
    history_lease: Option<tekes_agent_session::SessionHistoryLease>,
    revision: u64,
}

/// Strictly read-only binding for bounded/independent child & subagent execution traces.
/// Deliberately omits composer draft mutation or message submission APIs.
pub struct ChildRunBinding {
    runtime: Arc<SessionRuntime>,
    parent_scope: RunScope,
    child_scope: RunScope,
    controller: Arc<RunController>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    internal: Arc<Mutex<ChildRunInternal>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl Drop for ChildRunBinding {
    fn drop(&mut self) {
        self._watcher.abort();
        if let Ok(mut guard) = self.internal.lock() {
            guard.history_lease.take();
        }
    }
}

impl std::fmt::Debug for ChildRunBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildRunBinding")
            .field("child_session_id", &self.child_scope.session_id)
            .finish_non_exhaustive()
    }
}

impl ChildRunBinding {
    pub fn open_for_child(
        runtime: Arc<SessionRuntime>,
        parent_scope: RunScope,
        child_session_id: impl Into<String>,
        handle: Handle,
        wake: Arc<dyn UiWakeNotifier>,
    ) -> Result<Arc<Self>> {
        let child_id = child_session_id.into();
        let _guard = handle.enter();
        let controller = Arc::new(RunController::for_child(
            Arc::clone(&runtime),
            &parent_scope,
            child_id.clone(),
        )?);
        drop(_guard);
        let child_scope = RunScope {
            endpoint_id: parent_scope.endpoint_id.clone(),
            thread_id: parent_scope.thread_id.clone(),
            session_id: child_id,
        };
        let internal = Arc::new(Mutex::new(ChildRunInternal {
            opened_once: false,
            missing_reason: None,
            loading: true,
            history_lease: None,
            revision: 1,
        }));

        let mut run_rx = controller.subscribe();
        let mut store_rx = runtime.subscribe();
        let runtime_w = Arc::clone(&runtime);
        let internal_w = Arc::clone(&internal);
        let wake_w = Arc::clone(&wake);
        let sid_w = child_scope.session_id.clone();
        let watcher = handle.spawn(async move {
            loop {
                tokio::select! {
                    res = run_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                        Self::check_store_presence(&runtime_w, &sid_w, &internal_w);
                        wake_w.wake();
                    }
                    res = store_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                        Self::check_store_presence(&runtime_w, &sid_w, &internal_w);
                        wake_w.wake();
                    }
                }
            }
        });

        let binding = Arc::new(Self {
            runtime,
            parent_scope,
            child_scope,
            controller,
            handle,
            wake,
            internal,
            _watcher: watcher,
        });
        Ok(binding)
    }

    fn check_store_presence(
        runtime: &SessionRuntime,
        child_session_id: &str,
        internal: &Mutex<ChildRunInternal>,
    ) {
        let missing = runtime.inspect(|store| {
            store
                .sessions
                .get(child_session_id)
                .is_none_or(|s| s.summary.is_none())
        });
        let mut guard = internal.lock().expect("child run internal lock");
        if guard.opened_once && missing && guard.missing_reason.is_none() {
            guard.missing_reason = Some(format!(
                "子代理会话 {child_session_id} 已过期或不存在 (Missing)"
            ));
            guard.revision = guard.revision.saturating_add(1);
        }
    }

    pub fn child_session_id(&self) -> &str {
        &self.child_scope.session_id
    }

    pub fn refresh(&self) {
        let sid = self.child_scope.session_id.clone();
        let runtime = Arc::clone(&self.runtime);
        let scope = self.child_scope.clone();
        let ctrl = Arc::clone(&self.controller);
        let internal = Arc::clone(&self.internal);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let res = async {
                let lease = runtime.acquire_history(&sid).await?;
                let _ = ctrl.refresh_child_catalog(&scope).await;
                Ok::<tekes_agent_session::SessionHistoryLease, tekes_agent_contract::Error>(lease)
            }
            .await;
            let old_lease = {
                let mut guard = internal.lock().expect("child run lock");
                guard.loading = false;
                let mut old = None;
                match res {
                    Ok(lease) => {
                        old = guard.history_lease.replace(lease);
                        guard.opened_once = true;
                        guard.missing_reason = None;
                    }
                    Err(err) => {
                        guard.missing_reason = Some(format!("子代理轨迹不可用或已过期: {err}"));
                    }
                }
                guard.revision = guard.revision.saturating_add(1);
                old
            };
            drop(old_lease);
            Self::check_store_presence(&runtime, &sid, &internal);
            wake.wake();
        });
    }

    pub fn refresh_blocking(&self) -> Result<()> {
        let sid = self.child_scope.session_id.clone();
        let runtime = Arc::clone(&self.runtime);
        let scope = self.child_scope.clone();
        let ctrl = Arc::clone(&self.controller);
        let before_rev = ctrl.snapshot().revision;
        let mut rx = ctrl.subscribe();
        let res = self.handle.block_on(async move {
            let lease = runtime.acquire_history(&sid).await?;
            // Catalog enrichment is best-effort for harnesses implementing subagents.catalog.
            let _ = ctrl.refresh_child_catalog(&scope).await;
            if ctrl.snapshot().revision == before_rev || ctrl.snapshot().transcript.turns.is_empty()
            {
                let _ =
                    tokio::time::timeout(std::time::Duration::from_millis(250), rx.changed()).await;
            }
            Ok::<tekes_agent_session::SessionHistoryLease, tekes_agent_contract::Error>(lease)
        });
        let (out_res, old_lease) = {
            let mut guard = self.internal.lock().expect("child run lock");
            guard.loading = false;
            let mut old = None;
            let out = match res {
                Ok(lease) => {
                    old = guard.history_lease.replace(lease);
                    guard.opened_once = true;
                    guard.missing_reason = None;
                    Ok(())
                }
                Err(err) => {
                    guard.missing_reason = Some(format!("子代理轨迹不可用或已过期: {err}"));
                    Err(err)
                }
            };
            guard.revision = guard.revision.saturating_add(1);
            (out, old)
        };
        drop(old_lease);
        Self::check_store_presence(&self.runtime, &self.child_scope.session_id, &self.internal);
        self.wake.wake();
        out_res
    }

    pub fn load_earlier_blocking(&self) -> Result<bool> {
        let ctrl = Arc::clone(&self.controller);
        let scope = self.child_scope.clone();
        let before_turns = ctrl.snapshot().transcript.turns.len();
        let mut rx = ctrl.subscribe();
        let res = self.handle.block_on(async move {
            let loaded = ctrl.load_earlier(&scope).await?;
            if loaded && ctrl.snapshot().transcript.turns.len() <= before_turns {
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
                while ctrl.snapshot().transcript.turns.len() <= before_turns
                    && tokio::time::Instant::now() < deadline
                {
                    if tokio::time::timeout(std::time::Duration::from_millis(100), rx.changed())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Ok(loaded)
        });
        match &res {
            Ok(_) => {
                self.internal.lock().expect("child run lock").revision += 1;
            }
            Err(err) => {
                let mut guard = self.internal.lock().expect("child run lock");
                guard.missing_reason = Some(format!("加载历史失败: {err}"));
                guard.revision += 1;
            }
        }
        self.wake.wake();
        res
    }

    pub fn load_earlier(&self) {
        let ctrl = Arc::clone(&self.controller);
        let scope = self.child_scope.clone();
        let internal = Arc::clone(&self.internal);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            match ctrl.load_earlier(&scope).await {
                Ok(_) => {
                    internal.lock().expect("child run lock").revision += 1;
                }
                Err(err) => {
                    let mut guard = internal.lock().expect("child run lock");
                    guard.missing_reason = Some(format!("加载历史失败: {err}"));
                    guard.revision += 1;
                }
            }
            wake.wake();
        });
    }

    pub fn mark_missing(&self, reason: impl Into<String>) {
        let mut guard = self.internal.lock().expect("child run lock");
        guard.loading = false;
        guard.missing_reason = Some(reason.into());
        guard.revision = guard.revision.saturating_add(1);
        drop(guard);
        self.wake.wake();
    }

    pub fn view_snapshot(&self) -> ChildRunViewSnapshot {
        Self::check_store_presence(&self.runtime, &self.child_scope.session_id, &self.internal);
        let snap = self.controller.snapshot();
        let guard = self.internal.lock().expect("child run lock");
        let status = if let Some(msg) = &guard.missing_reason {
            ChildRunStatus::Missing(msg.clone())
        } else if guard.loading && snap.transcript.turns.is_empty() {
            ChildRunStatus::Loading
        } else {
            ChildRunStatus::Ready
        };
        let can_load_earlier =
            matches!(status, ChildRunStatus::Ready) && snap.transcript.has_more_before;
        let revision = snap.revision.saturating_add(guard.revision);
        ChildRunViewSnapshot {
            parent_scope: self.parent_scope.clone(),
            child_scope: self.child_scope.clone(),
            status,
            snapshot: snap,
            can_load_earlier,
            revision,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideChatAccess {
    Available,
    Creating,
    Closing,
    Unsupported,
    Disconnected,
    Unknown(String),
    Closed,
}

#[derive(Clone, Debug)]
pub struct SideChatViewSnapshot {
    pub access: SideChatAccess,
    pub chat: Option<SideChat>,
    pub run_snapshot: Option<Arc<RunSnapshot>>,
    pub error: Option<String>,
    pub revision: u64,
}

struct SideChatBindingState {
    parent_scope: Option<RunScope>,
    active_chat: Option<SideChat>,
    chat_controller: Option<Arc<RunController>>,
    closed: bool,
    last_error: Option<String>,
    revision: u64,
}

static NEXT_SIDECHAT_RPC: AtomicU64 = AtomicU64::new(1);

/// Binding owning a `SideChatController` pointer + isolated per-branch `RunController`
/// so drafting or IME preedit in SideChat never overwrites the parent `RunWorkbenchBinding` draft.
pub struct SideChatBinding {
    runtime: Arc<SessionRuntime>,
    sidechat: Arc<SideChatController>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    state: Arc<Mutex<SideChatBindingState>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl Drop for SideChatBinding {
    fn drop(&mut self) {
        self._watcher.abort();
    }
}

impl std::fmt::Debug for SideChatBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SideChatBinding").finish_non_exhaustive()
    }
}

impl SideChatBinding {
    pub fn new(
        runtime: Arc<SessionRuntime>,
        handle: Handle,
        wake: Arc<dyn UiWakeNotifier>,
    ) -> Arc<Self> {
        let sidechat = Arc::new(SideChatController::new(Arc::clone(&runtime)));
        let state = Arc::new(Mutex::new(SideChatBindingState {
            parent_scope: None,
            active_chat: None,
            chat_controller: None,
            closed: false,
            last_error: None,
            revision: 1,
        }));
        let mut side_rx = sidechat.subscribe();
        let mut store_rx = runtime.subscribe();
        let wake_w = Arc::clone(&wake);
        let state_w = Arc::clone(&state);
        let watcher = handle.spawn(async move {
            loop {
                tokio::select! {
                    res = side_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                        state_w.lock().expect("sidechat lock").revision += 1;
                        wake_w.wake();
                    }
                    res = store_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                        state_w.lock().expect("sidechat lock").revision += 1;
                        wake_w.wake();
                    }
                }
            }
        });
        Arc::new(Self {
            runtime,
            sidechat,
            handle,
            wake,
            state,
            _watcher: watcher,
        })
    }

    pub fn active_scope(&self) -> Option<RunScope> {
        self.state
            .lock()
            .expect("sidechat lock")
            .active_chat
            .as_ref()
            .map(|c| c.scope.clone())
    }

    pub fn active_destination(&self) -> Option<ComposerDestination> {
        self.active_scope().map(ComposerDestination::run)
    }

    pub fn sidechat_controller(&self) -> &Arc<SideChatController> {
        &self.sidechat
    }

    fn attach_opened_chat(&self, parent: RunScope, chat: SideChat, ctrl: Arc<RunController>) {
        let mut guard = self.state.lock().expect("sidechat lock");
        guard.parent_scope = Some(parent);
        guard.active_chat = Some(chat);
        guard.chat_controller = Some(ctrl);
        guard.closed = false;
        guard.last_error = None;
        guard.revision = guard.revision.saturating_add(1);
    }

    pub fn create_for_parent(&self, parent: RunScope, title: Option<&str>) {
        self.state.lock().expect("sidechat lock").parent_scope = Some(parent.clone());
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let rpc_id = format!(
            "sidechat-create-{}",
            NEXT_SIDECHAT_RPC.fetch_add(1, Ordering::Relaxed)
        );
        let sidechat = Arc::clone(&self.sidechat);
        let runtime = Arc::clone(&self.runtime);
        let title_owned = title.map(str::to_owned);
        let state = Arc::clone(&self.state);
        let wake = Arc::clone(&self.wake);
        self.handle.spawn(async move {
            let res: Result<(SideChat, Arc<RunController>)> = async {
                let chat = sidechat
                    .create_at_epoch(parent.clone(), title_owned.as_deref(), epoch, &rpc_id)
                    .await?;
                let ctrl = Arc::new(RunController::new(Arc::clone(&runtime))?);
                ctrl.open(chat.scope.clone()).await?;
                ctrl.select(ComposerDestination::run(chat.scope.clone()))?;
                Ok((chat, ctrl))
            }
            .await;
            let mut guard = state.lock().expect("sidechat lock");
            match res {
                Ok((chat, ctrl)) => {
                    guard.active_chat = Some(chat);
                    guard.chat_controller = Some(ctrl);
                    guard.closed = false;
                    guard.last_error = None;
                }
                Err(err) => {
                    guard.last_error = Some(err.to_string());
                }
            }
            guard.revision = guard.revision.saturating_add(1);
            drop(guard);
            wake.wake();
        });
    }

    pub fn create_for_parent_blocking(
        &self,
        parent: RunScope,
        title: Option<&str>,
    ) -> Result<SideChat> {
        self.state.lock().expect("sidechat lock").parent_scope = Some(parent.clone());
        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let rpc_id = format!(
            "sidechat-create-{}",
            NEXT_SIDECHAT_RPC.fetch_add(1, Ordering::Relaxed)
        );
        let sidechat = Arc::clone(&self.sidechat);
        let runtime = Arc::clone(&self.runtime);
        let title_owned = title.map(str::to_owned);
        let handle_ref = self.handle.clone();
        let res: Result<(SideChat, Arc<RunController>)> = self.handle.block_on(async move {
            let chat = sidechat
                .create_at_epoch(parent.clone(), title_owned.as_deref(), epoch, &rpc_id)
                .await?;
            let _guard = handle_ref.enter();
            let ctrl = Arc::new(RunController::new(Arc::clone(&runtime))?);
            ctrl.open(chat.scope.clone()).await?;
            ctrl.select(ComposerDestination::run(chat.scope.clone()))?;
            Ok((chat, ctrl))
        });
        match res {
            Ok((chat, ctrl)) => {
                let ret = chat.clone();
                let parent_saved = {
                    self.state
                        .lock()
                        .expect("sidechat lock")
                        .parent_scope
                        .clone()
                        .unwrap_or_else(|| chat.parent.clone())
                };
                self.attach_opened_chat(parent_saved, chat, ctrl);
                self.wake.wake();
                Ok(ret)
            }
            Err(err) => {
                let mut guard = self.state.lock().expect("sidechat lock");
                guard.last_error = Some(err.to_string());
                guard.revision = guard.revision.saturating_add(1);
                drop(guard);
                self.wake.wake();
                Err(err)
            }
        }
    }

    pub fn set_draft_text(&self, text: String) {
        let (ctrl_opt, scope_opt) = {
            let guard = self.state.lock().expect("sidechat lock");
            (
                guard.chat_controller.clone(),
                guard.active_chat.as_ref().map(|c| c.scope.clone()),
            )
        };
        if let (Some(ctrl), Some(scope)) = (ctrl_opt, scope_opt) {
            let _ = ctrl.edit_text(&ComposerDestination::run(scope), text);
            let mut guard = self.state.lock().expect("sidechat lock");
            guard.revision = guard.revision.saturating_add(1);
            drop(guard);
            self.wake.wake();
        }
    }

    pub fn send_prompt(&self) -> Result<()> {
        let (ctrl, scope) = {
            let guard = self.state.lock().expect("sidechat lock");
            (
                guard.chat_controller.clone(),
                guard.active_chat.as_ref().map(|c| c.scope.clone()),
            )
        };
        let Some(ctrl) = ctrl else {
            return Err(tekes_agent_contract::unsupported("SideChat 尚未就绪"));
        };
        let Some(scope) = scope else {
            return Err(tekes_agent_contract::unsupported("SideChat 目标作用域缺失"));
        };
        let dest = ComposerDestination::run(scope);
        let ticket = ctrl.prepare(&dest, PromptMode::Queue)?;
        let wake = Arc::clone(&self.wake);
        let state = Arc::clone(&self.state);
        self.handle.spawn(async move {
            if let Err(err) = ctrl.dispatch(ticket, None).await {
                let mut guard = state.lock().expect("sidechat lock");
                guard.last_error = Some(err.to_string());
                guard.revision += 1;
            }
            wake.wake();
        });
        Ok(())
    }

    pub fn send_prompt_blocking(&self) -> Result<()> {
        let (ctrl, scope) = {
            let guard = self.state.lock().expect("sidechat lock");
            (
                guard.chat_controller.clone(),
                guard.active_chat.as_ref().map(|c| c.scope.clone()),
            )
        };
        let Some(ctrl) = ctrl else {
            return Err(tekes_agent_contract::unsupported("SideChat 尚未就绪"));
        };
        let Some(scope) = scope else {
            return Err(tekes_agent_contract::unsupported("SideChat 目标作用域缺失"));
        };
        let dest = ComposerDestination::run(scope);
        let ticket = ctrl.prepare(&dest, PromptMode::Queue)?;
        let res = self.handle.block_on(ctrl.dispatch(ticket, None));
        let mut guard = self.state.lock().expect("sidechat lock");
        if let Err(err) = &res {
            guard.last_error = Some(err.to_string());
        }
        guard.revision = guard.revision.saturating_add(1);
        drop(guard);
        self.wake.wake();
        res.map(|_| ())
    }

    pub fn close_owned_blocking(&self) -> Result<()> {
        let expected = {
            let guard = self.state.lock().expect("sidechat lock");
            guard
                .active_chat
                .clone()
                .ok_or_else(|| tekes_agent_contract::unsupported("没有打开的 SideChat"))?
        };
        let rpc_id = format!(
            "sidechat-close-{}",
            NEXT_SIDECHAT_RPC.fetch_add(1, Ordering::Relaxed)
        );
        let sidechat = Arc::clone(&self.sidechat);
        let expected_clone = expected.clone();
        let res = self.handle.block_on(async move {
            sidechat
                .close_owned_at_epoch(&expected_clone, expected_clone.connection_epoch, &rpc_id)
                .await
        });
        {
            let mut guard = self.state.lock().expect("sidechat lock");
            match &res {
                Ok(()) => {
                    guard.active_chat = None;
                    guard.chat_controller = None;
                    guard.closed = true;
                    guard.last_error = None;
                }
                Err(err) => {
                    guard.last_error = Some(err.to_string());
                }
            }
            guard.revision = guard.revision.saturating_add(1);
        }
        self.wake.wake();
        res
    }

    pub fn close_owned(&self) {
        let expected = {
            let guard = self.state.lock().expect("sidechat lock");
            guard.active_chat.clone()
        };
        let Some(expected) = expected else {
            return;
        };
        let sidechat = Arc::clone(&self.sidechat);
        let state = Arc::clone(&self.state);
        let wake = Arc::clone(&self.wake);
        let rpc_id = format!(
            "sidechat-close-{}",
            NEXT_SIDECHAT_RPC.fetch_add(1, Ordering::Relaxed)
        );
        self.handle.spawn(async move {
            let res = sidechat
                .close_owned_at_epoch(&expected, expected.connection_epoch, &rpc_id)
                .await;
            {
                let mut guard = state.lock().expect("sidechat lock");
                match res {
                    Ok(()) => {
                        guard.active_chat = None;
                        guard.chat_controller = None;
                        guard.closed = true;
                        guard.last_error = None;
                    }
                    Err(err) => {
                        guard.last_error = Some(err.to_string());
                    }
                }
                guard.revision += 1;
            }
            wake.wake();
        });
    }

    pub fn view_snapshot(&self) -> SideChatViewSnapshot {
        let side_snap = self.sidechat.snapshot();
        let support = self.sidechat.support_snapshot();
        let connected = self
            .runtime
            .inspect(|s| s.connection == ConnectionState::Ready);
        let guard = self.state.lock().expect("sidechat lock");
        let active_chat = guard.active_chat.as_ref().map(|c| {
            side_snap
                .chats
                .get(&c.scope.session_id)
                .cloned()
                .unwrap_or_else(|| c.clone())
        });
        let run_snapshot = guard.chat_controller.as_ref().map(|c| c.snapshot());
        let creating = guard
            .parent_scope
            .as_ref()
            .is_some_and(|p| side_snap.is_creating_for(p));
        let unknown_creation = guard
            .parent_scope
            .as_ref()
            .and_then(|p| side_snap.unknown_creation_for(p).map(str::to_owned));

        let access = if guard.closed && active_chat.is_none() {
            SideChatAccess::Closed
        } else if !connected {
            SideChatAccess::Disconnected
        } else if creating {
            SideChatAccess::Creating
        } else if let Some(msg) = unknown_creation {
            SideChatAccess::Unknown(msg)
        } else if let Some(chat) = &active_chat {
            match chat.phase {
                SideChatPhase::Closing => SideChatAccess::Closing,
                SideChatPhase::Unknown => SideChatAccess::Unknown(
                    chat.error
                        .clone()
                        .unwrap_or_else(|| "SideChat 状态未知，请先对账".to_owned()),
                ),
                SideChatPhase::Ready => match support {
                    Some(true) => SideChatAccess::Available,
                    Some(false) => SideChatAccess::Unsupported,
                    None => SideChatAccess::Disconnected,
                },
            }
        } else {
            match support {
                Some(true) => SideChatAccess::Available,
                Some(false) => SideChatAccess::Unsupported,
                None => SideChatAccess::Disconnected,
            }
        };

        let rev = guard
            .revision
            .saturating_add(run_snapshot.as_ref().map_or(0, |s| s.revision));
        SideChatViewSnapshot {
            access,
            chat: active_chat,
            run_snapshot,
            error: guard.last_error.clone(),
            revision: rev,
        }
    }
}
