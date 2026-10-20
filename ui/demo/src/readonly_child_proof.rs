//! Opt-in read-only UI proof over an owned real Native child session.
use crate::component_demo::{ComponentDemo, DemoCommand};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{ToolActivity, TranscriptContent, TranscriptContentRowKey};
use tekes_agent_contract::{PromptMode, Result, require, unsupported};
use tekes_agent_presentation::{ChildHistoryLayout, ComposerDestination};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneSnapshot};
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId};

async fn scheduled(view: &mut ComponentDemo, source: &Arc<ChildPaneBinding>) -> Result<()> {
    for _ in 0..3 {
        let commands = view.take_background_commands();
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
                return Err(unsupported(
                    "unexpected background command in read-only proof",
                ));
            };
            require(
                Arc::ptr_eq(&original, source),
                "read-only proof original source changed",
            )?;
            let snapshot = if let Some(intent) = read_intent {
                source.read_intent(intent).await?
            } else {
                if page {
                    source.load_earlier().await?;
                }
                source.snapshot().await?
            };
            view.update_child_snapshot(&panel_parent, source, snapshot)?;
        }
    }
    Err(unsupported("read-only proof did not quiesce"))
}
fn retained(view: &ComponentDemo) -> Result<Vec<(TranscriptContentRowKey, ComponentId)>> {
    let root = view
        .bounded_child_content_id()
        .ok_or_else(|| unsupported("read-only child content missing"))?;
    let content = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    Ok(content
        .row_keys()
        .iter()
        .filter(|key| matches!(key, TranscriptContentRowKey::Message(_)))
        .map(|key| (key.clone(), content.row_id(key).unwrap()))
        .collect())
}
pub async fn run(
    view: &mut ComponentDemo,
    runtime: &Arc<SessionRuntime>,
    child_id: &str,
) -> Result<serde_json::Value> {
    let parent = view.binding().clone();
    let draft = parent.current_run().unwrap().draft.clone();
    let source = Arc::new(
        ChildPaneBinding::for_run(&parent, child_id, ChildHistoryLayout::SameSession, None).await?,
    );
    let snapshot = source.snapshot().await?;
    let PaneSnapshot::ReadOnlyChild {
        scope, transcript, ..
    } = &snapshot
    else {
        return Err(unsupported("read-only Native layout required"));
    };
    require(
        scope.session_id == child_id
            && scope.thread_id == parent.scope().thread_id
            && scope.session_id != parent.scope().session_id,
        "read-only child proof scope mismatch",
    )?;
    let has_more = transcript.has_more_before;
    require(
        !transcript.items.is_empty() && !transcript.tools.is_empty(),
        "child messages/source call unavailable",
    )?;
    let tool_id = transcript.tools[0].id.clone();
    let request = view.begin_child_open(parent.scope())?;
    view.install_child_source(request, source.clone(), "有界子历史验证".into(), snapshot)?;
    scheduled(view, &source).await?;
    require(
        view.panel_composer_ids().is_none() && source.binding().current_run().is_none(),
        "read-only pane fabricated a Composer",
    )?;
    require(
        source
            .binding()
            .prepare_send(
                &ComposerDestination::run(source.binding().scope().clone()),
                PromptMode::Queue,
            )
            .is_err(),
        "read-only pane admitted Send",
    )?;
    let before = retained(view)?;
    require(!before.is_empty(), "no real child message widgets")?;
    let root = view.bounded_child_content_id().unwrap();
    let tool = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .row_id(&TranscriptContentRowKey::Tool(tool_id))
        .ok_or_else(|| unsupported("source call tool not rendered"))?;
    let ids = view
        .tree
        .get(tool)
        .unwrap()
        .component::<ToolActivity>()
        .unwrap()
        .content_ids()
        .unwrap();
    require(
        view.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press)?
            .is_empty(),
        "source call disclosure emitted a mutation",
    )?;
    require(
        view.tree
            .text_surface(ids.body)
            .is_some_and(|surface| surface.input.read_only && !surface.input.text.is_empty()),
        "source call details are not shared readonly content",
    )?;
    let (refresh, more) = view.bounded_child_buttons().unwrap();
    let commands = view.accessibility_action(
        if has_more { more } else { refresh },
        None,
        AccessibilityAction::Press,
    )?;
    let [
        DemoCommand::ReadChild {
            source: original,
            panel_parent,
            page,
            read_intent,
        },
    ] = commands.as_slice()
    else {
        return Err(unsupported("read-only UI did not capture one read"));
    };
    require(
        Arc::ptr_eq(original, &source) && panel_parent == parent.scope() && *page == has_more,
        "read-only gesture retargeted",
    )?;
    let intent = read_intent
        .clone()
        .ok_or_else(|| unsupported("read-only UI origin receipt missing"))?;
    view.update_child_snapshot(parent.scope(), &source, source.read_intent(intent).await?)?;
    scheduled(view, &source).await?;
    let after = retained(view)?;
    require(
        before.iter().all(|row| after.contains(row)),
        "child refresh/page replaced retained message widgets",
    )?;
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(5)).await?;
    let mut changes = parent
        .subscribe_run()
        .ok_or_else(|| unsupported("parent observation missing"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while parent
            .current_run()
            .is_none_or(|run| run.transcript.is_stale)
        {
            changes
                .changed()
                .await
                .map_err(|_| unsupported("parent observation closed"))?;
        }
        Ok::<(), tekes_agent_contract::Error>(())
    })
    .await
    .map_err(|_| unsupported("parent reconnect history timed out"))??;
    view.update_child_snapshot(parent.scope(), &source, source.snapshot().await?)?;
    scheduled(view, &source).await?;
    let final_rows = retained(view)?;
    require(
        after.iter().all(|row| final_rows.contains(row)),
        "child reconnect replaced existing widgets",
    )?;
    require(
        parent.current_run().unwrap().draft == draft,
        "read-only proof changed parent draft",
    )?;
    Ok(
        serde_json::json!({"readOnlyComponent":true,"childScope":true,"originalReadIntent":true,
        "messages":final_rows.len(),"sourceCallReadonlyDetails":true,"retainedWidgets":true,
        "paginationAvailable":has_more,"reconnect":true,"noComposer":true,"sendRejected":true,
        "parentDraftPreserved":true,"nativeInteraction":false,"readingAnchorAcceptance":false,"completeTranscript":false}),
    )
}
