//! Independent, stateless Workspace Service contract shared by hosts and renderers.
use crate::{Error, Result, require};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The public WSE namespace is independent of SessionEndpoint and agent execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceMethod {
    Capabilities,
    FilesList,
    FilesSearch,
    FilesRead,
    FilesWrite,
    GitRepository,
    GitStatus,
    GitDiff,
    GitBranches,
    GitLog,
    GitChangesNavigator,
    GitChangeBranch,
    GitCommit,
    GitPush,
    TurnChanges,
}
impl WorkspaceMethod {
    pub fn name(self) -> &'static str {
        match self {
            Self::Capabilities => "capabilities",
            Self::FilesList => "filesList",
            Self::FilesSearch => "filesSearch",
            Self::FilesRead => "filesRead",
            Self::FilesWrite => "filesWrite",
            Self::GitRepository => "gitRepository",
            Self::GitStatus => "gitStatus",
            Self::GitDiff => "gitDiff",
            Self::GitBranches => "gitBranches",
            Self::GitLog => "gitLog",
            Self::GitChangesNavigator => "gitChangesNavigator",
            Self::GitChangeBranch => "gitChangeBranch",
            Self::GitCommit => "gitCommit",
            Self::GitPush => "gitPush",
            Self::TurnChanges => "turnChanges",
        }
    }
    pub fn is_mutation(self) -> bool {
        matches!(
            self,
            Self::FilesWrite | Self::GitChangeBranch | Self::GitCommit | Self::GitPush
        )
    }
}

/// Connection settings and credentials are supplied by an authenticated host.
/// Implementations own no cache or retry ledger; callers own projection state.
#[async_trait]
pub trait WorkspaceServiceEndpoint: Send + Sync {
    fn connection_id(&self) -> &str;
    async fn call(&self, method: WorkspaceMethod, payload: Value, rpc_id: &str) -> Result<Value>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceServiceCapabilities {
    pub version: u32,
    pub methods: Vec<String>,
    pub mutations: bool,
    #[serde(default)]
    pub allowed_git_operations: Option<Vec<String>>,
    pub turn_changes: bool,
}
impl WorkspaceServiceCapabilities {
    pub fn supports(&self, method: WorkspaceMethod) -> bool {
        self.methods.iter().any(|name| name == method.name())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub hidden: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileListing {
    pub workspace_id: String,
    pub path: String,
    pub entries: Vec<WorkspaceFileEntry>,
    pub truncated: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileContent {
    pub workspace_id: String,
    pub path: String,
    pub encoding: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
    pub revision: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileWriteReceipt {
    pub workspace_id: String,
    pub path: String,
    pub revision: String,
    pub size: u64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTurnChanges {
    pub session_id: String,
    pub turn_id: String,
    pub metric: String,
    pub coverage: String,
    pub state: String,
    pub files: Vec<WorkspaceTurnFile>,
    pub file_count: u64,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub revision: u64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTurnFile {
    pub path: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub state: String,
    #[serde(default)]
    pub unified_diff: Option<String>,
}

/// Validate external request shape once; the server owns path resolution, grants,
/// repository rules, revision comparison and mutation replay authority.
pub fn validate_workspace_request(method: WorkspaceMethod, payload: &Value) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| Error::Protocol("workspace request must be an object".into()))?;
    if method == WorkspaceMethod::Capabilities {
        return require(object.is_empty(), "capabilities payload must be empty");
    }
    text(payload, "workspaceId", true)?;
    let fields: &[(&str, &str)] = match method {
        WorkspaceMethod::Capabilities => unreachable!(),
        WorkspaceMethod::FilesList => &[("path", "text"), ("limit", "number")],
        WorkspaceMethod::FilesSearch => &[("query", "required"), ("limit", "number")],
        WorkspaceMethod::FilesRead => &[("path", "required"), ("maxBytes", "number")],
        WorkspaceMethod::FilesWrite => &[
            ("path", "required"),
            ("content", "present"),
            ("expectedRevision", "required"),
        ],
        WorkspaceMethod::GitRepository
        | WorkspaceMethod::GitStatus
        | WorkspaceMethod::GitBranches => &[("repositoryPath", "text")],
        WorkspaceMethod::GitLog => &[
            ("repositoryPath", "text"),
            ("limit", "number"),
            ("offset", "number"),
            ("head", "text"),
        ],
        WorkspaceMethod::GitChangesNavigator => &[("includeClean", "bool")],
        WorkspaceMethod::GitChangeBranch => &[
            ("repositoryPath", "text"),
            ("operation", "branch_operation"),
            ("name", "required"),
        ],
        WorkspaceMethod::GitCommit => &[
            ("repositoryPath", "text"),
            ("message", "required"),
            ("paths", "selected_paths"),
        ],
        WorkspaceMethod::GitPush => &[("repositoryPath", "text"), ("remote", "required")],
        WorkspaceMethod::GitDiff => &[
            ("repositoryPath", "text"),
            ("scope", "scope"),
            ("paths", "paths"),
            ("revision", "text"),
        ],
        WorkspaceMethod::TurnChanges => &[
            ("sessionId", "required"),
            ("turnId", "required"),
            ("includeDiff", "bool"),
        ],
    };
    require(
        object.keys().all(|key| {
            key == "workspaceId" || fields.iter().any(|(name, _)| key.as_str() == *name)
        }),
        "unknown workspace request field",
    )?;
    for (name, kind) in fields {
        let Some(value) = payload.get(*name) else {
            require(
                !matches!(
                    *kind,
                    "required" | "present" | "selected_paths" | "branch_operation"
                ),
                &format!("missing workspace {name}"),
            )?;
            continue;
        };
        match *kind {
            "required" | "text" => {
                text(payload, name, *kind == "required")?;
            }
            "present" => {
                text(payload, name, false)?;
            }
            "branch_operation" => require(
                value
                    .as_str()
                    .is_some_and(|operation| matches!(operation, "create" | "switch")),
                "invalid Git branch operation",
            )?,
            "number" => require(
                value.as_u64().is_some(),
                &format!("workspace {name} must be a nonnegative integer"),
            )?,
            "bool" => require(
                value.is_boolean(),
                &format!("workspace {name} must be boolean"),
            )?,
            "paths" => require(
                value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string)),
                "workspace paths must be strings",
            )?,
            "selected_paths" => require(
                value.as_array().is_some_and(|items| {
                    !items.is_empty()
                        && items
                            .iter()
                            .all(|path| path.as_str().is_some_and(|path| !path.is_empty()))
                }),
                "Git commit requires selected paths",
            )?,
            "scope" => require(
                value.as_str().is_some_and(|s| {
                    ["all", "staged", "unstaged", "committed", "branch"].contains(&s)
                }),
                "invalid Git diff scope",
            )?,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str, nonempty: bool) -> Result<&'a str> {
    let result = value[key]
        .as_str()
        .ok_or_else(|| Error::Protocol(format!("workspace {key} must be text")))?;
    require(
        !nonempty || !result.is_empty(),
        &format!("workspace {key} is empty"),
    )?;
    Ok(result)
}

/// Validate scope and common DTOs at the remote trust boundary. Git domain values
/// remain lossless JSON so the service's full public facts are not truncated.
pub fn validate_workspace_response(
    method: WorkspaceMethod,
    request: &Value,
    value: &Value,
) -> Result<()> {
    use WorkspaceMethod as M;
    let keys: &[&str] = match method {
        M::FilesList | M::FilesSearch | M::FilesRead | M::FilesWrite => &["workspaceId"],
        M::TurnChanges => &["sessionId", "turnId"],
        _ => &[],
    };
    for key in keys {
        require(
            request.get(*key).is_some() && request.get(*key) == value.get(*key),
            "workspace response scope mismatch",
        )?;
    }
    match method {
        M::Capabilities => {
            let caps: WorkspaceServiceCapabilities = serde_json::from_value(value.clone())?;
            require(caps.version == 1, "unsupported workspace protocol version")?;
        }
        M::FilesList | M::FilesSearch => {
            let _: WorkspaceFileListing = serde_json::from_value(value.clone())?;
        }
        M::FilesRead => {
            let content: WorkspaceFileContent = serde_json::from_value(value.clone())?;
            require(
                content.encoding == "base64",
                "unsupported workspace content encoding",
            )?;
            let bytes = STANDARD
                .decode(&content.content)
                .map_err(|_| Error::Protocol("invalid workspace base64 content".into()))?;
            require(
                STANDARD.encode(&bytes) == content.content,
                "noncanonical workspace base64 content",
            )?;
            if request["maxBytes"] == 0 {
                require(
                    !content.truncated && bytes.len() as u64 == content.size,
                    "complete workspace read was truncated",
                )?;
            }
        }
        M::FilesWrite => {
            let _: WorkspaceFileWriteReceipt = serde_json::from_value(value.clone())?;
        }
        M::TurnChanges => {
            let _: WorkspaceTurnChanges = serde_json::from_value(value.clone())?;
        }
        _ => require(value.is_object(), "workspace Git result must be an object")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_authority_fields_cannot_enter_a_workspace_request() {
        assert!(
            validate_workspace_request(
                WorkspaceMethod::FilesList,
                &json!({"workspaceId":"server-id","path":""})
            )
            .is_ok()
        );
        for field in ["root", "url", "authorization"] {
            let mut request = json!({"workspaceId":"server-id","path":""});
            request[field] = json!("private");
            assert!(validate_workspace_request(WorkspaceMethod::FilesList, &request).is_err());
        }
        assert!(
            validate_workspace_request(
                WorkspaceMethod::GitCommit,
                &json!({"workspaceId":"id","message":"commit","paths":[4]})
            )
            .is_err()
        );
    }

    #[test]
    fn complete_file_read_rejects_a_prefix_and_wrong_workspace() {
        let request = json!({"workspaceId":"a","path":"proof","maxBytes":0});
        let mut value = json!({"workspaceId":"a","path":"proof","encoding":"base64","content":"YWJj","size":3,"truncated":false,"revision":"r"});
        assert!(validate_workspace_response(WorkspaceMethod::FilesRead, &request, &value).is_ok());
        value["truncated"] = json!(true);
        assert!(validate_workspace_response(WorkspaceMethod::FilesRead, &request, &value).is_err());
        value["truncated"] = json!(false);
        value["workspaceId"] = json!("b");
        assert!(validate_workspace_response(WorkspaceMethod::FilesRead, &request, &value).is_err());
    }

    #[test]
    fn helper_contract_allows_empty_file_content_and_requires_selected_commit_paths() {
        assert!(
            validate_workspace_request(
                WorkspaceMethod::FilesWrite,
                &json!({
                    "workspaceId":"w","path":"proof","content":"","expectedRevision":"a".repeat(64)
                })
            )
            .is_ok()
        );
        assert!(
            validate_workspace_request(
                WorkspaceMethod::FilesWrite,
                &json!({
                    "workspaceId":"w","path":"proof","expectedRevision":"a".repeat(64)
                })
            )
            .is_err()
        );
        for paths in [Value::Null, json!([]), json!([""])] {
            assert!(
                validate_workspace_request(
                    WorkspaceMethod::GitCommit,
                    &json!({
                        "workspaceId":"w","message":"selected commit","paths":paths
                    })
                )
                .is_err()
            );
        }
        assert!(
            validate_workspace_request(
                WorkspaceMethod::GitCommit,
                &json!({
                    "workspaceId":"w","message":"selected commit","paths":["proof"]
                })
            )
            .is_ok()
        );
        for operation in ["create", "switch"] {
            assert!(
                validate_workspace_request(
                    WorkspaceMethod::GitChangeBranch,
                    &json!({
                        "workspaceId":"w","operation":operation,"name":"feature"
                    })
                )
                .is_ok()
            );
        }
        assert!(
            validate_workspace_request(
                WorkspaceMethod::GitChangeBranch,
                &json!({
                    "workspaceId":"w","operation":"delete","name":"feature"
                })
            )
            .is_err()
        );
    }
}
