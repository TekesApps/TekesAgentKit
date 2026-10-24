use std::collections::BTreeMap;
use tekes_agent_components_ui::*;
use tekes_agent_contract::{
    SessionAddress, SessionSearchHit, ThreadActionsPresentation, ThreadActivity,
    ThreadPinPresentation,
};
use tekes_agent_presentation::{
    SidebarNameCorpus, SidebarNameDocument, SidebarObserverPhase, SidebarSearchInput,
    SidebarSearchKey, SidebarSearchPhase,
};
use tekes_agent_ui_bindings::{SidebarPublication, SidebarSearchMatch, SidebarSearchPublication};

fn address(id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: "local".into(),
        session_id: id.into(),
    }
}

fn row(id: &str, title: &str, pinned: bool) -> SidebarRow {
    SidebarRow {
        primary_folder_id: None,
        folder_path: Some("/workspace".into()),
        address: address(id),
        title: title.into(),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: Some(ThreadPinPresentation { pinned }),
        actions: ThreadActionsPresentation {
            endpoint: "local".into(),
            session_id: id.into(),
            label: "Actions".into(),
            items: vec![],
        },
    }
}

fn corpus(domain: &[&str], ready: bool) -> SidebarNameCorpus {
    let titles = [
        ("p", "zeta needle"),
        ("a", "alpha needle"),
        ("b", "beta needle"),
        ("c", "gamma body"),
        ("d", "delta other"),
        // A retained but non-visible record participates in FTS corpus statistics.
        ("archived-retained", "needle needle needle"),
    ];
    SidebarNameCorpus {
        endpoint_id: "local".into(),
        connection_epoch: 1,
        inventory_ready: ready,
        endpoint_domain: domain.iter().map(|endpoint| (*endpoint).into()).collect(),
        documents: titles
            .iter()
            .map(|(id, title)| SidebarNameDocument {
                address: address(id),
                title: (*title).into(),
            })
            .collect(),
    }
}

fn publication(corpus: SidebarNameCorpus) -> SidebarPublication {
    let mut activity_at = BTreeMap::new();
    // Activity intentionally disagrees with the FTS and provider tiers.
    for (id, activity) in [
        ("p", 1.0),
        ("a", 2.0),
        ("b", 3.0),
        ("c", 99.0),
        ("d", 100.0),
    ] {
        activity_at.insert(address(id), activity);
    }
    SidebarPublication {
        observation: tekes_agent_ui_bindings::SidebarObservation {
            generation: 1,
            phase: SidebarObserverPhase::Observing,
            converged: true,
        },
        activity_at,
        name_corpus: Some(corpus),
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![
            row("p", "zeta needle", true),
            row("a", "alpha needle", false),
            row("b", "beta needle", false),
            row("c", "gamma body", false),
            row("d", "delta other", false),
        ],
    }
}

fn search(query: &str, hits: &[&str]) -> SidebarSearchPublication {
    SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "fixture-reader".into(),
                endpoint_id: "local".into(),
                connection_epoch: 1,
                generation: 1,
                query: query.into(),
            }),
            phase: SidebarSearchPhase::Complete { has_more: false },
        },
        query: query.into(),
        hits: hits
            .iter()
            .map(|id| SidebarSearchMatch {
                address: address(id),
                hit: SessionSearchHit {
                    session_id: (*id).into(),
                    snippet: format!("{id} body hit"),
                    extra: Default::default(),
                },
            })
            .collect(),
        unresolved: vec![],
        result: None,
    }
}

#[test]
fn retained_projection_unions_local_and_provider_matches_without_showing_hidden_corpus_rows() {
    let source = publication(corpus(&["local"], true));
    let retained = source.name_corpus.clone().unwrap();
    let projected = source
        .project_search_for_display_with_sort(
            "local",
            &search("needle", &["c"]),
            &source.input(None),
            SidebarSort::RecentActivity,
        )
        .unwrap();
    let second_query = source
        .project_search_for_display_with_sort(
            "local",
            &search("body", &[]),
            &source.input(None),
            SidebarSort::RecentActivity,
        )
        .unwrap();
    assert_eq!(
        second_query
            .input
            .rows
            .iter()
            .map(|row| row.address.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["c"]
    );
    let ids = projected
        .input
        .rows
        .iter()
        .map(|row| row.address.session_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 4);
    assert!(ids[..3].iter().all(|id| ["p", "a", "b"].contains(id)));
    assert_eq!(ids[3], "c", "provider-only match follows local FTS matches");
    assert!(!ids.contains(&"d"));
    assert!(!ids.contains(&"archived-retained"));
    assert_eq!(source.name_corpus.as_ref(), Some(&retained));
}

#[test]
fn filtering_and_paging_consume_the_projection_while_the_full_corpus_stays_fixed() {
    let source = publication(corpus(&["local"], true));
    let retained = source.name_corpus.clone().unwrap();
    let query = search("needle", &["c"]);
    let projected = source
        .project_search_for_display_with_sort(
            "local",
            &query,
            &source.input(None),
            SidebarSort::RecentActivity,
        )
        .unwrap();
    let folder = projected.input.rows[0].folder_key();
    let (folder_outline, folder_page) = sidebar_paged_outline(
        &projected.input.rows,
        &SidebarPreferences::default(),
        None,
        Some(&folder),
        None,
        1,
        true,
    )
    .unwrap();
    assert_eq!(folder_outline.pinned.len(), 1);
    assert_eq!(folder_outline.sessions.len(), 1);
    assert_eq!(folder_outline.sessions[0].address, address("b"));
    assert!(folder_page.has_more);

    let (first_page, facts) = sidebar_paged_outline(
        &projected.input.rows,
        &SidebarPreferences::default(),
        None,
        None,
        None,
        1,
        true,
    )
    .unwrap();
    assert_eq!(first_page.sessions.len(), 1);
    assert_eq!(facts.ordinary_count, 3);
    assert!(facts.has_more);
    assert_eq!(source.name_corpus.as_ref(), Some(&retained));
    assert_eq!(query.query, "needle");
}

#[test]
fn component_preference_order_keeps_pins_and_title_sort_outside_fts_ranking() {
    let source = publication(corpus(&["local"], true));
    let projected = source
        .project_search_for_display_with_sort(
            "local",
            &search("needle", &["c"]),
            &source.input(None),
            SidebarSort::RecentActivity,
        )
        .unwrap();
    let preferences = SidebarPreferences {
        sort: SidebarSort::TitleAscending,
        pin_order: vec![address("p")],
        ..Default::default()
    };
    let ordered =
        ordered_sidebar_rows(&projected.input.rows, &preferences, Some(str::cmp)).unwrap();
    let ids = ordered
        .iter()
        .map(|row| row.address.session_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids[0], "p", "pinned order remains component-owned");
    assert_eq!(&ids[1..], &["a", "b", "c"]);
}

#[test]
fn incomplete_retained_corpus_does_not_claim_authoritative_empty_search() {
    for corpus in [
        corpus(&["local", "remote"], true),
        corpus(&["local"], false),
    ] {
        let source = publication(corpus);
        let projected = source
            .project_search_for_display_with_sort(
                "local",
                &search("not-present", &[]),
                &source.input(None),
                SidebarSort::RecentActivity,
            )
            .unwrap();
        let help = projected.input.search_help.unwrap();
        assert!(!help.local_results_ready);
        assert!(
            help.results_ready,
            "provider readiness remains independent from local corpus readiness"
        );
    }
}
