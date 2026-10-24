use std::{collections::BTreeMap, sync::Arc, time::Duration};

use tekes_agent_components_ui::{SidePanel, SidePanelTabKind};
use tekes_agent_contract::PromptMode;
use tekes_agent_presentation::{ComposerDestination, RunScope, SideChat};
use tekes_agent_ui_bindings::{AttachmentFile, RunBinding, SideChatBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::AccessibilityAction;

async fn setup() -> (
    Arc<SharedAgent>,
    BTreeMap<String, Arc<tekes_agent_session::SessionRuntime>>,
    BTreeMap<String, Arc<SideChatBinding>>,
    ComponentDemo,
) {
    let mut agent = SharedAgent::default();
    for endpoint in ["endpoint-a", "endpoint-b"] {
        agent
            .register(
                endpoint.into(),
                "native".into(),
                endpoint.into(),
                Arc::new(
                    Fixture::default()
                        .with_endpoint_id(endpoint)
                        .with_uploads()
                        .with_side_chats(),
                ),
            )
            .unwrap();
    }
    let agent = Arc::new(agent);
    let runtimes: BTreeMap<_, _> = ["endpoint-a", "endpoint-b"]
        .into_iter()
        .map(|id| {
            let runtime = agent.endpoint(id).unwrap().runtime.clone();
            (id.to_owned(), runtime)
        })
        .collect();
    for runtime in runtimes.values() {
        runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    }
    let runs = ["endpoint-a", "endpoint-b"].into_iter().map(|endpoint| {
        let runtime = runtimes[endpoint].clone();
        async move {
            Arc::new(
                RunBinding::open_main(runtime, RunScope::main(endpoint, "a"))
                    .await
                    .unwrap(),
            )
        }
    });
    let runs = futures_util::future::join_all(runs).await;
    let owners: BTreeMap<_, _> = ["endpoint-a", "endpoint-b"]
        .into_iter()
        .map(|endpoint| {
            (
                endpoint.to_owned(),
                Arc::new(SideChatBinding::new(runtimes[endpoint].clone())),
            )
        })
        .collect();
    let mut view = ComponentDemo::new(runs).unwrap();
    view.set_file_picker_available(true);
    view.register_side_chat_binding(owners["endpoint-a"].clone())
        .unwrap();
    view.sync().unwrap();
    (agent, runtimes, owners, view)
}

fn press(
    view: &mut ComponentDemo,
    id: tekes_ui_controls::composition::ComponentId,
) -> Vec<DemoCommand> {
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
        panic!("AX create command")
    };
    request
}

fn open_command(view: &mut ComponentDemo, chat: &SideChat) -> DemoCommand {
    view.sync().unwrap();
    let id = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .tab_id(&SidePanelTabKind::SideChat {
            session_id: chat.scope.session_id.clone(),
        })
        .unwrap();
    let mut commands = press(view, id);
    assert_eq!(commands.len(), 1);
    commands.remove(0)
}

fn selected(view: &ComponentDemo) -> Option<SidePanelTabKind> {
    view.tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .displayed_input()
        .unwrap()
        .selected
        .clone()
}

fn file() -> AttachmentFile {
    AttachmentFile::new(
        "多端侧聊.txt".into(),
        "text/plain".into(),
        Arc::from(&b"multi-host attachment"[..]),
    )
    .unwrap()
}

#[tokio::test]
async fn endpoint_scoped_runs_keep_drafts_attachments_and_preparation_across_ax_navigation() {
    let (_agent, runtimes, owners, mut view) = setup().await;
    let owner_a = &owners["endpoint-a"];
    let owner_b = &owners["endpoint-b"];
    view.register_side_chat_binding(owner_b.clone()).unwrap();

    let request_a = create_request(&mut view);
    let chat_a = owner_a
        .create(request_a.parent().clone(), None, "multi-host-a")
        .await
        .unwrap();
    let DemoCommand::OpenSideChat {
        request: open_a_request,
        chat: opened_a,
    } = open_command(&mut view, &chat_a)
    else {
        panic!("AX open command for endpoint A")
    };
    assert_eq!(opened_a, chat_a);
    let run_a = owner_a.open(&chat_a).await.unwrap();
    let binding_a = run_a.binding().clone();
    assert!(view.install_side_chat_run(open_a_request, run_a).unwrap());
    let dest_a = ComposerDestination::run(binding_a.scope().clone());
    binding_a
        .edit_text(&dest_a, "A 端的中文草稿".into())
        .unwrap();
    let ids_a = view.panel_composer_ids().unwrap();
    let mut commands = press(&mut view, ids_a.attachment);
    let DemoCommand::SelectFiles { request: upload } = commands.remove(0) else {
        panic!("AX attachment command")
    };
    let report = upload.upload(&[file()]).await.unwrap();
    view.complete_file_selection(&upload, vec![file()], Ok(Some(report)))
        .unwrap();
    assert_eq!(binding_a.current_run().unwrap().draft.artifacts.len(), 1);
    let prepared = binding_a.prepare_send(&dest_a, PromptMode::Queue).unwrap();

    view.select(1).unwrap();
    let request_b = create_request(&mut view);
    let chat_b = owner_b
        .create(request_b.parent().clone(), None, "multi-host-b")
        .await
        .unwrap();
    let DemoCommand::OpenSideChat {
        request: open_b_request,
        chat: opened_b,
    } = open_command(&mut view, &chat_b)
    else {
        panic!("AX open command for endpoint B")
    };
    assert_eq!(opened_b, chat_b);
    let run_b = owner_b.open(&chat_b).await.unwrap();
    let binding_b = run_b.binding().clone();
    assert!(view.install_side_chat_run(open_b_request, run_b).unwrap());
    binding_b
        .edit_text(
            &ComposerDestination::run(binding_b.scope().clone()),
            "B 端自己的草稿".into(),
        )
        .unwrap();

    assert_eq!(chat_a.parent.session_id, "a");
    assert_eq!(chat_b.parent.session_id, "a");
    assert_eq!(chat_a.scope.session_id, chat_b.scope.session_id);
    assert_ne!(chat_a.parent.endpoint_id, chat_b.parent.endpoint_id);
    assert_eq!(
        binding_b.current_run().unwrap().draft.text,
        "B 端自己的草稿"
    );
    assert!(view.has_side_chat_run_binding(&chat_a, &binding_a));
    assert!(view.has_side_chat_run_binding(&chat_b, &binding_b));

    let close_b = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap()
        .close_id();
    let mut commands = press(&mut view, close_b);
    let DemoCommand::CloseSideChat(closed) = commands.remove(0) else {
        panic!("AX close command for endpoint B")
    };
    assert_eq!(closed, chat_b);
    owner_b.close(&closed, "multi-host-close-b").await.unwrap();
    view.sync().unwrap();
    assert!(!view.has_side_chat_run(&chat_b));
    assert!(view.has_side_chat_run_binding(&chat_a, &binding_a));

    view.select(0).unwrap();
    assert_eq!(
        selected(&view),
        Some(SidePanelTabKind::SideChat {
            session_id: chat_a.scope.session_id.clone(),
        })
    );
    assert_eq!(view.panel_composer_ids(), Some(ids_a));
    assert!(
        binding_a
            .current_run()
            .unwrap()
            .submissions
            .iter()
            .any(|submission| submission.id == prepared.id())
    );
    assert!(
        binding_a.current_run().unwrap().draft.text.is_empty(),
        "prepared draft remains owned by its original ticket"
    );
    binding_a.cancel_prepared(prepared).unwrap();
    assert_eq!(
        binding_a.current_run().unwrap().draft.text,
        "A 端的中文草稿"
    );
    assert_eq!(binding_a.current_run().unwrap().draft.artifacts.len(), 1);
    for runtime in runtimes.values() {
        runtime.stop().await.unwrap();
    }
}

#[tokio::test]
async fn stale_open_and_same_endpoint_noncanonical_owner_cannot_replace_the_mounted_run() {
    let (agent, runtimes, owners, mut view) = setup().await;
    let owner_a = &owners["endpoint-a"];
    let owner_b = &owners["endpoint-b"];
    view.register_side_chat_binding(owner_b.clone()).unwrap();

    let create_a = create_request(&mut view);
    let chat_a = owner_a
        .create(create_a.parent().clone(), None, "late-open-a")
        .await
        .unwrap();
    let DemoCommand::OpenSideChat {
        request: late_a,
        chat: opened_a,
    } = open_command(&mut view, &chat_a)
    else {
        panic!("AX open command for endpoint A")
    };
    assert_eq!(opened_a, chat_a);

    view.select(1).unwrap();
    let create_b = create_request(&mut view);
    let chat_b = owner_b
        .create(create_b.parent().clone(), None, "current-open-b")
        .await
        .unwrap();
    let DemoCommand::OpenSideChat {
        request: current_b,
        chat: opened_b,
    } = open_command(&mut view, &chat_b)
    else {
        panic!("AX open command for endpoint B")
    };
    assert_eq!(opened_b, chat_b);
    let run_b = owner_b.open(&chat_b).await.unwrap();
    let binding_b = run_b.binding().clone();
    assert!(view.install_side_chat_run(current_b, run_b).unwrap());
    assert!(view.has_side_chat_run_binding(&chat_b, &binding_b));
    let ids_b = view.panel_composer_ids().unwrap();
    view.tree.focus(Some(ids_b.editor), true);

    let late_run = owner_a.open(&chat_a).await.unwrap();
    let binding_a = late_run.binding().clone();
    assert!(!view.install_side_chat_run(late_a, late_run).unwrap());
    assert!(view.has_side_chat_run_binding(&chat_a, &binding_a));
    assert_eq!(
        selected(&view),
        Some(SidePanelTabKind::SideChat {
            session_id: chat_b.scope.session_id.clone(),
        })
    );
    assert_eq!(view.panel_composer_ids(), Some(ids_b));
    assert_eq!(view.tree.focused(), Some(ids_b.editor));

    // A separate controller over an identical endpoint/session can produce the
    // same pointer-shaped scope. Only the registered Arc owns install authority.
    let duplicate_runtime = Arc::new(tekes_agent_session::SessionRuntime::spawn(
        "endpoint-a",
        Arc::new(
            Fixture::default()
                .with_endpoint_id("endpoint-a")
                .with_side_chats(),
        ),
    ));
    duplicate_runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let duplicate_owner = Arc::new(SideChatBinding::new(duplicate_runtime.clone()));
    assert!(
        view.register_side_chat_binding(duplicate_owner.clone())
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &agent.endpoint("endpoint-a").unwrap().runtime,
        &runtimes["endpoint-a"]
    ));
    let duplicate = duplicate_owner
        .create(chat_a.parent.clone(), None, "noncanonical-owner")
        .await
        .unwrap();
    assert_eq!(duplicate.scope, chat_a.scope);
    assert_eq!(duplicate.parent, chat_a.parent);
    let duplicate_run = duplicate_owner.open(&duplicate).await.unwrap();
    let duplicate_binding = duplicate_run.binding().clone();
    assert!(!view.install_side_chat_run(create_a, duplicate_run).unwrap());
    assert!(view.has_side_chat_run_binding(&chat_a, &binding_a));
    assert!(!view.has_side_chat_run_binding(&duplicate, &duplicate_binding));
    assert_eq!(view.panel_composer_ids(), Some(ids_b));
    duplicate_runtime.stop().await.unwrap();
    for runtime in runtimes.values() {
        runtime.stop().await.unwrap();
    }
}

#[tokio::test]
async fn unavailable_endpoint_and_canonical_owner_replacement_are_rejected() {
    let (agent, runtimes, owners, mut view) = setup().await;
    let owner_a = owners["endpoint-a"].clone();
    let owner_b = owners["endpoint-b"].clone();
    view.register_side_chat_binding(owner_a.clone()).unwrap();
    assert!(
        view.register_side_chat_binding(Arc::new(SideChatBinding::new(
            runtimes["endpoint-a"].clone(),
        )))
        .is_err()
    );

    view.select(1).unwrap();
    // Endpoint B is mounted but has no owner, so creation stays unavailable.
    let panel = view
        .tree
        .get(view.panel_id())
        .unwrap()
        .component::<SidePanel>()
        .unwrap();
    assert_eq!(
        panel.displayed_input().unwrap().supports_side_chat,
        Some(false)
    );
    let create = panel.create_side_chat_id();
    assert!(press(&mut view, create).is_empty());
    assert!(Arc::ptr_eq(
        &agent.endpoint("endpoint-b").unwrap().runtime,
        &runtimes["endpoint-b"]
    ));
    assert!(Arc::ptr_eq(&owner_b, &owners["endpoint-b"]));
    for runtime in runtimes.values() {
        runtime.stop().await.unwrap();
    }
}
