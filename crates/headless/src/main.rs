//! No-UI batch client. Configuration and credentials arrive only through stdin, never argv/logs.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_contract::*;
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_session::SessionRuntime;
#[tokio::main]
async fn main() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut input)
        .map_err(|_| Error::Transport("stdin unavailable".into()))?;
    require(input.len() <= 16 * 1024 * 1024, "headless input too large")?;
    let config: Value = serde_json::from_str(&input)?;
    let kind = string(&config, "kind")?;
    let identity = config["identity"].as_str().unwrap_or(kind);
    let connector: Arc<dyn EndpointConnector> = match kind {
        "native" => Arc::new(KernelConnector::new(
            identity,
            string(&config, "url")?,
            string(&config, "bearer")?.into(),
        )?),
        "pi" => {
            let mut pi = PiConfiguration::new(
                PathBuf::from(string(&config, "executable")?),
                PathBuf::from(string(&config, "dataRoot")?),
            );
            pi.identity = identity.into();
            pi.arguments =
                serde_json::from_value(config.get("arguments").cloned().unwrap_or(json!([])))?;
            pi.environment =
                serde_json::from_value(config.get("environment").cloned().unwrap_or(json!({})))?;
            pi.extensions =
                serde_json::from_value(config.get("extensions").cloned().unwrap_or(json!([])))?;
            pi.session_roots =
                serde_json::from_value(config.get("sessionRoots").cloned().unwrap_or(json!([])))?;
            Arc::new(PiConnector::new(pi).await?)
        }
        "dsh" => {
            let mut dsh = DshConfiguration::new(identity, string(&config, "url")?)?;
            if let Some(cookie) = config["cookie"].as_str() {
                dsh.authorization = Some(("Cookie".into(), cookie.into()));
            } else if let Some(token) = config["bearer"].as_str() {
                dsh.authorization = Some(("Authorization".into(), format!("Bearer {token}")));
            }
            Arc::new(DshConnector::new(dsh))
        }
        _ => return Err(unsupported("unknown endpoint kind")),
    };
    let runtime = SessionRuntime::spawn(identity, connector);
    runtime.wait_ready(Duration::from_secs(30)).await?;
    println!(
        "{}",
        json!({"ready":true,"kind":kind,"version":runtime.snapshot().host.as_ref().map(|h|h.product.version.clone())})
    );
    let mut variables = BTreeMap::<String, String>::new();
    for command in array(&config, "commands")? {
        let operation = string(command, "op")?;
        let id = command["sessionId"]
            .as_str()
            .map(|id| variables.get(id).map(String::as_str).unwrap_or(id));
        let output = match operation {
            "list" => runtime.inspect(|s| {
                json!(
                    s.sessions
                        .values()
                        .filter_map(|s| s.summary.clone())
                        .collect::<Vec<_>>()
                )
            }),
            "create" => {
                let id = runtime
                    .create_session(string(command, "cwd")?, &uuid::Uuid::new_v4().to_string())
                    .await?;
                if let Some(name) = command["saveAs"].as_str() {
                    variables.insert(name.into(), id.clone());
                }
                json!({"sessionId":id})
            }
            "open" => {
                runtime
                    .open_session(id.ok_or_else(|| unsupported("sessionId required"))?)
                    .await?;
                Value::Null
            }
            "close" => {
                runtime
                    .close_session(id.ok_or_else(|| unsupported("sessionId required"))?)
                    .await?;
                Value::Null
            }
            "earlier" => json!(
                runtime
                    .load_earlier(id.ok_or_else(|| unsupported("sessionId required"))?)
                    .await?
            ),
            "reconnect" => {
                runtime.reconnect().await?;
                Value::Null
            }
            "snapshot" => serde_json::to_value(&*runtime.snapshot())?,
            "waitSettled" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                let mut changes = runtime.subscribe();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let settled = changes.inspect(|s| {
                            s.sessions.get(id).is_some_and(|s| {
                                s.journal_ready
                                    && !s.run_open
                                    && if let Some(native) = &s.native_conversation {
                                        native
                                            .entries
                                            .iter()
                                            .any(|e| e["message"]["role"] == "assistant")
                                    } else {
                                        s.entries.values().any(|e| e.event.kind == "turn/end")
                                    }
                            })
                        });
                        if settled {
                            break;
                        }
                        changes.changed().await.map_err(|_| Error::Disconnected)?;
                    }
                    Ok::<_, Error>(())
                })
                .await
                .map_err(|_| Error::Timeout)??;
                Value::Null
            }
            "promptAndAnswer" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                let payload = json!({"sessionId":id,"mode":"queue","content":[{"type":"text","text":string(command,"text")?}]});
                let key = uuid::Uuid::new_v4().to_string();
                let prompt = runtime.call("session.prompt", payload, &key);
                let answer = async {
                    let mut changes = runtime.subscribe();
                    loop {
                        let pending = changes.inspect(|s| {
                            s.actionables
                                .values()
                                .find(|a| a["sessionId"] == id)
                                .cloned()
                        });
                        if let Some(pending) = pending {
                            runtime
                                .respond(
                                    string(&pending, "id")?,
                                    integer(&pending, "revision")?,
                                    command["outcome"].clone(),
                                )
                                .await?;
                            return Ok::<_, Error>(());
                        }
                        changes.changed().await.map_err(|_| Error::Disconnected)?;
                    }
                };
                let (response, ()) = tokio::time::timeout(Duration::from_secs(20), async {
                    tokio::try_join!(prompt, answer)
                })
                .await
                .map_err(|_| Error::Timeout)??;
                response
            }
            "call" => {
                let mut args = command.get("args").cloned().unwrap_or(json!({}));
                if let Some(id) = id {
                    args["sessionId"] = json!(id);
                }
                runtime
                    .call(
                        string(command, "method")?,
                        args,
                        command["rpcId"]
                            .as_str()
                            .unwrap_or(&uuid::Uuid::new_v4().to_string()),
                    )
                    .await?
            }
            _ => return Err(unsupported(&format!("headless operation {operation}"))),
        };
        println!("{}", json!({"op":operation,"result":output}));
    }
    runtime.stop().await?;
    Ok(())
}
