// Actual ComponentDemo policy, not direct-to-SDK routing. Appended to the
// unchanged Run/rail/Start probes on an explicitly fixed #34 candidate.
async fn padding_demo() -> (std::sync::Arc<tekes_agent_session::SessionRuntime>, tekes_agent_ui_demo::component_demo::ComponentDemo, Text) {
    use std::sync::Arc;
    use tekes_agent_ui_bindings::RunBinding;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let a = Arc::new(RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a")).await.unwrap());
    let b = Arc::new(RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b")).await.unwrap());
    let mut demo = tekes_agent_ui_demo::component_demo::ComponentDemo::new([a, b]).unwrap();
    demo.enable_transcript_content().unwrap();
    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), 1.);
    padding_demo_render(&mut demo, &mut text);
    (runtime, demo, text)
}
fn padding_demo_render(demo: &mut tekes_agent_ui_demo::component_demo::ComponentDemo, text: &mut Text) {
    let frame = Rect::new(10., 10., tekes_agent_ui_demo::component_demo::ComponentDemo::minimum_window_width() as f32, 740.);
    demo.render(&mut Scene::new(), text, frame, frame, Theme::LIGHT.into()).unwrap();
}
fn padding_point(demo: &tekes_agent_ui_demo::component_demo::ComponentDemo) -> Point {
    let ids = demo.ids();
    let band = demo.tree.get(ids.input_band).unwrap().frame();
    let point = Point::new(band.min_x() + 2., band.center().y);
    assert!(!demo.tree.get(ids.editor).unwrap().frame().contains(point));
    point
}
#[tokio::test]
async fn real_host_forwards_same_editor_padding_during_preedit_but_blocks_send_and_selection() {
    use tekes_agent_ui_demo::component_demo::SELECT_B;
    let (runtime, mut demo, mut text) = padding_demo().await;
    let ids = demo.ids();
    demo.tree.focus(Some(ids.editor), true);
    assert!(demo.event_with_text(&ContentEvent::Preedit {
        text: "中文组字".into(), selection: 1..3, clauses: vec![], replacement: None,
    }, &mut text).unwrap().is_empty());
    padding_demo_render(&mut demo, &mut text);
    let event = ContentEvent::PointerDown(padding_point(&demo));
    let focus = demo.tree.capture_focus();
    let baseline = demo.editor().input.external_baseline();
    let marked = demo.editor().input.marked.clone();
    assert_eq!(demo.tree.padding_focus_target(&event), Some(ids.editor));
    // Query itself is observational, and the real host sends the original event.
    assert_eq!(demo.tree.capture_focus(), focus);
    assert_eq!(demo.editor().input.external_baseline(), baseline);
    assert!(demo.event_with_text(&event, &mut text).unwrap().is_empty());
    assert!(demo.event_with_text(&ContentEvent::PointerUp(padding_point(&demo)), &mut text).unwrap().is_empty());
    assert_eq!(demo.tree.capture_focus(), focus);
    assert_eq!(demo.editor().input.external_baseline(), baseline);
    assert_eq!(demo.editor().input.marked, marked);
    for id in [ids.send, SELECT_B] {
        let point = demo.tree.get(id).unwrap().frame().center();
        let down = ContentEvent::PointerDown(point);
        assert_eq!(demo.tree.padding_focus_target(&down), None);
        assert!(demo.event_with_text(&down, &mut text).unwrap().is_empty());
        assert!(demo.event_with_text(&ContentEvent::PointerUp(point), &mut text).unwrap().is_empty());
    }
    assert_eq!(demo.selected(), 0);
    assert_eq!(demo.tree.capture_focus(), focus);
    assert_eq!(demo.editor().input.external_baseline(), baseline);
    assert_eq!(demo.editor().input.marked, marked);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn real_host_rebinds_returned_session_editor_only_after_successful_render() {
    let (runtime, mut demo, mut text) = padding_demo().await;
    let a = demo.ids();
    demo.tree.focus(Some(a.editor), true);
    demo.event_with_text(&ContentEvent::Commit { text: "A 草稿".into(), replacement: None }, &mut text).unwrap();
    padding_demo_render(&mut demo, &mut text);
    demo.select(1).unwrap();
    padding_demo_render(&mut demo, &mut text);
    let b = demo.ids();
    assert_ne!(a.editor, b.editor);
    assert_eq!(demo.tree.padding_focus_target(&ContentEvent::PointerDown(padding_point(&demo))), Some(b.editor));
    demo.event_with_text(&ContentEvent::Commit { text: "B 草稿".into(), replacement: None }, &mut text).unwrap();
    demo.select(0).unwrap();
    let point = padding_point(&demo);
    assert_eq!(demo.tree.padding_focus_target(&ContentEvent::PointerDown(point)), None);
    padding_demo_render(&mut demo, &mut text);
    assert_eq!(demo.tree.padding_focus_target(&ContentEvent::PointerDown(padding_point(&demo))), Some(a.editor));
    assert_eq!(demo.editor().input.text, "A 草稿");
    demo.tree.focus(None, false);
    let point = padding_point(&demo);
    demo.event_with_text(&ContentEvent::PointerDown(point), &mut text).unwrap();
    demo.event_with_text(&ContentEvent::PointerUp(point), &mut text).unwrap();
    assert_eq!(demo.tree.focused(), Some(a.editor));
    assert_eq!(demo.editor().input.text, "A 草稿");
    demo.select(1).unwrap();
    padding_demo_render(&mut demo, &mut text);
    assert_eq!(demo.editor().input.text, "B 草稿");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn real_host_does_not_bind_a_collapsed_pane_and_restores_declaration_on_reopen() {
    let (runtime, mut demo, mut text) = padding_demo().await;
    let before = demo.editor().input.text.clone();
    let hidden = Rect::new(10., 10., tekes_agent_ui_demo::component_demo::ComponentDemo::minimum_window_width() as f32, 0.);
    demo.render(&mut Scene::new(), &mut text, hidden, hidden, Theme::LIGHT.into()).unwrap();
    assert_eq!(demo.tree.padding_focus_target(&ContentEvent::PointerDown(padding_point(&demo))), None);
    padding_demo_render(&mut demo, &mut text);
    assert_eq!(demo.tree.padding_focus_target(&ContentEvent::PointerDown(padding_point(&demo))), Some(demo.ids().editor));
    assert_eq!(demo.editor().input.text, before);
    runtime.stop().await.unwrap();
}
