//! Public fixture SEP -> existing runtime/presentation -> second consumer -> SDK.
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{OutputMessage, TranscriptThinkingMode};
use tekes_agent_presentation::{ComposerDestination, RunScope, TranscriptItemKind};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_consumer_check::install_run_thinking_output;
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_ui_controls::composition::{AccessibilityAction, TextFieldContent};
#[tokio::test]
async fn normalized_run_sources_reach_second_consumer_and_survive_reconnect_without_a_host_state_machine()
 {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_reading_details()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let scope = RunScope::main("fixture", "a");
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), scope.clone())
            .await
            .unwrap(),
    );
    let snapshot = binding.current_run().unwrap();
    let source = snapshot
        .transcript
        .items
        .iter()
        .find(|row| row.kind == TranscriptItemKind::Assistant && !row.reasoning.is_empty())
        .unwrap();
    let (item_id, original_text) = (source.id.clone(), source.reasoning.clone());
    // Read-only envelopes use their own read-invocation publication, not a
    // fabricated writable Run. This is a value/API check, not a Native SEP proof.
    let readonly = tekes_agent_ui_bindings::PaneSnapshot::ReadOnlyChild {
        scope: scope.clone(),
        publication: snapshot.revision + 1000,
        connection_epoch: snapshot.connection_epoch,
        connection: snapshot.connection.clone(),
        transcript: snapshot.transcript.clone(),
    };
    let read_frame = readonly
        .thinking_frame(
            &source.turn_id,
            tekes_agent_components_ui::TranscriptThinkingDisplayInput {
                mode: TranscriptThinkingMode::None,
                display_revision: 1,
                elapsed_seconds: 1,
            },
        )
        .unwrap();
    assert!(read_frame.matches_source(
        &scope,
        &source.turn_id,
        snapshot.revision + 1000,
        snapshot.connection_epoch
    ));
    assert!(!read_frame.matches_source(
        &scope,
        &source.turn_id,
        snapshot.revision,
        snapshot.connection_epoch
    ));

    let mut view = OutputMessage::new(901, scope.clone());
    install_run_thinking_output(
        &mut view,
        &snapshot,
        &item_id,
        TranscriptThinkingMode::None,
        1,
        1,
    )
    .unwrap();
    let ids = view
        .content_ids()
        .into_iter()
        .find(|row| row.kind == tekes_agent_components_ui::OutputContentKind::Reasoning)
        .unwrap();
    let root = view.id();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(ids.disclosure.unwrap(), None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<OutputMessage>()
            .unwrap()
            .handle_actions(&actions)
    );
    assert_eq!(
        host.text_surface(ids.body).unwrap().input.text,
        original_text.trim()
    );
    assert_eq!(
        binding
            .current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .find(|row| row.id == item_id)
            .unwrap()
            .reasoning,
        original_text
    );
    assert!(host.focus(Some(ids.body), true));
    host.get_mut(ids.body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..2);
    binding
        .edit_text(&ComposerDestination::run(scope), "保留主草稿".into())
        .unwrap();
    let draft = binding.current_run().unwrap().draft.clone();
    let old_epoch = snapshot.connection_epoch;
    let mut changes = binding.subscribe_run().unwrap();
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let renewed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = binding.current_run().unwrap();
            if snapshot.connection_epoch > old_epoch && !snapshot.transcript.is_stale {
                break snapshot;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    install_run_thinking_output(
        host.get_mut(root)
            .unwrap()
            .component_mut::<OutputMessage>()
            .unwrap(),
        &renewed,
        &item_id,
        TranscriptThinkingMode::None,
        1,
        2,
    )
    .unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids()
            .into_iter()
            .find(|row| row.kind == tekes_agent_components_ui::OutputContentKind::Reasoning)
            .unwrap(),
        ids
    );
    assert_eq!(host.focused(), Some(ids.body));
    assert_eq!(host.text_surface(ids.body).unwrap().input.selection(), 0..2);
    assert_eq!(binding.current_run().unwrap().draft, draft);
    assert!(renewed.submissions.is_empty());
    runtime.stop().await.unwrap();
}

fn paging(
    scope: &RunScope,
    snapshot: &tekes_agent_presentation::RunSnapshot,
) -> tekes_agent_components_ui::TranscriptPagingFacts {
    tekes_agent_components_ui::TranscriptPagingFacts {
        scope: scope.clone(),
        supported: false,
        has_more_before: snapshot.transcript.has_more_before,
        is_stale: snapshot.transcript.is_stale,
        source_ready: true,
        read: tekes_agent_components_ui::TranscriptPageRead::Idle,
    }
}
fn display(
    revision: u64,
    mode: TranscriptThinkingMode,
) -> tekes_agent_components_ui::TranscriptThinkingDisplayInput {
    tekes_agent_components_ui::TranscriptThinkingDisplayInput {
        mode,
        display_revision: revision,
        elapsed_seconds: revision,
    }
}

#[tokio::test]
async fn whole_run_thinking_preflight_fences_old_plans_and_preserves_original_controls() {
    use tekes_agent_components_ui::*;
    use tekes_ui_controls::composition::*;
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_reading_details()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let scope = RunScope::main("fixture", "a");
    let binding = RunBinding::open_main(runtime.clone(), scope.clone())
        .await
        .unwrap();
    let snapshot = binding.current_run().unwrap();
    let mut run = RunPort::standard(2000, 2010..2200, scope.clone()).unwrap();
    run.set_thinking_display(display(1, TranscriptThinkingMode::Compact))
        .unwrap();
    run.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&scope, &snapshot),
        true,
    )
    .unwrap();
    let first = tekes_agent_ui_consumer_check::prepare_thinking_run(
        &mut run,
        &snapshot,
        display(1, TranscriptThinkingMode::Compact),
        paging(&scope, &snapshot),
    )
    .unwrap();
    let rows = first.item_ids();
    run.set_thinking_display(display(2, TranscriptThinkingMode::None))
        .unwrap();
    assert!(run.apply_prepared_update(&first).is_err());
    assert_eq!(
        run.displayed_snapshot().unwrap().revision,
        snapshot.revision
    );
    assert!(!run.transcript().unwrap().row_keys().is_empty());
    let plan = tekes_agent_ui_consumer_check::prepare_thinking_run(
        &mut run,
        &snapshot,
        display(2, TranscriptThinkingMode::None),
        paging(&scope, &snapshot),
    )
    .unwrap();
    assert_eq!(plan.item_ids(), rows);
    run.apply_prepared_update(&plan).unwrap();
    let root = run.ids().root;
    let editor = run.ids().composer.editor;
    let mut host = run.into_host().unwrap();
    let output_id = rows
        .iter()
        .copied()
        .find(|id| {
            host.get(*id).is_some_and(|node| {
                node.component::<OutputMessage>().is_some_and(|view| {
                    view.content_ids()
                        .iter()
                        .any(|part| part.kind == OutputContentKind::Reasoning)
                })
            })
        })
        .unwrap();
    let original = host
        .get(output_id)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids();
    assert!(host.focus(Some(editor), true));
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let run = host
        .get_mut(root)
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap();
    assert!(
        run.set_thinking_display(display(1, TranscriptThinkingMode::Compact))
            .is_err()
    );
    assert!(
        run.set_thinking_display(display(2, TranscriptThinkingMode::Compact))
            .is_err()
    );
    let plan = tekes_agent_ui_consumer_check::prepare_thinking_run(
        run,
        &snapshot,
        display(3, TranscriptThinkingMode::Compact),
        paging(&scope, &snapshot),
    )
    .unwrap();
    assert!(plan.controlled_draft().is_none());
    run.apply_prepared_update(&plan).unwrap();
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(
        host.text_surface(editor).unwrap().input.external_baseline(),
        baseline
    );
    assert_eq!(
        host.get(output_id)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids(),
        original
    );
    assert!(binding.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn missing_provenance_cannot_partially_replace_a_whole_transcript_or_consume_row_ids() {
    use tekes_agent_components_ui::*;
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_reading_details()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let scope = RunScope::main("fixture", "a");
    let binding = RunBinding::open_main(runtime.clone(), scope.clone())
        .await
        .unwrap();
    let snapshot = binding.current_run().unwrap();
    let mut content =
        TranscriptContent::new(3000, 3010..3200, scope.clone(), Default::default()).unwrap();
    content
        .set_thinking_display(display(1, TranscriptThinkingMode::Compact))
        .unwrap();
    let valid = content
        .prepare_update(
            &scope,
            snapshot.revision,
            snapshot.connection_epoch,
            &snapshot.transcript,
            true,
        )
        .unwrap();
    let ids = valid.item_ids();
    let mut invalid = snapshot.transcript.clone();
    invalid.thinking_sources.by_id.clear();
    assert!(
        content
            .update(
                &scope,
                snapshot.revision,
                snapshot.connection_epoch,
                &invalid,
                true
            )
            .is_err()
    );
    assert!(content.row_keys().is_empty());
    let after = content
        .prepare_update(
            &scope,
            snapshot.revision,
            snapshot.connection_epoch,
            &snapshot.transcript,
            true,
        )
        .unwrap();
    assert_eq!(after.item_ids(), ids);
    content.apply_prepared_update(&after).unwrap();
    let keys = content.row_keys().to_vec();
    assert!(
        content
            .update(
                &scope,
                snapshot.revision + 1,
                snapshot.connection_epoch,
                &invalid,
                true
            )
            .is_err()
    );
    assert_eq!(content.row_keys(), keys);
    content
        .update(
            &scope,
            snapshot.revision + 1,
            snapshot.connection_epoch,
            &snapshot.transcript,
            true,
        )
        .unwrap();
    runtime.stop().await.unwrap();
}

#[test]
fn readonly_live_status_ticks_use_child_publication_and_retire_preference_plans() {
    use serde_json::json;
    use tekes_agent_components_ui::*;
    use tekes_agent_presentation::PresentationStore;
    use tekes_agent_session::ConnectionState;
    use tekes_agent_session::MemoryProjectionStore;
    let scope = RunScope::main("sep", "s");
    let mut store = MemoryProjectionStore::new("sep");
    store.apply("journal:s",json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"g","throughSequence":10,"windowLimit":50,"entries":[{"event":{"type":"turn/start","seq":10,"time":1,"data":{"turn":1}}}],"hasMoreBefore":false}})).unwrap();
    store.apply("journal:s",json!({"type":"transient","generation":"g","sessionId":"s","event":{"type":"assistant/chunk","seq":999,"time":2,"data":{"turn":1,"step":1,"chunk":{"type":"reasoning-delta","delta":"保留的中文思考"}}}})).unwrap();
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    let mut presentation = PresentationStore::default();
    presentation
        .select(ComposerDestination::run(scope.clone()))
        .unwrap();
    let source = presentation.refresh(&store).unwrap();
    let mut view =
        ReadOnlyTranscript::standard(4000, 4010..4200, scope.clone(), Default::default()).unwrap();
    view.set_thinking_display(display(1, TranscriptThinkingMode::None))
        .unwrap();
    let input = ReadOnlyTranscriptInput {
        scope: &scope,
        publication: 500,
        connection_epoch: source.connection_epoch,
        connection: &source.connection,
        transcript: &source.transcript,
        frame_current: true,
        source_ready: true,
        supports_pagination: false,
        reading: false,
        read_error: None,
    };
    view.update(input).unwrap();
    let ids = view.prepare_update(input).unwrap().item_ids();
    let root = view.ids().root;
    let mut host = view.into_host().unwrap();
    let row = ids
        .iter()
        .copied()
        .find(|id| {
            host.get(*id)
                .is_some_and(|node| node.component::<OutputMessage>().is_some())
        })
        .unwrap();
    let parts = host
        .get(row)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].kind, OutputContentKind::Status);
    let text_before = host.text_surface(parts[0].body).unwrap().input.text.clone();
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap();
    let old = view.prepare_update(input).unwrap();
    view.set_thinking_display(display(2, TranscriptThinkingMode::None))
        .unwrap();
    assert!(view.apply_prepared_update(&old).is_err());
    let tick = view.prepare_update(input).unwrap();
    assert_eq!(tick.item_ids(), ids);
    view.apply_prepared_update(&tick).unwrap();
    assert_eq!(
        host.get(row)
            .unwrap()
            .component::<OutputMessage>()
            .unwrap()
            .content_ids(),
        parts
    );
    assert_ne!(
        host.text_surface(parts[0].body).unwrap().input.text,
        text_before
    );
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap();
    view.set_thinking_display(display(3, TranscriptThinkingMode::Compact))
        .unwrap();
    view.update(input).unwrap();
    let parts = host
        .get(row)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].kind, OutputContentKind::Reasoning);
    assert!(parts[0].disclosure.is_some());
}

#[tokio::test]
async fn controlled_whitespace_record_leaves_no_transcript_row_or_reading_gap() {
    use tekes_agent_components_ui::*;
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_reading_details()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let scope = RunScope::main("fixture", "a");
    let binding = RunBinding::open_main(runtime.clone(), scope.clone())
        .await
        .unwrap();
    let original = binding.current_run().unwrap();
    // Explicit captured value fixture, not a claim that this extra record was
    // emitted by the protocol fixture. No production identity is inferred.
    let mut transcript = original.transcript.clone();
    let mut whitespace = transcript
        .items
        .iter()
        .find(|row| row.kind == TranscriptItemKind::Assistant)
        .unwrap()
        .clone();
    let facts = transcript.thinking_sources.by_id[&whitespace.id].clone();
    whitespace.id = "explicit-whitespace-fixture".into();
    whitespace.record_id = None;
    whitespace.reconciliation_id = None;
    whitespace.source_sequence = Some(1_000_000);
    whitespace.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 1_000_000,
        content_index: None,
    });
    whitespace.text.clear();
    whitespace.reasoning = " \n\t".into();
    whitespace.attachments.clear();
    whitespace.content_blocks = Some(vec![TranscriptContentBlock {
        index: 0,
        content: TranscriptBlockContent::Reasoning {
            text: whitespace.reasoning.clone(),
        },
    }]);
    let id = whitespace.id.clone();
    transcript.thinking_sources.by_id.insert(id.clone(), facts);
    transcript.items.push(whitespace);
    let mut content =
        TranscriptContent::new(5000, 5010..5300, scope.clone(), Default::default()).unwrap();
    content
        .set_thinking_display(display(1, TranscriptThinkingMode::Compact))
        .unwrap();
    content
        .update(
            &scope,
            original.revision,
            original.connection_epoch,
            &transcript,
            true,
        )
        .unwrap();
    assert!(
        content
            .message_id(TranscriptMessageKey::Item(&id))
            .is_none()
    );
    assert_eq!(transcript.items.last().unwrap().reasoning, " \n\t");
    runtime.stop().await.unwrap();
}
