//! Public conversation-search extension result. Keep provider additions lossless.
use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchHit {
    pub session_id: String,
    pub snippet: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchResults {
    pub items: Vec<SessionSearchHit>,
    /// Required: missing is not evidence that a bounded result is exhaustive.
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl SessionSearchResults {
    pub fn read(value: Value) -> Result<Self> {
        let result: Self = serde_json::from_value(value)?;
        require(
            result.items.iter().all(|hit| !hit.session_id.is_empty()),
            "empty search session identity",
        )?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn search_results_require_explicit_completion_and_preserve_provider_fields() {
        let raw = json!({"items":[{"sessionId":"same","snippet":"中文 👩🏽‍💻","title":"extra title"}],"hasMore":true,"cursor":"opaque"});
        let result = SessionSearchResults::read(raw.clone()).unwrap();
        assert!(result.has_more);
        assert_eq!(result.items[0].snippet, "中文 👩🏽‍💻");
        assert_eq!(serde_json::to_value(&result).unwrap(), raw);
        assert!(SessionSearchResults::read(json!({"items":[]})).is_err());
        assert!(SessionSearchResults::read(json!({"items":[],"hasMore":null})).is_err());
    }
    #[test]
    fn malformed_search_results_never_become_successful_empty_results() {
        for raw in [
            json!({"items":[{"sessionId":"","snippet":"text"}],"hasMore":false}),
            json!({"items":[{"sessionId":"a"}],"hasMore":false}),
            json!({"items":{},"hasMore":false}),
        ] {
            assert!(SessionSearchResults::read(raw).is_err());
        }
    }
}
