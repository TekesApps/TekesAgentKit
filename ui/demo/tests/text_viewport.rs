#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::{AgentText, RunSurface};
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
use tekes_ui_tokens::{Palette, Rgba, Theme, proposal};
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

#[tokio::test]
async fn real_run_uses_public_fill_and_multiline_bounded_grow_without_custom_arrange() {
    use tekes_agent_components_ui::{COMPOSER, Intent, SEND};
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let mut scene = Scene::new();
    let f = Rect::new(50., 70., 900., 700.);
    ui.render(&mut scene, &mut text, f, f, Palette::from(Theme::LIGHT))
        .unwrap();
    assert!(ui.editor().input.multiline);
    let min = ui.editor().outer.size.height;
    ui.tree.focus(Some(COMPOSER), true);
    let actions = ui.event(&ContentEvent::Commit {
        text: "中文 English 😀\n".repeat(30),
        replacement: None,
    });
    for a in actions {
        if let Intent::Edit { destination, text } = a {
            let mut draft = c.snapshot().draft.clone();
            draft.text = text;
            c.edit(&destination, draft).unwrap();
        }
    }
    ui.sync(&c.snapshot()).unwrap();
    ui.render(&mut scene, &mut text, f, f, Palette::from(Theme::LIGHT))
        .unwrap();
    let field = &ui
        .tree
        .get(COMPOSER)
        .unwrap()
        .component::<AgentText>()
        .unwrap()
        .field;
    let tekes_ui_controls::composition::TextViewport::Grow { max, .. } = field.viewport else {
        panic!("public bounded grow")
    };
    assert_eq!(ui.editor().outer.size.height, max);
    assert!(max > min && ui.editor().scroll.limit().y > 0.);
    let nodes = ui.tree.semantics();
    let send = nodes.iter().find(|n| n.id == SEND).unwrap();
    assert!(send.frame.max_y() <= f.max_y());
    assert!(send.frame.min_y() > ui.editor().outer.max_y());
    assert!(ui.transcript().field.surface.outer.size.height > 0.);
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn multiline_editor_keeps_explicit_button_send_policy_and_preedit_during_resize() {
    use tekes_agent_components_ui::{COMPOSER, Intent};
    use tekes_ui_controls::ControlKey;
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    let actions = ui.event(&ContentEvent::Key {
        key: ControlKey::Enter,
        down: true,
    });
    assert!(!actions.iter().any(|i| matches!(i, Intent::Send { .. })));
    assert_eq!(ui.editor().input.text, "\n");
    ui.event(&ContentEvent::Preedit {
        text: "拼".into(),
        selection: 1..1,
        clauses: vec![],
        replacement: None,
    });
    let before = format!("{:?}", ui.editor().input);
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.5);
    let frame = Rect::new(50., 70., 480., 600.);
    ui.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Palette::from(Theme::LIGHT),
    )
    .unwrap();
    assert_eq!(format!("{:?}", ui.editor().input), before);
    assert_eq!(ui.tree.focused(), Some(COMPOSER));
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn real_composing_editor_reflows_inside_clipped_native_viewport() {
    use tekes_agent_components_ui::COMPOSER;
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    ui.tree.focus(Some(COMPOSER), true);
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let initial = Rect::new(16., 16., 968., 728.);
    ui.render(
        &mut Scene::new(),
        &mut text,
        initial,
        Rect::new(0., 0., 1000., 760.),
        Theme::LIGHT.into(),
    )
    .unwrap();
    ui.event(&ContentEvent::Commit {
        text: "中文 English 😀 é 12345 ".repeat(140),
        replacement: None,
    });
    ui.event(&ContentEvent::Preedit {
        text: "ni".into(),
        selection: 2..2,
        replacement: None,
        clauses: vec![],
    });
    let editor = format!("{:?}", ui.editor().input);
    for (scale, frame, clip) in [
        (1.5, initial, Rect::new(0., 0., 1000., 760.)),
        (2., initial, Rect::new(32., 0., 872., 760.)),
        (
            2.,
            Rect::new(16., 16., 688., 568.),
            Rect::new(32., 0., 592., 600.),
        ),
        (1., initial, Rect::new(0., 0., 1000., 760.)),
    ] {
        text.1 = scale;
        ui.render(
            &mut Scene::new(),
            &mut text,
            frame,
            clip,
            Theme::LIGHT.into(),
        )
        .unwrap();
        assert_eq!(format!("{:?}", ui.editor().input), editor);
        assert!(
            ui.editor()
                .visible_frame()
                .unwrap()
                .contains(ui.editor().caret().center())
        );
        let node = ui
            .tree
            .semantics()
            .into_iter()
            .find(|n| n.id == COMPOSER)
            .unwrap();
        assert_eq!(node.visible, ui.editor().visible_frame());
    }
    runtime.stop().await.unwrap();
}
