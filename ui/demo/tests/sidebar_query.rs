use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{Sidebar, SidebarLabels, SidebarSearchIntent, SidebarSearchPhase};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{SidebarSearchBinding, SidebarSearchPublication};
use tekes_agent_ui_demo::fixture::Fixture;
async fn setup(fixture: &Fixture) -> Arc<SessionRuntime> {
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(fixture.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    runtime
}
async fn until(
    source: &SidebarSearchBinding,
    predicate: impl Fn(&SidebarSearchPublication) -> bool,
) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate(&source.snapshot()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn debounce_keeps_exact_query_and_ranked_scoped_hits_without_repeating_same_query() {
    let fixture = Fixture::default().with_session_search();
    fixture.set_search_result("  中文 👩🏽‍💻  ", json!({"items":[{"sessionId":"b","snippet":"first","extra":"retained"},{"sessionId":"a","snippet":"第二"},{"sessionId":"b","snippet":"last"},{"sessionId":"outside","snippet":"not in inventory"}],"hasMore":true}));
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("discarded").unwrap();
    source.set_query("  中文 👩🏽‍💻  ").unwrap();
    assert_eq!(fixture.search_calls(), 0);
    assert_eq!(source.snapshot().query, "  中文 👩🏽‍💻  ");
    until(&source, |state| {
        matches!(state.input.phase, SidebarSearchPhase::Complete { .. })
    })
    .await;
    let result = source.snapshot();
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| hit.address.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "a", "outside"]
    );
    assert!(
        result
            .hits
            .iter()
            .all(|hit| hit.address.endpoint_id == "fixture")
    );
    assert_eq!(result.hits[0].hit.snippet, "last");
    assert_eq!(result.unresolved[0].session_id, "outside");
    assert_eq!(
        result.result.as_ref().unwrap().items[0].extra["extra"],
        "retained"
    );
    assert_eq!(fixture.search_calls(), 1);
    source.set_query("  中文 👩🏽‍💻  ").unwrap();
    assert_eq!(source.snapshot().input.revision, result.input.revision);
    assert_eq!(fixture.search_calls(), 1);
    source.set_query(" \n ").unwrap();
    assert!(source.snapshot().input.key.is_none());
    assert!(source.snapshot().hits.is_empty());
    assert!(source.snapshot().result.is_none());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn clearing_an_inflight_read_cancels_its_reader_without_a_submission_or_late_result() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_session_search()
        .with_search_gate(gate);
    fixture.set_search_result(
        "old",
        json!({"items":[{"sessionId":"a","snippet":"old"}],"hasMore":false}),
    );
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("old").unwrap();
    wait_for(|| fixture.active_searches() == 1).await;
    let key = source.snapshot().input.key.clone().unwrap();
    source.set_query("").unwrap();
    let cleared = source.snapshot();
    wait_for(|| fixture.active_searches() == 0).await;
    assert_eq!(source.snapshot(), cleared);
    assert!(source.retry(&SidebarSearchIntent::Retry { key }).is_err());
    assert_eq!(fixture.search_calls(), 1);
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn failure_retries_once_for_the_original_owner_and_query_aba_cannot_accept_old_callbacks() {
    let fixture = Fixture::default().with_session_search();
    fixture.set_search_failure(true);
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("A").unwrap();
    until(&source, |state| {
        matches!(
            state.input.phase,
            SidebarSearchPhase::Failed {
                retryable: true,
                ..
            }
        )
    })
    .await;
    let first = source.snapshot().input.key.clone().unwrap();
    let mut sidebar = Sidebar::new(52, SidebarLabels::default());
    sidebar.update_search(&source.snapshot().input).unwrap();
    let retry = SidebarSearchIntent::Retry { key: first.clone() };
    fixture.set_search_failure(false);
    source.retry(&retry).unwrap();
    assert!(source.retry(&retry).is_err());
    until(&source, |state| {
        matches!(state.input.phase, SidebarSearchPhase::Complete { .. })
    })
    .await;
    assert_eq!(fixture.search_calls(), 2);
    source.set_query("B").unwrap();
    source.set_query("A").unwrap();
    assert_ne!(source.snapshot().input.key, Some(first));
    assert!(source.retry(&retry).is_err());
    let other = SidebarSearchBinding::new(runtime.clone()).unwrap();
    other.set_query("A").unwrap();
    assert!(other.retry(&retry).is_err());
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn reconnect_retires_blocked_result_and_drop_aborts_only_the_search_reader() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_session_search()
        .with_search_gate(gate.clone());
    fixture.set_search_result(
        "query",
        json!({"items":[{"sessionId":"a","snippet":"owned"}],"hasMore":false}),
    );
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("query").unwrap();
    wait_for(|| fixture.active_searches() == 1).await;
    let old = source.snapshot().input.key.clone().unwrap();
    runtime.reconnect().await.unwrap();
    until(&source, |state| {
        state
            .input
            .key
            .as_ref()
            .is_some_and(|key| key.connection_epoch > old.connection_epoch)
            && state.input.phase == SidebarSearchPhase::Searching
    })
    .await;
    wait_for(|| fixture.search_calls() == 2 && fixture.active_searches() == 1).await;
    gate.add_permits(1);
    until(&source, |state| {
        matches!(state.input.phase, SidebarSearchPhase::Complete { .. })
    })
    .await;
    assert_ne!(source.snapshot().input.key, Some(old));
    assert!(fixture.search_calls() <= 2);
    source.set_query("blocked").unwrap();
    wait_for(|| fixture.active_searches() == 1).await;
    let mut changes = source.subscribe();
    drop(source);
    wait_for(|| fixture.active_searches() == 0).await;
    let _ = changes.changed().await;
    assert!(changes.borrow().input.key.is_none());
    assert_eq!(
        runtime.inspect(|state| state.connection.clone()),
        tekes_agent_session::ConnectionState::Ready
    );
    runtime.stop().await.unwrap();
}
#[tokio::test]
async fn unsupported_and_malformed_responses_are_explicit_failures_without_fabricating_empty_success()
 {
    let fixture = Fixture::default();
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("query").unwrap();
    assert!(matches!(
        source.snapshot().input.phase,
        SidebarSearchPhase::Failed {
            retryable: false,
            ..
        }
    ));
    assert_eq!(fixture.search_calls(), 0);
    runtime.stop().await.unwrap();
    let fixture = Fixture::default().with_session_search();
    fixture.set_search_result("bad", json!({"items":[]}));
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("bad").unwrap();
    until(&source, |state| {
        matches!(state.input.phase, SidebarSearchPhase::Failed { .. })
    })
    .await;
    assert!(source.snapshot().result.is_none());
    assert!(source.snapshot().hits.is_empty());
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn independent_public_consumer_constructs_the_shared_query_and_updates_the_mounted_footer() {
    use tekes_agent_ui_bindings::SidebarBinding;
    use tekes_agent_web::SharedAgent;
    let fixture = Fixture::default().with_session_search();
    fixture.set_search_result(
        "中文",
        json!({"items":[{"sessionId":"a","snippet":"中文"}],"hasMore":true}),
    );
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "owned search fixture".into(),
            Arc::new(fixture),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let inventory = SidebarBinding::new(agent, "fixture").unwrap();
    let search = tekes_agent_ui_consumer_check::new_sidebar_search(&inventory).unwrap();
    search.set_query("中文").unwrap();
    until(&search, |state| state.result.is_some()).await;
    let mut sidebar = Sidebar::new(53, SidebarLabels::default());
    tekes_agent_ui_consumer_check::mount_sidebar_search(&mut sidebar, &search).unwrap();
    tekes_agent_ui_consumer_check::sync_sidebar_search(&mut sidebar, &search).unwrap();
    assert_eq!(sidebar.displayed_search(), Some(&search.snapshot().input));
    assert_eq!(search.snapshot().hits[0].address.endpoint_id, "fixture");
    let host = sidebar.into_host().unwrap();
    assert!(
        host.semantics()
            .iter()
            .any(|node| node.semantics.name == "还有更多匹配结果，请缩小搜索范围。")
    );
    runtime.stop().await.unwrap();
}

#[test]
fn explicit_source_replacement_rebases_clock_and_fences_late_clears_and_old_raw_retries() {
    use tekes_agent_components_ui::{SidebarSearchInput, SidebarSearchKey};
    use tekes_ui_controls::composition::*;
    let key = |owner: &str| SidebarSearchKey {
        owner: owner.into(),
        endpoint_id: "fixture".into(),
        connection_epoch: 1,
        generation: 1,
        query: "same".into(),
    };
    let input = |owner: &str, revision| SidebarSearchInput {
        revision,
        key: Some(key(owner)),
        phase: SidebarSearchPhase::Failed {
            message: "failed".into(),
            has_more: false,
            retryable: true,
        },
    };
    let mut sidebar = Sidebar::new(54, SidebarLabels::default());
    let a = input("A", 100);
    sidebar.replace_search_source("A", &a).unwrap();
    let old = sidebar.search_retry_id().unwrap();
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    host.accessibility_action(old, None, AccessibilityAction::Press);
    let captured = host.drain_actions();
    let sidebar = host
        .get_mut(root)
        .unwrap()
        .component_mut::<Sidebar>()
        .unwrap();
    let b = input("B", 1);
    sidebar.replace_search_source("B", &b).unwrap();
    assert_ne!(sidebar.search_retry_id(), Some(old));
    assert!(sidebar.take_search_intents(&captured).is_empty());
    let late_clear = SidebarSearchInput {
        revision: 101,
        key: None,
        phase: SidebarSearchPhase::Idle,
    };
    assert_eq!(
        sidebar.update_search_from("A", &late_clear),
        Err("sidebar search source replaced")
    );
    assert_eq!(
        sidebar.update_search(&late_clear),
        Err("bound sidebar search requires original source")
    );
    assert_eq!(sidebar.displayed_search(), Some(&b));
    let mut invalid = b.clone();
    invalid.revision = 0;
    assert!(sidebar.replace_search_source("B", &invalid).is_err());
    assert_eq!(sidebar.displayed_search(), Some(&b));
    sidebar.replace_search_source("A", &a).unwrap();
    assert_ne!(sidebar.search_retry_id(), Some(old));
    assert!(sidebar.take_search_intents(&captured).is_empty());
}

#[tokio::test]
async fn connection_notifications_read_the_current_query_and_do_not_restart_an_already_current_attempt()
 {
    let fixture = Fixture::default().with_session_search();
    fixture.set_search_result(
        "B",
        json!({"items":[{"sessionId":"b","snippet":"current B"}],"hasMore":false}),
    );
    let runtime = setup(&fixture).await;
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("A").unwrap();
    source.set_query("B").unwrap();
    let b = source.snapshot();
    source.reconcile_connection().unwrap();
    source.reconcile_connection().unwrap();
    assert_eq!(source.snapshot(), b);
    until(&source, |state| state.result.is_some()).await;
    assert_eq!(fixture.search_calls(), 1);
    runtime.reconnect().await.unwrap();
    source.set_query("B").unwrap();
    let resumed = source.snapshot();
    assert_eq!(
        resumed.input.key.as_ref().unwrap().connection_epoch,
        runtime.inspect(|state| state.connection_epoch)
    );
    source.reconcile_connection().unwrap();
    assert_eq!(source.snapshot(), resumed);
    until(&source, |state| {
        state.result.is_some()
            && state.input.key.as_ref().unwrap().connection_epoch
                > b.input.key.as_ref().unwrap().connection_epoch
    })
    .await;
    assert_eq!(source.snapshot().query, "B");
    assert_eq!(fixture.search_calls(), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn unknown_search_hit_resolves_through_public_inventory_and_the_second_consumer_keeps_drafts()
{
    use tekes_agent_components_ui::{ComposerDestination, RunScope};
    use tekes_agent_ui_bindings::{RunBinding, SidebarBinding};
    use tekes_agent_web::SharedAgent;
    let fixture = Fixture::default().with_session_search().with_lifecycle();
    fixture.set_search_result(
        "body",
        json!({"items":[{"sessionId":"late","snippet":"命中正文"}],"hasMore":false}),
    );
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "owned refresh fixture".into(),
            Arc::new(fixture.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let run = RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    run.edit_text(
        &ComposerDestination::run(run.scope().clone()),
        "原会话中文草稿".into(),
    )
    .unwrap();
    let inventory = SidebarBinding::new(agent, "fixture").unwrap();
    wait_for(|| inventory.snapshot().loaded).await;
    fixture.install_unannounced_session("late", "迟到的真实库存会话");
    assert!(runtime.inspect(|s| !s.sessions.contains_key("late")));
    let source = tekes_agent_ui_consumer_check::new_sidebar_search(&inventory).unwrap();
    source.set_query("body").unwrap();
    until(&source, |state| state.result.is_some()).await;
    assert!(source.snapshot().unresolved.is_empty());
    wait_for(|| {
        inventory
            .snapshot()
            .rows
            .iter()
            .any(|row| row.address.session_id == "late")
    })
    .await;
    let mut sidebar = Sidebar::new(55, SidebarLabels::default());
    sidebar.update(&inventory.snapshot().input(None)).unwrap();
    tekes_agent_ui_consumer_check::mount_sidebar_search(&mut sidebar, &source).unwrap();
    let hit = source.snapshot().hits[0].address.clone();
    assert!(sidebar.row_id(&hit).is_some());
    let frame = run.current_run().unwrap();
    assert_eq!(frame.draft.text, "原会话中文草稿");
    assert!(frame.submissions.is_empty());
    assert_eq!(fixture.inventory_reads(), 2);
    source.set_query("body").unwrap();
    assert_eq!(fixture.search_calls(), 1);
    assert_eq!(fixture.inventory_reads(), 2);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn refresh_failure_is_search_failure_and_explicit_retry_uses_the_same_query_without_submission()
 {
    let fixture = Fixture::default().with_session_search();
    let runtime = setup(&fixture).await;
    fixture.install_unannounced_session("late", "late");
    fixture.set_search_result(
        "original",
        json!({"items":[{"sessionId":"late","snippet":"正文"}],"hasMore":false}),
    );
    fixture.set_inventory_refresh_failure(true);
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("original").unwrap();
    until(&source, |state| {
        matches!(
            state.input.phase,
            SidebarSearchPhase::Failed {
                retryable: true,
                ..
            }
        )
    })
    .await;
    assert!(source.snapshot().hits.is_empty() && source.snapshot().result.is_none());
    assert!(runtime.inspect(|s| !s.sessions.contains_key("late")));
    assert_eq!(
        runtime.inspect(|s| s.connection.clone()),
        tekes_agent_session::ConnectionState::Ready
    );
    let old = source.snapshot().input.key.clone().unwrap();
    fixture.set_inventory_refresh_failure(false);
    source
        .retry(&SidebarSearchIntent::Retry { key: old.clone() })
        .unwrap();
    assert!(
        source
            .retry(&SidebarSearchIntent::Retry { key: old })
            .is_err()
    );
    until(&source, |state| state.result.is_some()).await;
    assert_eq!(source.snapshot().query, "original");
    assert!(source.snapshot().unresolved.is_empty());
    assert_eq!(fixture.search_calls(), 2);
    assert_eq!(fixture.inventory_reads(), 3);
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn replacing_query_during_shared_refresh_never_publishes_the_retired_query() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let fixture = Fixture::default()
        .with_session_search()
        .with_inventory_refresh_gate(gate.clone());
    let runtime = setup(&fixture).await;
    fixture.install_unannounced_session("late", "late");
    fixture.set_search_result(
        "old",
        json!({"items":[{"sessionId":"late","snippet":"old"}],"hasMore":false}),
    );
    fixture.set_search_result(
        "fresh",
        json!({"items":[{"sessionId":"a","snippet":"fresh"}],"hasMore":false}),
    );
    let source = SidebarSearchBinding::new(runtime.clone()).unwrap();
    source.set_query("old").unwrap();
    wait_for(|| fixture.inventory_reads() == 2).await;
    source.set_query("fresh").unwrap();
    until(&source, |state| state.result.is_some()).await;
    let current = source.snapshot();
    assert_eq!(current.query, "fresh");
    gate.add_permits(1);
    wait_for(|| runtime.inspect(|s| s.sessions.contains_key("late"))).await;
    assert_eq!(source.snapshot().query, "fresh");
    assert_eq!(source.snapshot().input.key, current.input.key);
    assert_eq!(source.snapshot().hits[0].address.session_id, "a");
    assert_eq!(fixture.search_calls(), 2);
    assert_eq!(fixture.inventory_reads(), 2);
    runtime.stop().await.unwrap();
}
