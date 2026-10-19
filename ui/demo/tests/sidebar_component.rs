use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tekes_agent_components_ui::{Sidebar, SidebarInput, SidebarIntent, SidebarLabels, SidebarRow};
use tekes_agent_contract::*;
use tekes_agent_presentation::{ThreadActionContext, thread_action_command, thread_actions};
use tekes_ui_controls::composition::*;
fn make_row(endpoint: &str, id: &str, ready: bool) -> SidebarRow {
    let mut capabilities =
        EndpointCapabilities::methods(&["session.rename", "session.fork", "session.archive"]);
    capabilities.supports_archive_session = true;
    let actions = thread_actions(&ThreadActionContext {
        endpoint,
        session_id: id,
        kind: "native",
        url_scheme: "tekes",
        capabilities: &capabilities,
        ready,
        running: false,
        archived: false,
        pin: None,
        host_methods: &BTreeSet::new(),
    });
    SidebarRow {
        address: SessionAddress {
            endpoint_id: endpoint.into(),
            session_id: id.into(),
        },
        title: format!("{endpoint} / {id}"),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: !ready,
        },
        pin: None,
        actions,
    }
}
fn activate(host: &mut ContentHost, root: ComponentId, target: ComponentId) -> Vec<SidebarIntent> {
    host.accessibility_action(target, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    host.get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .take_intents(&actions)
}
fn update(
    host: &mut ContentHost,
    root: ComponentId,
    input: &SidebarInput,
) -> std::result::Result<(), &'static str> {
    host.get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap()
        .update(input)
}
#[test]
fn same_session_ids_on_two_endpoints_keep_scoped_selection_and_authoritative_actions() {
    let native = make_row("native", "same-id", true);
    let pi = make_row("pi", "same-id", true);
    let mut input = SidebarInput {
        revision: 1,
        rows: vec![native.clone(), pi.clone()],
        selected: Some(native.address.clone()),
    };
    let mut sidebar = Sidebar::new(30, SidebarLabels::default());
    let root = sidebar.id();
    sidebar.update(&input).unwrap();
    let a = sidebar.row_id(&native.address).unwrap();
    let b = sidebar.row_id(&pi.address).unwrap();
    assert_ne!(a, b);
    let copy = sidebar.action_id(&native.address, "copySessionId").unwrap();
    let disabled = sidebar.action_id(&native.address, "pin").unwrap();
    let mut host = sidebar.into_host().unwrap();
    assert!(
        host.semantics()
            .iter()
            .find(|node| node.id == a)
            .unwrap()
            .semantics
            .selected
    );
    assert!(
        !host
            .semantics()
            .iter()
            .find(|node| node.id == b)
            .unwrap()
            .semantics
            .selected
    );

    assert_eq!(
        activate(&mut host, root, b),
        vec![SidebarIntent::Select {
            address: pi.address.clone()
        }]
    );
    assert!(activate(&mut host, root, disabled).is_empty());
    let intents = activate(&mut host, root, copy);
    let [
        SidebarIntent::Action {
            address,
            definition,
        },
    ] = intents.as_slice()
    else {
        panic!("shared action");
    };
    assert_eq!(address, &native.address);
    assert_eq!(
        definition.copy_text.as_deref(),
        Some("tekes://native/same-id")
    );
    input.revision += 1;
    input.rows.reverse();
    update(&mut host, root, &input).unwrap();
    let sidebar = host.get(root).unwrap().component::<Sidebar>().unwrap();
    assert_eq!(sidebar.row_id(&native.address), Some(a));
    assert_eq!(sidebar.row_id(&pi.address), Some(b));
    assert_eq!(
        address, &native.address,
        "a captured callback was retargeted after reorder"
    );
}
#[test]
fn stale_invalid_and_removed_inventory_never_retarget_actions_or_change_a_valid_tree() {
    let row = make_row("native", "a", true);
    let input = SidebarInput {
        revision: 8,
        rows: vec![row.clone()],
        selected: Some(row.address.clone()),
    };
    let mut sidebar = Sidebar::new(31, SidebarLabels::default());
    let root = sidebar.id();
    sidebar.update(&input).unwrap();
    let id = sidebar.row_id(&row.address).unwrap();
    let mut host = sidebar.into_host().unwrap();
    let mut invalid = input.clone();
    invalid.revision = 7;
    assert!(update(&mut host, root, &invalid).is_err());
    invalid.revision = 9;
    invalid.rows[0].actions.endpoint = "different".into();
    assert!(update(&mut host, root, &invalid).is_err());
    assert_eq!(
        activate(&mut host, root, id),
        vec![SidebarIntent::Select {
            address: row.address.clone()
        }]
    );
    let mut invalid = input.clone();
    invalid.revision = 9;
    invalid.rows.push(row.clone());
    assert!(update(&mut host, root, &invalid).is_err());
    let empty = SidebarInput {
        revision: 9,
        rows: Vec::new(),
        selected: Some(row.address.clone()),
    };
    update(&mut host, root, &empty).unwrap();
    assert!(activate(&mut host, root, id).is_empty());
    let restored = SidebarInput {
        revision: 10,
        ..input
    };
    update(&mut host, root, &restored).unwrap();
    let new_id = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&row.address)
        .unwrap();
    assert_ne!(new_id, id, "retired identity was reused for pending input");
}
#[test]
fn shared_dispatch_rechecks_capabilities_instead_of_trusting_the_displayed_definition() {
    let row = make_row("native", "a", true);
    let mut sidebar = Sidebar::new(32, SidebarLabels::default());
    sidebar
        .update(&SidebarInput {
            revision: 1,
            rows: vec![row.clone()],
            selected: Some(row.address.clone()),
        })
        .unwrap();
    let fork = sidebar.action_id(&row.address, "fork").unwrap();
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let intents = activate(&mut host, root, fork);
    let [
        SidebarIntent::Action {
            address,
            definition,
        },
    ] = intents.as_slice()
    else {
        panic!("fork callback");
    };
    let fresh = row_factory_unavailable();
    assert!(
        thread_action_command(
            &fresh.actions,
            &ThreadActionRequest {
                session_id: address.session_id.clone(),
                action_id: definition.id.clone(),
                title: None
            }
        )
        .is_err()
    );
    fn row_factory_unavailable() -> SidebarRow {
        make_row("native", "a", false)
    }
}

#[derive(Clone)]
struct InventoryFixture;
struct InventoryEndpoint(Arc<dyn SessionEndpoint>);
#[async_trait::async_trait]
impl EndpointConnector for InventoryFixture {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(InventoryEndpoint(
            tekes_agent_ui_demo::fixture::Fixture::default()
                .connect()
                .await?,
        )))
    }
}
#[async_trait::async_trait]
impl SessionEndpoint for InventoryEndpoint {
    fn identity(&self) -> &str {
        self.0.identity()
    }
    fn host(&self) -> &HostDescription {
        self.0.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        self.0.capabilities()
    }
    async fn call(
        &self,
        method: &str,
        payload: serde_json::Value,
        rpc_id: &str,
    ) -> Result<serde_json::Value> {
        self.0.call(method, payload, rpc_id).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.0.history_page(request).await
    }
    async fn respond(&self, id: &str, epoch: i64, outcome: serde_json::Value) -> Result<()> {
        self.0.respond(id, epoch, outcome).await
    }
    async fn close(&self) {
        self.0.close().await;
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        use futures_util::StreamExt;
        let inventory = matches!(target, StreamTarget::SessionInventory);
        Ok(Box::pin(self.0.follow(target).await?.map(move|frame|frame.map(|mut frame| {
            if inventory && frame["type"]=="baseline" {frame["snapshot"]["items"]=serde_json::json!([{"sessionId":"a","title":"Owned fixture","status":"idle","running":false,"updatedAt":1,"blank":false}]);}
            frame
        }))))
    }
}
async fn inventory(agent: &tekes_agent_web::SharedAgent) -> serde_json::Value {
    let request = serde_json::json!({"endpoint":"fixture","target":"state"});
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tokio::select! {value=rx.recv()=>value.unwrap(),result=agent.follow(&request,tx)=>panic!("observer ended: {result:?}"),_=tokio::time::sleep(Duration::from_secs(3))=>panic!("inventory timeout")}
}
async fn service_input(agent: &tekes_agent_web::SharedAgent, revision: u64) -> SidebarInput {
    let value = inventory(agent).await;
    let session = &value["sessions"][0];
    let endpoint = agent.endpoint("fixture").unwrap();
    let row = SidebarRow {
        address: SessionAddress {
            endpoint_id: "fixture".into(),
            session_id: session["sessionId"].as_str().unwrap().into(),
        },
        title: session["summary"]["title"].as_str().unwrap().into(),
        activity: serde_json::from_value(session["activity"].clone()).unwrap(),
        pin: session
            .get("pin")
            .map(|pin| serde_json::from_value(pin.clone()).unwrap()),
        actions: agent.thread_actions(&endpoint, "a").unwrap(),
    };
    SidebarInput {
        revision,
        selected: Some(row.address.clone()),
        rows: vec![row],
    }
}
struct OwnedDirectory(std::path::PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[tokio::test]
async fn actual_shared_service_pin_receipt_and_reconnect_publish_authority_without_ui_storage() {
    let directory = OwnedDirectory(std::env::temp_dir().join(format!(
            "agentkit-sidebar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    std::fs::create_dir(&directory.0).unwrap();
    let mut agent = tekes_agent_web::SharedAgent::default();
    agent
        .open_thread_preferences(&directory.0.join("pins.json"), "owned-ui-test-account")
        .unwrap();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Owned fixture".into(),
            Arc::new(InventoryFixture),
        )
        .unwrap();
    let endpoint = agent.endpoint("fixture").unwrap();
    endpoint
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let input = service_input(&agent, 1).await;
    assert!(input.rows[0].pin.is_none());
    let mut sidebar = Sidebar::new(33, SidebarLabels::default());
    let root = sidebar.id();
    sidebar.update(&input).unwrap();
    let pin = sidebar.action_id(&input.rows[0].address, "pin").unwrap();
    let mut host = sidebar.into_host().unwrap();
    let intents = activate(&mut host, root, pin);
    let [
        SidebarIntent::Action {
            address,
            definition,
        },
    ] = intents.as_slice()
    else {
        panic!("pin callback");
    };
    let request = serde_json::json!({"op":"call","endpoint":address.endpoint_id,"method":"thread.performAction","payload":{"sessionId":address.session_id,"actionId":definition.id},"rpcId":"owned-sidebar-pin"});
    agent.request(&request).await.unwrap();
    agent.request(&request).await.unwrap();
    let next = service_input(&agent, 2).await;
    assert_eq!(next.rows[0].pin.as_ref().map(|pin| pin.pinned), Some(true));
    update(&mut host, root, &next).unwrap();
    assert!(
        activate(&mut host, root, pin).is_empty(),
        "old pin binding survived new authoritative definition"
    );
    endpoint.runtime.reconnect().await.unwrap();
    endpoint
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    assert_eq!(
        service_input(&agent, 3).await.rows[0]
            .pin
            .as_ref()
            .map(|pin| pin.pinned),
        Some(true)
    );
    endpoint.runtime.stop().await.unwrap();
}
