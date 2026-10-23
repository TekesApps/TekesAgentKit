//! Real SEP data and original components; no native window or alternate state machine.
use crate::component_demo::{ComponentDemo, DemoCommand};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_components_ui::{RunPort, TranscriptContent};
use tekes_agent_contract::{Error, Result, require, unsupported};
use tekes_agent_presentation::{ComposerDestination, RunPhase, TranscriptItemKind};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::ModelSelectionPreparation;
use tekes_ui_controls::composition::{AccessibilityAction, ComponentId, ContentEvent};

const MARKER: &str = "ENDPOINT_MIXED_TOOL_RUN_PROOF";
const FINAL: &str = "MIXED_FINAL_OWNED";

pub async fn run(view: &mut ComponentDemo, runtime: &Arc<SessionRuntime>) -> Result<Value> {
    let binding = view.binding().clone();
    let other = view.bindings()[1].clone();
    other.edit_text(
        &ComposerDestination::run(other.scope().clone()),
        "另一个会话的草稿".into(),
    )?;
    let other_draft = other.current_run().unwrap().draft.clone();
    binding.edit_text(
        &ComposerDestination::run(binding.scope().clone()),
        MARKER.into(),
    )?;
    view.sync()?;
    let mut commands =
        view.accessibility_action(view.ids().send, None, AccessibilityAction::Press)?;
    require(
        commands.len() == 1,
        "mixed probe requires one original Send",
    )?;
    let DemoCommand::Send {
        binding: sender,
        ticket,
    } = commands.remove(0)
    else {
        return Err(unsupported("mixed probe Send source unavailable"));
    };
    require(
        Arc::ptr_eq(&binding, &sender),
        "mixed probe changed Send owner",
    )?;
    let dispatch = tokio::spawn(async move {
        sender
            .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
            .await
    });
    let ids = view.ids();
    require(
        view.tree.focus(Some(ids.editor), true),
        "mixed Composer focus unavailable",
    )?;
    require(
        view.event(&ContentEvent::Preedit {
            text: "流式中文组字".into(),
            selection: 6..6,
            clauses: vec![],
            replacement: None,
        })?
        .is_empty(),
        "preedit emitted a command",
    )?;
    let baseline = view
        .tree
        .text_surface(ids.editor)
        .unwrap()
        .input
        .external_baseline();
    let mut changes = binding
        .subscribe_run()
        .ok_or_else(|| unsupported("mixed run is not writable"))?;
    let mut owners: BTreeMap<String, (ComponentId, usize, usize)> = BTreeMap::new();
    let mut live = false;
    let completed = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            view.sync()?;
            // Inspect the exact frame published with the original controls;
            // the producer can advance while this consumer is synchronizing.
            let snapshot = view
                .tree
                .get(ComponentId(ids.root.0 + 16))
                .unwrap()
                .component::<RunPort>()
                .unwrap()
                .displayed_snapshot()
                .ok_or_else(|| unsupported("mixed displayed frame missing"))?
                .clone();
            let content_id = view
                .transcript_content_id(ids)
                .ok_or_else(|| unsupported("mixed Transcript missing"))?;
            let content = view
                .tree
                .get(content_id)
                .unwrap()
                .component::<TranscriptContent>()
                .unwrap();
            for tool in snapshot
                .transcript
                .tools
                .iter()
                .filter(|tool| tool.id.contains("mixed-"))
            {
                let control = content
                    .retained_tool_activity(&tool.id)
                    .ok_or_else(|| unsupported("mixed canonical control missing"))?;
                let address = control as *const _ as usize;
                let entry = owners
                    .entry(tool.id.clone())
                    .or_insert((control.id(), address, 0));
                require(
                    (entry.0, entry.1) == (control.id(), address),
                    "mixed original control replaced",
                )?;
                entry.2 += 1;
                live |= snapshot
                    .run
                    .as_ref()
                    .is_some_and(|run| run.phase == RunPhase::Running);
            }
            require(
                view.tree.focused() == Some(ids.editor),
                "mixed update changed focus",
            )?;
            require(
                view.tree
                    .text_surface(ids.editor)
                    .unwrap()
                    .input
                    .external_baseline()
                    == baseline,
                "mixed update changed preedit",
            )?;
            let finished = snapshot
                .run
                .as_ref()
                .is_some_and(|run| run.phase == RunPhase::Idle)
                && snapshot.transcript.items.iter().any(|item| {
                    item.kind == TranscriptItemKind::Assistant && item.text.contains(FINAL)
                });
            if finished {
                return Ok::<_, Error>(snapshot);
            }
            changes.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| {
        eprintln!(
            "MIXED_TIMEOUT_FRAME {}",
            binding
                .current_run()
                .map(|snapshot| json!(&*snapshot))
                .unwrap_or(Value::Null)
        );
        Error::Timeout
    })??;
    tokio::time::timeout(Duration::from_secs(5), dispatch)
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Disconnected)??;
    let tools: Vec<_> = completed
        .transcript
        .tools
        .iter()
        .filter(|tool| tool.id.contains("mixed-"))
        .collect();
    require(
        tools.len() == 3 && owners.len() == 3,
        "mixed three-call source missing or duplicated",
    )?;
    require(
        live && owners
            .values()
            .all(|(_, _, observations)| *observations >= 2),
        "mixed live controls were not observed across updates",
    )?;
    eprintln!("MIXED_TOOL_SOURCE_RECEIPT {}", json!(tools));
    require(
        tools.iter().all(|tool| {
            tool.result_error.is_none()
                && tool
                    .result
                    .as_ref()
                    .is_some_and(|result| result.to_string().contains("proof line one"))
        }),
        "mixed real read result missing",
    )?;
    require(
        completed
            .transcript
            .items
            .iter()
            .any(|item| item.display_reasoning().contains("MIXED_REASONING_OWNED")),
        "mixed reasoning source missing",
    )?;
    let turn = tools[0].turn_id.clone();
    require(
        tools.iter().all(|tool| tool.turn_id == turn),
        "mixed calls crossed turn identity",
    )?;
    let content_id = view.transcript_content_id(ids).unwrap();
    view.tree
        .get_mut(content_id)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .set_activity_expanded(&turn, true)
        .map_err(unsupported)?;
    view.sync()?;
    let content = view
        .tree
        .get(content_id)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    let group = content
        .tool_run(&tools[0].id)
        .ok_or_else(|| unsupported("mixed visible group missing"))?;
    require(
        group
            .summary()
            .is_some_and(|model| model.tool_ids.len() == 3),
        "mixed calls did not compose into one original group",
    )?;
    require(
        view.accessibility_action(ids.send, None, AccessibilityAction::Press)?
            .is_empty(),
        "mixed preedit allowed another Send",
    )?;
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(15)).await?;
    let mut fresh = binding.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = fresh.borrow_and_update().clone();
            if !snapshot.transcript.is_stale
                && snapshot.connection_epoch > completed.connection_epoch
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .any(|item| item.text.contains(FINAL))
            {
                require(
                    snapshot
                        .transcript
                        .items
                        .iter()
                        .filter(|item| item.kind == TranscriptItemKind::User && item.text == MARKER)
                        .count()
                        == 1,
                    "mixed reconnect duplicated input",
                )?;
                require(
                    snapshot
                        .transcript
                        .tools
                        .iter()
                        .filter(|tool| tool.id.contains("mixed-"))
                        .count()
                        == 3,
                    "mixed reconnect duplicated tools",
                )?;
                break Ok::<_, Error>(());
            }
            fresh.changed().await.map_err(|_| Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| Error::Timeout)??;
    view.sync()?;
    require(
        other.current_run().unwrap().draft == other_draft,
        "mixed source changed other draft",
    )?;
    require(
        view.tree.focused() == Some(ids.editor)
            && view
                .tree
                .text_surface(ids.editor)
                .unwrap()
                .input
                .external_baseline()
                == baseline,
        "mixed reconnect changed Composer",
    )?;
    Ok(
        json!({"realHarness":true,"canonicalTools":3,"realReadResults":true,"reasoningSource":true,
        "oneContinuousGroup":true,"liveControlsObserved":true,"originalControlsRetained":true,
        "syntheticPreeditPreserved":true,"focusPreserved":true,"sendBlockedDuringPreedit":true,
        "otherDraftPreserved":true,"reconnectNoDuplicateInput":true,"reconnectNoDuplicateTools":true,
        "nativeInteraction":false,"physicalIme":false,"readingAnchorAcceptance":false}),
    )
}
