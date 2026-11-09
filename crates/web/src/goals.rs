//! Harness Goal normalization. Receipts acknowledge a mutation; a separate read
//! refreshes its live view, so a failed refresh never disguises the mutation.
use crate::*;

impl SharedAgent {
    pub(crate) async fn goal_read(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?;
        let endpoint = shared.connected()?;
        let capabilities = endpoint.capabilities();
        require(
            capabilities.supports("goals.get"),
            "Goal reading is unavailable",
        )?;
        let epoch = shared.runtime.snapshot().connection_epoch;
        shared
            .runtime
            .inspect(|state| validate_scope(state, session_id, epoch, false))?;
        let reply = shared
            .runtime
            .call_at_epoch(
                epoch,
                "goals.get",
                json!({"sessionId":session_id}),
                &format!("agent-goal-read-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let goal = normalize_goal(&shared.kind, reply)?;
        shared.runtime.inspect(|state| {
            validate_scope(state, session_id, epoch, false)?;
            let writable = validate_scope(state, session_id, epoch, true).is_ok();
            Ok(json!({
                "endpointId":state.endpoint_id,"sessionId":session_id,"connectionEpoch":epoch,
                "goal":goal.as_ref().map(goal_json),
                "capabilities":goal_capabilities(&shared.kind, &capabilities, goal.as_ref(), writable)
            }))
        })
    }

    pub(crate) async fn goal_action(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let session_id = string(payload, "sessionId")?.to_owned();
        let epoch = payload["connectionEpoch"]
            .as_u64()
            .ok_or_else(|| Error::Protocol("connectionEpoch must be an unsigned integer".into()))?;
        shared
            .runtime
            .inspect(|state| validate_scope(state, &session_id, epoch, true))?;
        let endpoint = shared.connected()?;
        let capabilities = endpoint.capabilities();
        require(
            capabilities.supports("goals.get"),
            "Goal reading is unavailable",
        )?;
        let reply = shared
            .runtime
            .call_at_epoch(
                epoch,
                "goals.get",
                json!({"sessionId":session_id}),
                &format!("agent-goal-read-{}", uuid::Uuid::new_v4()),
            )
            .await?;
        let goal = normalize_goal(&shared.kind, reply)?;
        let (method, request) =
            action_request(&shared.kind, &capabilities, goal.as_ref(), payload)?;
        let runtime = shared.runtime.clone();
        let validation_runtime = runtime.clone();
        let validation_session = session_id.clone();
        let receipt = runtime
            .call_at_epoch_with_validation(epoch, method, request, rpc_id, move || {
                validation_runtime
                    .inspect(|state| validate_scope(state, &validation_session, epoch, true))
            })
            .await?;
        Ok(
            json!({"endpointId":endpoint.identity(),"sessionId":session_id,"connectionEpoch":epoch,"action":payload["action"],"acknowledged":true,"receipt":receipt}),
        )
    }
}

fn native(kind: &str) -> bool {
    matches!(kind, "native" | "tekes" | "kernel")
}

fn validate_scope(
    state: &MemoryProjectionStore,
    session_id: &str,
    epoch: u64,
    mutation: bool,
) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "Goal connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready),
        "Goal requires a ready connection",
    )?;
    require(
        !session_id.is_empty() && state.sessions.contains_key(session_id),
        "Goal session is unavailable",
    )?;
    if mutation {
        require(
            RunPresentation::read(state, &RunScope::main(&state.endpoint_id, session_id), true)
                .can_submit,
            "Goal changes are unavailable for this session",
        )?;
    }
    Ok(())
}

/// Validate each Harness reply once at the normalization boundary. In
/// particular, a missing Native wrapper or malformed object is not "no goal".
fn normalize_goal(kind: &str, reply: Value) -> Result<Option<GoalView>> {
    let value = if native(kind) {
        reply
            .get("goal")
            .cloned()
            .ok_or_else(|| Error::Protocol("missing Kernel Goal wrapper".into()))?
    } else {
        require(matches!(kind, "pi" | "dsh"), "unknown Goal Harness")?;
        reply
    };
    if value.is_null() {
        return Ok(None);
    }
    let goal: GoalView = serde_json::from_value(value)?;
    require(
        !goal.id.is_empty() && goal.revision > 0 && !goal.objective.trim().is_empty(),
        "invalid Goal identity or objective",
    )?;
    require(
        matches!(
            goal.phase.as_str(),
            "active" | "paused" | "blocked" | "complete"
        ),
        "invalid Goal phase",
    )?;
    require(
        if native(kind) {
            matches!(goal.activation.as_str(), "active" | "inactive")
        } else {
            matches!(goal.activation.as_str(), "armed" | "disarmed")
        },
        "invalid Goal activation",
    )?;
    require(goal.max_goal_rounds > 0, "invalid Goal round budget")?;
    if let Some(reason) = &goal.blocked_reason {
        require(
            reason.is_null()
                || reason.as_str().is_some_and(|text| !text.is_empty())
                || reason.as_object().is_some_and(|_| {
                    reason["code"].as_str().is_some_and(|text| !text.is_empty())
                        && reason["message"]
                            .as_str()
                            .is_some_and(|text| !text.is_empty())
                }),
            "invalid Goal blocked reason",
        )?;
    }
    Ok(Some(goal))
}

fn goal_json(goal: &GoalView) -> Value {
    let blocked_reason = match &goal.blocked_reason {
        Some(Value::String(message)) => json!({"message":message}),
        Some(value) => value.clone(),
        None => Value::Null,
    };
    json!({"ref":{"id":goal.id,"revision":goal.revision},"objective":goal.objective,"phase":goal.phase,"activation":goal.activation,
        "maxGoalRounds":goal.max_goal_rounds,"roundsStarted":goal.rounds_started,"createdAt":goal.created_at,"updatedAt":goal.updated_at,"blockedReason":blocked_reason})
}

fn goal_capabilities(
    kind: &str,
    methods: &EndpointCapabilities,
    goal: Option<&GoalView>,
    writable: bool,
) -> Value {
    let create =
        goal.is_none() || kind == "dsh" && goal.is_some_and(|goal| goal.phase == "complete");
    let resumable = goal.is_some_and(|goal| {
        matches!(goal.phase.as_str(), "paused" | "blocked")
            || !native(kind) && goal.phase == "active" && goal.activation == "disarmed"
    });
    json!({
        "canCreate":writable && create && methods.supports(if native(kind) {"goals.edit"} else {"goals.create"}),
        "canEdit":writable && goal.is_some() && methods.supports("goals.edit"),
        "canPause":writable && goal.is_some_and(|goal|goal.phase == "active") && methods.supports("goals.pause"),
        "canResume":writable && resumable && methods.supports("goals.resume"),
        "canClear":writable && goal.is_some() && methods.supports("goals.clear"),
        "canSetCreateBudget":!native(kind),
        "createMayRun":kind != "pi", "resumeMayRun":true
    })
}

fn action_request(
    kind: &str,
    methods: &EndpointCapabilities,
    goal: Option<&GoalView>,
    payload: &Value,
) -> Result<(&'static str, Value)> {
    let action = string(payload, "action")?;
    let (method, capability) = match action {
        "create" => (
            if native(kind) {
                "goals.edit"
            } else {
                "goals.create"
            },
            "canCreate",
        ),
        "edit" => ("goals.edit", "canEdit"),
        "pause" => ("goals.pause", "canPause"),
        "resume" => ("goals.resume", "canResume"),
        "clear" => ("goals.clear", "canClear"),
        _ => return Err(Error::Protocol("unknown Goal action".into())),
    };
    require(
        goal_capabilities(kind, methods, goal, true)[capability] == true,
        "Goal action is unavailable",
    )?;
    let mut request = json!({"sessionId":string(payload,"sessionId")?});
    if action != "create" {
        let goal = goal.ok_or_else(|| Error::Protocol("Goal is unavailable".into()))?;
        let reference: GoalReference = serde_json::from_value(payload["ref"].clone())?;
        require(
            reference.id == goal.id && reference.revision == goal.revision,
            "Goal reference is stale; refresh before changing it",
        )?;
        request["ref"] = serde_json::to_value(reference)?;
    } else if native(kind) {
        // Kernel's revision-zero edit is the public create contract. The
        // placeholder is supplied by this authority, never guessed by the UI.
        request["ref"] = json!({"id":uuid::Uuid::new_v4().to_string(),"revision":0});
    }
    if matches!(action, "create" | "edit") {
        let objective = string(payload, "objective")?;
        let limit = if native(kind) { 16384 } else { 65536 };
        require(
            !objective.trim().is_empty() && objective.len() <= limit,
            "invalid Goal objective",
        )?;
        request["objective"] = json!(objective);
    }
    if let Some(rounds) = payload.get("maxGoalRounds") {
        require(
            action == "create" && !native(kind),
            "Goal round budget cannot be changed by this action",
        )?;
        let rounds = rounds
            .as_u64()
            .filter(|rounds| (1..=10000).contains(rounds))
            .ok_or_else(|| Error::Protocol("invalid Goal round budget".into()))?;
        request["maxGoalRounds"] = json!(rounds);
    }
    Ok((method, request))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal() -> Value {
        json!({"id":"goal","revision":4,"objective":"ship","phase":"active","activation":"disarmed","maxGoalRounds":8,"roundsStarted":2,"createdAt":100.0,"updatedAt":200.0,"blockedReason":null})
    }
    fn methods() -> EndpointCapabilities {
        EndpointCapabilities::methods(&[
            "goals.get",
            "goals.create",
            "goals.edit",
            "goals.pause",
            "goals.resume",
            "goals.clear",
        ])
    }
    fn state() -> MemoryProjectionStore {
        let mut state = MemoryProjectionStore::new("endpoint");
        state.connection = ConnectionState::Ready;
        state.connection_epoch = 3;
        state
            .sessions
            .entry("session".into())
            .or_default()
            .journal_ready = true;
        state
    }

    #[derive(Clone)]
    struct GoalConnector {
        calls: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
        fail_edit: bool,
    }
    struct GoalEndpoint {
        connector: GoalConnector,
        host: HostDescription,
    }
    #[async_trait]
    impl EndpointConnector for GoalConnector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(GoalEndpoint {
                connector: self.clone(),
                host: serde_json::from_value(
                    json!({"protocolVersion":4,"product":{"name":"goal-fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false}),
                )?,
            }))
        }
    }
    #[async_trait]
    impl SessionEndpoint for GoalEndpoint {
        fn identity(&self) -> &str {
            "goal-endpoint"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            methods()
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::stream;
            let frame = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"session","running":false,"blank":false,"updatedAt":1,"title":"Goal test"}]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
                StreamTarget::SessionJournal { address, .. } => {
                    json!({"type":"snapshot","snapshot":{"sessionId":address.session_id,"generation":"g","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})
                }
                _ => return Err(unsupported("fixture stream")),
            };
            Ok(Box::pin(
                stream::once(async { Ok(frame) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
            self.connector
                .calls
                .lock()
                .unwrap()
                .push((method.to_owned(), payload));
            if method == "goals.edit" && self.connector.fail_edit {
                return Err(Error::Timeout);
            }
            Ok(if method == "goals.get" {
                goal()
            } else {
                json!({"id":"goal","revision":5})
            })
        }
        async fn close(&self) {}
    }

    #[tokio::test]
    async fn public_goal_calls_keep_session_epoch_and_acknowledgement_without_retrying_failures() {
        for fail_edit in [false, true] {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let connector = GoalConnector {
                calls: calls.clone(),
                fail_edit,
            };
            let mut agent = SharedAgent::default();
            agent
                .register(
                    "goal-endpoint".into(),
                    "pi".into(),
                    "Goal".into(),
                    Arc::new(connector),
                )
                .unwrap();
            let endpoint = agent.endpoint("goal-endpoint").unwrap();
            endpoint
                .runtime
                .wait_ready(std::time::Duration::from_secs(2))
                .await
                .unwrap();
            endpoint.runtime.open_session("session").await.unwrap();
            let view = agent
                .goal_read(&endpoint, &json!({"sessionId":"session"}))
                .await
                .unwrap();
            assert_eq!(view["endpointId"], "goal-endpoint");
            assert_eq!(view["goal"]["ref"], json!({"id":"goal","revision":4}));
            let request = json!({"sessionId":"session","connectionEpoch":view["connectionEpoch"],"action":"edit","ref":view["goal"]["ref"],"objective":"new objective"});
            let result = agent.goal_action(&endpoint, &request, "edit-once").await;
            if fail_edit {
                assert!(matches!(result, Err(Error::Timeout)));
            } else {
                let receipt = result.unwrap();
                assert_eq!(receipt["acknowledged"], true);
                assert_eq!(receipt["receipt"], json!({"id":"goal","revision":5}));
                assert!(
                    receipt.get("goal").is_none(),
                    "a receipt is not a fabricated refreshed view"
                );
            }
            let writes: Vec<_> = calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "goals.edit")
                .cloned()
                .collect();
            assert_eq!(
                writes,
                vec![(
                    "goals.edit".into(),
                    json!({"sessionId":"session","ref":{"id":"goal","revision":4},"objective":"new objective"})
                )]
            );
            endpoint.runtime.stop().await.unwrap();
        }
    }

    #[test]
    fn normalize_preserves_goal_refs_budget_and_harness_activation() {
        let mut kernel = goal();
        kernel["activation"] = json!("active");
        kernel["blockedReason"] = json!("budget exhausted");
        let normalized = normalize_goal("native", json!({"goal":kernel}))
            .unwrap()
            .unwrap();
        assert_eq!(
            goal_json(&normalized)["blockedReason"],
            json!({"message":"budget exhausted"})
        );
        assert_eq!(
            goal_json(&normalized)["ref"],
            json!({"id":"goal","revision":4})
        );
        for kind in ["pi", "dsh"] {
            let mut wire = goal();
            wire["blockedReason"] = json!({"code":"budget-exhausted","message":"Limit reached"});
            let normalized = normalize_goal(kind, wire).unwrap().unwrap();
            assert_eq!(normalized.rounds_started, 2);
            assert_eq!(
                goal_json(&normalized)["blockedReason"]["code"],
                "budget-exhausted"
            );
        }
        assert!(
            normalize_goal("native", json!({"goal":null}))
                .unwrap()
                .is_none()
        );
        assert!(normalize_goal("dsh", Value::Null).unwrap().is_none());
    }

    #[test]
    fn malformed_reply_is_not_an_empty_goal_or_a_mutation_reference() {
        assert!(normalize_goal("native", Value::Null).is_err());
        assert!(normalize_goal("pi", json!({"goal":null})).is_err());
        for (key, value) in [
            ("revision", json!(0)),
            ("phase", json!("invented")),
            ("activation", json!("active")),
            ("maxGoalRounds", json!(0)),
            ("blockedReason", json!({"message":"without code"})),
        ] {
            let mut wire = goal();
            wire[key] = value;
            assert!(normalize_goal("pi", wire).is_err(), "{key}");
        }
    }

    #[test]
    fn actions_use_exact_goal_ref_and_do_not_invent_budget_edits() {
        let current = normalize_goal("pi", goal()).unwrap().unwrap();
        let request = json!({"sessionId":"session","action":"edit","ref":{"id":"goal","revision":4},"objective":"new"});
        let (method, forwarded) =
            action_request("pi", &methods(), Some(&current), &request).unwrap();
        assert_eq!(method, "goals.edit");
        assert_eq!(
            forwarded,
            json!({"sessionId":"session","ref":{"id":"goal","revision":4},"objective":"new"})
        );
        let mut stale = request.clone();
        stale["ref"]["revision"] = json!(3);
        assert!(action_request("pi", &methods(), Some(&current), &stale).is_err());
        let mut budget = request;
        budget["maxGoalRounds"] = json!(12);
        assert!(action_request("pi", &methods(), Some(&current), &budget).is_err());
        assert!(
            action_request(
                "pi",
                &EndpointCapabilities::default(),
                Some(&current),
                &stale
            )
            .is_err()
        );
    }

    #[test]
    fn creation_matches_public_contract_and_exposes_possible_execution() {
        let payload = json!({"sessionId":"session","action":"create","objective":"ship"});
        let (method, request) = action_request("native", &methods(), None, &payload).unwrap();
        assert_eq!(method, "goals.edit");
        assert_eq!(request["ref"]["revision"], 0);
        assert!(!request["ref"]["id"].as_str().unwrap().is_empty());
        for kind in ["pi", "dsh"] {
            let (method, request) = action_request(kind, &methods(), None, &payload).unwrap();
            assert_eq!(method, "goals.create");
            assert!(request.get("ref").is_none());
        }
        assert_eq!(
            goal_capabilities("native", &methods(), None, true)["createMayRun"],
            true
        );
        assert_eq!(
            goal_capabilities("dsh", &methods(), None, true)["createMayRun"],
            true
        );
        assert_eq!(
            goal_capabilities("pi", &methods(), None, true)["resumeMayRun"],
            true
        );
    }

    #[test]
    fn stale_disconnected_archived_and_historical_sessions_cannot_mutate_goals() {
        let ready = state();
        assert!(validate_scope(&ready, "session", 3, true).is_ok());
        assert!(validate_scope(&ready, "session", 2, true).is_err());
        assert!(validate_scope(&ready, "other", 3, true).is_err());
        let mut archived = ready.clone();
        archived.archived_session_ids.push("session".into());
        assert!(validate_scope(&archived, "session", 3, true).is_err());
        assert!(validate_scope(&archived, "session", 3, false).is_ok());
        let mut historical = ready.clone();
        historical
            .sessions
            .get_mut("session")
            .unwrap()
            .projections
            .insert(
                "historyAccess".into(),
                tekes_agent_session::VersionedValue {
                    sequence: 1,
                    value: json!({"mode":"read-only"}),
                },
            );
        assert!(validate_scope(&historical, "session", 3, true).is_err());
        let mut disconnected = ready;
        disconnected.connection = ConnectionState::Connecting;
        assert!(validate_scope(&disconnected, "session", 3, false).is_err());
    }
}
