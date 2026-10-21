use tekes_agent_components_ui::*;
use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer, composition::*};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::{Rgba, Theme, proposal};
struct Text;
impl ButtonTextRenderer for Text {
    fn measure(&mut self, text: &str, _: proposal::TextRole, _: f32) -> Size {
        Size::new(text.chars().count() as f32 * 10., 20.)
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
fn row(endpoint: &str, pinned: bool) -> SidebarRow {
    SidebarRow {
        primary_folder_id: None,
        folder_path: None,
        address: SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: "same".into(),
        },
        title: endpoint.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned }),
        actions: ThreadActionsPresentation {
            endpoint: endpoint.into(),
            session_id: "same".into(),
            label: "操作".into(),
            items: ["copy", "fork", "archive"]
                .map(|id| ThreadActionDefinition {
                    id: id.into(),
                    label: id.into(),
                    icon: "".into(),
                    interaction: if id == "copy" { "copy" } else { "execute" }.into(),
                    enabled: true,
                    disabled_reason: None,
                    copy_text: (id == "copy").then(|| format!("{endpoint}:same")),
                })
                .into(),
        },
    }
}
fn input() -> SidebarInput {
    SidebarInput {
        revision: 1,
        loaded: true,
        error: None,
        folder_filter: None,
        search_help: None,
        rows: vec![row("native", true), row("pi", true)],
        selected: Some(row("native", true).address),
    }
}
fn sidebar(host: &ContentHost) -> &Sidebar {
    host.get(ComponentId(990u64 << 32))
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
}
fn sidebar_mut(host: &mut ContentHost) -> &mut Sidebar {
    host.get_mut(ComponentId(990u64 << 32))
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
}
fn menu(host: &ContentHost) -> ComponentId {
    host.get(sidebar(host).session_menu_id().unwrap())
        .unwrap()
        .component::<SidebarSessionMenu>()
        .unwrap()
        .menu_id()
        .unwrap()
}
fn paint(host: &mut ContentHost) {
    let frame = Rect::new(0., 0., 280., 600.);
    host.render(
        &mut Scene::new(),
        &mut Text,
        frame,
        Rect::new(0., 0., 800., 800.),
        Theme::LIGHT.into(),
    )
    .unwrap();
}
fn setup() -> ContentHost {
    let mut sidebar = Sidebar::new(990, Default::default()).with_session_menu();
    sidebar
        .update_preferences(&SidebarPreferencesSnapshot::default(), true)
        .unwrap();
    sidebar.update(&input()).unwrap();
    let mut host = sidebar.into_host().unwrap();
    paint(&mut host);
    host
}
fn open(host: &mut ContentHost) {
    host.accessibility_action(menu(host), None, AccessibilityAction::Press);
    paint(host);
}
fn part(host: &ContentHost, label: &str) -> SemanticPart {
    host.semantics()
        .into_iter()
        .find(|n| n.id == menu(host))
        .unwrap()
        .parts
        .into_iter()
        .find(|p| p.semantics.name == label)
        .unwrap()
}
fn choose(host: &mut ContentHost, label: &str) -> Vec<ContentActionRecord> {
    open(host);
    let part = part(host, label);
    host.accessibility_action(menu(host), Some(part.index), AccessibilityAction::Press);
    paint(host);
    host.drain_action_records()
}
#[test]
fn pin_order_commands_capture_original_preferences_keep_boundaries_and_do_not_reorder_optimistically()
 {
    let mut host = setup();
    assert!(
        sidebar(&host)
            .action_id(&input().rows[0].address, "copy")
            .is_none()
    );
    let record = choose(&mut host, "上移置顶");
    assert!(
        sidebar(&host).take_menu_intents(&record).is_empty(),
        "ClientKit boundary command is a no-op"
    );
    let record = choose(&mut host, "下移置顶");
    let intents =
        tekes_agent_ui_consumer_check::sidebar_session_menu_intents(sidebar(&host), &record);
    let [SidebarSessionMenuIntent::Preferences(change)] = intents.as_slice() else {
        panic!("original pin order command")
    };
    assert_eq!(change.expected, SidebarPreferencesSnapshot::default());
    assert_eq!(
        change.value.pin_order,
        [
            input().rows[1].address.clone(),
            input().rows[0].address.clone()
        ]
    );
    assert!(
        sidebar(&host)
            .preferences()
            .preferences
            .pin_order
            .is_empty()
    );
    sidebar_mut(&mut host)
        .update_preferences(
            &SidebarPreferencesSnapshot {
                revision: 2,
                preferences: change.value.clone(),
            },
            true,
        )
        .unwrap();
    assert!(sidebar(&host).take_menu_intents(&record).is_empty());
}
#[test]
fn selection_endpoint_and_pending_changes_revoke_old_records_but_captured_intents_keep_their_source()
 {
    let mut host = setup();
    let record = choose(&mut host, "copy");
    let captured = sidebar(&host).take_menu_intents(&record);
    let [
        SidebarSessionMenuIntent::Shared(SidebarIntent::Action {
            address,
            definition,
        }),
    ] = captured.as_slice()
    else {
        panic!("shared action")
    };
    assert_eq!(address.endpoint_id, "native");
    assert_eq!(definition.copy_text.as_deref(), Some("native:same"));
    let mut next = input();
    next.selected = Some(next.rows[1].address.clone());
    sidebar_mut(&mut host).update(&next).unwrap();
    paint(&mut host);
    assert!(sidebar(&host).take_menu_intents(&record).is_empty());
    let record = choose(&mut host, "fork");
    sidebar_mut(&mut host).set_pending(&next.rows[1].address, "fork", true);
    paint(&mut host);
    assert!(sidebar(&host).take_menu_intents(&record).is_empty());
    open(&mut host);
    assert!(part(&host, "fork").semantics.disabled);
    assert_eq!(address.endpoint_id, "native");
}
#[test]
fn irrelevant_activity_refresh_preserves_the_open_menu_and_current_sdk_model() {
    let mut host = setup();
    open(&mut host);
    let id = menu(&host);
    let model = host
        .get(id)
        .unwrap()
        .component::<MenuContent>()
        .unwrap()
        .model_revision();
    let mut next = input();
    next.revision = 2;
    next.rows[0].activity.status = "running".into();
    sidebar_mut(&mut host).update(&next).unwrap();
    paint(&mut host);
    let control = host.get(id).unwrap().component::<MenuContent>().unwrap();
    assert_eq!(control.model_revision(), model);
    assert!(control.menu.is_open());
    assert_eq!(part(&host, "copy").semantics.name, "copy");
}
#[test]
fn read_only_pin_order_and_unrestored_inventory_do_not_gain_actions() {
    let mut host = setup();
    sidebar_mut(&mut host)
        .update_preferences(&SidebarPreferencesSnapshot::default(), false)
        .unwrap();
    paint(&mut host);
    open(&mut host);
    assert!(part(&host, "下移置顶").semantics.disabled);
    let mut next = input();
    next.loaded = false;
    next.revision = 2;
    sidebar_mut(&mut host).update(&next).unwrap();
    paint(&mut host);
    assert!(
        !host
            .semantics()
            .iter()
            .any(|node| node.semantics.role == ContentRole::MenuButton)
    );
}
