//! An independent consumer uses only the public AgentKit component and SDK.
use tekes_agent_components_ui::*;
use tekes_ui_controls::composition::*;

fn model(owner: SidePanelLauncherOwner, revision: u64) -> SidePanelLauncherInput {
    SidePanelLauncherInput {
        revision,
        owner,
        destinations: SidePanelDestination::ALL
            .into_iter()
            .map(|destination| {
                let mut item = SidePanelDestinationItem::unavailable(destination);
                item.available = true;
                item.enabled = true;
                item
            })
            .collect(),
    }
}
fn component(host: &ContentHost, root: ComponentId) -> &SidePanelLauncher {
    host.get(root)
        .unwrap()
        .component::<SidePanelLauncher>()
        .unwrap()
}
fn update(host: &mut ContentHost, root: ComponentId, input: &SidePanelLauncherInput) {
    host.get_mut(root)
        .unwrap()
        .component_mut::<SidePanelLauncher>()
        .unwrap()
        .update(input)
        .unwrap();
}
fn press(
    host: &mut ContentHost,
    root: ComponentId,
    destination: SidePanelDestination,
) -> SidePanelLauncherIntent {
    let id = component(host, root).button_id(destination).unwrap();
    assert!(
        host.accessibility_action(id, None, AccessibilityAction::Press)
            .activated
    );
    let actions = host.drain_actions();
    let mut intents = component(host, root).take_intents(&actions);
    assert_eq!(intents.len(), 1);
    intents.remove(0)
}

#[test]
fn workspace_callbacks_are_explicit_and_stale_owner_revision_and_remount_are_rejected() {
    let owner = SidePanelLauncherOwner::Workspace {
        id: "workspace-with-no-session".into(),
    };
    let mut input = model(owner.clone(), 1);
    let mut view = SidePanelLauncher::new(63100, "Side Panel Views");
    view.update(&input).unwrap();
    let root = view.id();
    let mut host = view.into_host().unwrap();
    let intent = press(&mut host, root, SidePanelDestination::Browser);
    assert_eq!(intent.owner, owner);
    assert_eq!(intent.destination, SidePanelDestination::Browser);
    assert!(component(&host, root).accepts(&intent));
    let old = component(&host, root)
        .button_id(SidePanelDestination::Browser)
        .unwrap();
    input.revision = 2;
    input.owner = SidePanelLauncherOwner::Run(RunScope::main("endpoint", "a"));
    update(&mut host, root, &input);
    assert!(host.get(old).is_none());
    assert!(!component(&host, root).accepts(&intent));
    input.revision = 3;
    input.owner = owner.clone();
    update(&mut host, root, &input);
    assert_ne!(
        component(&host, root).button_id(SidePanelDestination::Browser),
        Some(old)
    );
    assert!(!component(&host, root).accepts(&intent));
    let mut remount = SidePanelLauncher::new(63100, "same numerical IDs, new mount");
    remount.update(&model(owner, 1)).unwrap();
    assert_eq!(remount.button_id(SidePanelDestination::Browser), Some(old));
    assert!(!remount.accepts(&intent));
}

#[test]
fn unavailable_destinations_are_absent_and_disabled_original_buttons_do_not_activate() {
    let mut input = model(
        SidePanelLauncherOwner::Workspace {
            id: "workspace".into(),
        },
        1,
    );
    input.destinations[1].available = false;
    input.destinations[2].enabled = false;
    let mut launcher = SidePanelLauncher::new(63101, "Views");
    launcher.update(&input).unwrap();
    assert!(launcher.button_id(SidePanelDestination::Review).is_none());
    let terminal = launcher.button_id(SidePanelDestination::Terminal).unwrap();
    let root = launcher.id();
    let mut host = launcher.into_host().unwrap();
    assert!(
        !host
            .accessibility_action(terminal, None, AccessibilityAction::Press)
            .activated
    );
    assert!(host.drain_actions().is_empty());
    let old = component(&host, root)
        .button_id(SidePanelDestination::Files)
        .unwrap();
    input.revision = 2;
    input.destinations[4].available = false;
    update(&mut host, root, &input);
    assert!(host.get(old).is_none());
    input.revision = 3;
    input.destinations[4].available = true;
    update(&mut host, root, &input);
    assert_ne!(
        component(&host, root).button_id(SidePanelDestination::Files),
        Some(old)
    );
    assert_eq!(
        press(&mut host, root, SidePanelDestination::Files).destination,
        SidePanelDestination::Files
    );
}

#[test]
fn invalid_frames_do_not_first_retire_valid_controls() {
    let input = model(
        SidePanelLauncherOwner::Workspace {
            id: "workspace".into(),
        },
        1,
    );
    let mut launcher = SidePanelLauncher::new(63102, "Views");
    launcher.update(&input).unwrap();
    let button = launcher.button_id(SidePanelDestination::Summary);
    let mut invalid = input.clone();
    invalid.revision = 2;
    invalid.destinations[1].destination = SidePanelDestination::Summary;
    assert!(launcher.update(&invalid).is_err());
    assert_eq!(launcher.displayed_input(), Some(&input));
    assert_eq!(launcher.button_id(SidePanelDestination::Summary), button);
    invalid = input.clone();
    invalid.owner = SidePanelLauncherOwner::Workspace { id: "".into() };
    assert!(launcher.update(&invalid).is_err());
    assert_eq!(launcher.button_id(SidePanelDestination::Summary), button);
}

#[test]
fn panel_launcher_is_hidden_while_selected_or_unprojected_and_old_callbacks_do_not_revive() {
    let parent = RunScope::main("endpoint", "a");
    let mut input = SidePanelInput {
        revision: 1,
        parent: parent.clone(),
        projection_ready: true,
        tabs: vec![],
        selected: None,
        side_chats: Default::default(),
        supports_side_chat: None,
    };
    let mut panel = SidePanel::new(63103, Default::default(), 4);
    panel.update(&input).unwrap();
    let mut launcher = model(SidePanelLauncherOwner::Run(parent), 1);
    panel.configure_launcher(63104, &launcher).unwrap();
    let root = panel.id();
    let old = panel
        .launcher()
        .unwrap()
        .button_id(SidePanelDestination::Summary)
        .unwrap();
    let mut host = panel.into_host().unwrap();
    assert!(
        host.accessibility_action(old, None, AccessibilityAction::Press)
            .activated
    );
    let actions = host.drain_actions();
    let intents = host
        .get(root)
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .take_intents(&actions);
    let [SidePanelIntent::Launch(captured)] = intents.as_slice() else {
        panic!("original launcher callback")
    };
    let captured = captured.clone();
    input.revision = 2;
    input.tabs.push(SidePanelTab::summary("Summary"));
    input.selected = Some(SidePanelTabKind::Summary);
    let panel = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap();
    panel.update(&input).unwrap();
    assert!(!panel.accepts_launcher_intent(&captured));
    launcher.revision = 2;
    panel.configure_launcher(63104, &launcher).unwrap();
    assert!(
        panel
            .launcher()
            .unwrap()
            .button_id(SidePanelDestination::Summary)
            .is_none(),
        "hidden publications must not build interactive controls"
    );
    assert!(host.get(old).is_none());
    input.revision = 3;
    input.tabs.clear();
    input.selected = None;
    launcher.revision = 3;
    let panel = host
        .get_mut(root)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap();
    panel.update(&input).unwrap();
    panel.configure_launcher(63104, &launcher).unwrap();
    assert!(!panel.accepts_launcher_intent(&captured));
    let new = panel
        .launcher()
        .unwrap()
        .button_id(SidePanelDestination::Summary)
        .unwrap();
    assert_ne!(new, old);
    input.revision = 4;
    input.projection_ready = false;
    host.get_mut(root)
        .unwrap()
        .component_mut::<SidePanel>()
        .unwrap()
        .update(&input)
        .unwrap();
    assert!(host.get(new).is_none());
}
