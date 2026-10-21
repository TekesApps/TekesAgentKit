use async_trait::async_trait;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{SidePanel, SidePanelTab, SidePanelTabKind};
use tekes_agent_contract::{
    EndpointCapabilities, EndpointConnector, FrameStream, HistoryPage, HistoryRequest,
    HostDescription, SessionEndpoint, StreamTarget,
};
use tekes_agent_contract::{PromptMode, Result};
use tekes_agent_presentation::{
    ComposerDestination, PreparedSubmission, RunScope, SideChatPhase, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{PaneAccess, RunBinding, SideChatBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand, SELECT_B},
    fixture::Fixture,
};
use tekes_ui_controls::composition::*;
use tokio::sync::Notify;

#[derive(Clone)]
struct InterruptedDiscard {
    fixture: Fixture,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}
struct InterruptedEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    connector: InterruptedDiscard,
}
#[async_trait]
impl EndpointConnector for InterruptedDiscard {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(InterruptedEndpoint {
            inner: self.fixture.connect().await?,
            connector: self.clone(),
        }))
    }
}
#[async_trait]
impl SessionEndpoint for InterruptedEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        self.inner.capabilities()
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn respond(&self, session: &str, id: i64, response: Value) -> Result<()> {
        self.inner.respond(session, id, response).await
    }
    async fn call(&self, method: &str, params: Value, rpc: &str) -> Result<Value> {
        let value = self.inner.call(method, params, rpc).await?;
        if method == "session.discard" {
            // Server performed the mutation, but its reply has not arrived.
            self.connector.entered.notify_one();
            self.connector.release.notified().await;
        }
        Ok(value)
    }
    async fn close(&self) {
        self.inner.close().await;
    }
}

struct Prepare;
#[async_trait::async_trait]
impl SubmissionPreparation for Prepare {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}
async fn setup() -> (Arc<SessionRuntime>, Arc<SideChatBinding>, ComponentDemo) {
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(Fixture::default().with_side_chats()),
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
    let source = Arc::new(SideChatBinding::new(runtime.clone()));
    let mut view = ComponentDemo::new([a, b]).unwrap();
    view.set_side_chat_binding(source.clone());
    view.sync().unwrap();
    (runtime, source, view)
}
fn press(view: &mut ComponentDemo, id: ComponentId) -> Vec<DemoCommand> {
    view.accessibility_action(id, None, AccessibilityAction::Press)
        .unwrap()
}
fn create_request(view: &mut ComponentDemo) -> tekes_agent_ui_demo::component_demo::PanelRequest {
    let id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .create_side_chat_id();
    let mut commands = press(view, id);
    assert_eq!(commands.len(), 1);
    let DemoCommand::CreateSideChat(request) = commands.remove(0) else {
        panic!("create intent")
    };
    request
}

#[tokio::test]
async fn parent_projection_loss_retires_the_pane_but_rebuilds_from_its_original_draft_and_pointer()
{
    let (runtime, source, mut view) = setup().await;
    view.enable_transcript_content().unwrap();
    let parent = view.binding().scope().clone();
    let request = create_request(&mut view);
    let chat = source
        .create(parent.clone(), None, "projection-create")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    let scratch = run.binding().clone();
    view.install_side_chat_run(request, run).unwrap();
    let ids = view.panel_composer_ids().unwrap();
    let old_document = view.transcript_content_id(ids).unwrap();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Preedit {
        text: "面板组字".into(),
        selection: 1..3,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(
        view.set_panel_parent_projection_ready(&parent, false)
            .is_err()
    );
    assert!(view.tree.get(ids.root).is_some());
    view.event(&ContentEvent::Commit {
        text: "完整面板草稿".into(),
        replacement: None,
    })
    .unwrap();
    let draft = scratch.current_run().unwrap().draft.text.clone();
    assert!(draft.contains("完整面板草稿"));
    view.set_panel_parent_projection_ready(&parent, false)
        .unwrap();
    assert!(view.tree.get(ids.root).is_none());
    assert!(source.pointer(&parent, &chat.scope.session_id).is_ok());
    assert!(view.has_side_chat_run(&chat));
    assert!(view.begin_child_open(&parent).is_err());
    view.set_panel_parent_projection_ready(&parent, true)
        .unwrap();
    assert_eq!(view.panel_composer_ids(), Some(ids));
    let new_document = view.transcript_content_id(ids).unwrap();
    assert_ne!(new_document, old_document);
    assert!(view.tree.get(old_document).is_none());
    assert_eq!(
        view.tree
            .get(ids.root)
            .unwrap()
            .component::<tekes_agent_components_ui::Composer>()
            .unwrap()
            .editor()
            .input
            .text,
        draft
    );
    view.select(1).unwrap();
    assert!(view.tree.get(ids.root).is_none());
    view.select(0).unwrap();
    assert_ne!(view.transcript_content_id(ids), Some(new_document));
    assert_eq!(scratch.current_run().unwrap().draft.text, draft);
    assert!(scratch.current_run().unwrap().submissions.is_empty());
    assert!(source.pointer(&parent, &chat.scope.session_id).is_ok());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn a_create_reply_from_before_projection_loss_cannot_reselect_after_ready_returns() {
    let (runtime, source, mut view) = setup().await;
    let parent = view.binding().scope().clone();
    let request = create_request(&mut view);
    let chat = source
        .create(parent.clone(), None, "late-projection-create")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    view.set_panel_parent_projection_ready(&parent, false)
        .unwrap();
    view.set_panel_parent_projection_ready(&parent, true)
        .unwrap();
    assert!(!view.install_side_chat_run(request, run).unwrap());
    assert!(view.panel_composer_ids().is_none());
    assert!(view.has_side_chat_run(&chat));
    assert!(source.pointer(&parent, &chat.scope.session_id).is_ok());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn late_creation_keeps_its_parent_and_never_steals_another_sessions_panel() {
    let (runtime, source, mut view) = setup().await;
    let request = create_request(&mut view);
    let parent = request.parent().clone();
    assert!(press(&mut view, SELECT_B).is_empty());
    let chat = source
        .create(parent.clone(), Some("Parent A"), "create-a")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    assert!(!view.install_side_chat_run(request, run).unwrap());
    assert!(view.has_side_chat_run(&chat));
    assert!(view.panel_composer_ids().is_none());
    let input = source
        .input(
            1,
            view.binding().scope(),
            vec![SidePanelTab::summary("Summary")],
            None,
        )
        .unwrap();
    assert_eq!(input.tabs.len(), 1);
    assert!(
        input.side_chats.chats.contains_key(&chat.scope.session_id),
        "full owner snapshot also contains inactive parents"
    );
    assert!(
        source
            .pointer(view.binding().scope(), &chat.scope.session_id)
            .is_err()
    );
    let mut foreign = chat.clone();
    foreign.parent = view.binding().scope().clone();
    assert!(source.close(&foreign, "foreign-close").await.is_err());
    assert_eq!(source.snapshot().chats[&chat.scope.session_id], chat);
    source.close(&chat, "owned-close").await.unwrap();
    view.sync().unwrap();
    assert!(
        !view.has_side_chat_run(&chat),
        "confirmed discard also retires inactive parent content"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn scratch_composer_ime_send_and_close_use_its_own_run_without_touching_parent_drafts() {
    let (runtime, source, mut view) = setup().await;
    let parent = view.binding().clone();
    parent
        .edit_text(
            &ComposerDestination::run(parent.scope().clone()),
            "父草稿".into(),
        )
        .unwrap();
    let request = create_request(&mut view);
    let chat = source
        .create(request.parent().clone(), None, "create")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    let scratch = run.binding().clone();
    assert!(view.install_side_chat_run(request, run).unwrap());
    let ids = view.panel_composer_ids().unwrap();
    assert_ne!(ids, view.ids());
    view.tree.focus(Some(ids.editor), true);
    assert_eq!(view.tree.focused(), Some(ids.editor));
    view.event(&ContentEvent::Preedit {
        text: "中文组字".into(),
        selection: 4..4,
        clauses: vec![],
        replacement: None,
    })
    .unwrap();
    assert!(view.composing());
    assert!(press(&mut view, ids.send).is_empty());
    assert!(press(&mut view, SELECT_B).is_empty());
    assert!(
        view.event(&ContentEvent::Commit {
            text: "SideChat 正文".into(),
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    let mut commands = press(&mut view, ids.send);
    assert_eq!(commands.len(), 1);
    let DemoCommand::Send { binding, ticket } = commands.remove(0) else {
        panic!("scratch send")
    };
    assert!(Arc::ptr_eq(&binding, &scratch));
    binding
        .dispatch_prepared_with(ticket, &Prepare)
        .await
        .unwrap();
    let mut changes = scratch.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !runtime.inspect(|state| state.sessions[&chat.scope.session_id].run_open) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    view.tree.focus(Some(ids.editor), true);
    view.event(&ContentEvent::Commit {
        text: "独立暂存草稿".into(),
        replacement: None,
    })
    .unwrap();
    assert_eq!(scratch.current_run().unwrap().draft.text, "独立暂存草稿");
    assert_eq!(parent.current_run().unwrap().draft.text, "父草稿");
    let close_id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let mut commands = press(&mut view, close_id);
    let DemoCommand::CloseSideChat(expected) = commands.remove(0) else {
        panic!("owned close")
    };
    assert_eq!(expected, chat);
    source.close(&expected, "close-running").await.unwrap();
    assert_eq!(scratch.access(), PaneAccess::Missing);
    view.sync().unwrap();
    assert!(view.panel_composer_ids().is_none());
    assert!(!view.has_side_chat_run(&chat));
    assert_eq!(parent.current_run().unwrap().draft.text, "父草稿");
    assert!(
        scratch
            .send_with(
                &ComposerDestination::run(chat.scope),
                PromptMode::Queue,
                &Prepare
            )
            .await
            .is_err()
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn stale_callbacks_and_failed_reads_preserve_pointer_until_authoritative_absence() {
    let (runtime, source, mut view) = setup().await;
    let request = create_request(&mut view);
    let chat = source
        .create(request.parent().clone(), None, "create")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    let binding = run.binding().clone();
    view.install_side_chat_run(request, run).unwrap();
    let destination = ComposerDestination::run(chat.scope.clone());
    binding.edit_text(&destination, "保留草稿".into()).unwrap();
    // Test the existing controller's public reconciliation contract without a
    // UI-specific mutation store or access to a harness's private database.
    let mut unknown = chat.clone();
    unknown.phase = SideChatPhase::Unknown;
    // A stale expected phase must be rejected before closing the current pointer.
    assert!(source.close(&unknown, "stale-close").await.is_err());
    assert!(!source.reconcile(&chat, Err("read failed".into())));
    assert_eq!(binding.current_run().unwrap().draft.text, "保留草稿");
    assert!(
        !source.reconcile(&unknown, Ok(false)),
        "stale expected value cannot remove a live pointer"
    );
    assert!(source.reconcile(&chat, Ok(false)));
    view.sync().unwrap();
    assert!(view.panel_composer_ids().is_none());
    assert!(source.open(&chat).await.is_err());
    assert_eq!(source.support_snapshot(), Some(true));
    runtime.stop().await.unwrap();
    assert_eq!(source.support_snapshot(), None);
    // Endpoint mismatch and host-supplied scratch tabs cannot enter the model.
    assert!(
        source
            .input(2, &RunScope::main("other", "a"), vec![], None)
            .is_err()
    );
    assert!(
        source
            .input(
                3,
                &chat.parent,
                vec![SidePanelTab::side_chat(&chat)],
                Some(SidePanelTabKind::Summary)
            )
            .is_err()
    );
}

#[tokio::test]
async fn interrupted_discard_keeps_readable_draft_and_disables_mutations_until_reconciled() {
    let connector = InterruptedDiscard {
        fixture: Fixture::default().with_side_chats(),
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let runtime = Arc::new(SessionRuntime::spawn(
        "fixture",
        Arc::new(connector.clone()),
    ));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let source = Arc::new(SideChatBinding::new(runtime.clone()));
    let chat = source
        .create(RunScope::main("fixture", "a"), None, "create")
        .await
        .unwrap();
    let run = source.open(&chat).await.unwrap();
    let binding = run.binding();
    let target = ComposerDestination::run(chat.scope.clone());
    binding
        .edit_text(&target, "未确认关闭时保留".into())
        .unwrap();
    let closing_source = source.clone();
    let closing_chat = chat.clone();
    let closing = tokio::spawn(async move { closing_source.close(&closing_chat, "discard").await });
    tokio::time::timeout(Duration::from_secs(3), connector.entered.notified())
        .await
        .unwrap();
    assert_eq!(binding.access(), PaneAccess::Closing);
    assert!(
        !source.reconcile(&source.snapshot().chats[&chat.scope.session_id], Ok(false)),
        "in-flight close cannot be reconciled out from under its guard"
    );
    closing.abort();
    assert!(closing.await.unwrap_err().is_cancelled());
    assert_eq!(binding.access(), PaneAccess::Unknown);
    assert_eq!(
        binding.current_run().unwrap().draft.text,
        "未确认关闭时保留"
    );
    assert!(!binding.access().composer_gate().send);
    assert!(
        binding
            .send_with(&target, PromptMode::Queue, &Prepare)
            .await
            .is_err()
    );
    assert!(binding.cancel(&target).await.is_err());
    let unknown = source.snapshot().chats[&chat.scope.session_id].clone();
    assert!(!source.reconcile(&unknown, Err("read offline".into())));
    assert_eq!(binding.access(), PaneAccess::Unknown);
    let input = source
        .input(
            7,
            &chat.parent,
            vec![],
            Some(SidePanelTabKind::SideChat {
                session_id: chat.scope.session_id.clone(),
            }),
        )
        .unwrap();
    assert!(input.selected.is_some(), "Unknown is not disappearance");
    assert_eq!(
        input.side_chats.chats[&chat.scope.session_id].phase,
        SideChatPhase::Unknown
    );
    // The fixture above confirms discard execution; this explicit host result
    // stands for an authoritative existence query, never inventory absence.
    assert!(source.reconcile(&unknown, Ok(false)));
    assert_eq!(binding.access(), PaneAccess::Missing);
    connector.release.notify_one();
    runtime.stop().await.unwrap();
}
