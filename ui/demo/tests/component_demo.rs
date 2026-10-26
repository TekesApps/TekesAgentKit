use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{Result, require};
use tekes_agent_presentation::{
    ComposerDestination, PreparedSubmission, RunScope, RunSnapshot, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_A, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::{ControlKey, composition::*};

async fn setup() -> (Arc<SessionRuntime>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let a = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "b"))
            .await
            .unwrap(),
    );
    (runtime, ComponentDemo::new([a, b]).unwrap())
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
fn commit(view: &mut ComponentDemo, text: &str) {
    assert!(
        view.event(&ContentEvent::Commit {
            text: text.into(),
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
}
async fn until(binding: &RunBinding, predicate: impl Fn(&RunSnapshot) -> bool) {
    let mut changes = binding.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate(&binding.current_run().unwrap()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
struct NoSettings;
#[async_trait::async_trait]
impl SubmissionPreparation for NoSettings {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        ticket: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        require(
            ticket.settings().is_empty(),
            "explicit host settings required",
        )
    }
}

#[tokio::test]
async fn abandoned_local_admission_restores_controlled_draft_without_automatic_retry() {
    let (runtime, mut view) = setup().await;
    let ids = view.ids();
    let original_tail = runtime.inspect(|store| store.sessions["a"].tail_sequence);
    view.tree.focus(Some(ids.editor), true);
    commit(&mut view, "尚未发出的草稿");
    let commands: [DemoCommand; 1] = press(&mut view, ids.send)
        .try_into()
        .ok()
        .expect("captured send");
    let [DemoCommand::Send { binding, ticket }] = commands else {
        panic!("send")
    };
    assert_eq!(view.editor().input.text, "");
    binding.cancel_prepared(ticket).unwrap();
    view.sync().unwrap();
    assert_eq!(view.editor().input.text, "尚未发出的草稿");
    // The existing failed optimistic echo is presentation, not a durable remote
    // message. Do not remove it just to make preflight cleanup look successful.
    assert_eq!(
        runtime.inspect(|store| store.sessions["a"].tail_sequence),
        original_tail
    );
    assert_eq!(
        binding
            .current_run()
            .unwrap()
            .submissions
            .last()
            .unwrap()
            .phase,
        tekes_agent_presentation::SubmissionPhase::Failed
    );
    // A later host notification cannot turn a local preflight cancellation into
    // another send. Only the user's next activation produces another command.
    assert!(view.drain_commands().unwrap().is_empty());
    let commands: [DemoCommand; 1] = press(&mut view, ids.send)
        .try_into()
        .ok()
        .expect("explicit retry");
    let [DemoCommand::Send { binding, ticket }] = commands else {
        panic!("send")
    };
    binding.cancel_prepared(ticket).unwrap();
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn component_host_preserves_ime_barrier_and_each_real_composer_instance() {
    let (runtime, mut view) = setup().await;
    let a_ids = view.ids();
    assert!(view.tree.focus(Some(a_ids.editor), true));
    view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(view.composing());
    assert!(press(&mut view, a_ids.send).is_empty());
    assert!(press(&mut view, SELECT_B).is_empty());
    assert!(view.select(1).is_err());
    assert_eq!(view.selected(), 0);
    assert!(
        view.event(&ContentEvent::Key {
            key: ControlKey::Enter,
            down: true
        })
        .unwrap()
        .is_empty()
    );
    commit(&mut view, "草稿 A");
    assert!(!view.composing());
    let a_draft = view.editor().input.text.clone();
    assert_eq!(view.binding().current_run().unwrap().draft.text, a_draft);
    assert!(press(&mut view, SELECT_B).is_empty());
    let b_ids = view.ids();
    assert_ne!(a_ids, b_ids);
    assert_eq!(view.editor().input.text, "");
    commit(&mut view, "草稿 B");
    view.accessibility_action(b_ids.editor, None, AccessibilityAction::SetSelection(1..3))
        .unwrap();
    assert_eq!(view.editor().input.selection(), 1..3);
    press(&mut view, SELECT_A);
    assert_eq!(view.ids(), a_ids);
    assert_eq!(view.editor().input.text, a_draft);
    press(&mut view, SELECT_B);
    assert_eq!(view.editor().input.text, "草稿 B");
    assert_eq!(view.editor().input.selection(), 1..3);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn native_host_command_freezes_admission_and_later_edit_and_navigation_cannot_retarget_it() {
    let (runtime, mut view) = setup().await;
    let ids = view.ids();
    view.tree.focus(Some(ids.editor), true);
    commit(&mut view, "只发送这条");
    let commands = press(&mut view, ids.send);
    let commands: [DemoCommand; 1] = commands.try_into().ok().expect("one captured send");
    let [DemoCommand::Send { binding, ticket }] = commands else {
        panic!("send")
    };
    assert_eq!(
        view.editor().input.text,
        "",
        "controlled clear occurs before spawning transport"
    );
    assert_eq!(
        ticket.destination(),
        &ComposerDestination::run(RunScope::main("fixture", "a"))
    );
    view.tree.focus(Some(ids.editor), true);
    commit(&mut view, "之后的新草稿");
    press(&mut view, SELECT_B);
    commit(&mut view, "B 独立草稿");
    binding
        .dispatch_prepared_with(ticket, &NoSettings)
        .await
        .unwrap();
    until(&binding, |snapshot| {
        snapshot
            .transcript
            .items
            .iter()
            .any(|item| item.is_provisional)
    })
    .await;
    until(&binding, |snapshot| {
        !snapshot
            .transcript
            .items
            .iter()
            .any(|item| item.is_provisional)
            && snapshot.run.as_ref().is_some_and(|run| !run.can_cancel)
    })
    .await;
    let snapshot = binding.current_run().unwrap();
    assert_eq!(
        snapshot
            .transcript
            .items
            .iter()
            .filter(|item| item.text == "只发送这条")
            .count(),
        1
    );
    assert!(
        !snapshot
            .transcript
            .items
            .iter()
            .any(|item| item.text == "之后的新草稿")
    );
    assert_eq!(snapshot.draft.text, "之后的新草稿");
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    until(&binding, |snapshot| !snapshot.transcript.is_stale).await;
    until(view.binding(), |snapshot| !snapshot.transcript.is_stale).await;
    view.sync().unwrap();
    assert_eq!(view.editor().input.text, "B 独立草稿");
    press(&mut view, SELECT_A);
    assert_eq!(view.editor().input.text, "之后的新草稿");
    assert_eq!(
        view.binding()
            .current_run()
            .unwrap()
            .transcript
            .items
            .iter()
            .filter(|item| item.text == "只发送这条")
            .count(),
        1
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn canonical_content_mounts_per_session_and_preserves_original_widgets_and_drafts() {
    use tekes_agent_components_ui::TranscriptContent;
    let (_runtime, mut view) = setup().await;
    view.enable_transcript_content().unwrap();
    let a_ids = view.ids();
    let a_root = view.transcript_content_id(a_ids).unwrap();
    let rows = view
        .tree
        .get(a_root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap()
        .row_keys()
        .to_vec();
    let first = rows.first().map(|key| {
        view.tree
            .get(a_root)
            .unwrap()
            .component::<TranscriptContent>()
            .unwrap()
            .row_id(key)
            .unwrap()
    });
    view.tree.focus(Some(a_ids.editor), true);
    commit(&mut view, "A 内容树草稿");
    view.select(1).unwrap();
    let b_ids = view.ids();
    let b_root = view.transcript_content_id(b_ids).unwrap();
    assert_ne!(a_root, b_root);
    view.tree.focus(Some(b_ids.editor), true);
    commit(&mut view, "B 内容树草稿");
    view.select(0).unwrap();
    assert_eq!(view.editor().input.text, "A 内容树草稿");
    assert_eq!(view.transcript_content_id(view.ids()), Some(a_root));
    let content = view
        .tree
        .get(a_root)
        .unwrap()
        .component::<TranscriptContent>()
        .unwrap();
    assert_eq!(content.row_keys(), rows);
    if let Some(key) = rows.first() {
        assert_eq!(content.row_id(key), first);
    }
    view.select(1).unwrap();
    assert_eq!(view.editor().input.text, "B 内容树草稿");
    assert_eq!(view.transcript_content_id(view.ids()), Some(b_root));
}
