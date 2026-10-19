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
