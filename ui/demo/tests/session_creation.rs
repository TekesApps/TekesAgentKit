use std::{sync::Arc, time::Duration};
use tekes_agent_contract::CreateSessionRequest;
use tekes_agent_presentation::{ComposerDestination, RunScope};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    InitialPresetChoices, InitialSessionCreation, ModelSelectionPreparation, RunBinding,
};
use tekes_agent_ui_demo::{
    component_demo::{
        CREATE_AUTOMATIC, ComponentDemo, CreateSessionCommand, DemoCommand, NEW_SESSION, SELECT_B,
    },
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
fn input() -> CreateSessionRequest {
    CreateSessionRequest {
        cwd: Some("/fixture".into()),
        workspace_id: None,
        session_id: None,
        agent_preset: None,
    }
}
async fn setup() -> (Fixture, Arc<SessionRuntime>, ComponentDemo) {
    let fixture = Fixture::default().with_presets(false).with_models();
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "A 原草稿".into(),
    )
    .unwrap();
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.configure_creation(input(), "test-create").unwrap();
    (fixture, runtime, view)
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
async fn open(runtime: &SessionRuntime, view: &mut ComponentDemo) {
    let mut commands = press(view, NEW_SESSION);
    let DemoCommand::ReadInitialPresets { request } = commands.remove(0) else {
        panic!("catalog read")
    };
    let source = InitialPresetChoices::load(runtime, 1, "test-presets")
        .await
        .unwrap();
    assert!(view.complete_initial_presets(&request, Ok(source)).unwrap());
}
fn choose(view: &mut ComponentDemo, automatic: bool) -> CreateSessionCommand {
    let target = if automatic {
        CREATE_AUTOMATIC
    } else {
        view.initial_preset_choice_id("coding").unwrap()
    };
    let mut commands = press(view, target);
    assert_eq!(commands.len(), 1);
    let DemoCommand::CreateSession(command) = commands.remove(0) else {
        panic!("captured create")
    };
    command
}
async fn execute(runtime: Arc<SessionRuntime>, command: &CreateSessionCommand) -> Arc<RunBinding> {
    let creator = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let created = creator
        .create_initial_session(command.creation())
        .await
        .unwrap();
    Arc::new(
        RunBinding::open_main(runtime, RunScope::main("fixture", created.session_id))
            .await
            .unwrap(),
    )
}
#[tokio::test]
async fn interactive_choice_creates_once_and_normal_composer_sends_without_touching_parent_draft() {
    let (fixture, runtime, mut view) = setup().await;
    open(&runtime, &mut view).await;
    let command = choose(&mut view, false);
    let wrong = view.bindings()[1].clone();
    assert!(view.complete_creation(&command, Ok(wrong)).is_err());
    assert!(press(&mut view, NEW_SESSION).is_empty());
    assert!(press(&mut view, CREATE_AUTOMATIC).is_empty());
    let binding = execute(runtime.clone(), &command).await;
    assert!(
        view.complete_creation(&command, Ok(binding.clone()))
            .unwrap()
    );
    assert_eq!(view.binding().scope(), binding.scope());
    assert_eq!(
        view.bindings()[0].current_run().unwrap().draft.text,
        "A 原草稿"
    );
    assert_eq!(fixture.creation_receipts().len(), 1);
    assert_eq!(fixture.creation_receipts()[0]["agentPreset"], "coding");
    let ids = view.ids();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Commit {
        text: "新会话的中文消息".into(),
        replacement: None,
    })
    .unwrap();
    let mut commands = press(&mut view, ids.send);
    let DemoCommand::Send { binding, ticket } = commands.remove(0) else {
        panic!("existing send")
    };
    binding
        .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let snapshot = binding.current_run().unwrap();
            if snapshot.can_submit
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .any(|i| i.kind == tekes_agent_presentation::TranscriptItemKind::Assistant)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        binding
            .current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .filter(|i| i.text == "新会话的中文消息")
            .count(),
        1
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn late_catalog_creation_and_error_do_not_steal_new_navigation() {
    let (fixture, runtime, mut view) = setup().await;
    let mut commands = press(&mut view, NEW_SESSION);
    let DemoCommand::ReadInitialPresets { request } = commands.remove(0) else {
        panic!("read")
    };
    press(&mut view, SELECT_B);
    let source = InitialPresetChoices::load(&runtime, 1, "late")
        .await
        .unwrap();
    assert!(!view.complete_initial_presets(&request, Ok(source)).unwrap());
    assert!(view.initial_preset_choice_id("coding").is_none());
    view.select(0).unwrap();
    open(&runtime, &mut view).await;
    let command = choose(&mut view, false);
    press(&mut view, SELECT_B);
    let binding = execute(runtime.clone(), &command).await;
    assert!(!view.complete_creation(&command, Ok(binding)).unwrap());
    assert_eq!(view.binding().scope().session_id, "b");
    assert!(
        !view
            .complete_creation(&command, Err("late error".into()))
            .unwrap()
    );
    assert_eq!(fixture.creation_receipts().len(), 1);
    assert_eq!(
        view.bindings()[0].current_run().unwrap().draft.text,
        "A 原草稿"
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn automatic_creation_retains_immutable_identities_and_unknown_error_blocks_replacement_clicks()
 {
    let (fixture, runtime, mut view) = setup().await;
    open(&runtime, &mut view).await;
    let command = choose(&mut view, true);
    let first = command.creation().execute(&runtime).await.unwrap();
    let again = command.creation().execute(&runtime).await.unwrap();
    assert_eq!(first.session_id, again.session_id);
    assert_eq!(first.session_id, command.creation().session_id());
    assert_eq!(fixture.creation_receipts().len(), 1);
    assert!(fixture.creation_receipts()[0].get("agentPreset").is_none());
    assert!(
        !view
            .complete_creation(&command, Err("response lost".into()))
            .unwrap()
    );
    assert!(press(&mut view, NEW_SESSION).is_empty());
    assert!(press(&mut view, CREATE_AUTOMATIC).is_empty());
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name.contains("不会自动重试"))
    );
    assert_eq!(fixture.creation_receipts().len(), 1);
    assert!(
        InitialSessionCreation::automatic(
            "fixture",
            CreateSessionRequest {
                agent_preset: Some("coding".into()),
                ..input()
            },
            "invalid"
        )
        .is_err()
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn marked_text_blocks_new_session_and_late_completion_preserves_current_composition() {
    let (_, runtime, mut view) = setup().await;
    let ids = view.ids();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Preedit {
        text: "组字".into(),
        selection: 2..2,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(press(&mut view, NEW_SESSION).is_empty());
    view.event(&ContentEvent::Commit {
        text: "中文".into(),
        replacement: None,
    })
    .unwrap();
    open(&runtime, &mut view).await;
    let command = choose(&mut view, false);
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Preedit {
        text: "继续组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    let binding = execute(runtime.clone(), &command).await;
    assert!(!view.complete_creation(&command, Ok(binding)).unwrap());
    assert!(view.composing());
    assert_eq!(view.binding().scope().session_id, "a");
    runtime.stop().await.unwrap();
}
