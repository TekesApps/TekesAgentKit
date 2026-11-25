//! Normalized Harness settings binding over `settings.describe` / `settings.mutate`,
//! with epoch fencing, CAS `expectedRevision` conflict surfacing, read-only state,
//! and strict secret masking (`••••••••`).

use crate::UiWakeNotifier;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tekes_agent_contract::{Error, Result, require};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tokio::runtime::Handle;

pub const MASKED_SECRET_DISPLAY: &str = "••••••••";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessSettingKind {
    Enum(Vec<String>),
    Integer,
    Boolean,
    String,
    Secret,
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessSettingField {
    pub namespace: String,
    pub path: Vec<String>,
    pub key: String,
    pub label: String,
    pub kind: HarnessSettingKind,
    pub display_value: String,
    pub raw_editable_value: Option<Value>,
    pub writable: bool,
    pub is_secret: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HarnessSettingsState {
    pub loaded: bool,
    pub namespace: String,
    pub namespace_label: String,
    pub revision: u64,
    pub read_only: bool,
    pub kernel_format: bool,
    pub fields: Vec<HarnessSettingField>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub conflict_revision: Option<u64>,
    pub ui_revision: u64,
}

static NEXT_SETTINGS_RPC: AtomicU64 = AtomicU64::new(1);

pub struct HarnessSettingsBinding {
    runtime: Arc<SessionRuntime>,
    handle: Handle,
    wake: Arc<dyn UiWakeNotifier>,
    state: Arc<Mutex<HarnessSettingsState>>,
    _watcher: tokio::task::JoinHandle<()>,
}

impl Drop for HarnessSettingsBinding {
    fn drop(&mut self) {
        self._watcher.abort();
    }
}

impl std::fmt::Debug for HarnessSettingsBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HarnessSettingsBinding")
            .finish_non_exhaustive()
    }
}

fn key_looks_secret(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
}

fn value_to_display(val: &Value) -> String {
    match val {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_fixture_namespaces(
    raw: &Value,
) -> Option<(String, String, u64, bool, Vec<HarnessSettingField>)> {
    let namespaces = raw.get("namespaces")?.as_array()?;
    let first = namespaces.first()?;
    let fields_arr = first.get("fields")?.as_array()?;
    let ns_id = first["id"].as_str().unwrap_or("execution").to_owned();
    let ns_label = first["label"].as_str().unwrap_or(&ns_id).to_owned();
    let revision = first["revision"]
        .as_u64()
        .or_else(|| raw["revision"].as_u64())
        .unwrap_or(0);
    let read_only = first["readOnly"]
        .as_bool()
        .or_else(|| raw["readOnly"].as_bool())
        .unwrap_or(false);

    let mut fields = Vec::new();
    for item in fields_arr {
        let key = item["key"].as_str().unwrap_or("").to_owned();
        if key.is_empty() {
            continue;
        }
        let label = item["label"].as_str().unwrap_or(&key).to_owned();
        let is_secret = item["secret"].as_bool().unwrap_or(false)
            || item["kind"].as_str() == Some("secret")
            || key_looks_secret(&key);
        if is_secret {
            fields.push(HarnessSettingField {
                namespace: ns_id.clone(),
                path: vec![key.clone()],
                key,
                label,
                kind: HarnessSettingKind::Secret,
                display_value: MASKED_SECRET_DISPLAY.to_owned(),
                raw_editable_value: None,
                writable: false,
                is_secret: true,
            });
            continue;
        }
        let kind = match item["kind"].as_str() {
            Some("enum") => HarnessSettingKind::Enum(
                item["options"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            Some("integer") => HarnessSettingKind::Integer,
            Some("boolean") => HarnessSettingKind::Boolean,
            Some("string") => HarnessSettingKind::String,
            _ => HarnessSettingKind::ReadOnly,
        };
        let value = item.get("value").cloned().unwrap_or(Value::Null);
        fields.push(HarnessSettingField {
            namespace: ns_id.clone(),
            path: vec![key.clone()],
            key,
            label,
            kind,
            display_value: value_to_display(&value),
            raw_editable_value: Some(value),
            writable: !read_only,
            is_secret: false,
        });
    }
    Some((ns_id, ns_label, revision, read_only, fields))
}

fn collect_kernel_schema_fields(
    ns_id: &str,
    schema: &Value,
    value: &Value,
    path: &[String],
    secrets: &[Vec<String>],
    writable: bool,
    out: &mut Vec<HarnessSettingField>,
) {
    let key_name = path.last().map(String::as_str).unwrap_or("");
    let is_secret = schema["meta"]["role"] == "secret"
        || schema["format"] == "password"
        || secrets.iter().any(|s| path.starts_with(s))
        || key_looks_secret(key_name);
    if !path.is_empty() && is_secret {
        out.push(HarnessSettingField {
            namespace: ns_id.to_owned(),
            path: path.to_vec(),
            key: path.join("."),
            label: schema["title"].as_str().unwrap_or(key_name).to_owned(),
            kind: HarnessSettingKind::Secret,
            display_value: MASKED_SECRET_DISPLAY.to_owned(),
            raw_editable_value: None,
            writable: false,
            is_secret: true,
        });
        return;
    }

    let schema_type = schema["type"].as_str().unwrap_or("");
    if schema_type == "object" {
        if let Some(props) = schema
            .get("properties")
            .or_else(|| schema.get("dict"))
            .and_then(Value::as_object)
        {
            for (k, child) in props {
                let mut next = path.to_vec();
                next.push(k.clone());
                collect_kernel_schema_fields(
                    ns_id,
                    child,
                    value.get(k).unwrap_or(&Value::Null),
                    &next,
                    secrets,
                    writable,
                    out,
                );
            }
        }
        return;
    }
    if path.is_empty() {
        return;
    }
    let enum_opts: Option<Vec<String>> = schema
        .get("enum")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty());
    let kind = if let Some(opts) = enum_opts {
        HarnessSettingKind::Enum(opts)
    } else {
        match schema_type {
            "integer" | "natural" => HarnessSettingKind::Integer,
            "boolean" => HarnessSettingKind::Boolean,
            "string" => HarnessSettingKind::String,
            _ => HarnessSettingKind::ReadOnly,
        }
    };
    let field_writable =
        writable && schema["readOnly"] != true && !matches!(kind, HarnessSettingKind::ReadOnly);
    out.push(HarnessSettingField {
        namespace: ns_id.to_owned(),
        path: path.to_vec(),
        key: path.join("."),
        label: schema["title"].as_str().unwrap_or(key_name).to_owned(),
        kind,
        display_value: value_to_display(value),
        raw_editable_value: Some(value.clone()),
        writable: field_writable,
        is_secret: false,
    });
}

fn parse_kernel_namespaces(
    raw: &Value,
    can_mutate: bool,
) -> Option<(String, String, u64, bool, Vec<HarnessSettingField>)> {
    let namespaces = raw.get("namespaces")?.as_array()?;
    let writable = raw.get("writable")?.as_bool()? && can_mutate;
    let mut fields = Vec::new();
    let mut first_ns = String::new();
    let mut first_rev = 0u64;
    for ns in namespaces {
        let ns_id = ns.get("ns")?.as_str()?.to_owned();
        let rev = ns.get("revision")?.as_u64()?;
        if first_ns.is_empty() {
            first_ns = ns_id.clone();
            first_rev = rev;
        }
        let secrets: Vec<Vec<String>> = ns
            .get("secrets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| serde_json::from_value(s["path"].clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        collect_kernel_schema_fields(
            &ns_id,
            &ns["schema"],
            &ns["value"],
            &[],
            &secrets,
            writable,
            &mut fields,
        );
    }
    Some((
        first_ns.clone(),
        format!("Kernel · {first_ns}"),
        first_rev,
        !writable,
        fields,
    ))
}

impl HarnessSettingsBinding {
    pub fn new(
        runtime: Arc<SessionRuntime>,
        handle: Handle,
        wake: Arc<dyn UiWakeNotifier>,
    ) -> Arc<Self> {
        let state = Arc::new(Mutex::new(HarnessSettingsState {
            loaded: false,
            namespace: "execution".to_owned(),
            namespace_label: "Agent 运行策略 (Harness)".to_owned(),
            revision: 0,
            read_only: false,
            kernel_format: false,
            fields: Vec::new(),
            last_status: None,
            last_error: None,
            conflict_revision: None,
            ui_revision: 1,
        }));
        let mut store_rx = runtime.subscribe();
        let wake_w = Arc::clone(&wake);
        let state_w = Arc::clone(&state);
        let watcher = handle.spawn(async move {
            while store_rx.changed().await.is_ok() {
                state_w.lock().expect("settings state lock").ui_revision += 1;
                wake_w.wake();
            }
        });
        Arc::new(Self {
            runtime,
            handle,
            wake,
            state,
            _watcher: watcher,
        })
    }

    pub fn snapshot(&self) -> HarnessSettingsState {
        self.state.lock().expect("settings state lock").clone()
    }

    pub fn ui_revision(&self) -> u64 {
        self.state.lock().expect("settings state lock").ui_revision
    }

    fn apply_describe_payload(&self, raw: &Value, can_mutate: bool) -> Result<()> {
        let mut guard = self.state.lock().expect("settings state lock");
        if let Some((ns, label, rev, read_only, fields)) = parse_fixture_namespaces(raw) {
            guard.loaded = true;
            guard.namespace = ns;
            guard.namespace_label = label;
            guard.revision = rev;
            guard.read_only = read_only;
            guard.kernel_format = false;
            guard.fields = fields;
            guard.last_error = None;
            guard.conflict_revision = None;
            guard.last_status = Some(format!("已同步策略 (rev {rev})"));
            guard.ui_revision = guard.ui_revision.saturating_add(1);
            return Ok(());
        }
        if let Some((ns, label, rev, read_only, fields)) = parse_kernel_namespaces(raw, can_mutate)
        {
            guard.loaded = true;
            guard.namespace = ns;
            guard.namespace_label = label;
            guard.revision = rev;
            guard.read_only = read_only;
            guard.kernel_format = true;
            guard.fields = fields;
            guard.last_error = None;
            guard.conflict_revision = None;
            guard.last_status = Some(format!("已同步 Kernel 策略 (rev {rev})"));
            guard.ui_revision = guard.ui_revision.saturating_add(1);
            return Ok(());
        }
        Err(Error::Protocol(
            "unrecognized settings.describe response".into(),
        ))
    }

    pub fn refresh_blocking(&self) -> Result<()> {
        let (epoch, can_mutate) = self.runtime.inspect(|state| {
            (
                state.connection_epoch,
                state.endpoint_capabilities.supports("settings.mutate"),
            )
        });
        let rpc_id = format!(
            "settings-desc-{}",
            NEXT_SETTINGS_RPC.fetch_add(1, Ordering::Relaxed)
        );
        let runtime = Arc::clone(&self.runtime);
        let rt_check = Arc::clone(&self.runtime);
        let res = self.handle.block_on(async move {
            runtime
                .call_optional_at_epoch_with_validation(
                    epoch,
                    "settings.describe",
                    json!({}),
                    &rpc_id,
                    move || {
                        rt_check.inspect(|s| {
                            require(
                                s.connection == ConnectionState::Ready
                                    && s.connection_epoch == epoch,
                                "settings connection epoch changed",
                            )
                        })
                    },
                )
                .await
        });
        match res {
            Ok(raw) => {
                self.apply_describe_payload(&raw, can_mutate)?;
                self.wake.wake();
                Ok(())
            }
            Err(err) => {
                let mut guard = self.state.lock().expect("settings state lock");
                guard.last_error = Some(err.to_string());
                guard.ui_revision = guard.ui_revision.saturating_add(1);
                drop(guard);
                self.wake.wake();
                Err(err)
            }
        }
    }

    pub fn refresh(&self) {
        let runtime = Arc::clone(&self.runtime);
        let state = Arc::clone(&self.state);
        let wake = Arc::clone(&self.wake);
        let (epoch, can_mutate) = runtime.inspect(|s| {
            (
                s.connection_epoch,
                s.endpoint_capabilities.supports("settings.mutate"),
            )
        });
        let rpc_id = format!(
            "settings-desc-{}",
            NEXT_SETTINGS_RPC.fetch_add(1, Ordering::Relaxed)
        );
        self.handle.spawn(async move {
            let rt_check = Arc::clone(&runtime);
            let res = runtime
                .call_optional_at_epoch_with_validation(
                    epoch,
                    "settings.describe",
                    json!({}),
                    &rpc_id,
                    move || {
                        rt_check.inspect(|s| {
                            require(
                                s.connection == ConnectionState::Ready
                                    && s.connection_epoch == epoch,
                                "settings connection epoch changed",
                            )
                        })
                    },
                )
                .await;
            let mut guard = state.lock().expect("settings state lock");
            match res {
                Ok(raw) => {
                    if let Some((ns, label, rev, read_only, fields)) =
                        parse_fixture_namespaces(&raw)
                    {
                        guard.loaded = true;
                        guard.namespace = ns;
                        guard.namespace_label = label;
                        guard.revision = rev;
                        guard.read_only = read_only;
                        guard.kernel_format = false;
                        guard.fields = fields;
                        guard.last_error = None;
                        guard.conflict_revision = None;
                        guard.last_status = Some(format!("已同步策略 (rev {rev})"));
                    } else if let Some((ns, label, rev, read_only, fields)) =
                        parse_kernel_namespaces(&raw, can_mutate)
                    {
                        guard.loaded = true;
                        guard.namespace = ns;
                        guard.namespace_label = label;
                        guard.revision = rev;
                        guard.read_only = read_only;
                        guard.kernel_format = true;
                        guard.fields = fields;
                        guard.last_error = None;
                        guard.conflict_revision = None;
                        guard.last_status = Some(format!("已同步 Kernel 策略 (rev {rev})"));
                    } else {
                        guard.last_error = Some("无法解析 settings.describe 响应".to_owned());
                    }
                }
                Err(err) => {
                    guard.last_error = Some(err.to_string());
                }
            }
            guard.ui_revision = guard.ui_revision.saturating_add(1);
            drop(guard);
            wake.wake();
        });
    }

    pub fn apply_patch_at_revision_blocking(
        &self,
        patch: BTreeMap<String, Value>,
        expected_revision_override: Option<u64>,
    ) -> Result<u64> {
        let (ns, rev, read_only, kernel_format, fields) = {
            let guard = self.state.lock().expect("settings state lock");
            (
                guard.namespace.clone(),
                expected_revision_override.unwrap_or(guard.revision),
                guard.read_only,
                guard.kernel_format,
                guard.fields.clone(),
            )
        };
        if read_only {
            let msg = "设置当前为只读状态 (readOnly)，拒绝写入".to_owned();
            let mut guard = self.state.lock().expect("settings state lock");
            guard.last_error = Some(msg.clone());
            guard.ui_revision += 1;
            drop(guard);
            self.wake.wake();
            return Err(Error::Protocol(msg));
        }
        for key in patch.keys() {
            if fields.iter().any(|f| &f.key == key && f.is_secret) || key_looks_secret(key) {
                return Err(Error::Protocol("秘钥字段禁止经由表单明文写入".into()));
            }
        }

        let payload = if kernel_format {
            let ops: Vec<Value> = patch
                .iter()
                .map(|(k, v)| {
                    let path: Vec<String> = k.split('.').map(str::to_owned).collect();
                    json!({ "op": "set", "path": path, "value": v })
                })
                .collect();
            json!({
                "ns": ns,
                "expectedRevision": rev,
                "operations": ops,
            })
        } else {
            json!({
                "namespace": ns,
                "expectedRevision": rev,
                "values": patch,
            })
        };

        let epoch = self.runtime.inspect(|s| s.connection_epoch);
        let rpc_id = format!(
            "settings-mut-{}",
            NEXT_SETTINGS_RPC.fetch_add(1, Ordering::Relaxed)
        );
        let runtime = Arc::clone(&self.runtime);
        let rt_check = Arc::clone(&self.runtime);
        let res = self.handle.block_on(async move {
            runtime
                .call_optional_at_epoch_with_validation(
                    epoch,
                    "settings.mutate",
                    payload,
                    &rpc_id,
                    move || {
                        rt_check.inspect(|s| {
                            require(
                                s.connection == ConnectionState::Ready
                                    && s.connection_epoch == epoch,
                                "settings connection epoch changed before mutate",
                            )
                        })
                    },
                )
                .await
        });

        match res {
            Ok(reply) => {
                let new_rev = reply["revision"].as_u64().unwrap_or(rev + 1);
                let _ = self.refresh_blocking();
                Ok(new_rev)
            }
            Err(Error::Remote(body)) if body["code"] == "conflict" => {
                let server_rev = body["currentRevision"].as_u64();
                let mut guard = self.state.lock().expect("settings state lock");
                guard.conflict_revision = server_rev;
                guard.last_error = Some(format!(
                    "CAS 版本冲突：提交 rev {rev} 已过期（远端当前 rev {}），请先刷新。",
                    server_rev
                        .map(|r| r.to_string())
                        .unwrap_or_else(|| "?".to_owned())
                ));
                guard.ui_revision += 1;
                drop(guard);
                self.wake.wake();
                Err(Error::Remote(body))
            }
            Err(err) => {
                let mut guard = self.state.lock().expect("settings state lock");
                guard.last_error = Some(err.to_string());
                guard.ui_revision += 1;
                drop(guard);
                self.wake.wake();
                Err(err)
            }
        }
    }

    pub fn set_field_value(&self, key: &str, value: Value) {
        let mut patch = BTreeMap::new();
        patch.insert(key.to_owned(), value);
        let _ = self.apply_patch_at_revision_blocking(patch, None);
    }

    /// Non-blocking CAS settings mutation safe to invoke directly from UI action handlers.
    pub fn apply_patch_async(&self, patch: BTreeMap<String, Value>) {
        let (ns, rev, read_only, kernel_format, fields) = {
            let guard = self.state.lock().expect("settings state lock");
            (
                guard.namespace.clone(),
                guard.revision,
                guard.read_only,
                guard.kernel_format,
                guard.fields.clone(),
            )
        };
        if read_only {
            let mut guard = self.state.lock().expect("settings state lock");
            guard.last_error = Some("设置当前为只读状态 (readOnly)，拒绝写入".to_owned());
            guard.ui_revision += 1;
            drop(guard);
            self.wake.wake();
            return;
        }
        for key in patch.keys() {
            if fields.iter().any(|f| &f.key == key && f.is_secret) || key_looks_secret(key) {
                let mut guard = self.state.lock().expect("settings state lock");
                guard.last_error = Some("秘钥字段禁止经由表单明文写入".to_owned());
                guard.ui_revision += 1;
                drop(guard);
                self.wake.wake();
                return;
            }
        }
        let payload = if kernel_format {
            let ops: Vec<Value> = patch
                .iter()
                .map(|(k, v)| {
                    let path: Vec<String> = k.split('.').map(str::to_owned).collect();
                    json!({ "op": "set", "path": path, "value": v })
                })
                .collect();
            json!({
                "ns": ns,
                "expectedRevision": rev,
                "operations": ops,
            })
        } else {
            json!({
                "namespace": ns,
                "expectedRevision": rev,
                "values": patch,
            })
        };
        let runtime = Arc::clone(&self.runtime);
        let state = Arc::clone(&self.state);
        let wake = Arc::clone(&self.wake);
        let epoch = runtime.inspect(|s| s.connection_epoch);
        let rpc_id = format!(
            "settings-mut-{}",
            NEXT_SETTINGS_RPC.fetch_add(1, Ordering::Relaxed)
        );
        self.handle.spawn(async move {
            let rt_check = Arc::clone(&runtime);
            let res = runtime
                .call_optional_at_epoch_with_validation(
                    epoch,
                    "settings.mutate",
                    payload,
                    &rpc_id,
                    move || {
                        rt_check.inspect(|s| {
                            require(
                                s.connection == ConnectionState::Ready
                                    && s.connection_epoch == epoch,
                                "settings connection epoch changed before mutate",
                            )
                        })
                    },
                )
                .await;
            match res {
                Ok(_) => {
                    let can_mutate =
                        runtime.inspect(|s| s.endpoint_capabilities.supports("settings.mutate"));
                    let rt_check2 = Arc::clone(&runtime);
                    let desc_id = format!(
                        "settings-desc-{}",
                        NEXT_SETTINGS_RPC.fetch_add(1, Ordering::Relaxed)
                    );
                    if let Ok(raw) = runtime
                        .call_optional_at_epoch_with_validation(
                            epoch,
                            "settings.describe",
                            json!({}),
                            &desc_id,
                            move || {
                                rt_check2.inspect(|s| {
                                    require(
                                        s.connection == ConnectionState::Ready
                                            && s.connection_epoch == epoch,
                                        "settings connection epoch changed",
                                    )
                                })
                            },
                        )
                        .await
                    {
                        let mut guard = state.lock().expect("settings state lock");
                        if let Some((ns, label, new_rev, ro, fields)) =
                            parse_fixture_namespaces(&raw)
                        {
                            guard.loaded = true;
                            guard.namespace = ns;
                            guard.namespace_label = label;
                            guard.revision = new_rev;
                            guard.read_only = ro;
                            guard.kernel_format = false;
                            guard.fields = fields;
                            guard.last_error = None;
                            guard.conflict_revision = None;
                            guard.last_status = Some(format!("已更新并同步 (rev {new_rev})"));
                        } else if let Some((ns, label, new_rev, ro, fields)) =
                            parse_kernel_namespaces(&raw, can_mutate)
                        {
                            guard.loaded = true;
                            guard.namespace = ns;
                            guard.namespace_label = label;
                            guard.revision = new_rev;
                            guard.read_only = ro;
                            guard.kernel_format = true;
                            guard.fields = fields;
                            guard.last_error = None;
                            guard.conflict_revision = None;
                            guard.last_status = Some(format!("已更新并同步 (rev {new_rev})"));
                        }
                        guard.ui_revision += 1;
                    }
                }
                Err(Error::Remote(body)) if body["code"] == "conflict" => {
                    let server_rev = body["currentRevision"].as_u64();
                    let mut guard = state.lock().expect("settings state lock");
                    guard.conflict_revision = server_rev;
                    guard.last_error = Some(format!(
                        "CAS 版本冲突：提交 rev {rev} 已过期（远端当前 rev {}），请先刷新。",
                        server_rev
                            .map(|r| r.to_string())
                            .unwrap_or_else(|| "?".to_owned())
                    ));
                    guard.ui_revision += 1;
                }
                Err(err) => {
                    let mut guard = state.lock().expect("settings state lock");
                    guard.last_error = Some(err.to_string());
                    guard.ui_revision += 1;
                }
            }
            wake.wake();
        });
    }
}
