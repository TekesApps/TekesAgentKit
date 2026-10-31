//! Original SDK toolbar/nav and retained pane under real CoreText. This is not
//! native NSWindow toolbar, popup input, physical IME or VoiceOver acceptance.
use super::*;
use tekes_ui_controls::extended::{Axis, Stack};

#[test]
fn host_header_keeps_original_pane_and_composer_while_library_typography_reflows() {
    for theme in Theme::ALL {
        for scale in [1., 1.4, 2.] {
            let parent = RunScope::main("public", "panel-layout");
            let title = "任务与子代理共享导航，完整标题 👩🏽‍💻 e\u{301}，不截断源字符串";
            let mut input = SidePanelInput {
                revision: 1,
                parent: parent.clone(),
                projection_ready: true,
                tabs: vec![SidePanelTab::summary(title)],
                selected: Some(SidePanelTabKind::Summary),
                side_chats: Default::default(),
                supports_side_chat: None,
            };
            let mut panel = SidePanel::new(63610, Default::default(), 2);
            panel.update(&input).unwrap();
            let diagnostic_tab = panel.tab_id(&SidePanelTabKind::Summary).unwrap();
            let diagnostic_close = panel.close_id();
            let body = ComponentId(63611_u64 << 32);
            let token = panel.content_token(&SidePanelTabKind::Summary).unwrap();
            panel
                .install_content(
                    &token,
                    Content::new(
                        body,
                        TextFieldContent::read_only_content(
                            "原面板正文",
                            "保留的原始面板内容",
                            proposal::TextRole::UiBody,
                        ),
                    ),
                )
                .unwrap();
            panel.set_header_placement(SidePanelHeaderPlacement::Host);
            panel
                .set_layout(SidePanelLayout {
                    content_top_inset: 12.,
                })
                .unwrap();
            let pane = panel.id();
            let mut navigation = SidePanelNavigation::new(63612, Default::default());
            navigation.update(&input, true).unwrap();
            let nav = navigation.id();
            let tab = navigation
                .tab_button_id(&SidePanelTabKind::Summary)
                .unwrap();
            let composer = tekes_agent_ui_consumer_check::new_composer(63613);
            let editor = composer.ids().editor;
            let mut host = ContentHost::new(Content::new(
                ComponentId(13),
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: 0.,
                    },
                    flexible_child: Some(pane),
                    children: vec![
                        Content::new(nav, navigation),
                        Content::new(pane, panel),
                        Content::new(composer.ids().root, composer),
                    ],
                },
            ))
            .unwrap();
            host.set_action_events(true);
            let mut text = Text::new(scale, true);
            let frame = Rect::new(20., 30., 770., 1000.);
            host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                .unwrap();
            assert!(host.get(diagnostic_tab).is_none() && host.get(diagnostic_close).is_none());
            let body_owner = host.text_surface(body).unwrap() as *const _ as usize;
            let tab_owner =
                host.get(tab).unwrap().component::<Button>().unwrap() as *const _ as usize;
            let toolbar_owner = host
                .get(ComponentId(nav.0 + 1))
                .unwrap()
                .component::<ToolbarGroup>()
                .unwrap() as *const _ as usize;
            host.focus(Some(editor), true);
            host.event(&ContentEvent::Preedit {
                text: "宿主工具栏组字".into(),
                selection: 1..3,
                replacement: None,
                clauses: vec![],
            });
            let baseline = host.text_surface(editor).unwrap().input.external_baseline();
            for width in [770., 320., 180., 770.] {
                input.revision += 1;
                host.get_mut(nav)
                    .unwrap()
                    .component_mut::<SidePanelNavigation>()
                    .unwrap()
                    .update(&input, true)
                    .unwrap();
                host.get_mut(pane)
                    .unwrap()
                    .component_mut::<SidePanel>()
                    .unwrap()
                    .update(&input)
                    .unwrap();
                let frame = Rect::new(20., 30., width, 1000.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                assert_eq!(
                    host.text_surface(body).unwrap() as *const _ as usize,
                    body_owner
                );
                assert_eq!(
                    host.get(tab).unwrap().component::<Button>().unwrap() as *const _ as usize,
                    tab_owner
                );
                assert_eq!(
                    host.get(ComponentId(nav.0 + 1))
                        .unwrap()
                        .component::<ToolbarGroup>()
                        .unwrap() as *const _ as usize,
                    toolbar_owner
                );
                assert_eq!(
                    host.get(tab).unwrap().component::<Button>().unwrap().label,
                    title
                );
                assert_eq!(host.focused(), Some(editor));
                assert_eq!(
                    host.text_surface(editor).unwrap().input.external_baseline(),
                    baseline
                );
                let geometry = host
                    .get(pane)
                    .unwrap()
                    .component::<SidePanel>()
                    .unwrap()
                    .layout_geometry()
                    .unwrap();
                assert!(
                    (geometry.body.min_y()
                        - geometry.canvas.min_y()
                        - tekes_ui_tokens::Spacing::Regular.value())
                    .abs()
                        < 0.02,
                    "external header cannot leave diagnostic row clearance"
                );
                assert!(
                    text.labels
                        .iter()
                        .filter(|(_, label)| label.source() == title)
                        .all(|(_, label)| label.paragraph().lines.len() == 1)
                );
            }
            assert!(
                text.labels
                    .iter()
                    .filter(|(_, label)| label.source() == title)
                    .any(|(_, label)| label.is_truncated())
            );
        }
    }
}
