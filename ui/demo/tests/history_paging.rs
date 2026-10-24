use std::{sync::Arc, time::Duration};
use tekes_agent_presentation::{ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{HistoryPagePhase, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
async fn setup(fixture: Fixture) -> (Arc<SessionRuntime>, Arc<RunBinding>, Arc<RunBinding>) {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture)));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    (runtime, a, b)
}
#[tokio::test]
async fn original_scope_single_flight_watch_and_frozen_page_do_not_touch_b_draft() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default().with_history_page_gate(gate.clone());
    let (runtime, a, b) = setup(fixture.clone()).await;
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "B 独立草稿".into(),
    )
    .unwrap();
    let draft = b.current_run().unwrap().draft.clone();
    let before = runtime.inspect(|s| {
        (
            s.sessions["a"].history_through_sequence,
            s.sessions["a"].history_base_sequence,
        )
    });
    let mut changes = a.subscribe_history_pages();
    let request = a.begin_history_page().unwrap();
    assert_eq!(request.scope(), a.scope());
    changes.changed().await.unwrap();
    assert_eq!(a.history_page_state().phase, HistoryPagePhase::Loading);
    assert!(a.begin_history_page().is_err());
    let read = tokio::spawn(request.load());
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.history_page_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(a.load_earlier().await.is_err());
    assert_eq!(fixture.history_page_calls(), 1);
    assert_eq!(b.history_page_state().phase, HistoryPagePhase::Idle);
    gate.add_permits(1);
    assert!(read.await.unwrap().unwrap());
    changes.changed().await.unwrap();
    assert_eq!(
        a.history_page_state().phase,
        HistoryPagePhase::Loaded { extended: true }
    );
    let after = runtime.inspect(|s| {
        (
            s.sessions["a"].history_through_sequence,
            s.sessions["a"].history_base_sequence,
        )
    });
    assert_eq!(before.0, after.0);
    assert!(after.1 < before.1);
    assert_eq!(b.current_run().unwrap().draft, draft);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn failed_page_never_retries_without_a_new_explicit_gesture() {
    let fixture = Fixture::default();
    fixture.set_history_page_fails(true);
    let (runtime, a, _) = setup(fixture.clone()).await;
    assert!(a.load_earlier().await.is_err());
    assert!(matches!(
        a.history_page_state().phase,
        HistoryPagePhase::Failed { .. }
    ));
    assert_eq!(fixture.history_page_calls(), 1);
    fixture.set_history_page_fails(false);
    assert!(a.begin_history_page().unwrap().load().await.unwrap());
    assert_eq!(fixture.history_page_calls(), 2);
    assert_eq!(
        a.history_page_state().phase,
        HistoryPagePhase::Loaded { extended: true }
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn dropping_unstarted_or_aborted_requests_is_interrupted_not_fake_completion() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default().with_history_page_gate(gate.clone());
    let (runtime, a, _) = setup(fixture.clone()).await;
    drop(a.begin_history_page().unwrap());
    assert_eq!(a.history_page_state().phase, HistoryPagePhase::Interrupted);
    assert_eq!(fixture.history_page_calls(), 0);
    let read = tokio::spawn(a.begin_history_page().unwrap().load());
    tokio::time::timeout(Duration::from_secs(3), async {
        while fixture.history_page_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    read.abort();
    assert!(read.await.unwrap_err().is_cancelled());
    assert_eq!(a.history_page_state().phase, HistoryPagePhase::Interrupted);
    gate.add_permits(1); // Only the observer was aborted; the backend may still complete.
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn old_epoch_drop_cannot_clear_a_new_request_and_expired_capture_does_no_rpc() {
    let fixture = Fixture::default();
    let (runtime, a, _) = setup(fixture.clone()).await;
    let old = a.begin_history_page().unwrap();
    runtime.reconnect().await.unwrap();
    assert!(!a.history_page_state().is_current);
    assert!(a.history_page_state().source_ready);
    assert_eq!(
        a.history_page_state()
            .controls(&a.current_run().unwrap().transcript, true)
            .read,
        tekes_agent_components_ui::TranscriptPageRead::Idle
    );
    let fresh = a.begin_history_page().unwrap();
    drop(old);
    assert_eq!(a.history_page_state().phase, HistoryPagePhase::Loading);
    assert!(fresh.load().await.unwrap());
    assert_eq!(fixture.history_page_calls(), 1);
    let expired = a.begin_history_page().unwrap();
    runtime.reconnect().await.unwrap();
    assert!(expired.load().await.is_err());
    assert_eq!(fixture.history_page_calls(), 1);
    assert!(!a.history_page_state().is_current);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn retiring_a_child_pointer_revokes_a_captured_read_without_rpc() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::ChildPaneBinding;
    let fixture = Fixture::default().with_children();
    let (runtime, parent, _) = setup(fixture.clone()).await;
    let child = ChildPaneBinding::for_run(
        &parent,
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    let request = child.binding().begin_history_page().unwrap();
    child.retire();
    assert!(request.load().await.is_err());
    assert!(!child.binding().history_page_state().is_current);
    assert_eq!(fixture.history_page_calls(), 0);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn public_paging_button_captures_before_navigation_and_disables_duplicate_activation() {
    use tekes_agent_ui_demo::component_demo::{ComponentDemo, DemoCommand, SELECT_B};
    use tekes_ui_controls::composition::AccessibilityAction;
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (runtime, a, b) = setup(Fixture::default().with_history_page_gate(gate.clone())).await;
    let mut view = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    let button = view.history_page_button().unwrap();
    let commands = view
        .accessibility_action(button, None, AccessibilityAction::Press)
        .unwrap();
    let mut requests = commands.into_iter().filter_map(|command| match command {
        DemoCommand::ReadHistoryPage(request) => Some(request),
        _ => None,
    });
    let request = requests.next().expect("public paging action");
    assert!(requests.next().is_none());
    assert_eq!(request.scope(), a.scope());
    assert_eq!(a.history_page_state().phase, HistoryPagePhase::Loading);
    assert!(
        view.accessibility_action(button, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    let read = tokio::spawn(request.load());
    gate.add_permits(1);
    assert!(read.await.unwrap().unwrap());
    view.sync().unwrap();
    assert_eq!(view.selected(), 1);
    assert_eq!(
        a.history_page_state().phase,
        HistoryPagePhase::Loaded { extended: true }
    );
    assert_eq!(b.history_page_state().phase, HistoryPagePhase::Idle);
    runtime.stop().await.unwrap();
}
