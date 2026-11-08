use tekes_agent_contract::{
    ArtifactSelectionReference, ArtifactSelectionResolution, ArtifactSelectionTarget,
    Utf16SelectionRange,
};

#[test]
fn persisted_offsets_follow_existing_clientkit_integral_json_semantics() {
    for value in [
        r#"{"location":1,"length":2}"#,
        r#"{"location":1.0,"length":2e0}"#,
        r#"{"location":1e0,"length":2.0}"#,
    ] {
        let range: Utf16SelectionRange = serde_json::from_str(value).unwrap();
        assert_eq!(
            range,
            Utf16SelectionRange {
                location: 1,
                length: 2
            }
        );
    }
    for value in [
        r#"{"location":1.5,"length":2}"#,
        r#"{"location":1,"length":-1}"#,
        r#"{"location":true,"length":2}"#,
        r#"{"location":"1","length":2}"#,
    ] {
        assert!(serde_json::from_str::<Utf16SelectionRange>(value).is_err());
    }
}

fn capture() -> ArtifactSelectionReference {
    ArtifactSelectionReference::capture(
        "doc-A".into(),
        "artifact-17".into(),
        "character".into(),
        vec!["biography".into(), "paragraph-3".into()],
        "revision-42".into(),
        "sha256:fixture".into(),
        "开头🐉中文选中尾",
        Utf16SelectionRange {
            location: 4,
            length: 2,
        },
        2,
        2,
    )
    .unwrap()
}

fn target<'a>(
    reference: &'a ArtifactSelectionReference,
    field_text: &'a str,
) -> ArtifactSelectionTarget<'a> {
    ArtifactSelectionTarget {
        document_id: Some(&reference.document_id),
        artifact_id: Some(&reference.artifact_id),
        artifact_type: Some(&reference.artifact_type),
        field_path: Some(&reference.field_path),
        revision: Some(&reference.revision),
        digest: Some(&reference.digest),
        field_text: Some(field_text),
    }
}

#[test]
fn capture_uses_utf16_coordinates_and_scalar_safe_context_for_chinese_and_emoji() {
    let reference = capture();
    assert_eq!(reference.quote, "中文");
    assert_eq!(reference.context_before, "🐉");
    assert_eq!(reference.context_after, "选中");
    assert_eq!(reference.range.location, 4);
    assert_eq!(reference.range.length, 2);
    reference.validate().unwrap();
}

#[test]
fn exact_resolution_requires_document_artifact_type_field_revision_digest_and_quote() {
    let reference = capture();
    assert_eq!(
        reference.resolve(&target(&reference, "开头🐉中文选中尾")),
        ArtifactSelectionResolution::Exact
    );

    for changed in [
        ArtifactSelectionTarget {
            document_id: Some("doc-B"),
            ..target(&reference, "开头🐉中文选中尾")
        },
        ArtifactSelectionTarget {
            artifact_id: Some("artifact-elsewhere"),
            ..target(&reference, "开头🐉中文选中尾")
        },
        ArtifactSelectionTarget {
            artifact_type: Some("location"),
            ..target(&reference, "开头🐉中文选中尾")
        },
        ArtifactSelectionTarget {
            field_path: Some(&["biography".into(), "paragraph-4".into()]),
            ..target(&reference, "开头🐉中文选中尾")
        },
    ] {
        assert_eq!(
            reference.resolve(&changed),
            ArtifactSelectionResolution::Unavailable
        );
    }

    assert_eq!(
        reference.resolve(&ArtifactSelectionTarget {
            revision: Some("revision-43"),
            ..target(&reference, "开头🐉中文选中尾")
        }),
        ArtifactSelectionResolution::Stale
    );
    assert_eq!(
        reference.resolve(&ArtifactSelectionTarget {
            digest: Some("sha256:changed"),
            ..target(&reference, "开头🐉中文选中尾")
        }),
        ArtifactSelectionResolution::Stale
    );
}

#[test]
fn missing_target_fields_are_unavailable_and_quote_or_range_mismatch_is_stale() {
    let reference = capture();
    assert_eq!(
        reference.resolve(&ArtifactSelectionTarget::default()),
        ArtifactSelectionResolution::Unavailable
    );
    assert_eq!(
        reference.resolve(&ArtifactSelectionTarget {
            field_text: None,
            ..target(&reference, "开头🐉中文选中尾")
        }),
        ArtifactSelectionResolution::Unavailable
    );
    assert_eq!(
        reference.resolve(&target(&reference, "开头🐉别的选中尾")),
        ArtifactSelectionResolution::Stale
    );
    // The same quote exists later, but resolution never searches for a replacement range.
    assert_eq!(
        reference.resolve(&target(&reference, "开头🐉错选中中文尾")),
        ArtifactSelectionResolution::Stale
    );
    let invalid_range = ArtifactSelectionReference {
        range: Utf16SelectionRange {
            location: u64::MAX,
            length: 4,
        },
        ..reference.clone()
    };
    assert_eq!(
        invalid_range.resolve(&target(&invalid_range, "开头🐉中文选中尾")),
        ArtifactSelectionResolution::Stale
    );
}

#[test]
fn identical_text_in_another_source_cannot_resolve_the_capture() {
    let reference = capture();
    let other_path = vec!["summary".into(), "paragraph-3".into()];
    let other_artifact = "artifact-18";
    let other_document = "doc-B";
    for target in [
        ArtifactSelectionTarget {
            artifact_id: Some(other_artifact),
            ..target(&reference, "开头🐉中文选中尾")
        },
        ArtifactSelectionTarget {
            document_id: Some(other_document),
            ..target(&reference, "开头🐉中文选中尾")
        },
        ArtifactSelectionTarget {
            field_path: Some(&other_path),
            ..target(&reference, "开头🐉中文选中尾")
        },
    ] {
        assert_eq!(
            reference.resolve(&target),
            ArtifactSelectionResolution::Unavailable
        );
    }
}

#[test]
fn capture_rejects_surrogate_split_empty_scope_and_unrepresentable_ranges() {
    let bad_range = Utf16SelectionRange {
        location: 3,
        length: 1,
    };
    assert!(
        ArtifactSelectionReference::capture(
            "doc-A".into(),
            "artifact-17".into(),
            "character".into(),
            vec!["biography".into()],
            "revision-42".into(),
            "sha256:fixture".into(),
            "开头🐉中文选中尾",
            bad_range,
            0,
            0,
        )
        .is_err()
    );
    assert!(
        ArtifactSelectionReference::capture(
            "doc-A".into(),
            "artifact-17".into(),
            "character".into(),
            vec!["".into()],
            "revision-42".into(),
            "sha256:fixture".into(),
            "text",
            Utf16SelectionRange {
                location: 0,
                length: 1,
            },
            0,
            0,
        )
        .is_err()
    );
    assert!(
        Utf16SelectionRange {
            location: u64::MAX,
            length: 1,
        }
        .validate()
        .is_err()
    );
    assert!(serde_json::from_str::<Utf16SelectionRange>(r#"{"location":-1,"length":1}"#).is_err());
    assert!(
        serde_json::from_str::<Utf16SelectionRange>(r#"{"location":9007199254740992,"length":0}"#)
            .is_ok()
    );
    let out_of_safe_range =
        serde_json::from_str::<Utf16SelectionRange>(r#"{"location":9007199254740992,"length":0}"#)
            .unwrap();
    assert!(out_of_safe_range.validate().is_err());
    let malformed_quote = ArtifactSelectionReference {
        quote: "短".into(),
        ..capture()
    };
    assert!(malformed_quote.validate().is_err());
}

#[test]
fn caret_capture_and_context_limits_do_not_split_supplementary_scalars() {
    let reference = ArtifactSelectionReference::capture(
        "doc-A".into(),
        "artifact-17".into(),
        "character".into(),
        vec!["biography".into()],
        "revision-42".into(),
        "sha256:fixture".into(),
        "🐉中🐉",
        Utf16SelectionRange {
            location: 2,
            length: 0,
        },
        1,
        2,
    )
    .unwrap();
    assert_eq!(reference.quote, "");
    assert_eq!(reference.context_before, "");
    assert_eq!(reference.context_after, "中");
    assert_eq!(
        reference.resolve(&target(&reference, "🐉中🐉")),
        ArtifactSelectionResolution::Exact
    );
}
