#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_agent_presentation::RunSnapshot;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    composition::{Component, ContentEvent, ScrollContent},
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
struct Text(CoreTextShaper, f32);
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for Text {
    fn type_scale(&self) -> f32 {
        self.1
    }
    fn shape_surface(
        &mut self,
        s: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            s,
            &TextStyle::system(role.style().size * self.1),
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, s: &str, r: proposal::TextRole, w: f32) -> Size {
        let p = self.shape_surface(s, r, Some(w));
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

fn update(run: &mut RunView, snapshot: &RunSnapshot) {
    run.update(
        snapshot,
        Default::default(),
        ComposerControlGate::OPEN,
        TranscriptPagingFacts {
            scope: run.scope().clone(),
            supported: true,
            has_more_before: snapshot.transcript.has_more_before,
            is_stale: snapshot.transcript.is_stale,
            source_ready: true,
            read: TranscriptPageRead::Idle,
        },
        true,
    )
    .unwrap();
}
#[tokio::test]
async fn shared_column_bounds_follow_pane_policy_without_replacing_text_or_draft_state() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    snapshot.draft.text = "中文草稿保持原编辑器".into();
    let mut run = RunView::standard(1500, 1504..1800, RunScope::main("fixture", "a")).unwrap();
    update(&mut run, &snapshot);
    let ids = run.ids();
    let transcript = run.transcript_id();
    let rows = run.transcript().unwrap().row_keys().to_vec();
    let mut host = run.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let initial = Rect::new(20., 40., 900., 900.);
    host.render(
        &mut Scene::new(),
        &mut text,
        initial,
        initial,
        Theme::LIGHT.into(),
    )
    .unwrap();
    host.focus(Some(ids.composer.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "继续组字".into(),
        selection: 0..4,
        clauses: vec![],
        replacement: None,
    });
    let baseline = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    let value = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_value()
        .cloned();
    for mode in [
        RunColumnLayout::default(),
        RunColumnLayout::FullWidth,
        RunColumnLayout::Insets {
            leading: 0.18,
            trailing: 0.08,
        },
    ] {
        host.get_mut(ids.root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap()
            .set_column_layout(mode)
            .unwrap();
        for width in [420., 760., 1280.] {
            // Expected pane bounds do not call the private layout helper.
            let gutter = tekes_ui_tokens::ControlSize::FocusBar.value()
                + 3. * tekes_ui_tokens::Spacing::Loose.value();
            let (left, expected_width) = match mode {
                RunColumnLayout::Reading { maximum_width } => {
                    let content_width = (width - 2. * gutter).max(320.).min(maximum_width);
                    (gutter.max((width - content_width) * 0.5), content_width)
                }
                RunColumnLayout::FullWidth => (gutter, width - 2. * gutter),
                RunColumnLayout::Insets { leading, trailing } => {
                    (width * leading, width * (1. - leading - trailing))
                }
            };
            for scale in [1., 1.5] {
                text.1 = scale;
                for theme in [Theme::LIGHT, Theme::DARK] {
                    let frame = Rect::new(20., 40., width, 900.);
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    let viewport = host.get(ids.viewport).unwrap().frame();
                    let composer = host.get(ids.composer.root).unwrap().frame();
                    let geometry = host
                        .get(ids.root)
                        .unwrap()
                        .component::<RunView>()
                        .unwrap()
                        .layout_geometry()
                        .unwrap();
                    assert_eq!(
                        viewport, frame,
                        "document remains the pane's scroll coordinate system"
                    );
                    assert!(
                        (geometry.content_column.min_x() - (frame.min_x() + left)).abs() < 0.01
                    );
                    assert!((geometry.content_column.size.width - expected_width).abs() < 0.01);
                    assert!((composer.min_x() - geometry.content_column.min_x()).abs() < 0.01);
                    assert!(
                        (composer.size.width - geometry.content_column.size.width).abs() < 0.01
                    );
                    assert!(
                        (composer.max_y()
                            - (frame.max_y() - tekes_ui_tokens::Spacing::Regular.value()))
                        .abs()
                            < 0.01
                    );
                    assert!(
                        (geometry.visible_document.max_y()
                            - (composer.min_y() - tekes_ui_tokens::Spacing::Loose.value()))
                        .abs()
                            < 0.01
                    );
                    assert!(
                        viewport.max_y() > composer.min_y(),
                        "document is an underlay, not a docked sibling"
                    );
                    assert_eq!(
                        host.get(ids.viewport).unwrap().visible(),
                        Some(geometry.visible_document)
                    );
                    assert_eq!(host.focused(), Some(ids.composer.editor));
                    let input = &host.text_surface(ids.composer.editor).unwrap().input;
                    assert_eq!(input.external_baseline(), baseline);
                    assert_eq!(input.external_value(), value.as_ref());
                    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
                    assert!(run.composer().is_composing());
                    for row in run.transcript().unwrap().children() {
                        if row.component::<InputMessage>().is_some()
                            || row.component::<OutputMessage>().is_some()
                        {
                            assert!(
                                (row.frame().min_x() - geometry.content_column.min_x()).abs()
                                    < 0.01
                            );
                            assert!(row.frame().max_x() <= geometry.content_column.max_x() + 0.01);
                        }
                    }
                    assert_eq!(run.transcript_id(), transcript);
                    assert_eq!(run.transcript().unwrap().row_keys(), rows);
                }
            }
        }
    }
    runtime.stop().await.unwrap();
}

fn reference_number(value: &serde_json::Value) -> f32 {
    value
        .as_f64()
        .map(|n| n as f32)
        .unwrap_or_else(|| match value.as_str().unwrap() {
            "nan" => f32::NAN,
            "infinity" => f32::INFINITY,
            "negative_infinity" => f32::NEG_INFINITY,
            other => panic!("unexpected reference number {other}"),
        })
}
#[tokio::test]
async fn column_geometry_and_normalization_match_frozen_clientkit_view_vectors() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/clientkit-run-column.json")).unwrap();
    assert_eq!(
        tekes_ui_tokens::ControlSize::FocusBar.value(),
        reference_number(&reference["focus_bar"])
    );
    assert_eq!(
        tekes_ui_tokens::Spacing::Loose.value(),
        reference_number(&reference["loose_spacing"])
    );
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut view = RunView::standard(1900, 1904..2200, RunScope::main("fixture", "a")).unwrap();
    update(&mut view, &controller.snapshot());
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    for case in reference["cases"].as_array().unwrap() {
        let policy = match case["mode"].as_str().unwrap() {
            "reading" => RunColumnLayout::default(),
            "full" => RunColumnLayout::FullWidth,
            "insets" => RunColumnLayout::document_pane(
                reference_number(&case["leading"]),
                (!case["trailing"].is_null()).then(|| reference_number(&case["trailing"])),
            ),
            _ => unreachable!(),
        };
        let viewport = reference_number(&case["viewport"]);
        let run = host
            .get_mut(ids.root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap();
        run.set_column_layout(policy).unwrap();
        let geometry = run.column_geometry(viewport);
        assert!(
            (geometry.leading - reference_number(&case["expected_x"])).abs() < 0.001,
            "{case}"
        );
        assert!(
            (geometry.width - reference_number(&case["expected_width"])).abs() < 0.001,
            "{case}"
        );
        if case["mode"] == "insets" {
            assert_eq!(
                run.column_layout(),
                RunColumnLayout::Insets {
                    leading: reference_number(&case["normalized_leading"]),
                    trailing: reference_number(&case["normalized_trailing"])
                }
            );
        }
        if viewport.is_finite() && viewport > 0. {
            let frame = Rect::new(20., 40., viewport, 900.);
            host.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
            assert_eq!(host.get(ids.viewport).unwrap().frame(), frame);
            {
                let actual = host.get(ids.composer.root).unwrap().frame();
                assert!(
                    (actual.min_x() - (frame.min_x() + reference_number(&case["expected_x"])))
                        .abs()
                        < 0.01,
                    "{case}"
                );
                assert!(
                    (actual.size.width - reference_number(&case["expected_width"])).abs() < 0.01,
                    "{case}"
                );
            }
        }
    }
    runtime.stop().await.unwrap();
}

#[test]
fn view_input_precedence_and_test_host_floor_follow_shared_column() {
    assert_eq!(
        RunColumnLayout::from_view_inputs(true, Some(0.1), None),
        RunColumnLayout::Insets {
            leading: 0.1,
            trailing: 0.1
        }
    );
    assert_eq!(
        RunColumnLayout::from_view_inputs(false, None, Some(0.4)),
        RunColumnLayout::default()
    );
    assert_eq!(
        RunColumnLayout::from_view_inputs(true, None, Some(0.4)),
        RunColumnLayout::FullWidth
    );
    let pane_floor =
        RunColumnLayout::MINIMUM_CONTENT_WIDTH + 2. * RunColumnLayout::horizontal_inset();
    let expected = 180.
        + 2. * pane_floor
        + 2. * tekes_ui_tokens::Spacing::Regular.value()
        + 2. * tekes_agent_ui_demo::component_demo::DEMO_CONTENT_INSET;
    assert_eq!(
        tekes_agent_ui_demo::component_demo::ComponentDemo::minimum_window_width(),
        expected as f64
    );
}

#[tokio::test]
async fn archived_composer_has_no_layout_slot_and_library_blurs_original_preedit() {
    use tekes_agent_presentation::RunPhase;
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    let mut run = RunView::standard(2400, 2404..2600, RunScope::main("fixture", "a")).unwrap();
    update(&mut run, &snapshot);
    let ids = run.ids();
    let mut host = run.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 700., 900.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let viewport_before = host.get(ids.viewport).unwrap().frame();
    let before = host
        .get(ids.root)
        .unwrap()
        .component::<RunView>()
        .unwrap()
        .layout_geometry()
        .unwrap()
        .visible_document;
    let composer_height = host.get(ids.composer.root).unwrap().frame().size.height;
    assert!(host.focus(Some(ids.composer.editor), true));
    host.event(&ContentEvent::Preedit {
        text: "未完成中文组字".into(),
        selection: 7..7,
        clauses: vec![],
        replacement: None,
    });
    let before_editor = host
        .text_surface(ids.composer.editor)
        .unwrap()
        .input
        .external_baseline();
    snapshot.revision += 1;
    snapshot.run.as_mut().unwrap().phase = RunPhase::Archived;
    snapshot.run.as_mut().unwrap().can_submit = false;
    snapshot.can_submit = false;
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap();
    update(run, &snapshot);
    assert!(!run.composer().is_presented());
    assert!(
        !run.composer().is_composing(),
        "hidden preedit must not trap host navigation"
    );
    assert_eq!(
        run.composer().editor().input.external_baseline(),
        before_editor,
        "visibility does not mutate the generic editor"
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focused().is_none());
    assert!(host.focused_text().is_none());
    assert_eq!(host.get(ids.viewport).unwrap().frame(), viewport_before);
    let archived = host
        .get(ids.root)
        .unwrap()
        .component::<RunView>()
        .unwrap()
        .layout_geometry()
        .unwrap()
        .visible_document;
    assert!(
        (archived.size.height
            - before.size.height
            - composer_height
            - tekes_ui_tokens::Spacing::Regular.value()
            - tekes_ui_tokens::Spacing::Loose.value())
        .abs()
            < 0.1
    );
    assert!(!host.semantics().iter().any(|n| n.id == ids.composer.root));
    let run = host
        .get_mut(ids.root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap();
    assert!(
        run.take_intents(&[]).is_empty(),
        "library blur must not synthesize Send/Edit"
    );
    assert_eq!(run.composer().editor().input.text, "未完成中文组字");
    assert!(
        run.composer().editor().input.marked.is_none(),
        "TekesUI blur owns unmark"
    );
    snapshot.revision += 1;
    snapshot.run.as_mut().unwrap().phase = RunPhase::Interrupted;
    snapshot.run.as_mut().unwrap().can_submit = true;
    snapshot.can_submit = true;
    update(run, &snapshot);
    assert!(run.composer().is_presented());
    assert_eq!(run.composer().editor().input.text, "未完成中文组字");
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focused().is_none(), "unarchive never steals focus");
    assert!(host.focus(Some(ids.composer.editor), true));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn collapsed_run_retains_geometry_and_scroll_without_measuring_at_zero_and_never_steals_focus()
 {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut run = RunView::standard(2700, 2704..2900, RunScope::main("fixture", "a")).unwrap();
    update(&mut run, &snapshot);
    let ids = run.ids();
    let editor = run.composer().editor() as *const _;
    let mut host = run.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(20., 40., 760., 650.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let geometry = host
        .get(ids.root)
        .unwrap()
        .component::<RunView>()
        .unwrap()
        .layout_geometry()
        .unwrap();
    assert_eq!(geometry.publication, Some(snapshot.revision));
    assert_eq!(geometry.connection_epoch, Some(snapshot.connection_epoch));
    host.event(&ContentEvent::Scroll {
        position: geometry.visible_document.center(),
        delta: Point::new(0., 400.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focus(Some(ids.composer.editor), true));
    let before: Vec<_> = host
        .get(ids.root)
        .unwrap()
        .component::<RunView>()
        .unwrap()
        .children()
        .iter()
        .map(|node| (node.id(), node.frame()))
        .collect();
    let offset = host
        .get(ids.viewport)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap()
        .scroll
        .offset;
    for collapsed in [Rect::new(20., 40., 0., 650.), Rect::new(20., 40., 760., 0.)] {
        host.render(
            &mut Scene::new(),
            &mut text,
            collapsed,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap();
        let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
        assert!(!run.is_pane_presented());
        assert_eq!(run.layout_geometry(), Some(geometry));
        assert_eq!(
            run.children()
                .iter()
                .map(|node| (node.id(), node.frame()))
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(run.composer().editor() as *const _, editor);
        assert!(host.focused().is_none());
        assert!(host.semantics().is_empty());
        assert_eq!(
            host.get(ids.viewport)
                .unwrap()
                .component::<ScrollContent>()
                .unwrap()
                .scroll
                .offset,
            offset
        );
    }
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    assert!(run.is_pane_presented());
    assert_eq!(run.layout_geometry(), Some(geometry));
    assert_eq!(run.composer().editor() as *const _, editor);
    assert!(host.focused().is_none());
    assert_eq!(
        host.get(ids.viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset,
        offset
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn overlay_clearance_keeps_the_final_row_visible_at_the_library_scroll_limit() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut run = RunView::standard(3000, 3004..3200, RunScope::main("fixture", "a")).unwrap();
    update(&mut run, &controller.snapshot());
    let ids = run.ids();
    let mut host = run.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(20., 40., 760., 650.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let scroll = host
        .get_mut(ids.viewport)
        .unwrap()
        .component_mut::<ScrollContent>()
        .unwrap();
    assert!(scroll.scroll.limit().y > 0.);
    scroll.scroll.scroll_by(0., 1e8); // Test-only public library command, no offset assignment/compensation.
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    let geometry = run.layout_geometry().unwrap();
    let final_row = run
        .transcript()
        .unwrap()
        .children()
        .iter()
        .rev()
        .find(|node| node.frame().size.height > 0.)
        .unwrap();
    assert!(final_row.visible().is_some());
    assert!(
        final_row.frame().max_y() <= geometry.visible_document.max_y() + 0.1,
        "final footnote must rest above the Composer clearance"
    );
    assert!(geometry.visible_document.max_y() < geometry.composer.unwrap().min_y());
    let scroll = host
        .get(ids.viewport)
        .unwrap()
        .component::<ScrollContent>()
        .unwrap();
    assert_eq!(scroll.scroll.offset.y, scroll.scroll.limit().y);
    let offset = scroll.scroll.offset;
    host.event(&ContentEvent::Scroll {
        position: geometry.composer.unwrap().center(),
        delta: Point::new(0., -100.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 2.,
    });
    assert_eq!(
        host.get(ids.viewport)
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .scroll
            .offset,
        offset,
        "wheel over Composer must not leak through the overlay into history"
    );
    runtime.stop().await.unwrap();
}
