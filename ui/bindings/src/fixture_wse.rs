//! Deterministic `WorkspaceServiceEndpoint` fixture + `cap-std` confined read-only host FS endpoint.

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use cap_std::{ambient_authority, fs::Dir};
use image::{ImageBuffer, Rgba, codecs::png::PngEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use tekes_agent_contract::{
    Error, Result, WorkspaceFileContent, WorkspaceFileEntry, WorkspaceFileListing, WorkspaceMethod,
    WorkspaceServiceCapabilities, WorkspaceServiceEndpoint, WorkspaceTurnChanges,
    WorkspaceTurnFile, require, string, validate_workspace_request, validate_workspace_response,
};

#[derive(Clone, Debug)]
struct StoredFile {
    bytes: Vec<u8>,
    revision: u64,
    truncated_preview: bool,
    logical_size: u64,
}

struct FixtureWsState {
    workspace_id: String,
    branch: String,
    git_revision: u64,
    mutations_enabled: bool,
    files: BTreeMap<String, StoredFile>,
    changed_status: BTreeMap<String, (&'static str, u64, u64)>,
    commits: Vec<String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256-{:x}", hasher.finalize())
}

fn generate_sample_png_bytes() -> Vec<u8> {
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(64, 64);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let checker = ((x / 8) + (y / 8)) % 2 == 0;
        let r = if checker { 48 } else { 220 };
        let g = (x * 4) as u8;
        let b = (y * 4) as u8;
        *pixel = Rgba([r, g, b, 255]);
    }
    let mut cursor = Cursor::new(Vec::new());
    let encoder = PngEncoder::new(&mut cursor);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        64,
        64,
        image::ExtendedColorType::Rgba8,
    )
    .expect("encode fixture PNG");
    cursor.into_inner()
}

#[derive(Clone)]
pub struct DeterministicWorkspaceEndpoint {
    id: String,
    state: Arc<Mutex<FixtureWsState>>,
}

impl Default for DeterministicWorkspaceEndpoint {
    fn default() -> Self {
        Self::new("ws-main")
    }
}

impl DeterministicWorkspaceEndpoint {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        let mut files = BTreeMap::new();
        files.insert(
            "src/lib.rs".into(),
            StoredFile {
                bytes: b"pub fn mount_agent_workbench(slot: &str) -> &'static str {\n    match slot {\n        \"focus\" => \"primary\",\n        \"others\" => \"docked\",\n        _ => \"nav\",\n    }\n}\n\npub fn compute_delta(added: usize, removed: usize) -> usize {\n    added.saturating_add(removed)\n}\n".to_vec(),
                revision: 1,
                truncated_preview: false,
                logical_size: 230,
            },
        );
        files.insert(
            "docs/architecture.md".into(),
            StoredFile {
                bytes: b"# AppHost Architecture Overview\n\n- Topology A: AgentRun in `focus`, Summary/Files in `others`\n- Topology B: DomainCanvas in `focus`, AgentRun/Summary/Files in `others`\n\n```rust\nstate.promote(\"agent-run\")?;\n```\n".to_vec(),
                revision: 1,
                truncated_preview: false,
                logical_size: 205,
            },
        );
        files.insert(
            "docs/tekesui-apphost-architecture.md".into(),
            StoredFile {
                bytes: b"# TekesUI v0.4.1 Integration\n\n100% safe Rust containers managed through `SharedAgentWorkbench` and `WindowWake`.\n".to_vec(),
                revision: 1,
                truncated_preview: false,
                logical_size: 118,
            },
        );
        let png_bytes = generate_sample_png_bytes();
        files.insert(
            "assets/preview.png".into(),
            StoredFile {
                logical_size: png_bytes.len() as u64,
                bytes: png_bytes,
                revision: 1,
                truncated_preview: false,
            },
        );
        files.insert(
            "specs/chassis.cadspec".into(),
            StoredFile {
                bytes: b"CADSPEC v2\ncomponent: aerospace_bracket_01\nmaterial: Ti-6Al-4V\ntolerance_mm: 0.02\nvertices: 14820\nmass_kg: 1.42\nstatus: verified\n".to_vec(),
                revision: 1,
                truncated_preview: false,
                logical_size: 136,
            },
        );
        files.insert(
            "bin/firmware.bin".into(),
            StoredFile {
                bytes: vec![0x7F, b'E', b'L', b'F', 0x00, 0xFF, 0xFE, 0x80, 0x99, 0x10],
                revision: 1,
                truncated_preview: false,
                logical_size: 10,
            },
        );
        let log_preview = b"[2026-11-20T15:00:00Z] trace head line 1\n[2026-11-20T15:00:01Z] truncated preview (16 MiB total on disk)\n".to_vec();
        files.insert(
            "logs/huge.log".into(),
            StoredFile {
                bytes: log_preview,
                revision: 1,
                truncated_preview: true,
                logical_size: 16 * 1024 * 1024,
            },
        );

        let mut changed_status = BTreeMap::new();
        changed_status.insert("src/lib.rs".into(), ("modified", 8, 2));
        changed_status.insert("docs/architecture.md".into(), ("modified", 4, 0));

        Self {
            id: "fixture-wse".into(),
            state: Arc::new(Mutex::new(FixtureWsState {
                workspace_id: workspace_id.into(),
                branch: "main".into(),
                git_revision: 1,
                mutations_enabled: true,
                files,
                changed_status,
                commits: Vec::new(),
            })),
        }
    }

    pub fn set_mutations_enabled(&self, enabled: bool) {
        self.state.lock().expect("wse lock").mutations_enabled = enabled;
    }

    pub fn advance_file_revision(&self, path: &str, new_text: impl AsRef<[u8]>) -> u64 {
        let mut guard = self.state.lock().expect("wse lock");
        guard.git_revision += 1;
        let rev = guard.git_revision;
        let bytes = new_text.as_ref().to_vec();
        let len = bytes.len() as u64;
        guard.files.insert(
            path.to_owned(),
            StoredFile {
                bytes,
                revision: rev,
                truncated_preview: false,
                logical_size: len,
            },
        );
        guard
            .changed_status
            .insert(path.to_owned(), ("modified", 12, 3));
        rev
    }

    pub fn committed_messages(&self) -> Vec<String> {
        self.state.lock().expect("wse lock").commits.clone()
    }

    fn entries_under(guard: &FixtureWsState, dir_path: &str) -> Vec<WorkspaceFileEntry> {
        let prefix = if dir_path.is_empty() {
            String::new()
        } else {
            format!("{}/", dir_path.trim_matches('/'))
        };
        let mut dirs_seen = std::collections::BTreeSet::new();
        let mut entries = Vec::new();

        for path in guard.files.keys() {
            if !path.starts_with(&prefix) {
                continue;
            }
            let rest = &path[prefix.len()..];
            if let Some((dir, _)) = rest.split_once('/') {
                if dirs_seen.insert(dir.to_owned()) {
                    entries.push(WorkspaceFileEntry {
                        name: dir.to_owned(),
                        path: format!("{prefix}{dir}"),
                        kind: "directory".into(),
                        hidden: dir.starts_with('.'),
                    });
                }
            } else if !rest.is_empty() {
                entries.push(WorkspaceFileEntry {
                    name: rest.to_owned(),
                    path: path.clone(),
                    kind: "file".into(),
                    hidden: rest.starts_with('.'),
                });
            }
        }
        entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
        entries
    }
}

#[async_trait]
impl WorkspaceServiceEndpoint for DeterministicWorkspaceEndpoint {
    fn connection_id(&self) -> &str {
        &self.id
    }

    async fn call(&self, method: WorkspaceMethod, payload: Value, _rpc_id: &str) -> Result<Value> {
        validate_workspace_request(method, &payload)?;
        let guard = self.state.lock().expect("wse lock");
        if method != WorkspaceMethod::Capabilities {
            let req_ws = string(&payload, "workspaceId")?;
            require(req_ws == guard.workspace_id, "unknown workspaceId")?;
        }
        let response = match method {
            WorkspaceMethod::Capabilities => {
                let mut methods = vec![
                    "capabilities",
                    "filesList",
                    "filesSearch",
                    "filesRead",
                    "gitStatus",
                    "gitDiff",
                    "turnChanges",
                ];
                if guard.mutations_enabled {
                    methods.push("gitCommit");
                    methods.push("filesWrite");
                }
                serde_json::to_value(WorkspaceServiceCapabilities {
                    version: 1,
                    methods: methods.into_iter().map(str::to_owned).collect(),
                    mutations: guard.mutations_enabled,
                    allowed_git_operations: if guard.mutations_enabled {
                        Some(vec![
                            "gitStatus".into(),
                            "gitDiff".into(),
                            "gitCommit".into(),
                        ])
                    } else {
                        Some(vec!["gitStatus".into(), "gitDiff".into()])
                    },
                    turn_changes: true,
                })?
            }
            WorkspaceMethod::FilesList => {
                let path = payload["path"].as_str().unwrap_or("");
                let entries = Self::entries_under(&guard, path);
                serde_json::to_value(WorkspaceFileListing {
                    workspace_id: guard.workspace_id.clone(),
                    path: path.to_owned(),
                    entries,
                    truncated: false,
                })?
            }
            WorkspaceMethod::FilesSearch => {
                let query = string(&payload, "query")?.to_lowercase();
                let entries: Vec<WorkspaceFileEntry> = guard
                    .files
                    .keys()
                    .filter(|p| p.to_lowercase().contains(&query))
                    .map(|p| WorkspaceFileEntry {
                        name: p.rsplit('/').next().unwrap_or(p).to_owned(),
                        path: p.clone(),
                        kind: "file".into(),
                        hidden: false,
                    })
                    .collect();
                serde_json::to_value(WorkspaceFileListing {
                    workspace_id: guard.workspace_id.clone(),
                    path: String::new(),
                    entries,
                    truncated: false,
                })?
            }
            WorkspaceMethod::FilesRead => {
                let path = string(&payload, "path")?;
                let file = guard
                    .files
                    .get(path)
                    .ok_or_else(|| Error::Protocol(format!("missing file: {path}")))?;
                let max_bytes = payload["maxBytes"].as_u64().unwrap_or(0) as usize;
                let slice = if max_bytes > 0 && file.bytes.len() > max_bytes {
                    &file.bytes[..max_bytes]
                } else {
                    &file.bytes[..]
                };
                let truncated =
                    file.truncated_preview || (max_bytes > 0 && file.bytes.len() > max_bytes);
                serde_json::to_value(WorkspaceFileContent {
                    workspace_id: guard.workspace_id.clone(),
                    path: path.to_owned(),
                    encoding: "base64".into(),
                    content: STANDARD.encode(slice),
                    size: file.logical_size,
                    truncated,
                    revision: Some(format!("r{}", file.revision)),
                })?
            }
            WorkspaceMethod::GitStatus => {
                let files: Vec<Value> = guard
                    .changed_status
                    .iter()
                    .map(|(path, (status, add, del))| {
                        json!({
                            "path": path,
                            "status": status,
                            "additions": add,
                            "deletions": del,
                        })
                    })
                    .collect();
                json!({
                    "workspaceId": guard.workspace_id,
                    "branch": guard.branch,
                    "revision": format!("r{}", guard.git_revision),
                    "files": files,
                })
            }
            WorkspaceMethod::GitDiff => {
                let paths = payload["paths"].as_array().cloned().unwrap_or_default();
                let target = paths
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or("src/lib.rs");
                let rev = guard.git_revision;
                let diff = match target {
                    "src/lib.rs" => format!(
                        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,8 @@\n pub fn mount_agent_workbench(slot: &str) -> &'static str {{\n+    // revision r{rev}\n     match slot {{\n         \"focus\" => \"primary\",\n-        \"others\" => \"side\",\n+        \"others\" => \"docked\",\n         _ => \"nav\",\n     }}\n }}"
                    ),
                    "docs/architecture.md" => format!(
                        "diff --git a/docs/architecture.md b/docs/architecture.md\n--- a/docs/architecture.md\n+++ b/docs/architecture.md\n@@ -1,3 +1,5 @@\n # AppHost Architecture Overview\n+- revision r{rev}\n - Topology A: AgentRun in `focus`"
                    ),
                    other => format!(
                        "diff --git a/{other} b/{other}\n--- a/{other}\n+++ b/{other}\n@@ -1 +1,2 @@\n+// revision r{rev}\n // updated"
                    ),
                };
                json!({
                    "workspaceId": guard.workspace_id,
                    "path": target,
                    "revision": format!("r{rev}"),
                    "digest": sha256_hex(diff.as_bytes()),
                    "diff": diff,
                })
            }
            WorkspaceMethod::TurnChanges => {
                let session_id = string(&payload, "sessionId")?.to_owned();
                let turn_id = string(&payload, "turnId")?.to_owned();
                serde_json::to_value(WorkspaceTurnChanges {
                    session_id,
                    turn_id,
                    metric: "lines".into(),
                    coverage: "exact".into(),
                    state: "completed".into(),
                    files: vec![WorkspaceTurnFile {
                        path: "src/lib.rs".into(),
                        additions: Some(4),
                        deletions: Some(1),
                        state: "modified".into(),
                        unified_diff: Some(
                            "diff --git a/src/lib.rs b/src/lib.rs\n@@ -1,4 +1,5 @@\n+    // turn verified\n     \"focus\" => \"primary\",".into(),
                        ),
                    }],
                    file_count: 1,
                    additions: Some(4),
                    deletions: Some(1),
                    revision: guard.git_revision,
                })?
            }
            WorkspaceMethod::GitCommit => {
                require(guard.mutations_enabled, "workspace mutations disabled")?;
                let message = string(&payload, "message")?.to_owned();
                let paths: Vec<String> = payload["paths"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                drop(guard);
                let mut mut_guard = self.state.lock().expect("wse lock");
                mut_guard.commits.push(message.clone());
                for p in &paths {
                    mut_guard.changed_status.remove(p);
                }
                mut_guard.git_revision += 1;
                json!({
                    "workspaceId": mut_guard.workspace_id,
                    "committed": true,
                    "message": message,
                    "revision": format!("r{}", mut_guard.git_revision),
                })
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "fixture workspace method: {}",
                    other.name()
                )));
            }
        };
        validate_workspace_response(method, &payload, &response)?;
        Ok(response)
    }
}

/// Safe read-only local filesystem `WorkspaceServiceEndpoint` strictly confined to `root`
/// via `cap_std::fs::Dir` (prevents `..` traversal and symlink escapes).
pub struct ReadOnlyFsWorkspaceEndpoint {
    id: String,
    workspace_id: String,
    root: Dir,
}

impl ReadOnlyFsWorkspaceEndpoint {
    pub fn open(workspace_id: impl Into<String>, root_path: impl AsRef<Path>) -> Result<Self> {
        let dir = Dir::open_ambient_dir(root_path.as_ref(), ambient_authority())
            .map_err(|e| Error::Transport(format!("open workspace dir failed: {e}")))?;
        Ok(Self {
            id: "readonly-fs-wse".into(),
            workspace_id: workspace_id.into(),
            root: dir,
        })
    }

    pub fn sanitize_relative_path(raw: &str) -> Result<PathBuf> {
        require(!raw.contains('\0'), "NUL byte in workspace path")?;
        let trimmed = raw.trim_start_matches('/');
        if trimmed.is_empty() {
            return Ok(PathBuf::from("."));
        }
        let path = Path::new(trimmed);
        for comp in path.components() {
            match comp {
                Component::Normal(_) => {}
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(Error::Protocol(format!(
                        "path escapes confined workspace root: {raw}"
                    )));
                }
            }
        }
        Ok(path.to_path_buf())
    }
}

#[async_trait]
impl WorkspaceServiceEndpoint for ReadOnlyFsWorkspaceEndpoint {
    fn connection_id(&self) -> &str {
        &self.id
    }

    async fn call(&self, method: WorkspaceMethod, payload: Value, _rpc_id: &str) -> Result<Value> {
        validate_workspace_request(method, &payload)?;
        if method != WorkspaceMethod::Capabilities {
            require(
                string(&payload, "workspaceId")? == self.workspace_id,
                "workspaceId mismatch",
            )?;
        }
        let response = match method {
            WorkspaceMethod::Capabilities => serde_json::to_value(WorkspaceServiceCapabilities {
                version: 1,
                methods: vec![
                    "capabilities".into(),
                    "filesList".into(),
                    "filesSearch".into(),
                    "filesRead".into(),
                ],
                mutations: false,
                allowed_git_operations: Some(vec![]),
                turn_changes: false,
            })?,
            WorkspaceMethod::FilesList => {
                let raw_path = payload["path"].as_str().unwrap_or("");
                let rel = Self::sanitize_relative_path(raw_path)?;
                let mut entries = Vec::new();
                let iter = self
                    .root
                    .read_dir(&rel)
                    .map_err(|e| Error::Transport(format!("read_dir failed: {e}")))?;
                for item in iter {
                    let entry = item.map_err(|e| Error::Transport(format!("dir entry: {e}")))?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let kind = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        "directory"
                    } else {
                        "file"
                    };
                    let joined = if raw_path.trim_matches('/').is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{name}", raw_path.trim_matches('/'))
                    };
                    entries.push(WorkspaceFileEntry {
                        hidden: name.starts_with('.'),
                        name,
                        path: joined,
                        kind: kind.into(),
                    });
                }
                entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
                serde_json::to_value(WorkspaceFileListing {
                    workspace_id: self.workspace_id.clone(),
                    path: raw_path.to_owned(),
                    entries,
                    truncated: false,
                })?
            }
            WorkspaceMethod::FilesSearch => {
                let query = string(&payload, "query")?.to_lowercase();
                let mut found = Vec::new();
                fn walk(
                    dir: &Dir,
                    rel_prefix: &str,
                    query: &str,
                    out: &mut Vec<WorkspaceFileEntry>,
                ) {
                    if out.len() >= 100 {
                        return;
                    }
                    let Ok(iter) = dir.read_dir(if rel_prefix.is_empty() {
                        "."
                    } else {
                        rel_prefix
                    }) else {
                        return;
                    };
                    for item in iter.flatten() {
                        let name = item.file_name().to_string_lossy().into_owned();
                        if name == ".git" || name == "target" {
                            continue;
                        }
                        let joined = if rel_prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{rel_prefix}/{name}")
                        };
                        if item.file_type().is_ok_and(|t| t.is_dir()) {
                            walk(dir, &joined, query, out);
                        } else if joined.to_lowercase().contains(query) {
                            out.push(WorkspaceFileEntry {
                                name,
                                path: joined,
                                kind: "file".into(),
                                hidden: false,
                            });
                        }
                    }
                }
                walk(&self.root, "", &query, &mut found);
                serde_json::to_value(WorkspaceFileListing {
                    workspace_id: self.workspace_id.clone(),
                    path: String::new(),
                    entries: found,
                    truncated: false,
                })?
            }
            WorkspaceMethod::FilesRead => {
                let raw_path = string(&payload, "path")?;
                let rel = Self::sanitize_relative_path(raw_path)?;
                let mut file = self
                    .root
                    .open(&rel)
                    .map_err(|e| Error::Transport(format!("open failed: {e}")))?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)
                    .map_err(|e| Error::Transport(format!("read failed: {e}")))?;
                let total_size = buf.len() as u64;
                let max_bytes = payload["maxBytes"].as_u64().unwrap_or(0) as usize;
                let truncated = max_bytes > 0 && buf.len() > max_bytes;
                let slice = if truncated {
                    &buf[..max_bytes]
                } else {
                    &buf[..]
                };
                serde_json::to_value(WorkspaceFileContent {
                    workspace_id: self.workspace_id.clone(),
                    path: raw_path.to_owned(),
                    encoding: "base64".into(),
                    content: STANDARD.encode(slice),
                    size: total_size,
                    truncated,
                    revision: Some(sha256_hex(slice)),
                })?
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "read-only fs workspace refuses method: {}",
                    other.name()
                )));
            }
        };
        validate_workspace_response(method, &payload, &response)?;
        Ok(response)
    }
}
