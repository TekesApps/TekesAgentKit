use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;
fn key(query: &str, generation: u64) -> SidebarSearchKey {
    SidebarSearchKey {
        owner: "owned-source".into(),
        endpoint_id: "dsh".into(),
        connection_epoch: 7,
        generation,
        query: query.into(),
    }
}
fn failed(revision: u64, generation: u64) -> SidebarSearchInput {
    SidebarSearchInput {
        revision,
        key: Some(key("中文 👩🏽‍💻", generation)),
        phase: SidebarSearchPhase::Failed {
            message: "搜索暂不可用".into(),
            has_more: true,
            retryable: true,
        },
    }
}
fn update(
    host: &mut ContentHost,
    root: ComponentId,
    input: &SidebarSearchInput,
) -> Result<(), &'static str> {
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update_search(input)
}
fn intents(
    host: &ContentHost,
    root: ComponentId,
    actions: &[ContentAction],
) -> Vec<SidebarSearchIntent> {
    host.get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_search_intents(actions)
}
#[test]
fn footer_tracks_search_without_rewriting_inventory_and_retires_original_retry_gestures() {
    let mut sidebar = Sidebar::new(48, SidebarLabels::default());
    sidebar
        .update(&SidebarInput {
            folder_filter: None,
            search_help: None,
            revision: 1,
            loaded: false,
            error: None,
            rows: vec![],
            selected: None,
        })
        .unwrap();
    let first = failed(1, 1);
    sidebar.update_search(&first).unwrap();
    let root = sidebar.id();
    let retry = sidebar.search_retry_id().unwrap();
    let mut host = sidebar.into_host().unwrap();
    let owner = host
        .get(retry)
        .unwrap()
        .component::<ButtonContent>()
        .unwrap() as *const _ as usize;
    host.accessibility_action(retry, None, AccessibilityAction::Press);
    let captured = host.drain_actions();
    assert_eq!(
        intents(&host, root, &captured),
        vec![SidebarSearchIntent::Retry {
            key: first.key.clone().unwrap()
        }]
    );
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    assert!(sidebar.set_search_retry_pending(first.key.as_ref().unwrap(), true));
    assert!(intents(&host, root, &captured).is_empty());
    let second = SidebarSearchInput {
        revision: 2,
        ..first.clone()
    };
    update(&mut host, root, &second).unwrap();
    assert_eq!(
        host.get(retry)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap() as *const _ as usize,
        owner
    );
    assert!(
        host.get(retry)
            .unwrap()
            .component::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled
    );
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .revision,
        1
    );
    let mut searching = first.clone();
    searching.revision = 3;
    searching.key.as_mut().unwrap().generation = 2;
    searching.phase = SidebarSearchPhase::Searching;
    update(&mut host, root, &searching).unwrap();
    assert!(host.get(retry).is_none());
    assert!(intents(&host, root, &captured).is_empty());
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "正在搜索…")
    );
    let next = failed(4, 2);
    update(&mut host, root, &next).unwrap();
    let next_id = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .search_retry_id()
        .unwrap();
    assert_ne!(retry, next_id);
    assert!(intents(&host, root, &captured).is_empty());
    update(
        &mut host,
        root,
        &SidebarSearchInput {
            revision: 5,
            key: None,
            phase: SidebarSearchPhase::Idle,
        },
    )
    .unwrap();
    assert!(host.get(next_id).is_none());
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "会话搜索状态")
    );
    assert_eq!(
        update(&mut host, root, &failed(6, 2)),
        Err("retired sidebar search attempt")
    );
    update(&mut host, root, &failed(6, 3)).unwrap();
}
#[test]
fn publication_identity_conflicts_invalid_sources_and_epoch_aba_never_replace_the_tree() {
    let mut sidebar = Sidebar::new(49, SidebarLabels::default());
    let first = failed(9, 1);
    sidebar.update_search(&first).unwrap();
    let id = sidebar.search_retry_id();
    let mut bad = first.clone();
    bad.phase = SidebarSearchPhase::Searching;
    assert_eq!(
        sidebar.update_search(&bad),
        Err("conflicting sidebar search publication")
    );
    bad.revision = 8;
    assert_eq!(
        sidebar.update_search(&bad),
        Err("stale sidebar search input")
    );
    bad.revision = 10;
    bad.key = None;
    assert_eq!(
        sidebar.update_search(&bad),
        Err("search status has no source")
    );
    bad = first.clone();
    bad.revision = 10;
    bad.key.as_mut().unwrap().query = "other query".into();
    assert_eq!(
        sidebar.update_search(&bad),
        Err("retired sidebar search attempt")
    );
    bad.key.as_mut().unwrap().generation = 2;
    bad.key.as_mut().unwrap().connection_epoch = 6;
    assert_eq!(
        sidebar.update_search(&bad),
        Err("retired sidebar search attempt")
    );
    assert_eq!(sidebar.displayed_search(), Some(&first));
    assert_eq!(sidebar.search_retry_id(), id);
}
#[test]
fn only_matching_search_status_is_visible_and_retry_requires_explicit_capability() {
    let mut sidebar = Sidebar::new(50, SidebarLabels::default());
    let root = sidebar.id();
    let mut input = failed(1, 1);
    input.phase = SidebarSearchPhase::Failed {
        message: "不支持会话内容搜索".into(),
        has_more: false,
        retryable: false,
    };
    sidebar.update_search(&input).unwrap();
    assert!(sidebar.search_retry_id().is_none());
    let mut host = sidebar.into_host().unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "不支持会话内容搜索")
    );
    input.revision += 1;
    input.phase = SidebarSearchPhase::Complete { has_more: true };
    update(&mut host, root, &input).unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "还有更多匹配结果，请缩小搜索范围。")
    );
    input.revision += 1;
    input.phase = SidebarSearchPhase::Complete { has_more: false };
    update(&mut host, root, &input).unwrap();
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "会话搜索状态")
    );
}

#[cfg(target_os = "macos")]
mod geometry {
    use super::*;
    use std::rc::Rc;
    use tekes_ui_controls::{
        ButtonTextAlign, ButtonTextRenderer, TextSurfaceHost, TextSurfaceTextRenderer,
    };
    use tekes_ui_scene::{Point, Rect, Scene, Size};
    use tekes_ui_text::{
        Paragraph,
        coretext::{CoreTextFonts, CoreTextShaper},
        model::ShapeOptions,
        style::TextStyle,
        traits::Shaper,
    };
    use tekes_ui_tokens::{Rgba, Spacing, Theme, proposal};
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

    #[test]
    fn footer_uses_source_insets_and_bottom_anchor_even_with_a_tall_failure() {
        for scale in [0.75, 1., 1.6, 2.] {
            for width in [140., 320.] {
                for theme in [Theme::LIGHT, Theme::DARK] {
                    let mut sidebar = Sidebar::new(51, SidebarLabels::default());
                    let mut input = failed(1, 1);
                    input.phase = SidebarSearchPhase::Failed {
                        message: "中文搜索失败：保留原会话列表，等待用户显式重试。".repeat(4),
                        has_more: true,
                        retryable: true,
                    };
                    sidebar.update_search(&input).unwrap();
                    let root = sidebar.id();
                    let footer_id = sidebar.search_footer_id();
                    let viewport = sidebar.inventory_viewport_id();
                    let region = sidebar.inventory_region_id();
                    let retry = sidebar.search_retry_id().unwrap();
                    let mut host = sidebar.into_host().unwrap();
                    let mut text = Text(CoreTextShaper::new(CoreTextFonts::new()), scale);
                    let frame = Rect::new(20., 40., width, 180.);
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    let footer = host.get(footer_id).unwrap().frame();
                    let button = host.get(retry).unwrap().frame();
                    assert!((footer.max_y() - frame.max_y()).abs() < 0.1);
                    assert!(
                        (button.min_x() - frame.min_x() - Spacing::Regular.value()).abs() < 0.1
                    );
                    assert!(
                        (frame.max_y() - button.max_y() - Spacing::Regular.value()).abs() < 0.1
                    );
                    assert!(host.get(retry).unwrap().visible().is_some());
                    if host.get(region).unwrap().frame().size.height == 0. {
                        assert!(host.get(viewport).unwrap().visible().is_none());
                        assert!(
                            host.semantics()
                                .iter()
                                .filter(|node| node.parent == Some(region))
                                .all(|node| node.visible.is_none())
                        );
                    }
                    assert!(
                        host.get(viewport).unwrap().frame().max_y()
                            <= footer.min_y().max(frame.min_y()) + 0.1
                    );
                    host.accessibility_action(retry, None, AccessibilityAction::Press);
                    let captured = host.drain_actions();
                    assert_eq!(intents(&host, root, &captured).len(), 1);
                    input.revision += 1;
                    input.phase = SidebarSearchPhase::Complete { has_more: false };
                    update(&mut host, root, &input).unwrap();
                    host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                        .unwrap();
                    assert!(host.get(retry).is_none());
                    assert!(intents(&host, root, &captured).is_empty());
                    assert_eq!(host.get(footer_id).unwrap().frame().size.height, 0.);
                    assert!(
                        (host.get(region).unwrap().frame().max_y() - frame.max_y()).abs() < 0.1
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn demo_routes_retry_to_its_original_search_source_without_touching_composer() {
    use std::sync::Arc;
    use std::time::Duration;
    use tekes_agent_presentation::{ComposerDestination, RunScope};
    use tekes_agent_session::SessionRuntime;
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::{
        component_demo::{ComponentDemo, DemoCommand},
        fixture::Fixture,
    };
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    binding
        .edit_text(
            &ComposerDestination::run(binding.scope().clone()),
            "原中文草稿".into(),
        )
        .unwrap();
    let mut view = ComponentDemo::new([binding.clone()]).unwrap();
    let first = failed(1, 1);
    view.set_sidebar_search(&first).unwrap();
    let retry = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .search_retry_id()
        .unwrap();
    let commands = view
        .accessibility_action(retry, None, AccessibilityAction::Press)
        .unwrap();
    let [DemoCommand::SidebarSearch(SidebarSearchIntent::Retry { key })] = commands.as_slice()
    else {
        panic!("original search callback");
    };
    assert_eq!(Some(key), first.key.as_ref());
    let mut next = first.clone();
    next.revision += 1;
    next.key.as_mut().unwrap().generation += 1;
    next.phase = SidebarSearchPhase::Searching;
    view.set_sidebar_search(&next).unwrap();
    assert!(
        view.accessibility_action(retry, None, AccessibilityAction::Press)
            .unwrap()
            .is_empty()
    );
    assert_eq!(binding.current_run().unwrap().draft.text, "原中文草稿");
    assert!(binding.current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}
