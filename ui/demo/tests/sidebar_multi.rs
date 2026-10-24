use std::{sync::Arc, time::Duration};
use tekes_agent_contract::{SessionAddress, ThreadActionDefinition};
use tekes_agent_ui_bindings::{
    SidebarActionOutcome, SidebarMultiEndpointBinding, SidebarMultiPublication,
};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;

fn address(endpoint: &str, session: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: session.into(),
    }
}

async fn loaded(binding: &SidebarMultiEndpointBinding) -> Arc<SidebarMultiPublication> {
    let mut changes = binding.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let publication = binding.snapshot();
            if publication.loaded {
                return publication;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

async fn setup() -> (Arc<SharedAgent>, SidebarMultiEndpointBinding) {
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
                        .with_lifecycle(),
                ),
            )
            .unwrap();
    }
    let agent = Arc::new(agent);
    for endpoint in ["endpoint-a", "endpoint-b"] {
        agent
            .endpoint(endpoint)
            .unwrap()
            .runtime
            .wait_ready(Duration::from_secs(3))
            .await
            .unwrap();
    }
    let binding =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    loaded(&binding).await;
    (agent, binding)
}

#[tokio::test]
async fn fixed_set_rejects_empty_duplicate_and_unknown_endpoints() {
    let mut agent = SharedAgent::default();
    agent
        .register(
            "endpoint-a".into(),
            "native".into(),
            "A".into(),
            Arc::new(Fixture::default().with_endpoint_id("endpoint-a")),
        )
        .unwrap();
    let agent = Arc::new(agent);
    assert!(SidebarMultiEndpointBinding::new(agent.clone(), Vec::<String>::new()).is_err());
    assert!(SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-a"]).is_err());
    assert!(SidebarMultiEndpointBinding::new(agent, ["missing"]).is_err());
}

#[tokio::test]
async fn duplicate_session_ids_remain_endpoint_qualified_and_selected_read_is_owner_fenced() {
    let (agent, binding) = setup().await;
    let publication = binding.snapshot();
    let same_id: Vec<_> = publication
        .rows
        .iter()
        .filter(|row| row.address.session_id == "a")
        .collect();
    assert_eq!(same_id.len(), 2);
    assert_eq!(
        same_id
            .iter()
            .map(|row| row.address.endpoint_id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        ["endpoint-a", "endpoint-b"].into()
    );

    let selected = address("endpoint-b", "a");
    let receipt = binding.read_selected(&selected, 11).unwrap();
    assert!(receipt.is_current(&binding));
    let mut display = publication.input(Some(selected.clone()));
    display.rows.retain(|row| row.address != selected);
    assert!(receipt.project_into(&binding, &mut display, 11));
    assert!(display.rows.iter().any(|row| row.address == selected));
    assert!(!receipt.project_into(&binding, &mut display, 12));

    let replacement =
        SidebarMultiEndpointBinding::new(agent, ["endpoint-a", "endpoint-b"]).unwrap();
    assert!(!receipt.is_current(&replacement));
    let mut replaced_display = replacement.snapshot().input(Some(selected.clone()));
    replaced_display.rows.retain(|row| row.address != selected);
    assert!(!receipt.project_into(&replacement, &mut replaced_display, 11));
}

#[tokio::test]
async fn captured_action_routes_to_its_address_and_expires_with_aggregate_owner() {
    let (agent, binding) = setup().await;
    let target = address("endpoint-b", "a");
    let row = binding
        .snapshot()
        .rows
        .iter()
        .find(|row| row.address == target)
        .cloned()
        .unwrap();
    let definition: ThreadActionDefinition = row
        .actions
        .items
        .iter()
        .find(|action| action.id == "rename" && action.enabled)
        .unwrap()
        .clone();
    let ticket = binding.capture_action(&target, &definition).unwrap();
    let result = binding
        .perform_action(ticket, Some("B 端重命名".into()), "multi-rename-b")
        .await
        .unwrap();
    assert!(matches!(
        result,
        SidebarActionOutcome::Mutation { address, .. } if address == target
    ));
    let updated = tokio::time::timeout(Duration::from_secs(4), async {
        let mut changes = binding.subscribe();
        loop {
            let publication = binding.snapshot();
            if publication
                .rows
                .iter()
                .any(|row| row.address == target && row.title == "B 端重命名")
            {
                return publication;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        updated
            .rows
            .iter()
            .find(|row| row.address == address("endpoint-a", "a"))
            .unwrap()
            .title,
        "会话 a",
        "the same session ID at endpoint A was not mutated"
    );

    let row_b = updated
        .rows
        .iter()
        .find(|row| row.address == target)
        .unwrap();
    let captured_before_removal = binding
        .capture_action(
            &target,
            row_b
                .actions
                .items
                .iter()
                .find(|action| action.id == "rename" && action.enabled)
                .unwrap(),
        )
        .unwrap();
    let archive = row_b
        .actions
        .items
        .iter()
        .find(|action| action.id == "archive" && action.enabled)
        .unwrap();
    let archive_ticket = binding.capture_action(&target, archive).unwrap();
    binding
        .perform_action(archive_ticket, None, "multi-archive-b")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        let mut changes = binding.subscribe();
        loop {
            if !binding
                .snapshot()
                .rows
                .iter()
                .any(|row| row.address == target)
            {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(
        binding
            .perform_action(
                captured_before_removal,
                Some("删除后的旧票据".into()),
                "removed-row-action",
            )
            .await
            .is_err()
    );

    let second_target = address("endpoint-b", "b");
    let after_archive = binding.snapshot();
    let second_row = after_archive
        .rows
        .iter()
        .find(|row| row.address == second_target)
        .unwrap();
    let old_ticket = binding.capture_action(
        &second_target,
        second_row
            .actions
            .items
            .iter()
            .find(|action| action.id == "rename" && action.enabled)
            .unwrap(),
    );
    let replacement =
        SidebarMultiEndpointBinding::new(agent, ["endpoint-a", "endpoint-b"]).unwrap();
    assert!(old_ticket.is_ok());
    assert!(
        replacement
            .perform_action(old_ticket.unwrap(), Some("不可投递".into()), "old-owner",)
            .await
            .is_err()
    );
}
