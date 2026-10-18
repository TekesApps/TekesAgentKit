#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::{AgentText, RunSurface, TRANSCRIPT};
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
struct Text(CoreTextShaper);
impl TextSurfaceTextRenderer for Text {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for Text {
    fn shape_surface(
        &mut self,
        s: &str,
        role: proposal::TextRole,
        width: Option<f32>,
    ) -> Rc<Paragraph> {
        Rc::new(self.0.shape(
            s,
            &TextStyle::system(role.style().size),
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
async fn append_and_reflow_preserve_reading_intent_without_stealing_focus() {
    let (runtime, c) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let mut ui = RunSurface::new();
    ui.sync(&c.snapshot()).unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()));
    let mut scene = Scene::new();
    let render = |ui: &mut RunSurface, text: &mut Text, scene: &mut Scene, w| {
        let f = Rect::new(0., 0., w, 700.);
        ui.render(scene, text, f, f, Palette::from(Theme::LIGHT))
            .unwrap();
    };
    render(&mut ui, &mut text, &mut scene, 900.);
    ui.follow_latest();
    render(&mut ui, &mut text, &mut scene, 900.);
    let before = ui.transcript().field.surface.scroll.offset.y;
    assert!(before > 0.);
    let position = ui.transcript().field.surface.inner.center();
    ui.event(&ContentEvent::Scroll {
        position,
        delta: Point::new(0., -90.),
        precise: true,
        begin: true,
        end: true,
        momentum: false,
        time: 1.,
    });
    render(&mut ui, &mut text, &mut scene, 900.);
    assert!(
        !ui.transcript()
            .field
            .reading
            .as_ref()
            .unwrap()
            .is_following_latest()
    );
    let offset = ui.transcript().field.surface.scroll.offset.y;
    let old_focus = ui.tree.focused();
    // Incoming tail text is a document input, not a custom scroll correction.
    ui.tree
        .get_mut(TRANSCRIPT)
        .unwrap()
        .component_mut::<AgentText>()
        .unwrap()
        .field
        .surface
        .input
        .text
        .push_str("\n追加消息一\n追加消息二");
    render(&mut ui, &mut text, &mut scene, 900.);
    assert!((ui.transcript().field.surface.scroll.offset.y - offset).abs() < 1.);
    assert_eq!(ui.tree.focused(), old_focus);
    render(&mut ui, &mut text, &mut scene, 600.);
    assert!(
        !ui.transcript()
            .field
            .reading
            .as_ref()
            .unwrap()
            .is_following_latest()
    );
    ui.follow_latest();
    render(&mut ui, &mut text, &mut scene, 600.);
    assert_eq!(
        ui.transcript().field.surface.scroll.offset.y,
        ui.transcript().field.surface.scroll.limit().y
    );
    runtime.stop().await.unwrap();
}
