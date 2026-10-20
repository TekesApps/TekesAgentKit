//! Receiver-only probe for the explicitly captured #31 source. It consumes the
//! real retained Summary rows and existing reading transactions, not a new view.
#![cfg(target_os = "macos")]
#[path = "agentkit-reading-candidate.rs"]
mod reading_regression;
#[path = "agentkit-run-preflight-candidate.rs"]
mod run_regression;
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer, composition::*,
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{
    LabelLayout, LabelOptions, Paragraph,
    coretext::{CoreTextFonts, CoreTextShaper},
    model::ShapeOptions,
    style::TextStyle,
    traits::Shaper,
};
use tekes_ui_tokens::{Rgba, Theme, proposal};
struct Text {
    shaper: CoreTextShaper,
    scale: f32,
    labels: Vec<Rc<LabelLayout>>,
    available: bool,
}
impl Text {
    fn new(scale: f32) -> Self {
        Self {
            shaper: CoreTextShaper::new(CoreTextFonts::new()),
            scale,
            labels: vec![],
            available: true,
        }
    }
    fn style(&self, role: proposal::TextRole) -> TextStyle {
        let m = role.style();
        if proposal::text_family(role.id()) == Some("mono") {
            TextStyle::named("Menlo", m.size * self.scale)
        } else {
            TextStyle::system(m.size * self.scale)
        }
        .with_weight(m.weight)
    }
}
impl TextSurfaceHost for Text {
    fn shape_surface(
        &mut self,
        source: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.shaper.shape(
            source,
            &self.style(role),
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
        if !self.available {
            return None;
        }
        let layout = Rc::new(
            self.shaper
                .shape_label(
                    source,
                    &self.style(role),
                    &ShapeOptions::with_width(f64::from(width)),
                    options,
                )
                .with_min_line_height(f64::from(role.style().line_height * self.scale)),
        );
        self.labels.push(layout.clone());
        Some(layout)
    }
    fn type_scale(&self) -> f32 {
        self.scale
    }
}
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, role: proposal::TextRole, width: f32) -> Size {
        let p = self.shape_surface(text, role, Some(width));
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
fn input(title: &str) -> SummaryInput {
    let scope = RunScope::main("label-candidate", "original");
    SummaryInput {
        revision: 1,
        scope: scope.clone(),
        facts: None,
        host_sections: vec![SummarySection {
            id: "repository".into(),
            rows: vec![
                SummaryRow {
                    id: "value".into(),
                    title: title.into(),
                    value: Default::default(),
                    action: None,
                },
                SummaryRow {
                    id: "action".into(),
                    title: title.into(),
                    value: Default::default(),
                    action: Some(SummaryAction::OpenArtifact {
                        scope,
                        path: "/owned/original".into(),
                    }),
                },
            ],
        }],
    }
}
fn render(
    host: &mut ContentHost,
    text: &mut Text,
    width: f32,
    dark: bool,
) -> Result<(), CompositionError> {
    // Give the actual rows enough document space; viewport/reading tests are
    // independently retained below through the unchanged regression modules.
    let frame = Rect::new(0., 0., width, 100_000.);
    host.render(
        &mut Scene::new(),
        text,
        frame,
        frame,
        if dark { Theme::DARK } else { Theme::LIGHT }.into(),
    )
}
#[test]
fn summary_header_is_single_line_while_original_rows_wrap_and_keep_names_and_actions() {
    let sources = [
        "很长的中文任务与分支标题".repeat(30),
        "very long English task and branch title ".repeat(30),
        "👩🏽‍💻 e\u{301} 家庭👨‍👩‍👧‍👦 ".repeat(30),
    ];
    let mut cases = 0;
    for title in sources {
        let labels = SummaryLabels {
            section: |id| format!("{id} · 长标题 English 👩🏽‍💻 e\u{301} 组合字素 ").repeat(16),
            ..Default::default()
        };
        let header_title = (labels.section)("repository");
        let mut view = SummaryView::new(160, labels);
        let mut value = input(&title);
        view.update(&value).unwrap();
        let root = view.id();
        let passive = view.row_id("repository", "value").unwrap();
        let action = view.action_id("repository", "action").unwrap();
        let header = view.section_header_id("repository").unwrap();
        let mut host = view.into_host().unwrap();
        for scale in [0.85, 1., 1.4] {
            let mut text = Text::new(scale);
            for width in [240., 420., 900.] {
                for dark in [false, true] {
                    text.labels.clear();
                    render(&mut host, &mut text, width, dark).unwrap();
                    assert!(
                        text.labels.iter().all(|p| p.source() != title),
                        "ordinary Summary rows must not opt into the header's single-line policy"
                    );
                    for id in [passive, action] {
                        assert!(
                            host.get(id).unwrap().frame().size.height
                                > 2. * proposal::TextRole::UiBody.style().line_height * scale,
                            "the long original row title must wrap"
                        );
                    }
                    assert_eq!(
                        host.get(passive)
                            .unwrap()
                            .component::<TextContent>()
                            .unwrap()
                            .text,
                        title
                    );
                    assert_eq!(
                        host.get(action)
                            .unwrap()
                            .component::<ButtonContent>()
                            .unwrap()
                            .label,
                        title
                    );
                    for id in [passive, action] {
                        let ax = host.semantics().into_iter().find(|n| n.id == id).unwrap();
                        assert_eq!(ax.semantics.name, title);
                        assert_eq!(ax.frame, host.get(id).unwrap().frame());
                    }
                    let headers: Vec<_> = text
                        .labels
                        .iter()
                        .filter(|p| p.source() == header_title)
                        .collect();
                    assert!(
                        !headers.is_empty(),
                        "actual Summary header never requested label layout"
                    );
                    assert!(
                        headers
                            .iter()
                            .all(|p| p.paragraph().lines.len() == 1 && p.is_truncated())
                    );
                    let header_ax: Vec<_> = host
                        .semantics()
                        .into_iter()
                        .filter(|n| n.semantics.name == header_title)
                        .collect();
                    assert_eq!(
                        header_ax.len(),
                        1,
                        "decorative title must not add a second AX node"
                    );
                    assert_eq!(header_ax[0].id, header);
                    assert!(
                        header_ax[0].frame.size.height
                            < 2. * proposal::TextRole::UiBody.style().line_height * scale
                    );
                    cases += 1;
                }
            }
        }
        host.accessibility_action(action, None, AccessibilityAction::Press);
        let actions = host.drain_actions();
        assert!(
            matches!(host.get(root).unwrap().component::<SummaryView>().unwrap().take_intents(&actions).as_slice(),
            [SummaryIntent { action: SummaryAction::OpenArtifact { path, .. }, .. }] if path == "/owned/original")
        );
        value.revision += 1;
        value.host_sections[0].rows[0].title = "更新后的完整标题".repeat(30);
        let view = host
            .get_mut(root)
            .unwrap()
            .component_mut::<SummaryView>()
            .unwrap();
        view.update(&value).unwrap();
        assert_eq!(view.row_id("repository", "value"), Some(passive));
        assert_eq!(view.action_id("repository", "action"), Some(action));
        render(&mut host, &mut Text::new(1.), 240., false).unwrap();
        assert_eq!(
            host.get(passive)
                .unwrap()
                .component::<TextContent>()
                .unwrap()
                .text,
            value.host_sections[0].rows[0].title
        );
        host.accessibility_action(header, None, AccessibilityAction::Press);
        assert!(
            !host
                .semantics()
                .iter()
                .any(|n| n.id == passive || n.id == action)
        );
        assert!(
            host.get(root)
                .unwrap()
                .component::<SummaryView>()
                .unwrap()
                .take_intents(&actions)
                .is_empty()
        );
    }
    assert_eq!(cases, 54);
}
#[test]
fn missing_label_service_is_explicit_and_default_text_still_naturally_wraps() {
    let mut view = SummaryView::new(161, Default::default());
    view.update(&input(&"很长的标题".repeat(30))).unwrap();
    let mut host = view.into_host().unwrap();
    let mut text = Text::new(1.);
    text.available = false;
    assert!(matches!(
        render(&mut host, &mut text, 240., false),
        Err(CompositionError::MissingLabelServices(_))
    ));
    let source = "普通正文继续自然换行 ".repeat(20);
    let id = ComponentId(1);
    let mut plain = ContentHost::new(Content::new(id, TextContent::new(&source))).unwrap();
    let p = text.shape_surface(&source, proposal::TextRole::UiBody, Some(240.));
    assert!(p.lines.len() > 1);
    render(&mut plain, &mut text, 240., false).unwrap();
    assert_eq!(
        plain
            .get(id)
            .unwrap()
            .component::<TextContent>()
            .unwrap()
            .text,
        source
    );
}
