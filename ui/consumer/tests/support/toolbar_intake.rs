//! Real Kit controls inside the new SDK toolbar, not a copied toolbar model.
use super::*;
use tekes_ui_controls::extended::{Axis, Stack};

#[test]
fn original_kit_options_and_composer_survive_public_toolbar_typography_and_reflow() {
    for theme in Theme::ALL {
        let mut base_height = 0.;
        for scale in [1., 1.4, 2.] {
            let mut options = SidebarOptions::new(64001, Default::default());
            options
                .update(&SidebarPreferencesSnapshot::default(), true, true)
                .unwrap();
            let options_id = options.id();
            let menu_id = options.menu_id();
            let composer = tekes_agent_ui_consumer_check::new_composer(64000);
            let editor = composer.ids().editor;
            let toolbar = ComponentId(64002u64 << 32);
            let mut host = ContentHost::new(Content::new(
                ComponentId(10),
                StackContent {
                    model: Stack {
                        axis: Axis::Vertical,
                        gap: 0.,
                    },
                    flexible_child: None,
                    children: vec![
                        Content::new(
                            toolbar,
                            ToolbarContent::new(vec![
                                Content::new(
                                    ComponentId(toolbar.0 + 1),
                                    ToolbarGroup::new(
                                        ToolbarPlacement::Navigation,
                                        vec![Content::new(options_id, options)],
                                    ),
                                ),
                                Content::new(
                                    ComponentId(toolbar.0 + 2),
                                    ToolbarGroup::new(
                                        ToolbarPlacement::Principal,
                                        vec![Content::new(
                                            ComponentId(toolbar.0 + 10),
                                            TextContent::new(
                                                "当前工作区 / 一个需要截断的完整窗口标题",
                                            ),
                                        )],
                                    ),
                                ),
                                Content::new(
                                    ComponentId(toolbar.0 + 3),
                                    ToolbarGroup::new(
                                        ToolbarPlacement::PrimaryAction,
                                        vec![Content::new(
                                            ComponentId(toolbar.0 + 11),
                                            ButtonContent::new("宿主动作"),
                                        )],
                                    ),
                                ),
                            ]),
                        ),
                        Content::new(composer.ids().root, composer),
                    ],
                },
            ))
            .unwrap();
            host.set_action_events(true);
            let mut text = Text::new(scale, true);
            let frame = Rect::new(0., 0., 1100., 600.);
            host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                .unwrap();
            let height = host.get(toolbar).unwrap().frame().size.height;
            if scale == 1. {
                base_height = height;
            }
            assert!((height - base_height * scale).abs() < 0.02);
            let original_options = host
                .get(options_id)
                .unwrap()
                .component::<SidebarOptions>()
                .unwrap() as *const _ as usize;
            host.focus(Some(editor), true);
            host.event(&ContentEvent::Preedit {
                text: "工具栏变化时中文组字".into(),
                selection: 1..3,
                replacement: None,
                clauses: vec![],
            });
            let baseline = host.text_surface(editor).unwrap().input.external_baseline();
            let owner = host.text_surface(editor).unwrap() as *const _ as usize;
            for width in [770., 320., 1100.] {
                host.get_mut(toolbar)
                    .unwrap()
                    .component_mut::<ToolbarContent>()
                    .unwrap()
                    .children
                    .swap(0, 2);
                let frame = Rect::new(0., 0., width, 600.);
                host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                    .unwrap();
                assert_eq!(host.focused(), Some(editor));
                assert_eq!(
                    host.text_surface(editor).unwrap().input.external_baseline(),
                    baseline
                );
                assert_eq!(
                    host.text_surface(editor).unwrap() as *const _ as usize,
                    owner
                );
                assert_eq!(
                    host.get(options_id)
                        .unwrap()
                        .component::<SidebarOptions>()
                        .unwrap() as *const _ as usize,
                    original_options
                );
            }
            host.event(&ContentEvent::Commit {
                text: "中文".into(),
                replacement: None,
            });
            // The caller's normal menu operation after composition; original
            // semantic parts produce the existing typed preference callback.
            host.accessibility_action(menu_id, None, AccessibilityAction::Press);
            host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                .unwrap();
            let part = |host: &ContentHost, name: &str| {
                host.semantics()
                    .into_iter()
                    .flat_map(|n| n.parts)
                    .find(|p| p.semantics.name == name)
                    .unwrap()
            };
            let sort = part(&host, "排序方式");
            host.accessibility_action(menu_id, Some(sort.index), AccessibilityAction::Press);
            host.render(&mut Scene::new(), &mut text, frame, frame, theme.into())
                .unwrap();
            let ascending = part(&host, "标题 A–Z");
            host.accessibility_action(menu_id, Some(ascending.index), AccessibilityAction::Press);
            let records = host.drain_action_records();
            let options = host
                .get(options_id)
                .unwrap()
                .component::<SidebarOptions>()
                .unwrap();
            let intents = tekes_agent_ui_consumer_check::sidebar_options_intents(options, &records);
            assert_eq!(intents.len(), 1);
            assert!(intents[0].reset_paging);
        }
    }
}
