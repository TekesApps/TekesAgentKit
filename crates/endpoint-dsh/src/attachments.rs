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
        .filter(|entry| entry.event.kind == "user/message")
        .filter_map(|entry| entry.event.data["content"].as_array())
        .flat_map(|content| content.iter())
        .find(|part| part["type"] == "image" && part["attachment"]["attachmentId"] == id)
        .map(|part| part["attachment"].clone())
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
    fn backwards_search_requires_progress_and_rejects_empty_continuing_page() {
        assert_eq!(earlier_cursor(&page(3, true), Some(8)).unwrap(), 3);
        assert!(earlier_cursor(&page(3, true), Some(3)).is_err());
        let mut empty = page(3, true);
        empty.entries.clear();
        assert!(earlier_cursor(&empty, Some(8)).is_err());
    }
}
