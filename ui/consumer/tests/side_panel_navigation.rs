//! Public host of the original SDK toolbar/menu. Semantic actions are not native
//! popup, physical IME, VoiceOver or NSWindow-toolbar acceptance.
use tekes_agent_components_ui::*;
use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer, composition::*};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Rgba, Theme, proposal};
const NS: u32 = 63600;
fn input() -> SidePanelInput {
    let parent = RunScope::main("public", "parent");
    let mut input = SidePanelInput {
        revision: 1,
        parent: parent.clone(),
        projection_ready: true,
        tabs: vec![SidePanelTab::summary("Summary")],
        selected: Some(SidePanelTabKind::Summary),
        side_chats: Default::default(),
        supports_side_chat: Some(true),
    };
    for id in ["scratch-a", "scratch-b"] {
        let chat = SideChat {
            parent: parent.clone(),
            scope: RunScope::main("public", id),
            title: format!("original {id}"),
            phase: SideChatPhase::Ready,
            error: None,
        };
        input.tabs.push(SidePanelTab::side_chat(&chat));
        input.side_chats.chats.insert(id.into(), chat);
    }
    input
}
struct Text;
impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, _: proposal::TextRole, width: f32) -> Size {
        Size::new((text.chars().count() as f32 * 8.).min(width), 20.)
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
fn paint(host: &mut ContentHost) {
    let frame = Rect::new(20., 20., 800., 42.);
    host.render(
        &mut Scene::new(),
        &mut Text,
        frame,
        Rect::new(0., 0., 900., 700.),
        Theme::LIGHT.into(),
    )
    .unwrap();
}
fn navigation(host: &ContentHost) -> &SidePanelNavigation {
    host.get(ComponentId((NS as u64) << 32))
        .unwrap()
        .component::<SidePanelNavigation>()
        .unwrap()
}
fn update(host: &mut ContentHost, input: &SidePanelInput, enabled: bool) {
    host.get_mut(ComponentId((NS as u64) << 32))
        .unwrap()
        .component_mut::<SidePanelNavigation>()
        .unwrap()
        .update(input, enabled)
        .unwrap();
    paint(host);
}
fn setup(input: &SidePanelInput) -> ContentHost {
    let mut navigation = SidePanelNavigation::new(NS, Default::default());
    navigation.update(input, true).unwrap();
    let mut host = navigation.into_host().unwrap();
    paint(&mut host);
    host
}
fn choose_chat(host: &mut ContentHost, index: usize) {
    let menu = navigation(host).side_chat_menu_id().unwrap();
    host.accessibility_action(menu, None, AccessibilityAction::Press);
    paint(host);
    let part = host
        .semantics()
        .into_iter()
        .find(|node| node.id == menu)
        .unwrap()
        .parts
        .into_iter()
        .filter(|part| {
            part.semantics.role == ContentRole::MenuItem && part.semantics.name == "SideChat"
        })
        .nth(index)
        .unwrap();
    assert!(
        host.accessibility_action(menu, Some(part.index), AccessibilityAction::Press)
            .activated
    );
    paint(host);
}
#[test]
fn family_menu_captures_original_a_and_b_without_polling_the_last_selection() {
    let input = input();
    let mut host = setup(&input);
    assert!(
        navigation(&host)
            .tab_button_id(&input.tabs[1].kind)
            .is_none(),
        "one family menu replaces individual SideChat buttons"
    );
    choose_chat(&mut host, 0);
    choose_chat(&mut host, 1);
    let records = host.drain_action_records();
    let intents = navigation(&host).take_intents(&records, &host);
    assert_eq!(
        intents,
        vec![
            SidePanelIntent::Select {
                parent: input.parent.clone(),
                tab: input.tabs[1].clone()
            },
            SidePanelIntent::Select {
                parent: input.parent.clone(),
                tab: input.tabs[2].clone()
            }
        ]
    );
    let old = navigation(&host).side_chat_menu_id().unwrap();
    let mut next = input.clone();
    next.revision += 1;
    next.selected = Some(next.tabs[2].kind.clone());
    update(&mut host, &next, true);
    assert_ne!(navigation(&host).side_chat_menu_id(), Some(old));
    assert!(navigation(&host).take_intents(&records, &host).is_empty());
}
#[test]
fn close_target_capabilities_and_same_id_remount_are_fenced_by_original_sdk_mounts() {
    let mut input = input();
    let mut host = setup(&input);
    let initial = input.clone();
    let close = navigation(&host).close_id().unwrap();
    host.accessibility_action(close, None, AccessibilityAction::Press);
    let records = host.drain_action_records();
    let mut detached = SidePanelNavigation::new(NS, Default::default());
    detached.update(&input, true).unwrap();
    assert!(
        detached.take_intents(&records, &host).is_empty(),
        "a detached component cannot interpret another Host's current records"
    );
    assert_eq!(
        navigation(&host).take_intents(&records, &host),
        vec![SidePanelIntent::Close {
            parent: input.parent.clone(),
            tab: SidePanelTabKind::Summary
        }]
    );
    let summary = navigation(&host)
        .tab_button_id(&SidePanelTabKind::Summary)
        .unwrap();
    input.revision += 1;
    input.selected = Some(input.tabs[1].kind.clone());
    update(&mut host, &input, true);
    assert_eq!(
        navigation(&host).tab_button_id(&SidePanelTabKind::Summary),
        Some(summary)
    );
    assert_ne!(navigation(&host).close_id(), Some(close));
    assert!(navigation(&host).take_intents(&records, &host).is_empty());
    let close = navigation(&host).close_id().unwrap();
    host.accessibility_action(close, None, AccessibilityAction::Press);
    let records = host.drain_action_records();
    let mut remounted = setup(&initial);
    update(&mut remounted, &input, true);
    assert_eq!(
        navigation(&remounted).close_id(),
        Some(close),
        "deliberately recreate the same numeric target ID"
    );
    assert!(
        navigation(&remounted)
            .take_intents(&records, &remounted)
            .is_empty()
    );
    input.revision += 1;
    input.projection_ready = false;
    update(&mut remounted, &input, true);
    let close = navigation(&remounted).close_id().unwrap();
    assert!(
        !remounted
            .accessibility_action(close, None, AccessibilityAction::Press)
            .activated
    );
    assert!(
        navigation(&remounted)
            .take_intents(&records, &remounted)
            .is_empty()
    );
}
#[test]
fn shared_chat_capabilities_and_validation_cannot_retarget_queued_callbacks() {
    let mut input = input();
    input.selected = Some(input.tabs[1].kind.clone());
    let mut host = setup(&input);
    let menu = navigation(&host).side_chat_menu_id().unwrap();
    host.accessibility_action(menu, None, AccessibilityAction::Press);
    paint(&mut host);
    let part = host
        .semantics()
        .into_iter()
        .find(|node| node.id == menu)
        .unwrap()
        .parts
        .into_iter()
        .find(|part| part.semantics.name == "新建 SideChat")
        .unwrap();
    host.accessibility_action(menu, Some(part.index), AccessibilityAction::Press);
    let records = host.drain_action_records();
    assert_eq!(
        navigation(&host).take_intents(&records, &host),
        vec![SidePanelIntent::CreateSideChat {
            parent: input.parent.clone()
        }]
    );
    input.revision += 1;
    input.supports_side_chat = Some(false);
    input.side_chats.chats.get_mut("scratch-a").unwrap().phase = SideChatPhase::Closing;
    update(&mut host, &input, true);
    let close = navigation(&host).close_id().unwrap();
    assert!(
        !host
            .accessibility_action(close, None, AccessibilityAction::Press)
            .activated
    );
    assert!(navigation(&host).take_intents(&records, &host).is_empty());
    let menu = navigation(&host).side_chat_menu_id().unwrap();
    let mut bad = input.clone();
    bad.revision += 1;
    bad.tabs.remove(1);
    bad.selected = Some(SidePanelTabKind::Summary);
    assert!(
        host.get_mut(navigation(&host).id())
            .unwrap()
            .component_mut::<SidePanelNavigation>()
            .unwrap()
            .update(&bad, true)
            .is_err()
    );
    assert_eq!(navigation(&host).side_chat_menu_id(), Some(menu));
    input.revision += 1;
    update(&mut host, &input, false);
    assert!(
        !host
            .accessibility_action(
                navigation(&host).side_chat_menu_id().unwrap(),
                None,
                AccessibilityAction::Press
            )
            .activated
    );
}
