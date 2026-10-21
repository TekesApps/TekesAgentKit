// Receiver-only #35 integration on actual AgentKit Sidebar/ComponentDemo.
// The original SDK owns declarations, dwell, suppression, routing and painting.
struct HelpClockText {
    inner: Text,
    millis: u64,
}
impl TextSurfaceTextRenderer for HelpClockText {
    fn push_visible(&mut self, _: &mut Scene, _: &Paragraph, _: Point, _: Rgba, _: Rect) {}
}
impl TextSurfaceHost for HelpClockText {
    fn shape_surface(&mut self, s: &str, role: proposal::TextRole, width: Option<f32>) -> Rc<Paragraph> {
        self.inner.shape_surface(s, role, width)
    }
    fn shape_label(&mut self, s: &str, role: proposal::TextRole, width: f32, options: tekes_ui_text::LabelOptions) -> Option<Rc<tekes_ui_text::LabelLayout>> {
        let metrics = role.style();
        Some(Rc::new(self.inner.0.shape_label(s, &TextStyle::system(metrics.size).with_weight(metrics.weight), &ShapeOptions::with_width(f64::from(width)), options).with_min_line_height(f64::from(metrics.line_height))))
    }
}
impl ButtonTextRenderer for HelpClockText {
    fn measure(&mut self, s: &str, role: proposal::TextRole, width: f32) -> Size { self.inner.measure(s, role, width) }
    fn draw(&mut self, scene: &mut Scene, s: &str, role: proposal::TextRole, frame: Rect, align: ButtonTextAlign, color: Rgba) { self.inner.draw(scene, s, role, frame, align, color); }
    fn text_surface_host(&mut self) -> Option<&mut dyn TextSurfaceHost> { Some(self) }
    fn motion_context(&self) -> Option<tekes_ui_controls::ControlMotionContext> {
        Some(tekes_ui_controls::ControlMotionContext { now: std::time::Duration::from_millis(self.millis), reduced_motion: false })
    }
}
fn help_row(id: &str) -> SidebarRow {
    SidebarRow { primary_folder_id: None, folder_path: Some("/fixture/原会话目录".into()), address: RunScope::main("fixture", id).address(), title: format!("原会话 {id}"), activity: ThreadActivity { status: "idle".into(), is_stale: false }, pin: None, actions: ThreadActionsPresentation { endpoint: "fixture".into(), session_id: id.into(), label: "操作".into(), items: vec![] } }
}
fn help_search(revision: u64, query: &str, ids: &[&str]) -> tekes_agent_ui_bindings::SidebarSearchPublication {
    use tekes_agent_ui_bindings::{SidebarSearchMatch, SidebarSearchPublication};
    SidebarSearchPublication {
        input: SidebarSearchInput { revision, key: Some(SidebarSearchKey { owner: "receiver-help".into(), endpoint_id: "fixture".into(), connection_epoch: 1, generation: revision, query: query.into() }), phase: SidebarSearchPhase::Complete { has_more: false } },
        query: query.into(),
        hits: ids.iter().map(|id| SidebarSearchMatch { address: RunScope::main("fixture", *id).address(), hit: tekes_agent_contract::SessionSearchHit { session_id: (*id).into(), snippet: format!("{query} · 当前中文摘要 {id}"), extra: Default::default() } }).collect(),
        unresolved: vec![], result: None,
    }
}
async fn help_demo() -> (std::sync::Arc<tekes_agent_session::SessionRuntime>, tekes_agent_ui_demo::component_demo::ComponentDemo, HelpClockText) {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::{RunBinding, SidebarPublication};
    assert!(sidebar_row_help_supported());
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a")).await.unwrap());
    let mut demo = tekes_agent_ui_demo::component_demo::ComponentDemo::new([binding]).unwrap();
    demo.enable_transcript_content().unwrap();
    demo.set_sidebar_source(Arc::new(SidebarPublication { activity_at: Default::default(), revision: 1, loaded: true, error: None, rows: vec![help_row("a"), help_row("b")] })).unwrap();
    demo.bind_sidebar_search_publication("receiver-help", &help_search(1, "A", &["a", "b"])).unwrap();
    let mut text = HelpClockText { inner: Text(CoreTextShaper::new(CoreTextFonts::new()), 1.), millis: 0 };
    help_render(&mut demo, &mut text);
    (runtime, demo, text)
}
fn help_render(demo: &mut tekes_agent_ui_demo::component_demo::ComponentDemo, text: &mut HelpClockText) {
    let frame = Rect::new(0., 0., tekes_agent_ui_demo::component_demo::ComponentDemo::minimum_window_width() as f32, 760.);
    demo.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into()).unwrap();
}
fn help_owner(demo: &tekes_agent_ui_demo::component_demo::ComponentDemo, id: &str) -> ComponentId {
    demo.tree.get(demo.sidebar_id()).unwrap().component::<Sidebar>().unwrap().row_id(&RunScope::main("fixture", id).address()).unwrap()
}
fn help_hover(demo: &mut tekes_agent_ui_demo::component_demo::ComponentDemo, text: &mut HelpClockText, row: ComponentId) {
    let point = demo.tree.get(row).unwrap().visible().unwrap().center();
    assert!(demo.event_with_text(&ContentEvent::PointerMove(point), text).unwrap().is_empty());
    help_render(demo, text); text.millis += 500; help_render(demo, text);
    assert_eq!(demo.tree.help_tooltip().unwrap().0, row);
}
#[tokio::test]
async fn real_sidebar_help_reuses_original_owner_without_disturbing_composer_preedit() {
    let (runtime, mut demo, mut text) = help_demo().await;
    let editor = demo.ids().editor;
    demo.tree.focus(Some(editor), true);
    demo.event_with_text(&ContentEvent::Preedit { text: "中文组字".into(), selection: 1..3, clauses: vec![], replacement: None }, &mut text).unwrap();
    help_render(&mut demo, &mut text);
    let row = help_owner(&demo, "a");
    let widget = demo.tree.get(row).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize;
    let nodes = demo.tree.semantics().len();
    let focus = demo.tree.capture_focus(); let marked = demo.editor().input.marked.clone(); let baseline = demo.editor().input.external_baseline();
    help_hover(&mut demo, &mut text, row);
    assert_eq!(demo.tree.help_tooltip().unwrap().1, "A · 当前中文摘要 a");
    assert_eq!(demo.tree.semantics().len(), nodes);
    assert_eq!(demo.tree.semantics().iter().find(|node| node.id == row).unwrap().semantics.description.as_deref(), Some("A · 当前中文摘要 a"));
    assert_eq!(demo.tree.get(row).unwrap().component::<ButtonContent>().unwrap() as *const _ as usize, widget);
    assert_eq!(demo.tree.capture_focus(), focus); assert_eq!(demo.editor().input.marked, marked); assert_eq!(demo.editor().input.external_baseline(), baseline);
    assert!(demo.binding().current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn query_change_and_clear_revoke_old_tooltip_and_keep_original_row() {
    let (runtime, mut demo, mut text) = help_demo().await;
    let row = help_owner(&demo, "a"); help_hover(&mut demo, &mut text, row);
    demo.update_sidebar_search_publication("receiver-help", &help_search(2, "B", &["a", "b"])).unwrap();
    assert!(demo.tree.help_tooltip().is_none());
    assert_eq!(help_owner(&demo, "a"), row);
    help_render(&mut demo, &mut text); help_hover(&mut demo, &mut text, row);
    assert_eq!(demo.tree.help_tooltip().unwrap().1, "B · 当前中文摘要 a");
    let mut clear = help_search(3, "", &[]); clear.input.key = None; clear.input.phase = SidebarSearchPhase::Idle;
    demo.update_sidebar_search_publication("receiver-help", &clear).unwrap();
    assert!(demo.tree.help_tooltip().is_none()); help_render(&mut demo, &mut text);
    assert_eq!(demo.tree.get(row).unwrap().help_text(), Some("/fixture/原会话目录"));
    assert_eq!(help_owner(&demo, "a"), row);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn escape_then_same_owner_query_help_update_stays_dismissed_until_fresh_input() {
    let (runtime, mut demo, mut text) = help_demo().await;
    let row = help_owner(&demo, "a"); help_hover(&mut demo, &mut text, row);
    demo.event_with_text(&ContentEvent::Key { key: tekes_ui_controls::ControlKey::Escape, down: true }, &mut text).unwrap();
    assert!(demo.tree.help_tooltip().is_none());
    demo.update_sidebar_search_publication("receiver-help", &help_search(2, "B", &["a", "b"])).unwrap();
    text.millis += 1500; help_render(&mut demo, &mut text);
    assert!(demo.tree.help_tooltip().is_none(), "metadata is not a fresh hover/focus gesture");
    let point = demo.tree.get(row).unwrap().visible().unwrap().center();
    demo.event_with_text(&ContentEvent::PointerMove(point), &mut text).unwrap();
    text.millis += 500; help_render(&mut demo, &mut text);
    assert!(demo.tree.help_tooltip().is_none(), "moving within the dismissed owner is not a new hover request");
    demo.event_with_text(&ContentEvent::PointerMove(Point::new(-1., -1.)), &mut text).unwrap();
    help_hover(&mut demo, &mut text, row);
    assert_eq!(demo.tree.help_tooltip().unwrap().1, "B · 当前中文摘要 a");
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn filtered_row_removal_and_reappearance_do_not_reuse_old_help_owner_or_selection_action() {
    let (runtime, mut demo, mut text) = help_demo().await;
    let a = help_owner(&demo, "a"); help_hover(&mut demo, &mut text, a);
    demo.tree.accessibility_action(a, None, AccessibilityAction::Press);
    let old = demo.tree.drain_actions();
    help_hover(&mut demo, &mut text, a);
    demo.update_sidebar_search_publication("receiver-help", &help_search(2, "B", &["b"])).unwrap();
    assert!(demo.tree.help_tooltip().is_none());
    assert!(demo.tree.get(a).is_none());
    assert!(demo.tree.get(demo.sidebar_id()).unwrap().component::<Sidebar>().unwrap().take_intents(&old).is_empty());
    demo.update_sidebar_search_publication("receiver-help", &help_search(3, "C", &["a"])).unwrap();
    help_render(&mut demo, &mut text); let returned = help_owner(&demo, "a");
    assert_ne!(returned, a); assert!(demo.tree.help_tooltip().is_none());
    help_hover(&mut demo, &mut text, returned);
    assert_eq!(demo.tree.help_tooltip().unwrap().1, "C · 当前中文摘要 a");
    assert_eq!(demo.selected(), 0);
    assert!(demo.binding().current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
