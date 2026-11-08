//! No-UI batch client. Configuration and credentials arrive only through stdin, never argv/logs.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_contract::*;
use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
use tekes_agent_presentation::{
    ChildHistoryLayout, ChildRun, ComposerDestination, ComposerDraft, RunController, RunScope,
    SideChatController,
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
    let mut child_runs = BTreeMap::<(String, String), ChildRun>::new();
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
            "childCreate" => {
                let child = runtime
                    .create_child(
                        id.ok_or_else(|| unsupported("parent sessionId required"))?,
                        command["label"].as_str(),
                        command["rpcId"]
                            .as_str()
                            .unwrap_or(&uuid::Uuid::new_v4().to_string()),
                    )
                    .await?;
                if let Some(name) = command["saveAs"].as_str() {
                    variables.insert(name.into(), json!(child.session_id));
                }
                serde_json::to_value(child)?
            }
            "childRead" => {
                let child = string(command, "childSessionId")?;
                let child = variables
                    .get(child)
                    .and_then(Value::as_str)
                    .unwrap_or(child);
                runtime
                    .read_child(
                        id.ok_or_else(|| unsupported("parent sessionId required"))?,
                        child,
                    )
                    .await?
            }
            "childHistory" | "childEarlier" => {
                let parent = id
                    .ok_or_else(|| unsupported("parent sessionId required"))?
                    .to_owned();
                let child = string(command, "childSessionId")?;
                let child = variables
                    .get(child)
                    .and_then(Value::as_str)
                    .unwrap_or(child)
                    .to_owned();
                let key = (parent.clone(), child.clone());
                if !child_runs.contains_key(&key) {
                    let layout = if kind == "native" {
                        ChildHistoryLayout::SameSession
                    } else {
                        ChildHistoryLayout::IndependentSessions
                    };
                    let run = ChildRun::open(
                        runtime.clone(),
                        RunScope::main(identity, parent),
                        child,
                        layout,
                        None,
                    )
                    .await?;
                    child_runs.insert(key.clone(), run);
                }
                let run = &child_runs[&key];
                if operation == "childEarlier" {
                    json!({"loaded":run.load_earlier().await?})
                } else {
                    serde_json::to_value(run.transcript().await?)?
                }
            }
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
                        if command["mode"] == "steer" {
                            PromptMode::Steer
                        } else {
                            PromptMode::Queue
                        },
                        serde_json::from_value(command["content"].clone())?,
                        command["rpcId"]
                            .as_str()
                            .unwrap_or(&uuid::Uuid::new_v4().to_string()),
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
            #[cfg(feature = "acceptance-source-witness")]
            "presentationSource" => {
                presentation.select(ComposerDestination::run(RunScope::main(
                    identity,
                    id.ok_or_else(|| unsupported("sessionId required"))?,
                )))?;
                let witness = presentation.refresh_with_source_witness();
                let value = json!({"snapshot":witness.snapshot().as_ref(),"authority":witness.authority(),
                    "projectionSucceeded":witness.projection_succeeded(),
                    "evidenceScope":"original-publication-and-same-memory-input",
                    "serverTransactionalFreshnessProven":false});
                if let Some(name) = command["saveAs"].as_str() {
                    variables.insert(
                        name.into(),
                        value
                            .pointer(string(command, "savePointer")?)
                            .cloned()
                            .ok_or_else(|| unsupported("source witness save pointer missing"))?,
                    );
                }
                value
            }
            #[cfg(feature = "acceptance-source-witness")]
            "waitSourceRunOpen" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                // Subscribe before reading the baseline so an opening transition cannot be
                // lost between the initial projection read and the first notification wait.
                let mut changes = runtime.subscribe();
                let expected_epoch = changes.inspect(source_run_open_epoch)?;
                tokio::time::timeout(Duration::from_secs(15), async {
                    loop {
                        let opened = changes.inspect(|store| {
                            Ok::<_, Error>(
                                source_run_open_observed(store, id, expected_epoch)?
                                    && command["inputText"].as_str().is_none_or(|text| {
                                        source_run_input_observed(store, id, text)
                                    }),
                            )
                        })?;
                        if opened {
                            return Ok::<_, Error>(());
                        }
                        changes.changed().await.map_err(|_| Error::Disconnected)?;
                    }
                })
                .await
                .map_err(|_| Error::Timeout)??;
                json!({"sessionId":id,"connectionEpoch":expected_epoch,"runOpen":true})
            }
            "presentation" => {
                presentation.select(ComposerDestination::run(RunScope::main(
                    identity,
                    id.ok_or_else(|| unsupported("sessionId required"))?,
                )))?;
                let value = serde_json::to_value(presentation.snapshot().as_ref())?;
                if let Some(name) = command["saveAs"].as_str() {
                    variables.insert(
                        name.into(),
                        value
                            .pointer(string(command, "savePointer")?)
                            .cloned()
                            .ok_or_else(|| unsupported("presentation save pointer missing"))?,
                    );
                }
                value
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
                if command["captureNativePreview"] == true {
                    // Capture a publication from the active dispatch, without a second
                    // submission or a timing-based identity association.
                    let mut changes = runtime.subscribe();
                    let capture = tokio::time::timeout(Duration::from_secs(30), async {
                        loop {
                            let observed = changes.inspect(|store| {
                                store
                                    .sessions
                                    .get(id.unwrap())
                                    .filter(|session| {
                                        session.native_events.iter().any(|event| {
                                            event["type"] == "message_update"
                                                && event["message"]["role"] == "assistant"
                                        })
                                    })
                                    .cloned()
                            });
                            if let Some(session) = observed {
                                break Ok::<_, Error>(session);
                            }
                            changes.changed().await.map_err(|_| Error::Disconnected)?;
                        }
                    });
                    let (result, preview) =
                        tokio::join!(presentation.dispatch(ticket, None), capture);
                    let mut result = result?;
                    result["nativePreview"] =
                        serde_json::to_value(preview.map_err(|_| Error::Timeout)??)?;
                    result
                } else {
                    presentation.dispatch(ticket, None).await?
                }
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
            // Fixture-only observation; membership is not archive authority.
            // Public lifecycle acknowledgements are checked separately by the caller.
            "waitInventoryMembership" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                let expected = command["present"]
                    .as_bool()
                    .ok_or_else(|| unsupported("present boolean required"))?;
                let mut changes = runtime.subscribe();
                tokio::time::timeout(Duration::from_secs(15), async {
                    loop {
                        let converged = {
                            let state = runtime.snapshot();
                            let present = state
                                .sessions
                                .get(id)
                                .and_then(|value| value.summary.as_ref())
                                .is_some_and(|summary| summary["status"] != "archived")
                                && !state.archived_session_ids.iter().any(|value| value == id);
                            state.inventory_ready
                                && matches!(
                                    state.connection,
                                    tekes_agent_session::ConnectionState::Ready
                                )
                                && present == expected
                        };
                        if converged {
                            return Ok::<_, tekes_agent_contract::Error>(());
                        }
                        changes
                            .changed()
                            .await
                            .map_err(|_| tekes_agent_contract::Error::Disconnected)?;
                    }
                })
                .await
                .map_err(|_| tekes_agent_contract::Error::Timeout)??;
                json!({"sessionId":id,"present":expected,"archiveAuthorityInferred":false})
            }
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
                                    && if let Some(text) = command["inputText"].as_str() {
                                        tekes_agent_presentation::TranscriptSnapshot::read(
                                            &RunScope::main(identity, id),
                                            s,
                                        )
                                        .is_ok_and(|snapshot| settled_reply_after(&snapshot, text))
                                    } else if let Some(native) = &s.native_conversation {
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
            "waitNativeEvent" => {
                let id = id.ok_or_else(|| unsupported("sessionId required"))?;
                let event_type = string(command, "eventType")?;
                let role = command["role"].as_str();
                let mut changes = runtime.subscribe();
                tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let observed = changes.inspect(|store| {
                            store.sessions.get(id).is_some_and(|session| {
                                session.native_events.iter().any(|event| {
                                    event["type"] == event_type
                                        && role.is_none_or(|role| event["message"]["role"] == role)
                                })
                            })
                        });
                        if observed {
                            break;
                        }
                        changes.changed().await.map_err(|_| Error::Disconnected)?;
                    }
                    Ok::<_, Error>(())
                })
                .await
                .map_err(|_| Error::Timeout)??;
                json!({"observed":true,"eventType":event_type,"role":role})
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

#[cfg(feature = "acceptance-source-witness")]
fn source_run_open_epoch(store: &tekes_agent_session::MemoryProjectionStore) -> Result<u64> {
    if store.connection == tekes_agent_session::ConnectionState::Ready {
        Ok(store.connection_epoch)
    } else {
        Err(Error::Disconnected)
    }
}

#[cfg(feature = "acceptance-source-witness")]
fn source_run_open_observed(
    store: &tekes_agent_session::MemoryProjectionStore,
    session_id: &str,
    expected_epoch: u64,
) -> Result<bool> {
    if store.connection != tekes_agent_session::ConnectionState::Ready
        || store.connection_epoch != expected_epoch
    {
        return Err(Error::Disconnected);
    }
    Ok(store
        .sessions
        .get(session_id)
        .is_some_and(|session| session.run_open))
}

#[cfg(feature = "acceptance-source-witness")]
fn source_run_input_observed(
    store: &tekes_agent_session::MemoryProjectionStore,
    session_id: &str,
    text: &str,
) -> bool {
    store.sessions.get(session_id).is_some_and(|session| {
        tekes_agent_presentation::TranscriptSnapshot::read(
            &RunScope::main(&store.endpoint_id, session_id),
            session,
        )
        .is_ok_and(|snapshot| {
            !snapshot.is_stale
                && snapshot.items.iter().any(|item| {
                    item.kind == tekes_agent_presentation::TranscriptItemKind::User
                        && !item.is_provisional
                        && item.text == text
                })
        })
    })
}

/// Match the latest exact authored input, never an absolute count in a clipped window.
fn settled_reply_after(
    snapshot: &tekes_agent_presentation::TranscriptSnapshot,
    text: &str,
) -> bool {
    use tekes_agent_presentation::TranscriptItemKind;
    let Some(index) = snapshot.items.iter().rposition(|item| {
        item.kind == TranscriptItemKind::User && !item.is_provisional && item.text == text
    }) else {
        return false;
    };
    let turn = &snapshot.items[index].turn_id;
    snapshot.items[index + 1..].iter().any(|item| {
        item.turn_id == *turn
            && item.kind == TranscriptItemKind::Assistant
            && item.is_final
            && !item.is_provisional
    })
}

#[cfg(test)]
mod settled_tests {
    use super::*;
    fn source(
        messages: Vec<(&str, &str)>,
        streaming: bool,
    ) -> tekes_agent_presentation::TranscriptSnapshot {
        let entries:Vec<_>=messages.into_iter().enumerate().map(|(n,(role,text))|json!({"type":"message","id":format!("entry-{n}"),"message":{"role":role,"timestamp":n,"content":text}})).collect();
        let session = tekes_agent_session::SessionProjection {
            journal_ready: true,
            native_conversation: Some(NativeConversationSnapshot {
                session_id: "s".into(),
                generation: "g".into(),
                leaf_id: entries
                    .last()
                    .and_then(|entry| entry["id"].as_str())
                    .map(str::to_owned),
                entries,
                has_more_before: true,
                is_streaming: streaming,
                state: json!({}),
            }),
            ..Default::default()
        };
        tekes_agent_presentation::TranscriptSnapshot::read(&RunScope::main("e", "s"), &session)
            .unwrap()
    }
    #[test]
    fn exact_input_requires_its_own_completed_reply_in_the_current_window() {
        assert!(!settled_reply_after(
            &source(vec![("user", "old"), ("assistant", "reply")], false),
            "target"
        ));
        assert!(!settled_reply_after(
            &source(vec![("user", "target")], false),
            "target"
        ));
        assert!(!settled_reply_after(
            &source(vec![("user", "target"), ("assistant", "reply")], true),
            "target"
        ));
        assert!(settled_reply_after(
            &source(vec![("user", "target"), ("assistant", "reply")], false),
            "target"
        ));
    }
    #[test]
    fn another_turn_and_an_older_identical_input_cannot_satisfy_the_wait() {
        assert!(!settled_reply_after(
            &source(
                vec![
                    ("user", "target"),
                    ("user", "other"),
                    ("assistant", "reply")
                ],
                false
            ),
            "target"
        ));
        assert!(!settled_reply_after(
            &source(
                vec![
                    ("user", "target"),
                    ("assistant", "reply"),
                    ("user", "target")
                ],
                false
            ),
            "target"
        ));
    }
}

#[cfg(all(test, feature = "acceptance-source-witness"))]
mod source_run_open_tests {
    use super::*;

    fn ready_store() -> tekes_agent_session::MemoryProjectionStore {
        let mut store = tekes_agent_session::MemoryProjectionStore::default();
        store.connection = tekes_agent_session::ConnectionState::Ready;
        store.connection_epoch = 7;
        store
    }

    #[test]
    fn optional_running_input_requires_the_original_authored_user_in_a_ready_projection() {
        let mut store = ready_store();
        let session = tekes_agent_session::SessionProjection {
            native_conversation: Some(serde_json::from_value(json!({
            "sessionId":"child","generation":"g","leafId":null,"hasMoreBefore":false,
            "isStreaming":true,"state":{},"entries":[
                {"id":"assistant","type":"message","message":{"role":"assistant","content":[{"type":"text","text":"marker"}]}}
            ]
        })).unwrap()),
            journal_ready: true,
            ..Default::default()
        };
        store.sessions.insert("child".into(), session);
        assert!(!source_run_input_observed(&store, "child", "marker"));
        store.sessions.get_mut("child").unwrap().native_conversation.as_mut().unwrap().entries.push(
            json!({"id":"user","type":"message","message":{"role":"user","content":[{"type":"text","text":"marker"}]}}));
        assert!(source_run_input_observed(&store, "child", "marker"));
        assert!(!source_run_input_observed(&store, "other", "marker"));
        store.sessions.get_mut("child").unwrap().journal_ready = false;
        assert!(!source_run_input_observed(&store, "child", "marker"));
    }

    #[test]
    fn source_run_open_requires_ready_projection_and_the_original_epoch() {
        let mut store = ready_store();
        assert_eq!(source_run_open_epoch(&store).unwrap(), 7);
        assert!(!source_run_open_observed(&store, "child", 7).unwrap());

        store.sessions.insert(
            "child".into(),
            tekes_agent_session::SessionProjection::default(),
        );
        assert!(!source_run_open_observed(&store, "child", 7).unwrap());
        store.sessions.insert(
            "child".into(),
            tekes_agent_session::SessionProjection {
                run_open: true,
                ..Default::default()
            },
        );
        assert!(source_run_open_observed(&store, "child", 7).unwrap());

        store.connection = tekes_agent_session::ConnectionState::Connecting;
        assert!(matches!(
            source_run_open_observed(&store, "child", 7),
            Err(Error::Disconnected)
        ));
        assert!(matches!(
            source_run_open_epoch(&store),
            Err(Error::Disconnected)
        ));

        store.connection = tekes_agent_session::ConnectionState::Ready;
        store.connection_epoch = 8;
        assert!(matches!(
            source_run_open_observed(&store, "child", 7),
            Err(Error::Disconnected)
        ));
    }
}
