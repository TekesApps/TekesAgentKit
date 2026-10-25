//! Opt-in offscreen consumer for the frozen #55 Run cancel/Unknown slice.
//!
//! Configuration is read from stdin and never echoed. Provider/runtime gates
//! are supplied by a separate owned protocol runner; this program composes the
//! public SessionRuntime and RunController only.
#[cfg(not(feature = "acceptance-source-witness"))]
fn main() {
    eprintln!("run_lifecycle_acceptance requires --features acceptance-source-witness");
    std::process::exit(2);
}

#[cfg(feature = "acceptance-source-witness")]
mod acceptance {
    use serde_json::{Value, json};
    use std::{io::Read, path::Path, sync::Arc, time::Duration};
    use tekes_agent_contract::*;
    use tekes_agent_endpoint_dsh::{DshConfiguration, DshConnector};
    use tekes_agent_endpoint_kernel::KernelConnector;
    use tekes_agent_endpoint_pi::{PiConfiguration, PiConnector};
    use tekes_agent_presentation::{ComposerDestination, ComposerDraft, RunController, RunScope};
    use tekes_agent_session::{ConnectionState, SessionRuntime};
    use uuid::Uuid;

    fn emit(value: Value) {
        println!("{value}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }

    async fn endpoint(config: &Value) -> Result<(String, Arc<dyn EndpointConnector>)> {
        let kind = string(config, "kind")?;
        let identity = config["identity"].as_str().unwrap_or(kind).to_owned();
        let connector: Arc<dyn EndpointConnector> = match kind {
            "native" => Arc::new(KernelConnector::new(
                &identity,
                string(config, "url")?,
                string(config, "bearer")?.to_owned(),
            )?),
            "pi" => {
                let mut pi = PiConfiguration::new(
                    string(config, "executable")?.into(),
                    string(config, "dataRoot")?.into(),
                );
                pi.identity = identity.clone();
                pi.arguments =
                    serde_json::from_value(config.get("arguments").cloned().unwrap_or(json!([])))?;
                pi.environment = serde_json::from_value(
                    config.get("environment").cloned().unwrap_or(json!({})),
                )?;
                pi.extensions =
                    serde_json::from_value(config.get("extensions").cloned().unwrap_or(json!([])))?;
                pi.session_roots = serde_json::from_value(
                    config.get("sessionRoots").cloned().unwrap_or(json!([])),
                )?;
                Arc::new(PiConnector::new(pi).await?) as Arc<dyn EndpointConnector>
            }
            "dsh" => {
                let mut dsh = DshConfiguration::new(&identity, string(config, "url")?)?;
                if let Some(cookie) = config["cookie"].as_str() {
                    dsh.authorization = Some(("Cookie".into(), cookie.into()));
                } else if let Some(token) = config["bearer"].as_str() {
                    dsh.authorization = Some(("Authorization".into(), format!("Bearer {token}")));
                }
                Arc::new(DshConnector::new(dsh))
            }
            _ => return Err(unsupported("unknown endpoint kind")),
        };
        Ok((identity, connector))
    }

    fn session_row(snapshot: &tekes_agent_session::MemoryProjectionStore, id: &str) -> Value {
        snapshot
            .sessions
            .get(id)
            .and_then(|s| serde_json::to_value(s).ok())
            .unwrap_or(Value::Null)
    }

    fn source(controller: &RunController, scope: &RunScope, ticket_id: &str) -> Value {
        let witness = controller.refresh_with_source_witness();
        let snapshot = witness.snapshot();
        let authority = witness.authority();
        let submission = snapshot.submissions.iter().find(|s| s.id == ticket_id);
        json!({
            "publication": snapshot.revision,
            "connectionEpoch": snapshot.connection_epoch,
            "scope": scope,
            "run": &snapshot.run,
            "transcript": &snapshot.transcript,
            "summary": &snapshot.summary,
            "submission": submission.map(|s| json!({"id":s.id,"phase":s.phase,"reconciled":s.reconciled,"operation":s.operation,"resolvedScope":s.resolved_scope})),
            "draft": &snapshot.draft,
            "authority": authority,
            "session": session_row(authority, &scope.session_id),
            "projectionSucceeded": witness.projection_succeeded(),
            "serverTransactionalFreshnessProven": false,
            "sameMemoryAuthority": true
        })
    }

    async fn wait_for(
        controller: &RunController,
        runtime: &SessionRuntime,
        predicate: impl Fn(
            &tekes_agent_presentation::RunSnapshot,
            &tekes_agent_session::MemoryProjectionStore,
        ) -> bool,
        timeout: Duration,
    ) -> Result<()> {
        let mut changes = controller.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                let snapshot = controller.snapshot();
                if runtime.inspect(|store| predicate(&snapshot, store)) {
                    return Ok::<_, Error>(());
                }
                if runtime.inspect(|s| {
                    s.connection == ConnectionState::Stopped
                        || matches!(&s.connection, ConnectionState::Failed(_))
                }) {
                    return Err(Error::Disconnected);
                }
                changes.changed().await.map_err(|_| Error::Disconnected)?;
            }
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(())
    }

    async fn create_or_select(runtime: &SessionRuntime, config: &Value) -> Result<String> {
        let id = if let Some(id) = config["sessionId"].as_str() {
            id.to_owned()
        } else {
            let cwd = string(config, "cwd")?;
            runtime
                .create_session(
                    cwd,
                    &format!("agentkit-lifecycle-create-{}", Uuid::new_v4()),
                )
                .await?
        };
        if let (Some(provider), Some(model)) =
            (config["provider"].as_str(), config["model"].as_str())
        {
            runtime
                .call(
                    "session.selectModel",
                    json!({"sessionId":id,"provider":provider,"model":model}),
                    &format!("agentkit-lifecycle-model-{}", Uuid::new_v4()),
                )
                .await?;
        }
        runtime.open_session(&id).await?;
        Ok(id)
    }

    async fn cancel(
        controller: &RunController,
        runtime: &SessionRuntime,
        scope: &RunScope,
        config: &Value,
    ) -> Result<Value> {
        let destination = ComposerDestination::run(scope.clone());
        let prompt = string(config, "prompt")?;
        let cancel_signal = Path::new(string(config, "cancelSignal")?);
        require(
            cancel_signal.is_absolute() && !cancel_signal.exists() && !cancel_signal.is_symlink(),
            "cancelSignal must be a new absolute file",
        )?;
        controller.open(scope.clone()).await?;
        controller.edit(
            &destination,
            ComposerDraft {
                text: prompt.to_owned(),
                ..Default::default()
            },
        )?;
        let ticket = controller.prepare(&destination, PromptMode::Queue)?;
        let ticket_id = ticket.id().to_owned();
        let dispatch = controller.dispatch(ticket, None).await?;
        wait_for(
            controller,
            runtime,
            |snapshot, store| {
                snapshot.run.as_ref().is_some_and(|run| run.can_cancel)
                    && snapshot
                        .submissions
                        .iter()
                        .any(|submission| submission.id == ticket_id && submission.reconciled)
                    && snapshot.transcript.items.iter().any(|item| {
                        item.reconciliation_id.as_deref() == Some(ticket_id.as_str())
                            && !item.is_provisional
                            && snapshot.transcript.turns.iter().any(|turn| {
                                turn.id == item.turn_id
                                    && turn.input == prompt
                                    && turn.status == "running"
                            })
                    })
                    && store
                        .sessions
                        .get(&scope.session_id)
                        .is_some_and(|s| s.run_open)
            },
            Duration::from_secs(40),
        )
        .await?;
        controller.edit_text(&destination, string(config, "postPrepareDraft")?.to_owned())?;
        let before = source(controller, scope, &ticket_id);
        emit(
            json!({"event":"run_cancellable","ticketId":ticket_id,"dispatch":dispatch,"capture":before,
                "cancelSignal":cancel_signal}),
        );
        let signal_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            if cancel_signal.is_file() {
                let bytes = tokio::fs::read(cancel_signal)
                    .await
                    .map_err(|_| Error::Transport("cancel signal read failed".into()))?;
                require(
                    bytes.as_slice() == b"cancel\n" || bytes.as_slice() == b"cancel",
                    "cancel signal content invalid",
                )?;
                break;
            }
            require(
                tokio::time::Instant::now() < signal_deadline,
                "external held-provider cancel signal timed out",
            )?;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        controller.cancel(scope).await?;
        wait_for(
            controller,
            runtime,
            |snapshot, store| {
                let has_aborted_original_turn = snapshot
                    .transcript
                    .turns
                    .iter()
                    .any(|turn| turn.input == prompt && turn.status == "aborted");
                has_aborted_original_turn
                    && snapshot.run.as_ref().is_some_and(|run| !run.can_cancel)
                    && store
                        .sessions
                        .get(&scope.session_id)
                        .is_some_and(|s| !s.run_open && s.journal_ready)
            },
            Duration::from_secs(40),
        )
        .await?;
        let after = source(controller, scope, &ticket_id);
        require(
            after["transcript"]["turns"]
                .as_array()
                .is_some_and(|turns| {
                    turns
                        .iter()
                        .any(|turn| turn["input"] == prompt && turn["status"] == "aborted")
                }),
            "original public Transcript has no aborted turn",
        )?;
        require(
            after["draft"]["text"] == string(config, "postPrepareDraft")?,
            "post-prepare parent draft changed",
        )?;
        Ok(
            json!({"event":"cancel_settled","ticketId":ticket_id,"cancelCallCompleted":true,"before":before,"after":after,
            "interpretation":"same original prompt is publicly settled as aborted; external runner binds it to the held provider request"}),
        )
    }

    async fn unknown(
        controller: &RunController,
        _runtime: &SessionRuntime,
        scope: &RunScope,
        config: &Value,
    ) -> Result<Value> {
        require(
            scope.endpoint_id == "pi",
            "Unknown mode is frozen for Pi only",
        )?;
        let signal = Path::new(string(config, "releaseSignal")?);
        require(
            signal.is_absolute() && !signal.exists() && !signal.is_symlink(),
            "releaseSignal must be a new absolute file",
        )?;
        let destination = ComposerDestination::run(scope.clone());
        controller.open(scope.clone()).await?;
        controller.edit(
            &destination,
            ComposerDraft {
                text: string(config, "prompt")?.to_owned(),
                ..Default::default()
            },
        )?;
        let ticket = controller.prepare(&destination, PromptMode::Queue)?;
        let ticket_id = ticket.id().to_owned();
        // Deliberately no outer timeout: wait for Pi's unchanged internal RPC deadline
        // so RunController itself records the original dispatched submission Unknown.
        let dispatch_result = controller.dispatch(ticket, None).await;
        controller.edit_text(&destination, string(config, "postPrepareDraft")?.to_owned())?;
        let unknown_capture = source(controller, scope, &ticket_id);
        let state = unknown_capture["submission"].clone();
        require(
            state["phase"] == "unknown" && state["reconciled"] == false,
            "original submission did not become Unknown and unreconciled",
        )?;
        emit(
            json!({"event":"submission_unknown","ticketId":ticket_id,"dispatchError":"redacted",
            "capture":unknown_capture,"releaseSignal":signal}),
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            if signal.is_file() {
                let bytes = tokio::fs::read(signal)
                    .await
                    .map_err(|_| Error::Transport("release signal read failed".into()))?;
                require(
                    bytes.as_slice() == b"release\n" || bytes.as_slice() == b"release",
                    "release signal content invalid",
                )?;
                break;
            }
            require(
                tokio::time::Instant::now() < deadline,
                "external release signal timed out",
            )?;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        // Release gate is external. Keep this original connector generation alive
        // so its buffered native response can reconcile the original receipt.
        let settle_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let current = source(controller, scope, &ticket_id);
            if current["submission"]["reconciled"] == true
                || tokio::time::Instant::now() >= settle_deadline
            {
                if current["submission"]["reconciled"] != true {
                    emit(
                        json!({"event":"unknown_remains_unreconciled","ticketId":ticket_id,
                        "originalDispatchReturnedError":dispatch_result.is_err(),"afterRelease":current,
                        "secondDispatch":false,"manualAcknowledgement":false}),
                    );
                    return Err(Error::Timeout);
                }
                require(
                    current["draft"]["text"] == string(config, "postPrepareDraft")?,
                    "post-prepare draft changed during reconciliation",
                )?;
                return Ok(json!({"event":"unknown_reconciled","ticketId":ticket_id,
                    "originalDispatchReturnedError":dispatch_result.is_err(),"afterRelease":current,
                    "secondDispatch":false,"manualAcknowledgement":false}));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub async fn run() -> Result<()> {
        let mut input = String::new();
        std::io::stdin()
            .take(1024 * 1024)
            .read_to_string(&mut input)
            .map_err(|_| Error::Transport("stdin unavailable".into()))?;
        let config: Value = serde_json::from_str(&input)?;
        let (identity, connector) = endpoint(&config).await?;
        let runtime = Arc::new(SessionRuntime::spawn(string(&config, "kind")?, connector));
        let result = async {
            runtime.wait_ready(Duration::from_secs(30)).await?;
            let session = create_or_select(&runtime, &config).await?;
            let scope = RunScope::main(identity, session);
            let controller = RunController::new(runtime.clone())?;
            let result = match string(&config, "action")? {
                "cancel" => cancel(&controller, &runtime, &scope, &config).await?,
                "unknown" => unknown(&controller, &runtime, &scope, &config).await?,
                _ => return Err(unsupported("action must be cancel or unknown")),
            };
            emit(result);
            Ok::<_, Error>(())
        }
        .await;
        let stop = runtime.stop().await;
        result?;
        stop?;
        Ok(())
    }
}

#[cfg(feature = "acceptance-source-witness")]
#[tokio::main]
async fn main() {
    if let Err(error) = acceptance::run().await {
        let _ = error;
        eprintln!(
            "run_lifecycle_acceptance failed; details are retained by the external fixture owner"
        );
        std::process::exit(2);
    }
}
