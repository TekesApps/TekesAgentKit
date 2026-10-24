use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::RunScope;
use tekes_agent_ui_bindings::{
    RunBinding, SidebarBinding, SidebarMultiEndpointBinding, SidebarMultiPublication,
};
use tekes_agent_ui_bindings::{
    SidebarClassificationFact as Fact, SidebarClassificationOrigin as Origin,
};
use tekes_agent_ui_demo::fixture::Fixture;
use tekes_agent_web::SharedAgent;

fn address(endpoint: &str, session: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: session.into(),
    }
}

fn summary(id: &str, title: &str, fields: Value) -> Value {
    let mut value = json!({
        "sessionId": id,
        "title": title,
        "cwd": "/fixture",
        "updatedAt": 9_000.0,
        "running": false,
        "blank": false,
    });
    for (key, field_value) in fields.as_object().unwrap() {
        value[key.as_str()] = field_value.clone();
    }
    value
}

async fn wait_single(
    binding: &SidebarBinding,
    predicate: impl Fn(&tekes_agent_ui_bindings::SidebarPublication) -> bool,
) {
    let mut updates = binding.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate(&binding.snapshot()) {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

async fn wait_multi(
    binding: &SidebarMultiEndpointBinding,
    predicate: impl Fn(&SidebarMultiPublication) -> bool,
) {
    let mut updates = binding.subscribe();
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate(&binding.snapshot()) {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn classification_and_membership_share_the_original_inventory_frame_and_retain_history() {
    let fixture = Fixture::default()
        .with_endpoint_id("fixture")
        .with_session_search();
    fixture.install_unannounced_session("ephemeral", "ephemeral");
    fixture.install_unannounced_session("unknown-origin", "unknown origin");
    fixture.install_unannounced_session("unknown-parent", "unknown parent");
    fixture.install_unannounced_session("unknown-ephemeral", "unknown ephemeral");

    let mut agent = SharedAgent::default();
    agent
        .register(
            "fixture".into(),
            "native".into(),
            "Fixture".into(),
            Arc::new(fixture.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    let endpoint = agent.endpoint("fixture").unwrap();
    endpoint
        .runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let source = SidebarBinding::new(agent.clone(), "fixture").unwrap();
    let child_run = RunBinding::open_main(endpoint.runtime.clone(), RunScope::main("fixture", "a"))
        .await
        .unwrap();
    let original_history = child_run.current_run().unwrap().transcript.clone();
    assert!(!original_history.items.is_empty());

    for (id, fields) in [
        ("a", json!({"origin":"subagent","parentSessionId":"b"})),
        ("b", json!({"parentSessionId":"unloaded-parent"})),
        ("ephemeral", json!({"origin":"main","ephemeral":true})),
        (
            "unknown-origin",
            json!({"origin":"worker","parentSessionId":"b"}),
        ),
        ("unknown-parent", json!({"parentSessionId":{"session":"b"}})),
        ("unknown-ephemeral", json!({"ephemeral":"sometimes"})),
    ] {
        fixture
            .publish_inventory_summary(summary(id, id, fields))
            .unwrap();
    }

    wait_single(&source, |publication| {
        publication.loaded
            && publication
                .classification
                .as_ref()
                .is_some_and(|classification| {
                    [
                        "a",
                        "b",
                        "ephemeral",
                        "unknown-origin",
                        "unknown-parent",
                        "unknown-ephemeral",
                    ]
                    .into_iter()
                    .all(|id| classification.records.contains_key(&address("fixture", id)))
                        && classification.records[&address("fixture", "a")]
                            .facts
                            .origin
                            == Fact::Known(Origin::Subagent)
                        && classification.records[&address("fixture", "b")]
                            .facts
                            .parent_session_id
                            == Fact::Known("unloaded-parent".into())
                        && classification.records[&address("fixture", "ephemeral")]
                            .facts
                            .ephemeral
                            == Fact::Known(true)
                        && classification.records[&address("fixture", "unknown-ephemeral")]
                            .facts
                            .ephemeral
                            == Fact::Unknown(json!("sometimes"))
                })
    })
    .await;

    // Keep and inspect one immutable packet. The assertions below deliberately
    // do not join classification with a later snapshot or runtime inventory.
    let packet = source.observer_snapshot().unwrap();
    let publication = packet.publication();
    let publication = publication.as_ref();
    let classification = publication.classification.as_ref().unwrap();
    let corpus = publication.name_corpus.as_ref().unwrap();
    assert_eq!(classification.endpoint_id, "fixture");
    assert_eq!(classification.source_revision, publication.revision);
    assert_eq!(classification.connection_epoch, corpus.connection_epoch);
    assert_eq!(
        classification.observer_generation,
        publication.observation.generation
    );
    assert!(classification.inventory_ready);

    let row_addresses: BTreeSet<_> = publication
        .rows
        .iter()
        .map(|row| row.address.clone())
        .collect();
    for (session_id, record) in &classification.records {
        assert_eq!(
            record.included,
            row_addresses.contains(session_id),
            "included must describe the rows from this same publication: {session_id:?}"
        );
    }
    assert!(!classification.records[&address("fixture", "a")].included);
    assert!(!classification.records[&address("fixture", "ephemeral")].included);
    assert!(classification.records[&address("fixture", "b")].included);
    for id in ["unknown-origin", "unknown-parent", "unknown-ephemeral"] {
        assert!(classification.records[&address("fixture", id)].included);
    }

    let child = &classification.records[&address("fixture", "a")].facts;
    assert_eq!(child.origin, Fact::Known(Origin::Subagent));
    assert_eq!(child.parent_session_id, Fact::Known("b".into()));
    let parent_only = &classification.records[&address("fixture", "b")].facts;
    assert_eq!(parent_only.origin, Fact::Missing);
    assert_eq!(
        parent_only.parent_session_id,
        Fact::Known("unloaded-parent".into())
    );
    assert_eq!(
        classification.records[&address("fixture", "unknown-origin")]
            .facts
            .origin,
        Fact::Unknown(json!("worker"))
    );
    assert_eq!(
        classification.records[&address("fixture", "unknown-parent")]
            .facts
            .parent_session_id,
        Fact::Unknown(json!({"session":"b"}))
    );
    assert_eq!(
        classification.records[&address("fixture", "unknown-ephemeral")]
            .facts
            .ephemeral,
        Fact::Unknown(json!("sometimes"))
    );

    assert_eq!(
        child_run.current_run().unwrap().transcript,
        original_history
    );

    // A later canonical update changes membership through the same source,
    // while the original child transcript remains readable from that runtime.
    fixture
        .publish_inventory_summary(summary(
            "a",
            "reclassified child",
            json!({"origin":"fork","parentSessionId":"b"}),
        ))
        .unwrap();
    wait_single(&source, |next| {
        next.classification.as_ref().is_some_and(|next| {
            next.source_revision > classification.source_revision
                && next.records[&address("fixture", "a")].facts.origin == Fact::Known(Origin::Fork)
        })
    })
    .await;
    let updated = source.snapshot();
    let updated_classification = updated.classification.as_ref().unwrap();
    assert!(
        updated
            .rows
            .iter()
            .any(|row| row.address == address("fixture", "a"))
    );
    assert!(updated_classification.records[&address("fixture", "a")].included);
    assert_eq!(updated_classification.source_revision, updated.revision);

    let tekes_agent_ui_bindings::PaneSnapshot::Run { snapshot, .. } =
        child_run.snapshot().await.unwrap()
    else {
        panic!("classification changes do not remove canonical transcript history");
    };
    assert_eq!(snapshot.transcript, original_history);
    assert!(
        snapshot.transcript.items.iter().any(|item| {
            item.text.contains("历史问题") || item.text.contains("历史回答")
        })
    );
    endpoint.runtime.stop().await.unwrap();
}

#[tokio::test]
async fn multi_endpoint_classification_keeps_full_addresses_and_original_source_versions() {
    let fixture_a = Fixture::default()
        .with_endpoint_id("endpoint-a")
        .with_session_search();
    let fixture_b = Fixture::default()
        .with_endpoint_id("endpoint-b")
        .with_session_search();
    let mut agent = SharedAgent::default();
    agent
        .register(
            "endpoint-a".into(),
            "native".into(),
            "A".into(),
            Arc::new(fixture_a.clone()),
        )
        .unwrap();
    agent
        .register(
            "endpoint-b".into(),
            "native".into(),
            "B".into(),
            Arc::new(fixture_b.clone()),
        )
        .unwrap();
    let agent = Arc::new(agent);
    for endpoint_id in ["endpoint-a", "endpoint-b"] {
        agent
            .endpoint(endpoint_id)
            .unwrap()
            .runtime
            .wait_ready(Duration::from_secs(3))
            .await
            .unwrap();
    }
    let aggregate =
        SidebarMultiEndpointBinding::new(agent.clone(), ["endpoint-a", "endpoint-b"]).unwrap();
    fixture_a
        .publish_inventory_summary(summary(
            "a",
            "hidden A child",
            json!({"origin":"subagent","parentSessionId":"b"}),
        ))
        .unwrap();
    fixture_b
        .publish_inventory_summary(summary("a", "visible B row", json!({})))
        .unwrap();
    wait_multi(&aggregate, |publication| {
        publication.loaded
            && ["endpoint-a", "endpoint-b"].into_iter().all(|endpoint| {
                publication
                    .classification
                    .get(endpoint)
                    .is_some_and(|classification| {
                        classification
                            .records
                            .get(&address(endpoint, "a"))
                            .is_some_and(|record| {
                                if endpoint == "endpoint-a" {
                                    record.facts.origin == Fact::Known(Origin::Subagent)
                                } else {
                                    record.facts.origin == Fact::Missing
                                }
                            })
                    })
            })
    })
    .await;

    let original = aggregate.snapshot();
    for endpoint_id in ["endpoint-a", "endpoint-b"] {
        let classification = &original.classification[endpoint_id];
        let status = &original.endpoints[endpoint_id];
        let corpus = &original.name_corpora[endpoint_id];
        assert_eq!(classification.endpoint_id, endpoint_id);
        assert_eq!(classification.source_revision, status.revision);
        assert_eq!(classification.connection_epoch, corpus.connection_epoch);
        assert_eq!(
            classification.observer_generation,
            status.observation.generation
        );
    }
    let a = &original.classification["endpoint-a"].records[&address("endpoint-a", "a")];
    let b = &original.classification["endpoint-b"].records[&address("endpoint-b", "a")];
    assert!(!a.included);
    assert!(b.included);
    assert!(
        original
            .rows
            .iter()
            .all(|row| row.address != address("endpoint-a", "a"))
    );
    assert!(
        original
            .rows
            .iter()
            .any(|row| row.address == address("endpoint-b", "a"))
    );
    assert_ne!(
        address("endpoint-a", "a"),
        address("endpoint-b", "a"),
        "identical raw session IDs are distinct full addresses"
    );

    // A newer A packet must not rewrite B's facts or mutate the captured
    // older packet. Observer failure retention is covered by the original
    // observer tests using its explicit failure injector.
    fixture_a
        .publish_inventory_summary(summary(
            "a",
            "reclassified A row",
            json!({"origin":"fork","parentSessionId":"b"}),
        ))
        .unwrap();
    wait_multi(&aggregate, |publication| {
        publication.classification["endpoint-a"].records[&address("endpoint-a", "a")]
            .facts
            .origin
            == Fact::Known(Origin::Fork)
    })
    .await;
    let latest_a = aggregate.snapshot().classification["endpoint-a"].clone();
    assert_eq!(
        latest_a.records[&address("endpoint-a", "a")].facts.origin,
        Fact::Known(Origin::Fork)
    );
    assert_eq!(
        original.classification["endpoint-a"].records[&address("endpoint-a", "a")]
            .facts
            .origin,
        Fact::Known(Origin::Subagent)
    );
    assert_eq!(
        aggregate.snapshot().classification["endpoint-b"].records[&address("endpoint-b", "a")]
            .facts
            .origin,
        Fact::Missing
    );

    for endpoint_id in ["endpoint-a", "endpoint-b"] {
        agent
            .endpoint(endpoint_id)
            .unwrap()
            .runtime
            .stop()
            .await
            .unwrap();
    }
}
