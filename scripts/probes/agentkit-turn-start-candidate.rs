// Append to the unchanged original Run and rail probes, using only #32's
// original item reveal. No offsets are written and no margins are fabricated.
fn clipped_render(host: &mut ContentHost, text: &mut Text) {
    let frame = Rect::new(0., 0., 760., 650.);
    host.render(
        &mut Scene::new(),
        text,
        frame,
        Rect::new(0., 150., 760., 500.),
        Theme::LIGHT.into(),
    )
    .unwrap();
}
fn start_options() -> ScrollRevealOptions {
    ScrollRevealOptions {
        placement: ScrollRevealPlacement::Start(tekes_ui_controls::extended::Axis::Vertical),
        animated: false,
        ..Default::default()
    }
}
fn dispatch_start(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    intent: &RunTurnNavigation,
    text: &mut Text,
) -> ContentRevealResult {
    let kind =
        tekes_agent_ui_consumer_check::claim_turn_navigation(run(host, root), intent).unwrap();
    let RunTurnNavigationKind::TurnStart(item) = kind else {
        panic!("ordinary turn must retain Start semantics")
    };
    host.reveal_scroll_reading_item(intent.viewport(), version, item, start_options(), text)
        .unwrap()
}
#[tokio::test]
async fn actual_ordinary_rail_click_aligns_at_visible_clip_start_and_preserves_original_preedit() {
    let (runtime, mut host, root, _, version, _, mut text) = rail_setup().await;
    let editor = run(&host, root).ids().composer.editor;
    assert!(host.focus(Some(editor), true));
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "普通回合组字".into(),
            selection: 1..4,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    clipped_render(&mut host, &mut text);
    let before = baseline(&host, root);
    let original_editor = run(&host, root).composer().editor() as *const _;
    let target = run(&host, root).turn_targets().unwrap().remove(3);
    let row = rail(&host, root).row_id(target.turn_id()).unwrap();
    let point = host.get(row).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::PointerDown(point));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    let intent = &intents[0];
    assert_eq!(intent.target().turn_id(), target.turn_id());
    let x = scroll(&host, root).scroll.offset.x;
    let result = dispatch_start(&mut host, root, &version, intent, &mut text);
    assert_eq!(result.status, ContentRevealStatus::Completed);
    assert!(!result.clamped);
    clipped_render(&mut host, &mut text);
    let top = host
        .get(run(&host, root).ids().viewport)
        .unwrap()
        .visible()
        .unwrap()
        .min_y();
    assert!(top >= 150.);
    assert!((host.get(result.item).unwrap().frame().min_y() - top).abs() < 0.01);
    assert_eq!(scroll(&host, root).scroll.offset.x, x);
    assert!(!scroll(&host, root).is_following_latest());
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(
        run(&host, root).composer().editor() as *const _,
        original_editor
    );
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    assert!(run(&host, root).composer().is_composing());
    assert!(run(&host, root).claim_turn_navigation(intent).is_err());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn start_moves_an_already_visible_middle_target_and_a_viewport_tall_input_to_the_same_actual_start()
 {
    let (runtime, mut host, root, snapshot, version, _, mut text) = rail_setup().await;
    let mut next = snapshot.clone();
    next.revision += 1;
    let turn = next.transcript.turns[3].id.clone();
    let tall = "高消息中的中文内容，仍由原正文控件测量换行。\n".repeat(90);
    next.transcript
        .items
        .iter_mut()
        .find(|i| i.kind == TranscriptItemKind::User && i.turn_id == turn)
        .unwrap()
        .text = tall.clone();
    next.transcript.turns[3].input = tall;
    let plan = prepare(run(&host, root), &next).unwrap();
    let version = publish_plan(&mut host, root, &version, &plan).unwrap();
    clipped_render(&mut host, &mut text);
    let target = run(&host, root)
        .turn_targets()
        .unwrap()
        .into_iter()
        .find(|t| t.turn_id() == turn)
        .unwrap();
    let item = target.row_ids()[0];
    // First place the original tall row's midpoint in the clip. Start must
    // still move its top, rather than accepting partial visibility as success.
    host.reveal_scroll_reading_item(
        run(&host, root).ids().viewport,
        &version,
        item,
        ScrollRevealOptions {
            placement: ScrollRevealPlacement::Centered,
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    clipped_render(&mut host, &mut text);
    let viewport = run(&host, root).ids().viewport;
    let visible = host.get(viewport).unwrap().visible().unwrap();
    let row_frame = host.get(item).unwrap().frame();
    assert!(row_frame.size.height > visible.size.height);
    assert!(host.get(item).unwrap().visible().is_some());
    assert!(row_frame.min_y() < visible.min_y() - 50.);
    let target = run(&host, root)
        .turn_targets()
        .unwrap()
        .into_iter()
        .find(|t| t.turn_id() == turn)
        .unwrap();
    let intent =
        tekes_agent_ui_consumer_check::prepare_turn_navigation(run(&host, root), &target).unwrap();
    let result = dispatch_start(&mut host, root, &version, &intent, &mut text);
    assert!(!result.clamped);
    clipped_render(&mut host, &mut text);
    let visible = host.get(viewport).unwrap().visible().unwrap();
    assert!((host.get(item).unwrap().frame().min_y() - visible.min_y()).abs() < 0.01);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn actual_rail_freezes_main_history_before_a_later_reading_fact_and_public_consumer_calls_hooks_once()
 {
    #[derive(Default)]
    struct History(Vec<String>);
    impl RunTurnNavigationHost for History {
        fn record_turn_jump(&mut self, jump: &RunTurnJump) {
            self.0.push(format!(
                "record:{}:{}",
                jump.from_turn().unwrap_or("none"),
                jump.to_turn()
            ));
        }
        fn take_pending_turn_reveal(&mut self, scope: &RunScope) -> Option<String> {
            self.0.push(format!("take:{}", scope.thread_id));
            Some("held-reveal".into())
        }
    }
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    run_mut(&mut host, root)
        .set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: true,
        }))
        .unwrap();
    let row = rail(&host, root).row_id(targets[3].turn_id()).unwrap();
    let point = host.get(row).unwrap().visible().unwrap().center();
    host.event(&ContentEvent::PointerDown(point));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    run_mut(&mut host, root)
        .set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: false,
        }))
        .unwrap();
    let mut history = History::default();
    tekes_agent_ui_consumer_check::claim_turn_navigation_with_host(
        run(&host, root),
        &intents[0],
        &mut history,
    )
    .unwrap();
    assert_eq!(
        history.0,
        vec![
            format!(
                "record:{}:{}",
                targets.last().unwrap().turn_id(),
                targets[3].turn_id()
            ),
            "take:a".into()
        ]
    );
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert!(
        tekes_agent_ui_consumer_check::claim_turn_navigation_with_host(
            run(&host, root),
            &intents[0].clone(),
            &mut history
        )
        .is_err()
    );
    assert_eq!(history.0.len(), 2);
    render(&mut host, &mut text);
    runtime.stop().await.unwrap();
}
