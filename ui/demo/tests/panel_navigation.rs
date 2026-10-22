use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{SidePanel, SidePanelInput, SidePanelTabKind};
use tekes_agent_presentation::{AgentTask, ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{RunBinding, SideChatBinding, TaskPaneBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, OPEN_SUMMARY},
    fixture::Fixture,
};
use tekes_ui_controls::composition::AccessibilityAction;

async fn setup() -> (Arc<SessionRuntime>, Arc<RunBinding>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children().with_side_chats()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let parent = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let other = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([parent.clone(), other]).unwrap();
    view.set_side_chat_binding(Arc::new(SideChatBinding::new(runtime.clone())));
    view.sync().unwrap();
    (runtime, parent, view)
}
fn input(view: &ComponentDemo) -> SidePanelInput {
    view.tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .displayed_input()
        .unwrap()
        .clone()
}
fn close(view: &mut ComponentDemo) {
    let id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert!(
        commands.is_empty(),
        "ordinary tab close has no endpoint command"
    );
}
fn launcher_id(
    view: &ComponentDemo,
    destination: tekes_agent_components_ui::SidePanelDestination,
) -> tekes_ui_controls::composition::ComponentId {
    view.tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .launcher()
        .unwrap()
        .button_id(destination)
        .unwrap()
}

#[tokio::test]
async fn host_toolbar_uses_existing_panel_navigation_and_keeps_original_body_and_preedit() {
    use tekes_agent_components_ui::{SidePanelHeaderPlacement, SidePanelNavigation, SummaryView};
    use tekes_ui_controls::composition::ContentEvent;
    let (runtime, parent, mut view) = setup().await;
    let summary = view.summary_id();
    let owner = view
        .tree
        .get(summary)
        .unwrap()
        .component::<SummaryView>()
        .unwrap() as *const _ as usize;
    let diagnostic_close = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    view.use_panel_navigation_toolbar().unwrap();
    assert!(view.tree.get(diagnostic_close).is_none());
    assert_eq!(
        view.tree
            .get(summary)
            .unwrap()
            .component::<SummaryView>()
            .unwrap() as *const _ as usize,
        owner
    );
    assert_eq!(
        view.tree
            .get(view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .header_placement(),
        SidePanelHeaderPlacement::Host
    );
    let nav = view.panel_navigation_id().unwrap();
    let button = view
        .tree
        .get(nav)
        .unwrap()
        .component::<SidePanelNavigation>()
        .unwrap()
        .close_id()
        .unwrap();
    let commands = view
        .accessibility_action(button, None, AccessibilityAction::Press)
        .unwrap();
    assert!(commands.is_empty());
    assert!(input(&view).tabs.is_empty() && input(&view).selected.is_none());
    let reopen = launcher_id(
        &view,
        tekes_agent_components_ui::SidePanelDestination::Summary,
    );
    assert!(
        view.accessibility_action(reopen, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    let editor = view.ids().editor;
    view.tree.focus(Some(editor), true);
    view.tree.event(&ContentEvent::Preedit {
        text: "工具栏组字".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    });
    let baseline = view
        .tree
        .text_surface(editor)
        .unwrap()
        .input
        .external_baseline();
    let button = view
        .tree
        .get(nav)
        .unwrap()
        .component::<SidePanelNavigation>()
        .unwrap()
        .close_id()
        .unwrap();
    assert!(
        view.accessibility_action(button, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    view.sync().unwrap();
    assert_eq!(input(&view).selected, Some(SidePanelTabKind::Summary));
    assert_eq!(view.tree.focused(), Some(editor));
    assert_eq!(
        view.tree
            .text_surface(editor)
            .unwrap()
            .input
            .external_baseline(),
        baseline
    );
    assert!(parent.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn empty_panel_launcher_opens_summary_through_the_original_callback() {
    use tekes_agent_components_ui::SidePanelDestination;
    let (runtime, parent, mut view) = setup().await;
    close(&mut view);
    let id = launcher_id(&view, SidePanelDestination::Summary);
    let commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert!(commands.is_empty());
    assert_eq!(input(&view).selected, Some(SidePanelTabKind::Summary));
    assert!(view.tree.get(id).is_none());
    assert_eq!(view.binding().scope(), parent.scope());
    assert!(parent.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn empty_panel_launcher_creates_one_actual_scratch_pointer_through_the_existing_binding() {
    use tekes_agent_components_ui::SidePanelDestination;
    let (runtime, parent, mut view) = setup().await;
    let owner = Arc::new(SideChatBinding::new(runtime.clone()));
    view.set_side_chat_binding(owner.clone());
    view.sync().unwrap();
    close(&mut view);
    let id = launcher_id(&view, SidePanelDestination::SideChat);
    let mut commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(commands.len(), 1);
    let DemoCommand::CreateSideChat(request) = commands.remove(0) else {
        panic!("captured scratch request")
    };
    assert_eq!(request.parent(), parent.scope());
    let chat = owner
        .create(parent.scope().clone(), None, "launcher-create")
        .await
        .unwrap();
    let bound = owner.open(&chat).await.unwrap();
    assert!(view.install_side_chat_run(request, bound).unwrap());
    assert_eq!(
        input(&view).selected,
        Some(SidePanelTabKind::SideChat {
            session_id: chat.scope.session_id.clone()
        })
    );
    assert!(owner.snapshot().chats.contains_key(&chat.scope.session_id));
    assert!(parent.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
async fn task(
    view: &mut ComponentDemo,
    parent: &Arc<RunBinding>,
    id: &str,
    session: &str,
) -> Arc<RunBinding> {
    let source = Arc::new(TaskPaneBinding::for_run(parent));
    let token = source.begin(parent.scope().clone(), id).unwrap();
    assert!(source.commit(
        &token,
        Ok(Some(AgentTask {
            id: id.into(),
            scope: RunScope {
                session_id: session.into(),
                ..parent.scope().clone()
            },
            name: id.into(),
            status: "running".into(),
            kind: "task".into()
        }))
    ));
    view.open_task_source(source.clone()).unwrap();
    let binding = source.open_resolved().await.unwrap().unwrap();
    assert!(
        view.install_task_run(parent.scope().clone(), id.into(), binding.clone())
            .unwrap()
    );
    let _ = view.take_background_commands();
    binding
}

#[tokio::test]
async fn closing_last_summary_does_not_recreate_it_or_touch_the_main_draft() {
    let (runtime, parent, mut view) = setup().await;
    parent
        .edit_text(
            &ComposerDestination::run(parent.scope().clone()),
            "保留主草稿".into(),
        )
        .unwrap();
    view.sync().unwrap();
    close(&mut view);
    assert!(input(&view).tabs.is_empty());
    assert!(input(&view).selected.is_none());
    view.sync().unwrap();
    assert!(input(&view).tabs.is_empty());
    view.select(1).unwrap();
    assert_eq!(input(&view).selected, Some(SidePanelTabKind::Summary));
    view.select(0).unwrap();
    assert!(
        input(&view).tabs.is_empty(),
        "the closed document survives A→B→A"
    );
    view.accessibility_action(OPEN_SUMMARY, None, AccessibilityAction::Press)
        .unwrap();
    assert_eq!(input(&view).selected, Some(SidePanelTabKind::Summary));
    assert_eq!(input(&view).tabs.len(), 1);
    assert_eq!(parent.current_run().unwrap().draft.text, "保留主草稿");
    assert!(parent.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn closing_a_task_selects_the_last_remaining_open_tab() {
    let (runtime, parent, mut view) = setup().await;
    task(&mut view, &parent, "z-first", "child-a").await;
    task(&mut view, &parent, "a-second", "child-b").await;
    let first = SidePanelTabKind::Task {
        task_id: "z-first".into(),
    };
    let second = SidePanelTabKind::Task {
        task_id: "a-second".into(),
    };
    assert_eq!(
        input(&view)
            .tabs
            .iter()
            .map(|tab| tab.kind.clone())
            .collect::<Vec<_>>(),
        vec![SidePanelTabKind::Summary, first.clone(), second]
    );
    close(&mut view);
    assert_eq!(input(&view).selected, Some(first));
    view.open_summary().unwrap();
    close(&mut view);
    assert_eq!(input(&view).tabs.len(), 1);
    assert!(matches!(
        input(&view).selected,
        Some(SidePanelTabKind::Task { .. })
    ));
    close(&mut view);
    assert!(input(&view).tabs.is_empty());
    view.open_summary().unwrap();
    assert_eq!(input(&view).tabs.len(), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_scratch_open_keeps_the_pointer_without_selecting_a_closed_document() {
    let (runtime, parent, mut view) = setup().await;
    let owner = Arc::new(SideChatBinding::new(runtime.clone()));
    view.set_side_chat_binding(owner.clone());
    view.sync().unwrap();
    let id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .create_side_chat_id();
    let mut commands = view
        .accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap();
    let DemoCommand::CreateSideChat(request) = commands.remove(0) else {
        panic!("create intent")
    };
    close(&mut view);
    let chat = owner
        .create(parent.scope().clone(), None, "late-navigation")
        .await
        .unwrap();
    let bound = owner.open(&chat).await.unwrap();
    assert!(!view.install_side_chat_run(request, bound).unwrap());
    for _ in 0..3 {
        view.sync().unwrap();
        assert_eq!(input(&view).tabs.len(), 1);
        assert!(
            input(&view).selected.is_none(),
            "a valid late pointer is not a new navigation request"
        );
        assert!(view.panel_composer_ids().is_none());
    }
    assert!(owner.snapshot().chats.contains_key(&chat.scope.session_id));
    view.open_summary().unwrap();
    assert_eq!(
        input(&view)
            .tabs
            .iter()
            .map(|tab| tab.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            SidePanelTabKind::SideChat {
                session_id: chat.scope.session_id
            },
            SidePanelTabKind::Summary
        ]
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess};
    let (runtime, parent, mut view) = setup().await;
    let task_run = task(&mut view, &parent, "task", "child-a").await;
    let child = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-b",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    let request = view.begin_child_open(parent.scope()).unwrap();
    assert!(
        view.install_child_source(
            request,
            child.clone(),
            "Child".into(),
            child.snapshot().await.unwrap()
        )
        .unwrap()
    );
    let _ = view.take_background_commands();
    close(&mut view);
    assert_eq!(
        input(&view).selected,
        Some(SidePanelTabKind::Task {
            task_id: "task".into()
        })
    );
    assert_eq!(child.binding().access(), PaneAccess::Missing);
    assert_ne!(task_run.access(), PaneAccess::Missing);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_child_open_cannot_replace_a_newer_owner_of_the_same_tab() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::{ChildPaneBinding, PaneAccess};
    let (runtime, parent, mut view) = setup().await;
    let old_request = view.begin_child_open(parent.scope()).unwrap();
    let old = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    let new_request = view.begin_child_open(parent.scope()).unwrap();
    let new = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    assert!(
        view.install_child_source(
            new_request.clone(),
            new.clone(),
            "new owner".into(),
            new.snapshot().await.unwrap()
        )
        .unwrap()
    );
    let ids = view.panel_composer_ids().unwrap();
    assert!(
        !view
            .install_child_source(
                old_request,
                old.clone(),
                "late old owner".into(),
                old.snapshot().await.unwrap()
            )
            .unwrap()
    );
    let cached = view
        .cached_child_source(
            parent.scope(),
            parent.scope(),
            "child-a",
            ChildHistoryLayout::IndependentSessions,
        )
        .unwrap();
    assert!(
        Arc::ptr_eq(&cached, &new),
        "late result must not retire/replace the current original owner"
    );
    assert_ne!(new.binding().access(), PaneAccess::Missing);
    assert_eq!(view.panel_composer_ids(), Some(ids));
    assert!(
        !view
            .install_child_source(
                new_request,
                new.clone(),
                "duplicate title".into(),
                new.snapshot().await.unwrap(),
            )
            .unwrap()
    );
    assert_eq!(view.panel_composer_ids(), Some(ids));
    assert_eq!(input(&view).tabs.last().unwrap().title, "new owner");
    assert!(parent.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn closing_a_child_fences_a_never_installed_older_open_result() {
    use tekes_agent_presentation::ChildHistoryLayout;
    use tekes_agent_ui_bindings::ChildPaneBinding;
    let (runtime, parent, mut view) = setup().await;
    let new_request = view.begin_child_open(parent.scope()).unwrap();
    let new = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    assert!(
        view.install_child_source(
            new_request,
            new.clone(),
            "new".into(),
            new.snapshot().await.unwrap()
        )
        .unwrap()
    );
    let _ = view.take_background_commands();
    // This pending request is newer than the accepted body: only the subsequent
    // close tombstone can reject it. It has never owned the mounted pane.
    let old_request = view.begin_child_open(parent.scope()).unwrap();
    let old = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    close(&mut view);
    assert_eq!(input(&view).selected, Some(SidePanelTabKind::Summary));
    assert!(
        !view
            .install_child_source(
                old_request,
                old.clone(),
                "late".into(),
                old.snapshot().await.unwrap()
            )
            .unwrap()
    );
    assert!(
        view.cached_child_source(
            parent.scope(),
            parent.scope(),
            "child-a",
            ChildHistoryLayout::IndependentSessions
        )
        .is_none(),
        "closed child must not reappear through a previously uninstalled result"
    );
    assert_eq!(input(&view).tabs.len(), 1);
    // A new explicit open gets a new owner and can reopen the same business ID.
    let request = view.begin_child_open(parent.scope()).unwrap();
    let fresh = Arc::new(
        ChildPaneBinding::for_run(
            &parent,
            "child-a",
            ChildHistoryLayout::IndependentSessions,
            None,
        )
        .await
        .unwrap(),
    );
    assert!(
        view.install_child_source(
            request,
            fresh.clone(),
            "reopened".into(),
            fresh.snapshot().await.unwrap()
        )
        .unwrap()
    );
    assert_eq!(input(&view).tabs.len(), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn inactive_task_navigation_cannot_retire_old_owner_or_blur_current_parent() {
    use tekes_agent_ui_bindings::PaneAccess;
    let (runtime, a, mut view) = setup().await;
    let original = task(&mut view, &a, "task", "child-a").await;
    let original_ids = view.panel_composer_ids().unwrap();
    original
        .edit_text(
            &ComposerDestination::run(original.scope().clone()),
            "原任务草稿".into(),
        )
        .unwrap();
    view.select(1).unwrap();
    let b = view.binding().scope().clone();
    let request = view.begin_child_open(&b).unwrap();
    let editor = view.ids().editor;
    view.tree.focus(Some(editor), true);
    assert_eq!(view.tree.focused(), Some(editor));
    let baseline = view.editor().input.external_baseline();
    let rejected = Arc::new(TaskPaneBinding::for_run(&a));
    let token = rejected.begin(a.scope().clone(), "task").unwrap();
    assert!(rejected.commit(
        &token,
        Ok(Some(AgentTask {
            id: "task".into(),
            scope: RunScope {
                session_id: "child-b".into(),
                ..a.scope().clone()
            },
            name: "late inactive navigation".into(),
            status: "running".into(),
            kind: "task".into(),
        }))
    ));
    assert!(
        view.open_task_source(rejected).is_err(),
        "a host must explicitly navigate to the task parent first"
    );
    assert_eq!(view.binding().scope(), &b);
    assert_eq!(view.tree.focused(), Some(editor));
    assert_eq!(view.editor().input.external_baseline(), baseline);
    assert!(
        view.child_request_current(&request),
        "inactive navigation cannot invalidate B's pending request"
    );
    assert_ne!(original.access(), PaneAccess::Missing);
    assert_eq!(original.current_run().unwrap().draft.text, "原任务草稿");
    view.select(0).unwrap();
    assert_eq!(view.panel_composer_ids(), Some(original_ids));
    assert!(a.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn original_task_completion_can_cache_after_parent_switch_without_navigating() {
    let (runtime, a, mut view) = setup().await;
    let source = Arc::new(TaskPaneBinding::for_run(&a));
    let token = source.begin(a.scope().clone(), "pending-task").unwrap();
    assert!(source.commit(
        &token,
        Ok(Some(AgentTask {
            id: "pending-task".into(),
            scope: RunScope {
                session_id: "child-a".into(),
                ..a.scope().clone()
            },
            name: "original pending task".into(),
            status: "running".into(),
            kind: "task".into(),
        }))
    ));
    view.open_task_source(source.clone()).unwrap();
    let _ = view.take_background_commands();
    view.select(1).unwrap();
    let b = view.binding().scope().clone();
    let request = view.begin_child_open(&b).unwrap();
    let editor = view.ids().editor;
    view.tree.focus(Some(editor), true);
    assert_eq!(view.tree.focused(), Some(editor));
    let before = (
        view.editor().input.text.clone(),
        view.editor().input.anchor,
        view.editor().input.caret,
    );
    let owner = view.editor() as *const _ as usize;
    let run = source.open_resolved().await.unwrap().unwrap();
    run.edit_text(
        &ComposerDestination::run(run.scope().clone()),
        "异步任务草稿".into(),
    )
    .unwrap();
    // Acquiring the task's history can publish a new runtime revision to B.
    // Let the original SDK acknowledge it; text, selection and owner must stay.
    view.sync().unwrap();
    assert_eq!(
        (
            view.editor().input.text.clone(),
            view.editor().input.anchor,
            view.editor().input.caret
        ),
        before
    );
    assert_eq!(view.editor() as *const _ as usize, owner);
    assert_eq!(view.tree.focused(), Some(editor));
    let baseline = view.editor().input.external_baseline();
    assert!(
        view.install_task_run(a.scope().clone(), "pending-task".into(), run.clone())
            .unwrap()
    );
    assert_eq!(view.binding().scope(), &b);
    assert!(view.panel_composer_ids().is_none());
    assert_eq!(view.tree.focused(), Some(editor));
    assert_eq!(view.editor().input.external_baseline(), baseline);
    assert!(view.child_request_current(&request));
    view.select(0).unwrap();
    assert!(view.panel_composer_ids().is_some());
    assert_eq!(
        input(&view).selected,
        Some(SidePanelTabKind::Task {
            task_id: "pending-task".into()
        })
    );
    assert_eq!(run.current_run().unwrap().draft.text, "异步任务草稿");
    assert!(run.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[cfg(target_os = "macos")]
mod native_text {
    use super::*;
    use std::rc::Rc;
    use tekes_ui_controls::{
        ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
        composition::ContentEvent,
    };
    use tekes_ui_scene::{Point, Rect, Scene, Size};
    use tekes_ui_text::{
        Paragraph,
        coretext::{CoreTextFonts, CoreTextShaper},
        model::ShapeOptions,
        style::TextStyle,
        traits::Shaper,
    };
    use tekes_ui_tokens::{Rgba, Theme, proposal};
    struct Text(CoreTextShaper);
    impl TextSurfaceTextRenderer for Text {
        fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
    }
    impl TextSurfaceHost for Text {
        fn shape_surface(
            &mut self,
            text: &str,
            role: proposal::TextRole,
            width: Option<f32>,
        ) -> Rc<Paragraph> {
            let style = role.style();
            Rc::new(self.0.shape(
                text,
                &TextStyle::system(style.size).with_weight(style.weight),
                &ShapeOptions {
                    width: width.map(f64::from),
                    line_spacing: 0.,
                },
            ))
        }
        fn shape_label(
            &mut self,
            text: &str,
            role: proposal::TextRole,
            width: f32,
            options: tekes_ui_text::LabelOptions,
        ) -> Option<Rc<tekes_ui_text::LabelLayout>> {
            Some(tekes_agent_ui_demo::label_host::shape_label(
                &mut self.0,
                text,
                role,
                width,
                1.,
                options,
            ))
        }
    }
    impl ButtonTextRenderer for Text {
        fn measure(&mut self, text: &str, role: proposal::TextRole, width: f32) -> Size {
            let p = self.shape_surface(text, role, Some(width));
            Size::new(p.width as f32, p.height as f32)
        }
        fn draw(
            &mut self,
            _: &mut Scene,
            _: &str,
            _: proposal::TextRole,
            _: Rect,
            _: ButtonTextAlign,
            _: Rgba,
        ) {
        }
        fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
            Some(self)
        }
    }
    #[tokio::test]
    async fn scratch_close_ack_selects_the_last_task_without_blurring_main_preedit() {
        let (runtime, parent, mut view) = setup().await;
        let owner = Arc::new(SideChatBinding::new(runtime.clone()));
        view.set_side_chat_binding(owner.clone());
        task(&mut view, &parent, "last-task", "child-a").await;
        let create = view
            .tree
            .get(view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .create_side_chat_id();
        let mut commands = view
            .accessibility_action(create, None, AccessibilityAction::Press)
            .unwrap();
        let DemoCommand::CreateSideChat(request) = commands.remove(0) else {
            panic!("create")
        };
        let chat = owner
            .create(parent.scope().clone(), None, "close-ack")
            .await
            .unwrap();
        assert!(
            view.install_side_chat_run(request, owner.open(&chat).await.unwrap())
                .unwrap()
        );
        let close = view
            .tree
            .get(view.panel_id())
            .unwrap()
            .component::<SidePanel>()
            .unwrap()
            .close_id();
        let mut commands = view
            .accessibility_action(close, None, AccessibilityAction::Press)
            .unwrap();
        let DemoCommand::CloseSideChat(captured) = commands.remove(0) else {
            panic!("close")
        };
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
        let frame = Rect::new(0., 0., 1600., 1000.);
        view.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let editor = view.ids().editor;
        assert!(view.tree.focus(Some(editor), true));
        view.event_with_text(
            &ContentEvent::Preedit {
                text: "关闭回包期间中文组字".into(),
                selection: 2..4,
                replacement: None,
                clauses: vec![],
            },
            &mut text,
        )
        .unwrap();
        let before = view
            .tree
            .text_surface(editor)
            .unwrap()
            .input
            .external_baseline();
        let original = view.tree.text_surface(editor).unwrap() as *const _ as usize;
        owner.close(&captured, "confirmed-close-ack").await.unwrap();
        view.sync().unwrap();
        assert_eq!(
            input(&view).selected,
            Some(SidePanelTabKind::Task {
                task_id: "last-task".into()
            })
        );
        assert_eq!(view.tree.focused(), Some(editor));
        assert_eq!(
            view.tree.text_surface(editor).unwrap() as *const _ as usize,
            original
        );
        assert_eq!(
            view.tree
                .text_surface(editor)
                .unwrap()
                .input
                .external_baseline(),
            before
        );
        assert!(parent.current_run().unwrap().submissions.is_empty());
        runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn original_task_completion_preserves_another_parents_actual_preedit() {
        let (runtime, a, mut view) = setup().await;
        view.enable_transcript_content().unwrap();
        let source = Arc::new(TaskPaneBinding::for_run(&a));
        let token = source.begin(a.scope().clone(), "pending-task").unwrap();
        assert!(source.commit(
            &token,
            Ok(Some(AgentTask {
                id: "pending-task".into(),
                scope: RunScope {
                    session_id: "child-a".into(),
                    ..a.scope().clone()
                },
                name: "original pending task".into(),
                status: "running".into(),
                kind: "task".into(),
            }))
        ));
        view.open_task_source(source.clone()).unwrap();
        let _ = view.take_background_commands();
        view.select(1).unwrap();
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
        let frame = Rect::new(0., 0., 1600., 1000.);
        view.render(
            &mut Scene::new(),
            &mut text,
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let editor = view.ids().editor;
        view.tree.focus(Some(editor), true);
        assert_eq!(view.tree.focused(), Some(editor));
        let canonical = view.binding().current_run().unwrap();
        view.event_with_text(
            &ContentEvent::Preedit {
                text: "任务回传期间中文组字".into(),
                selection: 2..4,
                clauses: vec![],
                replacement: None,
            },
            &mut text,
        )
        .unwrap();
        let owner = view.tree.text_surface(editor).unwrap() as *const _ as usize;
        let baseline = view.editor().input.external_baseline();
        let run = source.open_resolved().await.unwrap().unwrap();
        assert!(
            view.install_task_run(a.scope().clone(), "pending-task".into(), run)
                .unwrap()
        );
        assert_eq!(view.binding().scope().session_id, "b");
        assert_eq!(view.tree.focused(), Some(editor));
        assert_eq!(
            view.tree.text_surface(editor).unwrap() as *const _ as usize,
            owner
        );
        assert_eq!(view.editor().input.external_baseline(), baseline);
        assert!(view.editor().input.marked.is_some());
        assert_eq!(view.binding().current_run().unwrap().draft, canonical.draft);
        assert_eq!(
            view.binding().current_run().unwrap().submissions.len(),
            canonical.submissions.len()
        );
        runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn explicit_summary_destination_is_visible_and_disabled_during_composition() {
        let (runtime, _, mut view) = setup().await;
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
        for width in [1000., 1200., 1600.] {
            let frame = Rect::new(0., 0., width, 760.);
            view.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let button = view.tree.get(OPEN_SUMMARY).unwrap();
            assert!(
                button.visible().unwrap().contains(button.frame().center()),
                "host destination must remain visibly clickable at native minimum width"
            );
        }
        let editor = view.ids().editor;
        view.tree.focus(Some(editor), true);
        view.event_with_text(
            &ContentEvent::Preedit {
                text: "导航期间中文组字".into(),
                selection: 2..4,
                replacement: None,
                clauses: vec![],
            },
            &mut text,
        )
        .unwrap();
        let before = view
            .tree
            .text_surface(editor)
            .unwrap()
            .input
            .external_baseline();
        view.accessibility_action(OPEN_SUMMARY, None, AccessibilityAction::Press)
            .unwrap();
        assert_eq!(
            view.tree
                .text_surface(editor)
                .unwrap()
                .input
                .external_baseline(),
            before
        );
        assert_eq!(view.tree.focused(), Some(editor));
        runtime.stop().await.unwrap();
    }
    #[tokio::test]
    async fn host_family_menu_creates_selects_and_discards_original_scratch_pointers() {
        use tekes_agent_components_ui::{SidePanelDestination, SidePanelNavigation};
        use tekes_ui_controls::composition::{ComponentId, ContentRole};
        fn paint(view: &mut ComponentDemo, text: &mut Text) {
            let frame = Rect::new(0., 0., 1600., 1000.);
            view.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
                .unwrap();
        }
        fn navigation(view: &ComponentDemo) -> &SidePanelNavigation {
            view.tree
                .get(view.panel_navigation_id().unwrap())
                .unwrap()
                .component::<SidePanelNavigation>()
                .unwrap()
        }
        fn press_close(view: &mut ComponentDemo) -> Vec<DemoCommand> {
            let id = navigation(view).close_id().unwrap();
            view.accessibility_action(id, None, AccessibilityAction::Press)
                .unwrap()
        }
        fn menu(
            view: &mut ComponentDemo,
            text: &mut Text,
            index: usize,
            checked: Option<usize>,
        ) -> Vec<DemoCommand> {
            let id = navigation(view).side_chat_menu_id().unwrap();
            assert!(
                view.accessibility_action(id, None, AccessibilityAction::Press)
                    .unwrap()
                    .is_empty()
            );
            paint(view, text);
            let nodes = view.tree.semantics();
            let items: Vec<_> = nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .parts
                .iter()
                .filter(|part| part.semantics.role == ContentRole::MenuItem)
                .collect();
            assert_eq!(items[0].semantics.name, "新建 SideChat");
            for item in &items[1..] {
                assert_eq!(item.semantics.name, "SideChat");
            }
            if let Some(checked) = checked {
                assert_eq!(items.len(), 3);
                for (i, item) in items.iter().enumerate().skip(1) {
                    assert_eq!(item.semantics.selected, i == checked);
                }
            }
            view.accessibility_action(id, Some(items[index].index), AccessibilityAction::Press)
                .unwrap()
        }
        let (runtime, parent, mut view) = setup().await;
        let owner = Arc::new(SideChatBinding::new(runtime.clone()));
        view.set_side_chat_binding(owner.clone());
        parent
            .edit_text(
                &ComposerDestination::run(parent.scope().clone()),
                "主会话草稿".into(),
            )
            .unwrap();
        view.use_panel_navigation_toolbar().unwrap();
        view.sync().unwrap();
        assert!(press_close(&mut view).is_empty());
        let create = launcher_id(&view, SidePanelDestination::SideChat);
        let mut commands = view
            .accessibility_action(create, None, AccessibilityAction::Press)
            .unwrap();
        assert_eq!(commands.len(), 1);
        let DemoCommand::CreateSideChat(first_request) = commands.remove(0) else {
            panic!("original launcher create")
        };
        let first = owner
            .create(parent.scope().clone(), None, "family-first")
            .await
            .unwrap();
        let first_run = owner.open(&first).await.unwrap();
        first_run
            .binding()
            .edit_text(
                &ComposerDestination::run(first.scope.clone()),
                "第一个侧聊草稿".into(),
            )
            .unwrap();
        assert!(
            view.install_side_chat_run(first_request, first_run)
                .unwrap()
        );
        let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
        paint(&mut view, &mut text);
        let first_editor = view.panel_composer_ids().unwrap().editor;
        let first_owner = view.tree.text_surface(first_editor).unwrap() as *const _ as usize;
        let mut commands = menu(&mut view, &mut text, 0, None);
        assert_eq!(commands.len(), 1);
        let DemoCommand::CreateSideChat(second_request) = commands.remove(0) else {
            panic!("original menu create")
        };
        let second = owner
            .create(parent.scope().clone(), None, "family-second")
            .await
            .unwrap();
        assert_ne!(first.scope, second.scope);
        assert!(
            view.install_side_chat_run(second_request, owner.open(&second).await.unwrap())
                .unwrap()
        );
        paint(&mut view, &mut text);
        assert_eq!(owner.snapshot().chats.len(), 2);
        assert!(menu(&mut view, &mut text, 1, Some(2)).is_empty());
        paint(&mut view, &mut text);
        assert_eq!(
            input(&view).selected,
            Some(SidePanelTabKind::SideChat {
                session_id: first.scope.session_id.clone()
            })
        );
        assert_eq!(view.panel_composer_ids().unwrap().editor, first_editor);
        assert_eq!(
            view.tree.text_surface(first_editor).unwrap() as *const _ as usize,
            first_owner
        );
        assert_eq!(
            view.tree.text_surface(first_editor).unwrap().input.text,
            "第一个侧聊草稿"
        );
        assert!(menu(&mut view, &mut text, 2, Some(1)).is_empty());
        assert_eq!(
            input(&view).selected,
            Some(SidePanelTabKind::SideChat {
                session_id: second.scope.session_id.clone()
            })
        );
        let mut commands = press_close(&mut view);
        assert_eq!(commands.len(), 1);
        let DemoCommand::CloseSideChat(captured) = commands.remove(0) else {
            panic!("original close")
        };
        assert_eq!(captured.scope, second.scope);
        assert!(
            owner
                .snapshot()
                .chats
                .contains_key(&second.scope.session_id)
        );
        assert_eq!(
            input(&view).selected,
            Some(SidePanelTabKind::SideChat {
                session_id: second.scope.session_id.clone()
            }),
            "do not remove the tab before discard succeeds"
        );
        owner.close(&captured, "family-close-second").await.unwrap();
        view.sync().unwrap();
        paint(&mut view, &mut text);
        assert!(
            !owner
                .snapshot()
                .chats
                .contains_key(&second.scope.session_id)
        );
        assert_eq!(
            input(&view).selected,
            Some(SidePanelTabKind::SideChat {
                session_id: first.scope.session_id.clone()
            })
        );
        assert_eq!(view.panel_composer_ids().unwrap().editor, first_editor);
        assert_eq!(
            view.tree.text_surface(first_editor).unwrap() as *const _ as usize,
            first_owner
        );
        let mut commands = press_close(&mut view);
        let DemoCommand::CloseSideChat(captured) = commands.remove(0) else {
            panic!("first close")
        };
        owner.close(&captured, "family-close-first").await.unwrap();
        view.sync().unwrap();
        assert!(owner.snapshot().chats.is_empty());
        assert!(input(&view).tabs.is_empty());
        assert!(input(&view).selected.is_none());
        assert_eq!(parent.current_run().unwrap().draft.text, "主会话草稿");
        assert!(parent.current_run().unwrap().submissions.is_empty());
        assert!(view.tree.get(ComponentId(first_editor.0)).is_none());
        runtime.stop().await.unwrap();
    }
}
