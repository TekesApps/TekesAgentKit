use crate::RunScope;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tekes_agent_contract::{Error, ForkRequest, Result, require, unsupported};
use tekes_agent_session::SessionRuntime;
use tokio::sync::watch;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SideChatPhase {
    Ready,
    Closing,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SideChat {
    pub parent: RunScope,
    pub scope: RunScope,
    pub title: String,
    pub phase: SideChatPhase,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SideChatSnapshot {
    pub creating: BTreeSet<String>,
    pub unknown_creations: BTreeMap<String, String>,
    pub chats: BTreeMap<String, SideChat>,
}
impl SideChatSnapshot {
    /// UI consumers do not construct or parse the controller's serialized parent keys.
    pub fn is_creating_for(&self, parent: &RunScope) -> bool {
        self.creating
            .contains(&serde_json::to_string(parent).expect("string-only RunScope"))
    }
    pub fn unknown_creation_for(&self, parent: &RunScope) -> Option<&str> {
        self.unknown_creations
            .get(&serde_json::to_string(parent).expect("string-only RunScope"))
            .map(String::as_str)
    }
}
/// Owns ephemeral pointers, not windows/tabs. Results retain the originating parent even after
/// navigation changes. Callers decide whether to show a tab by comparing that parent.
pub struct SideChatController {
    runtime: Arc<SessionRuntime>,
    state: Arc<SideChatState>,
}
struct SideChatState {
    value: Mutex<SideChatSnapshot>,
    changes: watch::Sender<SideChatSnapshot>,
}
impl SideChatState {
    fn new() -> Self {
        Self {
            value: Mutex::new(SideChatSnapshot::default()),
            changes: watch::channel(SideChatSnapshot::default()).0,
        }
    }
    fn lock(&self) -> std::result::Result<SideChatStateGuard<'_>, ()> {
        let value = self.value.lock().map_err(|_| ())?;
        let before = value.clone();
        Ok(SideChatStateGuard {
            value,
            before,
            changes: &self.changes,
        })
    }
}
struct SideChatStateGuard<'a> {
    value: std::sync::MutexGuard<'a, SideChatSnapshot>,
    before: SideChatSnapshot,
    changes: &'a watch::Sender<SideChatSnapshot>,
}
impl std::ops::Deref for SideChatStateGuard<'_> {
    type Target = SideChatSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl std::ops::DerefMut for SideChatStateGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
impl Drop for SideChatStateGuard<'_> {
    fn drop(&mut self) {
        if *self.value != self.before {
            self.changes.send_replace(self.value.clone());
        }
    }
}
struct MutationGuard {
    state: Arc<SideChatState>,
    key: String,
    creating: bool,
    armed: bool,
}
impl Drop for MutationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut state = self.state.lock().expect("side chat lock");
        if self.creating {
            state.creating.remove(&self.key);
            state.unknown_creations.insert(
                self.key.clone(),
                "creation outcome unknown; reconcile before retrying".into(),
            );
        } else if let Some(chat) = state.chats.get_mut(&self.key) {
            chat.phase = SideChatPhase::Unknown;
            chat.error = Some("close outcome unknown; reconcile before retrying".into());
        }
    }
}
impl SideChatController {
    pub fn new(runtime: Arc<SessionRuntime>) -> Self {
        Self {
            runtime,
            state: Arc::new(SideChatState::new()),
        }
    }
    pub fn snapshot(&self) -> SideChatSnapshot {
        self.state.lock().expect("side chat lock").clone()
    }
    /// A UI mount must observe and control the same authority that owns its pointer.
    /// Matching endpoint names alone does not prove runtime identity.
    pub fn uses_runtime(&self, runtime: &Arc<SessionRuntime>) -> bool {
        Arc::ptr_eq(&self.runtime, runtime)
    }
    /// Canonical local pointer/mutation publications, including cancellation-to-Unknown.
    /// Snapshot reads and failed no-op reconciliations do not wake observers.
    pub fn subscribe(&self) -> watch::Receiver<SideChatSnapshot> {
        self.state.changes.subscribe()
    }
    pub async fn supports(&self) -> Result<bool> {
        let caps = self.runtime.capabilities().await?;
        Ok(caps.supports_ephemeral_fork
            && caps.supports("session.fork")
            && caps.supports("session.discard")
            && caps.supports("session.cancel"))
    }
    pub async fn create(
        &self,
        parent: RunScope,
        title: Option<&str>,
        rpc_id: &str,
    ) -> Result<SideChat> {
        parent.validate()?;
        require(
            self.runtime
                .inspect(|s| s.endpoint_id == parent.endpoint_id),
            "side chat endpoint mismatch",
        )?;
        if !self.supports().await? {
            return Err(unsupported("ephemeral SideChat"));
        }
        let key = serde_json::to_string(&parent)?;
        {
            let mut state = self.state.lock().expect("side chat lock");
            require(
                !state.creating.contains(&key) && !state.unknown_creations.contains_key(&key),
                "creation pending or unknown",
            )?;
            state.creating.insert(key.clone());
        }
        let mut guard = MutationGuard {
            state: self.state.clone(),
            key: key.clone(),
            creating: true,
            armed: true,
        };
        let created = self
            .runtime
            .fork(
                ForkRequest {
                    session_id: parent.session_id.clone(),
                    at_seq: None,
                    entry_id: None,
                    ephemeral: true,
                },
                rpc_id,
            )
            .await;
        let created = match created {
            Ok(created) => created,
            Err(error) => {
                if matches!(
                    error,
                    Error::Remote(_) | Error::Unsupported(_) | Error::Protocol(_)
                ) {
                    self.state
                        .lock()
                        .expect("side chat lock")
                        .creating
                        .remove(&key);
                    guard.armed = false;
                }
                return Err(error);
            }
        };
        let chat = SideChat {
            parent,
            scope: RunScope::main(
                self.runtime.inspect(|s| s.endpoint_id.clone()),
                created.session_id,
            ),
            title: title
                .map(|t| format!("SideChat · {t}"))
                .unwrap_or_else(|| "SideChat".into()),
            phase: SideChatPhase::Ready,
            error: None,
        };
        // Record the successful fork before any optional rename await can be cancelled.
        {
            let mut state = self.state.lock().expect("side chat lock");
            state.creating.remove(&key);
            state
                .chats
                .insert(chat.scope.session_id.clone(), chat.clone());
        }
        guard.armed = false;
        let _ = self
            .runtime
            .rename(
                &chat.scope.session_id,
                &chat.title,
                &format!("{rpc_id}:rename"),
            )
            .await;
        Ok(chat)
    }
    /// Settles a running child before discard. A timeout/error retains the pointer and draft.
    pub async fn close(&self, id: &str, rpc_id: &str) -> Result<()> {
        {
            let mut state = self.state.lock().expect("side chat lock");
            let chat = state
                .chats
                .get_mut(id)
                .ok_or_else(|| unsupported("unknown SideChat"))?;
            require(
                chat.phase == SideChatPhase::Ready,
                "SideChat close pending or unknown",
            )?;
            chat.phase = SideChatPhase::Closing;
            chat.error = None;
        }
        let mut guard = MutationGuard {
            state: self.state.clone(),
            key: id.into(),
            creating: false,
            armed: true,
        };
        let result = self.close_inner(id, rpc_id).await;
        let mut state = self.state.lock().expect("side chat lock");
        match &result {
            Ok(()) => {
                state.chats.remove(id);
            }
            Err(error) => {
                let chat = state.chats.get_mut(id).expect("owned close");
                chat.phase = if matches!(
                    error,
                    Error::Remote(_) | Error::Unsupported(_) | Error::Protocol(_)
                ) {
                    SideChatPhase::Ready
                } else {
                    SideChatPhase::Unknown
                };
                chat.error = Some(error.to_string());
            }
        }
        guard.armed = false;
        result
    }
    async fn close_inner(&self, id: &str, rpc_id: &str) -> Result<()> {
        let _lease = self.runtime.acquire_history(id).await?;
        let running = || {
            self.runtime
                .inspect(|s| s.sessions.get(id).is_some_and(|s| s.run_open))
        };
        // turn/end may precede worker shutdown. Cancel is idempotent and settles that tail.
        self.runtime.cancel(id, &format!("{rpc_id}:cancel")).await?;
        if running() {
            let mut changes = self.runtime.subscribe();
            tokio::time::timeout(Duration::from_secs(5), async {
                while running() {
                    changes.changed().await.map_err(|_| Error::Disconnected)?;
                }
                Ok::<_, Error>(())
            })
            .await
            .map_err(|_| Error::Timeout)??;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match self.runtime.discard(id, rpc_id).await {
                Err(Error::Remote(error))
                    if error["code"] == "session-running"
                        && tokio::time::Instant::now() < deadline =>
                {
                    // Explicit rejection means no discard was performed. Only this known
                    // worker-settlement race is retried; timeouts/disconnects remain Unknown.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                result => return result,
            }
        }
    }
    /// Only an authoritative pointer query may call this. A failed query is not absence.
    /// The expected chat guards a close/reopen ABA race. An unknown mutation must be reconciled
    /// before another mutation; confirmed absence is also the host's draft-cleanup signal.
    pub fn reconcile(
        &self,
        expected: &SideChat,
        exists: std::result::Result<bool, String>,
    ) -> bool {
        let mut state = self.state.lock().expect("side chat lock");
        if expected.phase == SideChatPhase::Closing
            || state.chats.get(&expected.scope.session_id) != Some(expected)
        {
            return false;
        }
        match exists {
            Ok(false) => {
                state.chats.remove(&expected.scope.session_id);
                true
            }
            Ok(true) => {
                if let Some(chat) = state.chats.get_mut(&expected.scope.session_id) {
                    chat.phase = SideChatPhase::Ready;
                    chat.error = None;
                }
                false
            }
            Err(_) => false,
        }
    }
    /// Explicit host reconciliation of an uncertain fork; no timer or automatic retry clears it.
    pub fn reconcile_creation(
        &self,
        parent: &RunScope,
        discovered: Option<SideChat>,
    ) -> Result<()> {
        let key = serde_json::to_string(parent)?;
        let mut state = self.state.lock().expect("side chat lock");
        require(
            state.unknown_creations.contains_key(&key),
            "no unknown creation",
        )?;
        if let Some(chat) = discovered {
            require(
                chat.parent == *parent
                    && chat.scope.endpoint_id == parent.endpoint_id
                    && !chat.scope.session_id.is_empty()
                    && chat.scope.session_id != parent.session_id
                    && chat.scope.thread_id == chat.scope.session_id,
                "foreign SideChat",
            )?;
            state.chats.insert(chat.scope.session_id.clone(), chat);
        }
        state.unknown_creations.remove(&key);
        Ok(())
    }
}
