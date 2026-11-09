//! AgentKit-owned durable client preferences, independent of Harness databases.
//! One service owns a file lock; hosts supply only a private path and identity scope.
use crate::{SidebarPreferenceChange, SidebarPreferencesSnapshot};
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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PinIdentity {
    endpoint: String,
    session_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Scope {
    pins: BTreeSet<PinIdentity>,
    #[serde(default, rename = "applicationArchived")]
    application_archived: BTreeSet<PinIdentity>,
    #[serde(
        default = "initial_archive_revision",
        rename = "applicationArchiveRevision"
    )]
    application_archive_revision: u64,
    #[serde(default)]
    sidebar: SidebarPreferencesSnapshot,
    receipts: BTreeMap<String, ThreadActionRequest>,
}
fn initial_archive_revision() -> u64 {
    1
}
impl Default for Scope {
    fn default() -> Self {
        Self {
            pins: BTreeSet::new(),
            application_archived: BTreeSet::new(),
            application_archive_revision: 1,
            sidebar: Default::default(),
            receipts: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationArchiveSnapshot {
    pub revision: u64,
    pub session_ids: BTreeSet<String>,
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
        let mut document: Document = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Document {
                version: 5,
                scopes: BTreeMap::new(),
            },
            Err(e) => return Err(io_error(e)),
        };
        require(
            matches!(document.version, 1..=5),
            "unsupported thread preference version",
        )?;
        for value in document.scopes.values() {
            require(
                value.sidebar.revision > 0,
                "invalid Sidebar preference revision",
            )?;
            value.sidebar.preferences.validate()?;
            require(
                value.application_archive_revision > 0,
                "invalid application archive revision",
            )?;
            for identity in value.pins.iter().chain(&value.application_archived) {
                require(
                    !identity.endpoint.is_empty() && !identity.session_id.is_empty(),
                    "invalid thread preference identity",
                )?;
            }
        }
        let migrate = document.version != 5;
        let had_application_archive = document.version == 5;
        document.version = 5;
        for scope in document.scopes.values_mut() {
            scope.receipts.retain(|_, r| {
                matches!(r.action_id.as_str(), "pin" | "unpin")
                    || had_application_archive
                        && matches!(r.action_id.as_str(), "archive" | "unarchive")
            });
        }
        let store = Self {
            path,
            scope: scope.into(),
            state: Mutex::new(document),
            changes: watch::channel(0).0,
            _lock: lock,
        };
        if migrate {
            store.commit(&store.state.lock().expect("preferences lock"))?;
        }
        Ok(store)
    }
    pub fn sidebar_preferences(&self) -> SidebarPreferencesSnapshot {
        self.state
            .lock()
            .expect("preferences lock")
            .scopes
            .get(&self.scope)
            .map(|scope| scope.sidebar.clone())
            .unwrap_or_default()
    }
    /// Compare the original durable revision under the same store lock. A stale
    /// callback (including ABA) never overwrites newer layout preferences.
    pub fn set_sidebar_preferences(&self, change: &SidebarPreferenceChange) -> Result<()> {
        change.value.validate()?;
        let mut state = self.state.lock().expect("preferences lock");
        let current = state
            .scopes
            .get(&self.scope)
            .map(|scope| scope.sidebar.clone())
            .unwrap_or_default();
        require(
            change.expected == current,
            "stale Sidebar preference callback",
        )?;
        if change.value == current.preferences {
            return Ok(());
        }
        let revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| unsupported("Sidebar preference revision exhausted"))?;
        let mut next = state.clone();
        next.scopes.entry(self.scope.clone()).or_default().sidebar = SidebarPreferencesSnapshot {
            revision,
            preferences: change.value.clone(),
        };
        self.commit(&next)?;
        *state = next;
        self.changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(())
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
            .map(|s| {
                s.pins
                    .iter()
                    .map(|p| ThreadActionNavigation {
                        endpoint: p.endpoint.clone(),
                        session_id: p.session_id.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// One atomic organization snapshot; no Harness title, history or execution data is stored.
    pub fn application_archive_snapshot(&self, endpoint: &str) -> ApplicationArchiveSnapshot {
        let state = self.state.lock().expect("preferences lock");
        let scope = state.scopes.get(&self.scope);
        ApplicationArchiveSnapshot {
            revision: scope.map(|s| s.application_archive_revision).unwrap_or(1),
            session_ids: scope
                .into_iter()
                .flat_map(|s| &s.application_archived)
                .filter(|identity| identity.endpoint == endpoint)
                .map(|identity| identity.session_id.clone())
                .collect(),
        }
    }
    pub fn application_archive_state(
        &self,
        endpoint: &str,
        session: &str,
    ) -> Option<ApplicationArchiveState> {
        Some(ApplicationArchiveState {
            archived: self
                .application_archive_snapshot(endpoint)
                .session_ids
                .contains(session),
        })
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
        BTreeSet::from(["thread.setPinned".into(), "thread.setArchived".into()])
    }
    fn pin_state(&self, endpoint: &str, session: &str) -> Option<ThreadPinState> {
        let pins = self.pins();
        let index = pins
            .iter()
            .position(|p| p.endpoint == endpoint && p.session_id == session);
        Some(ThreadPinState {
            pinned: index.is_some(),
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
        require(
            !endpoint.is_empty() && !request.session_id.is_empty(),
            "local thread identity is required",
        )?;
        let mut next = state.clone();
        let scope = next.scopes.entry(self.scope.clone()).or_default();
        let identity = PinIdentity {
            endpoint: endpoint.into(),
            session_id: request.session_id.clone(),
        };
        match request.action_id.as_str() {
            "pin" => {
                scope.pins.insert(identity);
            }
            "unpin" => {
                scope.pins.remove(&identity);
            }
            "archive" | "unarchive" => {
                let changed = if request.action_id == "archive" {
                    scope.application_archived.insert(identity)
                } else {
                    scope.application_archived.remove(&identity)
                };
                if changed {
                    scope.application_archive_revision = scope
                        .application_archive_revision
                        .checked_add(1)
                        .ok_or_else(|| {
                            Error::Protocol("application archive revision exhausted".into())
                        })?;
                }
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
    async fn durable_relations_scopes_identity_replay_and_notifications() {
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
                .perform("pi", &request("same", "pin"), "2")
                .await
                .unwrap();
            assert!(store.pin_state("pi", "same").unwrap().pinned);
            assert!(
                store
                    .perform("pi", &request("same", "movePinUp"), "3")
                    .await
                    .is_err()
            );
            assert!(
                store
                    .perform("pi", &request("different", "pin"), "2")
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
                ["kernel", "pi"]
            );
            assert!(store.replay("pi", &request("same", "pin"), "2").unwrap());
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
    #[test]
    fn legacy_order_migrates_to_relations_without_losing_pins() {
        let path = path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"version":1,"scopes":{"s":{"pins":[{"endpoint":"z","sessionId":"same"},{"endpoint":"a","sessionId":"same"}],"receipts":{"old":{"sessionId":"same","actionId":"movePinUp"}}}}}"#).unwrap();
        let store = ThreadPreferences::open(&path, "s").unwrap();
        assert!(store.pin_state("z", "same").unwrap().pinned);
        assert!(store.pin_state("a", "same").unwrap().pinned);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["version"], 5);
        assert_eq!(value["scopes"]["s"]["receipts"], serde_json::json!({}));
        assert!(
            value["scopes"]["s"]["pins"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p.get("order").is_none())
        );
        drop(store);
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
        let archive_before = store.application_archive_snapshot("k");
        assert!(
            store
                .perform("k", &request("s", "archive"), "archive-failed")
                .await
                .is_err()
        );
        assert!(
            store
                .application_archive_snapshot("k")
                .session_ids
                .is_empty()
        );
        assert_eq!(
            store.application_archive_snapshot("k").revision,
            archive_before.revision
        );
        assert!(
            !store
                .replay("k", &request("s", "archive"), "archive-failed")
                .unwrap()
        );
        fs::remove_dir(&path).unwrap();
        drop(store);
        fs::write(&path, b"broken").unwrap();
        assert!(ThreadPreferences::open(&path, "scope").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn sidebar_preferences_migrate_persist_and_reject_stale_aba_without_changing_pin_membership() {
        use crate::SidebarSort;
        let path = path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"version":2,"scopes":{"s":{"pins":[{"endpoint":"pi","sessionId":"a"}],"receipts":{}}}}"#).unwrap();
        let last;
        {
            let store = ThreadPreferences::open(&path, "s").unwrap();
            let original = store.sidebar_preferences();
            let mut value = original.preferences.clone();
            value.sort = SidebarSort::TitleDescending;
            value.pin_order = vec![SessionAddress {
                endpoint_id: "pi".into(),
                session_id: "a".into(),
            }];
            value.pinned_expanded = false;
            store
                .set_sidebar_preferences(&SidebarPreferenceChange {
                    expected: original.clone(),
                    value,
                })
                .unwrap();
            let changed = store.sidebar_preferences();
            assert_eq!(changed.revision, original.revision + 1);
            store
                .set_sidebar_preferences(&SidebarPreferenceChange {
                    expected: changed,
                    value: original.preferences.clone(),
                })
                .unwrap();
            assert!(
                store
                    .set_sidebar_preferences(&SidebarPreferenceChange {
                        expected: original,
                        value: crate::SidebarPreferences {
                            pinned_expanded: false,
                            ..Default::default()
                        }
                    })
                    .is_err()
            );
            assert_eq!(store.pins().len(), 1);
            let expected = store.sidebar_preferences();
            let value = crate::SidebarPreferences {
                sort: SidebarSort::TitleDescending,
                pin_order: vec![SessionAddress {
                    endpoint_id: "pi".into(),
                    session_id: "a".into(),
                }],
                pinned_expanded: false,
                group_by_folder: true,
                collapsed_folders: vec![crate::SidebarFolderKey {
                    endpoint_id: "pi".into(),
                    identity: crate::SidebarFolderIdentity::Id("original-folder".into()),
                }],
            };
            store
                .set_sidebar_preferences(&SidebarPreferenceChange { expected, value })
                .unwrap();
            last = store.sidebar_preferences();
        }
        {
            let store = ThreadPreferences::open(&path, "s").unwrap();
            assert_eq!(store.sidebar_preferences(), last);
            assert_eq!(store.pins().len(), 1);
        }
        {
            let store = ThreadPreferences::open(&path, "other-account").unwrap();
            assert_eq!(
                store.sidebar_preferences(),
                SidebarPreferencesSnapshot::default()
            );
            assert!(store.pins().is_empty());
        }
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn format_three_migration_keeps_existing_sort_order_and_defaults_folder_state() {
        let path = path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"version":3,"scopes":{"s":{"pins":[],"receipts":{},"sidebar":{"revision":7,"preferences":{"sort":"titleAscending","pinOrder":[],"pinnedExpanded":false}}}}}"#).unwrap();
        let store = ThreadPreferences::open(&path, "s").unwrap();
        let value = store.sidebar_preferences();
        assert_eq!(value.revision, 7);
        assert_eq!(value.preferences.sort, crate::SidebarSort::TitleAscending);
        assert!(!value.preferences.pinned_expanded);
        assert!(!value.preferences.group_by_folder);
        assert!(value.preferences.collapsed_folders.is_empty());
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(document["version"], 5);
        drop(store);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn sidebar_write_failure_and_duplicate_order_do_not_publish_preferences() {
        let path = path();
        let store = ThreadPreferences::open(&path, "s").unwrap();
        let original = store.sidebar_preferences();
        let changes = store.subscribe();
        let mut value = original.preferences.clone();
        value.pinned_expanded = false;
        fs::create_dir(&path).unwrap();
        assert!(
            store
                .set_sidebar_preferences(&SidebarPreferenceChange {
                    expected: original.clone(),
                    value
                })
                .is_err()
        );
        assert_eq!(store.sidebar_preferences(), original);
        assert!(!changes.has_changed().unwrap());
        fs::remove_dir(&path).unwrap();
        let mut invalid = original.preferences.clone();
        let id = SessionAddress {
            endpoint_id: "p".into(),
            session_id: "a".into(),
        };
        invalid.pin_order = vec![id.clone(), id];
        assert!(
            store
                .set_sidebar_preferences(&SidebarPreferenceChange {
                    expected: original.clone(),
                    value: invalid
                })
                .is_err()
        );
        assert_eq!(store.sidebar_preferences(), original);
        drop(store);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn application_archive_is_durable_scoped_and_replay_does_not_undo_later_restore() {
        let path = path();
        {
            let store = ThreadPreferences::open(&path, "account/project").unwrap();
            let mut changes = store.subscribe();
            let initial = store.application_archive_snapshot("native");
            assert!(initial.session_ids.is_empty());
            assert!(
                !store
                    .perform("native", &request("same", "archive"), "archive-1")
                    .await
                    .unwrap()
                    .clear_selection
            );
            changes.changed().await.unwrap();
            assert!(
                store
                    .application_archive_snapshot("native")
                    .session_ids
                    .contains("same")
            );
            assert!(
                !store
                    .application_archive_snapshot("pi")
                    .session_ids
                    .contains("same")
            );
            assert_eq!(
                store.application_archive_snapshot("native").revision,
                initial.revision + 1
            );
            store
                .perform("pi", &request("same", "archive"), "archive-1")
                .await
                .unwrap();
            store
                .perform("native", &request("same", "unarchive"), "restore-1")
                .await
                .unwrap();
            let restored = store.application_archive_snapshot("native");
            store
                .perform("native", &request("same", "archive"), "archive-1")
                .await
                .unwrap();
            assert!(
                !store
                    .application_archive_state("native", "same")
                    .unwrap()
                    .archived,
                "replay is not a new archive after restore"
            );
            assert_eq!(
                store.application_archive_snapshot("native").revision,
                restored.revision
            );
            assert!(
                store
                    .perform("native", &request("other", "archive"), "archive-1")
                    .await
                    .is_err()
            );
        }
        {
            let reopened = ThreadPreferences::open(&path, "account/project").unwrap();
            assert!(
                reopened
                    .application_archive_state("pi", "same")
                    .unwrap()
                    .archived
            );
            assert!(
                !reopened
                    .application_archive_state("native", "same")
                    .unwrap()
                    .archived
            );
            assert!(
                reopened
                    .replay("pi", &request("same", "archive"), "archive-1")
                    .unwrap()
            );
            let document: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(document["version"], 5);
            assert!(document["scopes"]["account/project"].get("title").is_none());
        }
        {
            let other = ThreadPreferences::open(&path, "other-account/project").unwrap();
            assert!(
                other
                    .application_archive_snapshot("pi")
                    .session_ids
                    .is_empty()
            );
        }
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn version_four_preferences_migrate_with_empty_application_archive() {
        let path = path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path,br#"{"version":4,"scopes":{"s":{"pins":[{"endpoint":"native","sessionId":"keep"}],"receipts":{}}}}"#).unwrap();
        let store = ThreadPreferences::open(&path, "s").unwrap();
        assert!(store.pin_state("native", "keep").unwrap().pinned);
        assert!(
            store
                .application_archive_snapshot("native")
                .session_ids
                .is_empty()
        );
        drop(store);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
