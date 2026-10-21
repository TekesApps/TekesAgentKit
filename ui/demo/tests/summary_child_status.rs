//! Child inventory -> existing RunBinding -> original Summary controls, no child RPC.
#![cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "support/menu_host.rs"]
mod paint;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::*;
use tekes_agent_ui_bindings::RunBinding;
use tekes_agent_ui_demo::{
    component_demo::ComponentDemo,
    fixture::{Fixture, setup_fixture},
};
use tekes_ui_controls::composition::*;
const CHILD: &str = "b437cf34-5661-45e2-a674-113b27c92f89";
async fn status(binding: &RunBinding, expected: &str) {
    let mut changed = binding.subscribe_run().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if binding
                .current_run()
                .unwrap()
                .summary
                .as_ref()
                .is_some_and(|s| {
                    s.subagents.iter().any(|child| {
                        child.session_id.as_deref() == Some(CHILD) && child.status == expected
                    })
                })
            {
                break;
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|error| {
        panic!(
            "status {expected}: {error}; run={:?}; store={:?}",
            binding.current_run(),
            binding.subscribe_runtime().inspect(|s| (
                s.connection.clone(),
                s.last_error.clone(),
                s.sessions.get(CHILD).and_then(|s| s.summary.clone())
            ))
        )
    });
}
#[tokio::test]
async fn inventory_child_status_updates_the_retained_summary_without_touching_composition_or_hydrating_children()
 {
    let fixture = Fixture::default().with_summary_subagent();
    let (runtime, _) = setup_fixture(fixture.clone()).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    status(&binding, "notStarted").await;
    let mut view = ComponentDemo::new([binding.clone()]).unwrap();
    paint::paint(&mut view.tree);
    let root = view.summary_id();
    let call_id = binding
        .current_run()
        .unwrap()
        .summary
        .as_ref()
        .unwrap()
        .subagents
        .iter()
        .find(|child| child.session_id.as_deref() == Some(CHILD))
        .unwrap()
        .id
        .clone();
    let row = view
        .tree
        .get(root)
        .unwrap()
        .component::<SummaryView>()
        .unwrap()
        .action_id("subagents", &call_id)
        .unwrap();
    let control = view
        .tree
        .get(row)
        .unwrap()
        .component::<SummaryActionRow>()
        .unwrap()
        .button() as *const _ as usize;
    assert!(
        !view
            .tree
            .get(row)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button()
            .label
            .contains("completed")
    );
    view.tree.focus(Some(view.ids().editor), true);
    view.event(&ContentEvent::Preedit {
        text: "子状态更新期间中文组字".into(),
        selection: 1..3,
        replacement: None,
        clauses: vec![],
    })
    .unwrap();
    let baseline = view.editor().input.external_baseline();
    for (raw, indicator) in [
        ("running", Some(SummarySubagentIndicator::Running)),
        ("failed", Some(SummarySubagentIndicator::Failed)),
        ("completed", None),
    ] {
        fixture
            .publish_inventory_summary(
                json!({"sessionId":CHILD,"title":"Summary child","status":raw,"running":raw=="running","blank":true,"updatedAt":2}),
            )
            .unwrap();
        status(&binding, raw).await;
        view.sync().unwrap();
        paint::paint(&mut view.tree);
        let snapshot = binding.current_run().unwrap();
        let child = &snapshot.summary.as_ref().unwrap().subagents[0];
        assert_eq!(child.indicator(), indicator);
        assert_eq!(child.session_id.as_deref(), Some(CHILD));
        let button = view
            .tree
            .get(row)
            .unwrap()
            .component::<SummaryActionRow>()
            .unwrap()
            .button();
        assert_eq!(button as *const _ as usize, control);
        assert!(button.label.contains(raw));
        assert_eq!(view.tree.focused(), Some(view.ids().editor));
        assert_eq!(view.editor().input.external_baseline(), baseline);
        assert!(snapshot.submissions.is_empty());
    }
    assert!(
        !runtime.snapshot().sessions[CHILD].journal_ready,
        "Summary must not hydrate child history"
    );
}
