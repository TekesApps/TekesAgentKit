#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_agent_presentation::RunSnapshot;
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
                    assert!((viewport.min_x() - (frame.min_x() + left)).abs() < 0.01);
                    assert!((viewport.size.width - expected_width).abs() < 0.01);
                    assert!((composer.min_x() - viewport.min_x()).abs() < 0.01);
                    assert!((composer.size.width - viewport.size.width).abs() < 0.01);
                    assert!(composer.max_y() <= frame.max_y() + 0.01);
                    assert!(viewport.max_y() <= composer.min_y());
                    assert_eq!(host.focused(), Some(ids.composer.editor));
                    let input = &host.text_surface(ids.composer.editor).unwrap().input;
                    assert_eq!(input.external_baseline(), baseline);
                    assert_eq!(input.external_value(), value.as_ref());
                    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
                    assert!(run.composer().is_composing());
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
            for id in [ids.viewport, ids.composer.root] {
                let actual = host.get(id).unwrap().frame();
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
