use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tekes_agent_components_ui::{Sidebar, SidebarInput, SidebarLabels, SidebarRow};
use tekes_agent_contract::{
    SessionAddress, SessionSearchHit, ThreadActionDefinition, ThreadActionsPresentation,
    ThreadActivity,
};
use tekes_agent_presentation::{
    SidebarMultiSearchChild, SidebarMultiSearchInput, SidebarMultiSearchKey,
    SidebarMultiSearchPublication, SidebarNameCorpus, SidebarNameDocument, SidebarObservation,
    SidebarObserverEndpointInput, SidebarObserverKey, SidebarObserverPhase,
    SidebarObserverSourceKey, SidebarObserverStatusInput, SidebarSearchInput, SidebarSearchKey,
    SidebarSearchMatch, SidebarSearchPhase, SidebarSearchPublication,
};
use tekes_agent_ui_bindings::{
    SidebarClassificationFact, SidebarClassificationFacts, SidebarClassificationOrigin,
    SidebarClassificationPublication, SidebarClassificationRecord, SidebarEndpointStatus,
    SidebarMultiPublication, SidebarPublication,
};
use tekes_agent_ui_demo::sidebar_delivery_receipt::sidebar_delivery_receipt_from_facts;

fn address(endpoint: &str, session: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint.into(),
        session_id: session.into(),
    }
}

fn row(endpoint: &str, session: &str, index: usize) -> SidebarRow {
    SidebarRow {
        primary_folder_id: Some(format!("folder-{endpoint}")),
        folder_path: Some(format!("/fixture/{endpoint}")),
        address: address(endpoint, session),
        title: format!("title-{index}"),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: None,
        actions: ThreadActionsPresentation {
            endpoint: endpoint.into(),
            session_id: session.into(),
            label: "actions".into(),
            items: vec![ThreadActionDefinition {
                id: "copy".into(),
                label: "copy".into(),
                icon: "copy".into(),
                interaction: "copy".into(),
                enabled: true,
                disabled_reason: None,
                copy_text: Some("Authorization: Bearer fixture-secret".into()),
            }],
        },
    }
}

fn observer(endpoints: &[&str]) -> SidebarObserverStatusInput {
    let owner = Arc::new(());
    let children: Vec<_> = endpoints.iter().map(|_| Arc::new(())).collect();
    SidebarObserverStatusInput {
        source: SidebarObserverSourceKey::from_owner(&owner),
        revision: 7,
        endpoints: endpoints
            .iter()
            .zip(children)
            .map(|(endpoint, child)| SidebarObserverEndpointInput {
                key: SidebarObserverKey::from_owners(&owner, &child, endpoint, 3).unwrap(),
                revision: 7,
                observation: SidebarObservation {
                    generation: 3,
                    phase: SidebarObserverPhase::Observing,
                    converged: true,
                },
                error: None,
            })
            .collect(),
    }
}

fn corpus(endpoint: &str, all: &[SidebarRow]) -> SidebarNameCorpus {
    SidebarNameCorpus {
        endpoint_id: endpoint.into(),
        connection_epoch: 42,
        inventory_ready: true,
        endpoint_domain: vec![endpoint.into()],
        documents: all
            .iter()
            .filter(|row| row.address.endpoint_id == endpoint)
            .map(|row| SidebarNameDocument {
                address: row.address.clone(),
                title: row.title.clone(),
            })
            .collect(),
    }
}

fn facts(
    rows: Vec<SidebarRow>,
    endpoints: &[&str],
) -> (SidebarPublication, SidebarMultiPublication) {
    let activity_at: BTreeMap<SessionAddress, f64> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address.clone(), index as f64 + 100.0))
        .collect();
    let name_corpora = endpoints
        .iter()
        .map(|endpoint| ((*endpoint).to_owned(), corpus(endpoint, &rows)))
        .collect();
    let endpoint_statuses = endpoints
        .iter()
        .map(|endpoint| {
            (
                (*endpoint).to_owned(),
                SidebarEndpointStatus {
                    endpoint_id: (*endpoint).into(),
                    revision: 7,
                    loaded: true,
                    observation: SidebarObservation {
                        generation: 3,
                        phase: SidebarObserverPhase::Observing,
                        converged: true,
                    },
                    stale: false,
                    terminal_error: false,
                    error: None,
                },
            )
        })
        .collect();
    let multi = SidebarMultiPublication {
        classification: Default::default(),
        revision: 7,
        rows: rows.clone(),
        activity_at: activity_at.clone(),
        name_corpora,
        loaded: true,
        error: None,
        endpoints: endpoint_statuses,
    };
    let publication = SidebarPublication {
        classification: None,
        revision: 7,
        observation: tekes_agent_presentation::SidebarObservation::default(),
        rows,
        activity_at,
        name_corpus: None,
        loaded: true,
        error: None,
    };
    (publication, multi)
}

fn classification_source(
    endpoint: &str,
    rows: &[SidebarRow],
    archive_facts: Option<serde_json::Value>,
) -> SidebarClassificationPublication {
    use SidebarClassificationFact as Fact;

    SidebarClassificationPublication {
        endpoint_id: endpoint.into(),
        source_revision: 7,
        connection_epoch: 42,
        observer_generation: 3,
        inventory_ready: true,
        archive_facts,
        records: rows
            .iter()
            .filter(|row| row.address.endpoint_id == endpoint)
            .map(|row| {
                (
                    row.address.clone(),
                    SidebarClassificationRecord {
                        facts: SidebarClassificationFacts {
                            origin: Fact::Known(SidebarClassificationOrigin::Fork),
                            parent_session_id: Fact::Missing,
                            ephemeral: Fact::Known(false),
                        },
                        included: true,
                    },
                )
            })
            .collect(),
    }
}

fn classified_multi_facts(
    archive_facts: Option<serde_json::Value>,
) -> (SidebarPublication, SidebarMultiPublication) {
    let rows = vec![row("endpoint-a", "a-1", 0), row("endpoint-b", "b-1", 1)];
    let (mut publication, mut multi) = facts(rows, &["endpoint-a", "endpoint-b"]);
    publication.observation = tekes_agent_presentation::SidebarObservation {
        generation: 3,
        phase: SidebarObserverPhase::Observing,
        converged: true,
    };
    for status in multi.endpoints.values_mut() {
        status.observation = publication.observation;
    }
    multi.classification.insert(
        "endpoint-a".into(),
        classification_source("endpoint-a", &publication.rows, archive_facts),
    );
    (publication, multi)
}

fn classified_receipt(
    publication: &SidebarPublication,
    multi: &SidebarMultiPublication,
) -> Result<serde_json::Value, &'static str> {
    let mut sidebar = Sidebar::new(85, SidebarLabels::default());
    sidebar.update(&publication.input(None)).unwrap();
    sidebar_delivery_receipt_from_facts(
        publication,
        &observer(&["endpoint-a", "endpoint-b"]),
        2,
        Some(multi),
        &multi.folder_catalog().unwrap(),
        &sidebar,
        None,
    )
}

fn search(endpoints: &[&str], hit_addresses: &[SessionAddress]) -> SidebarMultiSearchPublication {
    let key = SidebarMultiSearchKey {
        owner: "fixture-search-owner".into(),
        generation: 11,
        query: "needle".into(),
    };
    let children = endpoints
        .iter()
        .map(|endpoint| {
            let matches: Vec<_> = hit_addresses
                .iter()
                .filter(|address| address.endpoint_id == *endpoint)
                .cloned()
                .collect();
            SidebarMultiSearchChild {
                endpoint_id: (*endpoint).into(),
                publication: SidebarSearchPublication {
                    input: SidebarSearchInput {
                        revision: 9,
                        key: Some(SidebarSearchKey {
                            owner: format!("reader-{endpoint}"),
                            endpoint_id: (*endpoint).into(),
                            connection_epoch: 42,
                            generation: 4,
                            query: "needle".into(),
                        }),
                        phase: SidebarSearchPhase::Complete { has_more: false },
                    },
                    query: "needle".into(),
                    hits: matches
                        .iter()
                        .map(|address| SidebarSearchMatch {
                            address: address.clone(),
                            hit: SessionSearchHit {
                                session_id: address.session_id.clone(),
                                snippet: "do not serialize snippets".into(),
                                extra: BTreeMap::new(),
                            },
                        })
                        .collect(),
                    unresolved: matches.into_iter().take(1).collect(),
                    result: None,
                },
            }
        })
        .collect();
    SidebarMultiSearchPublication {
        input: SidebarMultiSearchInput {
            owner: key.owner.clone(),
            generation: key.generation,
            revision: 13,
            key: Some(key),
        },
        query: "needle".into(),
        children,
    }
}

#[test]
fn serializes_full_endpoint_identity_separately_from_query_rows_and_paging() {
    let mut rows = vec![
        row("endpoint-a", "same-id", 0),
        row("endpoint-b", "same-id", 1),
    ];
    for index in 2..8 {
        let endpoint = if index % 2 == 0 {
            "endpoint-a"
        } else {
            "endpoint-b"
        };
        rows.push(row(endpoint, &format!("session-{index}"), index));
    }
    let all_rows = rows.clone();
    let endpoints = ["endpoint-a", "endpoint-b"];
    let (publication, multi) = facts(rows, &endpoints);
    let mut display_rows: Vec<_> = all_rows
        .iter()
        .filter(|row| row.title.contains("title") && row.address.session_id != "session-7")
        .cloned()
        .collect();
    display_rows.truncate(6);
    let selected = Some(address("endpoint-b", "same-id"));
    let input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: publication.revision + 101,
        loaded: publication.loaded,
        error: publication.error.clone(),
        rows: display_rows,
        selected,
    };
    let mut sidebar = Sidebar::new(81, SidebarLabels::default());
    sidebar.update(&input).unwrap();

    let same_id_hits = [
        address("endpoint-a", "same-id"),
        address("endpoint-b", "same-id"),
    ];
    let applied_search = search(&endpoints, &same_id_hits);
    sidebar
        .replace_multi_search_source(&applied_search.input.owner, &applied_search)
        .unwrap();

    let catalog = multi.folder_catalog().unwrap();
    let receipt = sidebar_delivery_receipt_from_facts(
        &publication,
        &observer(&endpoints),
        2,
        Some(&multi),
        &catalog,
        &sidebar,
        Some(&applied_search),
    )
    .unwrap();

    assert_eq!(
        receipt["source"]["rows"].as_array().unwrap().len(),
        all_rows.len()
    );
    assert_eq!(receipt["display"]["rows"].as_array().unwrap().len(), 6);
    assert_eq!(receipt["display"]["paging"]["limit"], 5);
    assert_eq!(receipt["display"]["paging"]["hasMore"], true);
    assert_eq!(receipt["display"]["rowIds"].as_array().unwrap().len(), 5);
    let ids: BTreeSet<_> = receipt["source"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["address"]["sessionId"] == "same-id")
        .map(|row| row["address"]["endpointId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        ids,
        ["endpoint-a".to_owned(), "endpoint-b".to_owned()]
            .into_iter()
            .collect()
    );
    assert_eq!(receipt["display"]["multiSearch"]["query"], "needle");
    assert_eq!(receipt["display"]["multiSearch"]["generation"], 11);
    assert_eq!(
        receipt["source"]["nameCorpora"].as_array().unwrap().len(),
        2
    );
    assert_eq!(receipt["nativeInteraction"], false);
    assert_eq!(receipt["display"]["rowIdsProveMount"], false);
    assert_eq!(receipt["source"]["revision"], 7);
    assert_eq!(receipt["display"]["revision"], 108);

    let serialized = receipt.to_string();
    assert!(!serialized.contains("fixture-secret"));
    assert!(!serialized.contains("do not serialize snippets"));
    assert!(!serialized.contains("copyText"));
    assert!(!serialized.contains("credentials"));
}

#[test]
fn rejects_display_and_multi_search_publications_from_another_source() {
    let source_row_a = row("endpoint-a", "same-id", 0);
    let source_row_b = row("endpoint-b", "same-id", 1);
    let (publication, multi) = facts(
        vec![source_row_a, source_row_b],
        &["endpoint-a", "endpoint-b"],
    );
    let unrelated = row("endpoint-a", "forged-same-revision", 2);
    let bad_input = SidebarInput {
        folder_filter: None,
        search_help: None,
        revision: publication.revision,
        loaded: true,
        error: None,
        rows: vec![unrelated],
        selected: None,
    };
    let mut sidebar = Sidebar::new(82, SidebarLabels::default());
    sidebar.update(&bad_input).unwrap();
    let catalog = multi.folder_catalog().unwrap();
    assert!(
        sidebar_delivery_receipt_from_facts(
            &publication,
            &observer(&["endpoint-a", "endpoint-b"]),
            2,
            Some(&multi),
            &catalog,
            &sidebar,
            None,
        )
        .is_err()
    );

    let original_input = publication.input(None);
    let mut search_sidebar = Sidebar::new(83, SidebarLabels::default());
    search_sidebar.update(&original_input).unwrap();
    let search = search(
        &["endpoint-a", "endpoint-b"],
        &[
            address("endpoint-a", "same-id"),
            address("endpoint-b", "same-id"),
        ],
    );
    let mut wrong_query = search.clone();
    wrong_query.query = "different".into();
    wrong_query.input.key.as_mut().unwrap().query = "different".into();
    for child in &mut wrong_query.children {
        child.publication.query = "different".into();
        child.publication.input.key.as_mut().unwrap().query = "different".into();
    }
    search_sidebar
        .replace_multi_search_source(&search.input.owner, &search)
        .unwrap();
    let catalog = multi.folder_catalog().unwrap();
    assert!(
        sidebar_delivery_receipt_from_facts(
            &publication,
            &observer(&["endpoint-a", "endpoint-b"]),
            2,
            Some(&multi),
            &catalog,
            &search_sidebar,
            Some(&wrong_query),
        )
        .is_err()
    );
}

#[test]
fn rejects_an_address_outside_the_original_endpoint_domain() {
    let (publication, multi) = facts(
        vec![row("foreign-endpoint", "same-id", 0)],
        &["endpoint-a", "endpoint-b"],
    );
    let mut sidebar = Sidebar::new(84, SidebarLabels::default());
    sidebar.update(&publication.input(None)).unwrap();
    assert!(
        sidebar_delivery_receipt_from_facts(
            &publication,
            &observer(&["endpoint-a", "endpoint-b"]),
            2,
            Some(&multi),
            &multi.folder_catalog().unwrap(),
            &sidebar,
            None,
        )
        .is_err()
    );
}

#[test]
fn rejects_classification_from_another_revision_generation_or_endpoint() {
    let (publication, mut multi) = classified_multi_facts(None);
    let source = multi.classification.get_mut("endpoint-a").unwrap();
    source.source_revision = 6;
    assert!(classified_receipt(&publication, &multi).is_err());

    let (publication, mut multi) = classified_multi_facts(None);
    let source = multi.classification.get_mut("endpoint-a").unwrap();
    source.observer_generation = 2;
    assert!(classified_receipt(&publication, &multi).is_err());

    let (publication, mut multi) = classified_multi_facts(None);
    let mut source = multi.classification.remove("endpoint-a").unwrap();
    source.endpoint_id = "retired-endpoint".into();
    multi
        .classification
        .insert("retired-endpoint".into(), source);
    assert!(classified_receipt(&publication, &multi).is_err());
}

#[test]
fn rejects_classification_records_that_do_not_match_source_rows_and_activity_keys() {
    let (publication, mut multi) = classified_multi_facts(None);
    multi
        .classification
        .get_mut("endpoint-a")
        .unwrap()
        .records
        .clear();
    assert!(classified_receipt(&publication, &multi).is_err());

    let (publication, mut multi) = classified_multi_facts(None);
    multi
        .classification
        .get_mut("endpoint-a")
        .unwrap()
        .records
        .values_mut()
        .next()
        .unwrap()
        .included = false;
    assert!(classified_receipt(&publication, &multi).is_err());

    let (mut publication, multi) = classified_multi_facts(None);
    publication
        .activity_at
        .remove(&address("endpoint-a", "a-1"));
    assert!(classified_receipt(&publication, &multi).is_err());
}

#[test]
fn classification_receipt_preserves_raw_archive_fact_values() {
    let (publication, multi) = classified_multi_facts(Some(serde_json::json!(false)));
    let receipt = classified_receipt(&publication, &multi).unwrap();
    assert_eq!(
        receipt["source"]["classification"][0]["archiveFacts"],
        serde_json::json!(false)
    );

    let raw = serde_json::json!({
        "workspaceReady": false,
        "archivedSessionIds": ["raw-id"],
        "retainedSummaryStatuses": {"raw-id": null, "future-id": {"state": "future"}}
    });
    let (publication, multi) = classified_multi_facts(Some(raw.clone()));
    let receipt = classified_receipt(&publication, &multi).unwrap();
    assert_eq!(receipt["source"]["classification"][0]["archiveFacts"], raw);
}

#[test]
fn classification_receipt_distinguishes_missing_archive_facts_from_raw_null() {
    let (publication, multi) = classified_multi_facts(None);
    let missing = classified_receipt(&publication, &multi).unwrap();
    let (publication, multi) = classified_multi_facts(Some(serde_json::Value::Null));
    let explicit_null = classified_receipt(&publication, &multi).unwrap();

    assert_eq!(
        missing["source"]["classification"][0]["archiveFacts"],
        serde_json::Value::Null
    );
    assert_eq!(
        explicit_null["source"]["classification"][0]["archiveFacts"],
        serde_json::Value::Null
    );
    assert!(
        missing["source"]["classification"][0]
            .get("archiveFacts")
            .is_none()
    );
    assert!(
        explicit_null["source"]["classification"][0]
            .get("archiveFacts")
            .is_some()
    );
    assert_ne!(missing, explicit_null);
}
