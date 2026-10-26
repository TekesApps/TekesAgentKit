//! Consumer proof through public Composer/Card/binding APIs and real SEP state.
use crate::component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, TranscriptItemKind, TranscriptSnapshot};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ModelSelectionPreparation, RunBinding};
use tekes_ui_controls::composition::AccessibilityAction;
const FOLLOWUP: &str = "AgentKit actionable followup proof";
fn send(
    view: &mut ComponentDemo,
    binding: &Arc<RunBinding>,
    text: &str,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    binding.edit_text(
        &ComposerDestination::run(binding.scope().clone()),
        text.into(),
    )?;
    view.sync()?;
    let mut commands =
        view.accessibility_action(view.ids().send, None, AccessibilityAction::Press)?;
    require(commands.len() == 1, "one public Send command required")?;
    let DemoCommand::Send {
        binding: source,
        ticket,
    } = commands.remove(0)
    else {
        return Err(unsupported("public Send unavailable"));
    };
    require(
        Arc::ptr_eq(binding, &source),
        "Send changed actionable source",
    )?;
    Ok(tokio::spawn(async move {
        source
            .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
            .await
    }))
}
fn answer_persisted(runtime: &SessionRuntime, binding: &RunBinding, kind: &str, cut: i64) -> bool {
    runtime.inspect(|store| {
        let Some(session) = store.sessions.get(&binding.scope().session_id) else {
            return false;
        };
        if kind == "pi" {
            return session.native_conversation.as_ref().is_some_and(|native| {
                native.entries.iter().any(|entry| {
                    entry["customType"] == "endpoint-ui-answer" && entry["data"]["value"] == "yes"
                })
            });
        }
        session.entries.iter().any(|(seq, entry)| {
            *seq > cut
                && entry.event.kind == "tool/result"
                && entry.event.data.get("error").is_none_or(Value::is_null)
                && entry.event.data["message"]["isError"] != true
                && entry.event.data["message"]
                    .to_string()
                    .contains("continue-proof")
        })
    })
}
fn followup_ids(history: &TranscriptSnapshot) -> Vec<String> {
    history
        .items
        .iter()
        .filter(|item| {
            item.kind == TranscriptItemKind::User
                && !item.is_provisional
                && item.text.contains(FOLLOWUP)
        })
        .map(|item| item.id.clone())
        .collect()
}
pub async fn run(
    view: &mut ComponentDemo,
    runtime: &Arc<SessionRuntime>,
    kind: &str,
) -> Result<Value> {
    require(
        matches!(kind, "native" | "pi" | "dsh"),
        "actionable probe needs configured Native/Pi/DSH question source",
    )?;
    let a = view.bindings()[0].clone();
    let b = view.bindings()[1].clone();
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "B 的独立问答草稿".into(),
    )?;
    let b_draft = b.current_run().unwrap().draft.clone();
    let cut = runtime.inspect(|store| store.sessions[&a.scope().session_id].tail_sequence);
    let dispatch = send(
        view,
        &a,
        if kind == "pi" {
            "/endpoint-ui-question"
        } else if kind == "native" {
            "NATIVE_ACTIONABLE_UI_PROOF"
        } else {
            "DSH_ACTIONABLE_UI_PROOF"
        },
    )?;
    let mut changes = a
        .subscribe_run()
        .ok_or_else(|| unsupported("actionable source is read-only"))?;
    let target = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            view.sync()?;
            if let Some(id) = view.actionable_choice_id(view.ids(), "option-0") {
                return Ok::<_, Error>(id);
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    let mut commands = view.accessibility_action(target, None, AccessibilityAction::Press)?;
    require(
        commands.len() == 1,
        "one public actionable response required",
    )?;
    let DemoCommand::RespondActionable {
        request,
        ids,
        intent,
    } = commands.remove(0)
    else {
        return Err(unsupported("wrong actionable callback"));
    };
    require(
        intent.prompt.scope() == a.scope(),
        "actionable changed original scope",
    )?;
    let expected = if kind == "pi" {
        "yes"
    } else {
        "continue-proof"
    };
    require(
        intent
            .prompt
            .choice(&intent.choice_id)
            .is_some_and(|choice| {
                choice.outcome()["answer"]["answers"][0]["selected"][0] == expected
            }),
        "question option mapping mismatch",
    )?;
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)?;
    request.respond().await?;
    require(
        request.respond().await.is_err(),
        "response capture attempted twice",
    )?;
    view.complete_actionable(ids, &intent, Ok(()))?;
    require(
        view.selected() == 1 && b.current_run().unwrap().draft == b_draft,
        "late response changed B",
    )?;
    tokio::time::timeout(Duration::from_secs(15), dispatch)
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Disconnected)??;
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)?;
    let previous_finals = a
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|item| item.kind == TranscriptItemKind::Assistant && item.is_final)
        .count();
    let followup = send(view, &a, FOLLOWUP)?;
    tokio::time::timeout(Duration::from_secs(15), followup)
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Disconnected)??;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = a.current_run().unwrap();
            let resolved = runtime.inspect(|store| {
                !store
                    .actionables
                    .values()
                    .any(|value| value["sessionId"] == a.scope().session_id)
            });
            if resolved
                && snapshot.can_submit
                && snapshot
                    .run
                    .as_ref()
                    .is_some_and(|run| run.phase == tekes_agent_presentation::RunPhase::Idle)
                && !snapshot.transcript.is_stale
                && followup_ids(&snapshot.transcript).len() == 1
                && answer_persisted(runtime, &a, kind, cut)
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .filter(|item| item.kind == TranscriptItemKind::Assistant && item.is_final)
                    .count()
                    > previous_finals
            {
                return Ok::<_, Error>(());
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    let before = followup_ids(&a.current_run().unwrap().transcript);
    let epoch = runtime.inspect(|s| s.connection_epoch);
    runtime.reconnect().await?;
    let mut updates = runtime.subscribe();
    let history = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let history = updates.inspect(|store| {
                if store.connection_epoch == epoch || !store.ready() {
                    return Ok(None);
                }
                let Some(session) = store
                    .sessions
                    .get(&a.scope().session_id)
                    .filter(|session| session.journal_ready)
                else {
                    return Ok(None);
                };
                TranscriptSnapshot::read(a.scope(), session).map(Some)
            })?;
            if let Some(history) = history {
                return Ok::<_, Error>(history);
            }
            updates.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    require(
        before == followup_ids(&history),
        "reconnect duplicated or lost followup input",
    )?;
    require(
        answer_persisted(runtime, &a, kind, cut),
        "reconnect lost canonical question answer",
    )?;
    require(
        !request.is_current() && request.respond().await.is_err(),
        "old response survived reconnect",
    )?;
    require(
        b.current_run().unwrap().draft == b_draft,
        "reconnect changed B draft",
    )?;
    view.sync()?;
    Ok(
        json!({"probeConfigured":true,"publicComposer":true,"publicCard":true,"originalScope":true,"oneAttempt":true,"sharedResolved":true,"canonicalAnswerRecord":true,
        "otherDraftPreserved":true,"followupInputCount":before.len(),"freshHistoryAfterReconnect":true,"reconnectNoDuplicateInput":true,"nativeInteraction":false,"physicalIME":false}),
    )
}
