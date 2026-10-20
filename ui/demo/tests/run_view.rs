use tekes_agent_components_ui::*;
use tekes_agent_presentation::RunSnapshot;
use tekes_ui_controls::composition::*;
fn ids() -> RunViewIds {
    RunViewIds {
        root: ComponentId(700_u64 << 32),
        indicators: ComponentId((700_u64 << 32) + 1),
        viewport: ComponentId((700_u64 << 32) + 2),
        composer: ComposerIds::new((700_u64 << 32) + 16),
    }
}
fn view() -> RunView {
    RunView::new(
        ids(),
        RunScope::main("fixture", "a"),
        Composer::new(ids().composer),
        701,
        702,
    )
    .unwrap()
    .with_transcript(703, 704..1000, Default::default())
    .unwrap()
}

#[tokio::test]
async fn run_routes_loaded_input_copy_and_blocks_it_while_composing() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut run = view();
    update(&mut run, &snapshot).unwrap();
    let rows: Vec<_> = run
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|key| run.transcript().unwrap().row_id(key).unwrap())
        .collect();
    let mut host = run.into_host().unwrap();
    let row_id = rows
        .into_iter()
        .find(|id| host.get(*id).unwrap().component::<InputMessage>().is_some())
        .unwrap();
    let row = host
        .get(row_id)
        .unwrap()
        .component::<InputMessage>()
        .unwrap();
    let button = row.copy_button_id().unwrap();
    host.accessibility_action(button, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids().root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap()
        .take_intents(&actions);
    assert!(
        matches!(intents.as_slice(), [RunViewIntent::CopyTranscript(copy)] if copy.scope()==&RunScope::main("fixture","a") && !copy.text().is_empty())
    );
    host.focus(Some(ids().composer.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    });
    assert!(
        host.get_mut(ids().root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
    runtime.stop().await.unwrap();
}
fn update(view: &mut RunView, snapshot: &RunSnapshot) -> Result<(), &'static str> {
    view.update(
        snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        TranscriptPagingFacts {
            scope: RunScope::main("fixture", "a"),
            supported: false,
            has_more_before: snapshot.transcript.has_more_before,
            is_stale: snapshot.transcript.is_stale,
            source_ready: true,
            read: TranscriptPageRead::Idle,
        },
        true,
    )
}
#[tokio::test]
async fn a_run_mounts_the_real_components_and_emits_original_destination_intents() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut view = view();
    update(&mut view, &snapshot).unwrap();
    let doc = view.transcript_id().unwrap();
    let mut host = view.into_host().unwrap();
    assert!(
        host.get(ids().indicators)
            .unwrap()
            .component::<Indicators>()
            .is_some()
    );
    assert_eq!(
        host.get(doc)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
            .row_keys()
            .len(),
        30
    );
    host.focus(Some(ids().composer.editor), true);
    host.event(&ContentEvent::Commit {
        text: "RunView 原始输入".into(),
        replacement: None,
    });
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids().root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap()
        .take_intents(&actions);
    let [RunViewIntent::Composer(ComposerIntent::Edit { destination, text })] = intents.as_slice()
    else {
        panic!("one shared edit intent");
    };
    assert_eq!(
        destination,
        &ComposerDestination::run(RunScope::main("fixture", "a"))
    );
    assert_eq!(text, "RunView 原始输入");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn source_epoch_and_same_publication_conflicts_leave_the_whole_view_unchanged() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let initial = controller.snapshot();
    let mut view = view();
    update(&mut view, &initial).unwrap();
    let before = view.composer().editor().input.text.clone();
    let mut unversioned = (*initial).clone();
    unversioned.revision = 0;
    let mut conflicting = (*initial).clone();
    conflicting.draft.text = "不可覆盖".into();
    let mut conflicting_epoch = (*initial).clone();
    conflicting_epoch.connection_epoch += 1;
    for bad in [unversioned, conflicting, conflicting_epoch] {
        assert!(update(&mut view, &bad).is_err());
        assert_eq!(view.composer().editor().input.text, before);
    }
    let mut foreign = (*initial).clone();
    foreign.revision += 1;
    foreign.selection.as_mut().unwrap().destination =
        ComposerDestination::run(RunScope::main("fixture", "b"));
    assert!(update(&mut view, &foreign).is_err());
    let mut next = (*initial).clone();
    next.revision += 1;
    next.connection_epoch += 1;
    update(&mut view, &next).unwrap();
    let mut old_epoch = next.clone();
    old_epoch.revision += 1;
    old_epoch.connection_epoch -= 1;
    assert!(update(&mut view, &old_epoch).is_err());
    assert_eq!(view.composer().editor().input.text, before);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn invalid_transcript_is_preflighted_before_a_new_draft_can_change_the_composer() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let initial = controller.snapshot();
    let mut view = view();
    update(&mut view, &initial).unwrap();
    let before = view.composer().editor().input.text.clone();
    let mut bad = (*initial).clone();
    bad.revision += 1;
    bad.draft.text = "尚未采用的新草稿".into();
    let item = bad
        .transcript
        .items
        .iter_mut()
        .find(|item| item.kind == TranscriptItemKind::User)
        .unwrap();
    item.content_blocks = Some(vec![
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: "重复块".into(),
            },
        },
        TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text {
                text: "重复块".into(),
            },
        },
    ]);
    assert!(update(&mut view, &bad).is_err());
    assert_eq!(view.composer().editor().input.text, before);
    update(&mut view, &initial).unwrap();
    runtime.stop().await.unwrap();
}
#[test]
fn namespaces_include_future_rows_hidden_context_and_reserved_controls() {
    let composer = Composer::new(ids().composer)
        .with_context(799, Default::default())
        .unwrap();
    let mut view = RunView::new(ids(), RunScope::main("fixture", "a"), composer, 701, 702).unwrap();
    assert!(
        view.install_transcript(
            TranscriptContent::new(
                703,
                704..800,
                RunScope::main("fixture", "a"),
                Default::default()
            )
            .unwrap()
        )
        .is_err()
    );
    assert!(view.transcript_id().is_none());
    let content = TranscriptContent::new(
        703,
        704..799,
        RunScope::main("fixture", "a"),
        Default::default(),
    )
    .unwrap();
    view.install_transcript(content).unwrap();
    assert!(
        view.install_transcript(
            TranscriptContent::new(
                800,
                801..900,
                RunScope::main("fixture", "a"),
                Default::default()
            )
            .unwrap()
        )
        .is_err()
    );
    let mut overlap = ids();
    overlap.viewport = overlap.composer.send;
    assert!(
        RunView::new(
            overlap,
            RunScope::main("fixture", "a"),
            Composer::new(overlap.composer),
            701,
            702
        )
        .is_err()
    );
}

#[tokio::test]
async fn preedit_blocks_send_and_history_intents_until_the_original_composition_finishes() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.revision += 1;
    snapshot.draft.text = "原稿".into();
    let mut view = view();
    let paging = TranscriptPagingFacts {
        scope: RunScope::main("fixture", "a"),
        supported: true,
        has_more_before: snapshot.transcript.has_more_before,
        is_stale: snapshot.transcript.is_stale,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    };
    view.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging,
        true,
    )
    .unwrap();
    let mut host = view.into_host().unwrap();
    host.focus(Some(ids().composer.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    });
    let actions = host.drain_actions();
    let view = host
        .get_mut(ids().root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap();
    assert!(view.composer().is_composing());
    assert!(view.take_intents(&actions).is_empty());
    for target in [ids().composer.send, ComponentId((701_u64 << 32) + 1)] {
        assert!(
            !view.allows_focus_target(target),
            "host must not forward a focus-stealing action during composition"
        );
    }
    assert!(view.allows_focus_target(ids().composer.editor));
    assert_eq!(host.focused(), Some(ids().composer.editor));
    host.event(&ContentEvent::Commit {
        text: "中文组字".into(),
        replacement: None,
    });
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids().root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap()
        .take_intents(&actions);
    assert!(matches!(
        intents.as_slice(),
        [RunViewIntent::Composer(ComposerIntent::Edit { .. })]
    ));
    assert!(
        host.get(ids().root)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .allows_focus_target(ComponentId((701_u64 << 32) + 1))
    );
    host.accessibility_action(
        ComponentId((701_u64 << 32) + 1),
        None,
        AccessibilityAction::Press,
    );
    let actions = host.drain_actions();
    let intents = host
        .get_mut(ids().root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap()
        .take_intents(&actions);
    assert!(
        matches!(intents.as_slice(),[RunViewIntent::LoadEarlier{scope}] if scope==&RunScope::main("fixture","a"))
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_auxiliary_facts_cannot_publish_a_new_draft_or_grant_controls() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let initial = controller.snapshot();
    let mut view = view();
    update(&mut view, &initial).unwrap();
    let mut next = (*initial).clone();
    next.revision += 1;
    next.draft.text = "未接收".into();
    let bad = TranscriptPagingFacts {
        scope: RunScope::main("fixture", "b"),
        supported: true,
        has_more_before: next.transcript.has_more_before,
        is_stale: next.transcript.is_stale,
        source_ready: true,
        read: TranscriptPageRead::Idle,
    };
    assert!(
        view.update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            bad,
            true
        )
        .is_err()
    );
    assert_eq!(view.composer().editor().input.text, initial.draft.text);
    next.run.as_mut().unwrap().actionables = vec![serde_json::json!({"id":"bad-source"})];
    assert!(update(&mut view, &next).is_err());
    assert_eq!(view.composer().editor().input.text, initial.draft.text);
    runtime.stop().await.unwrap();
}
