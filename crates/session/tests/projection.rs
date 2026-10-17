use serde_json::{Value, json};
use tekes_agent_contract::*;
use tekes_agent_session::*;
fn event(seq: i64, kind: &str) -> Value {
    json!({"event":{"seq":seq,"type":kind,"time":1,"data":{"turn":1,"step":1}}})
}
fn snapshot(cut: i64, entries: Vec<Value>, more: bool, revision: Option<&str>) -> Value {
    json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"g","throughSequence":cut,"windowLimit":50,"entries":entries,"hasMoreBefore":more,"historyRevision":revision}})
}
fn inventory() -> Value {
    json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"s","updatedAt":1,"running":false,"blank":false}]}})
}
fn store() -> MemoryProjectionStore {
    MemoryProjectionStore::new("kernel")
}
#[test]
fn baseline_first_and_generation_are_enforced() {
    let mut s = store();
    assert!(
        s.apply(
            "inventory",
            json!({"type":"remove","generation":"g","sessionId":"s"})
        )
        .is_err()
    );
    s.apply("inventory", inventory()).unwrap();
    assert!(s.apply("inventory", inventory()).is_err());
    assert!(
        s.apply(
            "inventory",
            json!({"type":"remove","generation":"other","sessionId":"s"})
        )
        .is_err()
    );
    assert!(s.sessions["s"].summary.is_some());
}
#[test]
fn frozen_cut_survives_live_append_and_pagination() {
    let mut s = store();
    s.apply(
        "journal:s",
        snapshot(
            3,
            vec![event(2, "turn/start"), event(3, "user/message")],
            true,
            None,
        ),
    )
    .unwrap();
    s.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":event(4,"assistant/message")})).unwrap();
    assert_eq!(s.sessions["s"].history_through_sequence, 3);
    let request = HistoryRequest {
        session_id: "s".into(),
        through_sequence: 3,
        before_sequence: Some(2),
        max_messages: 50,
    };
    let page:HistoryPage=serde_json::from_value(json!({"sessionId":"s","throughSequence":3,"entries":[event(0,"turn/start"),event(1,"user/message")],"hasMoreBefore":false})).unwrap();
    assert!(
        s.sessions
            .get_mut("s")
            .unwrap()
            .apply_page(page, &request)
            .unwrap()
    );
    assert_eq!(s.sessions["s"].tail_sequence, 4);
    assert_eq!(s.sessions["s"].entries.len(), 5);
    assert!(!s.sessions["s"].has_more_before);
}
#[test]
fn page_rejects_wrong_cut_and_noncontiguous_tail_without_mutation() {
    let mut s = store();
    s.apply(
        "journal:s",
        snapshot(
            5,
            vec![event(4, "turn/start"), event(5, "user/message")],
            true,
            None,
        ),
    )
    .unwrap();
    let request = HistoryRequest {
        session_id: "s".into(),
        through_sequence: 5,
        before_sequence: Some(4),
        max_messages: 50,
    };
    for (cut, seq) in [(6, 3), (5, 2)] {
        let page:HistoryPage=serde_json::from_value(json!({"sessionId":"s","throughSequence":cut,"entries":[event(seq,"user/message")],"hasMoreBefore":true})).unwrap();
        assert!(
            s.sessions
                .get_mut("s")
                .unwrap()
                .apply_page(page, &request)
                .is_err()
        );
        assert_eq!(s.sessions["s"].entries.len(), 2);
    }
}
#[test]
fn gaps_and_conflicts_do_not_advance_tail() {
    let mut s = store();
    s.apply(
        "journal:s",
        snapshot(0, vec![event(0, "turn/start")], false, None),
    )
    .unwrap();
    let frame =
        json!({"type":"event","generation":"g","sessionId":"s","entry":event(0,"turn/start")});
    s.apply("journal:s", frame).unwrap();
    for e in [event(0, "turn/end"), event(2, "turn/end")] {
        assert!(
            s.apply(
                "journal:s",
                json!({"type":"event","generation":"g","sessionId":"s","entry":e})
            )
            .is_err()
        );
    }
    assert_eq!(s.sessions["s"].tail_sequence, 0);
}
#[test]
fn rebaseline_retains_earlier_coverage_but_revision_discards_it() {
    let mut s = store();
    s.apply(
        "journal:s",
        snapshot(
            2,
            vec![
                event(0, "turn/start"),
                event(1, "user/message"),
                event(2, "assistant/message"),
            ],
            false,
            Some("r1"),
        ),
    )
    .unwrap();
    s.reset_journal("s");
    s.apply(
        "journal:s",
        snapshot(
            3,
            vec![event(2, "assistant/message"), event(3, "turn/end")],
            true,
            Some("r1"),
        ),
    )
    .unwrap();
    assert_eq!(s.sessions["s"].entries.len(), 4);
    assert!(!s.sessions["s"].has_more_before);
    s.reset_journal("s");
    s.apply(
        "journal:s",
        snapshot(3, vec![event(3, "turn/end")], false, Some("r2")),
    )
    .unwrap();
    assert_eq!(s.sessions["s"].entries.len(), 1);
}
#[test]
fn transient_does_not_move_durable_cursor_and_is_replaced() {
    let mut s = store();
    s.apply(
        "journal:s",
        snapshot(0, vec![event(0, "turn/start")], false, None),
    )
    .unwrap();
    s.apply("journal:s",json!({"type":"transient","generation":"g","sessionId":"s","event":{"type":"assistant/chunk","seq":99,"time":1,"data":{"turn":1,"step":1,"chunk":{"type":"text-delta","delta":"hello"}}}})).unwrap();
    assert_eq!(s.sessions["s"].tail_sequence, 0);
    assert_eq!(s.sessions["s"].previews["1:1"].text, "hello");
    s.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"s","entry":event(1,"assistant/message")})).unwrap();
    assert!(s.sessions["s"].previews.is_empty());
}
#[test]
fn projections_merge_per_key_and_null_clears() {
    let mut s = store();
    s.apply("control",json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[{"sessionId":"s","projections":{"asOfSeq":10,"values":{"title":"new"}}}]}})).unwrap();
    s.apply("control",json!({"type":"projection","generation":"g","state":{"sessionId":"s","projections":{"asOfSeq":5,"values":{"sessionTitle":{"title":"old"},"contextUsage":null}}}})).unwrap();
    assert_eq!(s.sessions["s"].projections["title"].value, "new");
    assert!(s.sessions["s"].projections["contextUsage"].value.is_null());
}
#[test]
fn actionables_revision_and_disconnect_remove_authority() {
    let mut s = store();
    s.apply("actionables", json!({"type":"reset","generation":"g"}))
        .unwrap();
    s.apply("actionables",json!({"type":"upsert","generation":"g","actionable":{"id":"a","sessionId":"s","revision":2.0,"kind":"custom","payload":null}})).unwrap();
    s.apply(
        "actionables",
        json!({"type":"resolved","generation":"g","actionableId":"a","revision":1}),
    )
    .unwrap();
    assert_eq!(s.actionables.len(), 1);
    s.disconnect();
    assert!(s.actionables.is_empty());
    assert!(!s.ready());
}
#[test]
fn control_baseline_is_atomic_and_replaces_empty_queues() {
    let mut s = store();
    let baseline = json!({"type":"baseline","generation":"g","snapshot":{"queues":[{"sessionId":"s","items":["a"]}],"jobs":[],"projections":[]}});
    s.apply("control", baseline).unwrap();
    s.disconnect();
    assert!(s.apply("control",json!({"type":"baseline","generation":"g2","snapshot":{"queues":[{"sessionId":"s","items":[]},{"sessionId":"s","items":[]}],"jobs":[],"projections":[]}})).is_err());
    assert_eq!(s.sessions["s"].queue.len(), 1);
    s.apply("control",json!({"type":"baseline","generation":"g2","snapshot":{"queues":[],"jobs":[],"projections":[]}})).unwrap();
    assert!(s.sessions["s"].queue.is_empty());
}
#[test]
fn endpoint_identity_and_wrong_stream_are_rejected() {
    let mut s = store();
    s.apply("inventory", inventory()).unwrap();
    assert!(
        s.session(&SessionAddress {
            endpoint_id: "other".into(),
            session_id: "s".into()
        })
        .is_none()
    );
    assert!(
        s.apply("journal:s", snapshot(0, vec![], false, None))
            .is_ok()
    );
    assert!(s.apply("journal:s",json!({"type":"event","generation":"g","sessionId":"other","entry":event(1,"turn/start")})).is_err());
    assert!(s.apply("control", inventory()).is_err());
}

#[test]
fn native_conversation_has_its_own_identity_and_never_advances_journal_cursor() {
    let mut store = store();
    let snapshot = json!({"sessionId":"s","generation":"g","leafId":"a","entries":[{"id":"a","type":"message","message":{"role":"user"}}],"hasMoreBefore":false,"isStreaming":false,"state":{}});
    store
        .apply(
            "journal:s",
            json!({"type":"conversation","generation":"g","snapshot":snapshot}),
        )
        .unwrap();
    assert_eq!(store.sessions["s"].tail_sequence, -1);
    assert!(store.sessions["s"].entries.is_empty());
    for index in 0..300 {
        store.apply("journal:s",json!({"type":"native-event","generation":"g","sessionId":"s","event":{"index":index}})).unwrap();
    }
    assert_eq!(store.sessions["s"].native_events.len(), 256);
    let mut bad = snapshot;
    bad["generation"] = json!("other");
    assert!(
        store
            .apply(
                "journal:s",
                json!({"type":"conversation-update","generation":"g","snapshot":bad})
            )
            .is_err()
    );
    store.disconnect();
    assert!(store.sessions["s"].native_events.is_empty());
    assert!(!store.sessions["s"].journal_ready);
}

#[test]
fn workspace_replacement_and_events_preserve_generation_and_authority() {
    let mut store = store();
    store.apply("workspace",json!({"type":"baseline","generation":"g","snapshot":{"items":[{"workspaceId":"w","path":"/work","title":"old"}],"archivedSessionIds":[],"pinnedSessionIds":[]}})).unwrap();
    store.apply("workspace",json!({"type":"replace","generation":"g","snapshot":{"items":[{"workspaceId":"w","path":"/new","title":"new"}],"archivedSessionIds":["s"],"pinnedSessionIds":["s"]}})).unwrap();
    assert_eq!(store.workspaces[0]["path"], "/new");
    assert_eq!(store.archived_session_ids, ["s"]);
    assert!(
        store
            .apply(
                "workspace",
                json!({"type":"reorder","generation":"g","workspaceIds":["absent"]})
            )
            .is_err()
    );
    assert_eq!(store.workspaces[0]["path"], "/new");
    store
        .apply(
            "events",
            json!({"type":"baseline","generation":"g","snapshot":{}}),
        )
        .unwrap();
    for index in 0..300 {
        store
            .apply(
                "events",
                json!({"type":"event","generation":"g","event":{"index":index,"type":"waterfall"}}),
            )
            .unwrap();
    }
    assert_eq!(store.endpoint_events.len(), 256);
    assert!(store.actionables.is_empty());
    store.disconnect();
    assert!(store.endpoint_events.is_empty());
    assert!(!store.workspace_ready);
}
