//! Normalize public harness content into Agent-owned facts, without parsing Markdown.
use crate::RunScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TranscriptBlockContent {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    /// References the existing, scope-qualified tool DTO. A streaming reference
    /// may precede that DTO; absence is explicit, never a fabricated tool result.
    ToolCall {
        tool_id: String,
    },
    Attachment {
        content: Value,
    },
    /// Preserve unsupported/malformed public blocks; do not silently drop data.
    Unknown {
        content: Value,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptContentBlock {
    /// Position in this message's public content, not identity or a reading anchor.
    pub index: usize,
    pub content: TranscriptBlockContent,
}
#[derive(Clone, Copy)]
pub(crate) enum ContentDialect {
    Journal,
    Native,
}
pub(crate) fn content_blocks(
    scope: &RunScope,
    value: &Value,
    dialect: ContentDialect,
) -> Option<Vec<TranscriptContentBlock>> {
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return Some(vec![TranscriptContentBlock {
            index: 0,
            content: TranscriptBlockContent::Text { text: text.into() },
        }]);
    }
    let values: Vec<_> = value
        .as_array()
        .map(|values| values.iter().collect())
        .unwrap_or_else(|| vec![value]);
    Some(
        values
            .into_iter()
            .enumerate()
            .map(|(index, raw)| {
                let kind = raw["type"].as_str().unwrap_or("");
                let content = match kind {
                    "text" => raw["text"]
                        .as_str()
                        .map(|text| TranscriptBlockContent::Text { text: text.into() }),
                    "reasoning" if matches!(dialect, ContentDialect::Journal) => raw["text"]
                        .as_str()
                        .map(|text| TranscriptBlockContent::Reasoning { text: text.into() }),
                    "thinking" if matches!(dialect, ContentDialect::Native) => raw["thinking"]
                        .as_str()
                        .map(|text| TranscriptBlockContent::Reasoning { text: text.into() }),
                    "tool-call" if matches!(dialect, ContentDialect::Journal) => tool(scope, raw),
                    "toolCall" if matches!(dialect, ContentDialect::Native) => tool(scope, raw),
                    "image" | "file" | "audio" => Some(TranscriptBlockContent::Attachment {
                        content: raw.clone(),
                    }),
                    _ => None,
                }
                .unwrap_or_else(|| TranscriptBlockContent::Unknown {
                    content: raw.clone(),
                });
                TranscriptContentBlock { index, content }
            })
            .collect(),
    )
}
fn tool(scope: &RunScope, raw: &Value) -> Option<TranscriptBlockContent> {
    raw["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(|id| TranscriptBlockContent::ToolCall {
            tool_id: crate::transcript::scoped(scope, &format!("tool:{id}")),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn public_block_order_and_unknown_payloads_are_retained_without_markdown_parsing() {
        let scope = RunScope::main("e", "a");
        let opaque = json!({"type":"future-card","version":3,"payload":{"label":"保留"}});
        let source = json!([
            {"type":"text","text":"前文\n\n```rust\nfn main() {}\n```"},
            {"type":"reasoning","text":"思考"},
            {"type":"tool-call","id":"c","name":"read","arguments":{}},
            {"type":"text","text":"后文"},
            {"type":"image","mimeType":"image/png","data":"AA=="}, opaque
        ]);
        let blocks = content_blocks(&scope, &source, ContentDialect::Journal).unwrap();
        assert_eq!(
            blocks.iter().map(|b| b.index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5]
        );
        assert!(
            matches!(&blocks[0].content,TranscriptBlockContent::Text{text} if text.contains("```rust"))
        );
        assert_eq!(
            blocks[1].content,
            TranscriptBlockContent::Reasoning {
                text: "思考".into()
            }
        );
        assert_eq!(
            blocks[2].content,
            TranscriptBlockContent::ToolCall {
                tool_id: crate::transcript::scoped(&scope, "tool:c")
            }
        );
        assert_eq!(
            blocks[3].content,
            TranscriptBlockContent::Text {
                text: "后文".into()
            }
        );
        assert_eq!(
            blocks[4].content,
            TranscriptBlockContent::Attachment {
                content: source[4].clone()
            }
        );
        assert_eq!(
            blocks[5].content,
            TranscriptBlockContent::Unknown { content: opaque }
        );
        let other = content_blocks(
            &RunScope::main("other", "a"),
            &source,
            ContentDialect::Journal,
        )
        .unwrap();
        assert_ne!(blocks[2].content, other[2].content);
    }
    #[test]
    fn native_thinking_and_tool_call_are_normalized_and_malformed_blocks_stay_opaque() {
        let scope = RunScope::main("e", "a");
        let source = json!([{"type":"thinking","thinking":"思考"},{"type":"toolCall","id":"c"},{"type":"text","text":123},{"type":"toolCall","id":""}]);
        let blocks = content_blocks(&scope, &source, ContentDialect::Native).unwrap();
        assert!(
            matches!(&blocks[0].content,TranscriptBlockContent::Reasoning{text} if text=="思考")
        );
        assert!(matches!(
            &blocks[1].content,
            TranscriptBlockContent::ToolCall { .. }
        ));
        assert_eq!(
            blocks[2].content,
            TranscriptBlockContent::Unknown {
                content: source[2].clone()
            }
        );
        assert_eq!(
            blocks[3].content,
            TranscriptBlockContent::Unknown {
                content: source[3].clone()
            }
        );
        let wrong_dialect = content_blocks(&scope, &source, ContentDialect::Journal).unwrap();
        assert!(matches!(
            &wrong_dialect[0].content,
            TranscriptBlockContent::Unknown { .. }
        ));
    }
    #[test]
    fn absent_content_is_distinct_from_known_empty_and_plain_string_content() {
        let scope = RunScope::main("e", "a");
        assert!(content_blocks(&scope, &Value::Null, ContentDialect::Native).is_none());
        assert!(
            content_blocks(&scope, &json!([]), ContentDialect::Native)
                .unwrap()
                .is_empty()
        );
        let plain = content_blocks(&scope, &json!(""), ContentDialect::Native).unwrap();
        assert_eq!(plain.len(), 1);
        assert_eq!(
            plain[0].content,
            TranscriptBlockContent::Text {
                text: String::new()
            }
        );
    }
}
