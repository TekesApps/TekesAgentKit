//! AgentKit-owned durable client preferences, independent of Harness databases.
//! One service owns a file lock; hosts supply only a private path and identity scope.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tekes_agent_contract::*;
use tokio::sync::watch;

#[derive(Clone, Default, Serialize, Deserialize)]
struct Scope {
    pins: Vec<ThreadActionNavigation>,
    receipts: BTreeMap<String, ThreadActionRequest>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Document {
    version: u32,
    scopes: BTreeMap<String, Scope>,
}
pub struct ThreadPreferences {
    path: PathBuf,
    scope: String,
    state: Mutex<Document>,
    changes: watch::Sender<u64>,
    _lock: File,
}
fn io_error(error: std::io::Error) -> Error {
    Error::Transport(format!("thread preferences: {error}"))
}
impl ThreadPreferences {
    pub fn open(path: impl AsRef<Path>, scope: &str) -> Result<Self> {
        require(!scope.is_empty(), "thread preference scope is required")?;
        let path = path.as_ref().to_path_buf();
        require(
            path.is_absolute(),
            "thread preference path must be absolute",
        )?;
        let parent = path
            .parent()
            .ok_or_else(|| unsupported("preference directory"))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let lock_path = path.with_extension("lock");
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(lock_path).map_err(io_error)?;
        lock.try_lock().map_err(|e| {
            Error::Transport(format!("thread preference store already in use: {e}"))
        })?;
        let document: Document = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Document {
                version: 1,
                scopes: BTreeMap::new(),
            },
            Err(e) => return Err(io_error(e)),
        };
        require(
            document.version == 1,
            "unsupported thread preference version",
        )?;
        for value in document.scopes.values() {
            let mut identities = BTreeSet::new();
            for pin in &value.pins {
                require(
                    !pin.endpoint.is_empty()
                        && !pin.session_id.is_empty()
                        && identities.insert((&pin.endpoint, &pin.session_id)),
                    "invalid duplicate thread pin",
                )?;
            }
        }
        Ok(Self {
            path,
            scope: scope.into(),
            state: Mutex::new(document),
            changes: watch::channel(0).0,
            _lock: lock,
        })
    }
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
    pub fn pins(&self) -> Vec<ThreadActionNavigation> {
        self.state
            .lock()
            .expect("preferences lock")
            .scopes
            .get(&self.scope)
            .map(|s| s.pins.clone())
            .unwrap_or_default()
    }
    /// Reconcile an acknowledged local action before checking today's menu state.
    pub fn replay(
        &self,
        endpoint: &str,
        request: &ThreadActionRequest,
        rpc_id: &str,
    ) -> Result<bool> {
        let state = self.state.lock().expect("preferences lock");
        Self::replayed(state.scopes.get(&self.scope), endpoint, request, rpc_id)
    }
    fn replayed(
        scope: Option<&Scope>,
        endpoint: &str,
        request: &ThreadActionRequest,
        rpc_id: &str,
    ) -> Result<bool> {
        let key = serde_json::to_string(&(endpoint, rpc_id))?;
        if let Some(old) = scope.and_then(|s| s.receipts.get(&key)) {
            require(
                old == request,
                "thread action RPC identity reused with different payload",
            )?;
            return Ok(true);
        }
        Ok(false)
    }
    fn commit(&self, document: &Document) -> Result<()> {
        let temp = self
            .path
            .with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp).map_err(io_error)?;
            file.write_all(&serde_json::to_vec(document)?)
                .map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            fs::rename(&temp, &self.path).map_err(io_error)?;
            // Rename is the commit point. Keep memory aligned even if directory sync is unsupported.
            if let Ok(parent) = File::open(self.path.parent().expect("parent")) {
                let _ = parent.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}
#[async_trait]
impl ThreadActionHost for ThreadPreferences {
    fn methods(&self) -> BTreeSet<String> {
        BTreeSet::from(["thread.setPinned".into(), "thread.movePin".into()])
    }
    fn pin_position(&self, endpoint: &str, session: &str) -> Option<ThreadPinPosition> {
        let pins = self.pins();
        let index = pins
            .iter()
            .position(|p| p.endpoint == endpoint && p.session_id == session);
        Some(ThreadPinPosition {
            pinned: index.is_some(),
            can_move_up: index.is_some_and(|i| i > 0),
            can_move_down: index.is_some_and(|i| i + 1 < pins.len()),
        })
    }
    async fn perform(
        &self,
        endpoint: &str,
        request: &ThreadActionRequest,
        rpc_id: &str,
    ) -> Result<ThreadActionOutcome> {
        require(!rpc_id.is_empty(), "thread action RPC identity required")?;
        let mut state = self.state.lock().expect("preferences lock");
        if Self::replayed(state.scopes.get(&self.scope), endpoint, request, rpc_id)? {
            return Ok(ThreadActionOutcome::default());
        }
        let mut next = state.clone();
        let scope = next.scopes.entry(self.scope.clone()).or_default();
        let index = scope
            .pins
            .iter()
            .position(|p| p.endpoint == endpoint && p.session_id == request.session_id);
        match request.action_id.as_str() {
            "pin" => {
                if index.is_none() {
                    scope.pins.push(ThreadActionNavigation {
                        endpoint: endpoint.into(),
                        session_id: request.session_id.clone(),
                    });
                }
            }
            "unpin" => {
                if let Some(i) = index {
                    scope.pins.remove(i);
                }
            }
            "movePinUp" | "movePinDown" => {
                let i = index.ok_or_else(|| unsupported("thread is not pinned"))?;
                let target = if request.action_id == "movePinUp" {
                    i.checked_sub(1)
                } else {
                    (i + 1 < scope.pins.len()).then_some(i + 1)
                }
                .ok_or_else(|| unsupported("pin order boundary"))?;
                scope.pins.swap(i, target);
            }
            _ => return Err(unsupported("unknown local thread action")),
        }
        scope
            .receipts
            .insert(serde_json::to_string(&(endpoint, rpc_id))?, request.clone());
        self.commit(&next)?;
        *state = next;
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(ThreadActionOutcome::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("agentkit-pin-test-{}", uuid::Uuid::new_v4()))
            .join("preferences.json")
    }
    fn request(session: &str, action: &str) -> ThreadActionRequest {
        ThreadActionRequest {
            session_id: session.into(),
            action_id: action.into(),
            title: None,
        }
    }
    #[tokio::test]
    async fn durable_order_scopes_identity_replay_and_notifications() {
        let path = path();
        {
            let store = ThreadPreferences::open(&path, "alice/workspace").unwrap();
            assert!(ThreadPreferences::open(&path, "other").is_err());
            let mut changes = store.subscribe();
            store
                .perform("kernel", &request("same", "pin"), "1")
                .await
                .unwrap();
            changes.changed().await.unwrap();
            store
                .perform("pi", &request("same", "pin"), "2")
                .await
                .unwrap();
            store
                .perform("pi", &request("same", "movePinUp"), "3")
                .await
                .unwrap();
            store
                .perform("pi", &request("same", "movePinUp"), "3")
                .await
                .unwrap();
            assert_eq!(store.pins()[0].endpoint, "pi");
            assert!(!store.pin_position("pi", "same").unwrap().can_move_up);
            assert!(
                store
                    .perform("pi", &request("different", "pin"), "3")
                    .await
                    .is_err()
            );
        }
        {
            let store = ThreadPreferences::open(&path, "alice/workspace").unwrap();
            assert_eq!(
                store
                    .pins()
                    .iter()
                    .map(|p| p.endpoint.as_str())
                    .collect::<Vec<_>>(),
                ["pi", "kernel"]
            );
            assert!(
                store
                    .replay("pi", &request("same", "movePinUp"), "3")
                    .unwrap()
            );
            store
                .perform("kernel", &request("same", "unpin"), "4")
                .await
                .unwrap();
        }
        {
            let store = ThreadPreferences::open(&path, "bob/workspace").unwrap();
            assert!(store.pins().is_empty());
            store
                .perform("dsh", &request("new", "pin"), "1")
                .await
                .unwrap();
        }
        {
            let store = ThreadPreferences::open(&path, "alice/workspace").unwrap();
            assert_eq!(store.pins().len(), 1);
            assert_eq!(store.pins()[0].endpoint, "pi");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn write_failure_does_not_publish_memory_or_receipt_and_corruption_is_not_reset() {
        let path = path();
        let store = ThreadPreferences::open(&path, "scope").unwrap();
        fs::create_dir(&path).unwrap(); // force rename to fail
        assert!(store.perform("k", &request("s", "pin"), "1").await.is_err());
        assert!(store.pins().is_empty());
        assert!(!store.replay("k", &request("s", "pin"), "1").unwrap());
        fs::remove_dir(&path).unwrap();
        drop(store);
        fs::write(&path, b"broken").unwrap();
        assert!(ThreadPreferences::open(&path, "scope").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
