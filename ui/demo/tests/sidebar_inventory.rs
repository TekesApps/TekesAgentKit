use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{ComposerDestination, RunScope, Sidebar, SidebarLabels};
use tekes_agent_ui_bindings::{RunBinding, SidebarBinding, SidebarPublication};
use tekes_agent_ui_demo::{component_demo::ComponentDemo, fixture::Fixture};
use tekes_agent_web::SharedAgent;

async fn until(source: &SidebarBinding, predicate: impl Fn(&SidebarPublication) -> bool) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate(&source.snapshot()) {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
fn summary(id: &str, title: Value, sequence: i64, time: f64, running: bool) -> Value {
    json!({"sessionId":id,"cwd":"/original-conversation","workingDirectory":"/current-execution","title":"stale top-level title","updatedAt":time,"running":running,"blank":false,
        "projections":{"asOfSeq":sequence,"values":{"title":title}}})
}
#[tokio::test]
async fn reconciled_titles_and_recent_activity_reach_host_and_independent_consumer_without_draft_changes()
 {
    let fixture = Fixture::default();
    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "pi".into(),
            "Fixture".into(),
            Arc::new(fixture.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let runtime = agent.endpoint("fixture").unwrap().runtime.clone();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let destination = ComposerDestination::run(binding.scope().clone());
    binding
        .edit_text(&destination, "中文草稿保持".into())
        .unwrap();
    let source = SidebarBinding::new(agent, "fixture").unwrap();
    until(&source, |s| s.loaded).await;
    let mut view = ComponentDemo::new([binding]).unwrap();
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    let mut second = Sidebar::new(992, SidebarLabels::default());
    for (id, title, seq, time, running, first) in [
        ("a", "甲", 100, 1000.5, false, "a"),
        ("b", "乙", 100, 2000.0, false, "b"),
        ("a", "开始执行", 101, 3000.0, true, "a"),
        ("b", "另一个会话", 101, 3500.0, false, "b"),
        // A running stream updates its clock without constantly moving to top.
        ("a", "流式更新", 102, 9000.0, true, "b"),
        ("a", "执行结束", 103, 9500.0, false, "a"),
    ] {
        fixture
            .publish_inventory_summary(summary(id, json!(title), seq, time, running))
            .unwrap();
        until(&source, |s| {
            s.rows
                .iter()
                .any(|r| r.address.session_id == id && r.title == title)
        })
        .await;
        assert_eq!(source.snapshot().rows[0].address.session_id, first);
        assert_eq!(
            source.snapshot().activity_at[&RunScope::main("fixture", id).address()],
            if id == "a" && seq == 102 {
                3000.0
            } else {
                time
            }
        );
        assert!(
            !source
                .snapshot()
                .folder_catalog()
                .unwrap()
                .folders
                .is_empty()
        );

        assert_eq!(
            source
                .snapshot()
                .rows
                .iter()
                .find(|r| r.address.session_id == id)
                .unwrap()
                .folder_path
                .as_deref(),
            Some("/original-conversation")
        );
        view.set_sidebar_source(source.snapshot()).unwrap();
        view.sync().unwrap();
        tekes_agent_ui_consumer_check::sync_observed_sidebar(&mut second, &source).unwrap();
        assert_eq!(
            second.displayed_input().unwrap().rows,
            source.snapshot().rows
        );
        assert_eq!(view.editor().input.text, "中文草稿保持");
        assert_eq!(view.selected(), 0);
    }
    // A late inventory block must not replace the already reconciled title.
    let revision = source.snapshot().revision;
    fixture
        .publish_inventory_summary(summary("a", json!("旧版本"), 50, 9600.0, false))
        .unwrap();
    until(&source, |s| s.revision > revision).await;
    assert_eq!(
        source
            .snapshot()
            .rows
            .iter()
            .find(|r| r.address.session_id == "a")
            .unwrap()
            .title,
        "执行结束"
    );
    fixture
        .publish_inventory_summary(summary("a", Value::Null, 104, 9700.0, false))
        .unwrap();
    until(&source, |s| {
        s.rows
            .iter()
            .any(|r| r.address.session_id == "a" && r.title == "New conversation")
    })
    .await;
    view.set_sidebar_source(source.snapshot()).unwrap();
    view.sync().unwrap();
    assert_eq!(view.editor().input.text, "中文草稿保持");
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
    drop(view);
    drop(source);
    runtime.stop().await.unwrap();
}
