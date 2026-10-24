use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_ui_bindings::{RunBinding, SidebarBinding};
use tekes_agent_ui_demo::{
    component_demo::{ComponentDemo, DemoCommand},
    fixture::Fixture,
};
use tekes_agent_web::SharedAgent;
use tekes_ui_controls::composition::*;
fn address(id: &str) -> SessionAddress {
    RunScope::main("fixture", id).address()
}
struct Pins(std::sync::atomic::AtomicBool);
#[async_trait::async_trait]
impl tekes_agent_contract::ThreadActionHost for Pins {
    fn methods(&self) -> std::collections::BTreeSet<String> {
        Default::default()
    }
    fn pin_state(&self, _endpoint: &str, id: &str) -> Option<tekes_agent_contract::ThreadPinState> {
        Some(tekes_agent_contract::ThreadPinState {
            pinned: id == "a" && self.0.load(std::sync::atomic::Ordering::SeqCst),
        })
    }
    async fn perform(
        &self,
        _endpoint: &str,
        _request: &tekes_agent_contract::ThreadActionRequest,
        _rpc: &str,
    ) -> tekes_agent_contract::Result<tekes_agent_contract::ThreadActionOutcome> {
        Err(tekes_agent_contract::unsupported("read-only fixture pins"))
    }
}
async fn setup() -> (Arc<SharedAgent>, Arc<SidebarBinding>, Fixture) {
    setup_with_pins(None).await
}
async fn setup_with_pins(
    pins: Option<Arc<Pins>>,
) -> (Arc<SharedAgent>, Arc<SidebarBinding>, Fixture) {
    let fixture = Fixture::default();
    let mut agent = SharedAgent::default();
    if let Some(pins) = pins {
        agent.install_thread_action_host(pins);
    }
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(fixture.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    agent
        .endpoint("fixture")
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let source = Arc::new(SidebarBinding::new(agent.clone(), "fixture").unwrap());
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !source.snapshot().loaded {
            assert!(
                source.snapshot().error.is_none(),
                "{:?}",
                source.snapshot().error
            );
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    (agent, source, fixture)
}
fn missing(source: &SidebarBinding, id: &str) -> SidebarInput {
    let mut input = source.snapshot().input(Some(address(id)));
    input.rows.retain(|r| r.address != address(id));
    input
}
#[tokio::test]
async fn selected_memory_read_is_independent_of_view_membership_and_does_not_open_history_or_refresh()
 {
    let (agent, source, fixture) = setup().await;
    let mut input = missing(&source, "a");
    let inventory_reads = fixture.inventory_reads();
    let pages = fixture.history_page_calls();
    let journal = source.runtime().inspect(|s| s.sessions["a"].journal_ready);
    let read = source.read_selected(&address("a"), 1).unwrap();
    assert!(tekes_agent_ui_consumer_check::supplement_selected_sidebar(
        &source, &read, &mut input, 1
    ));
    assert_eq!(
        input
            .rows
            .iter()
            .filter(|r| r.address == address("a"))
            .count(),
        1
    );
    let row = input
        .rows
        .iter()
        .find(|r| r.address == address("a"))
        .unwrap();
    assert_eq!(row.title, "会话 a");
    assert_eq!(row.folder_path.as_deref(), Some("/fixture"));
    assert_eq!(
        row.actions,
        agent
            .thread_actions(&agent.endpoint("fixture").unwrap(), "a")
            .unwrap()
    );
    assert!(!read.project_into(&source, &mut input, 1));
    assert_eq!(fixture.inventory_reads(), inventory_reads);
    assert_eq!(fixture.history_page_calls(), pages);
    assert_eq!(
        source.runtime().inspect(|s| s.sessions["a"].journal_ready),
        journal
    );
    let raw = agent
        .read_thread_inventory(&agent.endpoint("fixture").unwrap(), "a")
        .unwrap()
        .item
        .unwrap();
    assert!(raw.get("entries").is_none());
}
#[tokio::test]
async fn selected_read_rejects_selection_aba_foreign_owner_and_reconnect() {
    let (agent, source, _fixture) = setup().await;
    let read = source.read_selected(&address("a"), 1).unwrap();
    assert!(read.is_current(&source));
    let same_endpoint_other_binding = SidebarBinding::new(agent, "fixture").unwrap();
    assert!(
        !read.is_current(&same_endpoint_other_binding),
        "a selected read belongs to its originating SidebarBinding observer"
    );
    let original = missing(&source, "a");
    let mut input = original.clone();
    assert!(!read.project_into(&source, &mut input, 3));
    assert_eq!(input, original);
    input.selected = Some(address("b"));
    assert!(!read.project_into(&source, &mut input, 1));
    let (_, other, _) = setup().await;
    let mut input = original.clone();
    assert!(!read.project_into(&other, &mut input, 1));
    assert_eq!(input, original);
    assert!(
        source
            .read_selected(&RunScope::main("other", "a").address(), 1)
            .is_err()
    );
    assert!(source.read_selected(&address("a"), 0).is_err());
    source.runtime().reconnect().await.unwrap();
    source
        .runtime()
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    assert!(!read.project_into(&source, &mut input, 1));
    assert_eq!(input, original);
}
#[tokio::test]
async fn canonical_update_clear_archive_and_absence_do_not_resurrect_old_selected_metadata() {
    let (_agent, source, fixture) = setup().await;
    let old = source.read_selected(&address("a"), 1).unwrap();
    let mut input = missing(&source, "a");
    let sequence = source.runtime().inspect(|s| {
        s.sessions["a"]
            .projections
            .values()
            .map(|p| p.sequence)
            .max()
            .unwrap_or(0)
            + 1
    });
    fixture.publish_inventory_summary(json!({"sessionId":"a","cwd":"/current","title":"old summary", "status":"active","running":false,"blank":false,"updatedAt":42,"projections":{"asOfSeq":sequence,"values":{"title":null}}})).unwrap();
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while source
            .snapshot()
            .rows
            .iter()
            .all(|r| r.address != address("a") || r.title != "a")
        {
            assert!(
                source.snapshot().error.is_none(),
                "{:?}",
                source.snapshot().error
            );
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(!old.project_into(&source, &mut input, 1));
    let fresh = source.read_selected(&address("a"), 1).unwrap();
    assert!(fresh.project_into(&source, &mut input, 1));
    assert_eq!(input.rows.last().unwrap().title, "a");
    assert_eq!(
        input.rows.last().unwrap().folder_path.as_deref(),
        Some("/current")
    );
    fixture
        .publish_inventory_summary(
            json!({"sessionId":"a","cwd":"/current","status":"archived","running":false,"blank":false,"updatedAt":43}),
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while source
            .runtime()
            .inspect(|s| s.sessions["a"].summary.as_ref().unwrap()["status"] != "archived")
        {
            assert!(
                source.snapshot().error.is_none(),
                "{:?}",
                source.snapshot().error
            );
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let mut input = missing(&source, "a");
    assert!(!fresh.project_into(&source, &mut input, 1));
    assert!(
        !source
            .read_selected(&address("a"), 1)
            .unwrap()
            .project_into(&source, &mut input, 1)
    );
    input.selected = Some(address("unknown"));
    assert!(
        !source
            .read_selected(&address("unknown"), 2)
            .unwrap()
            .project_into(&source, &mut input, 2)
    );
}
#[tokio::test]
async fn selected_lookup_respects_filter_restoration_and_does_not_change_selection() {
    let (_agent, source, _fixture) = setup().await;
    let read = source.read_selected(&address("a"), 1).unwrap();
    let mut input = missing(&source, "a");
    input.folder_filter = Some(SidebarFolderKey {
        endpoint_id: "fixture".into(),
        identity: SidebarFolderIdentity::Path("/elsewhere".into()),
    });
    let original = input.clone();
    assert!(!read.project_into(&source, &mut input, 1));
    assert_eq!(input, original);
    input.folder_filter = None;
    input.search_help = Some(SidebarSearchHelp {
        results_ready: true,
        snippets: Default::default(),
        source: SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "query".into(),
                endpoint_id: "fixture".into(),
                connection_epoch: source.runtime().inspect(|s| s.connection_epoch),
                generation: 1,
                query: "unrelated".into(),
            }),
            phase: SidebarSearchPhase::Complete { has_more: false },
        },
    });
    let original = input.clone();
    assert!(!read.project_into(&source, &mut input, 1));
    assert_eq!(input, original);
    input.search_help = None;
    input.loaded = false;
    let original = input.clone();
    assert!(!read.project_into(&source, &mut input, 1));
    assert_eq!(input, original);
}
#[tokio::test]
async fn real_demo_supplements_current_selection_and_navigation_missing_only_from_published_inventory()
 {
    let (_agent, source, fixture) = setup().await;
    let runtime = source.runtime().clone();
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
    a.edit_text(
        &ComposerDestination::run(a.scope().clone()),
        "甲草稿".into(),
    )
    .unwrap();
    b.edit_text(
        &ComposerDestination::run(b.scope().clone()),
        "乙草稿".into(),
    )
    .unwrap();
    let mut view = ComponentDemo::new([a.clone()]).unwrap();
    view.set_sidebar_reader(source.clone()).unwrap();
    let mut stale = (*source.snapshot()).clone();
    stale.rows.retain(|r| r.address == address("b"));
    view.set_sidebar_source(Arc::new(stale.clone())).unwrap();
    view.sync().unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(sidebar.row_id(&address("a")).is_some());
    let row_b = sidebar.row_id(&address("b")).unwrap();
    let commands = view
        .accessibility_action(row_b, None, AccessibilityAction::Press)
        .unwrap();
    let [DemoCommand::OpenSession(request)]: [DemoCommand; 1] = commands.try_into().ok().unwrap()
    else {
        panic!("one navigation")
    };
    let reads = fixture.inventory_reads();
    stale.revision += 1;
    stale.rows.clear();
    view.set_sidebar_source(Arc::new(stale)).unwrap();
    view.sync().unwrap();
    view.set_sidebar_folder_filter(Some(SidebarFolderKey {
        endpoint_id: "fixture".into(),
        identity: SidebarFolderIdentity::Path("/outside".into()),
    }))
    .unwrap();
    assert!(view.complete_navigation(request, b.clone()).unwrap());
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .row_id(&address("b"))
            .is_none(),
        "navigation preserves reader filter"
    );
    view.set_sidebar_folder_filter(None).unwrap();
    assert_eq!(view.editor().input.text, "乙草稿");
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .row_id(&address("b"))
            .is_some()
    );
    view.select(0).unwrap();
    assert_eq!(view.editor().input.text, "甲草稿");
    assert_eq!(a.current_run().unwrap().draft.text, "甲草稿");
    assert_eq!(b.current_run().unwrap().draft.text, "乙草稿");
    assert!(a.current_run().unwrap().submissions.is_empty());
    assert!(b.current_run().unwrap().submissions.is_empty());
    assert_eq!(fixture.inventory_reads(), reads);
}

#[tokio::test]
async fn independent_selected_query_cannot_manufacture_a_global_pin_and_changed_pin_retires_the_read()
 {
    let pins = Arc::new(Pins(std::sync::atomic::AtomicBool::new(false)));
    let (_agent, source, _fixture) = setup_with_pins(Some(pins.clone())).await;
    let old = source.read_selected(&address("a"), 1).unwrap();
    let mut display = missing(&source, "a");
    let before = display.clone();
    pins.0.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(!old.project_into(&source, &mut display, 1));
    assert_eq!(display, before);
    assert!(
        !source
            .read_selected(&address("a"), 1)
            .unwrap()
            .project_into(&source, &mut display, 1)
    );
    assert_eq!(display, before);
}

#[tokio::test]
async fn archived_canonical_target_is_rejected_even_when_the_visible_inventory_still_has_its_old_row()
 {
    let (_agent, source, fixture) = setup().await;
    let runtime = source.runtime().clone();
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
    let mut view = ComponentDemo::new([a]).unwrap();
    view.set_sidebar_reader(source.clone()).unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let row = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("b"))
        .unwrap();
    let commands = view
        .accessibility_action(row, None, AccessibilityAction::Press)
        .unwrap();
    let [DemoCommand::OpenSession(request)]: [DemoCommand; 1] = commands.try_into().ok().unwrap()
    else {
        panic!("navigation")
    };
    fixture.publish_inventory_summary(json!({"sessionId":"b","cwd":"/fixture","status":"archived","running":false,"blank":false,"updatedAt":100})).unwrap();
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while runtime.inspect(|s| s.sessions["b"].summary.as_ref().unwrap()["status"] != "archived")
        {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(!view.complete_navigation(request, b).unwrap());
    assert_eq!(view.binding().scope().session_id, "a");
}
