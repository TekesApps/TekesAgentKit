//! Opt-in mutation proof over isolated real endpoint sessions. No alternate UI state.
use crate::component_demo::{ComponentDemo, DemoCommand};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{Result, SessionAddress, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, RunScope, TranscriptItemKind};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{RunBinding, TurnActionDispatcher};
use tekes_ui_controls::composition::AccessibilityAction;

pub async fn run(
    view: &mut ComponentDemo,
    runtime: &Arc<SessionRuntime>,
    dispatcher: TurnActionDispatcher,
) -> Result<Value> {
    let a = view.binding().clone();
    let b = view
        .bindings()
        .iter()
        .find(|source| source.scope() != a.scope())
        .cloned()
        .ok_or_else(|| unsupported("fork proof needs a second isolated session"))?;
    let before = a
        .current_run()
        .ok_or_else(|| unsupported("fork source missing"))?;
    let last = before
        .transcript
        .turns
        .last()
        .ok_or_else(|| unsupported("fork turn missing"))?;
    let action = last
        .footnote_actions
        .iter()
        .find(|action| action.enabled)
        .ok_or_else(|| unsupported("completed-turn fork unavailable"))?;
    let expected = before
        .transcript
        .items
        .iter()
        .rev()
        .find(|item| item.turn_id == last.id && item.kind == TranscriptItemKind::User)
        .ok_or_else(|| unsupported("selected turn has no input"))?
        .text
        .clone();
    let a_draft = before.draft.clone();
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "B 回合操作隔离草稿".into(),
    )?;
    let b_draft = b.current_run().unwrap().draft.clone();
    let root = ComponentDemo::turn_footnote_id();
    let button = view
        .tree
        .get(root)
        .and_then(|node| node.component::<tekes_agent_components_ui::TurnFootnote>())
        .and_then(|footer| footer.action_id(&action.id))
        .ok_or_else(|| unsupported("footnote control missing"))?;
    let commands = view.accessibility_action(button, None, AccessibilityAction::Press)?;
    let mut intents = commands.into_iter().filter_map(|command| {
        if let DemoCommand::TurnAction { binding, intent } = command {
            Some((binding, intent))
        } else {
            None
        }
    });
    let (source, intent) = intents
        .next()
        .ok_or_else(|| unsupported("footnote gesture not captured"))?;
    require(
        intents.next().is_none() && Arc::ptr_eq(&source, &a),
        "ambiguous footnote origin",
    )?;
    let request = dispatcher.prepare(
        source,
        &intent,
        format!(
            "ui-turn-proof:{}:{}",
            a.scope().session_id,
            intent.action().id
        ),
    )?;
    require(
        dispatcher
            .prepare(a.clone(), &intent, "duplicate-turn-gesture".into())
            .is_err(),
        "turn gesture admitted twice",
    )?;
    let token = view.begin_sidebar_action(
        SessionAddress {
            endpoint_id: a.scope().endpoint_id.clone(),
            session_id: a.scope().session_id.clone(),
        },
        intent.action().id.clone(),
    );
    let b_index = view
        .bindings()
        .iter()
        .position(|source| Arc::ptr_eq(source, &b))
        .unwrap();
    view.select(b_index)?;
    let outcome = request.execute().await?;
    require(
        view.apply_turn_action_outcome(&token, &outcome)?.is_none() && view.selected() == b_index,
        "late fork changed navigation",
    )?;
    let navigation = outcome
        .navigation
        .ok_or_else(|| unsupported("fork outcome omitted navigation"))?;
    require(
        navigation.endpoint == a.scope().endpoint_id
            && navigation.session_id != a.scope().session_id
            && navigation.session_id != b.scope().session_id,
        "fork navigation identity invalid",
    )?;
    let fork = RunBinding::open_main(
        runtime.clone(),
        RunScope::main(&navigation.endpoint, &navigation.session_id),
    )
    .await?;
    let mut updates = fork
        .subscribe_run()
        .ok_or_else(|| unsupported("fork history not observable"))?;
    tokio::time::timeout(
        Duration::from_secs(15),
        updates.wait_for(|snapshot| {
            !snapshot.transcript.is_stale
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .any(|item| item.kind == TranscriptItemKind::User && item.text == expected)
        }),
    )
    .await
    .map_err(|_| unsupported("fork history timed out"))?
    .map_err(|_| unsupported("fork observation closed"))?;
    require(
        a.current_run().unwrap().draft == a_draft && b.current_run().unwrap().draft == b_draft,
        "turn fork changed a draft",
    )?;
    let a_index = view
        .bindings()
        .iter()
        .position(|source| Arc::ptr_eq(source, &a))
        .unwrap();
    view.select(a_index)?;
    view.sync()?;
    Ok(
        json!({"sharedCatalog":true,"originalScope":true,"explicitBoundary":true,"boundaryKind":if intent.turn().fork_boundary.as_ref().and_then(|value|value.get("entryId")).is_some(){"nativeEntry"}else{"journalSequence"},"singleAdmission":true,"forkHistoryRestored":true,"lateNavigationIgnored":true,"originalDraftPreserved":true,"otherDraftPreserved":true,"nativeInteraction":false,"completeTranscript":false}),
    )
}
