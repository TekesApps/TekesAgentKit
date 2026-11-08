//! Search the registered Pi sessions through public get_entries responses.
//! A title match never starts a process; an unmaterialized session stays so.
use super::*;
impl PiSessionEndpoint {
    pub(crate) async fn search_sessions_value(&self, payload: &Value) -> Result<Value> {
        let query = string(payload, "query")?.trim().to_lowercase();
        require(query.len() <= 4096, "search query limit")?;
        if query.is_empty() {
            return Ok(json!({"items":[],"hasMore":false}));
        }
        let rows: Vec<_> = self
            .context
            .manifest
            .lock()
            .await
            .sessions
            .values()
            .filter(|row| !row.archived && !row.ephemeral)
            .cloned()
            .collect();
        // The existing inventory's updatedAt is the registered native file's mtime.
        let mut rows: Vec<_> = rows
            .into_iter()
            .map(|row| {
                (
                    std::fs::metadata(&row.file)
                        .and_then(|meta| meta.modified())
                        .unwrap_or(UNIX_EPOCH),
                    row,
                )
            })
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let mut items = Vec::new();
        for (_, row) in rows {
            let snippet = if let Some(title) = row
                .title
                .as_ref()
                .filter(|title| title.to_lowercase().contains(&query))
            {
                Some(title.clone())
            } else if row.file.exists() {
                let rpc = self.context.process(&row.id).await?;
                let entries = rpc.request("get_entries", json!({})).await?;
                let (branch, _) = active_branch(&entries, usize::MAX)?;
                timeline_texts(&branch)
                    .iter()
                    .find_map(|text| excerpt(text, &query))
            } else {
                None
            };
            if let Some(snippet) = snippet {
                let title = row.title.clone().unwrap_or_else(|| row.id.clone());
                items.push(json!({"sessionId":row.id,"snippet":snippet,"title":title,"label":title,"cwd":row.cwd}));
                if items.len() > 100 {
                    break;
                }
            }
        }
        Ok(super::extensions::bounded_search_results(items, 100))
    }
}
fn timeline_texts(branch: &[Value]) -> Vec<String> {
    let mut calls = BTreeSet::new();
    let mut texts = vec![];
    for entry in branch.iter().filter(|entry| entry["type"] == "message") {
        let message = &entry["message"];
        let blocks = message["content"].as_array().cloned().unwrap_or_default();
        if message["role"] == "assistant" {
            calls.extend(
                blocks
                    .iter()
                    .filter(|block| block["type"] == "toolCall")
                    .filter_map(|block| block["id"].as_str())
                    .map(str::to_owned),
            );
        }
        if message["role"] == "toolResult"
            && message["toolCallId"]
                .as_str()
                .is_some_and(|id| calls.contains(id))
        {
            continue;
        }
        let text = message["content"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        if !text.is_empty() {
            texts.push(text);
        }
    }
    texts
}
fn excerpt(text: &str, query: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let byte = lower.find(query)?;
    let index = lower[..byte].chars().count();
    // Lowercasing may expand a Unicode scalar. Keep offsets tied to the
    // original text rather than slicing it using offsets into the lowercase copy.
    let chars: Vec<_> = text.chars().collect();
    let map: Vec<_> = chars
        .iter()
        .enumerate()
        .flat_map(|(index, ch)| ch.to_lowercase().map(move |_| index))
        .collect();
    let first = map[index];
    let last = map[index + query.chars().count() - 1];
    Some(
        chars[first.saturating_sub(80)..(last + 1 + 160).min(chars.len())]
            .iter()
            .collect(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_uses_visible_message_text_not_native_metadata_reasoning_or_associated_tool_results() {
        let rows = vec![
            json!({"type":"message","id":"opaque-metadata-needle","message":{"role":"assistant","model":"model-needle","content":[{"type":"text","text":"正文 中文"},{"type":"thinking","thinking":"reasoning-only"},{"type":"toolCall","id":"call","name":"tool-name"}]}}),
            json!({"type":"message","message":{"role":"toolResult","toolCallId":"call","content":[{"text":"associated tool result"}]}}),
            json!({"type":"message","message":{"role":"user","content":"另一条输入"}}),
        ];
        assert_eq!(timeline_texts(&rows), vec!["正文 中文", "另一条输入"]);
    }
    #[test]
    fn snippet_contains_a_deep_match_and_maps_case_expansion_back_to_original_utf8() {
        let text = format!("{}中文 👩🏽‍💻 命中{}", "前".repeat(400), "后".repeat(400));
        let snippet = excerpt(&text, "👩🏽‍💻 命中").unwrap();
        assert!(snippet.contains("中文 👩🏽‍💻 命中"));
        assert!(snippet.chars().count() < 260);
        assert_eq!(excerpt("İ 中文", "i\u{307}"), Some("İ 中文".into()));
        assert!(excerpt("正文", "absent").is_none());
    }
}
