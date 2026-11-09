//! Resolve legacy image IDs through bounded, passive public history reads.
use super::*;

impl DshSessionEndpoint {
    pub(super) async fn read_attachment(&self, payload: Value, rpc_id: &str) -> Result<Value> {
        let session_id = string(&payload, "sessionId")?.to_owned();
        let modern = self.host.product.version == "0.2.2-rc.1";
        let attachment = if modern && payload.get("attachment").is_none() {
            let id = string(&payload, "attachmentId")?;
            Some(
                tokio::time::timeout(
                    Duration::from_secs(30),
                    self.find_image_reference(&session_id, id),
                )
                .await
                .map_err(|_| Error::Timeout)??,
            )
        } else {
            payload.get("attachment").cloned()
        };
        let request = attachment_request(&payload, attachment.as_ref(), modern)?;
        self.remote
            .call("session/attachment", json!({"request":request}), rpc_id)
            .await
    }

    async fn find_image_reference(&self, session_id: &str, attachment_id: &str) -> Result<Value> {
        let address = self.history_address(session_id).await?;
        let mut opening = self
            .remote
            .open(
                "session/follow",
                json!({"request":{"address":address,"maxMessages":32}}),
            )
            .await?;
        let first = opening.next().await.ok_or(Error::Disconnected)??;
        drop(opening);
        require(
            first["type"] == "snapshot",
            "attachment history needs opening snapshot",
        )?;
        let cut = integer(&first, "cursor")?;
        let mut request = HistoryRequest {
            session_id: session_id.into(),
            through_sequence: cut,
            before_sequence: None,
            max_messages: 32,
        };
        let mut page = HistoryPage {
            session_id: session_id.into(),
            through_sequence: cut,
            entries: decode_records(array(&first, "records")?)?,
            has_more_before: boolean(&first, "hasMore")?,
        };
        loop {
            page.validate(&request)?;
            if let Some(reference) = image_reference(&page.entries, attachment_id) {
                return Ok(reference);
            }
            if !page.has_more_before {
                return Err(Error::Protocol(
                    "image attachment not found in public session history".into(),
                ));
            }
            request.before_sequence = Some(earlier_cursor(&page, request.before_sequence)?);
            page = self.history_page(request.clone()).await?;
        }
    }
}

fn attachment_request(payload: &Value, reference: Option<&Value>, modern: bool) -> Result<Value> {
    let session_id = string(payload, "sessionId")?;
    let id = match reference {
        Some(reference) => string(reference, "attachmentId")?,
        None => string(payload, "attachmentId")?,
    };
    if let Some(requested) = payload.get("attachmentId") {
        require(requested == id, "attachment reference identity mismatch")?;
    }
    if modern {
        let reference = reference
            .ok_or_else(|| Error::Protocol("missing image attachment reference".into()))?;
        Ok(json!({"sessionId":session_id,"attachment":reference}))
    } else {
        Ok(json!({"sessionId":session_id,"attachmentId":id}))
    }
}

fn image_reference(entries: &[HistoryEntry], id: &str) -> Option<Value> {
    entries
        .iter()
        .rev()
        .find_map(|entry| image_in_event(&entry.event, id))
        .cloned()
}

// Match DSH's former imageInEvent reader: only declared first-party content
// fields carry attachment authority; arbitrary JSON in event data stays opaque.
fn image_in_event<'a>(event: &'a SessionEvent, id: &str) -> Option<&'a Value> {
    let data = &event.data;
    match event.kind.as_str() {
        "user/message" | "tool/ptc-dispatch" => return image_in_content(&data["content"], id),
        "system/message" | "developer/message" | "tool/result" | "team/message/queued" => {
            return image_in_content(&data["message"]["content"], id);
        }
        "agent/inbox/spliced" => {
            return data["inserted"]
                .as_array()?
                .iter()
                .find_map(|message| image_in_content(&message["content"], id));
        }
        "compaction/summary" => {
            return image_in_content(&data["summary"], id)
                .or_else(|| image_in_content(&data["rawOutput"], id));
        }
        "assistant/message" => {
            if let Some(reference) = image_in_content(&data["message"]["content"], id) {
                return Some(reference);
            }
        }
        "assistant/attempt" => {}
        _ => return None,
    }
    // assistantStreamChunks(stream, "block-end") reads only raw chunk records;
    // packed text runs and other chunks cannot declare completed image blocks.
    data["stream"].as_array()?.iter().find_map(|record| {
        if record["type"] == "chunk" && record["chunk"]["type"] == "block-end" {
            image_in_block(&record["chunk"]["block"], id)
        } else {
            None
        }
    })
}

fn image_in_content<'a>(content: &'a Value, id: &str) -> Option<&'a Value> {
    content
        .as_array()?
        .iter()
        .find_map(|block| image_in_block(block, id))
}

fn image_in_block<'a>(block: &'a Value, id: &str) -> Option<&'a Value> {
    (block["type"] == "image"
        && block["attachment"].is_object()
        && block["attachment"]["attachmentId"] == id)
        .then_some(&block["attachment"])
}

fn earlier_cursor(page: &HistoryPage, before: Option<i64>) -> Result<i64> {
    let next = page
        .entries
        .first()
        .map(|entry| entry.event.seq)
        .ok_or_else(|| Error::Protocol("empty continuing attachment history page".into()))?;
    require(
        next >= 0 && before.is_none_or(|before| next < before),
        "attachment history page made no progress",
    )?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference() -> Value {
        json!({"attachmentId":"image-id","mediaType":"image/png","bytes":68,"width":1,"height":1})
    }
    fn page(seq: i64, has_more: bool) -> HistoryPage {
        HistoryPage { session_id:"s".into(), through_sequence:10,
            entries:decode_records(&[json!({"type":"event","event":{"type":"user/message","seq":seq,"time":1,"data":{"content":[{"type":"image","attachment":reference()}],"source":{"kind":"user"}}}})]).unwrap(),
            has_more_before:has_more }
    }
    #[test]
    fn resolves_legacy_id_from_public_image_record_and_preserves_new_reference() {
        let page = page(3, false);
        let reference = image_reference(&page.entries, "image-id").unwrap();
        let payload = json!({"sessionId":"s","attachmentId":"image-id"});
        assert_eq!(
            attachment_request(&payload, Some(&reference), true).unwrap(),
            json!({"sessionId":"s","attachment":reference})
        );
        assert_eq!(
            attachment_request(
                &json!({"sessionId":"s","attachment":reference}),
                Some(&reference),
                false
            )
            .unwrap(),
            payload
        );
        assert!(image_reference(&page.entries, "missing").is_none());
        assert!(
            attachment_request(
                &json!({"sessionId":"s","attachmentId":"other"}),
                Some(&reference),
                true
            )
            .is_err()
        );
    }
    #[test]
    fn legacy_id_reader_preserves_all_declared_first_party_image_fields() {
        let image = json!({"type":"image","attachment":reference()});
        let cases = [
            ("user/message", json!({"content":[image.clone()]})),
            ("tool/ptc-dispatch", json!({"content":[image.clone()]})),
            (
                "system/message",
                json!({"message":{"content":[image.clone()]}}),
            ),
            (
                "developer/message",
                json!({"message":{"content":[image.clone()]}}),
            ),
            (
                "tool/result",
                json!({"message":{"content":[image.clone()]}}),
            ),
            (
                "team/message/queued",
                json!({"message":{"content":[image.clone()]}}),
            ),
            (
                "agent/inbox/spliced",
                json!({"inserted":[{"content":[]},{"content":[image.clone()]}]}),
            ),
            ("compaction/summary", json!({"summary":[image.clone()]})),
            (
                "compaction/summary",
                json!({"summary":[],"rawOutput":[image.clone()]}),
            ),
            (
                "assistant/message",
                json!({"message":{"content":[image.clone()]}}),
            ),
            (
                "assistant/message",
                json!({"message":{"content":[]},"stream":[{"type":"chunk","chunk":{"type":"block-end","block":image.clone()}}]}),
            ),
            (
                "assistant/attempt",
                json!({"stream":[{"type":"text-chunks","texts":["prefix"],"dt":[]},{"type":"chunk","chunk":{"type":"block-end","block":image.clone()}}]}),
            ),
        ];
        for (kind, data) in cases {
            let entries = decode_records(&[
                json!({"type":"event","event":{"type":kind,"seq":3,"time":1,"data":data}}),
            ])
            .unwrap();
            assert_eq!(
                image_reference(&entries, "image-id"),
                Some(reference()),
                "{kind}"
            );
        }
    }

    #[test]
    fn image_lookup_does_not_scan_unknown_json_or_unfinished_assistant_chunks() {
        let image = json!({"type":"image","attachment":reference()});
        let cases = [
            (
                "tool/result",
                json!({"result":{"nested":{"content":[image.clone()]}}}),
            ),
            (
                "user/message",
                json!({"content":[{"type":"text","data":{"content":[image.clone()]}}]}),
            ),
            (
                "assistant/message",
                json!({"stream":[{"type":"chunk","chunk":{"type":"block-start","block":image.clone()}}]}),
            ),
            (
                "assistant/attempt",
                json!({"stream":[{"type":"block-end","block":image.clone()}]}),
            ),
            (
                "plugin:opaque",
                json!({"content":[image.clone()],"message":{"content":[image.clone()]}}),
            ),
        ];
        for (kind, data) in cases {
            let entries = decode_records(&[json!({"type":"event","event":{"type":kind,"seq":3,"time":1,"data":data,"ignorable":true}})]).unwrap();
            assert!(image_reference(&entries, "image-id").is_none(), "{kind}");
        }
    }

    #[test]
    fn backwards_search_requires_progress_and_rejects_empty_continuing_page() {
        assert_eq!(earlier_cursor(&page(3, true), Some(8)).unwrap(), 3);
        assert!(earlier_cursor(&page(3, true), Some(3)).is_err());
        let mut empty = page(3, true);
        empty.entries.clear();
        assert!(earlier_cursor(&empty, Some(8)).is_err());
    }
}
