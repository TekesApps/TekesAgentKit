use std::time::Duration;
use tekes_agent_components_ui::{
    Composer, ComposerAffordances, ComposerEntry, ComposerIds, ComposerIntent, IndicatorFacts,
    Indicators, OutputUsage,
};
use tekes_agent_presentation::{ComposerDestination, RunController, RunScope, RunSnapshot};
use tekes_ui_controls::{ControlKey, composition::*};

fn composer(host: &ContentHost, ids: ComposerIds) -> &Composer {
    host.get(ids.root).unwrap().component::<Composer>().unwrap()
}
fn composer_mut(host: &mut ContentHost, ids: ComposerIds) -> &mut Composer {
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
}
fn deliver(host: &mut ContentHost, ids: ComposerIds, event: ContentEvent) -> Vec<ComposerIntent> {
    host.event(&event);
    let actions = host.drain_actions();
    composer_mut(host, ids).take_intents(&actions)
}
fn activate(host: &mut ContentHost, ids: ComposerIds, target: ComponentId) -> Vec<ComposerIntent> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    composer_mut(host, ids).take_intents(&actions)
}
async fn until(c: &RunController, predicate: impl Fn(&RunSnapshot) -> bool) {
    let mut changes = c.subscribe();
    tokio::time::timeout(
        Duration::from_secs(4),
        changes.wait_for(|value| predicate(value)),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn one_primary_tracks_live_run_and_ready_input_and_retains_original_buttons() {
    use tekes_agent_components_ui::{ComposerControlGate, ComposerPrimaryActionKind as Kind};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let ids = ComposerIds::new(9800);
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.run.as_mut().unwrap().phase = tekes_agent_presentation::RunPhase::Running;
    snapshot.run.as_mut().unwrap().can_cancel = true;
    snapshot.can_submit = true;
    snapshot.draft.text = "继续排队的新输入".into();
    let mut view = Composer::new(ids);
    view.update(&snapshot, Default::default()).unwrap();
    assert_eq!(view.primary_action_kind(), Kind::Send);
    let mut host = view.into_host().unwrap();
    assert!(host.get(ids.send).is_some());
    assert!(host.get(ids.cancel).is_none());
    host.accessibility_action(ids.send, None, AccessibilityAction::Press);
    let old_send = host.drain_actions();
    snapshot.revision += 1;
    snapshot.draft.text.clear();
    composer_mut(&mut host, ids)
        .update(&snapshot, Default::default())
        .unwrap();
    assert_eq!(composer(&host, ids).primary_action_kind(), Kind::Stop);
    assert_eq!(composer(&host, ids).primary_action_id(), ids.cancel);
    assert!(host.get(ids.send).is_none());
    assert!(host.get(ids.cancel).is_some());
    assert!(
        composer_mut(&mut host, ids)
            .take_intents(&old_send)
            .is_empty()
    );
    assert!(matches!(
        activate(&mut host, ids, ids.cancel).as_slice(),
        [ComposerIntent::Cancel { .. }]
    ));
    composer_mut(&mut host, ids)
        .update_gated(&snapshot, Default::default(), ComposerControlGate::CLOSED)
        .unwrap();
    assert_eq!(composer(&host, ids).primary_action_kind(), Kind::Stop);
    assert!(activate(&mut host, ids, ids.cancel).is_empty());
    snapshot.revision += 1;
    snapshot.draft.text = "恢复同一个发送按钮".into();
    composer_mut(&mut host, ids)
        .update(&snapshot, Default::default())
        .unwrap();
    assert_eq!(composer(&host, ids).primary_action_id(), ids.send);
    assert_eq!(
        host.get(ids.send)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .activations,
        1
    );
    assert!(host.get(ids.cancel).is_none());
    snapshot.revision += 1;
    snapshot.draft.text.clear();
    snapshot.run.as_mut().unwrap().phase = tekes_agent_presentation::RunPhase::Idle;
    snapshot.run.as_mut().unwrap().can_cancel = false;
    for turn in &mut snapshot.transcript.turns {
        turn.status = "running".into();
    }
    composer_mut(&mut host, ids)
        .update(&snapshot, Default::default())
        .unwrap();
    assert_eq!(composer(&host, ids).primary_action_kind(), Kind::Send);
    assert!(activate(&mut host, ids, ids.send).is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn standalone_composer_ime_and_switch_keep_destination_and_draft() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let ids = ComposerIds::new(800);
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(host.focus(Some(ids.editor), true));
    assert!(
        deliver(
            &mut host,
            ids,
            ContentEvent::Preedit {
                text: "zhongwen".into(),
                selection: 8..8,
                clauses: std::iter::once(0..8).collect(),
                replacement: None
            }
        )
        .is_empty()
    );
    assert!(composer(&host, ids).is_composing());
    assert!(activate(&mut host, ids, ids.send).is_empty());
    assert!(
        deliver(
            &mut host,
            ids,
            ContentEvent::Key {
                key: ControlKey::Enter,
                down: true
            }
        )
        .is_empty()
    );
    controller
        .open(RunScope::main("fixture", "b"))
        .await
        .unwrap();
    assert!(
        composer_mut(&mut host, ids)
            .update(&controller.snapshot(), ComposerAffordances::default())
            .is_err()
    );
    let intents = deliver(
        &mut host,
        ids,
        ContentEvent::Commit {
            text: "中文草稿 A".into(),
            replacement: None,
        },
    );
    let [ComposerIntent::Edit { destination, text }] = intents.as_slice() else {
        panic!("expected one committed draft");
    };
    assert_eq!(
        destination,
        &ComposerDestination::run(RunScope::main("fixture", "a"))
    );
    assert_eq!(text, "中文草稿 A");
    controller.edit_text(destination, text.clone()).unwrap();
    host.focus(None, false);
    composer_mut(&mut host, ids)
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    assert!(composer(&host, ids).editor().input.text.is_empty());
    controller
        .open(RunScope::main("fixture", "a"))
        .await
        .unwrap();
    composer_mut(&mut host, ids)
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    assert_eq!(composer(&host, ids).editor().input.text, "中文草稿 A");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn send_readiness_uses_shared_compiler_for_empty_and_artifact_only_drafts() {
    use tekes_agent_presentation::{ComposerArtifact, ComposerDraft};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let ids = ComposerIds::new(8800);
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    let mut host = view.into_host().unwrap();
    assert!(activate(&mut host, ids, ids.send).is_empty());
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    controller.edit_text(&destination, " \n\t ".into()).unwrap();
    composer_mut(&mut host, ids)
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    assert!(activate(&mut host, ids, ids.send).is_empty());
    controller
        .edit(
            &destination,
            ComposerDraft {
                artifacts: vec![ComposerArtifact::Attachment {
                    id: "image".into(),
                    name: "fixture image".into(),
                    content: serde_json::json!({"type":"image","url":"fixture://image"}),
                    facts: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    composer_mut(&mut host, ids)
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    assert!(matches!(
        activate(&mut host, ids, ids.send).as_slice(),
        [ComposerIntent::Send { .. }]
    ));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn host_affordance_and_run_authority_both_gate_composer_callbacks() {
    use tekes_agent_components_ui::ComposerControlGate;

    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&controller, |snapshot| snapshot.can_submit).await;
    let ids = ComposerIds::new(9200);
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.draft.text = "受控回调".into();
    let affordances = ComposerAffordances {
        attachment: true,
        model: true,
        preset: true,
    };
    let mut view = Composer::new(ids);
    view.update_gated(&snapshot, affordances, ComposerControlGate::OPEN)
        .unwrap();
    let mut host = view.into_host().unwrap();

    // Actions queued under an earlier open publication cannot outlive a pane
    // authority change, even when the host had advertised every entry.
    host.accessibility_action(ids.send, None, AccessibilityAction::Press);
    host.accessibility_action(ids.model, None, AccessibilityAction::Press);
    let stale = host.drain_actions();
    assert_eq!(stale.len(), 2, "both original controls must emit actions");
    snapshot.revision += 1;
    composer_mut(&mut host, ids)
        .update_gated(&snapshot, affordances, ComposerControlGate::CLOSED)
        .unwrap();
    assert!(composer_mut(&mut host, ids).take_intents(&stale).is_empty());
    assert!(activate(&mut host, ids, ids.send).is_empty());
    assert!(activate(&mut host, ids, ids.model).is_empty());

    // A host may keep the composer mounted while withdrawing a specific
    // capability. A stale or synthetic model activation must not recreate it.
    snapshot.revision += 1;
    composer_mut(&mut host, ids)
        .update_gated(&snapshot, affordances, ComposerControlGate::OPEN)
        .unwrap();
    assert!(matches!(
        activate(&mut host, ids, ids.model).as_slice(),
        [ComposerIntent::OpenEntry {
            entry: ComposerEntry::Model,
            ..
        }]
    ));
    host.accessibility_action(ids.model, None, AccessibilityAction::Press);
    let stale_model = host.drain_actions();
    assert_eq!(stale_model.len(), 1);
    snapshot.revision += 1;
    let attachment_only = ComposerAffordances {
        attachment: true,
        model: false,
        preset: false,
    };
    composer_mut(&mut host, ids)
        .update_gated(&snapshot, attachment_only, ComposerControlGate::OPEN)
        .unwrap();
    assert!(
        composer_mut(&mut host, ids)
            .take_intents(&stale_model)
            .is_empty()
    );
    assert!(activate(&mut host, ids, ids.model).is_empty());

    // Host UI capability does not override canonical Run admission authority.
    snapshot.revision += 1;
    snapshot.can_submit = false;
    snapshot.run.as_mut().unwrap().can_submit = false;
    composer_mut(&mut host, ids)
        .update_gated(&snapshot, affordances, ComposerControlGate::OPEN)
        .unwrap();
    assert!(activate(&mut host, ids, ids.send).is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn standalone_composer_send_receipt_preserves_later_input_and_scoped_entries() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&controller, |s| s.can_submit).await;
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "first input".into();
    controller.edit(&destination, draft).unwrap();
    let ids = ComposerIds::new(1000);
    let affordances = ComposerAffordances {
        attachment: true,
        model: true,
        preset: false,
    };
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), affordances).unwrap();
    let mut host = view.into_host().unwrap();
    assert!(host.get(ids.preset).is_none());
    assert_eq!(
        activate(&mut host, ids, ids.model),
        vec![ComposerIntent::OpenEntry {
            destination: destination.clone(),
            entry: ComposerEntry::Model
        }]
    );
    assert_eq!(
        activate(&mut host, ids, ids.send),
        vec![ComposerIntent::Send {
            destination: destination.clone()
        }]
    );
    let ticket = controller
        .prepare(&destination, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    host.focus(Some(ids.editor), true);
    let intent = deliver(
        &mut host,
        ids,
        ContentEvent::Commit {
            text: " next draft".into(),
            replacement: None,
        },
    );
    assert!(matches!(intent.as_slice(), [ComposerIntent::Edit { .. }]));
    let after_typing = composer(&host, ids).editor().input.text.clone();
    controller.dispatch(ticket, None).await.unwrap();
    let _ = composer_mut(&mut host, ids).update(&controller.snapshot(), affordances);
    assert_eq!(
        composer(&host, ids).editor().input.text,
        after_typing,
        "late clear erased newer typing"
    );
    assert!(
        activate(&mut host, ids, ids.send).is_empty(),
        "conflicted baseline cannot send"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn standalone_composer_real_dispatch_cancel_and_explicit_reconnect_do_not_replay() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    until(&controller, |s| s.can_submit).await;
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    let mut draft = controller.snapshot().draft.clone();
    draft.text = "formal component stream".into();
    controller.edit(&destination, draft).unwrap();
    let ids = ComposerIds::new(1200);
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    let mut host = view.into_host().unwrap();
    let intents = activate(&mut host, ids, ids.send);
    let [
        ComposerIntent::Send {
            destination: captured,
        },
    ] = intents.as_slice()
    else {
        panic!("send action");
    };
    let ticket = controller
        .prepare(captured, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    controller.dispatch(ticket, None).await.unwrap();
    until(&controller, |s| {
        s.run.as_ref().is_some_and(|run| run.can_cancel)
    })
    .await;
    composer_mut(&mut host, ids)
        .update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    let intents = activate(&mut host, ids, ids.cancel);
    let [ComposerIntent::Cancel { scope }] = intents.as_slice() else {
        panic!("cancel action");
    };
    controller.cancel(scope).await.unwrap();
    until(&controller, |s| {
        s.run.as_ref().is_some_and(|run| !run.can_cancel)
    })
    .await;
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    until(&controller, |s| !s.transcript.is_stale).await;
    assert_eq!(
        controller
            .snapshot()
            .transcript
            .items
            .iter()
            .filter(|item| item.text == "formal component stream")
            .count(),
        1
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn indicators_preserve_unknown_zero_partial_and_errors_without_inference() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    let mut component = Indicators::default();
    component.update(&snapshot).unwrap();
    assert!(
        !component
            .facts()
            .unwrap()
            .accessible_text()
            .contains("已完成")
    );
    let turn = snapshot.transcript.turns.last_mut().unwrap();
    turn.status = "running".into();
    turn.live_usage = None;
    assert_eq!(IndicatorFacts::read(&snapshot).output_usage, None);
    snapshot.transcript.turns.last_mut().unwrap().live_usage =
        Some(serde_json::from_value(serde_json::json!({"outputTokens":0})).unwrap());
    assert_eq!(
        IndicatorFacts::read(&snapshot).output_usage,
        Some(OutputUsage::Live { output_tokens: 0 })
    );
    let turn = snapshot.transcript.turns.last_mut().unwrap();
    turn.status = "completed".into();
    turn.reported_usage = Some(
        serde_json::from_value(serde_json::json!({"outputTokens":38,"isComplete":false})).unwrap(),
    );
    snapshot.connection.message = Some("source unavailable".into());
    snapshot.transcript.is_stale = true;
    let facts = IndicatorFacts::read(&snapshot);
    assert_eq!(
        facts.output_usage,
        Some(OutputUsage::Reported {
            output_tokens: 38,
            complete: false
        })
    );
    assert!(facts.accessible_text().contains("部分报告"));
    assert!(facts.accessible_text().contains("source unavailable"));
    snapshot.revision += 1;
    component.update(&snapshot).unwrap();
    snapshot.revision -= 1;
    assert!(component.update(&snapshot).is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn delayed_text_intent_cannot_erase_host_settings_or_cross_to_new_selection() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let destination = ComposerDestination::run(RunScope::main("fixture", "a"));
    let ids = ComposerIds::new(1400);
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), ComposerAffordances::default())
        .unwrap();
    let mut host = view.into_host().unwrap();
    host.focus(Some(ids.editor), true);
    let intents = deliver(
        &mut host,
        ids,
        ContentEvent::Commit {
            text: "pending local input".into(),
            replacement: None,
        },
    );
    let [
        ComposerIntent::Edit {
            destination: captured,
            text,
        },
    ] = intents.as_slice()
    else {
        panic!("text intent");
    };
    let mut newer = controller.snapshot().draft.clone();
    newer.settings.insert(
        "host-choice".into(),
        serde_json::json!("updated after input"),
    );
    controller.edit(&destination, newer).unwrap();
    controller
        .open(RunScope::main("fixture", "b"))
        .await
        .unwrap();
    controller.edit_text(captured, text.clone()).unwrap();
    assert!(controller.snapshot().draft.text.is_empty());
    controller
        .open(RunScope::main("fixture", "a"))
        .await
        .unwrap();
    assert_eq!(controller.snapshot().draft.text, "pending local input");
    assert_eq!(
        controller.snapshot().draft.settings["host-choice"],
        "updated after input"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn primary_stop_preserves_editor_preedit_and_focus_when_send_replaces_it() {
    use tekes_agent_presentation::RunPhase;
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut frame = (*controller.snapshot()).clone();
    frame.run.as_mut().unwrap().phase = RunPhase::Running;
    frame.run.as_mut().unwrap().can_cancel = true;
    frame.can_submit = true;
    frame.draft.text.clear();
    tekes_agent_ui_consumer_check::verify_primary_keeps_editor(&frame).unwrap();
    let ids = ComposerIds::new(9910);
    let mut view = Composer::new(ids);
    view.update(&frame, Default::default()).unwrap();
    let mut host = view.into_host().unwrap();
    host.focus(Some(ids.editor), true);
    assert!(
        deliver(
            &mut host,
            ids,
            ContentEvent::Preedit {
                text: "取消期间中文组字".into(),
                selection: 8..8,
                clauses: vec![],
                replacement: None,
            }
        )
        .is_empty()
    );
    let baseline = host
        .text_surface(ids.editor)
        .unwrap()
        .input
        .external_baseline();
    assert!(matches!(
        activate(&mut host, ids, ids.cancel).as_slice(),
        [ComposerIntent::Cancel { .. }]
    ));
    assert_eq!(
        host.focused(),
        Some(ids.editor),
        "primary Stop must not adopt editor focus"
    );
    assert_eq!(
        host.text_surface(ids.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    frame.revision += 1;
    frame.run.as_mut().unwrap().phase = RunPhase::Idle;
    frame.run.as_mut().unwrap().can_cancel = false;
    composer_mut(&mut host, ids)
        .update(&frame, Default::default())
        .unwrap();
    assert!(host.get(ids.cancel).is_none());
    assert_eq!(host.focused(), Some(ids.editor));
    assert_eq!(
        host.text_surface(ids.editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert!(activate(&mut host, ids, ids.send).is_empty());
    runtime.stop().await.unwrap();
}
