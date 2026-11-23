//! Ephemeral host-owned local `tekes-supervisor` launcher for `--live-kernel` in `tekes-agent-ui-demo`.
//! Keeps credentials strictly in memory / private bootstrap pipe (`TEKES-KERNEL-BOOTSTRAP/1`).

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct LiveKernelOptions {
    pub cwd: PathBuf,
    pub kernel_data_root: PathBuf,
    pub kernel_bin_dir: Option<PathBuf>,
    pub settings_path: PathBuf,
    pub credential_env: String,
}

impl LiveKernelOptions {
    pub fn with_defaults(
        cwd: Option<PathBuf>,
        kernel_data_root: Option<PathBuf>,
        kernel_bin_dir: Option<PathBuf>,
        settings_path: Option<PathBuf>,
        credential_env: Option<String>,
    ) -> Result<Self> {
        let home = home_dir()?;
        let cwd = match cwd {
            Some(p) => p,
            None => std::env::current_dir().context("resolve current working directory")?,
        };
        let cwd = fs::canonicalize(&cwd).unwrap_or(cwd);
        let kernel_data_root = kernel_data_root.unwrap_or_else(|| {
            home.join("Library/Application Support/TekesAgentKitDemo/kernel-0.9")
        });
        let settings_path = settings_path.unwrap_or_else(|| {
            home.join("Library/Application Support/Tekes/Settings/providers-and-routers.json")
        });
        let credential_env = credential_env.unwrap_or_else(|| "DEEPSEEK_KEY".to_owned());
        Ok(Self {
            cwd,
            kernel_data_root,
            kernel_bin_dir,
            settings_path,
            credential_env,
        })
    }
}

pub struct LiveKernelProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    pub url: String,
    pub bearer: String,
}

impl std::fmt::Debug for LiveKernelProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveKernelProcess")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl Drop for LiveKernelProcess {
    fn drop(&mut self) {
        // Closing stdin signals `tekes-supervisor` to exit gracefully.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl LiveKernelProcess {
    pub fn launch(options: &LiveKernelOptions) -> Result<Self> {
        let bin_dir = resolve_kernel_bin_dir(options.kernel_bin_dir.as_deref())?;
        let supervisor = bin_dir.join("tekes-supervisor");
        let worker = bin_dir.join("tekes-worker");

        ensure_private_dir(&options.kernel_data_root)?;
        let runtime_dir = options.kernel_data_root.join("runtime");
        ensure_private_dir(&runtime_dir)?;

        let projected =
            project_providers_and_credentials(&options.settings_path, &options.credential_env)?;

        let launch_json_path = runtime_dir.join("kernel-launch.json");
        let launch_doc = json!({
            "format": 1,
            "bootstrap_version": 1,
            "root": options.kernel_data_root,
            "worker": worker,
            "listen": "127.0.0.1:0",
            "configuration_mode": "provided",
            "providers": projected.providers,
            "credential_bindings": projected.credential_bindings,
        });
        write_private_json(&launch_json_path, &launch_doc)?;

        let stderr_path = runtime_dir.join("kernel-stderr.log");
        let stderr_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&stderr_path)
            .with_context(|| format!("open {}", stderr_path.display()))?;

        let token_bytes = read_urandom_32()?;
        let token_hex = hex_encode(&token_bytes);
        let bearer = base64url_encode(&token_bytes);

        let mut cmd = Command::new(&supervisor);
        cmd.arg("--built-in").arg(&launch_json_path);
        cmd.env_clear();
        if let Some(home) = std::env::var_os("HOME") {
            cmd.env("HOME", home);
        }
        if let Some(tmpdir) = std::env::var_os("TMPDIR") {
            cmd.env("TMPDIR", tmpdir);
        }
        cmd.env("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin");
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_file));

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn {}", supervisor.display()))?;
        let mut child_stdin = child
            .stdin
            .take()
            .context("tekes-supervisor stdin unavailable")?;
        let child_stdout = child
            .stdout
            .take()
            .context("tekes-supervisor stdout unavailable")?;

        let bootstrap = json!({
            "endpointToken": token_hex,
            "credentials": projected.credentials,
        });
        let bootstrap_bytes = serde_json::to_vec(&bootstrap)?;
        let len_u32 = u32::try_from(bootstrap_bytes.len())
            .context("Kernel bootstrap frame exceeded u32 length")?;

        if let Err(err) = (|| -> std::io::Result<()> {
            child_stdin.write_all(b"TEKES-KERNEL-BOOTSTRAP/1\n")?;
            child_stdin.write_all(&len_u32.to_be_bytes())?;
            child_stdin.write_all(&bootstrap_bytes)?;
            child_stdin.flush()
        })() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow::Error::new(err).context("write TEKES-KERNEL-BOOTSTRAP/1 frame"));
        }

        let (tx, rx) = mpsc::channel::<Result<String>>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(child_stdout);
            let mut first_line = String::new();
            match reader.read_line(&mut first_line) {
                Ok(0) => {
                    let _ = tx.send(Err(anyhow::anyhow!(
                        "tekes-supervisor exited before readiness"
                    )));
                }
                Ok(_) => {
                    let _ = tx.send(Ok(first_line));
                    let mut sink = String::new();
                    while reader.read_line(&mut sink).unwrap_or(0) > 0 {
                        sink.clear();
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(anyhow::Error::new(err)));
                }
            }
        });

        let line = match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(line)) => line,
            Ok(Err(err)) => {
                let tail = read_stderr_tail(&stderr_path);
                let _ = child.kill();
                let _ = child.wait();
                bail!("{err}; see {}: {tail}", stderr_path.display());
            }
            Err(_) => {
                let tail = read_stderr_tail(&stderr_path);
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "tekes-supervisor readiness timeout (30s); see {}: {tail}",
                    stderr_path.display()
                );
            }
        };

        let ready: Value = serde_json::from_str(line.trim()).with_context(|| {
            format!(
                "invalid tekes-supervisor readiness line; see {}",
                stderr_path.display()
            )
        })?;
        let proto = ready["protocolVersion"].as_u64().unwrap_or(0);
        if proto != 4 {
            let _ = child.kill();
            let _ = child.wait();
            bail!("expected TekesKernel protocolVersion 4, got {proto}");
        }
        let url = ready["url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("missing readiness url from tekes-supervisor")?
            .to_owned();

        Ok(Self {
            child,
            stdin: Some(child_stdin),
            url,
            bearer,
        })
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .context("HOME environment variable is not set")
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(path)
        .with_context(|| format!("create directory {}", path.display()))
}

fn write_private_json(path: &Path, val: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(val)?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(())
}

fn has_kernel_pair(dir: &Path) -> bool {
    dir.join("tekes-supervisor").is_file() && dir.join("tekes-worker").is_file()
}

fn resolve_kernel_bin_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = explicit {
        if has_kernel_pair(dir) {
            return Ok(dir.to_path_buf());
        }
        bail!(
            "--kernel-bin-dir {} missing tekes-supervisor / tekes-worker",
            dir.display()
        );
    }

    if let Ok(home) = home_dir() {
        let releases = home.join("Library/Application Support/TekesWebUI/releases");
        if let Ok(entries) = fs::read_dir(&releases) {
            let mut dirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|entry| entry.path().join("Kernel")))
                .filter(|dir| has_kernel_pair(dir))
                .collect();
            dirs.sort();
            if let Some(newest) = dirs.pop() {
                return Ok(newest);
            }
        }
        let dev_release = home.join("dev/TekesKernel/target/release");
        if has_kernel_pair(&dev_release) {
            return Ok(dev_release);
        }
    }

    bail!("unable to find tekes-supervisor & tekes-worker; pass --kernel-bin-dir <path>")
}

fn adapter_for_provider(provider: &Value, metadata: &Value) -> &'static str {
    if let Some(raw) = metadata.get("adapter").and_then(Value::as_str) {
        match raw {
            "responses" => return "responses",
            "anthropic_messages" => return "anthropic_messages",
            "chat_completions" => return "chat_completions",
            "google_generation" => return "google_generation",
            _ => {}
        }
    }
    match provider.get("api").and_then(Value::as_str).unwrap_or("") {
        "anthropic" => "anthropic_messages",
        "chat-completions" => "chat_completions",
        "google-interactions" => "google_generation",
        _ => "responses",
    }
}

fn dialect_for_provider(provider: &Value, metadata: &Value) -> String {
    if let Some(raw) = metadata
        .get("dialect")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return raw.to_owned();
    }
    match provider.get("api").and_then(Value::as_str).unwrap_or("") {
        "anthropic" => "anthropic_messages_v1".to_owned(),
        "google-interactions" => "google_generation_v1".to_owned(),
        "chat-completions" => "openai_chat_v1".to_owned(),
        "deepseek-responses" => "deepseek_responses_v1".to_owned(),
        _ => "openai_responses_v1".to_owned(),
    }
}

struct ProjectedProviders {
    providers: Value,
    credential_bindings: BTreeMap<String, String>,
    credentials: BTreeMap<String, String>,
}

fn project_providers_and_credentials(
    settings_path: &Path,
    credential_env: &str,
) -> Result<ProjectedProviders> {
    let raw = fs::read_to_string(settings_path).with_context(|| {
        format!(
            "read Tekes provider settings at {}",
            settings_path.display()
        )
    })?;
    let settings: Value = serde_json::from_str(&raw)?;
    let all_providers = settings["providers"]
        .as_array()
        .context("providers-and-routers.json missing `providers` array")?;

    let selected_ids: Vec<String> = settings["agentSelections"]["tekes"]["providerIDs"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let disabled_ids: BTreeSet<String> =
        settings["agentSelections"]["tekes"]["disabledProviderIDs"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

    let mut chosen: Vec<&Value> = all_providers
        .iter()
        .filter(|p| {
            let Some(id) = p.get("id").and_then(Value::as_str) else {
                return false;
            };
            (selected_ids.is_empty() || selected_ids.iter().any(|cand| cand == id))
                && !disabled_ids.contains(id)
        })
        .collect();
    if chosen.is_empty()
        && let Some(first_direct) = all_providers
            .iter()
            .find(|p| p.get("providerID").and_then(Value::as_str) == Some("deepseek"))
    {
        chosen.push(first_direct);
    }
    if chosen.is_empty() {
        bail!(
            "no enabled provider found for `tekes` in {}",
            settings_path.display()
        );
    }

    let mut projected_providers = Vec::new();
    let mut credential_bindings = BTreeMap::new();
    let mut credentials = BTreeMap::new();

    for (idx, provider) in chosen.iter().enumerate() {
        let id = provider["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("provider missing id")?;
        let provider_vendor = provider
            .get("providerID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let metadata = &provider["agentSettings"]["kernel"];
        let dialect = dialect_for_provider(provider, metadata);
        let adapter = adapter_for_provider(provider, metadata);
        let endpoint = provider["baseURL"]
            .as_str()
            .filter(|s| !s.is_empty())
            .with_context(|| format!("provider `{id}` missing baseURL"))?
            .trim_end_matches('/');
        let endpoint_owner = metadata
            .get("endpointOwner")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(if provider_vendor.is_empty() {
                "configured"
            } else {
                provider_vendor
            });
        let gateway_translation = metadata
            .get("gatewayTranslation")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("direct");
        let evidence_revision = metadata
            .get("evidenceRevision")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("configured");

        let mut models = Vec::new();
        if let Some(arr) = provider.get("models").and_then(Value::as_array) {
            for m in arr {
                if m.get("enabled").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                let Some(mid) = m.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let ctx = m
                    .get("contextWindow")
                    .and_then(Value::as_u64)
                    .unwrap_or(128_000);
                let compact = m
                    .get("compactTriggerTokens")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| (ctx * 4) / 5);
                let profile = m
                    .get("kernelProfile")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{dialect}:{mid}"));
                models.push(json!({
                    "id": mid,
                    "enabled": true,
                    "profile": profile,
                    "context_window_tokens": ctx,
                    "compact_trigger_tokens": compact,
                }));
            }
        }
        if models.is_empty() {
            continue;
        }

        let secret =
            resolve_provider_secret(id, provider_vendor, credential_env, chosen.len() == 1)?;
        let binding_name = format!("TEKES_HOST_PRIVATE_CREDENTIAL_{idx}");
        credential_bindings.insert(id.to_owned(), binding_name);
        credentials.insert(id.to_owned(), secret);

        projected_providers.push(json!({
            "id": id,
            "adapter": adapter,
            "dialect": dialect,
            "endpoint": endpoint,
            "endpoint_owner": endpoint_owner,
            "gateway_translation": gateway_translation,
            "evidence_revision": evidence_revision,
            "credential_key": id,
            "models": models,
        }));
    }

    if projected_providers.is_empty() {
        bail!(
            "selected providers in {} have no enabled models",
            settings_path.display()
        );
    }

    Ok(ProjectedProviders {
        providers: json!({
            "format": 1,
            "revision": 1,
            "providers": projected_providers,
        }),
        credential_bindings,
        credentials,
    })
}

fn resolve_provider_secret(
    provider_id: &str,
    provider_vendor: &str,
    credential_env: &str,
    allow_single_env: bool,
) -> Result<String> {
    if allow_single_env
        && !credential_env.trim().is_empty()
        && let Ok(val) = std::env::var(credential_env)
    {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }

    let mut candidates = vec![
        ("com.tekesapps.TekesWebUI.providers", provider_id),
        ("com.tekesapps.TekesDXF.gateway", provider_id),
    ];
    if !provider_vendor.is_empty() {
        candidates.push(("com.tekesapps.TekesUI.model-api-key", provider_vendor));
    }

    for (service, account) in candidates {
        if let Some(secret) = read_keychain_secret(service, account) {
            return Ok(secret);
        }
    }

    bail!(
        "missing API credential for provider `{provider_id}`; set env `{credential_env}` or store key in macOS Keychain"
    )
}

fn read_keychain_secret(service: &str, account: &str) -> Option<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn read_urandom_32() -> Result<[u8; 32]> {
    let mut file = fs::File::open("/dev/urandom").context("open /dev/urandom")?;
    let mut buf = [0_u8; 32];
    file.read_exact(&mut buf).context("read /dev/urandom")?;
    Ok(buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        }
    }
    out
}

fn read_stderr_tail(path: &Path) -> String {
    let Ok(raw) = fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = raw.lines().rev().take(8).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
}
