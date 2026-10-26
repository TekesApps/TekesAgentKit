// Append to the unchanged formal Run transaction probe. These are actual
// AgentKit rail controls, not a text/button substitute for its content.
use tekes_ui_controls::{ControlKey, ScrollRevealOptions, ScrollRevealPlacement};
fn rail(host: &ContentHost, root: ComponentId) -> &TurnRail {
    run(host, root).turn_rail().unwrap()
}
fn rail_mut(host: &mut ContentHost, root: ComponentId) -> &mut TurnRail {
    run_mut(host, root).turn_rail_mut().unwrap()
}
async fn rail_setup() -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    RunSnapshot,
    ReadingDocumentVersion,
    ReadingDocumentVersion,
    Text,
) {
    let (runtime, mut host, root, snapshot, version, mut text) = setup().await;
    tekes_agent_ui_consumer_check::install_turn_rail(run_mut(&mut host, root), 35000).unwrap();
    run_mut(&mut host, root)
        .set_rail_navigation_gate(TurnRailNavigationGate {
            turn_start: true,
            latest: true,
        })
        .unwrap();
    render(&mut host, &mut text);
    let r = rail(&host, root);
    let (rail_version, ()) = host
        .set_scroll_reading_document(
            r.viewport_id(),
            "real-agent-rail",
            r.document_revision(),
            r.item_ids(),
            |_| {},
        )
        .unwrap();
    render(&mut host, &mut text);
    (runtime, host, root, snapshot, version, rail_version, text)
}
fn navigate_rail(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    text: &mut Text,
) -> Option<TurnRailReveal> {
    let request = rail_mut(host, root).next_reveal()?;
    rail(host, root).claim_reveal(&request).unwrap();
    let result = host.reveal_scroll_reading_item(
        request.viewport(),
        version,
        request.item(),
        ScrollRevealOptions {
            placement: match request.reason() {
                TurnRailRevealReason::InitialLatest => ScrollRevealPlacement::Nearest,
                TurnRailRevealReason::FollowChanged => ScrollRevealPlacement::Centered,
            },
            animated: false,
            ..Default::default()
        },
        text,
    );
    assert!(rail_mut(host, root).finish_reveal(&request, result.is_ok()));
    result.unwrap();
    render(host, text);
    Some(request)
}
fn rail_intents(host: &mut ContentHost, root: ComponentId) -> Vec<RunTurnNavigation> {
    let actions = host.drain_actions();
    run_mut(host, root)
        .take_intents(&actions)
        .into_iter()
        .filter_map(|i| match i {
            RunPortIntent::TurnNavigation(n) => Some(*n),
            _ => None,
        })
        .collect()
}
#[tokio::test]
async fn actual_rail_exact_geometry_half_open_boundaries_and_full_single_ax_names_use_original_controls()
 {
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let geometry = run(&host, root).layout_geometry().unwrap().turn_rail;
    assert_eq!(rail(&host, root).layout_geometry(), Some(geometry));
    let ids = rail(&host, root).item_ids();
    assert_eq!(ids.len(), targets.len());
    for (i, id) in ids.iter().enumerate() {
        let f = host.get(*id).unwrap().frame();
        let layout = rail(&host, root)
            .action_layout(targets[i].turn_id())
            .unwrap();
        assert_eq!(f, layout.frame);
        assert_eq!(f.size.height, 10.);
        // ScrollContent reserves the library's focus bleed inside its viewport.
        // Rows fill that original document, without overriding the library inset.
        let document = host
            .get(ComponentId(rail(&host, root).id().0 + 2))
            .unwrap()
            .frame();
        assert_eq!(f.size.width, document.size.width);
        assert_eq!(f.min_x(), document.min_x());
        assert!(f.size.width <= geometry.track.size.width);
        let nodes = host.semantics();
        let named: Vec<_> = nodes
            .iter()
            .filter(|n| n.semantics.name.contains(targets[i].input()))
            .collect();
        assert_eq!(named.iter().filter(|n| n.id == *id).count(), 1);
        assert_eq!(nodes.iter().find(|n| n.id == *id).unwrap().frame, f);
    }
    let boundary = host.get(ids[1]).unwrap().frame().origin;
    host.event(&ContentEvent::PointerDown(Point::new(
        boundary.x + 1.,
        boundary.y,
    )));
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].target().turn_id(), targets[1].turn_id());
    host.event(&ContentEvent::PointerUp(Point::new(
        boundary.x + 1.,
        boundary.y,
    )));
    assert!(rail_intents(&mut host, root).is_empty());
    render(&mut host, &mut text);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn rail_press_release_cancel_keyboard_ax_and_preedit_capture_once_without_focus_substitution()
{
    let (runtime, mut host, root, _, _, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let row = rail(&host, root).row_id(targets[0].turn_id()).unwrap();
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "轨道组字".into(),
            selection: 0..2,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let point = host.get(row).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(point));
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    host.event(&ContentEvent::PointerDown(point));
    assert!(rail_intents(&mut host, root).is_empty());
    host.event(&ContentEvent::PointerMove(Point::new(900., 900.)));
    host.event(&ContentEvent::PointerUp(Point::new(900., 900.)));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(baseline(&host, root), before);
    host.accessibility_action(row, None, AccessibilityAction::Press);
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    host.event(&ContentEvent::Unmark);
    assert!(host.focus(Some(row), true));
    for key in [ControlKey::Enter, ControlKey::Space] {
        host.event(&ContentEvent::Key { key, down: true });
        let mut intents = rail_intents(&mut host, root);
        host.event(&ContentEvent::Key { key, down: false });
        intents.extend(rail_intents(&mut host, root));
        assert_eq!(intents.len(), 1);
    }
    assert_eq!(host.focused(), Some(row));
    host.event(&ContentEvent::PointerDown(point));
    assert_eq!(rail_intents(&mut host, root).len(), 1);
    // Host Cancel means window-wide input/focus cancellation in TekesUI.
    host.event(&ContentEvent::Cancel);
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    assert_eq!(host.focused(), None);
    // Keyboard state/activation only: wrapper ring layering is pending #33.
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn event_time_rail_capture_survives_in_place_publication_without_rebinding_and_disabled_gate_rejects()
 {
    let (runtime, mut host, root, snapshot, version, _, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    let id = rail(&host, root).row_id(targets[0].turn_id()).unwrap();
    let point = host.get(id).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(point));
    // Delay the owned intent drain across the actual shared publication plan.
    let next = append(&snapshot);
    let plan = prepare(run(&host, root), &next).unwrap();
    publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    let intents = rail_intents(&mut host, root);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].target().publication(), snapshot.revision);
    assert!(run(&host, root).claim_turn_navigation(&intents[0]).is_err());
    assert_eq!(rail(&host, root).row_id(targets[0].turn_id()), Some(id));
    host.event(&ContentEvent::PointerUp(point));
    assert!(rail_intents(&mut host, root).is_empty());
    run_mut(&mut host, root)
        .set_rail_navigation_gate(Default::default())
        .unwrap();
    render(&mut host, &mut text);
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    host.accessibility_action(id, None, AccessibilityAction::Press);
    assert!(rail_intents(&mut host, root).is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn rail_initial_and_changed_follow_are_honored_once_and_rejected_commands_require_explicit_retry()
 {
    let (runtime, mut host, root, _, _, rail_version, mut text) = rail_setup().await;
    let targets = run(&host, root).turn_targets().unwrap();
    rail_mut(&mut host, root).set_follow_target(Some(targets.last().unwrap().turn_id().into()));
    let first = navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert_eq!(first.reason(), TurnRailRevealReason::InitialLatest);
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).set_follow_target(Some(targets[0].turn_id().into()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).set_follow_target(Some(targets.last().unwrap().turn_id().into()));
    let pending = rail_mut(&mut host, root).next_reveal().unwrap();
    let clone = pending.clone();
    rail(&host, root).claim_reveal(&pending).unwrap();
    assert!(rail(&host, root).claim_reveal(&clone).is_err());
    assert!(rail_mut(&mut host, root).finish_reveal(&pending, false));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    rail_mut(&mut host, root).retry_reveal();
    let fresh = rail_mut(&mut host, root).next_reveal().unwrap();
    assert!(rail(&host, root).claim_reveal(&pending).is_err());
    rail(&host, root).claim_reveal(&fresh).unwrap();
    assert!(rail_mut(&mut host, root).finish_reveal(&fresh, true));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    runtime.stop().await.unwrap();
}

fn rail_scroll(host: &ContentHost, root: ComponentId) -> &ScrollContent {
    host.get(rail(host, root).viewport_id())
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
}
fn add_turns(snapshot: &RunSnapshot, count: usize) -> RunSnapshot {
    let mut next = snapshot.clone();
    next.revision += 1;
    let template = next.transcript.turns[0].clone();
    let input = next
        .transcript
        .items
        .iter()
        .find(|i| i.kind == TranscriptItemKind::User)
        .unwrap()
        .clone();
    for index in 0..count {
        let mut turn = template.clone();
        turn.id = format!("rail-history-{}-{index}", next.revision);
        turn.input = format!("历史问题 {index} 完整中文输入");
        let mut item = input.clone();
        item.id = format!("input-{}", turn.id);
        item.turn_id = turn.id.clone();
        item.text = turn.input.clone();
        item.record_id = None;
        item.reconciliation_id = None;
        item.source_sequence = Some(20000 + index as i64);
        item.source_position = Some(TranscriptSourcePosition::Journal {
            sequence: 20000 + index as i64,
            content_index: None,
        });
        next.transcript.turns.push(turn);
        next.transcript.items.push(item);
    }
    next
}
#[tokio::test]
async fn independent_rail_overflow_initial_latest_and_manual_scroll_do_not_move_the_transcript() {
    let (runtime, mut host, root, snapshot, version, rail_version, mut text) = rail_setup().await;
    let next = add_turns(&snapshot, 80);
    let plan = prepare(run(&host, root), &next).unwrap();
    // Both owners are checked before this synchronous publication. There is no
    // event, render, await or replacement of either owner between the updates.
    assert_eq!(
        rail_scroll(&host, root).reading_document_version(),
        Some(&rail_version)
    );
    let _version = publish_plan(&mut host, root, &version, &plan).unwrap();
    let r = rail(&host, root);
    let revision = r.document_revision();
    let ids = r.item_ids();
    let viewport = r.viewport_id();
    assert!(r.layout_geometry().is_none());
    let (rail_version, ()) = host
        .update_scroll_reading_document(viewport, &rail_version, revision, ids, |_| {})
        .unwrap();
    render(&mut host, &mut text);
    let main_before = scroll(&host, root).scroll.offset;
    assert!(rail_scroll(&host, root).scroll.limit().y > 200.);
    assert!(!rail_scroll(&host, root).scroll.scrollbar_visible(true));
    assert!(!rail_scroll(&host, root).scroll.axis_enabled(false));
    let targets = run(&host, root).turn_targets().unwrap();
    let last = targets.last().unwrap().turn_id().to_owned();
    rail_mut(&mut host, root).set_follow_target(Some(last.clone()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    let last_id = rail(&host, root).row_id(&last).unwrap();
    assert!(host.get(last_id).unwrap().visible().is_some());
    let before = rail_scroll(&host, root).scroll.offset.y;
    let point = rail(&host, root).layout_geometry().unwrap().track.center();
    host.event(&ContentEvent::Scroll {
        position: point,
        delta: Point::new(0., -90.),
        precise: true,
        begin: true,
        end: false,
        momentum: false,
        time: 1.,
    });
    render(&mut host, &mut text);
    let manual = rail_scroll(&host, root).scroll.offset.y;
    assert!(manual < before - 50.);
    rail_mut(&mut host, root).set_follow_target(Some(last));
    assert!(rail_mut(&mut host, root).next_reveal().is_none());
    render(&mut host, &mut text);
    assert_eq!(rail_scroll(&host, root).scroll.offset.y, manual);
    assert_eq!(scroll(&host, root).scroll.offset, main_before);
    rail_mut(&mut host, root).set_follow_target(Some(targets[0].turn_id().into()));
    navigate_rail(&mut host, root, &rail_version, &mut text).unwrap();
    assert!(
        host.get(rail(&host, root).row_id(targets[0].turn_id()).unwrap())
            .unwrap()
            .visible()
            .is_some()
    );
    assert_eq!(scroll(&host, root).scroll.offset, main_before);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unmount_remount_retires_both_navigation_and_reveal_without_replacing_the_original_editor()
{
    use tekes_ui_controls::extended::{Axis, Stack};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut view = RunPort::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
    view.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&view, &snapshot),
        true,
    )
    .unwrap();
    view.install_turn_rail(35000).unwrap();
    view.set_rail_navigation_gate(TurnRailNavigationGate {
        turn_start: true,
        latest: true,
    })
    .unwrap();
    let root = view.ids().root;
    let outer = ComponentId(1);
    let mut host = ContentHost::new(Content::new(
        outer,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: None,
            children: vec![Content::new(root, view)],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    render(&mut host, &mut text);
    let editor = run(&host, root).composer().editor() as *const _;
    let target = run(&host, root).turn_targets().unwrap().remove(0);
    let navigation = run(&host, root).prepare_turn_navigation(&target).unwrap();
    let reveal = rail_mut(&mut host, root).next_reveal().unwrap();
    let removed = host.remove(root).unwrap();
    assert!(
        removed
            .component::<RunPort>()
            .unwrap()
            .claim_turn_navigation(&navigation)
            .is_err()
    );
    host.insert(outer, removed).unwrap();
    render(&mut host, &mut text);
    assert_eq!(run(&host, root).composer().editor() as *const _, editor);
    assert!(run(&host, root).claim_turn_navigation(&navigation).is_err());
    assert!(rail(&host, root).claim_reveal(&reveal).is_err());
    let target = run(&host, root).turn_targets().unwrap().remove(0);
    let fresh = run(&host, root).prepare_turn_navigation(&target).unwrap();
    run(&host, root).claim_turn_navigation(&fresh).unwrap();
    runtime.stop().await.unwrap();
}
