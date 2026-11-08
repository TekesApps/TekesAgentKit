use tekes_agent_presentation::*;

fn thought(id: &str, order: i64) -> TranscriptThinkingItem<'_> {
    TranscriptThinkingItem {
        id,
        arrival_order: order,
        response_frame_id: None,
        is_provisional: true,
        reasoning_has_ended: false,
        has_reasoning: true,
        has_visible_content: false,
    }
}
fn input<'a>(items: &'a [TranscriptThinkingItem<'a>]) -> TranscriptThinkingInput<'a> {
    TranscriptThinkingInput {
        mode: TranscriptThinkingMode::None,
        is_settled: false,
        is_live_presentation_owner: true,
        has_final_output: false,
        elapsed_seconds: 1,
        items,
        calls: &[],
        narrations: &[],
    }
}
#[test]
fn default_and_legacy_preference_match_clientkit_without_owning_storage() {
    for value in ["", "none", "invalid", "COMPACT"] {
        assert_eq!(
            TranscriptThinkingMode::from_preference(value),
            TranscriptThinkingMode::None
        );
    }
    for value in ["compact", "compact-thinking"] {
        let mode = TranscriptThinkingMode::from_preference(value);
        assert_eq!(mode, TranscriptThinkingMode::Compact);
        assert_eq!(mode.preference_value(), "compact");
    }
    assert_eq!(TranscriptThinkingMode::default().preference_value(), "none");
}
#[test]
fn none_is_one_passive_status_without_a_hidden_reasoning_disclosure() {
    let items = [thought("reasoning", 2)];
    let mut facts = input(&items);
    for (seconds, dots) in [(0, 1), (1, 2), (2, 3), (3, 1), (u64::MAX, 1)] {
        facts.elapsed_seconds = seconds;
        let policy = TranscriptThinkingPolicy::read(&facts).unwrap();
        assert!(policy.disclosures.is_empty());
        assert_eq!(
            policy.placeholder,
            Some(TranscriptThinkingPlaceholder {
                item_id: "reasoning",
                dots
            })
        );
    }
}
#[test]
fn reasoning_end_does_not_replace_status_but_settlement_removes_it_and_restores_disclosures() {
    let mut items = [thought("reasoning", 2)];
    items[0].reasoning_has_ended = true;
    let mut facts = input(&items);
    assert!(
        TranscriptThinkingPolicy::read(&facts)
            .unwrap()
            .placeholder
            .is_some()
    );
    facts.is_settled = true;
    facts.is_live_presentation_owner = false;
    let policy = TranscriptThinkingPolicy::read(&facts).unwrap();
    assert_eq!(policy.placeholder, None);
    assert_eq!(
        policy.disclosures,
        vec![TranscriptReasoningDisclosure {
            item_id: "reasoning",
            active: false
        }]
    );
    assert!(
        items[0].has_reasoning,
        "display policy never deletes source reasoning"
    );
}
#[test]
fn later_narration_replaces_status_while_earlier_narration_remains_independent() {
    let mut narration = thought("narration", 1);
    narration.has_reasoning = false;
    narration.has_visible_content = true;
    let items = [narration, thought("reasoning", 2)];
    assert!(
        TranscriptThinkingPolicy::read(&input(&items))
            .unwrap()
            .placeholder
            .is_some()
    );
    let items = [
        TranscriptThinkingItem {
            arrival_order: 3,
            ..narration
        },
        items[1],
    ];
    assert!(
        TranscriptThinkingPolicy::read(&input(&items))
            .unwrap()
            .placeholder
            .is_none()
    );
}
#[test]
fn tool_replacement_uses_first_arrival_or_same_frame_not_vector_order() {
    let mut items = [thought("reasoning", 2)];
    items[0].response_frame_id = Some("frame-a");
    for call in [
        TranscriptThinkingCall {
            group_id: "frame-b",
            arrival_order: 3,
        },
        TranscriptThinkingCall {
            group_id: "frame-a",
            arrival_order: 1,
        },
    ] {
        let calls = [call];
        let mut facts = input(&items);
        facts.calls = &calls;
        assert_eq!(
            TranscriptThinkingPolicy::read(&facts).unwrap().placeholder,
            None
        );
    }
    let calls = [TranscriptThinkingCall {
        group_id: "frame-b",
        arrival_order: 1,
    }];
    let mut facts = input(&items);
    facts.calls = &calls;
    assert!(
        TranscriptThinkingPolicy::read(&facts)
            .unwrap()
            .placeholder
            .is_some()
    );
}
#[test]
fn same_frame_or_later_group_narration_replaces_status() {
    let mut items = [thought("reasoning", 2)];
    items[0].response_frame_id = Some("frame-a");
    for narration in [
        TranscriptThinkingNarration {
            group_id: "frame-a",
            arrival_order: 0,
        },
        TranscriptThinkingNarration {
            group_id: "frame-b",
            arrival_order: 2,
        },
    ] {
        let narrations = [narration];
        let mut facts = input(&items);
        facts.narrations = &narrations;
        assert!(
            TranscriptThinkingPolicy::read(&facts)
                .unwrap()
                .placeholder
                .is_none()
        );
    }
}
#[test]
fn only_newest_reasoning_owns_status_including_ties_and_has_no_newest_fallback() {
    let items = [thought("z", 4), thought("old", 2), thought("a", 4)];
    assert_eq!(
        TranscriptThinkingPolicy::read(&input(&items))
            .unwrap()
            .placeholder
            .unwrap()
            .item_id,
        "z"
    );
    let items = [
        TranscriptThinkingItem {
            has_visible_content: true,
            ..items[0]
        },
        items[1],
        items[2],
    ];
    assert!(
        TranscriptThinkingPolicy::read(&input(&items))
            .unwrap()
            .placeholder
            .is_none()
    );
}
#[test]
fn compact_keeps_disclosures_and_only_provisional_unended_owner_is_active() {
    let items = [thought("old", 2), thought("new", 4)];
    let policy = |items| {
        let mut facts = input(items);
        facts.mode = TranscriptThinkingMode::Compact;
        TranscriptThinkingPolicy::read(&facts).unwrap()
    };
    assert_eq!(
        policy(&items)
            .disclosures
            .iter()
            .filter(|row| row.active)
            .map(|row| row.item_id)
            .collect::<Vec<_>>(),
        ["new"]
    );
    let items = [
        TranscriptThinkingItem {
            is_provisional: false,
            ..items[0]
        },
        TranscriptThinkingItem {
            reasoning_has_ended: true,
            ..items[1]
        },
    ];
    let result = policy(&items);
    assert_eq!(result.disclosures.len(), 2);
    assert!(result.disclosures.iter().all(|row| !row.active));
    assert!(result.placeholder.is_none());
}
#[test]
fn final_output_or_lost_live_owner_never_reactivates_thinking() {
    let items = [thought("reasoning", 2)];
    let mut facts = input(&items);
    facts.has_final_output = true;
    assert!(
        TranscriptThinkingPolicy::read(&facts)
            .unwrap()
            .placeholder
            .is_none()
    );
    facts.mode = TranscriptThinkingMode::Compact;
    assert!(!TranscriptThinkingPolicy::read(&facts).unwrap().disclosures[0].active);
    facts.has_final_output = false;
    facts.is_live_presentation_owner = false;
    assert!(!TranscriptThinkingPolicy::read(&facts).unwrap().disclosures[0].active);
}
#[test]
fn compact_same_frame_tool_suppresses_active_header_without_hiding_retained_disclosure() {
    let mut items = [thought("reasoning", 2)];
    items[0].response_frame_id = Some("frame");
    let calls = [TranscriptThinkingCall {
        group_id: "frame",
        arrival_order: 0,
    }];
    let mut facts = input(&items);
    facts.mode = TranscriptThinkingMode::Compact;
    facts.calls = &calls;
    assert_eq!(
        TranscriptThinkingPolicy::read(&facts).unwrap().disclosures,
        [TranscriptReasoningDisclosure {
            item_id: "reasoning",
            active: false
        }]
    );
}
#[test]
fn duplicate_or_empty_source_identities_are_rejected() {
    for items in [
        vec![thought("", 1)],
        vec![thought("same", 1), thought("same", 2)],
        vec![TranscriptThinkingItem {
            response_frame_id: Some(""),
            ..thought("ok", 1)
        }],
    ] {
        assert!(TranscriptThinkingPolicy::read(&input(&items)).is_err());
    }
    let items = [thought("ok", 1)];
    let mut facts = input(&items);
    facts.calls = &[TranscriptThinkingCall {
        group_id: "",
        arrival_order: 1,
    }];
    assert!(TranscriptThinkingPolicy::read(&facts).is_err());
    facts.calls = &[];
    facts.narrations = &[TranscriptThinkingNarration {
        group_id: "",
        arrival_order: 1,
    }];
    assert!(TranscriptThinkingPolicy::read(&facts).is_err());
}

#[test]
fn display_frame_binds_source_and_hashes_source_facts_separately_from_settings_and_elapsed() {
    let scope = RunScope::main("sep", "session");
    let items = [thought("reasoning", 2)];
    let mut facts = input(&items);
    let before = TranscriptThinkingFrame::read(&scope, "turn", 5, 2, 1, &facts).unwrap();
    facts.mode = TranscriptThinkingMode::Compact;
    facts.elapsed_seconds = 42;
    let after = TranscriptThinkingFrame::read(&scope, "turn", 5, 2, 2, &facts).unwrap();
    assert_eq!(before.source_fingerprint(), after.source_fingerprint());
    assert!(before.matches_source(&scope, "turn", 5, 2));
    for (wrong_scope, turn, publication, epoch) in [
        (RunScope::main("sep", "other"), "turn", 5, 2),
        (scope.clone(), "other-turn", 5, 2),
        (scope.clone(), "turn", 6, 2),
        (scope.clone(), "turn", 5, 3),
    ] {
        assert!(!before.matches_source(&wrong_scope, turn, publication, epoch));
    }
    assert_eq!(
        before.display_for("reasoning"),
        Some(TranscriptReasoningDisplay::Placeholder { dots: 2 })
    );
    assert_eq!(
        after.display_for("reasoning"),
        Some(TranscriptReasoningDisplay::Disclosure { active: true })
    );
    assert_eq!(after.display_for("unknown"), None);
    facts.has_final_output = true;
    let changed = TranscriptThinkingFrame::read(&scope, "turn", 6, 2, 3, &facts).unwrap();
    assert_ne!(after.source_fingerprint(), changed.source_fingerprint());
}
#[test]
fn display_frame_requires_real_versions_and_turn_identity() {
    let scope = RunScope::main("sep", "session");
    let items = [thought("reasoning", 2)];
    for (turn, publication, revision) in [("", 1, 1), ("turn", 0, 1), ("turn", 1, 0)] {
        assert!(
            TranscriptThinkingFrame::read(&scope, turn, publication, 0, revision, &input(&items))
                .is_err()
        );
    }
}
