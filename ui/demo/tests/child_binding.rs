use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{PromptMode, Result};
use tekes_agent_presentation::{
    ChildComposerAvailability, ChildHistoryLayout, ComposerDestination, PreparedSubmission,
    RunScope, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess, PaneSnapshot, RunBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
struct Prepare;
struct CustomHistory;
#[async_trait::async_trait]
impl tekes_agent_presentation::ChildHistorySource for CustomHistory {
    async fn read(
        &self,
        _: &RunScope,
        _: &str,
    ) -> Result<tekes_agent_presentation::TranscriptSnapshot> {
        Ok(tekes_agent_presentation::TranscriptSnapshot {
            has_more_before: true,
            ..Default::default()
        })
    }
}
#[async_trait::async_trait]
impl SubmissionPreparation for Prepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}
async fn setup() -> (Fixture, Arc<SessionRuntime>) {
    let fixture = Fixture::default().with_children();
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    (fixture, runtime)
}
#[tokio::test]
async fn nested_child_uses_direct_parent_and_existing_catalog_policy_without_changing_readable_history()
 {
    let (fixture, runtime) = setup().await;
    let parent = RunScope {
        endpoint_id: "fixture".into(),
        thread_id: "a".into(),
        session_id: "child-a".into(),
    };
    let source = ChildPaneBinding::open(
        runtime.clone(),
        parent.clone(),
        "nested-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    let binding = source.binding();
    let target = ComposerDestination::run(binding.scope().clone());
    assert_eq!(source.parent(), &parent);
    assert_eq!(source.tab("nested").child_parent, Some(parent));
    assert_eq!(
        binding.current_run().unwrap().child_availability,
        Some(ChildComposerAvailability::Loading)
    );
    assert!(!binding.current_run().unwrap().can_submit);
    source.refresh_controls().await.unwrap();
    assert_eq!(
        binding.current_run().unwrap().child_availability,
        Some(ChildComposerAvailability::Available)
    );
    binding.edit_text(&target, "嵌套子任务消息".into()).unwrap();
    binding
        .send_with(&target, PromptMode::Queue, &Prepare)
        .await
        .unwrap();
    let mut changes = binding.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !binding
            .current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .any(|item| item.text.contains("嵌套子任务消息"))
        {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    binding.cancel(&target).await.unwrap();
    let count = binding.current_run().unwrap().transcript.items.len();
    fixture.set_child_mode("nested-a", "one-shot");
    source.refresh_controls().await.unwrap();
    assert_eq!(
        binding.current_run().unwrap().child_availability,
        Some(ChildComposerAvailability::OneShot)
    );
    assert!(!binding.current_run().unwrap().can_submit);
    assert!(binding.prepare_send(&target, PromptMode::Queue).is_err());
    assert!(
        tekes_agent_components_ui::IndicatorFacts::read(&binding.current_run().unwrap())
            .accessible_text()
            .contains("单次执行")
    );
    let mut legacy = serde_json::to_value(binding.current_run().unwrap().as_ref()).unwrap();
    legacy.as_object_mut().unwrap().remove("childAvailability");
    let legacy: tekes_agent_presentation::RunSnapshot = serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.child_availability, None);
    fixture.set_child_mode("nested-a", "continuable");
    fixture.set_parent_available(false);
    source.refresh_controls().await.unwrap();
    assert_eq!(
        binding.current_run().unwrap().child_availability,
        Some(ChildComposerAvailability::ParentUnavailable)
    );
    fixture.set_catalog_fails(true);
    assert!(source.refresh_controls().await.is_err());
    assert!(!binding.current_run().unwrap().can_submit);
    assert!(binding.current_run().unwrap().transcript.items.len() >= count);
    assert!(
        runtime.inspect(
            |state| !state.sessions.get("a").is_some_and(|session| session
                .entries
                .values()
                .any(|row| row.event.data.to_string().contains("嵌套子任务消息")))
        )
    );
    let old = binding.clone();
    source.retire();
    assert_eq!(old.access(), PaneAccess::Missing);
    assert!(old.edit_text(&target, "stale".into()).is_err());
    assert!(source.refresh_key().is_none());
    assert!(matches!(
        source.snapshot().await.unwrap(),
        PaneSnapshot::Missing { .. }
    ));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn host_child_open_captures_parent_cache_identity_and_preserves_a_b_a_sources() {
    use tekes_agent_ui_demo::child_pane_host::open_child_for_parent;

    let (_, runtime) = setup().await;
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
    let (source_a, _) = open_child_for_parent(
        a.clone(),
        a.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        None,
        None,
    )
    .await
    .unwrap();
    let (source_b, _) = open_child_for_parent(
        b.clone(),
        b.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(source_a.authorized_by(&a));
    assert!(source_b.authorized_by(&b));
    assert!(!source_a.authorized_by(&b));
    let mut navigation = ComponentDemo::new([a.clone(), b.clone()]).unwrap();
    let request = navigation.begin_child_open(a.scope()).unwrap();
    assert!(
        navigation
            .install_child_source(
                request,
                source_a.clone(),
                "A child".into(),
                source_a.snapshot().await.unwrap(),
            )
            .unwrap()
    );
    navigation.select(1).unwrap();
    navigation.select(0).unwrap();
    let cached_a = navigation
        .cached_child_source_for_binding(
            a.scope(),
            a.scope(),
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            &a,
        )
        .expect("A selection away and back preserves A's exact child owner");
    assert!(Arc::ptr_eq(&source_a, &cached_a));

    let (returned_a, _) = open_child_for_parent(
        a.clone(),
        a.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        Some(source_a.clone()),
        None,
    )
    .await
    .unwrap();
    assert!(
        Arc::ptr_eq(&source_a, &returned_a),
        "A to B to A keeps A's owner"
    );
    assert!(!Arc::ptr_eq(&source_b, &returned_a));

    // A scope-equal replacement is still a different captured pane authority.
    let replacement = Arc::new(
        RunBinding::open_main(runtime.clone(), a.scope().clone())
            .await
            .unwrap(),
    );
    let (replaced, _) = open_child_for_parent(
        replacement.clone(),
        replacement.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        Some(source_a.clone()),
        None,
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&replaced, &source_a));
    assert!(replaced.authorized_by(&replacement));
    assert!(!source_a.authorized_by(&replacement));
    let mut replacement_view = ComponentDemo::new([replacement.clone(), b.clone()]).unwrap();
    let request = replacement_view
        .begin_child_open(replacement.scope())
        .unwrap();
    assert!(
        replacement_view
            .install_child_source(
                request,
                source_a.clone(),
                "wrong parent cache".into(),
                source_a.snapshot().await.unwrap(),
            )
            .is_err()
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn host_child_open_inherits_parent_retirement_and_rejects_bad_page_size() {
    use tekes_agent_ui_demo::child_pane_host::open_child_for_parent;

    let (_, runtime) = setup().await;
    let root = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let (parent, _) = open_child_for_parent(
        root.clone(),
        root.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        None,
        None,
    )
    .await
    .unwrap();
    let (descendant, _) = open_child_for_parent(
        parent.binding().clone(),
        parent.binding().scope(),
        "nested-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        None,
        None,
    )
    .await
    .unwrap();
    let destination = ComposerDestination::run(descendant.binding().scope().clone());
    descendant
        .binding()
        .edit_text(&destination, "must remain unsent".into())
        .unwrap();
    parent.retire();
    assert_eq!(descendant.binding().access(), PaneAccess::Missing);
    assert!(
        descendant
            .binding()
            .prepare_send(&destination, PromptMode::Queue)
            .is_err()
    );
    #[cfg(feature = "acceptance-source-witness")]
    assert!(descendant.binding().refresh_with_source_witness().is_err());

    let invalid = open_child_for_parent(
        root.clone(),
        root.scope(),
        "bad-page",
        ChildHistoryLayout::IndependentSessions,
        0,
        None,
        None,
    )
    .await;
    assert!(
        invalid.is_err(),
        "cache misses cannot bypass page-size validation"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn host_child_open_preserves_custom_same_session_page_size_and_readonly_role() {
    use tekes_agent_ui_demo::child_pane_host::open_child_for_parent;

    let (_, runtime) = setup().await;
    let parent = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let (source, snapshot) = open_child_for_parent(
        parent.clone(),
        parent.scope(),
        "child-a",
        ChildHistoryLayout::SameSession,
        7,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(source.supports_pagination());
    let positions = |frame: &PaneSnapshot| {
        let PaneSnapshot::ReadOnlyChild { transcript, .. } = frame else {
            panic!("expected original readonly frame")
        };
        transcript
            .items
            .iter()
            .filter_map(|item| match item.source_position {
                Some(tekes_agent_presentation::TranscriptSourcePosition::Journal {
                    sequence,
                    ..
                }) => Some(sequence),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(positions(&snapshot), vec![113, 114, 117, 118]);
    assert!(
        source
            .binding()
            .prepare_send(
                &ComposerDestination::run(source.binding().scope().clone()),
                PromptMode::Queue
            )
            .is_err()
    );
    assert!(source.load_earlier().await.unwrap());
    assert_eq!(
        positions(&source.snapshot().await.unwrap()),
        vec![106, 109, 110, 113, 114, 117, 118]
    );
    runtime.stop().await.unwrap();
}

#[derive(Default)]
struct DelayedHostHistory {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl tekes_agent_presentation::ChildHistorySource for DelayedHostHistory {
    async fn read(
        &self,
        _: &RunScope,
        _: &str,
    ) -> Result<tekes_agent_presentation::TranscriptSnapshot> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(Default::default())
    }
}

#[tokio::test]
async fn host_child_open_does_not_return_a_history_read_after_parent_retirement() {
    use tekes_agent_ui_demo::child_pane_host::open_child_for_parent;

    let (_, runtime) = setup().await;
    let root = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let (parent, _) = open_child_for_parent(
        root.clone(),
        root.scope(),
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        50,
        None,
        None,
    )
    .await
    .unwrap();
    let delayed = Arc::new(DelayedHostHistory::default());
    let waiting = delayed.clone();
    let parent_binding = parent.binding().clone();
    let open = tokio::spawn(async move {
        open_child_for_parent(
            parent_binding.clone(),
            parent_binding.scope(),
            "nested-bounded",
            ChildHistoryLayout::SameSession,
            50,
            None,
            Some(waiting),
        )
        .await
    });
    delayed.entered.notified().await;
    parent.retire();
    delayed.release.notify_one();
    assert!(open.await.unwrap().is_err());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer() {
    let (_, runtime) = setup().await;
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
    let source = Arc::new(
        ChildPaneBinding::for_run(&b, "bounded-b", ChildHistoryLayout::SameSession, None)
            .await
            .unwrap(),
    );
    let custom = ChildPaneBinding::for_run(
        &b,
        "custom-bounded",
        ChildHistoryLayout::SameSession,
        Some(Arc::new(CustomHistory)),
    )
    .await
    .unwrap();
    assert!(!custom.supports_pagination());
    assert!(custom.load_earlier().await.is_err());
    let PaneSnapshot::ReadOnlyChild {
        transcript: before, ..
    } = source.snapshot().await.unwrap()
    else {
        panic!("bounded role")
    };
    assert!(before.has_more_before);
    assert!(
        before
            .items
            .iter()
            .any(|item| item.text.contains("bounded-b"))
    );
    assert!(
        before
            .items
            .iter()
            .all(|item| !item.text.contains("会话 b"))
    );
    assert!(source.binding().current_run().is_none());
    assert!(
        source
            .binding()
            .prepare_send(
                &ComposerDestination::run(source.binding().scope().clone()),
                PromptMode::Queue
            )
            .is_err()
    );
    assert!(source.refresh_controls().await.is_err());
    source.load_earlier().await.unwrap();
    assert!(matches!(
        source.binding().history_page_state().phase,
        tekes_agent_ui_bindings::HistoryPagePhase::Loaded { extended: true }
    ));
    let snapshot = source.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild {
        transcript: after, ..
    } = &snapshot
    else {
        panic!("bounded role")
    };
    assert!(after.items.len() > before.items.len());
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    let request = view
        .begin_child_open(&RunScope::main("fixture", "b"))
        .unwrap();
    assert!(
        view.install_child_source(request, source.clone(), "有界子任务".into(), snapshot)
            .unwrap()
    );
    assert!(view.panel_composer_ids().is_none());
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("有界子历史"))
    );
    assert_eq!(
        view.take_background_commands().len(),
        1,
        "one coalesced initial refresh, no frame pump"
    );
    view.sync().unwrap();
    assert!(view.take_background_commands().is_empty());
    view.update_child_snapshot(
        &RunScope::main("fixture", "b"),
        &source,
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    let (_, page) = view.bounded_child_buttons().unwrap();
    let first = view
        .accessibility_action(page, None, AccessibilityAction::Press)
        .unwrap();
    assert!(matches!(
        first.as_slice(),
        [DemoCommand::ReadChild { page: true, .. }]
    ));
    assert!(
        view.accessibility_action(page, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty(),
        "pagination is single-flight"
    );
    view.child_read_failed(
        &RunScope::main("fixture", "b"),
        &source,
        "test read interrupted".into(),
    )
    .unwrap();
    assert!(
        view.take_background_commands().is_empty(),
        "same-key read errors do not retry automatically"
    );
    assert!(matches!(
        view.accessibility_action(page, None, AccessibilityAction::Press)
            .unwrap()
            .as_slice(),
        [DemoCommand::ReadChild { page: true, .. }]
    ));
    source.load_earlier().await.unwrap();
    view.update_child_snapshot(
        &RunScope::main("fixture", "b"),
        &source,
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<tekes_agent_components_ui::SidePanel>()
        .unwrap()
        .close_id();
    assert!(
        view.accessibility_action(close, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(source.binding().access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn late_child_open_caches_original_parent_without_stealing_selection_and_owner_drop_revokes_callbacks()
 {
    let (_, runtime) = setup().await;
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
    let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
    let request = view.begin_child_open(a.scope()).unwrap();
    let source = Arc::new(
        ChildPaneBinding::for_run(&a, "child-a", ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap(),
    );
    view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
        .unwrap();
    assert!(
        !view
            .install_child_source(
                request,
                source.clone(),
                "Child A".into(),
                source.snapshot().await.unwrap()
            )
            .unwrap()
    );
    assert_eq!(view.selected(), 1);
    assert!(view.panel_composer_ids().is_none());
    let mut commands = view.take_background_commands();
    let DemoCommand::ReadChild {
        source: queued,
        page,
        ..
    } = commands.remove(0)
    else {
        panic!("catalog read")
    };
    assert!(!page && Arc::ptr_eq(&queued, &source));
    assert!(Arc::ptr_eq(
        &view
            .cached_child_source(
                a.scope(),
                a.scope(),
                "child-a",
                ChildHistoryLayout::IndependentSessions
            )
            .unwrap(),
        &source
    ));
    view.accessibility_action(SELECT_A, None, AccessibilityAction::Press)
        .unwrap();
    let request = view.begin_child_open(a.scope()).unwrap();
    view.install_child_source(
        request,
        source.clone(),
        "Child A".into(),
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    assert!(
        !view.panel_child_controls_ready(),
        "catalog read is pending"
    );
    source.refresh_controls().await.unwrap();
    view.update_child_snapshot(a.scope(), &source, source.snapshot().await.unwrap())
        .unwrap();
    assert!(
        view.panel_child_controls_ready(),
        "empty draft can accept input"
    );
    let ids = view.panel_composer_ids().unwrap();
    assert!(
        view.tree
            .get(ids.send)
            .unwrap()
            .component::<tekes_ui_controls::composition::ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled,
        "input readiness must not require an empty draft to be sendable"
    );
    let request = view.begin_child_open(a.scope()).unwrap();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Preedit {
        text: "子组字".into(),
        selection: 3..3,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(
        !view
            .install_child_source(
                request,
                source.clone(),
                "Child A".into(),
                source.snapshot().await.unwrap()
            )
            .unwrap()
    );
    assert!(view.composing());
    assert!(
        view.accessibility_action(SELECT_B, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    view.event(&ContentEvent::Commit {
        text: "子代理草稿".into(),
        replacement: None,
    })
    .unwrap();
    let request = view.begin_child_open(a.scope()).unwrap();
    view.install_child_source(
        request,
        source.clone(),
        "Child A".into(),
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    assert_eq!(view.panel_composer_ids(), Some(ids));
    assert_eq!(
        source.binding().current_run().unwrap().draft.text,
        "子代理草稿"
    );
    let orphan =
        ChildPaneBinding::for_run(&a, "child-a", ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap();
    let run = orphan.binding().clone();
    drop(orphan);
    assert_eq!(run.access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_child_read_error_cannot_pollute_a_replacement_source() {
    use tekes_agent_presentation::ChildHistoryLayout;

    let (_, runtime) = setup().await;
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
    let mut view = ComponentDemo::new([a, b.clone()]).unwrap();
    view.select(1).unwrap();

    let old = Arc::new(
        ChildPaneBinding::open(
            runtime.clone(),
            b.scope().clone(),
            "bounded-b",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap(),
    );
    let request = view.begin_child_open(b.scope()).unwrap();
    assert!(
        view.install_child_source(
            request,
            old.clone(),
            "Bounded child".into(),
            old.snapshot().await.unwrap(),
        )
        .unwrap()
    );
    let _ = view.take_background_commands();

    let current = Arc::new(
        ChildPaneBinding::open(
            runtime.clone(),
            b.scope().clone(),
            "bounded-b",
            ChildHistoryLayout::SameSession,
            None,
        )
        .await
        .unwrap(),
    );
    let replacement = view.begin_child_open(b.scope()).unwrap();
    assert!(
        view.install_child_source(
            replacement,
            current.clone(),
            "Bounded child".into(),
            current.snapshot().await.unwrap(),
        )
        .unwrap()
    );

    view.child_read_failed(b.scope(), &old, "late error from retired source".into())
        .unwrap();
    assert!(!view.tree.semantics().iter().any(|node| {
        node.semantics
            .name
            .contains("late error from retired source")
    }));

    view.child_read_failed(b.scope(), &current, "current source read error".into())
        .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("current source read error"))
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn nested_child_inherits_retirement_of_its_original_parent_pane() {
    let (_, runtime) = setup().await;
    let main = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let main_destination = ComposerDestination::run(main.scope().clone());
    main.edit_text(&main_destination, "原主 Run 草稿".into())
        .unwrap();
    let parent = ChildPaneBinding::for_run(
        &main,
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    let child = ChildPaneBinding::for_run(
        parent.binding(),
        "nested-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    child.refresh_controls().await.unwrap();
    let destination = ComposerDestination::run(child.binding().scope().clone());
    child
        .binding()
        .edit_text(&destination, "嵌套子代理草稿".into())
        .unwrap();
    assert!(tekes_agent_ui_consumer_check::pane_is_present(
        child.binding()
    ));
    let ticket = child
        .binding()
        .prepare_send(&destination, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    let tail = runtime.inspect(|state| state.sessions["nested-a"].tail_sequence);
    parent.retire();
    assert_eq!(child.binding().access(), PaneAccess::Missing);
    assert!(!tekes_agent_ui_consumer_check::pane_is_present(
        child.binding()
    ));
    assert!(
        child
            .binding()
            .dispatch_prepared_with(ticket, &Prepare)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.inspect(|state| state.sessions["nested-a"].tail_sequence),
        tail
    );
    assert!(
        child
            .binding()
            .edit_text(&destination, "旧回调".into())
            .is_err()
    );
    assert!(child.refresh_controls().await.is_err());
    assert_eq!(main.access(), PaneAccess::Ready);
    assert_eq!(main.current_run().unwrap().draft.text, "原主 Run 草稿");
    let replacement = ChildPaneBinding::for_run(
        &main,
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    assert_eq!(replacement.binding().access(), PaneAccess::Ready);
    assert_eq!(
        child.binding().access(),
        PaneAccess::Missing,
        "reopening A cannot revive the old descendant"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn task_to_another_scope_inherits_its_original_child_parent_authority() {
    use tekes_agent_presentation::AgentTask;
    use tekes_agent_ui_bindings::TaskPaneBinding;
    let (_, runtime) = setup().await;
    let main = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let parent = ChildPaneBinding::for_run(
        &main,
        "child-a",
        ChildHistoryLayout::IndependentSessions,
        None,
    )
    .await
    .unwrap();
    let task = TaskPaneBinding::for_run(parent.binding());
    let token = task
        .begin(parent.binding().scope().clone(), "nested-task")
        .unwrap();
    assert!(task.commit(
        &token,
        Ok(Some(AgentTask {
            id: "nested-task".into(),
            scope: RunScope {
                session_id: "nested-a".into(),
                ..parent.binding().scope().clone()
            },
            name: "Nested".into(),
            status: "running".into(),
            kind: "task".into()
        }))
    ));
    let mounted = task.open_resolved().await.unwrap().unwrap();
    assert_eq!(mounted.access(), PaneAccess::Ready);
    parent.retire();
    assert_eq!(mounted.access(), PaneAccess::Missing);
    assert!(task.open_resolved().await.is_err());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn closed_child_tombstone_discards_late_open_before_validating_retired_source() {
    let (_, runtime) = setup().await;
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
    let source = Arc::new(
        ChildPaneBinding::for_run(&a, "child-a", ChildHistoryLayout::IndependentSessions, None)
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([a.clone(), b]).unwrap();
    let first = view.begin_child_open(a.scope()).unwrap();
    view.install_child_source(
        first,
        source.clone(),
        "Child A".into(),
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    let late = view.begin_child_open(a.scope()).unwrap();
    let snapshot = source.snapshot().await.unwrap();
    let close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<tekes_agent_components_ui::SidePanel>()
        .unwrap()
        .close_id();
    view.accessibility_action(close, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(source.binding().access(), PaneAccess::Missing);
    // An already retired completion is an ignored delivery, not an error on
    // the parent's current pane, and cannot remount the closed child.
    assert!(
        !view
            .install_child_source(late, source.clone(), "Child A".into(), snapshot)
            .unwrap()
    );
    assert!(!view.child_pane_active(a.scope(), "child-a"));
    assert!(
        view.cached_child_source(
            a.scope(),
            a.scope(),
            "child-a",
            ChildHistoryLayout::IndependentSessions
        )
        .is_none()
    );
    runtime.stop().await.unwrap();
}
