//! Known foundation boundary, not AgentKit or ClientKit visual acceptance.
use tekes_ui_controls::{
    ButtonTextAlign, ButtonTextRenderer, composition::*, layout::MeasuredChild,
};
use tekes_ui_scene::{Point, Rect, Scene, Size};
use tekes_ui_tokens::{Rgba, Theme, proposal};
struct NoTextServices;
impl ButtonTextRenderer for NoTextServices {
    fn measure(&mut self, _: &str, _: proposal::TextRole, width: f32) -> Size {
        Size::new(width.min(100.), 20.)
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
}
#[derive(Debug)]
struct LayoutActive {
    active: bool,
    children: Vec<Content>,
}
impl Component for LayoutActive {
    fn active(&self) -> bool {
        self.active
    }
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.active = frame.size.width > 0. && frame.size.height > 0.;
        if self.active {
            self.children[0].arrange(text, frame, clip);
        }
    }
}
#[test]
fn known_boundary_layout_reactivation_bypasses_missing_text_services_preflight() {
    let frame = Rect::new(0., 0., 400., 300.);
    let mut renderer = NoTextServices;
    let mut active = ContentHost::new(Content::new(
        ComponentId(1),
        TextFieldContent::new("field", "must shape"),
    ))
    .unwrap();
    assert!(matches!(
        active.render(
            &mut Scene::new(),
            &mut renderer,
            frame,
            frame,
            Theme::LIGHT.into()
        ),
        Err(CompositionError::MissingTextServices(_))
    ));
    let mut dynamic = ContentHost::new(Content::new(
        ComponentId(10),
        LayoutActive {
            active: false,
            children: vec![Content::new(
                ComponentId(11),
                TextFieldContent::new("field", "must shape"),
            )],
        },
    ))
    .unwrap();
    let collapsed = Rect::new(0., 0., 0., 300.);
    assert!(
        dynamic
            .render(
                &mut Scene::new(),
                &mut renderer,
                collapsed,
                collapsed,
                Theme::LIGHT.into()
            )
            .is_ok()
    );
    let reopened = dynamic.render(
        &mut Scene::new(),
        &mut renderer,
        frame,
        frame,
        Theme::LIGHT.into(),
    );
    // Record the existing gap, not the desired acceptance. Desired: same
    // MissingTextServices as the always-active owner, before input publication.
    assert!(
        reopened.is_ok(),
        "known boundary changed; re-evaluate upstream contract: {reopened:?}"
    );
    assert!(dynamic.text_surface(ComponentId(11)).is_some());
    let _ = Point::new(0., 0.);
}
