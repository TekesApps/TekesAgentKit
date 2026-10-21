// Appended to the unchanged Run/rail probes. Uses the actual Composer subtree;
// bypasses the diagnostic host IME policy only to test the original SDK route.
// This is component routing evidence, not native IME-barrier acceptance (#34).
#[tokio::test]
async fn actual_composer_padding_focus_preserves_original_editor_preedit_and_actions() {
    let (runtime, mut host, root, _, _, mut text) = setup().await;
    let ids = run(&host, root).ids().composer;
    host.set_padding_focus(ids.input_band, Some(ids.editor)).unwrap();
    assert!(host.set_padding_focus(ids.toolbar, Some(ids.editor)).is_err());
    let band = host.get(ids.input_band).unwrap().frame();
    let point = Point::new(band.min_x() + 2., band.center().y);
    assert!(!host.get(ids.editor).unwrap().frame().contains(point));
    assert!(!host.get(ids.send).unwrap().frame().contains(point));
    host.focus(None, false);
    let before = baseline(&host, root);
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    assert_eq!(host.focused(), Some(ids.editor));
    assert_eq!(baseline(&host, root), before);
    assert!(host.drain_actions().is_empty());
    host.event_with_text(&ContentEvent::Preedit {
        text: "中文组字".into(), selection: 1..3, clauses: vec![], replacement: None,
    }, &mut text).unwrap();
    render(&mut host, &mut text);
    host.drain_actions();
    let before = baseline(&host, root);
    let marked = run(&host, root).composer().editor().input.marked.clone();
    let focus = host.capture_focus();
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    assert_eq!(host.capture_focus(), focus);
    assert_eq!(baseline(&host, root), before);
    assert_eq!(run(&host, root).composer().editor().input.marked, marked);
    assert!(host.drain_actions().is_empty());
    // A real input action wins over the parent's decorative focus declaration.
    host.event(&ContentEvent::Unmark);
    render(&mut host, &mut text);
    host.drain_actions();
    let primary = host.get(ids.send).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(primary));
    host.event(&ContentEvent::PointerUp(primary));
    let actions = host.drain_actions();
    let intents = run_mut(&mut host, root).take_intents(&actions);
    assert_eq!(intents.len(), 1);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn actual_composer_padding_binding_is_optional_and_does_not_steal_disabled_actions() {
    let (runtime, mut host, root, mut snapshot, _, mut text) = setup().await;
    let ids = run(&host, root).ids().composer;
    host.set_padding_focus(ids.input_band, Some(ids.editor)).unwrap();
    snapshot.revision += 1;
    run_mut(&mut host, root).composer_mut().update_gated(
        &snapshot, Default::default(), ComposerControlGate::CLOSED,
    ).unwrap();
    render(&mut host, &mut text);
    host.focus(None, false);
    let primary = host.get(ids.send).unwrap().frame().center();
    host.event(&ContentEvent::PointerDown(primary));
    host.event(&ContentEvent::PointerUp(primary));
    assert_eq!(host.focused(), None);
    assert!(host.drain_actions().is_empty());
    host.set_padding_focus(ids.input_band, None).unwrap();
    let band = host.get(ids.input_band).unwrap().frame();
    let point = Point::new(band.min_x() + 2., band.center().y);
    host.event(&ContentEvent::PointerDown(point));
    host.event(&ContentEvent::PointerUp(point));
    assert_eq!(host.focused(), None);
    runtime.stop().await.unwrap();
}
