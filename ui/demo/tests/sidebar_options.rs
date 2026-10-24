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
fn paint(host: &mut ContentHost) {
    host.render(
        &mut Scene::new(),
        &mut Text,
        Rect::new(20., 20., 180., 36.),
        Rect::new(0., 0., 800., 600.),
        Theme::LIGHT.into(),
    )
    .unwrap();
}
fn options(host: &ContentHost) -> &SidebarOptions {
    host.get(ComponentId(989u64 << 32))
        .unwrap()
        .component::<SidebarOptions>()
        .unwrap()
}
fn update(
    host: &mut ContentHost,
    input: &SidebarPreferencesSnapshot,
    writable: bool,
    titles: bool,
) {
    host.get_mut(ComponentId(989u64 << 32))
        .unwrap()
        .component_mut::<SidebarOptions>()
        .unwrap()
        .update(input, writable, titles)
        .unwrap();
}
fn setup() -> ContentHost {
    let mut view = SidebarOptions::new(989, Default::default());
    view.update(&SidebarPreferencesSnapshot::default(), true, true)
        .unwrap();
    let mut host = ContentHost::new(Content::new(view.id(), view)).unwrap();
    host.set_action_events(true);
    paint(&mut host);
    host
}
fn part(host: &ContentHost, name: &str) -> SemanticPart {
    host.semantics()
        .into_iter()
        .flat_map(|node| node.parts)
        .find(|part| part.semantics.name == name)
        .unwrap_or_else(|| panic!("missing menu part {name}"))
}
fn open(host: &mut ContentHost, family: &str) {
    let menu = options(host).menu_id();
    host.accessibility_action(menu, None, AccessibilityAction::Press);
    paint(host);
    let submenu = part(host, family);
    host.accessibility_action(menu, Some(submenu.index), AccessibilityAction::Press);
    paint(host);
}
fn select(host: &mut ContentHost, family: &str, name: &str) {
    open(host, family);
    let item = part(host, name);
    host.accessibility_action(
        options(host).menu_id(),
        Some(item.index),
        AccessibilityAction::Press,
    );
    paint(host);
}
#[test]
fn original_menu_captures_two_paths_before_drain_and_keeps_the_original_cas_baseline() {
    let mut host = setup();
    select(&mut host, "排序方式", "标题 A–Z");
    select(&mut host, "分组方式", "主文件夹");
    let records = host.drain_action_records();
    let intents = tekes_agent_ui_consumer_check::sidebar_options_intents(options(&host), &records);
    assert_eq!(intents.len(), 2);
    assert_eq!(intents[0].change.value.sort, SidebarSort::TitleAscending);
    assert!(!intents[0].change.value.group_by_folder);
    assert!(intents[0].reset_paging);
    assert_eq!(intents[1].change.value.sort, SidebarSort::RecentActivity);
    assert!(intents[1].change.value.group_by_folder);
    assert!(!intents[1].reset_paging);
    assert!(
        intents
            .iter()
            .all(|i| i.change.expected == SidebarPreferencesSnapshot::default())
    );
    assert!(host.drain_action_records().is_empty());
}
#[test]
fn unrelated_preference_update_permissions_and_foreign_menu_revoke_captured_actions() {
    let mut host = setup();
    select(&mut host, "排序方式", "最近活动");
    let records = host.drain_action_records();
    assert!(options(&host).take_intents(&records)[0].reset_paging);
    // Even if no checked state changes, an old menu must not adopt the new CAS revision.
    let mut next = SidebarPreferencesSnapshot {
        revision: 2,
        ..Default::default()
    };
    next.preferences.pinned_expanded = false;
    update(&mut host, &next, true, true);
    assert!(options(&host).take_intents(&records).is_empty());
    let foreign = setup();
    assert!(options(&foreign).take_intents(&records).is_empty());
    paint(&mut host);
    select(&mut host, "分组方式", "主文件夹");
    let records = host.drain_action_records();
    update(&mut host, &next, false, true);
    assert!(options(&host).take_intents(&records).is_empty());
    paint(&mut host);
    host.accessibility_action(options(&host).menu_id(), None, AccessibilityAction::Press);
    assert!(host.drain_action_records().is_empty());
}
#[test]
fn checked_choices_and_missing_localized_comparator_come_from_authoritative_input() {
    let mut host = setup();
    update(
        &mut host,
        &SidebarPreferencesSnapshot::default(),
        true,
        false,
    );
    paint(&mut host);
    open(&mut host, "排序方式");
    assert!(part(&host, "最近活动").semantics.selected);
    let disabled = part(&host, "标题 A–Z");
    assert!(disabled.semantics.disabled);
    host.accessibility_action(
        options(&host).menu_id(),
        Some(disabled.index),
        AccessibilityAction::Press,
    );
    assert!(host.drain_action_records().is_empty());
    let id = options(&host).menu_id();
    let before = options(&host).children()[0]
        .component::<MenuContent>()
        .unwrap()
        .model_revision();
    update(
        &mut host,
        &SidebarPreferencesSnapshot::default(),
        true,
        false,
    );
    assert_eq!(options(&host).menu_id(), id);
    assert_eq!(
        options(&host).children()[0]
            .component::<MenuContent>()
            .unwrap()
            .model_revision(),
        before
    );
}

#[tokio::test]
async fn menu_choice_uses_the_existing_durable_binding_and_does_not_rebase_a_duplicate() {
    use std::{
        sync::Arc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tekes_agent_ui_bindings::{RunBinding, SidebarBinding};
    use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};
    use tekes_agent_web::SharedAgent;
    let directory = std::env::temp_dir().join(format!(
        "agentkit-options-store-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = directory.join("preferences.json");
    let mut agent = SharedAgent::default();
    agent
        .open_thread_preferences(&path, "owned-options-test")
        .unwrap();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(Fixture::default()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    binding
        .edit_text(
            &ComposerDestination::run(binding.scope().clone()),
            "选项保留的中文草稿".into(),
        )
        .unwrap();
    let source = SidebarBinding::new(agent.clone(), "fixture").unwrap();
    let mut host = setup();
    update(&mut host, &source.preferences().unwrap(), true, false);
    paint(&mut host);
    select(&mut host, "分组方式", "主文件夹");
    let records = host.drain_action_records();
    let intent = options(&host).take_intents(&records).remove(0);
    source.set_preferences(&intent.change).unwrap();
    assert!(source.set_preferences(&intent.change).is_err());
    let persisted = source.preferences().unwrap();
    assert!(persisted.preferences.group_by_folder);
    let mut demo = ComponentDemo::new([binding]).unwrap();
    demo.set_sidebar_preferences(&persisted, true).unwrap();
    assert_eq!(demo.editor().input.text, "选项保留的中文草稿");
    assert!(demo.binding().current_run().unwrap().submissions.is_empty());
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        saved["scopes"]["owned-options-test"]["sidebar"]["preferences"]["groupByFolder"],
        true
    );
    drop(demo);
    drop(source);
    runtime.stop().await.unwrap();
    drop(agent);
    std::fs::remove_dir_all(directory).unwrap();
}
