//! Independent host of the public Summary + SDK label contract. CoreText
//! geometry evidence; native/GPU and physical IME are separate acceptance.
#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer, composition::*,
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    LabelLayout, LabelOptions, LabelOverflow, Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Rgba, Theme, proposal};

const TITLE: &str = "工作区 Summary 👩🏽‍💻 e\u{301} · repository/非常长的分支名称与完整状态标题 ";

#[path = "support/bounded_details.rs"]
mod bounded_details;
#[path = "support/input_aside_layout.rs"]
mod input_aside_layout;
#[path = "support/key_intake.rs"]
mod key_intake;
#[path = "support/output_aside_layout.rs"]
mod output_aside_layout;
#[path = "support/panel_launcher_layout.rs"]
mod panel_launcher_layout;
#[path = "support/panel_navigation_layout.rs"]
mod panel_navigation_layout;
#[path = "support/popover_intake.rs"]
mod popover_intake;
#[path = "support/toolbar_intake.rs"]
mod toolbar_intake;
struct Text {
    shaper: CoreTextShaper,
    scale: f32,
    labels: Vec<(LabelOptions, Rc<LabelLayout>)>,
    label_service: bool,
    drawn_paragraphs: Vec<(usize, Rgba)>,
}
impl Text {
    fn new(scale: f32, label_service: bool) -> Self {
        Self {
            shaper: CoreTextShaper::new(CoreTextFonts::new()),
            scale,
            labels: vec![],
            label_service,
            drawn_paragraphs: vec![],
        }
    }
    fn style(&self, role: proposal::TextRole) -> TextStyle {
        let style = role.style();
        TextStyle::system(style.size * self.scale).with_weight(style.weight)
    }
    fn label_ink(&self, source: &str) -> Vec<Rgba> {
        self.labels
            .iter()
            .filter(|(_, label)| label.source() == source)
            .flat_map(|(_, label)| {
                let pointer = label.paragraph() as *const _ as usize;
                self.drawn_paragraphs
                    .iter()
                    .filter_map(move |(drawn, color)| (*drawn == pointer).then_some(*color))
            })
            .collect()
    }
}
impl TextSurfaceTextRenderer for Text {
    fn push_visible(
        &mut self,
        _: &mut Scene,
        paragraph: &Paragraph,
        _: Point,
        color: Rgba,
        _: Rect,
    ) {
        self.drawn_paragraphs
            .push((paragraph as *const _ as usize, color));
    }
}
impl TextSurfaceHost for Text {
    fn type_scale(&self) -> f32 {
        self.scale
    }
    fn shape_surface(
        &mut self,
        source: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        let style = self.style(role);
        Rc::new(self.shaper.shape(
            source,
            &style,
            &ShapeOptions {
                width: width.map(f64::from),
                line_spacing: 0.,
            },
        ))
    }
    fn shape_label(
        &mut self,
        source: &str,
        role: proposal::TextRole,
        width: f32,
        options: LabelOptions,
    ) -> Option<Rc<LabelLayout>> {
        if !self.label_service {
            return None;
        }
        let style = self.style(role);
        let label = Rc::new(self.shaper.shape_label(
            source,
            &style,
            &ShapeOptions {
                width: Some(width.into()),
                line_spacing: 0.,
            },
            options,
        ));
        self.labels.push((options, label.clone()));
        Some(label)
    }
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, source: &str, role: proposal::TextRole, width: f32) -> Size {
        let p = self.shape_surface(source, role, Some(width));
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
fn input() -> SummaryContextInput {
    SummaryContextInput {
        revision: 1,
        owner: SummaryOwner::Workspace {
            id: "consumer-workspace".into(),
        },
        facts: None,
        ready: true,
        custom_rows: vec![],
        host_sections: vec![SummarySection {
            id: "host:repository".into(),
            rows: vec![SummaryRow {
                id: "branch".into(),
                title: "分支".into(),
                value: Default::default(),
                action: None,
            }],
        }],
    }
}
fn mounted() -> (ContentHost, ComponentId, ComponentId, ComponentId) {
    let mut summary = SummaryView::new(
        49000,
        SummaryLabels {
            section: |_| TITLE.repeat(6),
            ..Default::default()
        },
    );
    tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &input()).unwrap();
    let root = summary.id();
    let header = summary.section_header_id("host:repository").unwrap();
    let label = summary.section_header_label_id("host:repository").unwrap();
    (summary.into_host().unwrap(), root, header, label)
}
#[test]
fn original_summary_label_ellipsizes_one_line_without_losing_source_or_ax_owner() {
    for theme in [Theme::LIGHT, Theme::DARK] {
        for scale in [0.85, 1., 1.5] {
            let (mut host, root, header, label) = mounted();
            let mut text = Text::new(scale, true);
            let mut previous_width = 0.;
            for width in [96., 240., 420.] {
                text.labels.clear();
                let frame = Rect::new(0., 0., width, 600.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let (_, layout) = text
                    .labels
                    .iter()
                    .rev()
                    .find(|(_, layout)| layout.source() == TITLE.repeat(6))
                    .expect("original SDK requested the header label");
                assert!(text.labels.iter().all(|(options, layout)| {
                    if layout.source().is_empty() {
                        *options == LabelOptions::single_line(LabelOverflow::Clip)
                    } else {
                        *options == LabelOptions::single_line(LabelOverflow::TailEllipsis)
                            && layout.paragraph().lines.len() == 1
                            && layout.source() == TITLE.repeat(6)
                    }
                }));
                assert!(!layout.elided_utf16().is_empty());
                assert!(layout.width() <= f64::from(width));
                assert!(layout.width() > previous_width);
                previous_width = layout.width();
                assert_eq!(
                    host.get(label)
                        .unwrap()
                        .component::<TextContent>()
                        .unwrap()
                        .text,
                    TITLE.repeat(6)
                );
                let ax = host.semantics();
                assert_eq!(
                    ax.iter()
                        .filter(|node| node.semantics.name == TITLE.repeat(6))
                        .count(),
                    1
                );
                assert_eq!(
                    ax.iter()
                        .find(|node| node.id == header)
                        .unwrap()
                        .semantics
                        .name,
                    TITLE.repeat(6)
                );
                assert!(!ax.iter().any(|node| node.id == label));
                let mut next = input();
                next.revision += 1;
                let summary = host
                    .get_mut(root)
                    .unwrap()
                    .component_mut::<SummaryView>()
                    .unwrap();
                tekes_agent_ui_consumer_check::publish_summary_context(summary, &next).unwrap();
                assert_eq!(summary.section_header_id("host:repository"), Some(header));
                assert_eq!(
                    summary.section_header_label_id("host:repository"),
                    Some(label)
                );
            }
        }
    }
}
#[test]
fn missing_label_capability_is_explicit_and_never_silently_wraps() {
    let (mut host, _, _, label) = mounted();
    let frame = Rect::new(0., 0., 160., 600.);
    let error = host
        .render(
            &mut Scene::new(),
            &mut Text::new(1., false),
            frame,
            frame,
            Theme::LIGHT.into(),
        )
        .unwrap_err();
    assert!(matches!(error, CompositionError::MissingLabelServices(id) if id == label));
}

#[test]
fn summary_disclosure_and_domain_actions_keep_the_original_composer_focus_and_preedit() {
    use tekes_ui_controls::{
        ControlKey,
        extended::{Axis, Stack},
    };
    let scope = RunScope::main("consumer", "focus");
    let mut summary = SummaryView::new(49002, Default::default());
    summary
        .update(&SummaryInput {
            revision: 1,
            scope: scope.clone(),
            facts: None,
            host_sections: vec![SummarySection {
                id: "repository".into(),
                rows: vec![
                    SummaryRow {
                        id: "open".into(),
                        title: "原产物动作".into(),
                        value: Default::default(),
                        action: Some(SummaryAction::OpenArtifact {
                            scope: scope.clone(),
                            path: "artifact.txt".into(),
                        }),
                    },
                    SummaryRow {
                        id: "static".into(),
                        title: "可选择正文".into(),
                        value: Default::default(),
                        action: None,
                    },
                ],
            }],
        })
        .unwrap();
    let root = summary.id();
    let header = summary.section_header_id("repository").unwrap();
    let action = summary.action_id("repository", "open").unwrap();
    assert!(
        !summary.is_nonfocusing_disclosure_target(header),
        "no unlaid-out hit target"
    );
    let composer = tekes_agent_ui_consumer_check::new_composer(49003);
    let editor = composer.ids().editor;
    let mut host = ContentHost::new(Content::new(
        ComponentId(49),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 8.,
            },
            flexible_child: None,
            children: vec![
                Content::new(composer.ids().root, composer),
                Content::new(root, summary),
                Content::new(ComponentId(50), ButtonContent::new("下一控件")),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let frame = Rect::new(0., 0., 600., 1200.);
    let mut text = Text::new(1., true);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focus(Some(editor), true));
    host.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    });
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let original_editor = host.text_surface(editor).unwrap() as *const _ as usize;
    let mut intents = vec![];
    for id in [action, header] {
        let p = host.get(id).unwrap().visible().unwrap().center();
        assert!(host.event(&ContentEvent::PointerDown(p)).consumed);
        assert!(host.event(&ContentEvent::PointerUp(p)).activated);
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(
            host.text_surface(editor).unwrap() as *const _ as usize,
            original_editor
        );
        assert_eq!(
            host.text_surface(editor).unwrap().input.external_baseline(),
            baseline
        );
        assert!(host.text_surface(editor).unwrap().input.marked.is_some());
        let records = host.drain_actions();
        intents.extend(
            host.get(root)
                .unwrap()
                .component::<SummaryView>()
                .unwrap()
                .take_intents(&records),
        );
    }
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].scope, scope);
    assert!(
        matches!(&intents[0].action, SummaryAction::OpenArtifact{path,..} if path=="artifact.txt")
    );
    assert!(
        !host
            .accessibility_action(action, None, AccessibilityAction::Press)
            .activated,
        "closed action subtree rejects activation"
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(
        host.accessibility_action(header, None, AccessibilityAction::Press)
            .activated
    );
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(
        host.accessibility_action(action, None, AccessibilityAction::Press)
            .activated
    );
    for id in [header, action] {
        assert!(!host.focus(Some(id), true));
        host.accessibility_action(id, None, AccessibilityAction::Focus);
        assert_eq!(host.focused(), Some(editor));
        assert!(!host.focus_targets().iter().any(|(target, _)| *target == id));
    }
    assert_eq!(
        host.text_surface(editor).unwrap().input.external_baseline(),
        baseline
    );
    host.event(&ContentEvent::Unmark);
    host.event(&ContentEvent::Key {
        key: ControlKey::Tab { reverse: false },
        down: true,
    });
    assert!(![Some(header), Some(action)].contains(&host.focused()));
    assert!(
        !host
            .get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .is_nonfocusing_disclosure_target(action)
    );
}

#[test]
fn summary_focus_policy_leaves_original_host_controls_and_static_text_selectable() {
    let mut source = input();
    source.host_sections[0].rows.push(SummaryRow {
        id: "static".into(),
        title: "静态正文".into(),
        value: Default::default(),
        action: None,
    });
    source.custom_rows = vec![SummaryCustomRowSpec {
        section: "host:repository".into(),
        row: "branch".into(),
        content_key: "original-host-button".into(),
    }];
    let mut summary = SummaryView::new(49004, Default::default());
    tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &source).unwrap();
    let root = summary.id();
    let header = summary.section_header_id("host:repository").unwrap();
    let passive = summary
        .passive_text_id("host:repository", "static")
        .unwrap();
    let token = summary
        .custom_row_token("host:repository", "branch")
        .unwrap();
    let button = ComponentId(51);
    let mut host = summary.into_host().unwrap();
    tekes_agent_ui_consumer_check::install_summary_host_content(
        &mut host,
        root,
        &token,
        Content::new(button, ButtonContent::new("宿主自己的焦点策略")),
    )
    .unwrap();
    let frame = Rect::new(0., 0., 400., 600.);
    host.render(
        &mut Scene::new(),
        &mut Text::new(1., true),
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    assert!(host.focus(Some(button), true));
    assert!(!host.focus(Some(header), true));
    assert_eq!(host.focused(), Some(button));
    assert!(
        host.accessibility_action(button, None, AccessibilityAction::Press)
            .activated
    );
    assert!(host.focus_targets().iter().any(|(id, _)| *id == button));
    assert!(!host.focus_targets().iter().any(|(id, _)| *id == header));
    assert!(host.focus(Some(passive), true));
    host.accessibility_action(passive, None, AccessibilityAction::SetSelection(0..2));
    assert_eq!(
        host.text_surface(passive).unwrap().input.selected_text(),
        "静态"
    );
}

#[test]
fn default_summary_rows_use_source_bands_and_keep_original_text_action_and_selection() {
    use tekes_ui_tokens::Spacing;
    for theme in [Theme::LIGHT, Theme::DARK] {
        for scale in [0.85, 1., 1.5, 2.] {
            for width in [180., 320., 600.] {
                let scope = RunScope::main("consumer", "row-layout");
                let title = "中文标题 English 👩🏽‍💻 e\u{301} 长路径/".repeat(8);
                let mut input = SummaryContextInput {
                    revision: 1,
                    owner: SummaryOwner::Run(scope.clone()),
                    facts: None,
                    ready: true,
                    custom_rows: vec![],
                    host_sections: vec![SummarySection {
                        id: "repository".into(),
                        rows: vec![
                            SummaryRow {
                                id: "open".into(),
                                title: title.clone(),
                                value: Default::default(),
                                action: Some(SummaryAction::OpenArtifact {
                                    scope,
                                    path: "file.txt".into(),
                                }),
                            },
                            SummaryRow {
                                id: "text".into(),
                                title: title.clone(),
                                value: Default::default(),
                                action: None,
                            },
                        ],
                    }],
                };
                let mut summary = SummaryView::new(49006, Default::default());
                tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &input)
                    .unwrap();
                let root = summary.id();
                let action = summary.default_row_ids("repository", "open").unwrap();
                let passive = summary.default_row_ids("repository", "text").unwrap();
                assert_eq!(
                    summary.passive_text_id("repository", "text"),
                    Some(passive.title)
                );
                assert_eq!(summary.row_id("repository", "text"), Some(passive.title));
                let mut host = summary.into_host().unwrap();
                let mut text = Text::new(scale, true);
                let frame = Rect::new(0., 0., width, 10000.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let header_label = host
                    .get(root)
                    .unwrap()
                    .component::<SummaryView>()
                    .unwrap()
                    .section_header_label_id("repository")
                    .unwrap();
                assert!(
                    (host.get(header_label).unwrap().frame().min_x()
                        - host.get(action.title).unwrap().frame().min_x())
                    .abs()
                        < 0.1
                );
                for ids in [action, passive] {
                    let band = host.get(ids.row).unwrap().frame();
                    let label = host.get(ids.title).unwrap().frame();
                    assert!(
                        (label.min_x() - band.min_x() - 2. * Spacing::Regular.value()).abs() < 0.1
                    );
                    assert!(
                        (label.size.width - (band.size.width - 4. * Spacing::Regular.value()))
                            .abs()
                            < 0.1
                    );
                    assert!(
                        (band.size.height - label.size.height - 2. * Spacing::Regular.value())
                            .abs()
                            < 0.1
                    );
                    assert!((label.min_y() - band.min_y() - Spacing::Regular.value()).abs() < 0.1);
                    assert!(
                        label.size.height
                            > text.measure("Mg", proposal::TextRole::UiBody, 1e8).height,
                        "title must wrap"
                    );
                }
                assert_eq!(
                    host.semantics()
                        .iter()
                        .filter(|n| n.id == action.title)
                        .count(),
                    0,
                    "action title stays decorative"
                );
                let control = host
                    .get(action.row)
                    .unwrap()
                    .component::<SummaryActionRow>()
                    .unwrap()
                    .button() as *const _ as usize;
                let field = host.text_surface(passive.title).unwrap() as *const _ as usize;
                host.focus(Some(passive.title), true);
                host.accessibility_action(
                    passive.title,
                    None,
                    AccessibilityAction::SetSelection(0..2),
                );
                input.revision += 1;
                input.host_sections[0].rows[0].title.push_str(" · 更新");
                tekes_agent_ui_consumer_check::publish_summary_context(
                    host.get_mut(root)
                        .unwrap()
                        .component_mut::<SummaryView>()
                        .unwrap(),
                    &input,
                )
                .unwrap();
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                assert_eq!(
                    host.get(action.row)
                        .unwrap()
                        .component::<SummaryActionRow>()
                        .unwrap()
                        .button() as *const _ as usize,
                    control
                );
                assert_eq!(
                    host.text_surface(passive.title).unwrap() as *const _ as usize,
                    field
                );
                assert_eq!(
                    host.text_surface(passive.title)
                        .unwrap()
                        .input
                        .selected_text(),
                    "中文"
                );
                assert_eq!(host.focused(), Some(passive.title));
                assert!(
                    host.accessibility_action(action.row, None, AccessibilityAction::Press)
                        .activated
                );
                assert_eq!(host.focused(), Some(passive.title));
            }
        }
    }
}

#[test]
fn usage_metadata_stays_on_one_original_sdk_line_at_the_title_first_line() {
    use tekes_ui_tokens::Spacing;
    for scale in [0.85, 1., 1.5, 2.] {
        for width in [96., 240., 420.] {
            let scope = RunScope::main("consumer", "metadata");
            let mut summary = SummaryView::new(
                49007,
                SummaryLabels {
                    usage_row: |_| SummaryUsageRowText {
                        title: "很长的中文标题 English ".repeat(4),
                        value: "123,456,789,012,345,678,901,234,567,890".into(),
                    },
                    ..Default::default()
                },
            );
            summary
                .update(&SummaryInput {
                    revision: 1,
                    scope: scope.clone(),
                    host_sections: vec![],
                    facts: Some(SummaryFacts {
                        scope,
                        tasks: vec![],
                        subagents: vec![],
                        artifacts: vec![],
                        usage: Some(SummaryUsage {
                            input_tokens: 0,
                            cached_input_tokens: 0,
                            cache_creation_input_tokens: 0,
                            output_tokens: 0,
                            cost_usd: None,
                            is_cost_complete: true,
                        }),
                    }),
                })
                .unwrap();
            let ids = summary.usage_row_ids("total").unwrap();
            let mut host = summary.into_host().unwrap();
            let mut text = Text::new(scale, true);
            let frame = Rect::new(0., 0., width, 10000.);
            host.render(
                &mut Scene::new(),
                &mut text,
                frame,
                frame,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let band = host.get(ids.row).unwrap().frame();
            let title = host.get(ids.title).unwrap().frame();
            let value = host.get(ids.value).unwrap().frame();
            assert!((title.min_x() - band.min_x() - 2. * Spacing::Regular.value()).abs() < 0.1);
            assert!((value.max_x() - band.max_x() + 2. * Spacing::Regular.value()).abs() < 0.1);
            let line = text.measure("Mg", proposal::TextRole::UiBody, 1e8).height;
            assert!((value.min_y() - title.min_y() - (line - value.size.height) / 2.).abs() < 0.1);
            let labels: Vec<_> = text
                .labels
                .iter()
                .filter(|(_, layout)| layout.source() == "123,456,789,012,345,678,901,234,567,890")
                .collect();
            assert!(!labels.is_empty());
            assert!(labels.iter().all(|(policy, layout)| *policy
                == LabelOptions::single_line(LabelOverflow::Clip)
                && layout.paragraph().lines.len() == 1));
            assert_eq!(
                host.get(ids.value)
                    .unwrap()
                    .component::<TextContent>()
                    .unwrap()
                    .text,
                "123,456,789,012,345,678,901,234,567,890"
            );
        }
    }
}

#[test]
fn ordinary_metadata_uses_original_single_line_and_keeps_selection_actions_and_slots() {
    use tekes_ui_tokens::Spacing;
    let scope = RunScope::main("consumer", "metadata");
    for theme in Theme::ALL {
        for scale in [0.85, 1., 1.5, 2.] {
            for width in [96., 240., 420.] {
                let mut input = input();
                input.owner = SummaryOwner::Run(scope.clone());
                input.host_sections[0].rows = ["action", "passive"]
                    .map(|id| SummaryRow {
                        id: id.into(),
                        title: "中文标题 English e\u{301} · 长内容换行 ".repeat(7),
                        value: "12 files".into(),
                        action: (id == "action").then(|| SummaryAction::OpenArtifact {
                            scope: scope.clone(),
                            path: "/metadata/path".into(),
                        }),
                    })
                    .into();
                let mut summary = SummaryView::new(49901, SummaryLabels::default());
                tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &input)
                    .unwrap();
                let root = summary.id();
                let action = summary
                    .default_row_ids("host:repository", "action")
                    .unwrap();
                let passive = summary
                    .default_row_ids("host:repository", "passive")
                    .unwrap();
                let mut host = summary.into_host().unwrap();
                let mut text = Text::new(scale, true);
                let frame = Rect::new(0., 0., width, 10000.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                for ids in [action, passive] {
                    let title = host.get(ids.title).unwrap().frame();
                    let meta = host.get(ids.trailing).unwrap().frame();
                    let band = host.get(ids.row).unwrap().frame();
                    let line = text.measure("Mg", proposal::TextRole::UiBody, 1e8).height;
                    assert!(
                        (meta.max_x() - band.max_x() + 2. * Spacing::Regular.value()).abs() < 0.1
                    );
                    assert!(
                        (meta.min_y() - title.min_y() - (line - meta.size.height) / 2.).abs() < 0.1
                    );
                    assert!(title.max_x() <= meta.min_x());
                    assert_eq!(
                        host.get(ids.trailing)
                            .unwrap()
                            .component::<TextContent>()
                            .unwrap()
                            .text,
                        "12 files"
                    );
                }
                assert!(text.labels.iter().any(|(o, l)| *o
                    == LabelOptions::single_line(LabelOverflow::Clip)
                    && l.source() == "12 files"
                    && l.paragraph().lines.len() == 1));
                let metadata_ink = text.label_ink("12 files");
                assert_eq!(
                    metadata_ink.len(),
                    2,
                    "both original metadata labels must actually paint"
                );
                assert!(
                    metadata_ink
                        .iter()
                        .all(|color| *color
                            == tekes_ui_tokens::ColorRole::SecondaryText.resolve(theme)),
                    "metadata keeps its secondary role inside an actionable row: {metadata_ink:?}"
                );
                let metadata_owner = host
                    .get(action.trailing)
                    .unwrap()
                    .component::<TextContent>()
                    .unwrap() as *const _ as usize;
                for live_theme in [Theme::DARK, Theme::LIGHT_INCREASED] {
                    text.drawn_paragraphs.clear();
                    host.render(
                        &mut Scene::new(),
                        &mut text,
                        frame,
                        frame,
                        live_theme.into(),
                    )
                    .unwrap();
                    let ink = text.label_ink("12 files");
                    assert_eq!(ink.len(), 2);
                    assert!(
                        ink.iter().all(|color| *color
                            == tekes_ui_tokens::ColorRole::SecondaryText.resolve(live_theme)),
                        "library theme changes update ink without a new business publication"
                    );
                    assert_eq!(
                        host.get(action.trailing)
                            .unwrap()
                            .component::<TextContent>()
                            .unwrap() as *const _ as usize,
                        metadata_owner
                    );
                }
                let control = host
                    .get(action.row)
                    .unwrap()
                    .component::<SummaryActionRow>()
                    .unwrap()
                    .button() as *const _ as usize;
                let field = host.text_surface(passive.title).unwrap() as *const _ as usize;
                assert!(host.focus(Some(passive.title), true));
                host.accessibility_action(
                    passive.title,
                    None,
                    AccessibilityAction::SetSelection(0..2),
                );
                for value in [None, Some("更新值 123456789012345678901234567890")] {
                    input.revision += 1;
                    for row in &mut input.host_sections[0].rows {
                        row.value = value.map(Into::into).unwrap_or_default();
                    }
                    tekes_agent_ui_consumer_check::publish_summary_context(
                        host.get_mut(root)
                            .unwrap()
                            .component_mut::<SummaryView>()
                            .unwrap(),
                        &input,
                    )
                    .unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    assert_eq!(
                        host.get(action.row)
                            .unwrap()
                            .component::<SummaryActionRow>()
                            .unwrap()
                            .button() as *const _ as usize,
                        control
                    );
                    assert_eq!(
                        host.text_surface(passive.title).unwrap() as *const _ as usize,
                        field
                    );
                    assert_eq!(
                        host.text_surface(passive.title)
                            .unwrap()
                            .input
                            .selected_text(),
                        "中文"
                    );
                    assert_eq!(host.focused(), Some(passive.title));
                    assert_eq!(
                        host.get(action.trailing).unwrap().visible().is_some(),
                        value.is_some()
                    );
                    assert_eq!(
                        host.get(root)
                            .unwrap()
                            .component::<SummaryView>()
                            .unwrap()
                            .default_row_ids("host:repository", "action"),
                        Some(action)
                    );
                    assert!(
                        host.accessibility_action(action.row, None, AccessibilityAction::Press)
                            .activated
                    );
                    assert_eq!(host.focused(), Some(passive.title));
                }
            }
        }
    }
}

#[test]
fn artifact_metadata_formatter_captures_host_counts_without_replacing_original_rows() {
    use std::sync::{Arc, RwLock};
    let counts = Arc::new(RwLock::new(String::from("+12 -3")));
    let projection = counts.clone();
    let mut summary = SummaryView::new(
        49902,
        SummaryLabels {
            trailing: Arc::new(move |section, row| {
                (section == "artifacts" && row.id == "artifact")
                    .then(|| projection.read().unwrap().clone())
            }),
            ..Default::default()
        },
    );
    let mut source = input();
    source.host_sections[0].id = "artifacts".into();
    source.host_sections[0].rows[0].id = "artifact".into();
    source.host_sections[0].rows[0].title = "报告.md".into();
    tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &source).unwrap();
    let ids = summary.default_row_ids("artifacts", "artifact").unwrap();
    let root = summary.id();
    let mut host = summary.into_host().unwrap();
    assert_eq!(
        host.get(ids.trailing)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "+12 -3"
    );
    *counts.write().unwrap() = "+20 -5".into();
    assert_eq!(
        tekes_agent_ui_consumer_check::publish_summary_context(
            host.get_mut(root)
                .unwrap()
                .component_mut::<SummaryView>()
                .unwrap(),
            &source
        ),
        Err("Summary formatted content changed without publication")
    );
    assert_eq!(
        host.get(ids.trailing)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "+12 -3"
    );
    source.revision += 1;
    tekes_agent_ui_consumer_check::publish_summary_context(
        host.get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap(),
        &source,
    )
    .unwrap();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<SummaryView>()
            .unwrap()
            .default_row_ids("artifacts", "artifact"),
        Some(ids)
    );
    assert_eq!(
        host.get(ids.trailing)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "+20 -5"
    );
    assert_eq!(host.text_surface(ids.title).unwrap().input.text, "报告.md");
}

#[test]
fn repository_divider_follows_display_order_through_collapse_reorder_and_owner_change() {
    use tekes_ui_tokens::Spacing;
    let rule = proposal::metric("stroke", "stroke-rule").unwrap();
    for theme in Theme::ALL {
        for scale in [0.85, 1., 1.5] {
            for width in [180., 420.] {
                let mut source = input();
                let section = |id: &str| SummarySection {
                    id: id.into(),
                    rows: vec![SummaryRow {
                        id: "branch".into(),
                        title: "分支 main".into(),
                        value: Default::default(),
                        action: None,
                    }],
                };
                source.host_sections = vec![
                    SummarySection {
                        id: "empty".into(),
                        rows: vec![],
                    },
                    section("repository:first"),
                    section("host:info"),
                    section("repository:second"),
                    section("repository-alike"),
                ];
                let mut summary = SummaryView::new(49903, Default::default());
                tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &source)
                    .unwrap();
                assert_eq!(
                    summary.section_order(),
                    [
                        "repository:first",
                        "host:info",
                        "repository:second",
                        "repository-alike"
                    ]
                );
                let root = summary.id();
                let first_header = summary.section_header_id("repository:first").unwrap();
                let second_header = summary.section_header_id("repository:second").unwrap();
                let first_label = summary.section_header_label_id("repository:first").unwrap();
                let second_label = summary
                    .section_header_label_id("repository:second")
                    .unwrap();
                let field = summary
                    .passive_text_id("repository:first", "branch")
                    .unwrap();
                let mut host = summary.into_host().unwrap();
                let mut text = Text::new(scale, true);
                let frame = Rect::new(0., 0., width, 2000.);
                let mut scene = Scene::new();
                host.render(&mut scene, &mut text, frame, frame, theme.into())
                    .unwrap();
                let layouts: Vec<_> = host
                    .get(root)
                    .unwrap()
                    .component::<SummaryView>()
                    .unwrap()
                    .section_layouts()
                    .into_iter()
                    .cloned()
                    .collect();
                assert!(layouts[0].divider.is_none());
                assert!(layouts[1].divider.is_none());
                assert!(layouts[3].divider.is_none());
                let divider = layouts[2]
                    .divider
                    .expect("only non-first repository gets a divider");
                assert!(
                    (divider.min_x() - layouts[2].header.min_x() - 2. * Spacing::Regular.value())
                        .abs()
                        < 0.1
                );
                assert!(
                    (divider.size.width
                        - (layouts[2].header.size.width - 4. * Spacing::Regular.value()))
                    .abs()
                        < 0.1,
                    "divider must follow the actual SDK header frame, not the outer host width"
                );
                assert_eq!(divider.min_y(), layouts[2].header.min_y());
                assert!(
                    (layouts[2].header.size.height
                        - layouts[0].header.size.height
                        - rule
                        - Spacing::Regular.value())
                    .abs()
                        < 0.1
                );
                let a = host.get(first_label).unwrap().frame();
                let b = host.get(second_label).unwrap().frame();
                assert!(
                    (b.min_y()
                        - layouts[2].header.min_y()
                        - (a.min_y() - layouts[0].header.min_y())
                        - rule
                        - Spacing::Regular.value())
                    .abs()
                        < 0.1
                );
                let mut expected = Scene::new();
                tekes_ui_controls::layout_view::SeparatorView::default().render(
                    &mut expected,
                    divider,
                    tekes_ui_controls::extended::Axis::Horizontal,
                    Some(frame),
                    theme.into(),
                );
                for ink in expected
                    .primitives()
                    .iter()
                    .filter(|p| matches!(p, tekes_ui_scene::Primitive::Quad(_)))
                {
                    assert!(
                        scene.primitives().contains(ink),
                        "separator must use original SDK ink/theme"
                    );
                }
                host.focus(Some(field), true);
                host.accessibility_action(field, None, AccessibilityAction::SetSelection(0..2));
                assert!(
                    host.accessibility_action(second_header, None, AccessibilityAction::Press)
                        .activated
                );
                assert!(
                    host.get(root)
                        .unwrap()
                        .component::<SummaryView>()
                        .unwrap()
                        .section_layouts()
                        .is_empty()
                );
                scene.clear();
                host.render(&mut scene, &mut text, frame, frame, theme.into())
                    .unwrap();
                assert_eq!(host.focused(), Some(field));
                let second = host
                    .get(root)
                    .unwrap()
                    .component::<SummaryView>()
                    .unwrap()
                    .section_layouts()[2]
                    .clone();
                assert!(
                    !second.expanded && second.body.is_none() && second.divider == Some(divider)
                );
                source.revision += 1;
                source.host_sections.swap(1, 3);
                tekes_agent_ui_consumer_check::publish_summary_context(
                    host.get_mut(root)
                        .unwrap()
                        .component_mut::<SummaryView>()
                        .unwrap(),
                    &source,
                )
                .unwrap();
                assert!(
                    host.get(root)
                        .unwrap()
                        .component::<SummaryView>()
                        .unwrap()
                        .section_layouts()
                        .is_empty()
                );
                scene.clear();
                host.render(&mut scene, &mut text, frame, frame, theme.into())
                    .unwrap();
                let summary = host.get(root).unwrap().component::<SummaryView>().unwrap();
                assert_eq!(
                    summary.section_header_id("repository:first"),
                    Some(first_header)
                );
                assert_eq!(
                    summary.section_header_id("repository:second"),
                    Some(second_header)
                );
                assert!(summary.section_layouts()[0].divider.is_none());
                assert!(!summary.section_layouts()[0].expanded);
                assert!(summary.section_layouts()[2].divider.is_some());
                assert_eq!(
                    host.text_surface(field).unwrap().input.selected_text(),
                    "分支"
                );
                source.revision += 1;
                source.owner = SummaryOwner::Workspace {
                    id: "consumer-other-workspace".into(),
                };
                tekes_agent_ui_consumer_check::publish_summary_context(
                    host.get_mut(root)
                        .unwrap()
                        .component_mut::<SummaryView>()
                        .unwrap(),
                    &source,
                )
                .unwrap();
                scene.clear();
                host.render(&mut scene, &mut text, frame, frame, theme.into())
                    .unwrap();
                assert!(host.get(first_header).is_none());
                assert!(
                    host.get(root)
                        .unwrap()
                        .component::<SummaryView>()
                        .unwrap()
                        .section_layouts()[0]
                        .divider
                        .is_none()
                );
            }
        }
    }
}

#[test]
fn transcript_clock_is_a_display_input_and_stale_plans_preserve_the_original_caption() {
    let scope = RunScope::main("consumer", "footnote-time");
    let mut snapshot = TranscriptSnapshot {
        turns: vec![TranscriptTurn {
            id: "timed-turn".into(),
            status: "running".into(),
            input: "输入".into(),
            started_at: Some(10000.),
            ended_at: None,
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        ..Default::default()
    };
    let mut content = TranscriptContent::new(
        49904,
        49905..49920,
        scope.clone(),
        TranscriptContentLabels {
            footnote: TurnFootnoteLabels {
                finished_timestamp: |end, _| (end == 85000.).then(|| "周四 下午1:03".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let mut timing = TranscriptFootnoteTiming {
        revision: 1,
        now_ms: Some(75000.),
        live_turn_id: Some("timed-turn".into()),
        goal: None,
    };
    content
        .update_with_timing(&scope, 1, 1, &snapshot, true, &timing)
        .unwrap();
    let root = content.id();
    let footer = content
        .row_id(&TranscriptContentRowKey::TurnFootnote("timed-turn".into()))
        .unwrap();
    let mut host = ContentHost::new(Content::new(root, content)).unwrap();
    let info = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .information_id()
        .unwrap();
    let original = host.get(info).unwrap().component::<TextContent>().unwrap() as *const _ as usize;
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 5s"
    );
    timing.revision = 2;
    timing.now_ms = Some(80000.);
    let planned = host
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .prepare_update_with_timing(&scope, 1, 1, &snapshot, true, &timing)
        .unwrap();
    timing.revision = 3;
    timing.now_ms = Some(85000.);
    let content = host
        .get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap();
    content
        .update_with_timing(&scope, 1, 1, &snapshot, true, &timing)
        .unwrap();
    assert!(content.apply_prepared_update(&planned).is_err());
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 15s"
    );
    assert_eq!(
        host.get(info).unwrap().component::<TextContent>().unwrap() as *const _ as usize,
        original
    );
    snapshot.turns[0].status = "completed".into();
    snapshot.turns[0].ended_at = Some(85000.);
    let older_clock = TranscriptFootnoteTiming {
        revision: 2,
        now_ms: Some(80000.),
        live_turn_id: None,
        goal: None,
    };
    assert_eq!(
        host.get_mut(root)
            .unwrap()
            .component_mut::<TranscriptContent>()
            .unwrap()
            .update_with_timing(&scope, 2, 1, &snapshot, false, &older_clock),
        Err("stale or conflicting Transcript footnote timing")
    );
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "1m 15s"
    );
    timing.revision = 4;
    timing.live_turn_id = None;
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update_with_timing(&scope, 2, 1, &snapshot, false, &timing)
        .unwrap();
    // A timestamp by itself is not an output aside. The original SDK node is
    // privately retained while the turn has neither output nor fork controls.
    assert!(host.get(info).is_none());
    snapshot.items.push(TranscriptItem {
        id: "timed-answer".into(),
        record_id: None,
        reconciliation_id: None,
        turn_id: "timed-turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: "正式输出".into(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: true,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: Some(1),
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence: 1,
            content_index: None,
        }),
        time: Some(85000.),
        metadata: Default::default(),
    });
    host.get_mut(root)
        .unwrap()
        .component_mut::<TranscriptContent>()
        .unwrap()
        .update_with_timing(&scope, 3, 1, &snapshot, false, &timing)
        .unwrap();
    assert_eq!(
        host.get(info)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        "周四 下午1:03"
    );
    assert_eq!(
        host.get(info).unwrap().component::<TextContent>().unwrap() as *const _ as usize,
        original
    );
}

#[test]
fn output_aside_feedback_notes_and_auxiliary_actions_keep_the_original_composer() {
    use tekes_agent_ui_bindings::{
        SessionMessageFeedback, SessionMessageFeedbackRating, TurnFeedbackTarget,
    };
    use tekes_ui_controls::extended::{Axis, Stack};
    let scope = RunScope::main("consumer", "feedback-aside");
    let mut answer = TranscriptItem {
        id: "answer".into(),
        record_id: Some("public-answer".into()),
        reconciliation_id: None,
        turn_id: "turn".into(),
        kind: TranscriptItemKind::Assistant,
        text: String::new(),
        reasoning: String::new(),
        attachments: vec![],
        content_blocks: None,
        is_visible: true,
        is_final: true,
        is_provisional: false,
        reasoning_ended: true,
        source_sequence: Some(2),
        source_position: Some(TranscriptSourcePosition::Journal {
            sequence: 2,
            content_index: None,
        }),
        time: Some(13000.),
        metadata: Default::default(),
    };
    answer.metadata["feedbackMessageId"] = "public-answer".into();
    let mut input = answer.clone();
    input.id = "input".into();
    input.record_id = None;
    input.metadata = Default::default();
    input.kind = TranscriptItemKind::User;
    input.text = "原输入".into();
    input.source_sequence = Some(1);
    input.source_position = Some(TranscriptSourcePosition::Journal {
        sequence: 1,
        content_index: None,
    });
    let mut snapshot = TranscriptSnapshot {
        items: vec![input, answer],
        turns: vec![TranscriptTurn {
            id: "turn".into(),
            status: "completed".into(),
            input: "原输入".into(),
            started_at: Some(10000.),
            ended_at: Some(13000.),
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        ..Default::default()
    };
    let mut content = TranscriptContent::new(
        49970,
        49971..49995,
        scope.clone(),
        TranscriptContentLabels {
            feedback: Some(Default::default()),
            ..Default::default()
        },
    )
    .unwrap();
    content.update(&scope, 1, 1, &snapshot, true).unwrap();
    assert_eq!(
        content.feedback_controls().count(),
        0,
        "a proven empty answer alone has no aside"
    );
    let document = TranscriptDocument::read(&snapshot).unwrap();
    assert!(
        TurnFeedbackTarget::read(&document.sections[0]).is_some(),
        "display never changes target authority"
    );
    snapshot.items[1].text = "正式回答".into();
    content.update(&scope, 2, 1, &snapshot, true).unwrap();
    let (feedback, control) = content.feedback_controls().next().unwrap();
    let target = control.target().unwrap().clone();
    // The independent consumer observes cold/failure as metadata, never an
    // invented empty record; the original committed frame supersedes failure.
    let control = content.feedback_mut(feedback).unwrap();
    assert!(control.notice().is_empty());
    assert!(
        !control
            .children()
            .iter()
            .any(|node| node.id() == control.ids().refresh)
    );
    assert!(
        !control.children().iter().any(|node| {
            node.component::<TextContent>()
                .is_some_and(|text| text.text == "评价尚未加载")
        }),
        "cold metadata is not a permanent compact aside caption"
    );
    control.initial_read_failed("owned read failure".into());
    assert!(control.state().is_none());
    assert!(control.needs_refresh());
    assert_eq!(control.diagnostic(), Some("owned read failure"));
    assert_eq!(control.notice(), "无法加载评价");
    assert!(
        control
            .children()
            .iter()
            .any(|node| node.id() == control.ids().refresh)
    );
    let state = FeedbackControlState {
        target: target.clone(),
        publication: 1,
        connection_epoch: 1,
        current: None,
        put_available: true,
        delete_available: true,
    };
    content
        .feedback_mut(feedback)
        .unwrap()
        .update(state.clone())
        .unwrap();
    let committed = content.feedback_mut(feedback).unwrap();
    assert!(committed.notice().is_empty());
    assert!(!committed.needs_refresh());
    assert!(
        !committed
            .children()
            .iter()
            .any(|node| node.id() == committed.ids().refresh)
    );
    committed.initial_read_failed("late read failure".into());
    assert_eq!(committed.state(), Some(&state));
    assert!(committed.diagnostic().is_none());
    // A consumer can retain an old committed display after reconnect. Once
    // authority is revoked, a read failure must remain visible without turning
    // the old frame into a writable or empty record.
    let mut revoked = TurnFeedback::new(49968, Default::default());
    revoked.bind_target(target.clone()).unwrap();
    revoked.update(state.clone()).unwrap();
    revoked.invalidate_authority();
    revoked.initial_read_failed("reconnect read failure".into());
    assert_eq!(revoked.state(), Some(&state));
    assert!(revoked.needs_refresh());
    assert_eq!(revoked.notice(), "无法加载评价");
    assert_eq!(revoked.diagnostic(), Some("reconnect read failure"));
    let mut renewed = state.clone();
    renewed.publication += 1;
    renewed.connection_epoch += 1;
    revoked.update(renewed.clone()).unwrap();
    assert_eq!(revoked.state(), Some(&renewed));
    assert!(!revoked.needs_refresh());
    assert!(revoked.diagnostic().is_none());
    assert!(revoked.notice().is_empty());
    let root = content.id();
    let composer = tekes_agent_ui_consumer_check::new_composer(49969);
    let editor = composer.ids().editor;
    let mut host = ContentHost::new(Content::new(
        ComponentId(64),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 8.,
            },
            flexible_child: None,
            children: vec![
                Content::new(composer.ids().root, composer),
                Content::new(root, content),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let mut text = Text::new(1., true);
    let frame = Rect::new(0., 0., 900., 1400.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    host.focus(Some(editor), true);
    host.event(&ContentEvent::Preedit {
        text: "辅助动作期间中文组字".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    });
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let original = host.text_surface(editor).unwrap() as *const _ as usize;
    let ids = host
        .get(feedback)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap()
        .ids();
    assert!(
        host.get(ids.edit_note).is_none(),
        "unrated means no note entry, not a disabled placeholder"
    );
    let content = host
        .get(root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    let input = content
        .message_id(TranscriptMessageKey::Item("input"))
        .unwrap();
    let input_copy = host
        .get(input)
        .unwrap()
        .component::<InputMessage>()
        .unwrap()
        .copy_button_id()
        .unwrap();
    let footer = content
        .row_id(&TranscriptContentRowKey::TurnFootnote("turn".into()))
        .unwrap();
    let output_copy = host
        .get(footer)
        .unwrap()
        .component::<TurnFootnote>()
        .unwrap()
        .copy_button_id()
        .unwrap();
    for (index, button) in [input_copy, output_copy, ids.positive]
        .into_iter()
        .enumerate()
    {
        if index == 0 {
            let point = host.get(button).unwrap().frame().center();
            host.event(&ContentEvent::PointerDown(point));
            assert!(host.event(&ContentEvent::PointerUp(point)).activated);
        } else {
            assert!(
                host.accessibility_action(button, None, AccessibilityAction::Press)
                    .activated
            );
        }
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(
            host.text_surface(editor).unwrap().input.external_baseline(),
            baseline
        );
        assert_eq!(
            host.text_surface(editor).unwrap() as *const _ as usize,
            original
        );
        let actions = host.drain_actions();
        let content = host
            .get_mut(root)
            .unwrap()
            .component_mut::<TranscriptContent>()
            .unwrap();
        if index < 2 {
            let copy = content.take_copy_intents(&actions);
            assert_eq!(copy.len(), 1);
            assert_eq!(copy[0].scope(), &scope);
            assert_eq!(
                copy[0].text(),
                if index == 0 {
                    "原输入"
                } else {
                    "正式回答"
                }
            );
        } else {
            let gestures = content.take_feedback_intents(&actions);
            assert_eq!(gestures.len(), 1);
            assert_eq!(gestures[0].1.target(), &target);
        }
    }
    let mut rated = state;
    rated.publication = 2;
    rated.current = Some(SessionMessageFeedback {
        message_id: "public-answer".into(),
        rating: SessionMessageFeedbackRating::Positive,
        version: "committed:2".into(),
        note: Some("已提交的中文备注".into()),
        updated_at_milliseconds: None,
    });
    host.get_mut(feedback)
        .unwrap()
        .component_mut::<TurnFeedback>()
        .unwrap()
        .update(rated)
        .unwrap();
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let control = host
        .get(feedback)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap();
    let note = control.ids().edit_note;
    assert_eq!(
        host.get(note)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .label,
        "已提交的中文备注"
    );
    assert!(
        control
            .children()
            .iter()
            .filter_map(|node| node.component::<TextContent>())
            .all(|caption| !caption.text.is_empty()),
        "known feedback adds no empty caption band"
    );
    assert!(
        host.accessibility_action(note, None, AccessibilityAction::Press)
            .activated
    );
    let actions = host.drain_actions();
    assert!(
        host.get_mut(feedback)
            .unwrap()
            .component_mut::<TurnFeedback>()
            .unwrap()
            .take_intents(&actions)
            .is_empty()
    );
    assert!(
        host.get(feedback)
            .unwrap()
            .component::<TurnFeedback>()
            .unwrap()
            .is_open()
    );
    assert_eq!(host.focused(), Some(editor));
    assert_eq!(
        host.text_surface(editor).unwrap().input.external_baseline(),
        baseline
    );
    let note_ids = host
        .get(feedback)
        .unwrap()
        .component::<TurnFeedback>()
        .unwrap()
        .ids();
    let note_owner = host.text_surface(note_ids.editor).unwrap() as *const _ as usize;
    for theme in Theme::ALL {
        for scale in [0.85, 1., 1.5] {
            for width in [240., 400., 900.] {
                let mut text = Text::new(scale, true);
                let frame = Rect::new(0., 0., width, 1400.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let field = host
                    .get(note_ids.editor)
                    .unwrap()
                    .component::<TextFieldContent>()
                    .unwrap();
                assert_eq!(
                    field.surface.presentation,
                    tekes_ui_controls::TextSurfacePresentation::Content
                );
                assert_eq!(field.field.label, "评价备注");
                assert!((field.surface.outer.size.height - 120.).abs() <= 1.);
                assert!(field.surface.outer.size.width <= 400.);
                let cancel = host.get(note_ids.close).unwrap().frame();
                let save = host.get(note_ids.save).unwrap().frame();
                assert!(
                    cancel.max_x() < save.min_x(),
                    "Cancel precedes Save in source order"
                );
                assert!(
                    (save.max_x() - field.surface.outer.max_x()).abs() <= 1.,
                    "form actions trail the original editor"
                );
                assert_eq!(
                    host.get(note_ids.save)
                        .unwrap()
                        .component::<ButtonContent>()
                        .unwrap()
                        .variant,
                    tekes_ui_controls::paint::ButtonVariant::Primary
                );
                assert!(
                    host.semantics()
                        .iter()
                        .any(|node| node.semantics.name == "评价")
                );
                assert_eq!(
                    host.text_surface(note_ids.editor).unwrap() as *const _ as usize,
                    note_owner
                );
                assert_eq!(host.focused(), Some(editor));
                assert_eq!(
                    host.text_surface(editor).unwrap().input.external_baseline(),
                    baseline
                );
            }
        }
    }
}

#[test]
fn explicit_summary_hierarchy_moves_only_titles_and_retains_original_controls() {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    use tekes_ui_tokens::Spacing;
    for theme in Theme::ALL {
        for scale in [0.85, 1., 1.5] {
            for width in [144., 360.] {
                let level = Arc::new(AtomicU32::new(0));
                let projection = level.clone();
                let scope = RunScope::main("consumer", "indent");
                let mut source = input();
                source.owner = SummaryOwner::Run(scope.clone());
                source.host_sections[0].rows = ["action", "passive"]
                    .map(|id| SummaryRow {
                        id: id.into(),
                        title: "中文标题 English 👩🏽‍💻 e\u{301} · 换行内容 ".repeat(3),
                        value: "12 files".into(),
                        action: (id == "action").then(|| SummaryAction::OpenArtifact {
                            scope: scope.clone(),
                            path: "/captured/path".into(),
                        }),
                    })
                    .into();
                let mut summary = SummaryView::new(
                    49996,
                    SummaryLabels {
                        indent_level: Arc::new(move |owner, _, _| {
                            assert!(
                                matches!(owner,SummaryOwner::Run(scope) if scope.endpoint_id=="consumer" && scope.session_id=="indent")
                            );
                            projection.load(Ordering::SeqCst)
                        }),
                        ..Default::default()
                    },
                );
                tekes_agent_ui_consumer_check::publish_summary_context(&mut summary, &source)
                    .unwrap();
                let root = summary.id();
                let action = summary
                    .default_row_ids("host:repository", "action")
                    .unwrap();
                let passive = summary
                    .default_row_ids("host:repository", "passive")
                    .unwrap();
                let mut host = summary.into_host().unwrap();
                let mut text = Text::new(scale, true);
                let frame = Rect::new(0., 0., width, 10000.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                let field = host.text_surface(passive.title).unwrap() as *const _ as usize;
                let control = host
                    .get(action.row)
                    .unwrap()
                    .component::<SummaryActionRow>()
                    .unwrap()
                    .button() as *const _ as usize;
                if width > 300. {
                    host.focus(Some(passive.title), true);
                    host.accessibility_action(
                        passive.title,
                        None,
                        AccessibilityAction::SetSelection(0..2),
                    );
                }
                for depth in [1, 3, 0] {
                    level.store(depth, Ordering::SeqCst);
                    let row_before = host.get(action.title).unwrap().frame();
                    assert!(
                        tekes_agent_ui_consumer_check::publish_summary_context(
                            host.get_mut(root)
                                .unwrap()
                                .component_mut::<SummaryView>()
                                .unwrap(),
                            &source
                        )
                        .is_err(),
                        "changed hierarchy under the same publication must reject"
                    );
                    assert_eq!(host.get(action.title).unwrap().frame(), row_before);
                    source.revision += 1;
                    tekes_agent_ui_consumer_check::publish_summary_context(
                        host.get_mut(root)
                            .unwrap()
                            .component_mut::<SummaryView>()
                            .unwrap(),
                        &source,
                    )
                    .unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    for ids in [action, passive] {
                        let band = host.get(ids.row).unwrap().frame();
                        let title = host.get(ids.title).unwrap().frame();
                        let trailing = host.get(ids.trailing).unwrap().frame();
                        assert!(
                            (title.min_x()
                                - band.min_x()
                                - 2. * Spacing::Regular.value()
                                - depth as f32 * 14. * scale)
                                .abs()
                                < 0.1
                        );
                        assert!(
                            (trailing.max_x() - band.max_x() + 2. * Spacing::Regular.value()).abs()
                                < 0.1,
                            "metadata stays at the original trailing edge"
                        );
                        assert!(title.size.width >= 0. && trailing.size.width >= 0.);
                    }
                    assert_eq!(
                        host.text_surface(passive.title).unwrap() as *const _ as usize,
                        field
                    );
                    assert_eq!(
                        host.get(action.row)
                            .unwrap()
                            .component::<SummaryActionRow>()
                            .unwrap()
                            .button() as *const _ as usize,
                        control
                    );
                    if width > 300. {
                        assert_eq!(host.focused(), Some(passive.title));
                        assert_eq!(
                            host.text_surface(passive.title)
                                .unwrap()
                                .input
                                .selected_text(),
                            "中文"
                        );
                        assert!(
                            host.accessibility_action(action.row, None, AccessibilityAction::Press)
                                .activated
                        );
                        assert_eq!(host.focused(), Some(passive.title));
                        let actions = host.drain_actions();
                        let intents = host
                            .get(root)
                            .unwrap()
                            .component::<SummaryView>()
                            .unwrap()
                            .take_intents(&actions);
                        assert_eq!(intents.len(), 1);
                        assert_eq!(intents[0].scope, scope);
                    }
                }
            }
        }
    }
}

#[test]
fn second_host_copies_another_scope_without_retargeting_the_original_composing_editor() {
    use tekes_ui_controls::extended::{Axis, Stack};
    let parent = RunScope::main("consumer", "parent");
    let child = RunScope {
        endpoint_id: "consumer".into(),
        thread_id: "parent".into(),
        session_id: "child".into(),
    };
    let snapshot = |text: &str| TranscriptSnapshot {
        turns: vec![TranscriptTurn {
            id: "same-turn".into(),
            status: "completed".into(),
            input: String::new(),
            started_at: None,
            ended_at: None,
            usage: Default::default(),
            fork_boundary: None,
            footnote_actions: vec![],
            live_usage: None,
            reported_usage: None,
        }],
        items: vec![TranscriptItem {
            id: "same-item".into(),
            record_id: None,
            reconciliation_id: None,
            turn_id: "same-turn".into(),
            kind: TranscriptItemKind::Assistant,
            text: text.into(),
            reasoning: String::new(),
            attachments: vec![],
            content_blocks: None,
            is_visible: true,
            is_final: true,
            is_provisional: false,
            reasoning_ended: true,
            source_sequence: None,
            source_position: None,
            time: None,
            metadata: Default::default(),
        }],
        ..Default::default()
    };
    let composer = tekes_agent_ui_consumer_check::new_composer(49200);
    let editor = composer.ids().editor;
    let mut left =
        TranscriptContent::new(49201, 49202..49210, parent.clone(), Default::default()).unwrap();
    left.update(&parent, 1, 1, &snapshot("原主回复"), false)
        .unwrap();
    let left_id = left.id();
    let mut right =
        TranscriptContent::new(49211, 49212..49220, child.clone(), Default::default()).unwrap();
    right
        .update(&child, 1, 1, &snapshot("原子回复"), false)
        .unwrap();
    let right_id = right.id();
    let mut host = ContentHost::new(Content::new(
        ComponentId(492),
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 8.,
            },
            flexible_child: None,
            children: vec![
                Content::new(composer.ids().root, composer),
                Content::new(left_id, left),
                Content::new(right_id, right),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let mut text = Text::new(1., true);
    let frame = Rect::new(0., 0., 1800., 800.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    host.focus(Some(editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "原 Composer 中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    let baseline = host.text_surface(editor).unwrap().input.external_baseline();
    let owner = host.text_surface(editor).unwrap() as *const _ as usize;
    for (root, scope, expected) in [
        (right_id, &child, "原子回复"),
        (left_id, &parent, "原主回复"),
    ] {
        let copy = host
            .get(root)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
            .copy_action_ids()[0];
        let point = host.get(copy).unwrap().visible().unwrap().center();
        host.event_with_text(&ContentEvent::PointerDown(point), &mut text)
            .unwrap();
        host.event_with_text(&ContentEvent::PointerUp(point), &mut text)
            .unwrap();
        let actions = host.drain_actions();
        let captured = host
            .get(root)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
            .take_copy_intents(&actions);
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].scope(), scope);
        assert_eq!(captured[0].text(), expected);
        assert_eq!(captured[0].message(), None);
        assert!(captured[0].claim());
        assert!(!captured[0].clone().claim());
        assert_eq!(host.focused(), Some(editor));
        assert_eq!(
            host.text_surface(editor).unwrap() as *const _ as usize,
            owner
        );
        assert_eq!(
            host.text_surface(editor).unwrap().input.external_baseline(),
            baseline
        );
    }
}
