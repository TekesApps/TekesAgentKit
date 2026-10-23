//! Isolated real scratch fork -> original SidePanel/RunPort -> confirmed discard.
use crate::component_demo::{ComponentDemo, DemoCommand};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{Composer, SidePanel, SidePanelTabKind};
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, RunPhase, TranscriptItemKind};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ModelSelectionPreparation, PaneAccess, SideChatBinding};
use tekes_ui_controls::composition::{AccessibilityAction, ButtonContent, ComponentId};

const INPUT: &str = "AGENTKIT_REAL_SIDECHAT_OWNED_INPUT";
fn panel(view: &ComponentDemo) -> &SidePanel {
    view.tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
}
async fn press(view: &mut ComponentDemo, id: ComponentId) -> Result<Vec<DemoCommand>> {
    let commands = view.accessibility_action(id, None, AccessibilityAction::Press)?;
    let mut business = vec![];
    for command in commands {
        match command {
            // A newly mounted real fork schedules the normal read-only feedback
            // request alongside its first action. Execute the host callback;
            // do not count it as another Send or suppress the read.
            DemoCommand::ReadInitialFeedback(request) => {
                view.complete_initial_feedback((*request).execute().await)?;
            }
            command => business.push(command),
        }
    }
    Ok(business)
}
pub async fn run(
    view: &mut ComponentDemo,
    runtime: &Arc<SessionRuntime>,
    owner: &Arc<SideChatBinding>,
) -> Result<Value> {
    let main = view.binding().clone();
    let other = view.bindings()[1].clone();
    main.edit_text(
        &ComposerDestination::run(main.scope().clone()),
        "主会话保留草稿".into(),
    )?;
    other.edit_text(
        &ComposerDestination::run(other.scope().clone()),
        "另一会话保留草稿".into(),
    )?;
    let main_draft = main.current_run().unwrap().draft.clone();
    let other_draft = other.current_run().unwrap().draft.clone();
    view.sync()?;
    if owner.support_snapshot() == Some(false) {
        let create = panel(view).create_side_chat_id();
        require(
            view.tree
                .get(create)
                .unwrap()
                .component::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled,
            "unsupported SideChat creation remained enabled",
        )?;
        require(
            press(view, create).await?.is_empty() && owner.snapshot().chats.is_empty(),
            "unsupported SideChat created a command or pointer",
        )?;
        require(
            main.current_run().unwrap().draft == main_draft
                && other.current_run().unwrap().draft == other_draft,
            "unavailable SideChat changed drafts",
        )?;
        return Ok(
            json!({"realHarness":true,"capabilityUnavailable":true,"originalCreateDisabled":true,
            "noForkAttempted":true,"parentAndOtherDraftsPreserved":true,"scratchLifecycleAccepted":false,
            "nativeInteraction":false,"physicalIme":false,"crossProcess":false,"readingAnchorAcceptance":false}),
        );
    }
    require(
        owner.support_snapshot() == Some(true),
        "SideChat capability is not ready",
    )?;
    let create = panel(view).create_side_chat_id();
    let mut commands = press(view, create).await?;
    require(
        commands.len() == 1,
        "SideChat requires one original create command",
    )?;
    let DemoCommand::CreateSideChat(request) = commands.remove(0) else {
        return Err(unsupported("original SideChat create unavailable"));
    };
    require(
        request.parent() == main.scope(),
        "SideChat creation changed parent",
    )?;
    let chat = owner
        .create(
            main.scope().clone(),
            Some("owned proof"),
            "ui-sidechat-proof-create",
        )
        .await?;
    require(
        chat.scope != chat.parent && chat.parent == *main.scope(),
        "SideChat fork did not get independent scope",
    )?;
    let bound = owner.open(&chat).await?;
    let scratch = bound.binding().clone();
    require(
        view.install_side_chat_run(request, bound)?,
        "SideChat original pane was not selected",
    )?;
    let ids = view
        .panel_composer_ids()
        .ok_or_else(|| unsupported("SideChat Composer not mounted"))?;
    require(
        view.transcript_content_id(ids).is_some(),
        "SideChat original Transcript missing",
    )?;
    scratch.edit_text(&ComposerDestination::run(chat.scope.clone()), INPUT.into())?;
    view.sync()?;
    let mut commands = press(view, ids.send).await?;
    if commands.len() != 1 {
        eprintln!(
            "SIDECHAT_SEND_REJECTED {}",
            json!({"commands":commands.len(),"snapshot":scratch.current_run().map(|snapshot| json!(&*snapshot))})
        );
    }
    require(commands.len() == 1, "SideChat original Send missing")?;
    let DemoCommand::Send {
        binding: sender,
        ticket,
    } = commands.remove(0)
    else {
        return Err(unsupported("SideChat did not return Send"));
    };
    require(
        Arc::ptr_eq(&sender, &scratch),
        "SideChat Send used parent authority",
    )?;
    sender
        .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
        .await?;
    let mut changes = scratch
        .subscribe_run()
        .ok_or_else(|| unsupported("SideChat history unavailable"))?;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            let inputs = snapshot
                .transcript
                .items
                .iter()
                .filter(|item| {
                    item.kind == TranscriptItemKind::User
                        && item.text == INPUT
                        && !item.is_provisional
                })
                .count();
            if inputs == 1
                && snapshot
                    .run
                    .as_ref()
                    .is_some_and(|run| run.phase == RunPhase::Idle)
                && snapshot.transcript.turns.iter().any(|turn| {
                    turn.input == INPUT
                        && turn.status == "completed"
                        && snapshot.transcript.items.iter().any(|item| {
                            item.turn_id == turn.id
                                && item.kind == TranscriptItemKind::Assistant
                                && item.is_final
                                && !item.is_provisional
                                && item.text.contains("ENDPOINT_REPLY_OK")
                        })
                })
            {
                return Ok::<_, Error>(snapshot.connection_epoch);
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    view.sync()?;
    require(
        main.current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .all(|item| item.text != INPUT),
        "SideChat input leaked into parent history",
    )?;
    scratch.edit_text(
        &ComposerDestination::run(chat.scope.clone()),
        "SideChat 自己的未提交草稿".into(),
    )?;
    view.sync()?;
    let scratch_draft = scratch.current_run().unwrap().draft.clone();
    let summary = panel(view)
        .tab_id(&SidePanelTabKind::Summary)
        .ok_or_else(|| unsupported("Summary tab missing"))?;
    require(
        press(view, summary).await?.is_empty(),
        "Summary navigation emitted a remote command",
    )?;
    require(
        view.panel_composer_ids().is_none(),
        "Summary still exposes scratch Composer",
    )?;
    let tab = SidePanelTabKind::SideChat {
        session_id: chat.scope.session_id.clone(),
    };
    let back = panel(view)
        .tab_id(&tab)
        .ok_or_else(|| unsupported("original SideChat tab missing"))?;
    require(
        press(view, back).await?.is_empty(),
        "retained SideChat unexpectedly reopened authority",
    )?;
    require(
        view.panel_composer_ids() == Some(ids),
        "SideChat changed original Composer IDs",
    )?;
    require(
        view.tree
            .get(ids.root)
            .unwrap()
            .component::<Composer>()
            .unwrap()
            .editor()
            .input
            .text
            == scratch_draft.text,
        "SideChat draft did not restore through original editor",
    )?;
    let old_epoch = scratch.current_run().unwrap().connection_epoch;
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(15)).await?;
    let mut fresh = scratch.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = fresh.borrow_and_update().clone();
            if snapshot.connection_epoch > old_epoch && !snapshot.transcript.is_stale {
                require(
                    snapshot
                        .transcript
                        .items
                        .iter()
                        .filter(|item| {
                            item.kind == TranscriptItemKind::User
                                && item.text == INPUT
                                && !item.is_provisional
                        })
                        .count()
                        == 1,
                    "SideChat reconnect duplicated or lost input",
                )?;
                require(
                    snapshot.draft == scratch_draft,
                    "SideChat reconnect lost draft",
                )?;
                return Ok::<_, Error>(());
            }
            fresh.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    view.sync()?;
    let close = panel(view).close_id();
    let mut commands = press(view, close).await?;
    require(
        commands.len() == 1,
        "SideChat original close command missing",
    )?;
    let DemoCommand::CloseSideChat(expected) = commands.remove(0) else {
        return Err(unsupported("SideChat close did not retain its pointer"));
    };
    require(
        expected.scope == chat.scope && expected.parent == chat.parent,
        "SideChat close changed pointer",
    )?;
    owner.close(&expected, "ui-sidechat-proof-close").await?;
    view.sync()?;
    require(
        owner.pointer(main.scope(), &chat.scope.session_id).is_err()
            && scratch.access() == PaneAccess::Missing,
        "SideChat discard did not retire original authority",
    )?;
    require(
        !view.has_side_chat_run(&chat) && view.panel_composer_ids().is_none(),
        "discarded SideChat pane remained mounted",
    )?;
    require(
        main.current_run().unwrap().draft == main_draft
            && other.current_run().unwrap().draft == other_draft,
        "SideChat lifecycle changed other drafts",
    )?;
    Ok(
        json!({"realHarness":true,"capabilityUnavailable":false,"scratchLifecycleAccepted":true,"originalCreate":true,"independentForkScope":true,"scratchScope":chat.scope,
        "originalSidePanelRunPort":true,"originalSend":true,"scratchReply":true,"parentHistoryIsolated":true,
        "summaryNavigation":true,"originalComposerIdsRetained":true,"scratchDraftRestored":true,
        "freshReconnect":true,"noDuplicateInput":true,"originalClose":true,"confirmedDiscard":true,
        "retiredAuthority":true,"parentAndOtherDraftsPreserved":true,"nativeInteraction":false,
        "physicalIme":false,"crossProcess":false,"readingAnchorAcceptance":false}),
    )
}
