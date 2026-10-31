use tekes_agent_components_ui::{
    FamilyCatalog, FamilyCatalogInput, FamilyCatalogIntent, FamilyCatalogLabels, RunScope,
};
use tekes_agent_contract::{SubagentCatalog, SubagentEntry};
use tekes_ui_controls::composition::*;

fn child(id: &str, mode: Option<&str>, label: Option<&str>) -> SubagentEntry {
    SubagentEntry {
        kind: "child".into(),
        id: id.into(),
        mode: mode.map(str::to_owned),
        plane: None,
        has_children: None,
        label: label.map(str::to_owned),
        reason: None,
    }
}

fn input(parent: &str, epoch: u64, generation: u64, revision: u64) -> FamilyCatalogInput {
    FamilyCatalogInput {
        source: Default::default(),
        parent: RunScope::main("native", parent),
        connection_epoch: epoch,
        generation,
        revision,
        refreshing: false,
        catalog: Some(SubagentCatalog {
            entries: vec![
                child("bounded", Some("bounded"), Some("只读子代理")),
                child("continuable", Some("continuable"), Some("可继续子代理")),
                child("one-shot", Some("one-shot"), None),
                child("future-mode", Some("unknown-new-mode"), Some("新模式")),
                SubagentEntry {
                    kind: "task".into(),
                    id: "task-1".into(),
                    mode: None,
                    plane: None,
                    has_children: None,
                    label: Some("不是 child".into()),
                    reason: None,
                },
            ],
            parent_available: false,
        }),
        error: None,
    }
}

fn catalog(host: &ContentHost) -> &FamilyCatalog {
    host.get(ComponentId(71001u64 << 32))
        .unwrap()
        .component::<FamilyCatalog>()
        .unwrap()
}

fn catalog_mut(host: &mut ContentHost) -> &mut FamilyCatalog {
    host.get_mut(ComponentId(71001u64 << 32))
        .unwrap()
        .component_mut::<FamilyCatalog>()
        .unwrap()
}

fn activate(host: &mut ContentHost, id: ComponentId) -> Vec<ContentAction> {
    host.accessibility_action(id, None, AccessibilityAction::Press);
    host.drain_actions()
}

fn setup(input: &FamilyCatalogInput, namespace: u32) -> ContentHost {
    let mut widget = FamilyCatalog::new(namespace, FamilyCatalogLabels::default());
    widget.update(input).unwrap();
    let mut host = ContentHost::new(Content::new(widget.id(), widget)).unwrap();
    host.set_action_events(true);
    host
}

#[test]
fn catalog_uses_sdk_buttons_and_preserves_read_only_history_when_parent_is_unavailable() {
    let mut source = input("parent-a", 4, 9, 1);
    let mut host = setup(&source, 71001);
    let bounded = catalog(&host).child_button_id("bounded").unwrap();
    let button = host.get(bounded).unwrap().component::<Button>().unwrap();
    assert!(button.label.contains("只读子代理"));
    assert!(button.label.contains("有界历史"));
    assert!(button.label.contains("父会话不可用"));
    assert!(host.semantics().iter().any(|node| node.id == bounded));

    let actions = activate(&mut host, bounded);
    let intents = catalog(&host).take_intents(&actions);
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].parent, source.parent);
    assert_eq!(intents[0].child_id, "bounded");
    assert_eq!(intents[0].connection_epoch, 4);
    assert_eq!(intents[0].generation, 9);
    assert_eq!(intents[0].revision, 1);
    assert!(catalog(&host).accepts(&intents[0]));

    let unknown = catalog(&host).child_button_id("future-mode").unwrap();
    let unknown_button = host.get(unknown).unwrap().component::<Button>().unwrap();
    assert!(unknown_button.button.state.disabled);
    assert!(activate(&mut host, unknown).is_empty());
    assert_eq!(catalog(&host).take_intents(&actions).len(), 1);

    source.revision += 1;
    catalog_mut(&mut host).update(&source).unwrap();
    assert_eq!(catalog(&host).child_button_id("bounded"), Some(bounded));
    assert!(!catalog(&host).accepts(&intents[0]));
    source.catalog.as_mut().unwrap().entries[0].label = Some("替换后的条目".into());
    source.revision += 1;
    catalog_mut(&mut host).update(&source).unwrap();
    assert_eq!(catalog(&host).child_button_id("bounded"), Some(bounded));
    assert!(!catalog(&host).accepts(&intents[0]));
}

#[test]
fn intents_are_bound_to_widget_parent_and_a_to_b_to_a_generation() {
    let a = input("parent-a", 2, 11, 1);
    let mut host = setup(&a, 71001);
    let a_id = catalog(&host).child_button_id("bounded").unwrap();
    let actions = activate(&mut host, a_id);
    let old_a = catalog(&host).take_intents(&actions);
    assert_eq!(old_a.len(), 1);

    let b = input("parent-b", 2, 11, 1);
    catalog_mut(&mut host).update(&b).unwrap();
    let b_id = catalog(&host).child_button_id("bounded").unwrap();
    assert_ne!(a_id, b_id);
    assert!(!catalog(&host).accepts(&old_a[0]));

    let mut returned_a = input("parent-a", 2, 11, 1);
    catalog_mut(&mut host).update(&returned_a).unwrap();
    let returned_a_id = catalog(&host).child_button_id("bounded").unwrap();
    assert_ne!(a_id, returned_a_id);
    assert!(!catalog(&host).accepts(&old_a[0]));
    let actions = activate(&mut host, returned_a_id);
    let current = catalog(&host).take_intents(&actions);
    assert_eq!(current.len(), 1);
    assert!(catalog(&host).accepts(&current[0]));

    let other = setup(&returned_a, 71002);
    assert!(
        !other
            .get(ComponentId(71002u64 << 32))
            .unwrap()
            .component::<FamilyCatalog>()
            .unwrap()
            .accepts(&current[0])
    );
    assert!(
        !current[0].belongs_to_widget(
            other
                .get(ComponentId(71002u64 << 32))
                .unwrap()
                .component::<FamilyCatalog>()
                .unwrap()
        )
    );

    returned_a.revision += 1;
    catalog_mut(&mut host).update(&returned_a).unwrap();
    assert!(!catalog(&host).accepts(&current[0]));
}

#[test]
fn stale_conflicting_and_unavailable_publications_fail_closed() {
    let mut current = input("parent-a", 3, 7, 2);
    let mut host = setup(&current, 71001);
    let control = catalog(&host).child_button_id("bounded").unwrap();
    let actions = activate(&mut host, control);
    let captured = catalog(&host).take_intents(&actions);
    assert_eq!(captured.len(), 1);

    let mut same_version_conflict = current.clone();
    same_version_conflict.catalog.as_mut().unwrap().entries[0].label = Some("替换条目".into());
    assert!(
        catalog_mut(&mut host)
            .update(&same_version_conflict)
            .is_err()
    );
    assert!(catalog(&host).accepts(&captured[0]));

    let mut rollback = current.clone();
    rollback.revision -= 1;
    assert!(catalog_mut(&mut host).update(&rollback).is_err());

    let mut refreshing = current.clone();
    refreshing.revision += 1;
    refreshing.refreshing = true;
    catalog_mut(&mut host).update(&refreshing).unwrap();
    assert!(!catalog(&host).accepts(&captured[0]));
    let actions = activate(&mut host, control);
    assert!(catalog(&host).take_intents(&actions).is_empty());

    current.revision += 2;
    current.error = Some("catalog read failed".into());
    catalog_mut(&mut host).update(&current).unwrap();
    assert!(catalog(&host).child_button_id("bounded").is_none());
    let actions = activate(&mut host, control);
    assert!(catalog(&host).take_intents(&actions).is_empty());
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name.contains("无法读取"))
    );
}

#[test]
fn malformed_child_identity_is_rejected_before_replacing_the_visible_frame() {
    let first = input("parent-a", 1, 1, 1);
    let mut host = setup(&first, 71001);
    let old = catalog(&host).displayed_input().unwrap().clone();
    let mut invalid = input("parent-a", 1, 1, 2);
    invalid
        .catalog
        .as_mut()
        .unwrap()
        .entries
        .push(child("bounded", Some("bounded"), None));
    assert!(catalog_mut(&mut host).update(&invalid).is_err());
    assert_eq!(catalog(&host).displayed_input(), Some(&old));
}

#[test]
fn intent_fields_are_readable_by_host_binding_without_exposing_internal_capture() {
    fn host_can_validate(intent: &FamilyCatalogIntent) {
        let _parent = &intent.parent;
        let _child = &intent.child_id;
        let _epoch = intent.connection_epoch;
        let _generation = intent.generation;
        let _revision = intent.revision;
    }

    let source = input("parent-a", 8, 4, 3);
    let mut host = setup(&source, 71001);
    let button = catalog(&host).child_button_id("bounded").unwrap();
    let actions = activate(&mut host, button);
    for intent in catalog(&host).take_intents(&actions) {
        host_can_validate(&intent);
    }
}

#[test]
fn empty_catalog_retires_previously_mounted_controls() {
    let mut source = input("parent-a", 1, 1, 1);
    let mut host = setup(&source, 71001);
    let old = catalog(&host).child_button_id("bounded").unwrap();
    source.revision += 1;
    source.catalog.as_mut().unwrap().entries.clear();
    catalog_mut(&mut host).update(&source).unwrap();
    assert!(catalog(&host).child_button_id("bounded").is_none());
    assert!(!host.semantics().iter().any(|node| node.id == old));
}

#[test]
fn replacing_owner_with_equal_scope_counters_and_data_retires_original_controls() {
    let mut source = input("parent-a", 1, 1, 2);
    let mut host = setup(&source, 71001);
    let original_id = catalog(&host).child_button_id("bounded").unwrap();
    let actions = activate(&mut host, original_id);
    let original = catalog(&host).take_intents(&actions).pop().unwrap();
    source.source = Default::default();
    catalog_mut(&mut host).update(&source).unwrap();
    assert_ne!(catalog(&host).child_button_id("bounded"), Some(original_id));
    assert!(!catalog(&host).accepts(&original));
    assert!(catalog(&host).take_intents(&actions).is_empty());
}
