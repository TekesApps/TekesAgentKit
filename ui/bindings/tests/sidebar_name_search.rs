use std::sync::Arc;

use tekes_agent_contract::SessionAddress;
use tekes_agent_presentation::SidebarObserverSourceKey;
use tekes_agent_ui_bindings::{SidebarNameDocument, SidebarNameIndex};

fn source(name: &str) -> SidebarObserverSourceKey {
    SidebarObserverSourceKey::from_owner(&Arc::new(name.to_owned()))
}

fn document(endpoint_id: &str, session_id: &str, title: &str) -> SidebarNameDocument {
    SidebarNameDocument {
        address: SessionAddress {
            endpoint_id: endpoint_id.to_owned(),
            session_id: session_id.to_owned(),
        },
        title: title.to_owned(),
    }
}

fn addresses(matches: Vec<SessionAddress>) -> Vec<(String, String)> {
    matches
        .into_iter()
        .map(|address| (address.endpoint_id, address.session_id))
        .collect()
}

#[test]
fn uses_real_fts5_with_accent_prefix_and_and_matching() {
    let mut index = SidebarNameIndex::new().expect("bundled SQLite must expose FTS5");
    let source = source("host");
    index
        .replace_snapshot(
            &source,
            1,
            &[
                document("native", "one", "café espresso journal"),
                document("native", "two", "café elsewhere"),
                document("native", "three", "scaffolding notes"),
            ],
        )
        .unwrap();

    assert_eq!(
        addresses(index.matches("cafe esp").unwrap()),
        [("native".into(), "one".into())]
    );
    assert_eq!(
        addresses(index.matches("aff").unwrap()),
        Vec::<(String, String)>::new()
    );
    assert_eq!(
        addresses(index.matches("espresso cafe").unwrap()),
        [("native".into(), "one".into())]
    );
}

#[test]
fn quotes_and_operator_words_are_literal_terms() {
    let mut index = SidebarNameIndex::new().unwrap();
    let source = source("host");
    index
        .replace_snapshot(
            &source,
            1,
            &[
                document("native", "literal", "alpha OR beta"),
                document("native", "without-operator", "alpha beta"),
                document("native", "left-only", "alpha"),
                document("native", "right-only", "beta"),
                document("native", "quoted", "say hello now"),
            ],
        )
        .unwrap();

    assert_eq!(
        addresses(index.matches("alpha OR beta").unwrap()),
        [("native".into(), "literal".into())]
    );
    assert_eq!(
        addresses(index.matches("\"hello\"").unwrap()),
        [("native".into(), "quoted".into())]
    );
    assert_eq!(
        addresses(index.matches("alpha OR").unwrap()),
        [("native".into(), "literal".into())]
    );
}

#[test]
fn matches_contiguous_chinese_and_english_prefixes() {
    let mut index = SidebarNameIndex::new().unwrap();
    let source = source("host");
    index
        .replace_snapshot(
            &source,
            1,
            &[document("pi", "session", "中文AgentKit历史记录")],
        )
        .unwrap();

    assert_eq!(
        addresses(index.matches("中文Agent").unwrap()),
        [("pi".into(), "session".into())]
    );
}

#[test]
fn empty_query_and_missing_snapshot_have_distinct_results() {
    let index = SidebarNameIndex::new().unwrap();
    assert!(index.matches("").is_err());

    let mut index = index;
    index
        .replace_snapshot(&source("host"), 1, &[document("native", "one", "title")])
        .unwrap();
    assert!(index.matches(" \t\n").unwrap().is_empty());
}

#[test]
fn snapshots_replace_rename_delete_and_keep_endpoint_identity() {
    let mut index = SidebarNameIndex::new().unwrap();
    let source = source("host");
    index
        .replace_snapshot(
            &source,
            1,
            &[
                document("native", "same-id", "old title"),
                document("pi", "same-id", "old title"),
            ],
        )
        .unwrap();
    assert_eq!(
        addresses(index.matches("old").unwrap()),
        [
            ("native".into(), "same-id".into()),
            ("pi".into(), "same-id".into())
        ]
    );

    index
        .replace_snapshot(
            &source,
            2,
            &[
                document("native", "same-id", "renamed entry"),
                document("dsh", "new", "new title"),
            ],
        )
        .unwrap();
    assert!(index.matches("old").unwrap().is_empty());
    assert_eq!(
        addresses(index.matches("renam").unwrap()),
        [("native".into(), "same-id".into())]
    );
    assert_eq!(
        addresses(index.matches("new").unwrap()),
        [("dsh".into(), "new".into())]
    );
}

#[test]
fn invalid_replacements_are_atomic_and_source_change_starts_a_new_sequence() {
    let mut index = SidebarNameIndex::new().unwrap();
    let first_source = source("first host");
    index
        .replace_snapshot(
            &first_source,
            4,
            &[document("native", "kept", "stable title")],
        )
        .unwrap();

    assert!(
        index
            .replace_snapshot(
                &first_source,
                3,
                &[document("native", "older", "old title")]
            )
            .is_err()
    );
    assert!(
        index
            .replace_snapshot(
                &first_source,
                4,
                &[document("native", "kept", "conflicting title")]
            )
            .is_err()
    );
    assert!(
        index
            .replace_snapshot(
                &first_source,
                5,
                &[
                    document("native", "duplicate", "first"),
                    document("native", "duplicate", "second"),
                ],
            )
            .is_err()
    );
    assert!(
        index
            .replace_snapshot(&first_source, 5, &[document("", "incomplete", "bad")])
            .is_err()
    );
    assert_eq!(
        addresses(index.matches("stable").unwrap()),
        [("native".into(), "kept".into())]
    );

    let second_source = source("replacement host");
    index
        .replace_snapshot(
            &second_source,
            1,
            &[document("pi", "fresh", "replacement title")],
        )
        .unwrap();
    assert!(index.matches("stable").unwrap().is_empty());
    assert_eq!(
        addresses(index.matches("replacement").unwrap()),
        [("pi".into(), "fresh".into())]
    );
}

#[test]
fn same_revision_is_order_insensitive_and_identical_higher_revision_advances() {
    let mut index = SidebarNameIndex::new().unwrap();
    let source = source("host");
    let first = document("native", "one", "first title");
    let second = document("pi", "two", "second title");
    index
        .replace_snapshot(&source, 7, &[first.clone(), second.clone()])
        .unwrap();
    index
        .replace_snapshot(&source, 7, &[second.clone(), first.clone()])
        .unwrap();
    index
        .replace_snapshot(&source, 8, &[second, first])
        .unwrap();
    assert_eq!(
        addresses(index.matches("second").unwrap()),
        [("pi".into(), "two".into())]
    );
    assert!(index.replace_snapshot(&source, 0, &[]).is_err());
}
