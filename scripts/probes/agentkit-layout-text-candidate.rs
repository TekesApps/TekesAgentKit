// Appended to the unchanged formal RunView probe by --append-run-probe.
// No substitute Run/editor/reading controller or consumption-side repair.
#[derive(Default)]
struct WithoutTextServices {
    draws: usize,
}
impl ButtonTextRenderer for WithoutTextServices {
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
        self.draws += 1;
    }
}
fn missing_text(result: Result<(), CompositionError>) -> ComponentId {
    match result {
        Err(CompositionError::MissingTextServices(id)) => id,
        other => panic!("expected original active text service preflight: {other:?}"),
    }
}
#[tokio::test]
async fn actual_run_layout_reactivation_requires_services_then_recovers_original_editor_and_reading_anchor()
 {
    let mut cases = 0;
    for scale in [0.85, 1., 1.5] {
        for width in [420., 760.] {
            let (runtime, mut host, root, _, version, mut text) = setup().await;
            text.1 = scale;
            let full = Rect::new(0., 0., width, 650.);
            host.render(
                &mut Scene::new(),
                &mut text,
                full,
                full,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let editor = run(&host, root).ids().composer.editor;
            let widget =
                host.get(editor).unwrap().component::<AgentText>().unwrap() as *const _ as usize;
            let draft = editor_mut(&mut host, root).input.text.clone();
            let original = baseline(&host, root);
            let mut absent = WithoutTextServices::default();
            let mut rejected_scene = Scene::new();
            let required = missing_text(host.render(
                &mut rejected_scene,
                &mut absent,
                full,
                full,
                Theme::LIGHT.into(),
            ));
            assert!(host.text_surface(required).is_some());
            assert!(rejected_scene.is_empty());
            assert_eq!(absent.draws, 0);
            host.render(
                &mut Scene::new(),
                &mut text,
                full,
                full,
                Theme::LIGHT.into(),
            )
            .unwrap();
            wheel(&mut host, root, 420.);
            host.render(
                &mut Scene::new(),
                &mut text,
                full,
                full,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let item = anchor(&host, root);
            let y = host.get(item).unwrap().frame().min_y();
            let row_keys = rows(&host, root);
            for collapsed in [Rect::new(0., 0., 0., 650.), Rect::new(0., 0., width, 0.)] {
                // Entry-active collapse still requires the original text service.
                assert_eq!(
                    missing_text(host.render(
                        &mut Scene::new(),
                        &mut absent,
                        collapsed,
                        collapsed,
                        Theme::LIGHT.into()
                    )),
                    required
                );
                assert!(run(&host, root).is_pane_presented());
                host.render(
                    &mut Scene::new(),
                    &mut text,
                    collapsed,
                    collapsed,
                    Theme::LIGHT.into(),
                )
                .unwrap();
                assert!(!run(&host, root).is_pane_presented());
                host.render(
                    &mut Scene::new(),
                    &mut absent,
                    collapsed,
                    collapsed,
                    Theme::LIGHT.into(),
                )
                .unwrap();
                let mut scene = Scene::new();
                assert_eq!(
                    missing_text(host.render(
                        &mut scene,
                        &mut absent,
                        full,
                        full,
                        Theme::LIGHT.into()
                    )),
                    required
                );
                assert!(scene.is_empty());
                assert_eq!(absent.draws, 0);
                assert!(
                    run(&host, root).is_pane_presented(),
                    "application arrange changes remain applied"
                );
                assert_eq!(baseline(&host, root), original);
                assert_eq!(editor_mut(&mut host, root).input.text, draft);
                assert_eq!(
                    host.get(editor).unwrap().component::<AgentText>().unwrap() as *const _
                        as usize,
                    widget
                );
                assert_eq!(
                    scroll(&host, root).reading_document_version(),
                    Some(&version)
                );
                assert_eq!(host.focused(), None);
                host.render(
                    &mut Scene::new(),
                    &mut text,
                    full,
                    full,
                    Theme::LIGHT.into(),
                )
                .unwrap();
                assert_eq!(
                    host.focused(),
                    None,
                    "valid renderer recovery must not steal focus"
                );
                assert_eq!(baseline(&host, root), original);
                assert_eq!(rows(&host, root), row_keys);
                assert_eq!(
                    scroll(&host, root).reading_document_version(),
                    Some(&version)
                );
                assert!(!scroll(&host, root).is_following_latest());
                assert_eq!(anchor(&host, root), item);
                assert!(
                    (host.get(item).unwrap().frame().min_y() - y).abs() <= 1.,
                    "width={width} scale={scale}"
                );
                cases += 1;
            }
            runtime.stop().await.unwrap();
        }
    }
    assert_eq!(cases, 12);
}

#[tokio::test]
async fn actual_run_failed_width_change_retains_the_original_reading_item_on_valid_retry() {
    for scale in [0.85, 1., 1.5] {
        for (from, to) in [(760., 420.), (420., 760.)] {
            let (runtime, mut host, root, _, version, mut text) = setup().await;
            text.1 = scale;
            let before = Rect::new(0., 0., from, 650.);
            host.render(
                &mut Scene::new(),
                &mut text,
                before,
                before,
                Theme::LIGHT.into(),
            )
            .unwrap();
            wheel(&mut host, root, 420.);
            host.render(
                &mut Scene::new(),
                &mut text,
                before,
                before,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let item = anchor(&host, root);
            let y = host.get(item).unwrap().frame().min_y();
            let draft = baseline(&host, root);
            let collapsed = Rect::new(0., 0., 0., 650.);
            host.render(
                &mut Scene::new(),
                &mut text,
                collapsed,
                collapsed,
                Theme::LIGHT.into(),
            )
            .unwrap();
            let after = Rect::new(0., 0., to, 650.);
            let mut missing = WithoutTextServices::default();
            let mut scene = Scene::new();
            missing_text(host.render(&mut scene, &mut missing, after, after, Theme::LIGHT.into()));
            assert!(scene.is_empty());
            host.render(
                &mut Scene::new(),
                &mut text,
                after,
                after,
                Theme::LIGHT.into(),
            )
            .unwrap();
            assert_eq!(baseline(&host, root), draft);
            assert_eq!(
                scroll(&host, root).reading_document_version(),
                Some(&version)
            );
            assert!(!scroll(&host, root).is_following_latest());
            assert_eq!(
                anchor(&host, root),
                item,
                "from={from} to={to} scale={scale}"
            );
            assert!(
                (host.get(item).unwrap().frame().min_y() - y).abs() <= 1.,
                "from={from} to={to} scale={scale}"
            );
            runtime.stop().await.unwrap();
        }
    }
}
