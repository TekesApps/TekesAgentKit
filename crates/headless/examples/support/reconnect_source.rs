//! Strict source validators for the frozen reconnect acceptance observer.
//! This module is private to the example and does not change product behavior.
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use tekes_agent_contract::{Error, JournalSnapshot, ProjectionBlock, Result, require};

const SEQUENCED_QUEUE_PROJECTION_KEY: &str = "agentkit.queue.v1";

#[derive(Clone, Debug, PartialEq)]
pub struct PiFileObservation {
    pub canonical_path: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub bytes: u64,
    pub modified_seconds: u64,
    pub modified_nanos: u32,
}

impl PiFileObservation {
    /// Stat only a public SDK path inside the explicitly owned harness data root.
    /// Reject a symlink at any path component instead of following redirects.
    pub fn capture(path: &Path, owned_root: &Path) -> Result<Self> {
        let root = owned_root
            .canonicalize()
            .map_err(|error| Error::Protocol(error.to_string()))?;
        let requested = if path.is_absolute() {
            path.to_path_buf()
        } else {
            return Err(Error::Protocol("Pi sessionFile must be absolute".into()));
        };
        let canonical = requested
            .canonicalize()
            .map_err(|error| Error::Protocol(error.to_string()))?;
        require(
            canonical != root && canonical.starts_with(&root),
            "Pi sessionFile escaped owned root",
        )?;
        require(
            canonical == requested,
            "Pi sessionFile path is redirected or non-canonical",
        )?;
        let mut cursor = requested.as_path();
        while cursor.starts_with(&root) {
            let metadata =
                fs::symlink_metadata(cursor).map_err(|error| Error::Protocol(error.to_string()))?;
            require(
                !metadata.file_type().is_symlink(),
                "Pi sessionFile path contains a symlink",
            )?;
            if cursor == root {
                break;
            }
            cursor = cursor
                .parent()
                .ok_or_else(|| Error::Protocol("invalid Pi sessionFile path".into()))?;
        }
        require(
            cursor == root,
            "Pi sessionFile path does not traverse the owned root",
        )?;
        let metadata =
            fs::metadata(&canonical).map_err(|error| Error::Protocol(error.to_string()))?;
        require(metadata.is_file(), "Pi sessionFile is not a regular file")?;
        let modified = metadata
            .modified()
            .map_err(|error| Error::Protocol(error.to_string()))?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Protocol("Pi sessionFile mtime predates UNIX epoch".into()))?;
        #[cfg(unix)]
        let (device, inode) = {
            use std::os::unix::fs::MetadataExt;
            (metadata.dev(), metadata.ino())
        };
        #[cfg(not(unix))]
        return Err(Error::Protocol(
            "stable Pi file identity requires Unix metadata".into(),
        ));
        #[cfg(unix)]
        Ok(Self {
            canonical_path: canonical,
            device,
            inode,
            bytes: metadata.len(),
            modified_seconds: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
        })
    }

    pub fn endpoint_updated_at_ms(&self) -> f64 {
        std::time::Duration::new(self.modified_seconds, self.modified_nanos).as_secs_f64() * 1000.0
    }

    pub fn evidence(&self) -> Value {
        json!({"canonicalPath":self.canonical_path,"device":self.device,"inode":self.inode,
            "bytes":self.bytes,"mtimeSeconds":self.modified_seconds,"mtimeNanos":self.modified_nanos})
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DshSourceWitness {
    pub session_id: String,
    pub generation: String,
    pub through_sequence: i64,
    pub projections: ProjectionBlock,
    pub entries: Vec<Value>,
    pub has_more_before: bool,
}

impl DshSourceWitness {
    pub fn from_first_frame(
        frame: &Value,
        expected_session_id: &str,
        require_adapter_queue: bool,
    ) -> Result<Self> {
        require(
            frame["type"] == "snapshot",
            "fresh DSH follow did not begin with snapshot",
        )?;
        if let Some(kind) = frame["snapshot"]["projections"].get("kind") {
            require(
                kind == "sequenced",
                "fresh DSH follow carried a non-sequenced projection authority",
            )?;
        }
        let snapshot: JournalSnapshot = serde_json::from_value(frame["snapshot"].clone())?;
        snapshot.validate()?;
        require(
            !snapshot.has_more_before,
            "finite DSH source witness did not exhaust public history",
        )?;
        require(
            snapshot.session_id == expected_session_id,
            "DSH witness session pointer changed",
        )?;
        if let Some(generation) = frame.get("generation") {
            require(
                generation == &snapshot.generation,
                "DSH witness generation mismatch",
            )?;
        }
        let projections = snapshot.projections.clone().ok_or_else(|| {
            Error::Protocol("DSH fresh source omitted sequenced projections".into())
        })?;
        // SDK contract: a SessionProjectionBaseline belongs to this exact event cursor.
        require(
            projections.as_of_seq == snapshot.through_sequence,
            "DSH projection watermark differs from fresh follow cursor",
        )?;
        require(
            projections.values.contains_key("inbox"),
            "DSH fresh source omitted public SDK inbox projection",
        )?;
        if require_adapter_queue {
            require(
                projections
                    .values
                    .contains_key(SEQUENCED_QUEUE_PROJECTION_KEY),
                "DSH candidate source omitted normalized queue projection",
            )?;
        }
        Ok(Self {
            session_id: snapshot.session_id,
            generation: snapshot.generation,
            through_sequence: snapshot.through_sequence,
            projections,
            entries: snapshot
                .entries
                .into_iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<_, _>>()?,
            has_more_before: snapshot.has_more_before,
        })
    }

    /// Cover the exact retained window from an independently exhausted source.
    pub fn retained_entries(&self, floor: i64) -> Result<Vec<Value>> {
        require(
            self.entries
                .iter()
                .any(|entry| entry["event"]["seq"].as_i64() == Some(floor)),
            "DSH retained history floor missing from independent complete source",
        )?;
        Ok(self
            .entries
            .iter()
            .filter(|entry| {
                entry["event"]["seq"]
                    .as_i64()
                    .is_some_and(|seq| seq >= floor)
            })
            .cloned()
            .collect())
    }

    pub fn summary_projection_value(&self) -> Value {
        json!({"kind":"sequenced","asOfSeq":self.projections.as_of_seq,"values":self.projections.values})
    }

    pub fn matches_recovered_source(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.through_sequence == other.through_sequence
            && self.projections == other.projections
            && self.entries == other.entries
            && self.has_more_before == other.has_more_before
    }

    pub fn matches_runtime_values(
        &self,
        values: &std::collections::BTreeMap<String, tekes_agent_session::VersionedValue>,
        previous: Option<&std::collections::BTreeMap<String, tekes_agent_session::VersionedValue>>,
    ) -> bool {
        self.projections.values.len() == values.len()
            && self.projections.values.iter().all(|(key, expected)| {
                values.get(key).is_some_and(|actual| {
                    actual.value == *expected
                        && previous
                            .and_then(|previous| previous.get(key))
                            .is_none_or(|old| actual.sequence >= old.sequence)
                        && actual.sequence <= self.through_sequence
                })
            })
    }
}

pub fn without_dynamic_summary_fields(summary: &Value, projections: bool) -> Result<Value> {
    let mut object: Map<String, Value> = summary
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Protocol("session summary is missing or not an object".into()))?;
    object.remove("updatedAt");
    if projections {
        object.remove("projections");
    }
    Ok(Value::Object(object))
}

pub fn summaries_match_except_pi_updated_at(before: &Value, after: &Value) -> Result<bool> {
    Ok(without_dynamic_summary_fields(before, false)?
        == without_dynamic_summary_fields(after, false)?)
}

pub fn pi_summary_matches_file(summary: &Value, file: &PiFileObservation) -> bool {
    summary.get("updatedAt").and_then(Value::as_f64) == Some(file.endpoint_updated_at_ms())
}

pub fn summary_matches_dsh_witness(summary: &Value, witness: &DshSourceWitness) -> Result<bool> {
    let expected = witness.summary_projection_value();
    let source_time = witness
        .projections
        .values
        .get("sessionListMetadata")
        .and_then(|value| value.get("lastPromptAt"))
        .and_then(Value::as_f64);
    Ok(summary.get("projections") == Some(&expected)
        && source_time.is_some()
        && summary.get("updatedAt").and_then(Value::as_f64) == source_time)
}

#[cfg(test)]
pub fn monotonic_projection_values(before: &DshSourceWitness, after: &DshSourceWitness) -> bool {
    before
        .projections
        .values
        .keys()
        .eq(after.projections.values.keys())
        && before.projections.as_of_seq <= after.projections.as_of_seq
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn source_frame(session_id: &str, cursor: i64, as_of: i64, mut values: Value) -> Value {
        values["inbox"] = json!({"next-step":[],"next-turn":[]});
        json!({"type":"snapshot","snapshot":{
            "sessionId":session_id,"generation":"fresh-g1","throughSequence":cursor,
            "windowLimit":50,"entries":[],"hasMoreBefore":false,"historyRevision":null,
            "projections":{"kind":"sequenced","asOfSeq":as_of,"values":values}
        }})
    }

    #[test]
    fn dsh_witness_requires_fresh_typed_complete_exact_cursor_source() {
        let values = json!({"agentkit.queue.v1":{"items":[]},"title":"ok"});
        let witness =
            DshSourceWitness::from_first_frame(&source_frame("s", 9, 9, values.clone()), "s", true)
                .unwrap();
        assert_eq!(witness.projections.as_of_seq, 9);
        assert!(
            DshSourceWitness::from_first_frame(&source_frame("s", 9, 8, values.clone()), "s", true)
                .is_err()
        );
        let mut cached_hint = source_frame("s", 9, 9, values.clone());
        cached_hint["snapshot"]["projections"]["kind"] = json!("migration-required");
        assert!(DshSourceWitness::from_first_frame(&cached_hint, "s", true).is_err());
        assert!(
            DshSourceWitness::from_first_frame(
                &source_frame("s", 9, 9, json!({"title":"ok"})),
                "s",
                true
            )
            .is_err()
        );
        assert!(
            DshSourceWitness::from_first_frame(
                &source_frame("s", 9, 9, json!({"agentkit.queue.v1":{"items":[]}})),
                "other",
                true
            )
            .is_err()
        );
    }

    #[test]
    fn frozen_baseline_has_public_inbox_without_new_adapter_queue() {
        let frame = source_frame("s", 9, 9, json!({"title":"baseline"}));
        assert!(DshSourceWitness::from_first_frame(&frame, "s", false).is_ok());
        assert!(DshSourceWitness::from_first_frame(&frame, "s", true).is_err());
        let mut missing_inbox = frame;
        missing_inbox["snapshot"]["projections"]["values"]
            .as_object_mut()
            .unwrap()
            .remove("inbox");
        assert!(DshSourceWitness::from_first_frame(&missing_inbox, "s", false).is_err());
    }

    #[test]
    fn dsh_summary_requires_source_time_and_rejects_wrong_authority() {
        let no_time = DshSourceWitness::from_first_frame(
            &source_frame("s", 9, 9, json!({"agentkit.queue.v1":{"items":[]}})),
            "s",
            true,
        )
        .unwrap();
        assert!(
            !summary_matches_dsh_witness(
                &json!({"projections":no_time.summary_projection_value()}),
                &no_time
            )
            .unwrap()
        );
        let witness = DshSourceWitness::from_first_frame(
            &source_frame(
                "s",
                9,
                9,
                json!({"agentkit.queue.v1":{"items":[]},"sessionListMetadata":{"lastPromptAt":12}}),
            ),
            "s",
            true,
        )
        .unwrap();
        let mut summary = json!({"projections":witness.summary_projection_value(),"updatedAt":12});
        assert!(summary_matches_dsh_witness(&summary, &witness).unwrap());
        summary["projections"]["kind"] = json!("cached");
        assert!(!summary_matches_dsh_witness(&summary, &witness).unwrap());
        let mut mismatch = source_frame("s", 9, 9, json!({"agentkit.queue.v1":{"items":[]}}));
        mismatch["generation"] = json!("wrong");
        assert!(DshSourceWitness::from_first_frame(&mismatch, "s", true).is_err());
    }

    #[test]
    fn dsh_comparison_rejects_projection_content_or_cut_changes() {
        let first = DshSourceWitness::from_first_frame(
            &source_frame(
                "s",
                9,
                9,
                json!({"agentkit.queue.v1":{"items":[]},"title":"old"}),
            ),
            "s",
            true,
        )
        .unwrap();
        let changed = DshSourceWitness::from_first_frame(
            &source_frame(
                "s",
                9,
                9,
                json!({"agentkit.queue.v1":{"items":[]},"title":"new"}),
            ),
            "s",
            true,
        )
        .unwrap();
        let regressed = DshSourceWitness::from_first_frame(
            &source_frame(
                "s",
                8,
                8,
                json!({"agentkit.queue.v1":{"items":[]},"title":"old"}),
            ),
            "s",
            true,
        )
        .unwrap();
        assert!(!first.matches_recovered_source(&changed));
        assert!(!first.matches_recovered_source(&regressed));
        assert!(!monotonic_projection_values(&first, &regressed));
    }

    #[test]
    fn retained_history_keeps_exact_loaded_floor_and_compares_complete_content() {
        let mut source = DshSourceWitness::from_first_frame(
            &source_frame("s", 9, 9, json!({"agentkit.queue.v1":{"items":[]}})),
            "s",
            true,
        )
        .unwrap();
        source.entries = vec![
            json!({"event":{"seq":1,"data":{"text":"unloaded"}}}),
            json!({"event":{"seq":3,"data":{"text":"retained"}}}),
            json!({"event":{"seq":7,"data":{"text":"final"}}}),
        ];
        let retained = source.retained_entries(3).unwrap();
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0]["event"]["seq"], 3);
        assert!(source.retained_entries(2).is_err());
        let mut changed = source.clone();
        changed.entries[1]["event"]["data"]["text"] = json!("changed-same-seq");
        assert_ne!(retained, changed.retained_entries(3).unwrap());
        assert!(!source.matches_recovered_source(&changed));
        let mut partial = source_frame("s", 9, 9, json!({"agentkit.queue.v1":{"items":[]}}));
        partial["snapshot"]["hasMoreBefore"] = json!(true);
        assert!(DshSourceWitness::from_first_frame(&partial, "s", true).is_err());
    }

    #[test]
    fn dsh_runtime_values_reject_missing_keys_and_watermark_regression() {
        use tekes_agent_session::VersionedValue;
        let first = DshSourceWitness::from_first_frame(
            &source_frame(
                "s",
                9,
                9,
                json!({"agentkit.queue.v1":{"items":[]},"title":"ok"}),
            ),
            "s",
            true,
        )
        .unwrap();
        let previous = [
            (
                "inbox".to_owned(),
                VersionedValue {
                    sequence: 8,
                    value: json!({"next-step":[],"next-turn":[]}),
                },
            ),
            (
                "title".to_owned(),
                VersionedValue {
                    sequence: 8,
                    value: json!("ok"),
                },
            ),
            (
                "agentkit.queue.v1".to_owned(),
                VersionedValue {
                    sequence: 8,
                    value: json!({"items":[]}),
                },
            ),
        ]
        .into();
        let current = [
            (
                "inbox".to_owned(),
                VersionedValue {
                    sequence: 9,
                    value: json!({"next-step":[],"next-turn":[]}),
                },
            ),
            (
                "title".to_owned(),
                VersionedValue {
                    sequence: 7,
                    value: json!("ok"),
                },
            ),
            (
                "agentkit.queue.v1".to_owned(),
                VersionedValue {
                    sequence: 9,
                    value: json!({"items":[]}),
                },
            ),
        ]
        .into();
        assert!(!first.matches_runtime_values(&current, Some(&previous)));
        let mut missing = previous.clone();
        missing.remove("title");
        assert!(!first.matches_runtime_values(&missing, None));
        assert!(first.matches_runtime_values(&previous, None));
        let current = [
            (
                "inbox".to_owned(),
                VersionedValue {
                    sequence: 9,
                    value: json!({"next-step":[],"next-turn":[]}),
                },
            ),
            (
                "title".to_owned(),
                VersionedValue {
                    sequence: 9,
                    value: json!("ok"),
                },
            ),
            (
                "agentkit.queue.v1".to_owned(),
                VersionedValue {
                    sequence: 9,
                    value: json!({"items":[]}),
                },
            ),
        ]
        .into();
        assert!(first.matches_runtime_values(&current, Some(&previous)));
    }

    #[test]
    fn pi_file_source_rejects_escape_and_changed_identity_or_mtime() {
        let temp = std::env::temp_dir().join(format!(
            "agentkit-reconnect-source-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = temp.join("owned");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let file = root.join("session.jsonl");
        fs::write(&file, "one").unwrap();
        let first = PiFileObservation::capture(&file, &root).unwrap();
        fs::write(&file, "changed-size").unwrap();
        let second = PiFileObservation::capture(&file, &root).unwrap();
        assert_ne!(first, second);
        let outside = temp.join("outside.jsonl");
        fs::write(&outside, "outside").unwrap();
        assert!(PiFileObservation::capture(&outside, &root).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = root.join("redirect.jsonl");
            symlink(&outside, &link).unwrap();
            assert!(PiFileObservation::capture(&link, &root).is_err());
        }
        assert!(first.endpoint_updated_at_ms().is_finite());
        assert!(pi_summary_matches_file(
            &json!({"updatedAt":first.endpoint_updated_at_ms()}),
            &first
        ));
        assert!(!pi_summary_matches_file(
            &json!({"updatedAt":first.endpoint_updated_at_ms() + 1.0}),
            &first
        ));
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn pi_summary_comparison_preserves_every_field_except_updated_at() {
        let before = json!({"sessionId":"s","updatedAt":1,"other":true});
        let same = json!({"sessionId":"s","updatedAt":2,"other":true});
        let changed = json!({"sessionId":"s","updatedAt":2,"other":false});
        assert!(summaries_match_except_pi_updated_at(&before, &same).unwrap());
        assert!(!summaries_match_except_pi_updated_at(&before, &changed).unwrap());
    }
}
