use std::sync::Arc;
use tekes_agent_components_ui::*;
use tekes_agent_contract::{
    SessionAddress, SessionSearchHit, ThreadActionsPresentation, ThreadActivity,
};
use tekes_agent_ui_bindings::{SidebarPublication, SidebarSearchMatch, SidebarSearchPublication};

fn address(id: &str) -> SessionAddress {
    SessionAddress {
        endpoint_id: "fixture".into(),
        session_id: id.into(),
    }
}
fn row(id: &str) -> SidebarRow {
    SidebarRow {
        primary_folder_id: None,
        folder_path: None,
        address: address(id),
        title: format!("unrelated title {id}"),
        activity: ThreadActivity {
            status: "idle".into(),
            is_stale: false,
        },
        pin: None,
        actions: ThreadActionsPresentation {
            endpoint: "fixture".into(),
            session_id: id.into(),
            label: "操作".into(),
            items: vec![],
        },
    }
}
fn inventory() -> SidebarPublication {
    SidebarPublication {
        classification: None,
        observation: Default::default(),
        activity_at: Default::default(),
        name_corpus: None,
        revision: 1,
        loaded: true,
        error: None,
        rows: vec![row("a"), row("c"), row("b")],
    }
}

#[test]
fn active_hidden_hits_are_known_without_rows_or_snippets_and_replacement_forgets_them() {
    let mut source = inventory();
    let result = search();
    source.rows.retain(|row| row.address != address("b"));
    source.activity_at.insert(address("b"), 42.0);
    let projection = source.project_search("fixture", &result, None, 1).unwrap();
    assert!(projection.unresolved.is_empty());
    assert!(projection.input.search_help.as_ref().unwrap().results_ready);
    assert!(
        projection
            .input
            .rows
            .iter()
            .all(|row| row.address != address("b"))
    );
    assert!(!projection.snippets.contains_key(&address("b")));

    // Retained names alone cannot turn an archived/removed identity into an
    // active hit. A replacement inventory must forget its old activity facts.
    source.activity_at.clear();
    source.revision += 1;
    source.name_corpus = Some(tekes_agent_presentation::SidebarNameCorpus {
        endpoint_id: "fixture".into(),
        connection_epoch: 1,
        inventory_ready: true,
        endpoint_domain: vec!["fixture".into()],
        documents: vec![tekes_agent_presentation::SidebarNameDocument {
            address: address("b"),
            title: "正文命中".into(),
        }],
    });
    let next = source.project_search("fixture", &result, None, 2).unwrap();
    assert_eq!(next.unresolved, vec![address("b")]);
    assert!(!next.input.search_help.as_ref().unwrap().results_ready);
}

#[test]
fn rejects_invalid_active_inventory_facts_even_when_no_visible_row_uses_them() {
    for (endpoint, id, activity) in [
        ("other", "b", 1.0),
        ("fixture", "", 1.0),
        ("fixture", "hidden", f64::NAN),
    ] {
        let mut source = inventory();
        source.activity_at.insert(
            SessionAddress {
                endpoint_id: endpoint.into(),
                session_id: id.into(),
            },
            activity,
        );
        assert!(
            source
                .project_search("fixture", &search(), None, 1)
                .is_err()
        );
    }
}
fn search() -> SidebarSearchPublication {
    SidebarSearchPublication {
        input: SidebarSearchInput {
            revision: 1,
            key: Some(SidebarSearchKey {
                owner: "search-reader".into(),
                endpoint_id: "fixture".into(),
                connection_epoch: 1,
                generation: 1,
                query: "正文命中".into(),
            }),
            phase: SidebarSearchPhase::Complete { has_more: false },
        },
        query: "正文命中".into(),
        hits: ["b", "a"]
            .map(|id| SidebarSearchMatch {
                address: address(id),
                hit: SessionSearchHit {
                    session_id: id.into(),
                    snippet: format!("{id} 的正文摘要"),
                    extra: Default::default(),
                },
            })
            .into(),
        unresolved: vec![],
        result: None,
    }
}
#[test]
fn joins_body_only_hits_without_title_filter_or_provider_reordering_or_selection_change() {
    let source = inventory();
    let result = search();
    let projection = source
        .project_search("fixture", &result, Some(address("c")), 9)
        .unwrap();
    assert_eq!(
        projection
            .input
            .rows
            .iter()
            .map(|row| row.address.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(projection.input.selected, Some(address("c")));
    assert_eq!(projection.input.revision, 9);
    assert!(projection.input.loaded);
    assert_eq!(projection.snippets[&address("b")], "b 的正文摘要");
    assert_eq!(source.rows.len(), 3);
    assert_eq!(result.hits[0].address, address("b"));
}
#[test]
fn unresolved_inventory_is_not_confirmed_empty_and_recovers_without_another_query() {
    let mut source = inventory();
    let mut result = search();
    source.rows.clear();
    let first = source.project_search("fixture", &result, None, 1).unwrap();
    assert!(first.input.loaded);
    assert!(!first.input.search_help.as_ref().unwrap().results_ready);
    assert_eq!(first.unresolved, vec![address("b"), address("a")]);
    assert!(first.snippets.is_empty());
    result.unresolved = first.unresolved;
    source.rows = inventory().rows;
    let next = source.project_search("fixture", &result, None, 2).unwrap();
    assert!(next.input.loaded);
    assert!(next.unresolved.is_empty());
    assert_eq!(next.input.rows.len(), 2);
    result.input.phase = SidebarSearchPhase::Searching;
    let waiting = source.project_search("fixture", &result, None, 3).unwrap();
    assert!(waiting.input.loaded);
    assert!(!waiting.input.search_help.as_ref().unwrap().results_ready);
    assert!(waiting.input.rows.is_empty());
    assert!(waiting.snippets.is_empty());
}
#[test]
fn rejects_cross_endpoint_duplicate_inconsistent_and_retained_clear_payloads() {
    let source = inventory();
    let mut invalid_inventory = source.clone();
    invalid_inventory.revision = 0;
    assert!(
        invalid_inventory
            .project_search("fixture", &search(), None, 1)
            .is_err()
    );
    let mut result = search();
    result.hits[0].address.endpoint_id = "other".into();
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.hits.push(result.hits[0].clone());
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.hits[0].hit.session_id = "a".into();
    assert!(source.project_search("fixture", &result, None, 1).is_err());
    result = search();
    result.input.key = None;
    result.query.clear();
    result.input.phase = SidebarSearchPhase::Idle;
    assert!(source.project_search("fixture", &result, None, 1).is_err());
}
#[tokio::test]
async fn demo_projects_full_publications_and_clearing_restores_inventory_without_navigation_or_draft_changes()
 {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    view.set_sidebar_source(Arc::new(inventory())).unwrap();
    view.sync().unwrap();
    let old_unmatched_row = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("c"))
        .unwrap();
    view.tree.accessibility_action(
        old_unmatched_row,
        None,
        tekes_ui_controls::composition::AccessibilityAction::Press,
    );
    let mut result = search();
    view.bind_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(view.drain_commands().unwrap().is_empty());
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(sidebar.row_id(&address("c")).is_none());
    assert_eq!(sidebar.displayed_input().unwrap().rows.len(), 2);
    assert_eq!(
        view.sidebar_search_snippet(&address("a")),
        Some("a 的正文摘要")
    );
    let before = sidebar.displayed_input().unwrap().clone();
    let mut conflict = result.clone();
    conflict.hits.clear();
    assert!(
        view.update_sidebar_search_publication("search-reader", &conflict)
            .is_err()
    );
    assert_eq!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input(),
        Some(&before)
    );
    result.input.revision += 1;
    result.input.key.as_mut().unwrap().generation += 1;
    result.query = "另一个查询".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.hits.clear();
    result.input.phase = SidebarSearchPhase::Failed {
        message: "失败".into(),
        has_more: false,
        retryable: true,
    };
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert_eq!(view.sidebar_search_snippet(&address("a")), None);
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的本地会话名称")
    );
    result.input.revision += 1;
    result.input.phase = SidebarSearchPhase::Complete { has_more: true };
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        !view
            .tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的会话"),
        "empty first page with more data is not authoritative no-results"
    );
    result.input.revision += 1;
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的会话")
    );
    result.input.revision += 1;
    result.input.key = None;
    result.query = "  ".into();
    result.input.phase = SidebarSearchPhase::Idle;
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert_eq!(sidebar.displayed_input().unwrap().rows, inventory().rows);
    assert_ne!(sidebar.row_id(&address("c")), Some(old_unmatched_row));
    assert_eq!(
        sidebar.displayed_input().unwrap().selected,
        Some(address("a"))
    );
    assert_eq!(view.selected(), 0);
    assert!(view.editor().input.text.is_empty());
    runtime.stop().await.unwrap();
}

#[test]
fn help_is_an_atomic_query_scoped_part_of_the_row_publication() {
    use tekes_ui_controls::composition::*;
    let source = inventory();
    let publication = search();
    let projected = source
        .project_search("fixture", &publication, None, 11)
        .unwrap();
    let mut sidebar = Sidebar::new(994, SidebarLabels::default());
    sidebar
        .replace_search_source("search-reader", &publication.input)
        .unwrap();
    sidebar.update(&projected.input).unwrap();
    assert_eq!(sidebar.row_help(&address("a")), Some("a 的正文摘要"));
    let original = sidebar.displayed_input().unwrap().clone();
    let mut conflicting = original.clone();
    conflicting
        .search_help
        .as_mut()
        .unwrap()
        .snippets
        .insert(address("a"), "same revision must not retarget help".into());
    assert!(sidebar.update(&conflicting).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut foreign = original.clone();
    foreign.revision += 1;
    foreign
        .search_help
        .as_mut()
        .unwrap()
        .snippets
        .insert(address("not-displayed"), "unknown owner".into());
    assert!(sidebar.update(&foreign).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut pending_with_help = original.clone();
    pending_with_help.revision += 1;
    pending_with_help
        .search_help
        .as_mut()
        .unwrap()
        .results_ready = false;
    // Known Complete snippets remain usable while other provider hits await inventory.
    let mut partial_sidebar = Sidebar::new(995, SidebarLabels::default());
    partial_sidebar
        .replace_search_source("search-reader", &publication.input)
        .unwrap();
    assert!(partial_sidebar.update(&pending_with_help).is_ok());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut failed_ready = original.clone();
    failed_ready.revision += 1;
    failed_ready.search_help.as_mut().unwrap().source.phase = SidebarSearchPhase::Failed {
        message: "failure cannot authorize snippets".into(),
        has_more: false,
        retryable: true,
    };
    assert!(sidebar.update(&failed_ready).is_err());
    assert_eq!(sidebar.displayed_input(), Some(&original));
    let mut next = publication.input.clone();
    next.revision += 1;
    next.key.as_mut().unwrap().generation += 1;
    next.key.as_mut().unwrap().query = "新查询".into();
    sidebar.update_search_from("search-reader", &next).unwrap();
    assert_eq!(
        sidebar.row_help(&address("a")),
        None,
        "new footer revokes old help even before replacement rows arrive"
    );
    let root = sidebar.id();
    let mut host = sidebar.into_host().unwrap();
    let row = host
        .get(root)
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("a"))
        .unwrap();
    host.accessibility_action(row, None, AccessibilityAction::Press);
    let actions = host.drain_actions();
    assert_eq!(
        host.get(root)
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .take_intents(&actions),
        vec![SidebarIntent::Select {
            address: address("a")
        }]
    );
}

fn pinned_row(id: &str) -> SidebarRow {
    let mut value = row(id);
    value.folder_path = Some(format!("/original/{id}"));
    value.pin = Some(ThreadPinPresentation { pinned: true });
    value
}
#[test]
fn provider_membership_filters_pins_only_after_complete_and_joinable_results() {
    let mut source = inventory();
    source.rows[1] = pinned_row("c");
    let mut result = search();
    let projected = source.project_search("fixture", &result, None, 1).unwrap();
    assert_eq!(projected.input.rows, vec![row("a"), row("b")]);
    assert!(!projected.snippets.contains_key(&address("c")));
    result.input.phase = SidebarSearchPhase::Searching;
    let waiting = source.project_search("fixture", &result, None, 2).unwrap();
    assert!(waiting.input.loaded);
    assert!(waiting.input.rows.is_empty());
    assert!(waiting.snippets.is_empty());
    assert!(!waiting.input.search_help.unwrap().results_ready);
    result.input.phase = SidebarSearchPhase::Failed {
        message: "provider unavailable".into(),
        has_more: false,
        retryable: true,
    };
    result.hits.clear();
    let failed = source.project_search("fixture", &result, None, 3).unwrap();
    assert!(failed.input.rows.is_empty());
    assert!(!failed.input.search_help.unwrap().results_ready);
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    result.hits = search().hits;
    result.hits[0].address = address("unknown");
    result.hits[0].hit.session_id = "unknown".into();
    let unresolved = source.project_search("fixture", &result, None, 4).unwrap();
    assert_eq!(unresolved.unresolved, vec![address("unknown")]);
    assert_eq!(unresolved.input.rows, vec![row("a")]);
    assert_eq!(
        unresolved.snippets.get(&address("a")).map(String::as_str),
        Some("a 的正文摘要")
    );
    assert!(unresolved.input.loaded);
    assert!(!unresolved.input.search_help.unwrap().results_ready);
    result.hits = search().hits;
    let complete = source.project_search("fixture", &result, None, 5).unwrap();
    assert_eq!(complete.input.rows, vec![row("a"), row("b")]);
    assert!(complete.input.loaded);
    source.loaded = false;
    assert!(
        !source
            .project_search("fixture", &result, None, 6)
            .unwrap()
            .input
            .loaded
    );
}
#[tokio::test]
async fn query_hides_unmatched_pin_without_changing_pin_preference_or_composer_state() {
    use tekes_agent_ui_bindings::RunBinding;
    use tekes_agent_ui_demo::component_demo::ComponentDemo;
    use tekes_ui_controls::composition::*;
    let (runtime, _) = tekes_agent_ui_demo::fixture::setup().await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let mut view = ComponentDemo::new([binding]).unwrap();
    let mut source = inventory();
    source.rows[1] = pinned_row("c");
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
    view.tree.focus(Some(view.ids().editor), false);
    assert!(
        view.event(&ContentEvent::Preedit {
            text: "中文组字".into(),
            selection: 1..3,
            clauses: vec![],
            replacement: None
        })
        .unwrap()
        .is_empty()
    );
    let editor = format!(
        "{:?}",
        (
            &view.editor().input.text,
            view.editor().input.selection(),
            &view.editor().input.marked,
            view.editor().input.external_baseline()
        )
    );
    let focus = view.tree.focused();
    let old_pin_id = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap()
        .row_id(&address("c"))
        .unwrap();
    let mut result = search();
    view.bind_sidebar_search_publication("search-reader", &result)
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert!(sidebar.row_id(&address("c")).is_none());
    assert_eq!(
        sidebar.displayed_input().unwrap().selected,
        Some(address("a"))
    );
    assert!(source.rows.iter().any(|row| {
        row.address == address("c") && row.pin.as_ref().is_some_and(|pin| pin.pinned)
    }));
    view.tree
        .accessibility_action(old_pin_id, None, AccessibilityAction::Press);
    assert!(view.drain_commands().unwrap().is_empty());

    // A provider failure is diagnostic, not an authoritative empty result.
    result.input.revision += 1;
    result.input.key.as_mut().unwrap().generation += 1;
    result.query = "unavailable query".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.input.phase = SidebarSearchPhase::Failed {
        message: "query unavailable".into(),
        has_more: false,
        retryable: true,
    };
    result.hits.clear();
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(
        view.tree
            .semantics()
            .iter()
            .any(|node| node.semantics.name == "没有匹配的本地会话名称")
    );

    // A current provider hit cannot be mounted until the original public
    // inventory supplies it. The later inventory publication restores the row
    // without another query or changing the Composer's preedit/focus.
    result.input.revision += 1;
    result.input.key.as_mut().unwrap().generation += 1;
    result.query = "late query".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    result.hits = vec![SidebarSearchMatch {
        address: address("late"),
        hit: tekes_agent_ui_bindings::SessionSearchHit {
            session_id: "late".into(),
            snippet: "late match".into(),
            extra: Default::default(),
        },
    }];
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    assert!(
        view.tree
            .get(view.sidebar_id())
            .unwrap()
            .component::<Sidebar>()
            .unwrap()
            .displayed_input()
            .unwrap()
            .rows
            .is_empty()
    );
    source.revision += 1;
    source.rows.push(pinned_row("late"));
    view.set_sidebar_source(Arc::new(source.clone())).unwrap();
    view.sync().unwrap();
    let resolved = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    assert_eq!(
        resolved.displayed_input().unwrap().rows,
        vec![pinned_row("late")]
    );
    assert!(resolved.row_id(&address("late")).is_some());
    assert!(resolved.row_id(&address("c")).is_none());
    assert_eq!(
        resolved.search_snippet(&address("late")),
        Some("late match")
    );

    // Clearing the provider query restores the same inventory and pin setting.
    result.input.revision += 1;
    result.input.key = None;
    result.query = "  ".into();
    result.input.phase = SidebarSearchPhase::Idle;
    result.hits.clear();
    result.unresolved.clear();
    view.update_sidebar_search_publication("search-reader", &result)
        .unwrap();
    let sidebar = view
        .tree
        .get(view.sidebar_id())
        .unwrap()
        .component::<Sidebar>()
        .unwrap();
    let restored_pin = sidebar.row_id(&address("c")).unwrap();
    assert_ne!(restored_pin, old_pin_id);
    assert_eq!(sidebar.row_help(&address("c")), Some("/original/c"));
    view.tree
        .accessibility_action(old_pin_id, None, AccessibilityAction::Press);
    assert!(view.drain_commands().unwrap().is_empty());
    assert_eq!(
        format!(
            "{:?}",
            (
                &view.editor().input.text,
                view.editor().input.selection(),
                &view.editor().input.marked,
                view.editor().input.external_baseline()
            )
        ),
        editor
    );
    assert_eq!(view.tree.focused(), focus);
    assert_eq!(view.selected(), 0);
    assert!(view.binding().current_run().unwrap().submissions.is_empty());
    runtime.stop().await.unwrap();
}

#[test]
fn local_name_matches_survive_provider_failure_and_union_with_known_partial_hits() {
    let mut source = inventory();
    source.rows[1].title = "Café 中文 design".into();
    let mut result = search();
    result.query = "cafe des".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.input.phase = SidebarSearchPhase::Failed {
        message: "unsupported provider".into(),
        has_more: false,
        retryable: false,
    };
    result.hits.clear();
    let failed = source.project_search("fixture", &result, None, 1).unwrap();
    assert_eq!(failed.input.rows, vec![source.rows[1].clone()]);
    assert!(
        failed
            .input
            .search_help
            .as_ref()
            .unwrap()
            .local_results_ready
    );
    assert!(!failed.input.search_help.as_ref().unwrap().results_ready);
    assert!(failed.snippets.is_empty());
    result.input.phase = SidebarSearchPhase::Complete { has_more: false };
    result.hits = search().hits;
    result.hits[0].address = address("unknown");
    result.hits[0].hit.session_id = "unknown".into();
    let partial = source.project_search("fixture", &result, None, 2).unwrap();
    assert_eq!(
        partial.input.rows,
        vec![source.rows[0].clone(), source.rows[1].clone()]
    );
    assert_eq!(partial.unresolved, vec![address("unknown")]);
    assert_eq!(partial.snippets.len(), 1);
}

#[test]
fn persistent_name_projector_rejects_same_revision_rewrites_and_old_source_reuse() {
    use tekes_agent_presentation::SidebarObserverSourceKey;
    use tekes_agent_ui_bindings::SidebarSearchProjector;
    let owner = Arc::new(());
    let key = SidebarObserverSourceKey::from_owner(&owner);
    let mut projector = SidebarSearchProjector::default();
    let mut source = inventory();
    let mut result = search();
    result.query = "unrelated".into();
    result.input.key.as_mut().unwrap().query = result.query.clone();
    result.hits.clear();
    let display = source.input(None);
    assert_eq!(
        projector
            .project_for_display(&key, &source, "fixture", &result, &display)
            .unwrap()
            .input
            .rows
            .len(),
        3
    );
    source.rows[0].title = "Renamed".into();
    assert!(
        projector
            .project_for_display(&key, &source, "fixture", &result, &display)
            .is_err()
    );
    source.revision += 1;
    assert_eq!(
        projector
            .project_for_display(&key, &source, "fixture", &result, &display)
            .unwrap()
            .input
            .rows
            .len(),
        2
    );
    source.revision -= 1;
    assert!(
        projector
            .project_for_display(&key, &source, "fixture", &result, &display)
            .is_err()
    );
    let replacement = SidebarObserverSourceKey::from_owner(&Arc::new(()));
    assert_eq!(
        projector
            .project_for_display(&replacement, &source, "fixture", &result, &display)
            .unwrap()
            .input
            .rows
            .len(),
        2
    );
}
