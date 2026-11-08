use serde_json::{Value, json};
use tekes_agent_presentation::*;
use tekes_agent_session::*;
fn scope(id: &str) -> RunScope {
    RunScope::main("endpoint", id)
}
fn authority() -> MemoryProjectionStore {
    let mut store = MemoryProjectionStore::new("endpoint");
    store.connection = ConnectionState::Ready;
    store.inventory_ready = true;
    store.control_ready = true;
    store.actionables_ready = true;
    store.sessions.insert(
        "a".into(),
        SessionProjection {
            journal_ready: true,
            ..Default::default()
        },
    );
    store.sessions.insert(
        "b".into(),
        SessionProjection {
            journal_ready: true,
            ..Default::default()
        },
    );
    store
}
fn project(store: &mut MemoryProjectionStore, value: Value) {
    store
        .sessions
        .get_mut("a")
        .unwrap()
        .projections
        .insert("goal".into(), VersionedValue { sequence: 7, value });
}
#[test]
fn public_flat_and_folded_goal_projections_share_one_read_model() {
    let mut store = authority();
    for value in [
        json!({"id":"goal","revision":3,"phase":"complete","objective":"Owned goal","createdAt":10}),
        json!({"goal":{"id":"goal","revision":3,"phase":"complete","objective":"Owned goal"},"createdAt":10}),
    ] {
        project(&mut store, value);
        let value = GoalFacts::read(&store, &scope("a")).unwrap().unwrap();
        assert_eq!(
            (
                value.scope,
                value.sequence,
                value.id,
                value.revision,
                value.phase
            ),
            (scope("a"), 7, "goal".into(), 3, GoalPhase::Complete)
        );
        assert!(GoalFacts::read(&store, &scope("b")).unwrap().is_none());
    }
    project(
        &mut store,
        json!({"id":"goal","revision":4,"phase":"future-phase","objective":"Owned goal","createdAt":10}),
    );
    let future = GoalFacts::read(&store, &scope("a")).unwrap().unwrap();
    assert_eq!(future.phase.as_str(), "future-phase");
    assert!(!future.phase.is_terminal());
    assert_eq!(
        serde_json::to_value(&future.phase).unwrap(),
        json!("future-phase")
    );
    for value in [Value::Null, json!({"goal":null})] {
        project(&mut store, value);
        assert!(GoalFacts::read(&store, &scope("a")).unwrap().is_none());
    }
    for value in [
        json!({"id":"goal","revision":1,"phase":"complete","createdAt":10}),
        json!({"id":"goal","revision":1,"phase":"complete","objective":"Owned goal"}),
        json!({"goal":{"id":"goal","revision":1,"phase":"complete","objective":"Owned goal"}}),
        json!({"id":"goal","revision":0,"phase":"complete","objective":"Owned goal","createdAt":10}),
        json!({"id":"goal","revision":1,"phase":3,"objective":"Owned goal","createdAt":10}),
        json!({"id":"","revision":1,"phase":"active","objective":"Owned goal","createdAt":10}),
    ] {
        project(&mut store, value);
        assert!(GoalFacts::read(&store, &scope("a")).is_err());
    }
    assert!(GoalFacts::read(&store, &RunScope::main("other", "a")).is_err());
}
#[test]
fn run_publication_scopes_goal_and_invalid_goal_never_corrupts_history_or_draft() {
    let mut authority = authority();
    let mut core = PresentationStore::default();
    core.select(ComposerDestination::run(scope("a"))).unwrap();
    project(
        &mut authority,
        json!({"id":"g","revision":1,"phase":"paused","objective":"Owned goal","createdAt":10}),
    );
    let a = core.refresh(&authority).unwrap();
    assert_eq!(a.goal.unwrap().phase, GoalPhase::Paused);
    core.select(ComposerDestination::run(scope("b"))).unwrap();
    assert!(core.refresh(&authority).unwrap().goal.is_none());
    core.select(ComposerDestination::run(scope("a"))).unwrap();
    project(
        &mut authority,
        json!({"id":"g","revision":1,"phase":false,"objective":"Owned goal","createdAt":10}),
    );
    let invalid = core.refresh(&authority).unwrap();
    assert!(invalid.goal.is_none());
    assert!(!invalid.transcript.is_stale);
    assert!(
        invalid
            .transcript
            .diagnostics
            .iter()
            .any(|text| text.starts_with("Goal unavailable:"))
    );
    assert_eq!(invalid.draft, a.draft);
    let mut serialized = serde_json::to_value(invalid).unwrap();
    serialized.as_object_mut().unwrap().remove("goal");
    assert!(
        serde_json::from_value::<RunSnapshot>(serialized)
            .unwrap()
            .goal
            .is_none()
    );
}
