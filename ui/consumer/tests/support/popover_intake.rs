//! Public-SDK intake probe, not an AgentKit popup implementation. Deliberately
//! no second ContentHost, manual pointer forwarding or substitute focus engine.
use super::*;
use tekes_ui_controls::{
    extended::Popover,
    feedback_view::{OverlayEvent, PopoverView},
    layout::MeasuredChild,
};

struct Adapter<'a>(&'a mut dyn ButtonTextRenderer);
impl ButtonTextRenderer for Adapter<'_> {
    fn measure(&mut self, text: &str, role: proposal::TextRole, width: f32) -> Size {
        self.0.measure(text, role, width)
    }
    fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        role: proposal::TextRole,
        frame: Rect,
        align: ButtonTextAlign,
        color: Rgba,
    ) {
        self.0.draw(scene, text, role, frame, align, color)
    }
}

#[derive(Debug)]
struct Popup {
    model: Popover,
    view: PopoverView,
    children: Vec<Content>,
    painted: bool,
}
impl Component for Popup {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn paints_children(&self) -> bool {
        true
    }
    fn overlay_open(&self) -> bool {
        self.model.open
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, anchor: Rect, clip: Rect) {
        let layout = self.view.arrange(
            &mut Adapter(text),
            &self.model,
            anchor,
            Size::new(360., 220.),
            clip,
            Some(clip),
            proposal::TextRole::UiBody,
        );
        self.children[0].arrange(text, layout.content, clip);
    }
    fn paint_overlay(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        anchor: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        let children = &mut self.children;
        let painted = &mut self.painted;
        self.view.render_with_text_content(
            scene,
            &mut Adapter(text),
            &self.model,
            anchor,
            Size::new(360., 220.),
            clip,
            Some(clip),
            theme.palette,
            proposal::TextRole::UiBody,
            "original-content-child",
            |scene, text, _| {
                children[0].paint(scene, text, theme);
                *painted = true;
            },
        );
    }
    fn overlay_event(&mut self, event: &ContentEvent) -> ContentResponse {
        match event {
            ContentEvent::PointerDown(p) if self.view.pointer_down(&mut self.model, *p) => {
                return ContentResponse::consumed(false);
            }
            ContentEvent::PointerUp(p)
                if matches!(
                    self.view.pointer_up(&mut self.model, *p),
                    OverlayEvent::Dismissed { .. }
                ) =>
            {
                return ContentResponse::consumed(true);
            }
            _ => {}
        }
        ContentResponse::default()
    }
}

#[derive(Debug)]
struct Overlap {
    children: Vec<Content>,
}
impl Component for Overlap {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, _: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, _: Rect, clip: Rect) {
        self.children[0].arrange(text, Rect::new(20., 20., 80., 30.), clip);
        // Fixture background uses the SDK's actual popup content geometry.
        // Both original Content controls are visible; popup paint is last.
        let content = self.children[0]
            .component::<Popup>()
            .unwrap()
            .view
            .layout()
            .content;
        self.children[1].arrange(text, content, clip);
    }
}

#[test]
#[ignore = "TekesUI #49: public interactive Popover child routing intake gate"]
fn original_popover_content_child_wins_over_background_sibling() {
    let popup = ComponentId(51001);
    let child = ComponentId(51002);
    let background = ComponentId(51003);
    let mut model = Popover::default();
    model.show(1);
    let mut host = ContentHost::new(Content::new(
        ComponentId(51000),
        Overlap {
            children: vec![
                Content::new(
                    popup,
                    Popup {
                        model,
                        view: PopoverView::default(),
                        children: vec![Content::new(child, Button::new("Popup Save"))],
                        painted: false,
                    },
                ),
                Content::new(background, Button::new("Background Action")),
            ],
        },
    ))
    .unwrap();
    host.set_action_events(true);
    let mut text = Text::new(1., true);
    let frame = Rect::new(0., 0., 600., 500.);
    host.render(
        &mut Scene::new(),
        &mut text,
        frame,
        frame,
        Theme::LIGHT.into(),
    )
    .unwrap();
    let popup = host.get(popup).unwrap().component::<Popup>().unwrap();
    let point = host.get(child).unwrap().visible().unwrap().center();
    assert!(popup.painted && popup.view.layout().visible.unwrap().contains(point));
    assert!(
        host.get(background)
            .unwrap()
            .visible()
            .unwrap()
            .contains(point)
    );
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    let targets = host
        .drain_actions()
        .into_iter()
        .map(|action| action.target)
        .collect::<Vec<_>>();
    println!(
        "PUBLIC_POPOVER_INTAKE expected={child:?} actual={targets:?} background={background:?}"
    );
    assert_eq!(
        targets,
        vec![child],
        "painted popup must own its original Content action before the underlying document"
    );
}
