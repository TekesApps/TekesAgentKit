#![cfg(target_os = "macos")]
use std::rc::Rc;
use tekes_agent_components_ui::*;
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

#[tokio::test]
async fn sidebar_original_item_reveal_shows_selected_supplement_without_focusing_or_replaying() {
    use tekes_ui_controls::extended::{Axis, Stack};
    let address = |i| RunScope::main("sidebar-fixture", format!("row-{i}")).address();
    let mut input = SidebarInput {
        revision: 1,
        loaded: true,
        error: None,
        folder_filter: None,
        search_help: None,
        selected: Some(address(99)),
        rows: (0..100)
            .map(|i| SidebarRow {
                primary_folder_id: None,
                folder_path: None,
                address: address(i),
                title: format!("会话 {i}"),
                activity: ThreadActivity {
                    status: "idle".into(),
                    is_stale: false,
                },
                pin: None,
                actions: ThreadActionsPresentation {
                    endpoint: "sidebar-fixture".into(),
                    session_id: format!("row-{i}"),
                    label: "操作".into(),
                    items: vec![],
                },
            })
            .collect(),
    };
    let mut sidebar = Sidebar::new(900, Default::default());
    sidebar.update(&input).unwrap();
    sidebar
        .reconcile_navigation(&SidebarSelectionNavigation {
            revision: 1,
            selected: input.selected.clone(),
        })
        .unwrap();
    let target = tekes_agent_ui_consumer_check::sidebar_visibility_target(&sidebar).unwrap();
    let sidebar_root = sidebar.id();
    let ids = ComposerIds::new(901u64 << 32);
    let mut composer = Composer::new(ids);
    let (_runtime, controller) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    composer
        .update(&controller.snapshot(), Default::default())
        .unwrap();
    let root = ComponentId(902u64 << 32);
    let mut host = ContentHost::new(Content::new(
        root,
        StackContent {
            model: Stack {
                axis: Axis::Vertical,
                gap: 0.,
            },
            flexible_child: Some(sidebar_root),
            children: vec![
                Content::new(sidebar_root, sidebar),
                Content::new(ids.root, composer),
            ],
        },
    ))
    .unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    let paint = |host: &mut ContentHost, text: &mut Text| {
        let frame = Rect::new(0., 0., 380., 260.);
        host.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into())
            .unwrap();
    };
    host.focus(Some(ids.editor), true);
    host.event_with_text(
        &ContentEvent::Preedit {
            text: "中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None,
        },
        &mut text,
    )
    .unwrap();
    paint(&mut host, &mut text);
    let before = host.focused_text().unwrap().input.external_baseline();
    let (version, ()) = host
        .set_scroll_reading_document(
            target.viewport(),
            "sidebar-selected-fixture",
            1,
            vec![target.item()],
            |_| {},
        )
        .unwrap();
    host.get_mut(target.viewport())
        .unwrap()
        .component_mut::<ScrollContent>()
        .unwrap()
        .stop_following();
    paint(&mut host, &mut text);
    let was_visible = host.get(target.item()).unwrap().visible();
    assert!(
        was_visible.is_none(),
        "fixture target must start below the viewport: {was_visible:?}"
    );
    input.revision += 1;
    host.get_mut(sidebar_root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(matches!(
        tekes_agent_ui_consumer_check::reveal_sidebar_selection(
            &mut host,
            &target,
            &version,
            Default::default(),
            &mut text
        ),
        Err(SidebarRevealError::Selection(_))
    ));
    assert_eq!(
        host.get(target.viewport())
            .unwrap()
            .component::<ScrollContent>()
            .unwrap()
            .reading_document_version(),
        Some(&version)
    );
    let target = host
        .get(sidebar_root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .visibility_target()
        .unwrap();
    paint(&mut host, &mut text);
    let result = tekes_agent_ui_consumer_check::reveal_sidebar_selection(
        &mut host,
        &target,
        &version,
        tekes_ui_controls::ScrollRevealOptions {
            animated: false,
            ..Default::default()
        },
        &mut text,
    )
    .unwrap();
    assert_eq!(
        result.status,
        tekes_ui_controls::composition::ContentRevealStatus::Completed
    );
    paint(&mut host, &mut text);
    assert!(host.get(target.item()).unwrap().visible().is_some());
    assert_eq!(host.focused(), Some(ids.editor));
    let after = host.focused_text().unwrap().input.external_baseline();
    assert_eq!(before, after);
    let view = host
        .get(sidebar_root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(view.visibility_target().is_none());
    assert!(tekes_agent_ui_consumer_check::claim_sidebar_visibility(view, &target).is_err());
    assert_eq!(view.paging().limit, 5);
    assert!(
        tekes_agent_ui_consumer_check::reveal_sidebar_selection(
            &mut host,
            &target,
            &version,
            Default::default(),
            &mut text
        )
        .is_err()
    );
}
