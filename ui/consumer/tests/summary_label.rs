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
