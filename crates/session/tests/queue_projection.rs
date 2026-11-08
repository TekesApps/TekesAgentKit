use serde_json::{Value, json};
use tekes_agent_contract::*;
use tekes_agent_session::*;

const KEY: &str = SEQUENCED_QUEUE_PROJECTION_KEY;

fn authorized_store() -> MemoryProjectionStore {
    let mut value = serde_json::to_value(MemoryProjectionStore::new("adapter")).unwrap();
    value["sequenced_queue_projection_key"] = json!(KEY);
    serde_json::from_value(value).unwrap()
}

fn journal_projection(store: &mut MemoryProjectionStore, seq: i64, value: Value) {
    if !store
        .sessions
        .get("s")
        .is_some_and(|session| session.journal_ready)
    {
        store
            .apply(
                "journal:s",
                json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"j1","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}}),
            )
            .unwrap();
    }
    store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j1","sessionId":"s","projections":{"asOfSeq":seq,"values":{(KEY):value}}}),
        )
        .unwrap();
}

fn control_projection(store: &mut MemoryProjectionStore, seq: i64, value: Value) {
    store
        .apply(
            "control",
            json!({"type":"projection","generation":"c1","state":{"sessionId":"s","projections":{"asOfSeq":seq,"values":{(KEY):value}}}}),
        )
        .unwrap();
}

fn control_baseline(store: &mut MemoryProjectionStore, queue: Value, seq: i64, value: Value) {
    store
        .apply(
            "control",
            json!({"type":"baseline","generation":"c1","snapshot":{"queues":[{"sessionId":"s","items":queue}],"jobs":[],"projections":[{"sessionId":"s","projections":{"asOfSeq":seq,"values":{(KEY):value}}}]}}),
        )
        .unwrap();
}

#[test]
fn seq20_journal_survives_seq19_control_baseline_and_delta() {
    let mut store = authorized_store();
    journal_projection(&mut store, 20, json!({"items":["j20"]}));
    control_baseline(&mut store, json!(["raw19"]), 19, json!({"items":["c19"]}));
    control_projection(&mut store, 19, json!({"items":["delta19"]}));
    assert_eq!(store.sessions["s"].queue, vec![json!("j20")]);
}

#[test]
fn seq20_control_survives_late_seq19_journal_and_missing_key() {
    let mut store = authorized_store();
    control_baseline(&mut store, json!(["raw"]), 20, json!({"items":["c20"]}));
    journal_projection(&mut store, 19, json!({"items":["j19"]}));
    // A newer unrelated journal projection does not imply an empty queue.
    store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j1","sessionId":"s","projections":{"asOfSeq":21,"values":{"title":"new"}}}),
        )
        .unwrap();
    assert_eq!(store.sessions["s"].queue, vec![json!("c20")]);
}

#[test]
fn explicit_empty_clears_but_missing_key_and_old_sequence_do_not() {
    let mut store = authorized_store();
    control_baseline(&mut store, json!([]), 4, json!({"items":["first"]}));
    control_projection(&mut store, 5, json!({"items":[]}));
    assert!(store.sessions["s"].queue.is_empty());
    journal_projection(&mut store, 4, json!({"items":["stale"]}));
    store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j1","sessionId":"s","projections":{"asOfSeq":6,"values":{"title":"without-queue"}}}),
        )
        .unwrap();
    assert!(store.sessions["s"].queue.is_empty());
}

#[test]
fn same_sequence_is_idempotent_and_conflicts_are_atomic() {
    let mut store = authorized_store();
    control_baseline(&mut store, json!([]), 7, json!({"items":["stable"]}));
    control_projection(&mut store, 7, json!({"items":["stable"]}));
    let before = store.clone();
    assert!(store
        .apply(
            "control",
            json!({"type":"projection","generation":"c1","state":{"sessionId":"s","projections":{"asOfSeq":7,"values":{(KEY):{"items":["conflict"]},"also":"partial"}}}}),
        )
        .is_err());
    assert_eq!(store.sessions["s"].queue, before.sessions["s"].queue);
    assert_eq!(
        store.sessions["s"].projections,
        before.sessions["s"].projections
    );
}

#[test]
fn opaque_projection_does_not_authorize_queue_and_legacy_channels_remain() {
    let mut store = MemoryProjectionStore::new("native-or-pi");
    store.apply("journal:s", json!({"type":"snapshot","snapshot":{"sessionId":"s","generation":"j1","throughSequence":-1,"windowLimit":50,"entries":[],"hasMoreBefore":false}})).unwrap();
    store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j1","sessionId":"s","projections":{"asOfSeq":3,"values":{(KEY):{"items":["opaque"]}}}}),
        )
        .unwrap();
    assert!(store.sessions["s"].queue.is_empty());
    // Opaque values retain the existing generic equal-sequence replacement policy.
    store.apply("journal:s", json!({"type":"projection-block","generation":"j1","sessionId":"s","projections":{"asOfSeq":3,"values":{(KEY):{"items":["still-opaque"]}}}})).unwrap();
    assert_eq!(
        store.sessions["s"].projections[KEY].value["items"],
        json!(["still-opaque"])
    );
    assert!(store.sessions["s"].queue.is_empty());
    store
        .apply(
            "control",
            json!({"type":"baseline","generation":"c1","snapshot":{"queues":[{"sessionId":"s","items":["legacy"]}],"jobs":[],"projections":[]}}),
        )
        .unwrap();
    assert_eq!(store.sessions["s"].queue, vec![json!("legacy")]);
}

#[test]
fn projection_block_rejects_old_generation_and_cross_session_identity() {
    let mut store = authorized_store();
    journal_projection(&mut store, 3, json!({"items":["ok"]}));
    assert!(store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j2","sessionId":"s","projections":{"asOfSeq":4,"values":{(KEY):{"items":[]}}}}),
        )
        .is_err());
    assert!(store
        .apply(
            "journal:s",
            json!({"type":"projection-block","generation":"j1","sessionId":"other","projections":{"asOfSeq":4,"values":{(KEY):{"items":[]}}}}),
        )
        .is_err());
    assert_eq!(store.sessions["s"].queue, vec![json!("ok")]);
    assert!(!store.sessions.contains_key("other"));
}
