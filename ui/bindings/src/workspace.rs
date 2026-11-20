//! Stateful UI projection binding over `WorkspaceServiceEndpoint` with `(workspace_id, revision)`
//! fencing, DFS pre-order tree construction, decoded RGBA8 bitmap caching, and citation resolution.

use crate::UiWakeNotifier;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tekes_agent_contract::{
    ArtifactSelectionReference, ArtifactSelectionResolution, ArtifactSelectionTarget,
    WorkspaceFileContent, WorkspaceFileListing, WorkspaceMethod, WorkspaceServiceCapabilities,
    WorkspaceServiceEndpoint, WorkspaceTurnChanges,
};
use tokio::runtime::Handle;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChangesViewMode {
    #[default]
    WorkingTree,
    TurnChanges,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTreeItem {
    pub path: String,
    pub parent_path: Option<String>,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceChangedItem {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadedArtifactContent {
    Loading,
    CodeText {
        text: String,
        language: String,
    },
    Markdown(String),
    Image {
        rgba8_premul: Vec<u8>,
        width: u32,
        height: u32,
        raw_bytes: Vec<u8>,
    },
    Binary {
        size: u64,
    },
    Truncated {
        preview_text: String,
        size: u64,
    },
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedArtifact {
    pub workspace_id: String,
    pub path: String,
    pub revision: String,
    pub digest: String,
    pub content: LoadedArtifactContent,
}

struct WorkspaceUiState {
    workspace_id: String,
    revision_seq: u64,
    capabilities: Option<WorkspaceServiceCapabilities>,
    tree_items: Vec<WorkspaceTreeItem>,
    selected_file_path: Option<String>,
    file_search_query: String,
    search_results: Vec<WorkspaceTreeItem>,
    changes_mode: ChangesViewMode,
    git_branch: String,
    git_revision: String,
    working_files: Vec<WorkspaceChangedItem>,
    turn_changes: Option<WorkspaceTurnChanges>,
    selected_change_path: Option<String>,
    selected_diff_revision: String,
    selected_diff_digest: String,
    selected_diff_text: String,
    commit_message: String,
    last_commit_status: Option<String>,
    loaded_artifacts: BTreeMap<String, LoadedArtifact>,
}

pub struct WorkspaceBinding {
    endpoint: Arc<dyn WorkspaceServiceEndpoint>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    state: Mutex<WorkspaceUiState>,
}

impl std::fmt::Debug for WorkspaceBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceBinding")
            .field("workspace_id", &self.workspace_id())
            .finish_non_exhaustive()
    }
}

impl WorkspaceBinding {
    pub fn new(
        workspace_id: impl Into<String>,
        endpoint: Arc<dyn WorkspaceServiceEndpoint>,
        handle: Handle,
        wake: Arc<dyn UiWakeNotifier>,
    ) -> Arc<Self> {
        Arc::new(Self {
            endpoint,
            handle,
            wake,
            state: Mutex::new(WorkspaceUiState {
                workspace_id: workspace_id.into(),
                revision_seq: 1,
                capabilities: None,
                tree_items: Vec::new(),
                selected_file_path: None,
                file_search_query: String::new(),
                search_results: Vec::new(),
                changes_mode: ChangesViewMode::WorkingTree,
                git_branch: "main".into(),
                git_revision: "r1".into(),
                working_files: Vec::new(),
                turn_changes: None,
                selected_change_path: None,
                selected_diff_revision: String::new(),
                selected_diff_digest: String::new(),
                selected_diff_text: String::new(),
                commit_message: String::new(),
                last_commit_status: None,
                loaded_artifacts: BTreeMap::new(),
            }),
        })
    }

    pub fn workspace_id(&self) -> String {
        self.state.lock().expect("ws lock").workspace_id.clone()
    }

    pub fn ui_revision(&self) -> u64 {
        self.state.lock().expect("ws lock").revision_seq
    }

    fn bump(&self) {
        {
            let mut g = self.state.lock().expect("ws lock");
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        self.wake.wake();
    }

    pub fn refresh_all_blocking(&self) -> tekes_agent_contract::Result<()> {
        let ws_id = self.workspace_id();
        let endpoint = Arc::clone(&self.endpoint);
        let caps_val = self.handle.block_on(endpoint.call(
            WorkspaceMethod::Capabilities,
            json!({}),
            "wse-caps",
        ))?;
        let caps: WorkspaceServiceCapabilities = serde_json::from_value(caps_val)?;

        let items = Self::collect_tree_preorder(&endpoint, &self.handle, &ws_id)?;

        let mut git_branch = "main".to_owned();
        let mut git_revision = "r1".to_owned();
        let mut working_files = Vec::new();
        if caps.supports(WorkspaceMethod::GitStatus)
            && let Ok(status_val) = self.handle.block_on(endpoint.call(
                WorkspaceMethod::GitStatus,
                json!({ "workspaceId": ws_id }),
                "wse-status",
            ))
        {
            git_branch = status_val["branch"].as_str().unwrap_or("main").to_owned();
            git_revision = status_val["revision"].as_str().unwrap_or("r1").to_owned();
            if let Some(arr) = status_val["files"].as_array() {
                for f in arr {
                    if let Some(p) = f["path"].as_str() {
                        working_files.push(WorkspaceChangedItem {
                            path: p.to_owned(),
                            status: f["status"].as_str().unwrap_or("modified").to_owned(),
                            additions: f["additions"].as_u64().unwrap_or(0),
                            deletions: f["deletions"].as_u64().unwrap_or(0),
                        });
                    }
                }
            }
        }

        let first_change = working_files.first().map(|f| f.path.clone());
        {
            let mut g = self.state.lock().expect("ws lock");
            g.capabilities = Some(caps);
            g.tree_items = items;
            if g.selected_file_path.is_none() {
                g.selected_file_path = g
                    .tree_items
                    .iter()
                    .find(|i| !i.is_dir)
                    .map(|i| i.path.clone());
            }
            g.git_branch = git_branch;
            g.git_revision = git_revision;
            g.working_files = working_files;
            if g.selected_change_path.is_none() {
                g.selected_change_path = first_change.clone();
            }
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        if let Some(path) = first_change {
            let _ = self.load_diff_blocking(&path);
        }
        self.wake.wake();
        Ok(())
    }

    fn collect_tree_preorder(
        endpoint: &Arc<dyn WorkspaceServiceEndpoint>,
        handle: &Handle,
        ws_id: &str,
    ) -> tekes_agent_contract::Result<Vec<WorkspaceTreeItem>> {
        fn recurse(
            endpoint: &Arc<dyn WorkspaceServiceEndpoint>,
            handle: &Handle,
            ws_id: &str,
            dir_path: &str,
            parent: Option<&str>,
            out: &mut Vec<WorkspaceTreeItem>,
            depth: usize,
        ) -> tekes_agent_contract::Result<()> {
            if depth > 6 {
                return Ok(());
            }
            let listing_val = handle.block_on(endpoint.call(
                WorkspaceMethod::FilesList,
                json!({ "workspaceId": ws_id, "path": dir_path }),
                "wse-list",
            ))?;
            let listing: WorkspaceFileListing = serde_json::from_value(listing_val)?;
            for entry in listing.entries {
                if entry.hidden && entry.name == ".git" {
                    continue;
                }
                let is_dir = entry.kind == "directory";
                out.push(WorkspaceTreeItem {
                    path: entry.path.clone(),
                    parent_path: parent.map(str::to_owned),
                    name: entry.name.clone(),
                    is_dir,
                });
                if is_dir {
                    recurse(
                        endpoint,
                        handle,
                        ws_id,
                        &entry.path,
                        Some(&entry.path),
                        out,
                        depth + 1,
                    )?;
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        recurse(endpoint, handle, ws_id, "", None, &mut out, 0)?;
        Ok(out)
    }

    pub fn refresh_async(&self, session_id_for_turn_changes: Option<String>) {
        let endpoint = Arc::clone(&self.endpoint);
        let ws_id = self.workspace_id();
        let wake = Arc::clone(&self.wake);
        let expected_rev = {
            let g = self.state.lock().expect("ws lock");
            g.git_revision.clone()
        };
        let _ = (
            expected_rev,
            wake,
            endpoint,
            ws_id,
            session_id_for_turn_changes,
        );
        let _ = self.refresh_all_blocking();
    }

    pub fn load_turn_changes_blocking(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> tekes_agent_contract::Result<()> {
        let ws_id = self.workspace_id();
        let val = self.handle.block_on(self.endpoint.call(
            WorkspaceMethod::TurnChanges,
            json!({
                "workspaceId": ws_id,
                "sessionId": session_id,
                "turnId": turn_id,
                "includeDiff": true,
            }),
            "wse-turn-changes",
        ))?;
        let tc: WorkspaceTurnChanges = serde_json::from_value(val)?;
        {
            let mut g = self.state.lock().expect("ws lock");
            g.turn_changes = Some(tc);
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        self.wake.wake();
        Ok(())
    }

    pub fn set_file_search_query(&self, query: String) {
        let ws_id = self.workspace_id();
        let results = if query.trim().is_empty() {
            Vec::new()
        } else {
            self.handle
                .block_on(self.endpoint.call(
                    WorkspaceMethod::FilesSearch,
                    json!({ "workspaceId": ws_id, "query": query }),
                    "wse-search",
                ))
                .ok()
                .and_then(|v| serde_json::from_value::<WorkspaceFileListing>(v).ok())
                .map(|l| {
                    l.entries
                        .into_iter()
                        .map(|e| WorkspaceTreeItem {
                            path: e.path,
                            parent_path: None,
                            name: e.name,
                            is_dir: e.kind == "directory",
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        {
            let mut g = self.state.lock().expect("ws lock");
            g.file_search_query = query;
            g.search_results = results;
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        self.wake.wake();
    }

    pub fn select_file(&self, path: Option<String>) {
        self.state.lock().expect("ws lock").selected_file_path = path;
        self.bump();
    }

    pub fn selected_file(&self) -> Option<String> {
        self.state
            .lock()
            .expect("ws lock")
            .selected_file_path
            .clone()
    }

    pub fn visible_tree_items(&self) -> Vec<WorkspaceTreeItem> {
        let g = self.state.lock().expect("ws lock");
        if !g.file_search_query.trim().is_empty() {
            g.search_results.clone()
        } else {
            g.tree_items.clone()
        }
    }

    pub fn set_changes_mode(&self, mode: ChangesViewMode, session_id: Option<&str>) {
        {
            let mut g = self.state.lock().expect("ws lock");
            g.changes_mode = mode;
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        if matches!(mode, ChangesViewMode::TurnChanges)
            && let Some(sid) = session_id
        {
            let _ = self.load_turn_changes_blocking(sid, "turn-latest");
        }
        self.wake.wake();
    }

    pub fn select_change_and_load_diff(&self, path: String) {
        {
            let mut g = self.state.lock().expect("ws lock");
            g.selected_change_path = Some(path.clone());
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        let _ = self.load_diff_blocking(&path);
    }

    /// Loads diff while enforcing `(workspace_id, revision)` fencing so stale responses
    /// from an older workspace/revision never overwrite a newer selection.
    pub fn load_diff_blocking(&self, path: &str) -> tekes_agent_contract::Result<()> {
        let (ws_id, expected_rev) = {
            let g = self.state.lock().expect("ws lock");
            (g.workspace_id.clone(), g.git_revision.clone())
        };
        let val = self.handle.block_on(self.endpoint.call(
            WorkspaceMethod::GitDiff,
            json!({
                "workspaceId": ws_id,
                "paths": [path],
                "scope": "all",
            }),
            "wse-diff",
        ))?;
        let resp_ws = val["workspaceId"].as_str().unwrap_or("");
        let resp_rev = val["revision"].as_str().unwrap_or("");
        let mut g = self.state.lock().expect("ws lock");
        if g.workspace_id != ws_id
            || resp_ws != ws_id
            || g.selected_change_path.as_deref() != Some(path)
            || (!resp_rev.is_empty() && resp_rev != expected_rev && g.git_revision != resp_rev)
        {
            // Stale read fenced out.
            return Ok(());
        }
        g.selected_diff_revision = resp_rev.to_owned();
        g.selected_diff_digest = val["digest"].as_str().unwrap_or("").to_owned();
        g.selected_diff_text = val["diff"].as_str().unwrap_or("").to_owned();
        g.revision_seq = g.revision_seq.saturating_add(1);
        drop(g);
        self.wake.wake();
        Ok(())
    }

    /// Directly tests `(workspace_id, revision)` fencing by attempting to apply a diff payload from another revision.
    pub fn apply_diff_if_current(
        &self,
        workspace_id: &str,
        revision: &str,
        path: &str,
        diff: &str,
    ) -> bool {
        let mut g = self.state.lock().expect("ws lock");
        if g.workspace_id == workspace_id
            && g.git_revision == revision
            && g.selected_change_path.as_deref() == Some(path)
        {
            g.selected_diff_revision = revision.to_owned();
            g.selected_diff_text = diff.to_owned();
            g.revision_seq = g.revision_seq.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn set_commit_message(&self, message: String) {
        self.state.lock().expect("ws lock").commit_message = message;
    }

    pub fn can_commit(&self) -> bool {
        let g = self.state.lock().expect("ws lock");
        g.capabilities
            .as_ref()
            .is_some_and(|c| c.mutations && c.supports(WorkspaceMethod::GitCommit))
            && !g.working_files.is_empty()
    }

    pub fn commit_selected(&self, message: &str) -> tekes_agent_contract::Result<()> {
        let (ws_id, paths) = {
            let g = self.state.lock().expect("ws lock");
            let selected = g
                .selected_change_path
                .clone()
                .map(|p| vec![p])
                .unwrap_or_else(|| g.working_files.iter().map(|f| f.path.clone()).collect());
            (g.workspace_id.clone(), selected)
        };
        if paths.is_empty() || message.trim().is_empty() {
            return Ok(());
        }
        let res = self.handle.block_on(self.endpoint.call(
            WorkspaceMethod::GitCommit,
            json!({
                "workspaceId": ws_id,
                "message": message.trim(),
                "paths": paths,
            }),
            "wse-commit",
        ))?;
        self.state.lock().expect("ws lock").last_commit_status = Some(format!(
            "已提交到 {}",
            res["revision"].as_str().unwrap_or("head")
        ));
        let _ = self.refresh_all_blocking();
        Ok(())
    }

    pub fn load_artifact_blocking(&self, path: &str) -> LoadedArtifact {
        let ws_id = self.workspace_id();
        let result = self.handle.block_on(self.endpoint.call(
            WorkspaceMethod::FilesRead,
            json!({
                "workspaceId": ws_id,
                "path": path,
                "maxBytes": 262_144,
            }),
            "wse-read-artifact",
        ));

        let artifact = match result {
            Err(err) => LoadedArtifact {
                workspace_id: ws_id,
                path: path.to_owned(),
                revision: "err".into(),
                digest: String::new(),
                content: LoadedArtifactContent::Error(err.to_string()),
            },
            Ok(val) => {
                let parsed: tekes_agent_contract::Result<WorkspaceFileContent> =
                    serde_json::from_value(val).map_err(Into::into);
                match parsed {
                    Err(err) => LoadedArtifact {
                        workspace_id: ws_id,
                        path: path.to_owned(),
                        revision: "err".into(),
                        digest: String::new(),
                        content: LoadedArtifactContent::Error(err.to_string()),
                    },
                    Ok(file) => decode_artifact(&file),
                }
            }
        };
        {
            let mut g = self.state.lock().expect("ws lock");
            g.loaded_artifacts.insert(path.to_owned(), artifact.clone());
            g.revision_seq = g.revision_seq.saturating_add(1);
        }
        self.wake.wake();
        artifact
    }

    pub fn get_artifact(&self, path: &str) -> Option<LoadedArtifact> {
        self.state
            .lock()
            .expect("ws lock")
            .loaded_artifacts
            .get(path)
            .cloned()
    }

    pub fn resolve_quote_reference(
        &self,
        reference: &ArtifactSelectionReference,
    ) -> ArtifactSelectionResolution {
        let artifact = self.get_artifact(&reference.artifact_id);
        match artifact {
            None => ArtifactSelectionResolution::Unavailable,
            Some(art) => {
                let text_opt = match &art.content {
                    LoadedArtifactContent::CodeText { text, .. } => Some(text.as_str()),
                    LoadedArtifactContent::Markdown(text) => Some(text.as_str()),
                    _ => None,
                };
                reference.resolve(&ArtifactSelectionTarget {
                    document_id: Some(&format!("{}:{}", art.workspace_id, art.path)),
                    artifact_id: Some(&art.path),
                    artifact_type: Some("file"),
                    field_path: Some(&reference.field_path),
                    revision: Some(&art.revision),
                    digest: Some(&art.digest),
                    field_text: text_opt,
                })
            }
        }
    }

    pub fn snapshot_changes(&self) -> ChangesSnapshot {
        let g = self.state.lock().expect("ws lock");
        ChangesSnapshot {
            workspace_id: g.workspace_id.clone(),
            branch: g.git_branch.clone(),
            git_revision: g.git_revision.clone(),
            mode: g.changes_mode,
            working_files: g.working_files.clone(),
            turn_changes: g.turn_changes.clone(),
            selected_path: g.selected_change_path.clone(),
            selected_diff_revision: g.selected_diff_revision.clone(),
            selected_diff_digest: g.selected_diff_digest.clone(),
            selected_diff_text: g.selected_diff_text.clone(),
            can_commit: g
                .capabilities
                .as_ref()
                .is_some_and(|c| c.mutations && c.supports(WorkspaceMethod::GitCommit))
                && !g.working_files.is_empty(),
            last_commit_status: g.last_commit_status.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangesSnapshot {
    pub workspace_id: String,
    pub branch: String,
    pub git_revision: String,
    pub mode: ChangesViewMode,
    pub working_files: Vec<WorkspaceChangedItem>,
    pub turn_changes: Option<WorkspaceTurnChanges>,
    pub selected_path: Option<String>,
    pub selected_diff_revision: String,
    pub selected_diff_digest: String,
    pub selected_diff_text: String,
    pub can_commit: bool,
    pub last_commit_status: Option<String>,
}

pub fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256-{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    sha256_prefixed(bytes)
}

fn is_image_ext(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
}

fn decode_artifact(file: &WorkspaceFileContent) -> LoadedArtifact {
    let rev = file.revision.clone().unwrap_or_else(|| "r0".to_owned());
    let Ok(bytes) = STANDARD.decode(&file.content) else {
        return LoadedArtifact {
            workspace_id: file.workspace_id.clone(),
            path: file.path.clone(),
            revision: rev,
            digest: String::new(),
            content: LoadedArtifactContent::Error("Invalid base64 payload".into()),
        };
    };
    let digest = sha256_hex(&bytes);

    if is_image_ext(&file.path)
        && let Ok(dyn_img) = image::load_from_memory(&bytes)
    {
        let rgba = dyn_img.into_rgba8();
        let (width, height) = (rgba.width(), rgba.height());
        let mut premul = rgba.into_raw();
        for px in premul.chunks_exact_mut(4) {
            let a = u16::from(px[3]);
            px[0] = ((u16::from(px[0]) * a) / 255) as u8;
            px[1] = ((u16::from(px[1]) * a) / 255) as u8;
            px[2] = ((u16::from(px[2]) * a) / 255) as u8;
        }
        return LoadedArtifact {
            workspace_id: file.workspace_id.clone(),
            path: file.path.clone(),
            revision: rev,
            digest,
            content: LoadedArtifactContent::Image {
                rgba8_premul: premul,
                width,
                height,
                raw_bytes: bytes,
            },
        };
    }

    if let Ok(text) = String::from_utf8(bytes.clone()) {
        if file.truncated {
            return LoadedArtifact {
                workspace_id: file.workspace_id.clone(),
                path: file.path.clone(),
                revision: rev,
                digest,
                content: LoadedArtifactContent::Truncated {
                    preview_text: text,
                    size: file.size,
                },
            };
        }
        let lower = file.path.to_lowercase();
        if lower.ends_with(".md") || lower.ends_with(".markdown") {
            return LoadedArtifact {
                workspace_id: file.workspace_id.clone(),
                path: file.path.clone(),
                revision: rev,
                digest,
                content: LoadedArtifactContent::Markdown(text),
            };
        }
        let lang = lower.rsplit('.').next().unwrap_or("txt").to_owned();
        return LoadedArtifact {
            workspace_id: file.workspace_id.clone(),
            path: file.path.clone(),
            revision: rev,
            digest,
            content: LoadedArtifactContent::CodeText {
                text,
                language: lang,
            },
        };
    }

    LoadedArtifact {
        workspace_id: file.workspace_id.clone(),
        path: file.path.clone(),
        revision: rev,
        digest,
        content: LoadedArtifactContent::Binary { size: file.size },
    }
}
