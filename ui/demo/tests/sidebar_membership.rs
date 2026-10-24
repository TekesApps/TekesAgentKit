use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tekes_agent_components_ui::{SessionAddress, SidebarInput};
use tekes_agent_ui_bindings::{SidebarBinding, SidebarMultiEndpointBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;

fn summary(id: &str, fields: Value) -> Value {
    let mut summary = json!({
        "sessionId": id,
        "cwd": "/fixture",
        "updatedAt": 5_000.0,
        "running": false,
        "blank": false
    });
    for (key, value) in fields.as_object().unwrap() {
        summary[key.as_str()] = value.clone();
    }
    summary
}

async fn source(agent: &Arc<SharedAgent>, endpoint: &str) -> Arc<SidebarBinding> {
    agent
        .endpoint(endpoint)
        .unwrap()
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let source = Arc::new(SidebarBinding::new(agent.clone(), endpoint).unwrap());
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !source.snapshot().loaded {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    source
}

async fn wait_revision(source: &SidebarBinding, revision: u64) {
    let mut changes = source.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while source.snapshot().revision < revision {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

fn address(endpoint: &str, id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: id.into(),
    }
}

#[tokio::test]
async fn explicit_membership_is_shared_by_inventory_and_selected_supplementation() {
    let mut agent = SharedAgent::default();
    let alpha_fixture = Fixture::default().with_endpoint_id("alpha");
    let beta_fixture = Fixture::default().with_endpoint_id("beta");
    for (endpoint, fixture) in [("alpha", &alpha_fixture), ("beta", &beta_fixture)] {
        agent
            .register(
                endpoint.into(),
                "fixture".into(),
                endpoint.into(),
                Arc::new(fixture.clone()),
            )
            .unwrap();
    }
    let agent = Arc::new(agent);
    let alpha = source(&agent, "alpha").await;
    let beta = source(&agent, "beta").await;

    let old_alpha_selected = alpha.read_selected(&address("alpha", "a"), 1).unwrap();
    assert!(old_alpha_selected.has_active_row(&alpha));
    let alpha_revision = alpha.snapshot().revision + 1;
    alpha_fixture
        .publish_inventory_summary(summary("a", json!({"origin":"subagent"})))
        .unwrap();
    alpha_fixture
        .publish_inventory_summary(summary("b", json!({"ephemeral":true})))
        .unwrap();
    wait_revision(&alpha, alpha_revision).await;

    let beta_revision = beta.snapshot().revision + 1;
    beta_fixture
        .publish_inventory_summary(summary("a", json!({"origin":"main","ephemeral":false})))
        .unwrap();
    beta_fixture
        .publish_inventory_summary(summary("b", json!({"parentSessionId":"a"})))
        .unwrap();
    wait_revision(&beta, beta_revision).await;

    let mut changes = alpha.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !alpha.snapshot().rows.is_empty() {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(alpha.snapshot().rows.is_empty());
    assert!(!old_alpha_selected.is_current(&alpha));
    assert!(!old_alpha_selected.has_active_row(&alpha));
    let beta_rows = &beta.snapshot().rows;
    assert_eq!(beta_rows.len(), 2);
    assert!(
        beta_rows
            .iter()
            .any(|row| row.address == address("beta", "a"))
    );
    assert!(
        beta_rows
            .iter()
            .any(|row| row.address == address("beta", "b"))
    );
    assert!(
        !beta_rows
            .iter()
            .any(|row| row.address == address("alpha", "a"))
    );

    let aggregate = SidebarMultiEndpointBinding::new(agent.clone(), ["alpha", "beta"]).unwrap();
    let mut changes = aggregate.subscribe();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !aggregate.snapshot().loaded {
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(aggregate.snapshot().rows.len(), 2);
    assert!(
        aggregate
            .snapshot()
            .rows
            .iter()
            .all(|row| row.address.endpoint_id == "beta")
    );
    assert!(
        beta.read_selected(&address("beta", "b"), 1)
            .unwrap()
            .has_active_row(&beta)
    );
    for id in ["a", "b"] {
        let selected_address = address("alpha", id);
        let selected = alpha.read_selected(&selected_address, 1).unwrap();
        assert!(selected.is_current(&alpha));
        let endpoint = agent.endpoint("alpha").unwrap();
        assert!(
            agent
                .read_thread_inventory(&endpoint, id)
                .unwrap()
                .item
                .is_some()
        );
        let mut display = SidebarInput {
            selected: Some(selected_address),
            ..alpha.snapshot().input(None)
        };
        assert!(!selected.has_active_row(&alpha));
        assert!(
            !aggregate
                .read_selected(&address("alpha", id), 1)
                .unwrap()
                .has_active_row(&aggregate)
        );
        assert!(!selected.project_into(&alpha, &mut display, 1));
        assert!(display.rows.is_empty());
    }
}
