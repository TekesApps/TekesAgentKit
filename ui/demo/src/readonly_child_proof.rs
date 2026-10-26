//! Opt-in read-only UI proof over an owned real Native child session.
use crate::component_demo::{ComponentDemo, DemoCommand};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{OutputContentIds, TranscriptContent, TranscriptContentRowKey};
use tekes_agent_contract::{PromptMode, Result, require, unsupported};
use tekes_agent_presentation::{ChildHistoryLayout, ComposerDestination, TranscriptToolDetailKind};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneSnapshot};
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId};

fn source_call_details(content: &TranscriptContent, tool_id: &str) -> Result<OutputContentIds> {
    content
        .retained_tool_activity(tool_id)
        .and_then(|tool| tool.content_ids())
        .ok_or_else(|| unsupported("original source call details missing"))
}

async fn scheduled(view: &mut ComponentDemo, source: &Arc<ChildPaneBinding>) -> Result<()> {
    for _ in 0..3 {
        let commands = view.take_background_commands();
        if commands.is_empty() {
            return Ok(());
        }
        for command in commands {
            // Reconnect also schedules the parent's original read-only feedback
            // callback. Service it like the normal host, without suppressing it
            // or mistaking it for a child mutation.
            let command = match command {
                DemoCommand::ReadInitialFeedback(request) => {
                    view.complete_initial_feedback(request.execute().await)?;
                    continue;
                }
                command => command,
            };
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
    parent.edit_text(
        &ComposerDestination::run(parent.scope().clone()),
        "父会话中文草稿：分页和重连期间保留".into(),
    )?;
    let draft = parent.current_run().unwrap().draft.clone();
    require(
        !draft.text.is_empty(),
        "bounded proof requires a nonempty parent draft",
    )?;
    let source = Arc::new(
        ChildPaneBinding::open_with_page_size(
            runtime.clone(),
            parent.scope().clone(),
            child_id,
            ChildHistoryLayout::SameSession,
            None,
            1,
        )
        .await?,
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
        has_more,
        "real bounded proof needs an earlier page, not only refresh",
    )?;
    require(
        !transcript.items.is_empty() && !transcript.tools.is_empty(),
        "child messages/source call unavailable",
    )?;
    let tool_id = transcript.tools[0].id.clone();
    let tool_turn = transcript.tools[0].turn_id.clone();
    let source_arguments = transcript.tools[0].arguments.clone();
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
    let activity = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .row_id(&TranscriptContentRowKey::ActivityDisclosure(tool_turn));
    if let Some(activity) = activity {
        require(
            view.accessibility_action(activity, None, AccessibilityAction::Press)?
                .is_empty(),
            "source call activity disclosure emitted a mutation",
        )?;
    }
    let ids = source_call_details(
        view.tree
            .get(root)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap(),
        &tool_id,
    )?;
    let sections = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .retained_tool_activity(&tool_id)
        .unwrap()
        .tool_detail_sections();
    require(
        view.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press)?
            .is_empty(),
        "source call disclosure emitted a mutation",
    )?;
    require(
        !sections.is_empty()
            && sections.iter().all(|section| {
                view.tree.text_surface(section.body).is_some_and(|surface| {
                    surface.input.read_only && !surface.input.text.is_empty()
                })
            }),
        "source call sections are not original readonly content",
    )?;
    require(
        sections.iter().any(|section| {
            section.kind == TranscriptToolDetailKind::Input
                && view.tree.text_surface(section.body).is_some_and(|surface| {
                    serde_json::from_str::<serde_json::Value>(&surface.input.text)
                        .ok()
                        .as_ref()
                        == Some(&source_arguments)
                })
        }),
        "source call input text does not match original arguments",
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
        "parentDraftPreserved":true,"nonemptyParentDraft":true,"nativeInteraction":false,"readingAnchorAcceptance":false,"completeTranscript":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tekes_agent_presentation::{RunScope, TranscriptSnapshot};
    #[test]
    fn source_call_queries_original_control_instead_of_a_group_or_folded_row() {
        let scope = RunScope::main("sep", "child");
        let source: TranscriptSnapshot = serde_json::from_value(json!({
            "turns":[{"id":"origin","status":"completed","input":"","startedAt":null,"endedAt":null,
                       "usage":{"inputTokens":0,"outputTokens":0}}],
            "items":[],"tools":[{"id":"source-call","turnId":"origin","name":"task",
                "arguments":{"task_name":"owned"},"result":null,"phase":"done","callView":null,"resultView":null}],
            "hasMoreBefore":false,"isStale":false,"diagnostics":[]
        })).unwrap();
        let mut content =
            TranscriptContent::new(1000, 1001..2000, scope.clone(), Default::default()).unwrap();
        content.update(&scope, 1, 1, &source, true).unwrap();
        let before = source_call_details(&content, "source-call").unwrap();
        content.set_activity_expanded("origin", true).unwrap();
        content.update(&scope, 2, 1, &source, true).unwrap();
        let after = source_call_details(&content, "source-call").unwrap();
        assert_eq!(before.root, after.root);
        assert_eq!(before.body, after.body);
        assert!(source_call_details(&content, "foreign").is_err());
        // The public legacy row lookup is a reading group, not this control.
        assert_ne!(
            content.row_id(&TranscriptContentRowKey::Tool("source-call".into())),
            Some(after.root)
        );
    }
}
