//! Known-boundary consumer probe: passing reproduces missing compound focus routing.
#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer, composition::ContentEvent};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_text::{Paragraph, coretext::{CoreTextFonts, CoreTextShaper}, model::ShapeOptions, style::TextStyle, traits::Shaper};
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

#[tokio::test]
async fn known_boundary_unfocused_padding_does_not_focus_editor_but_existing_preedit_is_protected() {
    let (runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let ids = ComposerIds::new(50000);
    let mut view = Composer::new(ids);
    view.update(&controller.snapshot(), Default::default()).unwrap();
    let mut host = view.into_host().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 420., 120.);
    host.render(&mut Scene::new(), &mut text, frame, frame, Theme::LIGHT.into()).unwrap();
    let band = host.get(ids.root).unwrap().component::<Composer>().unwrap().layout_geometry().unwrap().input_band;
    let padding = Point::new(band.min_x() + 1., band.center().y);
    assert!(band.contains(padding));
    assert!(!host.get(ids.editor).unwrap().frame().contains(padding));
    assert_eq!(host.focused(), None);
    host.event(&ContentEvent::PointerDown(padding));
    host.event(&ContentEvent::PointerUp(padding));
    // Known boundary: decorative padding does not acquire the original editor.
    assert_eq!(host.focused(), None);
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::Preedit { text: "中文组字".into(), selection: 0..4, clauses: vec![], replacement: None });
    assert!(host.text_surface(ids.editor).unwrap().input.marked.is_some());
    host.event(&ContentEvent::PointerDown(padding));
    // Existing library behavior is safe here; do not report an IME regression.
    assert_eq!(host.focused(), Some(ids.editor));
    assert!(host.text_surface(ids.editor).unwrap().input.marked.is_some());
    runtime.stop().await.unwrap();
}
