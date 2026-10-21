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
struct Text {
    shaper: CoreTextShaper,
    scale: f32,
    labels: Vec<(LabelOptions, Rc<LabelLayout>)>,
    label_service: bool,
}
impl Text {
    fn new(scale: f32, label_service: bool) -> Self {
        Self {
            shaper: CoreTextShaper::new(CoreTextFonts::new()),
            scale,
            labels: vec![],
            label_service,
        }
    }
    fn style(&self, role: proposal::TextRole) -> TextStyle {
        let style = role.style();
        TextStyle::system(style.size * self.scale).with_weight(style.weight)
    }
}
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
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
                let (_, layout) = text.labels.last().expect("original SDK requested a label");
                assert!(text.labels.iter().all(|(options, layout)| {
                    *options == LabelOptions::single_line(LabelOverflow::TailEllipsis)
                        && layout.paragraph().lines.len() == 1
                        && layout.source() == TITLE.repeat(6)
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
