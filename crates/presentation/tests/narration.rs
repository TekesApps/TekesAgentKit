use tekes_agent_presentation::*;
fn item<'a>(
    id: &'a str,
    frame: &'a str,
    texts: &'a [String],
    canonical: bool,
) -> TranscriptNarrationItem<'a> {
    TranscriptNarrationItem {
        item_id: id,
        frame_id: frame,
        node_texts: texts,
        canonical_narration: canonical,
    }
}
fn choice(
    timeline: TranscriptNarrationTimeline,
    texts: &[String],
    fallback: &[String],
) -> TranscriptNarrationChoice {
    TranscriptNarrationPolicy::read(
        timeline,
        &[item("message", "frame", texts, false)],
        &[TranscriptNarrationFallback {
            frame_id: "frame",
            node_texts: fallback,
        }],
    )
    .unwrap()
    .choice_for("frame")
    .unwrap()
    .clone()
}
#[test]
fn fullest_visible_node_characters_then_canonical_role_then_identity_win() {
    let emoji = vec!["👩🏽‍💻".into()];
    let words = vec!["中文".into()];
    let policy = TranscriptNarrationPolicy::read(
        TranscriptNarrationTimeline::Running,
        &[
            item("emoji", "f", &emoji, false),
            item("z-assistant", "f", &words, false),
            item("canonical-a", "f", &words, true),
            item("canonical-z", "f", &words, true),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        policy.choice_for("f"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: "canonical-z".into()
        })
    );
}
#[test]
fn carrier_counts_each_node_but_prefix_comparison_joins_node_boundaries() {
    let split = vec!["e".into(), "\u{301}".into()];
    let combined = vec!["é".into()];
    let policy = TranscriptNarrationPolicy::read(
        TranscriptNarrationTimeline::Running,
        &[
            item("split", "f", &split, false),
            item("combined", "f", &combined, true),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        policy.choice_for("f"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: "split".into()
        })
    );
    assert_eq!(
        choice(
            TranscriptNarrationTimeline::Running,
            &split,
            &["é追加".into()]
        ),
        TranscriptNarrationChoice::GroupFallback {
            frame_id: "frame".into()
        }
    );
}
#[test]
fn running_keeps_only_a_strict_cumulative_prefix_and_settled_canonical_takes_ownership() {
    let text = vec!["旁白".into()];
    let fallback = vec!["旁白完整".into()];
    assert_eq!(
        choice(TranscriptNarrationTimeline::Running, &text, &fallback),
        TranscriptNarrationChoice::GroupFallback {
            frame_id: "frame".into()
        }
    );
    for (timeline, group) in [
        (TranscriptNarrationTimeline::Running, text.clone()),
        (
            TranscriptNarrationTimeline::Running,
            vec!["不同来源".into()],
        ),
        (TranscriptNarrationTimeline::Settled, fallback),
    ] {
        assert_eq!(
            choice(timeline, &text, &group),
            TranscriptNarrationChoice::Item {
                item_id: "message".into()
            }
        );
    }
}
#[test]
fn canonical_unicode_equality_and_whole_character_prefix_match_swift_without_rewriting_text() {
    let decomposed = vec!["cafe\u{301}".into()];
    let original = decomposed.clone();
    assert_eq!(
        choice(
            TranscriptNarrationTimeline::Running,
            &decomposed,
            &["café".into()]
        ),
        TranscriptNarrationChoice::Item {
            item_id: "message".into()
        }
    );
    assert_eq!(
        choice(
            TranscriptNarrationTimeline::Running,
            &decomposed,
            &["café追加".into()]
        ),
        TranscriptNarrationChoice::GroupFallback {
            frame_id: "frame".into()
        }
    );
    for (partial, full) in [("a", "a\u{20dd}more"), ("👩", "👩🏽‍💻more")] {
        assert_eq!(
            choice(
                TranscriptNarrationTimeline::Running,
                &[partial.into()],
                &[full.into()]
            ),
            TranscriptNarrationChoice::Item {
                item_id: "message".into()
            }
        );
    }
    assert_eq!(decomposed, original);
}
#[test]
fn empty_node_list_differs_from_an_existing_node_with_empty_text_and_frames_are_independent() {
    let empty_node = vec![String::new()];
    assert_eq!(
        choice(TranscriptNarrationTimeline::Settled, &[], &empty_node),
        TranscriptNarrationChoice::GroupFallback {
            frame_id: "frame".into()
        }
    );
    assert_eq!(
        choice(
            TranscriptNarrationTimeline::Settled,
            &empty_node,
            &["fallback".into()]
        ),
        TranscriptNarrationChoice::Item {
            item_id: "message".into()
        }
    );
    assert_eq!(
        choice(TranscriptNarrationTimeline::Running, &[], &[]),
        TranscriptNarrationChoice::Empty
    );
    let policy = TranscriptNarrationPolicy::read(
        TranscriptNarrationTimeline::Running,
        &[
            item("a", "first", &empty_node, false),
            item("b", "second", &empty_node, false),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        policy.choice_for("first"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: "a".into()
        })
    );
    assert_eq!(
        policy.choice_for("second"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: "b".into()
        })
    );
}
#[test]
fn duplicate_or_empty_source_identities_are_rejected() {
    let text = vec!["text".into()];
    assert!(
        TranscriptNarrationPolicy::read(
            TranscriptNarrationTimeline::Running,
            &[item("a", "f", &text, false), item("a", "f", &text, false)],
            &[]
        )
        .is_err()
    );
    assert!(
        TranscriptNarrationPolicy::read(
            TranscriptNarrationTimeline::Running,
            &[item("a", "", &text, false)],
            &[]
        )
        .is_err()
    );
    assert!(
        TranscriptNarrationPolicy::read(
            TranscriptNarrationTimeline::Running,
            &[],
            &[
                TranscriptNarrationFallback {
                    frame_id: "f",
                    node_texts: &text
                },
                TranscriptNarrationFallback {
                    frame_id: "f",
                    node_texts: &text
                }
            ]
        )
        .is_err()
    );
}

#[test]
fn unicode_identity_ranking_matches_source_and_canonically_ambiguous_tokens_are_not_merged() {
    let text = vec!["x".into()];
    let policy = TranscriptNarrationPolicy::read(
        TranscriptNarrationTimeline::Running,
        &[
            item("é", "f", &text, false),
            item("e\u{301}z", "f", &text, false),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        policy.choice_for("f"),
        Some(&TranscriptNarrationChoice::Item {
            item_id: "e\u{301}z".into()
        })
    );
    assert!(
        TranscriptNarrationPolicy::read(
            TranscriptNarrationTimeline::Running,
            &[
                item("é", "f", &text, false),
                item("e\u{301}", "f", &text, false)
            ],
            &[]
        )
        .is_err()
    );
    assert!(
        TranscriptNarrationPolicy::read(
            TranscriptNarrationTimeline::Running,
            &[
                item("a", "é", &text, false),
                item("b", "e\u{301}", &text, false)
            ],
            &[]
        )
        .is_err()
    );
}
