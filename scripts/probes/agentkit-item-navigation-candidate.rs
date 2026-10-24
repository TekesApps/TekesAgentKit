// Appended to the unchanged formal RunPort transaction probe. Only public
// stable IDs/options and the original backend cross the library boundary.
use tekes_ui_controls::{ScrollRevealOptions, ScrollRevealPlacement};
fn navigation(host: &ContentHost, root: ComponentId, index: usize) -> RunTurnNavigation {
    let view = run(host, root);
    let target = view.turn_targets().unwrap().remove(index);
    tekes_agent_ui_consumer_check::prepare_turn_navigation(view, &target).unwrap()
}
// Technical centered-placement probe only. #32 is still required for the
// TurnStart intent; this adapter is not a production turn-jump implementation.
fn reveal_navigation(
    host: &mut ContentHost,
    root: ComponentId,
    version: &ReadingDocumentVersion,
    request: &RunTurnNavigation,
    text: &mut Text,
) -> Result<(), String> {
    let kind = tekes_agent_ui_consumer_check::claim_turn_navigation(run(host, root), request)
        .map_err(str::to_owned)?;
    match kind {
        RunTurnNavigationKind::TurnStart(item) => {
            host.reveal_scroll_reading_item(
                request.viewport(),
                version,
                item,
                ScrollRevealOptions {
                    placement: ScrollRevealPlacement::Centered,
                    animated: false,
                    ..Default::default()
                },
                text,
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        RunTurnNavigationKind::Latest => {
            if scroll(host, root).reading_document_version() != Some(version) {
                return Err("stale latest attachment".into());
            }
            host.get_mut(request.viewport())
                .unwrap()
                .component_mut::<ScrollContent>()
                .unwrap()
                .follow_latest();
        }
    }
    // Always request/render after an accepted command, including changed=false.
    // Native publication may occur only after that successful render.
    render(host, text);
    Ok(())
}
#[tokio::test]
async fn original_run_item_navigation_and_latest_preserve_marked_editor_and_second_consumer_identity()
 {
    let (runtime, mut host, root, _, version, mut text) = setup().await;
    let editor = run(&host, root).ids().composer.editor;
    assert!(host.focus(Some(editor), true));
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "中文组字".into(),
            selection: 0..4,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let original = run(&host, root).composer().editor() as *const _;
    let request = navigation(&host, root, 0);
    let destination = request.target().turn_id().to_owned();
    let copy = request.clone();
    reveal_navigation(&mut host, root, &version, &request, &mut text).unwrap();
    assert!(
        run(&host, root)
            .turn_targets()
            .unwrap()
            .iter()
            .any(|t| t.turn_id() == destination && t.is_visible() == Some(true))
    );
    assert!(!scroll(&host, root).is_following_latest());
    assert!(reveal_navigation(&mut host, root, &version, &copy, &mut text).is_err());
    assert_eq!(run(&host, root).composer().editor() as *const _, original);
    assert_eq!(baseline(&host, root), before);
    assert!(run(&host, root).composer().is_composing());
    assert_eq!(host.focused(), Some(editor));
    let last = run(&host, root).turn_targets().unwrap().len() - 1;
    let latest = navigation(&host, root, last);
    assert_eq!(latest.kind(), RunTurnNavigationKind::Latest);
    reveal_navigation(&mut host, root, &version, &latest, &mut text).unwrap();
    assert!(scroll(&host, root).is_following_latest());
    assert_eq!(baseline(&host, root), before);
    assert!(run(&host, root).composer().is_composing());
    assert_eq!(host.focused(), Some(editor));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn old_reading_attachment_and_foreign_run_cannot_dispatch_a_local_turn_to_new_content() {
    let (runtime, mut host, root, snapshot, version, mut text) = setup().await;
    let request = navigation(&host, root, 0);
    let viewport = request.viewport();
    // Same document key and business revision, new library attachment: ABA.
    let (replacement, ()) = host
        .set_scroll_reading_document(
            viewport,
            ComposerDestination::run(run(&host, root).scope().clone()).storage_key(),
            snapshot.revision,
            run(&host, root)
                .transcript()
                .unwrap()
                .row_keys()
                .iter()
                .map(|key| run(&host, root).transcript().unwrap().row_id(key).unwrap())
                .collect(),
            |_| {},
        )
        .unwrap();
    render(&mut host, &mut text);
    let before = baseline(&host, root);
    let offset = scroll(&host, root).scroll.offset;
    assert!(reveal_navigation(&mut host, root, &version, &request, &mut text).is_err());
    assert_eq!(baseline(&host, root), before);
    assert_eq!(scroll(&host, root).scroll.offset, offset);
    let fresh = navigation(&host, root, 0);
    reveal_navigation(&mut host, root, &replacement, &fresh, &mut text).unwrap();
    let foreign = RunPort::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
    assert!(
        foreign
            .claim_turn_navigation(&navigation(&host, root, 0))
            .is_err()
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn item_navigation_then_append_and_paging_keep_manual_reading_and_original_composer() {
    let (runtime, mut host, root, snapshot, version, mut text) = setup().await;
    let request = navigation(&host, root, 2);
    reveal_navigation(&mut host, root, &version, &request, &mut text).unwrap();
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "持续组字".into(),
            selection: 0..2,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    render(&mut host, &mut text);
    let reading = anchor(&host, root);
    let y = host.get(reading).unwrap().frame().min_y();
    let before = baseline(&host, root);
    let next = append(&snapshot);
    let plan = prepare(run(&host, root), &next).unwrap();
    let next_version = publish_plan(&mut host, root, &version, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(anchor(&host, root), reading);
    assert!((host.get(reading).unwrap().frame().min_y() - y).abs() < 1.);
    assert!(!scroll(&host, root).is_following_latest());
    assert_eq!(baseline(&host, root), before);
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&next_version)
    );
    let mut prefix = next.clone();
    prefix.revision += 1;
    let mut older = prefix
        .transcript
        .items
        .iter()
        .find(|item| item.kind == TranscriptItemKind::User)
        .unwrap()
        .clone();
    older.id = "item-navigation-older".into();
    older.turn_id = "item-navigation-older-turn".into();
    older.record_id = None;
    older.reconciliation_id = None;
    older.source_sequence = Some(0);
    older.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 0,
        content_index: None,
    });
    older.metadata = Default::default();
    prefix.transcript.turns.insert(
        0,
        TranscriptTurn {
            id: older.turn_id.clone(),
            status: "completed".into(),
            input: older.text.clone(),
            started_at: Some(0.),
            ended_at: Some(1.),
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        },
    );
    prefix.transcript.items.insert(0, older);
    let plan = prepare(run(&host, root), &prefix).unwrap();
    let final_version = publish_plan(&mut host, root, &next_version, &plan).unwrap();
    render(&mut host, &mut text);
    assert_eq!(anchor(&host, root), reading);
    assert!((host.get(reading).unwrap().frame().min_y() - y).abs() < 1.);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    assert!(run(&host, root).composer().is_composing());
    assert_eq!(
        scroll(&host, root).reading_document_version(),
        Some(&final_version)
    );
    // Reusing the consumed original gesture cannot turn a stream append into
    // another explicit navigation or a return-to-latest command.
    assert!(run(&host, root).claim_turn_navigation(&request).is_err());
    runtime.stop().await.unwrap();
}

// A host-owned fixed pane nests the ORIGINAL Run under another library scroll.
// It changes application layout only; it never maps items or writes offsets.
#[derive(Debug)]
struct NestedRunPane {
    child: Vec<Content>,
    shift: f32,
    clipped: bool,
}
impl Component for NestedRunPane {
    fn children(&self) -> &[Content] {
        &self.child
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.child)
    }
    fn measure(
        &mut self,
        _: &mut dyn ButtonTextRenderer,
        a: Size,
    ) -> tekes_ui_controls::layout::MeasuredChild {
        tekes_ui_controls::layout::MeasuredChild::new(Size::new(a.width, 650.))
    }
    fn arrange(&mut self, t: &mut dyn ButtonTextRenderer, f: Rect, c: Rect) {
        let clip = if self.clipped {
            Rect::default()
        } else {
            f.intersection(&c).unwrap_or_default()
        };
        self.child[0].arrange(t, f.offset(0., self.shift), clip);
    }
}
const OUTER_NAV: ComponentId = ComponentId(60001);
const PANE_NAV: ComponentId = ComponentId(60003);
async fn nested_run() -> (
    std::sync::Arc<tekes_agent_session::SessionRuntime>,
    ContentHost,
    ComponentId,
    ReadingDocumentVersion,
    Text,
) {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.draft.text = "原草稿 中文".into();
    let mut view = RunPort::standard(2100, 2104..2500, RunScope::main("fixture", "a")).unwrap();
    view.update(
        &snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        paging(&view, &snapshot),
        true,
    )
    .unwrap();
    let root = view.ids().root;
    let original = Content::new(root, view);
    let text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let mut children = vec![];
    for i in 0..8 {
        children.push(Content::new(
            ComponentId(60100 + i),
            TextContent::new("Host content preceding original Run"),
        ));
    }
    children.push(Content::new(
        PANE_NAV,
        NestedRunPane {
            child: vec![original],
            shift: 0.,
            clipped: false,
        },
    ));
    children.push(Content::new(
        ComponentId(60200),
        TextContent::new("Host content following original Run"),
    ));
    let list = Content::new(
        ComponentId(60002),
        ListContent::new("Host document", children),
    );
    let mut host = ContentHost::new(Content::new(
        OUTER_NAV,
        ScrollContent::new(Default::default(), list),
    ))
    .unwrap();
    let viewport = run(&host, root).ids().viewport;
    let items = run(&host, root)
        .transcript()
        .unwrap()
        .row_keys()
        .iter()
        .map(|k| run(&host, root).transcript().unwrap().row_id(k).unwrap())
        .collect();
    let (version, ()) = host
        .set_scroll_reading_document(
            viewport,
            "original-nested-run",
            snapshot.revision,
            items,
            |_| {},
        )
        .unwrap();
    (runtime, host, root, version, text)
}
struct MotionText {
    original: Text,
    now: std::time::Duration,
    reduced: bool,
    frames: usize,
}
impl ButtonTextRenderer for MotionText {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> Size {
        self.original.measure(s, r, w)
    }
    fn draw(
        &mut self,
        s: &mut Scene,
        t: &str,
        r: proposal::TextRole,
        f: Rect,
        a: ButtonTextAlign,
        c: Rgba,
    ) {
        self.original.draw(s, t, r, f, a, c)
    }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> {
        Some(&mut self.original)
    }
    fn motion_context(&self) -> Option<tekes_ui_controls::ControlMotionContext> {
        Some(tekes_ui_controls::ControlMotionContext {
            now: self.now,
            reduced_motion: self.reduced,
        })
    }
    fn request_animation_frame(&mut self) {
        self.frames += 1;
    }
}
fn nested_paint(host: &mut ContentHost, text: &mut dyn ButtonTextRenderer) {
    let frame = Rect::new(0., 0., 760., 500.);
    host.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
        .unwrap();
}
fn nested_offsets(host: &ContentHost, root: ComponentId) -> [Point; 2] {
    [
        host.get(OUTER_NAV)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset,
        scroll(host, root).scroll.offset,
    ]
}
#[tokio::test]
async fn original_run_nested_ancestors_reveal_and_clipping_rejection_keep_the_marked_editor() {
    let (runtime, mut host, root, version, mut text) = nested_run().await;
    nested_paint(&mut host, &mut text);
    let editor = run(&host, root).ids().composer.editor;
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "嵌套组字".into(),
        selection: 0..2,
        clauses: vec![],
        replacement: None,
    });
    let before = baseline(&host, root);
    let request = navigation(&host, root, 5);
    let RunTurnNavigationKind::TurnStart(item) =
        tekes_agent_ui_consumer_check::claim_turn_navigation(run(&host, root), &request).unwrap()
    else {
        panic!()
    };
    host.reveal_scroll_reading_item(
        request.viewport(),
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
    nested_paint(&mut host, &mut text);
    assert!(host.get(item).unwrap().visible().is_some());
    let offsets = nested_offsets(&host, root);
    assert!(offsets[0].y > 0. && offsets[1].y > 0.);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(host.focused(), Some(editor));
    host.get_mut(PANE_NAV)
        .unwrap()
        .component_mut::<NestedRunPane>()
        .unwrap()
        .clipped = true;
    nested_paint(&mut host, &mut text);
    let offsets = nested_offsets(&host, root);
    let request = navigation(&host, root, 0);
    let RunTurnNavigationKind::TurnStart(item) =
        run(&host, root).claim_turn_navigation(&request).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        host.reveal_scroll_reading_item(
            request.viewport(),
            &version,
            item,
            ScrollRevealOptions {
                animated: false,
                ..Default::default()
            },
            &mut text
        ),
        Err(ContentRevealError::Clipped)
    );
    assert_eq!(nested_offsets(&host, root), offsets);
    assert_eq!(baseline(&host, root), before);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn real_run_same_extent_host_layout_moves_interrupt_both_directions_then_new_geometry_recovers()
 {
    for shift in [-45., 45.] {
        let (runtime, mut host, root, version, text) = nested_run().await;
        let mut text = MotionText {
            original: text,
            now: std::time::Duration::ZERO,
            reduced: false,
            frames: 0,
        };
        nested_paint(&mut host, &mut text);
        let editor = run(&host, root).ids().composer.editor;
        host.focus(Some(editor), true);
        host.event(&ContentEvent::Preedit {
            text: "布局组字".into(),
            selection: 0..2,
            clauses: vec![],
            replacement: None,
        });
        let before = baseline(&host, root);
        let request = navigation(&host, root, 5);
        let RunTurnNavigationKind::TurnStart(item) =
            run(&host, root).claim_turn_navigation(&request).unwrap()
        else {
            panic!()
        };
        let options = ScrollRevealOptions {
            placement: ScrollRevealPlacement::Centered,
            ..Default::default()
        };
        assert_eq!(
            host.reveal_scroll_reading_item(request.viewport(), &version, item, options, &mut text)
                .unwrap()
                .status,
            ContentRevealStatus::Animating
        );
        text.now = std::time::Duration::from_millis(20);
        nested_paint(&mut host, &mut text);
        let outer_extent = host
            .get(OUTER_NAV)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .document_size();
        host.get_mut(PANE_NAV)
            .unwrap()
            .component_mut::<NestedRunPane>()
            .unwrap()
            .shift = shift;
        text.now = std::time::Duration::from_millis(40);
        nested_paint(&mut host, &mut text);
        assert_eq!(
            host.get(OUTER_NAV)
                .unwrap()
                .component::<ScrollContent>()
                .unwrap()
                .scroll
                .document_size(),
            outer_extent
        );
        assert_eq!(
            host.scroll_reading_reveal_result().unwrap().status,
            ContentRevealStatus::Interrupted
        );
        let stopped = nested_offsets(&host, root);
        text.now = std::time::Duration::from_secs(1);
        nested_paint(&mut host, &mut text);
        assert_eq!(nested_offsets(&host, root), stopped);
        assert_eq!(baseline(&host, root), before);
        let request = navigation(&host, root, 5);
        let RunTurnNavigationKind::TurnStart(item) =
            run(&host, root).claim_turn_navigation(&request).unwrap()
        else {
            panic!()
        };
        host.reveal_scroll_reading_item(request.viewport(), &version, item, options, &mut text)
            .unwrap();
        text.now = std::time::Duration::from_secs(2);
        nested_paint(&mut host, &mut text);
        assert_eq!(
            host.scroll_reading_reveal_result().unwrap().status,
            ContentRevealStatus::Completed
        );
        assert!(host.get(item).unwrap().visible().is_some());
        assert_eq!(baseline(&host, root), before);
        text.reduced = true;
        let request = navigation(&host, root, 1);
        let RunTurnNavigationKind::TurnStart(item) =
            run(&host, root).claim_turn_navigation(&request).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            host.reveal_scroll_reading_item(request.viewport(), &version, item, options, &mut text)
                .unwrap()
                .status,
            ContentRevealStatus::Completed
        );
        assert_eq!(baseline(&host, root), before);
        assert_eq!(host.focused(), Some(editor));
        runtime.stop().await.unwrap();
    }
}
