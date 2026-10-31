//! A second product verifies the public RunPort hover rail using the original
//! ContentHost pointer route and the rectangles painted into TekesUI's Scene.
use super::Text;
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
use tekes_ui_scene::{Point, Primitive, Rect, Scene};
use tekes_ui_tokens::Theme;

const RUN_NAMESPACE: u32 = 0x6a10;
const ROW_NAMESPACE: u32 = 0x6b00;
const FRAME: Rect = Rect::new(0., 0., 900., 700.);

fn snapshot(scope: &RunScope, count: usize) -> RunSnapshot {
    let turns: Vec<_> = (0..count)
        .map(|index| {
            serde_json::json!({
                "id": format!("turn-{index}"),
                "status": "completed",
                "input": format!("问题 {index}"),
                "startedAt": null,
                "endedAt": null,
                "usage": { "inputTokens": 0, "outputTokens": 0 }
            })
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "revision": 1,
        "connectionEpoch": 1,
        "selection": {
            "destination": {
                "kind": "run",
                "scope": {
                    "endpointId": scope.endpoint_id,
                    "sessionId": scope.session_id,
                    "threadId": scope.thread_id
                }
            },
            "generation": 1
        },
        "connection": {
            "phase": "connected",
            "canSubmit": true,
            "historyIsStale": false,
            "message": null
        },
        "run": null,
        "transcript": { "turns": turns, "items": [], "tools": [], "hasMoreBefore": false, "isStale": false, "diagnostics": [] },
        "summary": null,
        "goal": null,
        "draft": { "text": "", "artifacts": [], "annotations": [], "settings": {} },
        "draftContextRevision": 1,
        "canSubmit": true,
        "childAvailability": null,
        "submissions": [],
        "events": []
    }))
    .unwrap()
}

fn render(host: &mut ContentHost, text: &mut Text) -> Scene {
    let mut scene = Scene::new();
    host.render(&mut scene, text, FRAME, FRAME, Theme::LIGHT.into())
        .unwrap();
    scene
}

fn run(host: &ContentHost) -> &RunPort {
    host.get(ComponentId((RUN_NAMESPACE as u64) << 32))
        .unwrap()
        .component::<RunPort>()
        .unwrap()
}

fn run_mut(host: &mut ContentHost) -> &mut RunPort {
    host.get_mut(ComponentId((RUN_NAMESPACE as u64) << 32))
        .unwrap()
        .component_mut::<RunPort>()
        .unwrap()
}

fn tick_rect(scene: &Scene, expected: Rect) -> Rect {
    let mut clips = vec![FRAME];
    for primitive in scene.primitives() {
        match primitive {
            Primitive::PushClip(rect) => {
                clips.push(clips.last().unwrap().intersection(rect).unwrap_or_default())
            }
            Primitive::PopClip => {
                clips.pop();
            }
            Primitive::Quad(quad)
                if (quad.rect.min_x() - expected.min_x()).abs() < 0.01
                    && (quad.rect.min_y() - expected.min_y()).abs() < 0.01
                    && (quad.rect.size.height - expected.size.height).abs() < 0.01 =>
            {
                let visible = clips
                    .last()
                    .unwrap()
                    .intersection(&quad.rect)
                    .expect("tick must survive its actual Scene clip");
                assert!((visible.min_x() - quad.rect.min_x()).abs() < 0.001);
                assert!((visible.min_y() - quad.rect.min_y()).abs() < 0.001);
                assert!((visible.size.width - quad.rect.size.width).abs() < 0.001);
                assert!((visible.size.height - quad.rect.size.height).abs() < 0.001);
                return quad.rect;
            }
            _ => {}
        }
    }
    panic!("missing tick {expected:?}");
}

fn expected_tick(host: &ContentHost, index: usize, length: f32) -> Rect {
    let rail = run(host).turn_rail().unwrap();
    let row = host.get(rail.item_ids()[index]).unwrap().frame();
    TurnRailGeometry {
        rail: row,
        track: row,
        document_size: row.size,
        centers_ticks: rail.layout_geometry().unwrap().centers_ticks,
    }
    .tick_rect(0, length)
    .unwrap()
    .offset(row.min_x(), row.min_y())
}

#[test]
fn public_run_rail_paints_hover_ramp_and_restores_it_after_pointer_leave() {
    for scale in [1., 2., 0.5] {
        verify_rail(scale);
    }
}
fn verify_rail(scale: f32) {
    let scope = RunScope::main("public-consumer", "rail-hover");
    let mut view = RunPort::standard(
        RUN_NAMESPACE,
        ROW_NAMESPACE..ROW_NAMESPACE + 90,
        scope.clone(),
    )
    .unwrap();
    view.install_turn_rail(0x6a30).unwrap();
    view.set_rail_navigation_gate(TurnRailNavigationGate {
        turn_start: true,
        latest: true,
    })
    .unwrap();
    let source = snapshot(&scope, 7);
    view.update(
        &source,
        Default::default(),
        ComposerControlGate::OPEN,
        TranscriptPagingFacts {
            scope: scope.clone(),
            supported: false,
            has_more_before: false,
            is_stale: false,
            source_ready: true,
            read: TranscriptPageRead::Idle,
        },
        true,
    )
    .unwrap();
    let root = view.ids().root;
    let editor = view.ids().composer.editor;
    let mut host = view.into_host().unwrap();

    let mut text = Text::new(1., true);
    text.backing_scale = scale;
    let resting_scene = render(&mut host, &mut text);
    let geometry = run(&host).layout_geometry().unwrap().turn_rail;
    let navigation_id = run(&host).turn_rail().unwrap().item_ids()[3];
    let navigation_semantics = host
        .semantics()
        .into_iter()
        .find(|node| node.id == navigation_id)
        .expect("original navigation owner must remain in the semantic tree");
    assert_eq!(navigation_semantics.semantics.role, ContentRole::Button);
    assert_eq!(navigation_semantics.semantics.name, "回合 4 · 问题 3");
    assert_eq!(geometry.document_size.height, 70.);
    assert_eq!(
        run(&host).turn_rail().unwrap().layout_geometry(),
        Some(geometry)
    );
    let first = host
        .get(run(&host).turn_rail().unwrap().item_ids()[0])
        .unwrap()
        .frame();
    assert_eq!(first.size.width, geometry.document_size.width);
    let last = host
        .get(*run(&host).turn_rail().unwrap().item_ids().last().unwrap())
        .unwrap()
        .frame();
    assert!(
        last.max_y() <= geometry.track.max_y(),
        "small document has room for every row"
    );
    for index in 0..7 {
        let resting = tick_rect(&resting_scene, expected_tick(&host, index, 6.));
        assert!((resting.size.width - 6.).abs() < 0.01);
    }

    assert!(host.focus(Some(editor), true));
    let focused = host.focused();
    let hover_row = 3usize;
    let row = host
        .get(run(&host).turn_rail().unwrap().item_ids()[hover_row])
        .unwrap()
        .frame();
    let pointer = row.center();
    let response = host.event(&ContentEvent::PointerMove(pointer));
    assert!(!response.consumed && !response.capture_pointer);
    assert_eq!(host.focused(), focused);

    let hover_scene = render(&mut host, &mut text);
    for (index, expected) in [8., 14., 20., 26., 20., 14., 8.].into_iter().enumerate() {
        let actual = tick_rect(&hover_scene, expected_tick(&host, index, expected));
        assert!((actual.size.width - expected).abs() < 0.01, "tick {index}");
    }

    let mut next = source.clone();
    next.revision += 1;
    run_mut(&mut host)
        .update(
            &next,
            Default::default(),
            ComposerControlGate::OPEN,
            TranscriptPagingFacts {
                scope: scope.clone(),
                supported: false,
                has_more_before: false,
                is_stale: false,
                source_ready: true,
                read: TranscriptPageRead::Idle,
            },
            true,
        )
        .unwrap();
    let scene = render(&mut host, &mut text);
    assert_eq!(
        tick_rect(&scene, expected_tick(&host, hover_row, 26.))
            .size
            .width,
        26.,
        "ordinary publication retains hover on the original mounted controller"
    );
    assert_eq!(host.focused(), focused);

    let target = host
        .get(run(&host).turn_rail().unwrap().item_ids()[hover_row])
        .unwrap()
        .frame();
    host.event(&ContentEvent::PointerDown(target.center()));
    host.event(&ContentEvent::PointerUp(target.center()));
    assert_eq!(host.focused(), focused, "navigation keeps Composer focus");
    let navigation = run_mut(&mut host).take_turn_navigation();
    assert_eq!(
        navigation.len(),
        1,
        "the original button captures one press"
    );
    assert_eq!(navigation[0].target().turn_id(), "turn-3");
    assert_eq!(host.focused(), focused);

    let leave = Point::new(geometry.track.max_x() + 20., geometry.track.max_y() + 20.);
    host.event(&ContentEvent::PointerMove(leave));
    let restored_scene = render(&mut host, &mut text);
    for index in 0..7 {
        let restored = tick_rect(&restored_scene, expected_tick(&host, index, 6.));
        assert!((restored.size.width - 6.).abs() < 0.01);
    }
    assert_eq!(host.focused(), focused);
    assert_eq!(run(&host).turn_rail().unwrap().item_ids().len(), 7);
    assert_eq!(run(&host).ids().root, root);
    text.backing_scale = scale * 1.5;
    let _ = render(&mut host, &mut text);
    assert_eq!(
        run(&host).turn_rail().unwrap().layout_geometry(),
        Some(run(&host).layout_geometry().unwrap().turn_rail)
    );
    let row = host
        .get(run(&host).turn_rail().unwrap().item_ids()[hover_row])
        .unwrap()
        .frame();
    host.event(&ContentEvent::PointerMove(row.center()));
    let scene = render(&mut host, &mut text);
    assert_eq!(
        tick_rect(&scene, expected_tick(&host, hover_row, 26.))
            .size
            .width,
        26.
    );
    assert_eq!(host.focused(), focused);
    let threshold = TurnRailGeometry::minimum_regular_width_at_scale(text.backing_scale);
    for width in [threshold - 0.25, threshold + 0.25, 4.] {
        run_mut(&mut host)
            .set_column_layout(RunColumnLayout::document_pane(
                width / FRAME.size.width,
                Some(0.45),
            ))
            .unwrap();
        let _ = render(&mut host, &mut text);
        let geometry = run(&host).layout_geometry().unwrap().turn_rail;
        assert_eq!(
            run(&host).turn_rail().unwrap().layout_geometry(),
            Some(geometry)
        );
        assert!(geometry.track.min_x() >= geometry.rail.min_x());
        assert!(geometry.track.max_x() <= geometry.rail.max_x() + 0.001);
        if geometry.document_size.width > 0. {
            let row = host
                .get(run(&host).turn_rail().unwrap().item_ids()[hover_row])
                .unwrap()
                .frame();
            host.event(&ContentEvent::PointerMove(row.center()));
            let scene = render(&mut host, &mut text);
            let expected = if geometry.centers_ticks {
                26_f32.min(geometry.document_size.width)
            } else {
                26.
            };
            let ink = tick_rect(&scene, expected_tick(&host, hover_row, expected));
            assert!((ink.size.width - expected).abs() < 0.01);
            assert!(ink.min_x() >= geometry.rail.min_x());
            assert!(ink.max_x() <= geometry.rail.max_x());
        }
    }
}
