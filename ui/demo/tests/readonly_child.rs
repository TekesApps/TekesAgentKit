use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tekes_agent_components_ui::*;
use tekes_agent_contract::{Result, ThreadActionDefinition};
use tekes_agent_presentation::{ChildHistoryLayout, ChildHistorySource, TranscriptSnapshot};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{ChildPaneBinding, PaneSnapshot, RunBinding};
use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};
use tekes_ui_controls::composition::*;
async fn runtime() -> Arc<SessionRuntime> {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_children()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime
}
async fn source(
    runtime: Arc<SessionRuntime>,
    custom: Option<Arc<dyn ChildHistorySource>>,
) -> Arc<ChildPaneBinding> {
    Arc::new(
        ChildPaneBinding::open(
            runtime,
            RunScope::main("fixture", "b"),
            "bounded-b",
            ChildHistoryLayout::SameSession,
            custom,
        )
        .await
        .unwrap(),
    )
}
async fn host(runtime: Arc<SessionRuntime>) -> ComponentDemo {
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime, RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.select(1).unwrap();
    view
}
#[derive(Default)]
struct Deferred {
    calls: AtomicUsize,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl ChildHistorySource for Deferred {
    async fn read(&self, _: &RunScope, _: &str) -> Result<TranscriptSnapshot> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(TranscriptSnapshot {
            diagnostics: vec![format!("read-{call}")],
            ..Default::default()
        })
    }
}
#[tokio::test]
async fn invocation_order_fences_an_older_custom_read_that_finishes_after_the_new_frame() {
    let runtime = runtime().await;
    let deferred = Arc::new(Deferred::default());
    let source = source(runtime.clone(), Some(deferred.clone())).await;
    let original = source.clone();
    let old = tokio::spawn(async move { original.snapshot().await });
    deferred.entered.notified().await;
    let newest = source.snapshot().await.unwrap();
    deferred.release.notify_one();
    let oldest = old.await.unwrap().unwrap();
    let (
        PaneSnapshot::ReadOnlyChild {
            publication: first, ..
        },
        PaneSnapshot::ReadOnlyChild {
            publication: second,
            ..
        },
    ) = (&oldest, &newest)
    else {
        panic!("read-only receipts")
    };
    assert!(
        first < second,
        "completion order cannot become publication order"
    );
    let mut view = host(runtime.clone()).await;
    let request = view
        .begin_child_open(&RunScope::main("fixture", "b"))
        .unwrap();
    view.install_child_source(request, source.clone(), "bounded".into(), newest)
        .unwrap();
    let root = view.bounded_child_content_id().unwrap();
    assert!(
        !view
            .update_child_snapshot(&RunScope::main("fixture", "b"), &source, oldest)
            .unwrap()
    );
    assert_eq!(view.bounded_child_content_id(), Some(root));
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("read-1"))
    );
    assert!(
        !view
            .tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("read-0"))
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn a_custom_read_cannot_pair_old_history_with_a_new_connection_epoch() {
    let runtime = runtime().await;
    let deferred = Arc::new(Deferred::default());
    let source = source(runtime.clone(), Some(deferred.clone())).await;
    let original = source.clone();
    let read = tokio::spawn(async move { original.snapshot().await });
    deferred.entered.notified().await;
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    deferred.release.notify_one();
    let error = read.await.unwrap().err().expect("old epoch rejected");
    assert!(error.to_string().contains("connection changed"));
    assert!(matches!(
        source.snapshot().await.unwrap(),
        PaneSnapshot::ReadOnlyChild { .. }
    ));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn retiring_the_original_child_during_a_read_returns_missing_instead_of_renderable_content() {
    let runtime = runtime().await;
    let deferred = Arc::new(Deferred::default());
    let source = source(runtime.clone(), Some(deferred.clone())).await;
    let original = source.clone();
    let read = tokio::spawn(async move { original.snapshot().await });
    deferred.entered.notified().await;
    source.retire();
    deferred.release.notify_one();
    assert!(matches!(
        read.await.unwrap().unwrap(),
        PaneSnapshot::Missing { .. }
    ));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn builtin_pagination_retains_real_message_widgets_selection_and_history_after_failure() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let first = source.snapshot().await.unwrap();
    let mut view = host(runtime.clone()).await;
    let request = view
        .begin_child_open(&RunScope::main("fixture", "b"))
        .unwrap();
    view.install_child_source(request, source.clone(), "bounded".into(), first)
        .unwrap();
    assert!(view.panel_composer_ids().is_none());
    let root = view.bounded_child_content_id().unwrap();
    let content = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    let (key, id) = content
        .row_keys()
        .iter()
        .find_map(|key| {
            let id = content.row_id(key)?;
            view.tree
                .get(id)?
                .component::<OutputMessage>()
                .map(|_| (key.clone(), id))
        })
        .expect("real child output widget");
    let body = view
        .tree
        .get(id)
        .unwrap()
        .component::<OutputMessage>()
        .unwrap()
        .content_ids()
        .into_iter()
        .find(|part| part.kind == OutputContentKind::Text)
        .unwrap()
        .body;
    view.tree
        .get_mut(body)
        .unwrap()
        .component_mut::<TextFieldContent>()
        .unwrap()
        .surface
        .input
        .select_range(0..1);
    source.load_earlier().await.unwrap();
    view.update_child_snapshot(
        &RunScope::main("fixture", "b"),
        &source,
        source.snapshot().await.unwrap(),
    )
    .unwrap();
    let after = view
        .tree
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    assert_eq!(after.row_id(&key), Some(id));
    assert_eq!(
        view.tree.text_surface(body).unwrap().input.selection(),
        0..1
    );
    let text = view.tree.text_surface(body).unwrap().input.text.clone();
    view.child_read_failed(
        &RunScope::main("fixture", "b"),
        &source,
        "read failed; retained".into(),
    )
    .unwrap();
    assert_eq!(view.tree.text_surface(body).unwrap().input.text, text);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("read failed; retained"))
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn readonly_component_disables_supplied_turn_mutations_and_allows_only_explicit_reads() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let mut snapshot = source.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut snapshot else {
        panic!("read-only")
    };
    let turn = transcript.turns.first_mut().unwrap();
    let turn_id = turn.id.clone();
    turn.fork_boundary = Some(serde_json::json!({"atSeq":1}));
    turn.footnote_actions = vec![ThreadActionDefinition {
        id: "fork".into(),
        label: "forged fork".into(),
        icon: "fork".into(),
        interaction: "execute".into(),
        enabled: true,
        disabled_reason: None,
        copy_text: None,
    }];
    let mut view = ReadOnlyTranscript::standard(
        1200,
        1203..1400,
        source.binding().scope().clone(),
        Default::default(),
    )
    .unwrap();
    view.update(source.readonly_input(&snapshot, false, None).unwrap())
        .unwrap();
    let footer = view
        .content()
        .row_id(&TranscriptContentRowKey::TurnFootnote(turn_id))
        .unwrap();
    let root = view.ids().root;
    let mut host = view.into_host().unwrap();
    let button = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .action_id("fork")
        .unwrap();
    assert!(
        host.get(button)
            .unwrap()
            .component::<Button>()
            .unwrap()
            .button
            .state
            .disabled
    );
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<ReadOnlyTranscript>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
    let mut input = source.readonly_input(&snapshot, false, None).unwrap();
    input.frame_current = false;
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap();
    view.update(input).unwrap();
    let refresh = view.ids().refresh;
    let more = view.paging_button_id();
    assert!(
        host.get(more)
            .unwrap()
            .component::<Button>()
            .unwrap()
            .button
            .state
            .disabled
    );
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .take_intents(&actions);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].read(), ReadOnlyTranscriptRead::Refresh);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn conflicting_or_malformed_frames_leave_the_last_readonly_document_intact() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let original = source.snapshot().await.unwrap();
    let mut view = ReadOnlyTranscript::standard(
        1500,
        1503..1700,
        source.binding().scope().clone(),
        Default::default(),
    )
    .unwrap();
    view.update(source.readonly_input(&original, false, None).unwrap())
        .unwrap();
    let retained: Vec<_> = view
        .content()
        .row_keys()
        .iter()
        .map(|key| (key.clone(), view.content().row_id(key).unwrap()))
        .collect();
    let mut conflict = original.clone();
    if let PaneSnapshot::ReadOnlyChild { connection, .. } = &mut conflict {
        connection.message = Some("same version altered".into());
    }
    assert!(
        view.update(source.readonly_input(&conflict, false, None).unwrap())
            .is_err()
    );
    let mut malformed = source.snapshot().await.unwrap();
    if let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut malformed {
        transcript.items[0].content_blocks = Some(vec![
            TranscriptContentBlock {
                index: 0,
                content: TranscriptBlockContent::Text { text: "a".into() },
            },
            TranscriptContentBlock {
                index: 0,
                content: TranscriptBlockContent::Text { text: "b".into() },
            },
        ]);
    }
    assert!(
        view.update(source.readonly_input(&malformed, false, None).unwrap())
            .is_err()
    );
    for (key, id) in retained {
        assert_eq!(view.content().row_id(&key), Some(id));
    }
    assert!(
        ReadOnlyTranscript::standard(
            1500,
            1501..1700,
            source.binding().scope().clone(),
            Default::default()
        )
        .is_err()
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn a_captured_page_cannot_cross_epochs_but_explicit_refresh_can_renew_the_frame() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let snapshot = source.snapshot().await.unwrap();
    let mut view = ReadOnlyTranscript::standard(
        1800,
        1803..2000,
        source.binding().scope().clone(),
        Default::default(),
    )
    .unwrap();
    view.update(source.readonly_input(&snapshot, false, None).unwrap())
        .unwrap();
    let root = view.ids().root;
    let more = view.paging_button_id();
    let refresh = view.ids().refresh;
    let mut host = view.into_host().unwrap();
    host.accessibility_action(more, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let mut reads = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .take_intents(&actions);
    assert_eq!(reads.len(), 1);
    let page = reads.remove(0);
    assert_eq!(page.read(), ReadOnlyTranscriptRead::Earlier);
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let error = source
        .read_intent(page)
        .await
        .err()
        .expect("old page rejected before IO");
    assert!(error.to_string().contains("epoch expired"));
    host.get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .update(source.readonly_input(&snapshot, false, None).unwrap())
        .unwrap();
    host.accessibility_action(refresh, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let refresh = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .take_intents(&actions)
        .remove(0);
    let duplicate = refresh.clone();
    let next = source.read_intent(refresh).await.unwrap();
    assert!(
        source
            .read_intent(duplicate)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("already admitted")
    );
    let (
        PaneSnapshot::ReadOnlyChild {
            connection_epoch: old,
            ..
        },
        PaneSnapshot::ReadOnlyChild {
            connection_epoch: new,
            ..
        },
    ) = (snapshot, next)
    else {
        panic!("frames")
    };
    assert!(new > old);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_child_frames_cannot_poison_the_host_cache_or_prevent_a_later_explicit_read() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let initial = source.snapshot().await.unwrap();
    let mut stale = initial.clone();
    let mut view = host(runtime.clone()).await;
    let request = view
        .begin_child_open(&RunScope::main("fixture", "b"))
        .unwrap();
    view.install_child_source(request, source.clone(), "bounded".into(), initial)
        .unwrap();
    // Finish the mount-triggered refresh before testing a new explicit retry.
    for command in view.take_background_commands() {
        let tekes_agent_ui_demo::component_demo::DemoCommand::ReadChild {
            source: mounted,
            panel_parent,
            ..
        } = command
        else {
            panic!("only mounted child refresh");
        };
        assert!(Arc::ptr_eq(&mounted, &source));
        let frame = mounted.snapshot().await.unwrap();
        view.update_child_snapshot(&panel_parent, &mounted, frame)
            .unwrap();
    }
    let original = view.bounded_child_content_id().unwrap();
    for change in ["blocks", "lane"] {
        let mut invalid = source.snapshot().await.unwrap();
        if let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut invalid {
            if change == "blocks" {
                transcript.items[0].content_blocks = Some(vec![
                    TranscriptContentBlock {
                        index: 0,
                        content: TranscriptBlockContent::Text { text: "a".into() },
                    },
                    TranscriptContentBlock {
                        index: 0,
                        content: TranscriptBlockContent::Text { text: "b".into() },
                    },
                ]);
            } else {
                transcript.items[0].kind = if transcript.items[0].kind == TranscriptItemKind::User {
                    TranscriptItemKind::Assistant
                } else {
                    TranscriptItemKind::User
                };
            }
        }
        assert!(
            view.update_child_snapshot(&RunScope::main("fixture", "b"), &source, invalid)
                .is_err()
        );
        view.sync().unwrap();
        assert_eq!(view.bounded_child_content_id(), Some(original));
    }
    let (refresh, _) = view.bounded_child_buttons().unwrap();
    let commands = view
        .accessibility_action(refresh, None, AccessibilityAction::Press)
        .unwrap();
    let [
        tekes_agent_ui_demo::component_demo::DemoCommand::ReadChild {
            source: retry_source,
            panel_parent,
            read_intent: Some(intent),
            page: false,
        },
    ] = commands.as_slice()
    else {
        panic!("one explicit retry on original source");
    };
    assert!(Arc::ptr_eq(retry_source, &source));
    let repaired = source.read_intent(intent.clone()).await.unwrap();
    assert!(
        view.update_child_snapshot(panel_parent, &source, repaired)
            .unwrap()
    );
    assert_eq!(view.bounded_child_content_id(), Some(original));
    if let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut stale {
        transcript.items[0].content_blocks = Some(vec![
            TranscriptContentBlock {
                index: 0,
                content: TranscriptBlockContent::Text {
                    text: "stale".into(),
                },
            },
            TranscriptContentBlock {
                index: 0,
                content: TranscriptBlockContent::Text {
                    text: "stale".into(),
                },
            },
        ]);
    }
    assert!(
        !view
            .update_child_snapshot(&RunScope::main("fixture", "b"), &source, stale)
            .unwrap()
    );
    view.sync().unwrap();
    runtime.stop().await.unwrap();
}
struct InvalidSource(TranscriptSnapshot);

#[tokio::test]
async fn readonly_offline_history_copies_loaded_user_text_without_a_write_controller() {
    let runtime = runtime().await;
    let source = source(runtime.clone(), None).await;
    let mut frame = source.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut frame else {
        panic!("read-only")
    };
    transcript.items[0].kind = tekes_agent_presentation::TranscriptItemKind::User;
    transcript.items[0].text = "只读历史输入".into();
    transcript.items[0].content_blocks = None;
    let mut view = ReadOnlyTranscript::standard(
        7000,
        7003..7100,
        source.binding().scope().clone(),
        Default::default(),
    )
    .unwrap();
    let mut input = source.readonly_input(&frame, false, Some("离线")).unwrap();
    input.source_ready = false;
    input.frame_current = false;
    view.update(input).unwrap();
    let rows: Vec<_> = view
        .content()
        .row_keys()
        .iter()
        .map(|key| view.content().row_id(key).unwrap())
        .collect();
    let root = view.ids().root;
    let mut host = view.into_host().unwrap();
    let button = rows
        .into_iter()
        .find_map(|id| host.get(id)?.component::<InputMessage>()?.copy_button_id())
        .unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap();
    assert!(
        view.take_intents(&actions).is_empty(),
        "copy does not refresh or mutate"
    );
    let copies = view.take_copy_intents(&actions);
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].scope(), source.binding().scope());
    assert_eq!(copies[0].text(), "只读历史输入");
    runtime.stop().await.unwrap();
}
#[async_trait::async_trait]
impl ChildHistorySource for InvalidSource {
    async fn read(&self, _: &RunScope, _: &str) -> Result<TranscriptSnapshot> {
        Ok(self.0.clone())
    }
}
#[tokio::test]
async fn invalid_custom_history_is_rejected_before_it_becomes_a_renderable_frame() {
    let runtime = runtime().await;
    let original = source(runtime.clone(), None).await;
    let PaneSnapshot::ReadOnlyChild { mut transcript, .. } = original.snapshot().await.unwrap()
    else {
        panic!("bounded")
    };
    transcript.items[0].content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 1,
            content: TranscriptBlockContent::Text { text: "a".into() },
        },
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: "b".into() },
        },
    ]);
    let source = source(runtime.clone(), Some(Arc::new(InvalidSource(transcript)))).await;
    assert!(
        source
            .snapshot()
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("content block order")
    );
    runtime.stop().await.unwrap();
}

#[test]
fn bounded_readonly_wrapper_rejects_an_independently_writable_feedback_configuration() {
    let scope = RunScope::main("fixture", "bounded-b");
    let mut content =
        TranscriptContent::new(803, 804..850, scope.clone(), Default::default()).unwrap();
    content
        .configure_feedback(Some(Default::default()))
        .unwrap();
    let ids = ReadOnlyTranscriptIds {
        root: ComponentId(800_u64 << 32),
        status: ComponentId((800_u64 << 32) + 1),
        refresh: ComponentId((800_u64 << 32) + 2),
        viewport: ComponentId((800_u64 << 32) + 3),
    };
    assert!(ReadOnlyTranscript::new(ids, scope, 801, content, Default::default()).is_err());
}

#[tokio::test]
async fn explicit_bounded_page_size_is_validated_and_retained_through_refresh_and_paging() {
    let runtime = runtime().await;
    let parent = RunScope::main("fixture", "b");
    for size in [0, 201] {
        assert!(
            ChildPaneBinding::open_with_page_size(
                runtime.clone(),
                parent.clone(),
                "bounded-b",
                ChildHistoryLayout::SameSession,
                None,
                size
            )
            .await
            .is_err()
        );
    }
    assert!(
        ChildPaneBinding::open_with_page_size(
            runtime.clone(),
            parent.clone(),
            "child-b",
            ChildHistoryLayout::IndependentSessions,
            None,
            4
        )
        .await
        .is_err()
    );
    let source = ChildPaneBinding::open_with_page_size(
        runtime.clone(),
        parent,
        "bounded-b",
        ChildHistoryLayout::SameSession,
        None,
        4,
    )
    .await
    .unwrap();
    let before = source.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild {
        transcript: first, ..
    } = &before
    else {
        panic!("bounded")
    };
    assert!(first.has_more_before && !first.items.is_empty());
    assert!(source.binding().load_earlier().await.unwrap());
    let after = source.snapshot().await.unwrap();
    let PaneSnapshot::ReadOnlyChild {
        transcript: next, ..
    } = after
    else {
        panic!("bounded")
    };
    assert!(next.items.len() > first.items.len());
    assert!(
        first
            .items
            .iter()
            .all(|row| next.items.iter().any(|new| new.id == row.id))
    );
    assert!(source.binding().current_run().is_none());
    runtime.stop().await.unwrap();
}

fn captured_original_refresh(
    source: &ChildPaneBinding,
    frame: &PaneSnapshot,
) -> ReadOnlyTranscriptIntent {
    let mut view = tekes_agent_ui_consumer_check::new_readonly_child(
        4000,
        4003..4200,
        source.binding().scope().clone(),
    )
    .unwrap();
    tekes_agent_ui_consumer_check::sync_readonly_child(&mut view, source, frame, false, None)
        .unwrap();
    captured_refresh_from_view(view)
}
fn captured_refresh_from_view(view: ReadOnlyTranscript) -> ReadOnlyTranscriptIntent {
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    host.accessibility_action(ids.refresh, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<ReadOnlyTranscript>()
        .unwrap()
        .take_intents(&actions)
        .remove(0)
}

#[tokio::test]
async fn same_scope_readonly_frame_cannot_be_relabelled_by_another_binding_instance() {
    let runtime = runtime().await;
    let first = source(runtime.clone(), None).await;
    let second = source(runtime.clone(), None).await;
    let frame = first.snapshot().await.unwrap();
    let other = second.snapshot().await.unwrap();
    assert_eq!(first.binding().scope(), second.binding().scope());
    assert!(second.readonly_input(&frame, false, None).is_err());
    let mut view = tekes_agent_ui_consumer_check::new_readonly_child(
        4000,
        4003..4200,
        first.binding().scope().clone(),
    )
    .unwrap();
    tekes_agent_ui_consumer_check::sync_readonly_child(&mut view, &first, &frame, false, None)
        .unwrap();
    assert!(
        tekes_agent_ui_consumer_check::sync_readonly_child(&mut view, &second, &other, false, None)
            .is_err(),
        "a mounted original source cannot silently become a different instance"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn same_scope_readonly_intent_cannot_cross_instances_or_consume_original_admission() {
    let runtime = runtime().await;
    let first = source(runtime.clone(), None).await;
    let second = source(runtime.clone(), None).await;
    let frame = first.snapshot().await.unwrap();
    let _ = second.snapshot().await.unwrap();
    let intent = captured_original_refresh(&first, &frame);
    let original = intent.clone();
    assert!(second.read_intent(intent).await.is_err());
    first.read_intent(original.clone()).await.unwrap();
    assert!(
        first.read_intent(original).await.is_err(),
        "cloned original gesture is still once only"
    );
    let fabricated = ReadOnlyTranscriptSource::default();
    for source_id in [None, Some(&fabricated)] {
        let mut display_frame = frame.clone();
        let PaneSnapshot::ReadOnlyChild { transcript, .. } = &mut display_frame else {
            panic!("bounded")
        };
        transcript.has_more_before = true;
        let mut display = tekes_agent_ui_consumer_check::new_readonly_child(
            4000,
            4003..4200,
            first.binding().scope().clone(),
        )
        .unwrap();
        let mut input = first.readonly_input(&display_frame, false, None).unwrap();
        input.source = source_id;
        input.supports_pagination = true;
        assert!(input.source_ready && input.frame_current);
        assert!(input.supports_pagination && input.transcript.has_more_before);
        display.update(input).unwrap();
        let root = display.ids().root;
        let mut display_host = display.into_host().unwrap();
        let refresh = display_host
            .get(root)
            .unwrap()
            .component::<ReadOnlyTranscript>()
            .unwrap()
            .ids()
            .refresh;
        let page = display_host
            .get(root)
            .unwrap()
            .component::<ReadOnlyTranscript>()
            .unwrap()
            .paging_button_id();
        display_host.accessibility_action(refresh, None, AccessibilityAction::Press);
        display_host.accessibility_action(page, None, AccessibilityAction::Press);
        let actions = display_host.drain_actions();
        let intents = display_host
            .get_mut(root)
            .unwrap()
            .component_mut::<ReadOnlyTranscript>()
            .unwrap()
            .take_intents(&actions);
        if source_id.is_none() {
            assert!(
                display_host
                    .get(refresh)
                    .unwrap()
                    .component::<Button>()
                    .unwrap()
                    .button
                    .state
                    .disabled,
                "display-only provenance must disable refresh"
            );
            assert!(
                display_host
                    .get(page)
                    .unwrap()
                    .component::<Button>()
                    .unwrap()
                    .button
                    .state
                    .disabled,
                "display-only provenance must disable paging"
            );
            assert!(
                intents.is_empty(),
                "display-only provenance must not produce read intents"
            );
        } else {
            assert_eq!(
                intents.len(),
                2,
                "fake provenance still reaches binding admission"
            );
            for intent in intents {
                assert!(first.read_intent(intent).await.is_err());
            }
        }
    }
    let intent = captured_original_refresh(&first, &frame);
    first.retire();
    assert!(
        second.read_intent(intent).await.is_err(),
        "retired A does not transfer authority to a new same-scope B"
    );
    runtime.stop().await.unwrap();
}
