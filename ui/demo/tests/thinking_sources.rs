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
