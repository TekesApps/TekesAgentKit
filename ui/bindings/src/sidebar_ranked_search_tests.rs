use super::{SidebarNameDocument, SidebarNameIndex, sidebar_search::order_recent};
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarObserverSourceKey;

fn address(endpoint_id: &str, session_id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: endpoint_id.into(),
        session_id: session_id.into(),
    }
}

fn doc(endpoint_id: &str, session_id: &str, title: &str) -> SidebarNameDocument {
    SidebarNameDocument {
        address: address(endpoint_id, session_id),
        title: title.into(),
    }
}

#[test]
fn full_retained_corpus_changes_bm25_without_changing_visible_membership() {
    let owner = Arc::new(());
    let source = SidebarObserverSourceKey::from_owner(&owner);
    let mut index = SidebarNameIndex::new().unwrap();
    let visible = vec![
        doc("one", "same-id", "amber notebook"),
        doc("two", "same-id", "amber notes"),
    ];
    let epochs = BTreeMap::from([("one".into(), 1), ("two".into(), 1)]);
    index
        .replace_ranked_snapshot(
            &source,
            7,
            &visible,
            epochs.clone(),
            vec!["one".into(), "two".into()],
            true,
        )
        .unwrap();
    let before = index.matches_with_scores("amber").unwrap();
    assert_eq!(before.len(), 2);
    assert_ne!(
        before[0].0, before[1].0,
        "full addresses distinguish cross-endpoint IDs"
    );

    // Archived/child/ephemeral retained names participate in IDF and bm25,
    // while the caller still filters result addresses against visible rows.
    let mut full = visible.clone();
    full.push(doc("one", "retained-hidden", "amber amber amber archive"));
    index
        .replace_ranked_snapshot(
            &source,
            8,
            &full,
            epochs,
            vec!["one".into(), "two".into()],
            true,
        )
        .unwrap();
    let after = index.matches_with_scores("amber").unwrap();
    assert_eq!(after.len(), 3);
    let visible_addresses: std::collections::BTreeSet<_> =
        visible.iter().map(|d| d.address.clone()).collect();
    let displayed: std::collections::BTreeSet<_> = after
        .iter()
        .map(|(address, _)| address.clone())
        .filter(|address| visible_addresses.contains(address))
        .collect();
    assert_eq!(displayed, visible_addresses);
    let old_score = before
        .iter()
        .find(|(address, _)| address.session_id == "same-id" && address.endpoint_id == "one")
        .unwrap()
        .1;
    let new_score = after
        .iter()
        .find(|(address, _)| address.session_id == "same-id" && address.endpoint_id == "one")
        .unwrap()
        .1;
    assert_ne!(
        old_score, new_score,
        "hidden retained names must affect the FTS corpus score"
    );
}

#[test]
fn ranked_snapshot_fences_metadata_and_source_replacement() {
    let owner = Arc::new(());
    let source = SidebarObserverSourceKey::from_owner(&owner);
    let next_owner = Arc::new(());
    let next_source = SidebarObserverSourceKey::from_owner(&next_owner);
    let docs = vec![doc("one", "s", "amber title")];
    let epochs = BTreeMap::from([("one".into(), 3)]);
    let mut index = SidebarNameIndex::new().unwrap();
    index
        .replace_ranked_snapshot(&source, 4, &docs, epochs.clone(), vec!["one".into()], true)
        .unwrap();
    assert!(
        index
            .replace_ranked_snapshot(&source, 4, &docs, epochs.clone(), vec!["one".into()], false)
            .is_err()
    );
    assert!(
        index
            .replace_ranked_snapshot(
                &source,
                4,
                &docs,
                BTreeMap::from([("one".into(), 4)]),
                vec!["one".into()],
                true
            )
            .is_err()
    );
    assert!(
        index
            .replace_ranked_snapshot(&source, 3, &docs, epochs.clone(), vec!["one".into()], true)
            .is_err()
    );
    index
        .replace_ranked_snapshot(&next_source, 1, &docs, epochs, vec!["one".into()], true)
        .unwrap();
}

#[test]
fn recent_sort_uses_local_rank_then_activity_then_full_address_with_provider_default() {
    use std::collections::BTreeSet;
    use tekes_agent_components_ui::SidebarRow;
    use tekes_agent_contract::{ThreadActionsPresentation, ThreadActivity};
    let row = |endpoint: &str, session: &str| SidebarRow {
        primary_folder_id: None,
        folder_path: None,
        address: address(endpoint, session),
        title: session.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: None,
        actions: ThreadActionsPresentation {
            endpoint: endpoint.into(),
            session_id: session.into(),
            label: String::new(),
            items: vec![],
        },
    };
    let mut rows = vec![
        row("two", "provider"),
        row("two", "same"),
        row("one", "same"),
        row("one", "local"),
    ];
    let activity = BTreeMap::from([
        (address("two", "provider"), 500.0),
        (address("two", "same"), 100.0),
        (address("one", "same"), 100.0),
        (address("one", "local"), 1.0),
    ]);
    let scores = BTreeMap::from([(address("one", "local"), -0.5)]);
    let provider = BTreeSet::from([
        address("two", "provider"),
        address("one", "same"),
        address("two", "same"),
    ]);
    order_recent(&mut rows, &activity, &scores, &provider);
    let ids: Vec<_> = rows.into_iter().map(|row| row.address).collect();
    assert_eq!(
        ids,
        vec![
            address("one", "local"),
            address("two", "provider"),
            address("one", "same"),
            address("two", "same")
        ]
    );
}
