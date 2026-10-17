//! No-UI batch client. Configuration and credentials arrive only through stdin, never argv/logs.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_contract::*;
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_presentation::{
    ComposerDestination, ComposerDraft, RunController, RunScope, SideChatController,
};
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
    let runtime = Arc::new(SessionRuntime::spawn(identity, connector));
    runtime.wait_ready(Duration::from_secs(30)).await?;
    println!(
        "{}",
        json!({"ready":true,"kind":kind,"version":runtime.snapshot().host.as_ref().map(|h|h.product.version.clone())})
    );
    let presentation = RunController::new(runtime.clone())?;
    let side_chats = SideChatController::new(runtime.clone());
    let mut variables = BTreeMap::<String, Value>::new();
    for command in array(&config, "commands")? {
        let operation = string(command, "op")?;
        let id = command["sessionId"]
            .as_str()
            .map(|id| variables.get(id).and_then(Value::as_str).unwrap_or(id));
        let output = match operation {
            "sideChatCreate" | "sideChatCreateError" => {
                let parent = RunScope::main(
                    identity,
                    id.ok_or_else(|| unsupported("sessionId required"))?,
                );
                let result = side_chats
                    .create(
                        parent,
                        command["title"].as_str(),
                        &uuid::Uuid::new_v4().to_string(),
                    )
                    .await;
                if operation == "sideChatCreateError" {
                    json!({"error":result.err().ok_or_else(|| Error::Protocol("expected SideChat failure".into()))?.to_string()})
                } else {
                    let chat = result?;
                    if let Some(name) = command["saveAs"].as_str() {
                        variables.insert(name.into(), json!(chat.scope.session_id));
                    }
                    serde_json::to_value(chat)?
                }
            }
            "sideChatClose" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                side_chats
                    .close(id, &uuid::Uuid::new_v4().to_string())
                    .await?;
                presentation
                    .forget_destination(&ComposerDestination::run(RunScope::main(identity, id)));
                json!({"discarded":true})
            }
            "sideChats" => serde_json::to_value(side_chats.snapshot())?,
            "childCatalog" => {
                let value = serde_json::to_value(
                    runtime
                        .subagent_catalog(id.ok_or_else(|| unsupported("sessionId required"))?)
                        .await?,
                )?;
                if let Some(name) = command["saveAs"].as_str() {
                    variables.insert(
                        name.into(),
                        value
                            .pointer(command["savePointer"].as_str().unwrap_or("/entries/0/id"))
                            .cloned()
                            .ok_or_else(|| Error::Protocol("child pointer missing".into()))?,
                    );
                }
                value
            }
            "childPrompt" => {
                runtime
                    .prompt_child(
                        id.ok_or_else(|| unsupported("sessionId required"))?,
                        command["childSessionId"]
                            .as_str()
                            .map(|id| variables.get(id).and_then(Value::as_str).unwrap_or(id))
                            .ok_or_else(|| unsupported("childSessionId required"))?,
                        PromptMode::Queue,
                        serde_json::from_value(command["content"].clone())?,
                        &uuid::Uuid::new_v4().to_string(),
                    )
                    .await?
            }
            "childCancel" => {
                runtime
                    .cancel_child(
                        id.ok_or_else(|| unsupported("sessionId required"))?,
                        command["childSessionId"]
                            .as_str()
                            .map(|id| variables.get(id).and_then(Value::as_str).unwrap_or(id))
                            .ok_or_else(|| unsupported("childSessionId required"))?,
                        &uuid::Uuid::new_v4().to_string(),
                    )
                    .await?;
                json!({"cancelled":true})
            }
            "presentation" => {
                presentation.select(ComposerDestination::run(RunScope::main(
                    identity,
                    id.ok_or_else(|| unsupported("sessionId required"))?,
                )))?;
                serde_json::to_value(presentation.snapshot().as_ref())?
            }
            "compose" => {
                let scope = RunScope::main(
                    identity,
                    id.ok_or_else(|| unsupported("sessionId required"))?,
                );
                presentation.open(scope.clone()).await?;
                let destination = ComposerDestination::run(scope);
                presentation.edit(
                    &destination,
                    ComposerDraft {
                        text: string(command, "text")?.into(),
                        ..Default::default()
                    },
                )?;
                let ticket = presentation.prepare(&destination, PromptMode::Queue)?;
                let echo = presentation.snapshot();
                require(
                    echo.transcript
                        .items
                        .iter()
                        .any(|i| i.id.starts_with("echo-item:")),
                    "missing optimistic echo",
                )?;
                presentation.dispatch(ticket, None).await?
            }
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
                    variables.insert(name.into(), json!(id));
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
                let minimum_turn = command["minimumTurn"].as_i64().unwrap_or(1);
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
                                            .filter(|e| e["message"]["role"] == "assistant")
                                            .count()
                                            >= minimum_turn as usize
                                    } else {
                                        s.entries.values().any(|e| {
                                            e.event.kind == "turn/end"
                                                && e.event.data["turn"]
                                                    .as_i64()
                                                    .is_some_and(|turn| turn >= minimum_turn)
                                        })
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
            "observeReady" => {
                use futures_util::StreamExt;
                let mut stream = runtime
                    .observe_files(
                        id.ok_or_else(|| unsupported("sessionId required"))?,
                        command["path"].as_str().unwrap_or("."),
                    )
                    .await?;
                tokio::time::timeout(Duration::from_secs(10), stream.next())
                    .await
                    .map_err(|_| Error::Timeout)?
                    .ok_or(Error::Disconnected)??
            }
            "waitGoal" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                let phase = string(command, "phase")?;
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        if let Some(goal) = runtime.goal(id).await?
                            && goal.phase == phase
                        {
                            return serde_json::to_value(goal).map_err(Error::from);
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                })
                .await
                .map_err(|_| Error::Timeout)??
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
            "call" | "callError" => {
                let mut args = resolve_variables(
                    command.get("args").cloned().unwrap_or(json!({})),
                    &variables,
                )?;
                if let Some(id) = id {
                    args["sessionId"] = json!(id);
                }
                let result = runtime
                    .call(
                        string(command, "method")?,
                        args,
                        command["rpcId"]
                            .as_str()
                            .unwrap_or(&uuid::Uuid::new_v4().to_string()),
                    )
                    .await;
                if operation == "callError" {
                    let error = result
                        .err()
                        .ok_or_else(|| Error::Protocol("expected call failure".into()))?;
                    json!({"error":error.to_string()})
                } else {
                    let result = result?;
                    if let Some(name) = command["saveAs"].as_str() {
                        let pointer = command["savePointer"].as_str().unwrap_or("/sessionId");
                        variables.insert(
                            name.into(),
                            result
                                .pointer(pointer)
                                .cloned()
                                .ok_or_else(|| Error::Protocol("save pointer missing".into()))?,
                        );
                    }
                    result
                }
            }
            _ => return Err(unsupported(&format!("headless operation {operation}"))),
        };
        println!("{}", json!({"op":operation,"result":output}));
    }
    runtime.stop().await?;
    Ok(())
}

fn resolve_variables(mut value: Value, variables: &BTreeMap<String, Value>) -> Result<Value> {
    match &mut value {
        Value::String(text) if text.starts_with('@') => {
            return variables
                .get(&text[1..])
                .cloned()
                .ok_or_else(|| Error::Protocol("unknown command variable".into()));
        }
        Value::Array(items) => {
            for item in items {
                *item = resolve_variables(item.take(), variables)?;
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                *item = resolve_variables(item.take(), variables)?;
            }
        }
        _ => {}
    }
    Ok(value)
}
