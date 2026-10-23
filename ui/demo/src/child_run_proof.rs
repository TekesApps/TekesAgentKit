//! Real independent child authority through original Summary and SidePanel controls.
use crate::{
    component_demo::{ComponentDemo, DemoCommand},
    sidechat_proof::{metadata, press},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{Composer, SidePanel, SidePanelTabKind, SummaryView};
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{
    ChildComposerAvailability, ChildHistoryLayout, ComposerDestination, RunPhase, SummaryAction,
    TranscriptItemKind,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    ChildPaneBinding, ModelSelectionPreparation, PaneAccess, RunBinding,
};
use tekes_ui_controls::composition::ContentEvent;
const INPUT: &str = "CHILD_UI_OWNED_INPUT";
const CANCEL: &str = "ENDPOINT_CANCEL_NATIVE_PROOF";
async fn flush(view: &mut ComponentDemo, source: &Arc<ChildPaneBinding>) -> Result<()> {
    for _ in 0..4 {
        let queued = view.take_background_commands();
        let commands = metadata(view, queued).await?;
        if commands.is_empty() {
            return Ok(());
        }
        for command in commands {
            let DemoCommand::ReadChild {
                panel_parent,
                source: original,
                page,
                read_intent,
            } = command
            else {
                return Err(unsupported("unexpected child proof background command"));
            };
            require(
                Arc::ptr_eq(&original, source) && !page && read_intent.is_none(),
                "child read authority changed",
            )?;
            source.refresh_controls().await?;
            require(
                view.update_child_snapshot(&panel_parent, source, source.snapshot().await?)?,
                "original child read rejected",
            )?;
        }
    }
    Err(unsupported("child proof did not quiesce"))
}
async fn completed(binding: &RunBinding, input: &str, cancelled: bool) -> Result<()> {
    let mut changes = binding
        .subscribe_run()
        .ok_or_else(|| unsupported("child watch missing"))?;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let s = changes.borrow_and_update().clone();
            let settled = s.transcript.turns.iter().any(|turn| {
                turn.input == input
                    && if cancelled {
                        matches!(
                            turn.status.as_str(),
                            "cancelled" | "interrupted" | "stopped" | "aborted"
                        )
                    } else {
                        turn.status == "completed"
                            && s.transcript.items.iter().any(|item| {
                                item.turn_id == turn.id
                                    && item.kind == TranscriptItemKind::Assistant
                                    && item.is_final
                                    && !item.is_provisional
                                    && item.text.contains("ENDPOINT_REPLY_OK")
                            })
                    }
            });
            if settled
                && s.run
                    .as_ref()
                    .is_some_and(|run| run.phase == RunPhase::Idle)
            {
                return Ok::<_, Error>(());
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| {
        eprintln!(
            "CHILD_COMPLETION_TIMEOUT {}",
            json!({"input":input,"cancelled":cancelled,
            "snapshot":binding.current_run().map(|snapshot|json!(&*snapshot))})
        );
        Error::Timeout
    })?
}
async fn send(
    view: &mut ComponentDemo,
    source: &Arc<ChildPaneBinding>,
    text: &str,
) -> Result<tokio::task::JoinHandle<Result<()>>> {
    let ids = view
        .panel_composer_ids()
        .ok_or_else(|| unsupported("child Composer missing"))?;
    view.tree.focus(Some(ids.editor), true);
    require(
        view.tree.focused() == Some(ids.editor),
        "child editor focus missing",
    )?;
    let commands = view.event(&ContentEvent::Commit {
        text: text.into(),
        replacement: None,
    })?;
    require(
        metadata(view, commands).await?.is_empty(),
        "child edit emitted mutation",
    )?;
    let mut commands = press(view, ids.send).await?;
    require(commands.len() == 1, "one original child Send required")?;
    let DemoCommand::Send { binding, ticket } = commands.remove(0) else {
        return Err(unsupported("child Send unavailable"));
    };
    require(
        Arc::ptr_eq(&binding, source.binding()),
        "child Send changed source",
    )?;
    Ok(tokio::spawn(async move {
        binding
            .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
            .await
    }))
}
async fn dispatched(task: tokio::task::JoinHandle<Result<()>>) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Disconnected)?
}
pub async fn run(
    view: &mut ComponentDemo,
    runtime: &Arc<SessionRuntime>,
    child_id: &str,
) -> Result<Value> {
    let main = view.binding().clone();
    let other = view.bindings()[1].clone();
    main.edit_text(
        &ComposerDestination::run(main.scope().clone()),
        "父草稿独立保留".into(),
    )?;
    other.edit_text(
        &ComposerDestination::run(other.scope().clone()),
        "另一个会话草稿".into(),
    )?;
    let main_draft = main.current_run().unwrap().draft.clone();
    let other_draft = other.current_run().unwrap().draft.clone();
    view.sync()?;
    let row = main
        .current_run()
        .unwrap()
        .summary
        .as_ref()
        .and_then(|facts| {
            facts
                .subagents
                .iter()
                .find(|row| row.session_id.as_deref() == Some(child_id))
        })
        .cloned()
        .ok_or_else(|| unsupported("real child Summary pointer missing"))?;
    let action = view
        .tree
        .get(view.summary_id())
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .action_id("subagents", &row.id)
        .ok_or_else(|| unsupported("original child Summary action missing"))?;
    let mut commands = press(view, action).await?;
    require(
        commands.len() == 1,
        "one original Summary callback required",
    )?;
    let DemoCommand::Summary(intent) = commands.remove(0) else {
        return Err(unsupported("child Summary callback missing"));
    };
    let SummaryAction::OpenSubagent {
        parent,
        child_id: captured,
    } = intent.action
    else {
        return Err(unsupported("wrong Summary destination"));
    };
    require(
        parent == *main.scope() && captured == child_id && intent.scope == *main.scope(),
        "Summary child source changed",
    )?;
    let request = view.begin_child_open(&intent.scope)?;
    let source = Arc::new(
        ChildPaneBinding::open(
            runtime.clone(),
            parent,
            child_id,
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await?,
    );
    source.refresh_controls().await?;
    require(
        view.install_child_source(request, source.clone(), row.title, source.snapshot().await?)?,
        "child original pane not selected",
    )?;
    flush(view, &source).await?;
    let binding = source.binding().clone();
    require(
        binding.current_run().unwrap().child_availability
            == Some(ChildComposerAvailability::Available),
        "real child is not continuable",
    )?;
    let ids = view
        .panel_composer_ids()
        .ok_or_else(|| unsupported("child original Composer missing"))?;
    require(
        view.transcript_content_id(ids).is_some(),
        "child original Transcript missing",
    )?;
    view.tree.focus(Some(ids.editor), true);
    require(
        view.tree.focused() == Some(ids.editor),
        "child focus unavailable",
    )?;
    let commands = view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })?;
    require(
        metadata(view, commands).await?.is_empty() && press(view, ids.send).await?.is_empty(),
        "child preedit sent a prompt",
    )?;
    let first = send(view, &source, INPUT).await?;
    completed(&binding, INPUT, false).await?;
    dispatched(first).await?;
    view.sync()?;
    flush(view, &source).await?;
    require(
        main.current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .all(|item| item.text != INPUT),
        "child input leaked into parent",
    )?;
    // Match the real host: dispatch asynchronously and keep publishing original
    // Run frames while the transport's prompt receipt is still outstanding.
    let cancellation = send(view, &source, CANCEL).await?;
    let mut changes = binding.subscribe_run().unwrap();
    let stop = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            changes.borrow_and_update();
            view.sync()?;
            flush(view, &source).await?;
            let run = view
                .tree
                .get(tekes_ui_controls::composition::ComponentId(ids.root.0 + 16))
                .unwrap()
                .component::<tekes_agent_components_ui::RunPort>()
                .unwrap();
            let frame = run
                .displayed_snapshot()
                .ok_or_else(|| unsupported("child displayed frame missing"))?;
            let composer = view
                .tree
                .get(ids.root)
                .unwrap()
                .component::<Composer>()
                .unwrap();
            if frame.run.as_ref().is_some_and(|run| run.can_cancel)
                && frame
                    .transcript
                    .items
                    .iter()
                    .any(|item| item.kind == TranscriptItemKind::User && item.text == CANCEL)
                && composer.primary_action_kind()
                    == tekes_agent_components_ui::ComposerPrimaryActionKind::Stop
                && view.panel_child_controls_ready()
            {
                return Ok::<_, Error>(composer.primary_action_id());
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| {
        eprintln!(
            "CHILD_STOP_TIMEOUT {}",
            binding
                .current_run()
                .map(|snapshot| json!(&*snapshot))
                .unwrap_or(Value::Null)
        );
        Error::Timeout
    })??;
    let mut commands = press(view, stop).await?;
    require(commands.len() == 1, "child original Stop missing")?;
    let DemoCommand::Cancel {
        binding: target,
        destination,
    } = commands.remove(0)
    else {
        return Err(unsupported("child Stop did not emit Cancel"));
    };
    require(
        Arc::ptr_eq(&target, &binding)
            && destination == ComposerDestination::run(binding.scope().clone()),
        "child Cancel changed authority",
    )?;
    target.cancel(&destination).await?;
    completed(&binding, CANCEL, true).await?;
    dispatched(cancellation).await?;
    view.sync()?;
    flush(view, &source).await?;
    require(
        view.tree.focused() == Some(ids.editor),
        "Stop/Send transition lost child editor focus",
    )?;
    let commands = view.event(&ContentEvent::Commit {
        text: "子代理的暂存草稿".into(),
        replacement: None,
    })?;
    require(
        metadata(view, commands).await?.is_empty(),
        "child draft emitted a command",
    )?;
    let draft = binding.current_run().unwrap().draft.clone();
    require(!draft.text.is_empty(), "child retained draft missing")?;
    let summary = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .tab_id(&SidePanelTabKind::Summary)
        .unwrap();
    require(
        press(view, summary).await?.is_empty(),
        "Summary switch emitted a mutation",
    )?;
    let tab = SidePanelTabKind::Subagent {
        child_id: child_id.into(),
    };
    let back = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .tab_id(&tab)
        .unwrap();
    require(
        press(view, back).await?.is_empty() && view.panel_composer_ids() == Some(ids),
        "child original Composer did not return",
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
            == draft.text,
        "child draft not restored",
    )?;
    let epoch = binding.current_run().unwrap().connection_epoch;
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(15)).await?;
    let mut fresh = binding.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let s = fresh.borrow_and_update().clone();
            if s.connection_epoch > epoch && !s.transcript.is_stale {
                require(s.draft == draft, "child reconnect changed draft")?;
                for text in [INPUT, CANCEL] {
                    require(
                        s.transcript
                            .items
                            .iter()
                            .filter(|item| {
                                item.kind == TranscriptItemKind::User
                                    && item.text == text
                                    && !item.is_provisional
                            })
                            .count()
                            == 1,
                        "child reconnect duplicated/lost input",
                    )?;
                }
                return Ok::<_, Error>(());
            }
            fresh.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    view.sync()?;
    flush(view, &source).await?;
    require(
        view.panel_child_controls_ready(),
        "reconnected original child controls are not ready",
    )?;
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    require(
        press(view, close).await?.is_empty(),
        "local child tab close emitted a remote command",
    )?;
    require(
        binding.access() == PaneAccess::Missing
            && binding
                .prepare_send(
                    &ComposerDestination::run(binding.scope().clone()),
                    tekes_agent_contract::PromptMode::Queue,
                )
                .is_err(),
        "closed child kept old control authority",
    )?;
    require(
        main.current_run().unwrap().draft == main_draft
            && other.current_run().unwrap().draft == other_draft,
        "child lifecycle changed other drafts",
    )?;
    Ok(
        json!({"realHarness":true,"originalSummaryPointer":true,"scope":binding.scope(),"originalSidePanelRunPort":true,
        "originalSend":true,"childReply":true,"originalCancel":true,"cancelSettled":true,"publishedStopForOwnInput":true,"primaryKeptEditorFocus":true,"syntheticPreeditBlockedSend":true,
        "parentHistoryIsolated":true,"draftRestored":true,"freshReconnect":true,"noDuplicateInput":true,
        "originalLocalClose":true,"oldAuthorityRetired":true,"otherDraftsPreserved":true,"nativeInteraction":false,
        "physicalIme":false,"crossProcess":false,"readingAnchorAcceptance":false}),
    )
}
