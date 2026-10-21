use std::rc::Rc;

use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer, composition::*,
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

/// Test-only real CoreText layout; no GPU/native/VoiceOver acceptance.
pub fn paint(host: &mut ContentHost) {
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let frame = Rect::new(0., 0., 1850., 800.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
}
pub fn select(
    view: &mut tekes_agent_ui_demo::component_demo::ComponentDemo,
    address: &tekes_agent_contract::SessionAddress,
    action: &str,
) -> (
    Vec<tekes_agent_ui_demo::component_demo::DemoCommand>,
    ComponentId,
    usize,
) {
    use tekes_agent_components_ui::{Sidebar, SidebarSessionMenu};
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let definition = sidebar
        .displayed_input()
        .unwrap()
        .rows
        .iter()
        .find(|r| &r.address == address)
        .unwrap()
        .actions
        .items
        .iter()
        .find(|a| a.id == action)
        .unwrap();
    let label = definition.label.clone();
    let root = sidebar.session_menu_id().unwrap();
    let menu = view
        .tree
        .get(root)
        .unwrap()
        .component::<SidebarSessionMenu>()
        .unwrap()
        .menu_id()
        .unwrap();
    paint(&mut view.tree);
    assert!(
        view.accessibility_action(menu, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    paint(&mut view.tree);
    let parts: Vec<_> = view
        .tree
        .semantics()
        .into_iter()
        .find(|n| n.id == menu)
        .unwrap()
        .parts
        .into_iter()
        .filter(|p| p.semantics.name == label)
        .collect();
    assert_eq!(parts.len(), 1, "fixture actions need distinct labels");
    let part = parts[0].index;
    let commands = view
        .accessibility_action(menu, Some(part), AccessibilityAction::Press)
        .unwrap();
    (commands, menu, part)
}
