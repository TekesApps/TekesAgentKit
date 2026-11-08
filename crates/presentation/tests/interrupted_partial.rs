use serde_json::{Value, json};
use tekes_agent_presentation::{RunScope, TranscriptItemKind, TranscriptSnapshot};
use tekes_agent_session::MemoryProjectionStore;

fn event(seq: i64, kind: &str, data: Value) -> Value {
    json!({"event":{"seq":seq,"time":seq+1,"type":kind,"data":data}})
}
fn open(store: &mut MemoryProjectionStore, id: &str) {
    store.apply(&format!("journal:{id}"), json!({"type":"snapshot","snapshot":{
        "sessionId":id,"generation":"g","throughSequence":2,"windowLimit":50,
        "entries":[event(0,"turn/start",json!({"turn":1})),
            event(1,"user/message",json!({"id":"input-2","role":"user","content":[{"type":"text","text":"input"}]})),
            event(2,"step/start",json!({"turn":1,"step":0}))],"hasMoreBefore":false
    }})).unwrap();
}
fn live(store: &mut MemoryProjectionStore, id: &str, seq: i64, kind: &str, data: Value) {
    store
        .apply(
            &format!("journal:{id}"),
            json!({"type":"event","generation":"g","sessionId":id,"entry":event(seq,kind,data)}),
        )
        .unwrap();
}
fn chunk(store: &mut MemoryProjectionStore, id: &str, text: &str) {
    store.apply(&format!("journal:{id}"), json!({"type":"transient","generation":"g","sessionId":id,
        "event":{"seq":99,"time":3,"type":"assistant/chunk","data":{"turn":1,"step":0,"chunk":{"type":"text-delta","delta":text}}}})).unwrap();
}
fn read(store: &MemoryProjectionStore, id: &str) -> TranscriptSnapshot {
    TranscriptSnapshot::read(&RunScope::main("kernel", id), &store.sessions[id]).unwrap()
}
fn partial(store: &MemoryProjectionStore, id: &str) {
    let view = read(store, id);
    let rows: Vec<_> = view
        .items
        .iter()
        .filter(|i| i.kind == TranscriptItemKind::Assistant)
        .collect();
    assert_eq!(rows.len(), 1, "observed partial must survive stop closure");
    assert_eq!(rows[0].text, "already observed partial");
    assert!(rows[0].is_provisional);
    assert!(!rows[0].is_final);
    assert!(rows[0].source_sequence.is_none());
    assert!(rows[0].source_position.is_none());
    assert!(rows[0].record_id.is_none());
}
#[test]
fn interrupted_partial_survives_actual_stop_order_without_durable_promotion() {
    let mut store = MemoryProjectionStore::new("kernel");
    open(&mut store, "s");
    chunk(&mut store, "s", "already observed partial");
    // M2 B public shape: error has no step or user_stop code; the prose is not a discriminator.
    live(
        &mut store,
        "s",
        3,
        "response/error",
        json!({"turn":1,"classification":"transport","recoverable":false,"message":"arbitrary localized interruption"}),
    );
    partial(&store, "s");
    live(&mut store, "s", 4, "step/end", json!({"turn":1,"step":0}));
    partial(&store, "s");
    live(
        &mut store,
        "s",
        5,
        "turn/end",
        json!({"turn":1,"reason":"interrupted"}),
    );
    partial(&store, "s");
    assert_eq!(read(&store, "s").turns[0].status, "interrupted");
    chunk(&mut store, "s", "late chunk must not mutate stopped text");
    partial(&store, "s");
    assert_eq!(store.sessions["s"].tail_sequence, 5);
    assert!(
        !store.sessions["s"]
            .entries
            .values()
            .any(|e| e.event.kind == "assistant/message")
    );
    store.reset_journal("s");
    open(&mut store, "s");
    assert!(
        !read(&store, "s")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
}
#[test]
fn committed_message_replaces_partial_once_and_normal_completion_drops_uncommitted_preview() {
    let mut store = MemoryProjectionStore::new("kernel");
    open(&mut store, "s");
    chunk(&mut store, "s", "already observed partial");
    live(
        &mut store,
        "s",
        3,
        "assistant/message",
        json!({"turn":1,"step":0,"message":{"id":"answer","role":"assistant","content":[{"type":"text","text":"durable answer"}]}}),
    );
    live(&mut store, "s", 4, "step/end", json!({"turn":1,"step":0}));
    live(
        &mut store,
        "s",
        5,
        "turn/end",
        json!({"turn":1,"reason":"completed"}),
    );
    let view = read(&store, "s");
    let rows: Vec<_> = view
        .items
        .iter()
        .filter(|i| i.kind == TranscriptItemKind::Assistant)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text, "durable answer");
    assert!(!rows[0].is_provisional);
    open(&mut store, "other");
    chunk(&mut store, "other", "uncommitted");
    live(
        &mut store,
        "other",
        3,
        "step/end",
        json!({"turn":1,"step":0}),
    );
    live(
        &mut store,
        "other",
        4,
        "turn/end",
        json!({"turn":1,"reason":"completed"}),
    );
    assert!(
        !read(&store, "other")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
}
#[test]
fn retained_partial_is_session_scoped_and_retires_on_next_turn() {
    let mut store = MemoryProjectionStore::new("kernel");
    open(&mut store, "s");
    open(&mut store, "other");
    chunk(&mut store, "s", "already observed partial");
    live(&mut store, "s", 3, "step/end", json!({"turn":1,"step":0}));
    live(
        &mut store,
        "s",
        4,
        "turn/end",
        json!({"turn":1,"reason":"interrupted"}),
    );
    partial(&store, "s");
    assert!(
        !read(&store, "other")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
    partial(&store, "s");
    live(&mut store, "s", 5, "turn/start", json!({"turn":2}));
    assert!(
        !read(&store, "s")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
}

#[test]
fn user_stop_aborted_partial_survives_public_stop_order_without_durable_promotion() {
    let mut store = MemoryProjectionStore::new("kernel");
    open(&mut store, "s");
    chunk(&mut store, "s", "already observed partial");
    // Exact Kernel public user_stop: response/error has no private reason; turn/end is aborted.
    live(
        &mut store,
        "s",
        3,
        "response/error",
        json!({"turn":1,"classification":"transport","recoverable":false,"message":"The response was stopped by the user."}),
    );
    partial(&store, "s");
    live(&mut store, "s", 4, "step/end", json!({"turn":1,"step":0}));
    partial(&store, "s");
    live(
        &mut store,
        "s",
        5,
        "turn/end",
        json!({"turn":1,"reason":"aborted"}),
    );
    partial(&store, "s");
    assert_eq!(read(&store, "s").turns[0].status, "aborted");
    let view = read(&store, "s");
    let row = view
        .items
        .iter()
        .find(|i| i.kind == TranscriptItemKind::Assistant)
        .unwrap();
    assert_eq!(row.metadata["localPartial"], true);
    assert!(!store.sessions["s"].run_open);
    store.apply("journal:s", json!({"type":"transient","generation":"g","sessionId":"s",
        "event":{"seq":100,"time":7,"type":"assistant/chunk","data":{"turn":1,"step":0,"chunk":{"type":"usage","usage":{"outputTokens":999}}}}})).unwrap();
    assert!(
        store.sessions["s"]
            .previews
            .values()
            .all(|p| p.output_tokens.is_none())
    );
    chunk(&mut store, "s", "late chunk must not mutate stopped text");
    partial(&store, "s");
    assert_eq!(store.sessions["s"].tail_sequence, 5);
    assert!(
        !store.sessions["s"]
            .entries
            .values()
            .any(|e| e.event.kind == "assistant/message")
    );
    live(&mut store, "s", 6, "turn/start", json!({"turn":2}));
    assert!(
        !read(&store, "s")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
    store.reset_journal("s");
    open(&mut store, "s");
    assert!(
        !read(&store, "s")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
}

#[test]
fn aborted_empty_preview_drops_and_late_usage_does_not_resurrect_it() {
    let mut store = MemoryProjectionStore::new("kernel");
    open(&mut store, "s");
    chunk(&mut store, "s", "");
    live(&mut store, "s", 3, "step/end", json!({"turn":1,"step":0}));
    live(
        &mut store,
        "s",
        4,
        "turn/end",
        json!({"turn":1,"reason":"aborted"}),
    );
    assert!(store.sessions["s"].previews.is_empty());
    store.apply("journal:s", json!({"type":"transient","generation":"g","sessionId":"s",
        "event":{"seq":100,"time":7,"type":"assistant/chunk","data":{"turn":1,"step":0,"chunk":{"type":"usage","usage":{"outputTokens":999}}}}})).unwrap();
    assert!(store.sessions["s"].previews.is_empty());
    assert!(
        !read(&store, "s")
            .items
            .iter()
            .any(|i| i.kind == TranscriptItemKind::Assistant)
    );
    assert_eq!(store.sessions["s"].tail_sequence, 4);
}
