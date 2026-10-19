use tekes_agent_components_ui::{
    RenameFailure, RenameIntent, RenameLabels, RenameSubmission, ThreadRename,
};
use tekes_agent_contract::{SessionAddress, ThreadActionDefinition};
use tekes_ui_controls::{ControlKey, composition::*};
fn target(id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: "native".into(),
        session_id: id.into(),
    }
}
fn enter() -> ContentKeyEvent {
    ContentKeyEvent {
        key: Some(ControlKey::Enter),
        down: true,
        repeat: false,
        modifiers: Default::default(),
        native_code: 36,
        characters: "\r".into(),
        characters_ignoring_modifiers: "\r".into(),
    }
}
fn definition() -> ThreadActionDefinition {
    ThreadActionDefinition {
        id: "rename".into(),
        label: "重命名".into(),
        icon: "pencil".into(),
        interaction: "rename".into(),
        enabled: true,
        disabled_reason: None,
        copy_text: None,
    }
}
fn panel(host: &ContentHost, root: ComponentId) -> &ThreadRename {
    host.get(root).unwrap().component::<ThreadRename>().unwrap()
}
fn panel_mut(host: &mut ContentHost, root: ComponentId) -> &mut ThreadRename {
    host.get_mut(root)
        .unwrap()
        .component_mut::<ThreadRename>()
        .unwrap()
}
fn drain(host: &mut ContentHost, root: ComponentId) -> Vec<RenameIntent> {
    let actions = host.drain_actions();
    panel_mut(host, root).take_intents(&actions)
}
fn submit(host: &mut ContentHost, root: ComponentId) -> RenameSubmission {
    let id = panel(host, root).ids().submit;
    host.accessibility_action(id, None, AccessibilityAction::Press);
    let mut intents = drain(host, root);
    assert_eq!(intents.len(), 1);
    let RenameIntent::Submit(ticket) = intents.remove(0) else {
        panic!("rename")
    };
    *ticket
}
fn edit(host: &mut ContentHost, root: ComponentId, text: &str) {
    let editor = panel(host, root).ids().editor;
    host.accessibility_action(editor, None, AccessibilityAction::SetValue(text.into()));
    drain(host, root);
}
#[test]
fn rename_ime_whitespace_and_double_submit_use_upstream_editing_without_touching_run_drafts() {
    let mut rename = ThreadRename::new(75, RenameLabels::default());
    rename.open(target("a"), definition(), "原会话").unwrap();
    let ids = rename.ids();
    let mut host = rename.into_host().unwrap();
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    });
    assert!(drain(&mut host, ids.root).is_empty());
    host.accessibility_action(ids.submit, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    assert!(panel(&host, ids.root).is_composing());
    host.event(&ContentEvent::TextKeyStart {
        id: 1,
        key: enter(),
        composing: true,
    });
    host.event(&ContentEvent::Commit {
        text: "中文".into(),
        replacement: None,
    });
    host.event(&ContentEvent::TextKeyEnd {
        id: 1,
        input_context_consumed: false,
    });
    assert!(
        drain(&mut host, ids.root).is_empty(),
        "IME commit Enter is not a rename submission"
    );
    edit(&mut host, ids.root, "   ");
    host.accessibility_action(ids.submit, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    edit(&mut host, ids.root, " 中文名称 ");
    let ticket = submit(&mut host, ids.root);
    assert_eq!(ticket.title, "中文名称");
    assert_eq!(ticket.address, target("a"));
    host.accessibility_action(ids.submit, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    assert!(panel(&host, ids.root).is_pending());
}
#[test]
fn uncertain_retry_keeps_identity_but_known_rejection_and_new_payload_get_new_attempts() {
    let mut rename = ThreadRename::new(76, RenameLabels::default());
    rename.open(target("a"), definition(), "Name").unwrap();
    let root = rename.ids().root;
    let mut host = rename.into_host().unwrap();
    let first = submit(&mut host, root);
    panel_mut(&mut host, root).complete(
        &first,
        Err(RenameFailure {
            message: "reply lost".into(),
            uncertain: true,
        }),
    );
    assert_eq!(panel(&host, root).editor().unwrap().input.text, "Name");
    let retry = submit(&mut host, root);
    assert_eq!(retry, first);
    panel_mut(&mut host, root).complete(
        &retry,
        Err(RenameFailure {
            message: "known rejection".into(),
            uncertain: false,
        }),
    );
    let new = submit(&mut host, root);
    assert_ne!(new.attempt, first.attempt);
    panel_mut(&mut host, root).complete(
        &new,
        Err(RenameFailure {
            message: "unknown".into(),
            uncertain: true,
        }),
    );
    edit(&mut host, root, "Changed");
    let changed = submit(&mut host, root);
    assert_ne!(changed.attempt, new.attempt);
    assert!(!panel_mut(&mut host, root).complete(&first, Ok(())));
    assert!(panel_mut(&mut host, root).complete(&changed, Ok(())));
    assert!(!panel(&host, root).is_open());
}
#[test]
fn superseded_targets_retire_widget_ids_and_late_results_cannot_restore_the_previous_title() {
    let mut rename = ThreadRename::new(77, RenameLabels::default());
    rename.open(target("a"), definition(), "A").unwrap();
    let old_ids = rename.ids();
    let mut host = rename.into_host().unwrap();
    let first = submit(&mut host, old_ids.root);
    panel_mut(&mut host, old_ids.root)
        .open(target("b"), definition(), "B")
        .unwrap();
    let ids = panel(&host, old_ids.root).ids();
    assert_ne!(ids.editor, old_ids.editor);
    assert!(!panel_mut(&mut host, ids.root).complete(
        &first,
        Err(RenameFailure {
            message: "late A".into(),
            uncertain: true
        })
    ));
    host.accessibility_action(old_ids.submit, None, AccessibilityAction::Press);
    assert!(drain(&mut host, ids.root).is_empty());
    assert_eq!(panel(&host, ids.root).editor().unwrap().input.text, "B");
    host.focus(Some(ids.editor), true);
    host.event(&ContentEvent::KeyInput(enter()));
    let intents = drain(&mut host, ids.root);
    assert!(
        matches!(intents.as_slice(),[RenameIntent::Submit(ticket)] if ticket.address == target("b"))
    );
}
