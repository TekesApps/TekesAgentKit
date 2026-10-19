//! Explicit demo host inputs. Harness adapters and presentation remain the shared implementation.
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_contract::{EndpointConnector, Error, Result, require, string};
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_presentation::{RunController, RunScope};
use tekes_agent_session::SessionRuntime;

// No Debug: connectors/configuration may contain credentials.
enum Backend {
    Native(KernelConnector),
    Pi(PiConfiguration),
    Dsh(DshConfiguration),
}
pub struct EndpointConfiguration {
    scopes: [RunScope; 2],
    backend: Backend,
}
pub struct ConnectedRun {
    pub runtime: Arc<SessionRuntime>,
    pub controller: Arc<RunController>,
    pub scopes: [RunScope; 2],
}
/// A single connector ownership transfer. SharedAgent and RunBinding must use
/// the runtime registered from this connector, never reconnect it independently.
pub struct ConfiguredConnector {
    pub connector: Arc<dyn EndpointConnector>,
    pub kind: &'static str,
    pub scopes: [RunScope; 2],
}

fn invalid(field: &str) -> Error {
    Error::Protocol(format!("invalid endpoint configuration: {field}"))
}
fn nonempty<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let text = string(value, key)?;
    require(!text.trim().is_empty(), &format!("empty {key}"))?;
    Ok(text)
}
fn strings(value: &Value, key: &str) -> Result<Vec<String>> {
    match value.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(|| invalid(key)))
            .collect(),
        _ => Err(invalid(key)),
    }
}
fn mapping(value: &Value, key: &str) -> Result<BTreeMap<String, String>> {
    match value.get(key) {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(items)) => items
            .iter()
            .map(|(name, item)| {
                let text = item.as_str().ok_or_else(|| invalid(key))?;
                require(
                    !name.is_empty() && !name.contains(['=', '\0']) && !text.contains('\0'),
                    "invalid environment entry",
                )?;
                Ok((name.clone(), text.to_owned()))
            })
            .collect(),
        _ => Err(invalid(key)),
    }
}
fn paths(value: &Value, key: &str) -> Result<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = strings(value, key)?
        .into_iter()
        .map(PathBuf::from)
        .collect();
    require(
        paths.iter().all(|path| path.is_absolute()),
        &format!("{key} must be absolute"),
    )?;
    Ok(paths)
}
fn credential(name: &str, env: &impl Fn(&str) -> Option<String>) -> Result<String> {
    require(!name.is_empty(), "empty environment variable name")?;
    let value = env(name).ok_or_else(|| invalid("required environment variable unavailable"))?;
    require(
        !value.trim().is_empty() && !value.contains(['\r', '\n', '\0']),
        "invalid credential environment value",
    )?;
    Ok(value)
}

impl EndpointConfiguration {
    /// Parsing performs no network/process/filesystem I/O. Environment authority is explicit.
    pub fn parse(value: &Value, env: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let kind = match value.get("kind") {
            None => "native", // Preserve the original Native demo configuration.
            Some(Value::String(kind)) => kind.as_str(),
            _ => return Err(invalid("kind")),
        };
        let allowed = match kind {
            "native" => &["url", "bearerEnv"][..],
            "dsh" => &["url", "bearerEnv", "cookieEnv"][..],
            "pi" => &[
                "executable",
                "arguments",
                "dataRoot",
                "environment",
                "environmentFromEnv",
                "extensions",
                "sessionRoots",
            ][..],
            _ => return Err(invalid("unknown endpoint kind")),
        };
        require(
            value.as_object().is_some_and(|object| {
                object.keys().all(|key| {
                    ["kind", "endpointId", "sessions"].contains(&key.as_str())
                        || allowed.contains(&key.as_str())
                })
            }),
            "unknown endpoint configuration field",
        )?;
        let id = nonempty(value, "endpointId")?;
        let sessions = strings(value, "sessions")?;
        require(
            sessions.len() == 2
                && sessions.iter().all(|id| !id.trim().is_empty())
                && sessions[0] != sessions[1],
            "two distinct nonempty sessions required",
        )?;
        let scopes = [
            RunScope::main(id, &sessions[0]),
            RunScope::main(id, &sessions[1]),
        ];
        let backend = match kind {
            "native" => Backend::Native(KernelConnector::new(
                id,
                nonempty(value, "url")?,
                credential(nonempty(value, "bearerEnv")?, &env)?,
            )?),
            "dsh" => {
                let mut config = DshConfiguration::new(id, nonempty(value, "url")?)?;
                require(
                    !(value.get("cookieEnv").is_some() && value.get("bearerEnv").is_some()),
                    "choose one DSH credential source",
                )?;
                config.authorization = if value.get("cookieEnv").is_some() {
                    Some((
                        "Cookie".into(),
                        credential(nonempty(value, "cookieEnv")?, &env)?,
                    ))
                } else if value.get("bearerEnv").is_some() {
                    Some((
                        "Authorization".into(),
                        format!(
                            "Bearer {}",
                            credential(nonempty(value, "bearerEnv")?, &env)?
                        ),
                    ))
                } else {
                    None
                };
                Backend::Dsh(config)
            }
            "pi" => {
                let mut config = PiConfiguration::new(
                    PathBuf::from(nonempty(value, "executable")?),
                    PathBuf::from(nonempty(value, "dataRoot")?),
                );
                require(
                    config.executable.is_absolute() && config.data_root.is_absolute(),
                    "Pi executable/dataRoot must be absolute",
                )?;
                config.identity = id.into();
                config.arguments = strings(value, "arguments")?;
                require(
                    config.arguments.iter().all(|arg| !arg.contains('\0')),
                    "invalid Pi argument",
                )?;
                config.environment = mapping(value, "environment")?;
                for (target, source) in mapping(value, "environmentFromEnv")? {
                    require(
                        !config.environment.contains_key(&target),
                        "duplicate Pi environment target",
                    )?;
                    let content = env(&source)
                        .ok_or_else(|| invalid("required environment variable unavailable"))?;
                    require(!content.contains('\0'), "invalid Pi environment value")?;
                    config.environment.insert(target, content);
                }
                config.extensions = paths(value, "extensions")?;
                config.session_roots = paths(value, "sessionRoots")?;
                Backend::Pi(config)
            }
            _ => unreachable!(),
        };
        Ok(Self { scopes, backend })
    }

    pub fn scopes(&self) -> &[RunScope; 2] {
        &self.scopes
    }

    pub async fn into_connector(self) -> Result<ConfiguredConnector> {
        let (kind, connector): (_, Arc<dyn EndpointConnector>) = match self.backend {
            Backend::Native(config) => ("native", Arc::new(config)),
            Backend::Pi(config) => ("pi", Arc::new(PiConnector::new(config).await?)),
            Backend::Dsh(config) => ("dsh", Arc::new(DshConnector::new(config))),
        };
        Ok(ConfiguredConnector {
            connector,
            kind,
            scopes: self.scopes,
        })
    }
    /// Open an existing session only; never provision sessions/models or retry a mutation here.
    pub async fn connect(self) -> Result<ConnectedRun> {
        let ConfiguredConnector {
            connector, scopes, ..
        } = self.into_connector().await?;
        let runtime = Arc::new(SessionRuntime::spawn(&scopes[0].endpoint_id, connector));
        let started = async {
            runtime.wait_ready(Duration::from_secs(30)).await?;
            let controller = Arc::new(RunController::new(runtime.clone())?);
            controller.open(scopes[0].clone()).await?;
            let mut changes = controller.subscribe();
            tokio::time::timeout(
                Duration::from_secs(10),
                changes.wait_for(|snapshot| !snapshot.transcript.is_stale),
            )
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)?;
            Ok::<_, Error>(controller)
        }
        .await;
        match started {
            Ok(controller) => Ok(ConnectedRun {
                runtime,
                controller,
                scopes,
            }),
            Err(error) => {
                let _ = runtime.stop().await;
                Err(error)
            }
        }
    }
}
