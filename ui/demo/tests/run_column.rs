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
            self.1,
            options,
        ))
    }

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
async fn summary_sections_follow_source_spacing_and_collapsed_bodies_leave_no_layout_or_focus_slot()
{
    use tekes_ui_controls::composition::AccessibilityAction;
    use tekes_ui_tokens::Spacing;
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    for scale in [1., 1.5, 2.] {
        for width in [320., 600.] {
            for theme in [Theme::LIGHT, Theme::DARK] {
                let mut view = SummaryView::new(30000, Default::default());
                view.update(&SummaryInput {
                    revision: snapshot.revision,
                    scope: RunScope::main("fixture", "a"),
                    facts: Some(SummaryFacts {
                        scope: RunScope::main("fixture", "a"),
                        tasks: vec![tekes_agent_presentation::AgentTask {
                            id: "summary-task".into(),
                            scope: RunScope::main("fixture", "a"),
                            name: "中文任务".into(),
                            status: "running".into(),
                            kind: "todo".into(),
                        }],
                        subagents: vec![],
                        artifacts: vec![],
                        usage: None,
                    }),
                    host_sections: vec![SummarySection {
                        id: "repository".into(),
                        rows: vec![SummaryRow {
                            id: "branch".into(),
                            title: "main".into(),
                            value: Default::default(),
                            action: Some(SummaryAction::OpenArtifact {
                                scope: RunScope::main("fixture", "a"),
                                path: "/owned/branch".into(),
                            }),
                        }],
                    }],
                })
                .unwrap();
                let root = view.id();
                let first = view.section_header_id("repository").unwrap();
                let row = view.row_id("repository", "branch").unwrap();
                let mut host = view.into_host().unwrap();
                let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
                let frame = Rect::new(20., 40., width, 1000.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
                let layouts = view.section_layouts();
                assert!(layouts.len() >= 2);
                assert_eq!(layouts[0].section_id, "repository");
                for pair in layouts.windows(2) {
                    assert!(
                        (pair[1].header.min_y()
                            - pair[0].body.unwrap().max_y()
                            - Spacing::Section.value())
                        .abs()
                            < 0.1
                    );
                }
                for layout in layouts {
                    assert_eq!(layout.header.max_y(), layout.body.unwrap().min_y());
                    assert!(layout.expanded);
                }
                assert!(host.focus(Some(row), true));
                host.accessibility_action(first, None, AccessibilityAction::Press);
                assert!(
                    host.get(root)
                        .unwrap()
                        .component::<SummaryView>()
                        .unwrap()
                        .section_layouts()
                        .is_empty()
                );
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let view = host.get(root).unwrap().component::<SummaryView>().unwrap();
                let layouts = view.section_layouts();
                assert_eq!(layouts[0].body, None);
                assert!(
                    (layouts[1].header.min_y()
                        - layouts[0].header.max_y()
                        - Spacing::Section.value())
                    .abs()
                        < 0.1
                );
                assert_eq!(host.focused(), Some(first));
                assert!(!host.focus_targets().iter().any(|(id, _)| *id == row));
                assert!(
                    !host
                        .semantics()
                        .iter()
                        .any(|node| node.semantics.name == "main")
                );
            }
        }
    }
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn composer_input_and_primary_share_one_band_without_field_chrome_or_an_empty_toolbar_slot() {
    use tekes_ui_controls::TextSurfacePresentation;
    use tekes_ui_tokens::{Spacing, Stroke};
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    for scale in [1., 1.5, 2.] {
        for theme in [Theme::LIGHT, Theme::DARK] {
            for width in [420., 900.] {
                for long in [false, true] {
                    let mut snapshot = (*controller.snapshot()).clone();
                    snapshot.draft.text = if long {
                        "长中文输入保持正文和操作区分离\n".repeat(25)
                    } else {
                        "短输入".into()
                    };
                    let ids = ComposerIds::new(24000);
                    let mut view = Composer::new(ids);
                    view.update(
                        &snapshot,
                        ComposerAffordances {
                            attachment: true,
                            model: true,
                            preset: true,
                        },
                    )
                    .unwrap();
                    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
                    let measured = view.measure(&mut text, Size::new(width, 1000.));
                    let frame = Rect::new(20., 40., width, measured.size.height);
                    let mut host = view.into_host().unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    let view = host.get(ids.root).unwrap().component::<Composer>().unwrap();
                    let g = view.layout_geometry().unwrap();
                    // The library must see the same capsule subtree as the
                    // user, so padding focus cannot absorb the setup toolbar.
                    let band = host.get(ids.input_band).unwrap();
                    assert_eq!(band.frame(), g.input_band);
                    assert_eq!(g.publication, snapshot.revision);
                    assert_eq!(view.editor().presentation, TextSurfacePresentation::Content);
                    assert!(!view.editor().input.read_only);
                    assert!(
                        g.primary.min_x() >= g.editor.max_x() + Spacing::Regular.value() - 0.1,
                        "scale={scale} width={width} long={long} geometry={g:?}"
                    );
                    assert!(g.primary.max_y() <= g.input_band.max_y() + 0.1);
                    assert!(g.primary.min_y() >= g.input_band.min_y());
                    let add = g.attachment.unwrap();
                    assert!((add.min_x() - g.primary.max_x() - Stroke::Rule.value()).abs() < 0.1);
                    assert!(add.max_x() <= g.input_band.max_x());
                    assert_eq!(add.center().y, g.primary.center().y);
                    let setup = g.setup.unwrap();
                    assert_eq!(setup.min_y(), g.input_band.max_y() + Spacing::Snug.value());
                    for id in [ids.model, ids.preset] {
                        assert!(host.get(id).unwrap().frame().min_y() >= setup.min_y());
                    }
                    assert!(host.get(ids.cancel).is_none());
                    assert!(g.context.is_none());
                    assert!(g.editor.max_y() <= g.input_band.max_y());
                    if long {
                        assert_eq!(
                            view.editor().outer.size.height,
                            view.editor().role.style().line_height * scale * 10.
                        );
                        assert!(view.editor().scroll.limit().y > 0.);
                        assert!(g.primary.center().y > g.input_band.center().y);
                    }
                }
            }
        }
    }
    let snapshot = controller.snapshot();
    let ids = ComposerIds::new(25000);
    let mut view = Composer::new(ids);
    view.update(&snapshot, Default::default()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let size = view.measure(&mut text, Size::new(420., 1000.)).size;
    let frame = Rect::new(0., 0., size.width, size.height);
    let mut host = view.into_host().unwrap();
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let g = host
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .layout_geometry()
        .unwrap();
    assert!(g.setup.is_none());
    assert!(g.attachment.is_none());
    assert_eq!(g.input_band.max_y(), frame.max_y());
    // A bounded host uses library editor scrolling instead of clipping away
    // the primary action when the draft grows past the available pane height.
    let mut snapshot = (*snapshot).clone();
    snapshot.revision += 1;
    snapshot.draft.text = "短窗口中的中文草稿\n".repeat(40);
    host.get_mut(ids.root)
        .unwrap()
        .component_mut::<Composer>()
        .unwrap()
        .update(&snapshot, Default::default())
        .unwrap();
    let short = Rect::new(0., 0., 420., 120.);
    host.focus(Some(ids.editor), true);
    host.render(
        &mut Scene::new(),
        &mut text,
        short,
        short,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let view = host.get(ids.root).unwrap().component::<Composer>().unwrap();
    let g = view.layout_geometry().unwrap();
    assert!(g.primary.min_y() >= short.min_y());
    assert!(g.primary.max_y() <= short.max_y());
    assert!(g.editor.min_y() >= short.min_y());
    assert!(g.editor.max_y() <= short.max_y());
    assert!(view.editor().scroll.limit().y > 0.);
    assert_eq!(host.focused(), Some(ids.editor));
    let ids = ComposerIds::new(26000);
    snapshot.revision += 1;
    snapshot.draft.artifacts = (0..8)
        .map(
            |index| tekes_agent_presentation::ComposerArtifact::Attachment {
                id: format!("card-{index}"),
                name: format!("附件 {index}"),
                content: serde_json::json!({"type":"image","url":"fixture://image"}),
                facts: None,
            },
        )
        .collect();
    let mut view = Composer::new(ids)
        .with_context(27000, Default::default())
        .unwrap();
    view.update(&snapshot, Default::default()).unwrap();
    // ContentHost grows a standalone intrinsic root. RunView is the real
    // bounded pane that assigns a smaller frame to the resident Composer.
    let base = 28000_u64 << 32;
    let mut run = RunView::new(
        RunViewIds {
            root: tekes_ui_controls::composition::ComponentId(base),
            indicators: tekes_ui_controls::composition::ComponentId(base + 1),
            viewport: tekes_ui_controls::composition::ComponentId(base + 2),
            composer: ids,
        },
        RunScope::main("fixture", "a"),
        view,
        27001,
        27002,
    )
    .unwrap();
    update(&mut run, &snapshot);
    let mut host = run.into_host().unwrap();
    host.render(
        &mut Scene::new(),
        &mut text,
        short,
        short,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let g = host
        .get(ids.root)
        .unwrap()
        .component::<Composer>()
        .unwrap()
        .layout_geometry()
        .unwrap();
    assert!(g.context.unwrap().min_y() < short.min_y());
    assert!(g.primary.min_y() >= short.min_y());
    assert!(g.primary.max_y() <= short.max_y());
    assert!(host.get(ids.send).unwrap().visible().is_some());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn rail_visibility_comes_from_arranged_rows_and_never_from_loaded_history() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut snapshot = (*controller.snapshot()).clone();
    for turn in &mut snapshot.transcript.turns {
        turn.input = format!("回合 {}", turn.id);
    }
    assert!(!snapshot.transcript.turns.is_empty());
    let mut run = RunView::standard(1900, 1904..2300, RunScope::main("fixture", "a")).unwrap();
    update(&mut run, &snapshot);
    assert!(
        run.turn_targets()
            .unwrap()
            .iter()
            .all(|target| target.is_visible().is_none())
    );
    let ids = run.ids();
    let mut host = run.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let pane = Rect::new(20., 40., 900., 900.);
    host.render(
        &mut Scene::new(),
        &mut text,
        pane,
        pane,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    let targets = run.turn_targets().unwrap();
    assert!(targets.iter().all(|target| target.is_visible().is_some()));
    for target in &targets {
        let actual_visible = target.row_ids().iter().any(|id| {
            host.get(*id)
                .unwrap()
                .visible()
                .is_some_and(|r| r.size.width > 0. && r.size.height > 0.)
        });
        assert_eq!(target.is_visible(), Some(actual_visible));
        run.validate_turn_target(target).unwrap();
    }
    let geometry = run.layout_geometry().unwrap().turn_rail;
    assert_eq!(
        geometry.document_size.height,
        targets.len() as f32 * TurnRailGeometry::ROW_HEIGHT
    );
    host.focus(Some(ids.composer.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 0..2,
        clauses: vec![],
        replacement: None,
    });
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .validate_turn_target(&targets[0])
            .is_err()
    );
    host.event(&ContentEvent::Unmark);
    let collapsed = Rect::new(20., 40., 0., 900.);
    host.render(
        &mut Scene::new(),
        &mut text,
        collapsed,
        collapsed,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    assert!(
        run.turn_targets()
            .unwrap()
            .iter()
            .all(|target| target.is_visible().is_none())
    );
    assert!(run.validate_turn_target(&targets[0]).is_err());
    host.render(
        &mut Scene::new(),
        &mut text,
        pane,
        pane,
        Theme::LIGHT.into(),
    )
    .unwrap();
    snapshot.revision += 1;
    update(
        host.get_mut(ids.root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap(),
        &snapshot,
    );
    let run = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    assert!(
        run.turn_targets()
            .unwrap()
            .iter()
            .all(|target| target.is_visible().is_none())
    );
    assert!(run.validate_turn_target(&targets[0]).is_err());
    runtime.stop().await.unwrap();
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

#[test]
fn summary_usage_columns_keep_values_on_right_and_long_titles_clear_at_library_scales() {
    use tekes_ui_controls::composition::TextContent;
    use tekes_ui_tokens::Spacing;
    for scale in [0.85, 1., 1.5, 2.] {
        for width in [96., 240., 420.] {
            for theme in [Theme::LIGHT, Theme::DARK] {
                let scope = RunScope::main("native", "usage-layout");
                let mut view = SummaryView::new(
                    30999,
                    SummaryLabels {
                        usage_row: |row| {
                            let mut text = SummaryUsageRowText::read(row);
                            text.title = "中文标题 English 👩🏽‍💻 e\u{301} ".repeat(12);
                            text
                        },
                        ..Default::default()
                    },
                );
                view.update(&SummaryInput {
                    revision: 1,
                    scope: scope.clone(),
                    facts: Some(SummaryFacts {
                        scope,
                        tasks: vec![],
                        subagents: vec![],
                        artifacts: vec![],
                        usage: Some(SummaryUsage {
                            input_tokens: 0,
                            cached_input_tokens: 0,
                            cache_creation_input_tokens: 0,
                            output_tokens: 1234,
                            cost_usd: None,
                            is_cost_complete: false,
                        }),
                    }),
                    host_sections: vec![],
                })
                .unwrap();
                let ids = view.usage_row_ids("output").unwrap();
                let mut host = view.into_host().unwrap();
                let fonts = CoreTextFonts::new();
                let mut text = Text(CoreTextShaper::new(fonts), scale);
                let frame = Rect::new(17., 23., width, 4000.);
                host.render(&mut Scene::default(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let row = host.get(ids.row).unwrap().frame();
                let title = host.get(ids.title).unwrap().frame();
                let value = host.get(ids.value).unwrap().frame();
                assert!((title.min_x() - row.min_x() - Spacing::Regular.value()).abs() < 0.1);
                assert!((value.max_x() - row.max_x() + Spacing::Regular.value()).abs() < 0.1);
                assert!(
                    title.max_x() <= value.min_x(),
                    "overlapping columns: {scale}/{width}"
                );
                let line = text.measure("Mg", proposal::TextRole::UiBody, width).height;
                assert!(value.min_y() >= title.min_y());
                assert!(value.min_y() <= title.min_y() + line);
                assert!(
                    title.size.height > line,
                    "fixture must wrap {scale}/{width}: title={title:?} value={value:?} line={line}"
                );
                assert_eq!(
                    host.get(ids.value)
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .role,
                    proposal::TextRole::UiCaption
                );
                assert!(
                    host.semantics()
                        .iter()
                        .any(|node| node.id == ids.title && node.semantics.name.contains("👩🏽‍💻"))
                );
            }
        }
    }
}

#[tokio::test]
async fn nonfocusing_turn_navigation_keeps_preedit_and_fences_clone_publication_and_pane_aba() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let snapshot = controller.snapshot();
    let mut view = RunView::standard(33000, 33004..33400, RunScope::main("fixture", "a")).unwrap();
    update(&mut view, &snapshot);
    let before = view.turn_targets().unwrap().remove(0);
    assert!(view.prepare_turn_navigation(&before).is_err());
    let ids = view.ids();
    let mut host = view.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 760., 650.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let view = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    let targets = view.turn_targets().unwrap();
    let navigation = view.prepare_turn_navigation(&targets[0]).unwrap();
    assert_eq!(navigation.viewport(), ids.viewport);
    assert_eq!(
        navigation.kind(),
        RunTurnNavigationKind::TurnStart(targets[0].row_ids()[0])
    );
    let clone = navigation.clone();
    let latest = view
        .prepare_turn_navigation(targets.last().unwrap())
        .unwrap();
    assert_eq!(latest.kind(), RunTurnNavigationKind::Latest);
    host.focus(Some(ids.composer.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 0..2,
        clauses: vec![],
        replacement: None,
    });
    let view = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    let baseline = view.composer().editor().input.external_baseline();
    assert!(view.composer().is_composing());
    assert_eq!(
        view.claim_turn_navigation(&navigation).unwrap(),
        navigation.kind()
    );
    assert!(view.claim_turn_navigation(&clone).is_err());
    assert_eq!(
        view.claim_turn_navigation(&latest).unwrap(),
        RunTurnNavigationKind::Latest
    );
    assert!(view.composer().is_composing());
    assert_eq!(view.composer().editor().input.external_baseline(), baseline);
    assert_eq!(host.focused(), Some(ids.composer.editor));
    let pending = view.prepare_turn_navigation(&targets[0]).unwrap();
    let collapsed = Rect::new(0., 0., 0., 650.);
    host.render(
        &mut Scene::new(),
        &mut text,
        collapsed,
        collapsed,
        Theme::LIGHT.into(),
    )
    .unwrap();
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let view = host.get(ids.root).unwrap().component::<RunView>().unwrap();
    assert!(view.claim_turn_navigation(&pending).is_err());
    let targets = view.turn_targets().unwrap();
    let old = view.prepare_turn_navigation(&targets[0]).unwrap();
    let mut next = (*snapshot).clone();
    next.revision += 1;
    host.event(&ContentEvent::Unmark);
    update(
        host.get_mut(ids.root)
            .unwrap()
            .component_mut::<RunView>()
            .unwrap(),
        &next,
    );
    assert!(
        host.get(ids.root)
            .unwrap()
            .component::<RunView>()
            .unwrap()
            .claim_turn_navigation(&old)
            .is_err()
    );
    runtime.stop().await.unwrap();
}

#[test]
fn unsupported_compact_library_never_mounts_an_overlapping_rail_or_replaces_the_composer() {
    let mut view = RunView::standard(33500, 33504..33600, RunScope::main("fixture", "a")).unwrap();
    let original = view.composer().editor() as *const _;
    let result = view.install_turn_rail(34000);
    assert_eq!(result.is_ok(), turn_rail_supported());
    assert_eq!(view.composer().editor() as *const _, original);
    if !turn_rail_supported() {
        assert!(view.turn_rail().is_none());
    }
}

#[tokio::test]
async fn main_history_captures_the_original_display_origin_and_never_replays_a_claimed_or_disabled_gesture()
 {
    #[derive(Default)]
    struct History(Vec<String>);
    impl RunTurnNavigationHost for History {
        fn record_turn_jump(&mut self, jump: &RunTurnJump) {
            self.0.push(format!(
                "record:{}:{}:{}",
                jump.scope().thread_id,
                jump.from_turn().unwrap_or("none"),
                jump.to_turn()
            ));
        }
        fn take_pending_turn_reveal(&mut self, scope: &RunScope) -> Option<String> {
            self.0.push(format!("take:{}", scope.thread_id));
            Some("old-held-reveal".into())
        }
    }
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut view = RunView::standard(34500, 34504..34900, RunScope::main("fixture", "a")).unwrap();
    update(&mut view, &controller.snapshot());
    assert!(
        view.set_turn_navigation_context(Some(RunTurnNavigationContext {
            bottom_is_visible: true
        }))
        .is_err()
    );
    let root = view.ids().root;
    let mut host = view.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 760., 650.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let view = host
        .get_mut(root)
        .unwrap()
        .component_mut::<RunView>()
        .unwrap();
    let targets = view.turn_targets().unwrap();
    view.set_turn_navigation_context(Some(RunTurnNavigationContext {
        bottom_is_visible: true,
    }))
    .unwrap();
    let intent = view.prepare_turn_navigation(&targets[0]).unwrap();
    assert_eq!(
        intent.host_jump().unwrap().from_turn(),
        Some(targets.last().unwrap().turn_id())
    );
    // A later display fact must not rebind the already captured history stop.
    view.set_turn_navigation_context(Some(RunTurnNavigationContext {
        bottom_is_visible: false,
    }))
    .unwrap();
    let mut history = History::default();
    view.claim_turn_navigation_with_host(&intent, &mut history)
        .unwrap();
    assert_eq!(
        history.0,
        vec![
            format!(
                "record:a:{}:{}",
                targets.last().unwrap().turn_id(),
                targets[0].turn_id()
            ),
            "take:a".into()
        ]
    );
    assert!(
        view.claim_turn_navigation_with_host(&intent.clone(), &mut history)
            .is_err()
    );
    let next = view.prepare_turn_navigation(&targets[0]).unwrap();
    let from = targets
        .iter()
        .find(|t| t.is_visible() == Some(true))
        .map(|t| t.turn_id());
    assert_eq!(next.host_jump().unwrap().from_turn(), from);
    view.set_turn_navigation_context(None).unwrap();
    view.set_turn_navigation_context(Some(RunTurnNavigationContext {
        bottom_is_visible: false,
    }))
    .unwrap();
    assert!(
        view.claim_turn_navigation_with_host(&next, &mut history)
            .is_err()
    );
    assert_eq!(history.0.len(), 2);
    view.set_turn_navigation_context(None).unwrap();
    let local = view.prepare_turn_navigation(&targets[0]).unwrap();
    assert!(local.host_jump().is_none());
    assert!(
        view.claim_turn_navigation_with_host(&local, &mut history)
            .is_err()
    );
    view.claim_turn_navigation(&local).unwrap();
    assert_eq!(history.0.len(), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn default_demo_keeps_the_resident_input_visible_across_session_switch_without_transcript_flag()
 {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
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
    let mut view = ComponentDemo::new([a, b]).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    for height in [520., 760.] {
        let frame = Rect::new(
            10.,
            10.,
            ComponentDemo::minimum_window_width() as f32,
            height - 20.,
        );
        for selected in [0, 1, 0] {
            view.select(selected).unwrap();
            view.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let ids = view.ids();
            for id in [ids.editor, ids.send, ids.input_band] {
                let rect = view.tree.get(id).unwrap().frame();
                assert!(
                    rect.min_y() >= frame.min_y() && rect.max_y() <= frame.max_y(),
                    "height={height} selected={selected} id={id:?} rect={rect:?}"
                );
                assert!(rect.size.height > 0.);
            }
        }
    }
    runtime.stop().await.unwrap();
}

#[test]
fn composer_notice_wraps_with_original_refresh_control_and_collapses_without_a_gap() {
    use tekes_ui_controls::composition::{Content, ContentHost};
    for scale in [0.85, 1., 1.5] {
        for width in [160., 320., 600.] {
            for theme in [Theme::LIGHT, Theme::DARK] {
                let mut notice = ComposerNotice::new(39998, Default::default());
                let mut input = ComposerNoticeInput {
                    connection_epoch: 1,
                    revision: 1,
                    destination: ComposerDestination::run(RunScope::main(
                        "fixture",
                        "notice-layout",
                    )),
                    enabled: true,
                    notice: Some(ComposerNoticeState {
                        text: "目录快照暂不可用，保持当前输入并显式刷新。".repeat(3),
                        can_refresh: true,
                        refreshing: false,
                    }),
                };
                notice.update(&input).unwrap();
                let root = notice.id();
                let label = notice.text_id();
                let refresh = notice.refresh_id();
                let mut host = ContentHost::new(Content::new(root, notice)).unwrap();
                let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
                let size = host
                    .get_mut(root)
                    .unwrap()
                    .measure(&mut text, Size::new(width, 2000.))
                    .size;
                let frame = Rect::new(17., 23., width, size.height);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let label_frame = host.get(label).unwrap().frame();
                let button = host.get(refresh).unwrap().frame();
                assert!(label_frame.max_x() <= button.min_x());
                assert!(button.max_x() <= frame.max_x());
                assert!(
                    label_frame.min_x()
                        >= frame.min_x() + tekes_ui_tokens::Spacing::Relaxed.value()
                );
                let line = text
                    .measure("Mg", proposal::TextRole::UiCaption, width)
                    .height;
                assert!(
                    label_frame.size.height > line,
                    "notice should wrap {scale}/{width}"
                );
                input.revision += 1;
                input.notice.as_mut().unwrap().can_refresh = false;
                host.get_mut(root)
                    .unwrap()
                    .component_mut::<ComposerNotice>()
                    .unwrap()
                    .update(&input)
                    .unwrap();
                assert!(host.get(refresh).is_none());
                input.revision += 1;
                input.notice = None;
                host.get_mut(root)
                    .unwrap()
                    .component_mut::<ComposerNotice>()
                    .unwrap()
                    .update(&input)
                    .unwrap();
                assert_eq!(
                    host.get_mut(root)
                        .unwrap()
                        .measure(&mut text, Size::new(width, 2000.))
                        .size
                        .height,
                    0.
                );
            }
        }
    }
}
