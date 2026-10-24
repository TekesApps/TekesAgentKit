use std::{fmt::Debug, sync::Arc};

use tekes_agent_presentation::{
    SidebarObservation, SidebarObserverEndpointInput, SidebarObserverKey, SidebarObserverPhase,
    SidebarObserverSourceKey, SidebarObserverStatusInput,
};

fn key(
    source: &Arc<String>,
    child: &Arc<String>,
    endpoint: &str,
    generation: u64,
) -> SidebarObserverKey {
    SidebarObserverKey::from_owners(source, child, endpoint, generation).unwrap()
}

fn status(
    source: SidebarObserverSourceKey,
    entries: Vec<SidebarObserverEndpointInput>,
) -> SidebarObserverStatusInput {
    SidebarObserverStatusInput {
        source,
        revision: 1,
        endpoints: entries,
    }
}

fn endpoint(
    key: SidebarObserverKey,
    revision: u64,
    generation: u64,
) -> SidebarObserverEndpointInput {
    SidebarObserverEndpointInput {
        key,
        revision,
        observation: SidebarObservation {
            generation,
            phase: SidebarObserverPhase::Failed,
            converged: true,
        },
        error: Some("observer stopped".into()),
    }
}

fn debug_text(value: &impl Debug) -> String {
    format!("{value:?}")
}

#[test]
fn source_and_retry_keys_use_allocation_identity_and_clone_stably() {
    let source = Arc::new(String::from("source"));
    let child = Arc::new(String::from("child"));
    let same_source = source.clone();
    let source_key = SidebarObserverSourceKey::from_owner(&source);
    let retry_key = key(&source, &child, "native", 3);

    assert!(source_key.matches_owner(&same_source));
    assert_eq!(
        source_key,
        SidebarObserverSourceKey::from_owner(&same_source)
    );
    assert_eq!(retry_key, retry_key.clone());
    assert!(retry_key.matches_owners(&source, &child, "native", 3));
    assert_eq!(retry_key.source(), &source_key);
    assert_eq!(retry_key.endpoint_id(), "native");
    assert_eq!(retry_key.generation(), 3);
}

#[test]
fn copied_labels_and_generations_cannot_substitute_for_original_owners() {
    let source = Arc::new(String::from("same label"));
    let child = Arc::new(String::from("same label"));
    let retry_key = key(&source, &child, "native", 4);
    let replacement_source = Arc::new(String::from("same label"));
    let replacement_child = Arc::new(String::from("same label"));

    assert!(!retry_key.matches_owners(&replacement_source, &child, "native", 4));
    assert!(!retry_key.matches_owners(&source, &replacement_child, "native", 4));
    assert!(!retry_key.matches_owners(&source, &child, "pi", 4));
    assert!(!retry_key.matches_owners(&source, &child, "native", 5));

    let next_generation = key(&source, &child, "native", 5);
    assert_ne!(retry_key, next_generation);
    assert!(next_generation.matches_owners(&source, &child, "native", 5));
}

#[test]
fn fake_owner_keys_are_constructible_but_do_not_match_real_owners() {
    let real_source = Arc::new(String::from("real source"));
    let real_child = Arc::new(String::from("real child"));
    let fake_source = Arc::new(String::from("fake source"));
    let fake_child = Arc::new(String::from("fake child"));
    let forged = key(&fake_source, &fake_child, "native", 1);

    assert!(!forged.matches_owners(&real_source, &real_child, "native", 1));
}

#[test]
fn constructors_reject_empty_endpoint_and_zero_generation() {
    let source = Arc::new(());
    let child = Arc::new(());

    assert!(SidebarObserverKey::from_owners(&source, &child, " ", 1).is_err());
    assert!(SidebarObserverKey::from_owners(&source, &child, "native", 0).is_err());
}

#[test]
fn status_validation_requires_nonempty_unique_coherent_endpoint_entries() {
    let source_owner = Arc::new(String::from("host"));
    let child_a = Arc::new(String::from("a"));
    let child_b = Arc::new(String::from("b"));
    let source = SidebarObserverSourceKey::from_owner(&source_owner);

    let valid = status(
        source.clone(),
        vec![
            endpoint(key(&source_owner, &child_a, "native", 2), 1, 2),
            endpoint(key(&source_owner, &child_b, "pi", 7), 2, 7),
        ],
    );
    assert!(valid.validate().is_ok());

    assert!(status(source.clone(), vec![]).validate().is_err());
    assert!(
        status(
            source.clone(),
            vec![endpoint(key(&source_owner, &child_a, "native", 2), 0, 2)],
        )
        .validate()
        .is_err()
    );
    assert!(
        status(
            source.clone(),
            vec![endpoint(key(&source_owner, &child_a, "native", 2), 1, 3)],
        )
        .validate()
        .is_err()
    );
    assert!(
        status(
            SidebarObserverSourceKey::from_owner(&Arc::new(String::from("other host"))),
            vec![endpoint(key(&source_owner, &child_a, "native", 2), 1, 2)],
        )
        .validate()
        .is_err()
    );
    assert!(
        status(
            source.clone(),
            vec![
                endpoint(key(&source_owner, &child_a, "native", 2), 1, 2),
                endpoint(key(&source_owner, &child_b, "native", 2), 1, 2),
            ],
        )
        .validate()
        .is_err()
    );
}

#[test]
fn debug_output_does_not_leak_owner_addresses() {
    let source = Arc::new(String::from("source"));
    let child = Arc::new(String::from("child"));
    let source_pointer = format!("{:p}", Arc::as_ptr(&source));
    let child_pointer = format!("{:p}", Arc::as_ptr(&child));
    let source_key = SidebarObserverSourceKey::from_owner(&source);
    let retry_key = key(&source, &child, "native", 2);

    assert!(!debug_text(&source_key).contains(&source_pointer));
    assert!(!debug_text(&retry_key).contains(&source_pointer));
    assert!(!debug_text(&retry_key).contains(&child_pointer));
}
