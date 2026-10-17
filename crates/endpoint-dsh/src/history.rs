use serde_json::{Value, json};
use tekes_agent_contract::*;
/// Expand DSH's packed page records while retaining their durable sequence and tool view.
pub fn decode_records(records: &[Value]) -> Result<Vec<HistoryEntry>> {
    let mut entries = Vec::new();
    for record in records {
        match string(record, "type")? {
            "event" => {
                let mut raw = record["event"].clone();
                if matches!(
                    raw["type"].as_str(),
                    Some("feedback/message-put" | "feedback/message-delete")
                ) && raw.get("ignorable").is_none()
                {
                    raw["ignorable"] = json!(true);
                }
                let event: SessionEvent = serde_json::from_value(raw)?;
                event.validate()?;
                let model_only = event
                    .metadata
                    .get("surfaceOp")
                    .filter(|v| !v.is_null() && *v != &json!("append"))
                    .map(|_| true);
                entries.push(HistoryEntry {
                    event,
                    view: record.get("view").filter(|v| !v.is_null()).cloned(),
                    is_model_only: model_only,
                });
            }
            "chunks" => {
                let event = &record["event"];
                let data = &event["data"];
                let seq = integer(event, "seq")?;
                let mut time = integer(event, "time")?;
                let kind = string(event, "type")?;
                let (payload_key, chunk_type) = match kind {
                    "chunkrow/text-chunks" => ("texts", "text-delta"),
                    "chunkrow/reasoning-chunks" => ("texts", "reasoning-delta"),
                    "chunkrow/tool-call-chunks" => ("args", "tool-call-delta"),
                    _ => return Err(Error::Protocol("unknown packed DSH chunk".into())),
                };
                let members = array(data, payload_key)?;
                let gaps = array(data, "dt")?;
                require(
                    !members.is_empty()
                        && members.len() <= 1_000_000
                        && gaps.len() + 1 == members.len(),
                    "invalid packed chunk arity",
                )?;
                require(
                    seq >= 0
                        && seq
                            .checked_add(members.len() as i64 - 1)
                            .is_some_and(|n| n <= 9_007_199_254_740_991),
                    "packed sequence overflow",
                )?;
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        let gap = gaps[index - 1]
                            .as_i64()
                            .ok_or_else(|| Error::Protocol("invalid chunk time delta".into()))?;
                        time = time
                            .checked_add(gap)
                            .ok_or_else(|| Error::Protocol("chunk time overflow".into()))?;
                    }
                    require(
                        (0..=9_007_199_254_740_991).contains(&time),
                        "invalid packed time",
                    )?;
                    let text = member
                        .as_str()
                        .ok_or_else(|| Error::Protocol("invalid chunk payload".into()))?;
                    let mut chunk = json!({"type":chunk_type,"index":data["index"]});
                    if payload_key == "texts" {
                        chunk["text"] = json!(text);
                    } else {
                        chunk["argumentsDelta"] = json!(text);
                        chunk["id"] = data["id"].clone();
                        if let Some(name) = data.get("name") {
                            chunk["name"] = name.clone();
                        }
                    }
                    let event: SessionEvent = serde_json::from_value(
                        json!({"type":"assistant/chunk","seq":seq+index as i64,"time":time,"data":{"turn":data["turn"],"step":data["step"],"chunk":chunk}}),
                    )?;
                    event.validate()?;
                    entries.push(HistoryEntry {
                        event,
                        view: None,
                        is_model_only: None,
                    });
                }
            }
            _ => return Err(Error::Protocol("unknown DSH history wrapper".into())),
        }
    }
    Ok(entries)
}
/// The optional assistant stream is process-local. Its revision/index never advances journal cursors.
#[derive(Default)]
pub(crate) struct AssistantStream {
    revision: i64,
    attempt: Option<Value>,
    blocks: std::collections::BTreeMap<i64, (String, String)>,
}
impl AssistantStream {
    pub(crate) fn open(&mut self, baseline: &Value) -> Result<Vec<Value>> {
        self.revision = integer(baseline, "revision")?;
        require(self.revision >= 0, "invalid assistant revision")?;
        self.attempt = baseline
            .get("activeAttempt")
            .filter(|v| !v.is_null())
            .cloned();
        self.blocks.clear();
        let Some(attempt) = self.attempt.clone() else {
            return Ok(Vec::new());
        };
        let mut count = 0;
        for record in array(&attempt, "stream")? {
            if record["type"] == "chunk" {
                self.accumulate(&record["chunk"])?;
                count += 1;
                continue;
            }
            let mut data = record
                .as_object()
                .cloned()
                .ok_or_else(|| Error::Protocol("invalid assistant baseline record".into()))?;
            let kind = data
                .remove("type")
                .and_then(|v| v.as_str().map(String::from))
                .ok_or_else(|| Error::Protocol("invalid assistant record type".into()))?;
            let time = data
                .remove("time0")
                .ok_or_else(|| Error::Protocol("missing assistant time".into()))?;
            data.insert("turn".into(), attempt["turn"].clone());
            data.insert("step".into(), attempt["step"].clone());
            for entry in decode_records(&[
                json!({"type":"chunks","event":{"type":format!("chunkrow/{kind}"),"seq":0,"time":time,"data":data}}),
            ])? {
                self.accumulate(&entry.event.data["chunk"])?;
                count += 1;
            }
        }
        require(
            count == integer(&attempt, "nextIndex")?,
            "assistant baseline index mismatch",
        )?;
        Ok(vec![self.presentation(&attempt, 0.0)])
    }
    fn accumulate(&mut self, chunk: &Value) -> Result<()> {
        let kind = string(chunk, "type")?;
        if matches!(kind, "text-delta" | "reasoning-delta") {
            let index = integer(chunk, "index")?;
            let block = self
                .blocks
                .entry(index)
                .or_insert_with(|| (kind.into(), String::new()));
            block.1.push_str(string(chunk, "text")?);
        } else if kind == "block-end" {
            let index = integer(chunk, "index")?;
            let block = &chunk["block"];
            let kind = string(block, "type")?;
            if matches!(kind, "text" | "reasoning") {
                self.blocks.insert(
                    index,
                    (format!("{kind}-delta"), string(block, "text")?.into()),
                );
            }
        }
        Ok(())
    }
    fn presentation(&self, attempt: &Value, time: f64) -> Value {
        let text = self
            .blocks
            .values()
            .filter(|(kind, _)| kind == "text-delta")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let reasoning = self
            .blocks
            .values()
            .filter(|(kind, _)| kind == "reasoning-delta")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("");
        json!({"type":"assistant/chunk","seq":0,"time":time,"data":{"turn":attempt["turn"],"step":attempt["step"],"presentation":{"text":text,"reasoning":reasoning},"chunk":{"type":"text-delta","text":""}}})
    }
    pub(crate) fn receive(&mut self, frame: &Value) -> Result<Vec<Value>> {
        let revision = integer(frame, "revision")?;
        require(revision == self.revision + 1, "assistant revision gap")?;
        self.revision = revision;
        match string(frame, "type")? {
            "start" => {
                require(self.attempt.is_none(), "overlapping assistant attempt")?;
                let mut attempt = frame.clone();
                attempt["nextIndex"] = json!(0);
                self.blocks.clear();
                self.attempt = Some(attempt.clone());
                Ok(vec![self.presentation(&attempt, 0.0)])
            }
            "chunk" => {
                let Some(mut attempt) = self.attempt.clone() else {
                    return Ok(Vec::new());
                };
                require(
                    frame["attemptId"] == attempt["attemptId"]
                        && integer(frame, "index")? == integer(&attempt, "nextIndex")?,
                    "assistant chunk identity/index mismatch",
                )?;
                self.accumulate(&frame["chunk"])?;
                attempt["nextIndex"] = json!(integer(&attempt, "nextIndex")? + 1);
                self.attempt = Some(attempt.clone());
                let time = frame["time"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .ok_or_else(|| Error::Protocol("invalid assistant time".into()))?;
                if frame["chunk"]["type"] != "block-end" {
                    Ok(vec![
                        json!({"type":"assistant/chunk","seq":0,"time":time,"data":{"turn":attempt["turn"],"step":attempt["step"],"chunk":frame["chunk"]}}),
                    ])
                } else {
                    Ok(vec![self.presentation(&attempt, time)])
                }
            }
            "end" => {
                let Some(attempt) = self.attempt.take() else {
                    return Ok(Vec::new());
                };
                require(
                    frame["attemptId"] == attempt["attemptId"]
                        && integer(frame, "index")? == integer(&attempt, "nextIndex")?,
                    "assistant end mismatch",
                )?;
                self.blocks.clear();
                Ok(vec![self.presentation(&attempt, 0.0)])
            }
            _ => Err(Error::Protocol("invalid assistant frame".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_log_only_events_and_plugin_records_survive_without_fake_surface_changes() {
        for kind in ["working-directory/change", "job/registered", "job/settled"] {
            let raw = json!({"type":"event","event":{"type":kind,"seq":2,"time":1,"data":{"opaque":"preserved"}}});
            let entries = decode_records(&[raw]).unwrap();
            assert_eq!(entries[0].event.kind, kind);
            assert_eq!(entries[0].event.data["opaque"], "preserved");
            assert_eq!(entries[0].event.ignorable, None);
        }
        let plugin = json!({"type":"event","event":{"type":"plugin:sample/state","seq":3,"time":1,"ignorable":true,"data":{"counter":4}}});
        assert_eq!(
            decode_records(&[plugin]).unwrap()[0].event.data["counter"],
            4
        );
        assert!(decode_records(&[json!({"type":"event","event":{"type":"unknown/required","seq":3,"time":1,"data":{}}})]).is_err());
    }
    #[test]
    fn packed_chunks_preserve_sequences_times_and_tool_identity() {
        let row = json!({"type":"chunks","event":{"type":"chunkrow/tool-call-chunks","seq":5,"time":100,"data":{"turn":1,"step":0,"index":2,"id":"tool","name":"read","args":["{","}"],"dt":[7]}}});
        let entries = decode_records(std::slice::from_ref(&row)).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.event.seq).collect::<Vec<_>>(),
            [5, 6]
        );
        assert_eq!(entries[1].event.time, 107.0);
        assert_eq!(entries[0].event.data["chunk"]["id"], "tool");
        let mut bad = row;
        bad["event"]["data"]["dt"] = json!([]);
        assert!(decode_records(&[bad]).is_err());
    }
    #[test]
    fn assistant_stream_rejects_gaps_without_creating_durable_events() {
        let mut stream = AssistantStream::default();
        stream.open(&json!({"revision":4})).unwrap();
        stream
            .receive(&json!({"type":"start","revision":5,"attemptId":"a","turn":1,"step":0}))
            .unwrap();
        let delta=stream.receive(&json!({"type":"chunk","revision":6,"attemptId":"a","index":0,"time":10,"chunk":{"type":"text-delta","index":0,"text":"hello"}})).unwrap();
        assert_eq!(delta[0]["data"]["chunk"]["text"], "hello");
        assert!(
            stream
                .receive(&json!({"type":"chunk","revision":8}))
                .is_err()
        );
        assert!(
            stream
                .receive(&json!({"type":"chunk","revision":7,"attemptId":"other","index":1}))
                .is_err()
        );
    }
}
