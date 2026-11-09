//! Ranked title search through the Harness public authority. This is not body search.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThreadSearchVisibility {
    Active,
    Archived,
    All,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadSearchRequest {
    pub connection_epoch: u64,
    pub query: String,
    pub limit: usize,
    pub visibility: ThreadSearchVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadSearchMatchClass {
    Exact,
    Prefix,
    Tokens,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSearchItem {
    pub session_id: String,
    pub title: String,
    pub updated_at: String,
    pub archived: bool,
    pub as_of_seq: u64,
    pub score: u16,
    pub match_class: ThreadSearchMatchClass,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSearchPage {
    pub endpoint_id: String,
    pub connection_epoch: u64,
    pub query: String,
    pub visibility: ThreadSearchVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub items: Vec<ThreadSearchItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub reached_end: bool,
    pub catalog_digest: String,
}
#[derive(Deserialize)]
struct KernelSearchItem {
    session_id: String,
    title: String,
    updated_at: String,
    archived: bool,
    as_of_seq: u64,
    score: u16,
    match_class: ThreadSearchMatchClass,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KernelSearchPage {
    format: u8,
    results: Vec<KernelSearchItem>,
    next_cursor: Option<String>,
    reached_end: bool,
    catalog_digest: String,
}

impl SharedAgent {
    pub(crate) async fn thread_search(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let request: ThreadSearchRequest = serde_json::from_value(payload.clone())?;
        require(
            (1..=50).contains(&request.limit),
            "title search limit must be between 1 and 50",
        )?;
        require(
            !request.query.trim().is_empty() && request.query.len() <= 512,
            "title search query must contain 1 to 512 bytes",
        )?;
        if let Some(id) = &request.session_id {
            require(!id.is_empty(), "empty title search session filter")?;
        }
        let validate = || {
            shared.runtime.inspect(|state| {
                require(
                    state.connection == ConnectionState::Ready
                        && state.connection_epoch == request.connection_epoch
                        && state.endpoint_capabilities.supports("thread.search"),
                    "title search source unavailable or changed",
                )
            })
        };
        validate()?;
        let mut upstream =
            json!({"query":request.query,"limit":request.limit,"visibility":request.visibility});
        if let Some(after) = &request.after {
            upstream["after"] = json!(after);
        }
        if let Some(id) = &request.session_id {
            upstream["sessionId"] = json!(id);
        }
        let raw = shared
            .runtime
            .call_at_epoch(request.connection_epoch, "thread.search", upstream, rpc_id)
            .await?;
        validate()?;
        serde_json::to_value(normalize_page(
            &shared.runtime.snapshot().endpoint_id,
            &request,
            raw,
        )?)
        .map_err(Into::into)
    }
}

fn normalize_page(
    endpoint: &str,
    request: &ThreadSearchRequest,
    raw: Value,
) -> Result<ThreadSearchPage> {
    let page: KernelSearchPage = serde_json::from_value(raw)?;
    require(page.format == 1, "unknown title search result format")?;
    require(
        page.catalog_digest.len() == 71
            && page.catalog_digest.starts_with("sha256-")
            && page.catalog_digest[7..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid title search catalog digest",
    )?;
    require(
        page.results.len() <= request.limit
            && match (&page.next_cursor, page.reached_end) {
                (None, true) => true,
                (Some(cursor), false) => !cursor.is_empty() && !page.results.is_empty(),
                _ => false,
            },
        "inconsistent title search pagination",
    )?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::with_capacity(page.results.len());
    for item in page.results {
        require(
            !item.session_id.is_empty() && seen.insert(item.session_id.clone()),
            "empty or duplicate title search identity",
        )?;
        require(
            matches!(
                (item.match_class, item.score),
                (ThreadSearchMatchClass::Exact, 3000)
                    | (ThreadSearchMatchClass::Prefix, 2000)
                    | (ThreadSearchMatchClass::Tokens, 1000)
            ),
            "invalid title search match score",
        )?;
        require(
            match request.visibility {
                ThreadSearchVisibility::Active => !item.archived,
                ThreadSearchVisibility::Archived => item.archived,
                ThreadSearchVisibility::All => true,
            } && request
                .session_id
                .as_ref()
                .is_none_or(|id| id == &item.session_id),
            "title search result is outside the requested scope",
        )?;
        items.push(ThreadSearchItem {
            session_id: item.session_id,
            title: item.title,
            updated_at: item.updated_at,
            archived: item.archived,
            as_of_seq: item.as_of_seq,
            score: item.score,
            match_class: item.match_class,
        });
    }
    Ok(ThreadSearchPage {
        endpoint_id: endpoint.into(),
        connection_epoch: request.connection_epoch,
        query: request.query.clone(),
        visibility: request.visibility,
        session_id: request.session_id.clone(),
        items,
        next_cursor: page.next_cursor,
        reached_end: page.reached_end,
        catalog_digest: page.catalog_digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ThreadSearchRequest {
        ThreadSearchRequest {
            connection_epoch: 4,
            query: "设计".into(),
            limit: 25,
            visibility: ThreadSearchVisibility::All,
            after: None,
            session_id: None,
        }
    }
    fn page() -> Value {
        json!({"format":1,"results":[
            {"session_id":"018f0000-0000-7000-8000-000000000001","title":"设计","updated_at":"2026-11-09T00:00:00.000Z","archived":true,"as_of_seq":12,"score":3000,"match_class":"exact"},
            {"session_id":"018f0000-0000-7000-8000-000000000002","title":"设计 review","updated_at":"2026-11-08T00:00:00.000Z","archived":false,"as_of_seq":8,"score":2000,"match_class":"prefix"}
        ],"nextCursor":"opaque.cursor","reachedEnd":false,"catalogDigest":format!("sha256-{}", "a".repeat(64))})
    }
    #[test]
    fn kernel_title_results_are_typed_without_reordering_or_body_search_fields() {
        let normalized = normalize_page("native", &request(), page()).unwrap();
        assert_eq!(normalized.items[0].title, "设计");
        assert!(normalized.items[0].archived);
        assert_eq!(normalized.items[1].score, 2000);
        assert_eq!(normalized.next_cursor.as_deref(), Some("opaque.cursor"));
        let public = serde_json::to_value(normalized).unwrap();
        assert_eq!(public["endpointId"], "native");
        assert_eq!(public["connectionEpoch"], 4);
        assert_eq!(
            public["items"][0]["sessionId"],
            "018f0000-0000-7000-8000-000000000001"
        );
        assert!(public["items"][0].get("snippet").is_none());
        assert!(public["items"][0].get("session_id").is_none());
    }
    #[test]
    fn malformed_completion_duplicate_and_out_of_scope_results_are_rejected() {
        let mut missing = page();
        missing.as_object_mut().unwrap().remove("reachedEnd");
        assert!(normalize_page("native", &request(), missing).is_err());
        let mut cursor = page();
        cursor["reachedEnd"] = json!(true);
        assert!(normalize_page("native", &request(), cursor).is_err());
        let mut duplicate = page();
        duplicate["results"][1] = duplicate["results"][0].clone();
        assert!(normalize_page("native", &request(), duplicate).is_err());
        let mut filtered = request();
        filtered.visibility = ThreadSearchVisibility::Active;
        assert!(normalize_page("native", &filtered, page()).is_err());
        filtered.visibility = ThreadSearchVisibility::All;
        filtered.session_id = Some("other".into());
        assert!(normalize_page("native", &filtered, page()).is_err());
    }
    #[derive(Clone)]
    struct SearchFixture {
        host: HostDescription,
        supported: bool,
        calls: Arc<std::sync::Mutex<Vec<(Value, String)>>>,
        fail: Arc<std::sync::atomic::AtomicBool>,
    }
    #[async_trait]
    impl EndpointConnector for SearchFixture {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(self.clone()))
        }
    }
    #[async_trait]
    impl SessionEndpoint for SearchFixture {
        fn identity(&self) -> &str {
            "titles"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            if self.supported {
                EndpointCapabilities::methods(&["thread.search"])
            } else {
                EndpointCapabilities::default()
            }
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            use futures_util::stream;
            let frame = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
                _ => return Err(unsupported("title fixture stream")),
            };
            Ok(Box::pin(
                stream::once(async { Ok(frame) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("title fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("title fixture response"))
        }
        async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
            require(method == "thread.search", "unexpected title fixture method")?;
            self.calls.lock().unwrap().push((payload, rpc_id.into()));
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                Err(Error::Remote(
                    json!({"code":"cursor-stale","message":"Catalog changed"}),
                ))
            } else {
                Ok(page())
            }
        }
        async fn close(&self) {}
    }
    #[tokio::test]
    async fn title_reads_check_capability_epoch_and_propagate_cursor_errors_without_retry() {
        for supported in [false, true] {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let fixture = SearchFixture {
                host: serde_json::from_value(json!({"protocolVersion":4,"product":{"name":"title fixture","version":"1"},"capabilities":["session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false})).unwrap(),
                supported,
                calls: calls.clone(),
                fail: fail.clone(),
            };
            let mut agent = SharedAgent::default();
            agent
                .register(
                    "titles".into(),
                    "tekes".into(),
                    "Titles".into(),
                    Arc::new(fixture),
                )
                .unwrap();
            let shared = agent.endpoint("titles").unwrap();
            shared
                .runtime
                .wait_ready(std::time::Duration::from_secs(2))
                .await
                .unwrap();
            let mut query = request();
            query.connection_epoch = shared.runtime.snapshot().connection_epoch;
            query.after = Some("opaque.cursor".into());
            let payload = serde_json::to_value(&query).unwrap();
            if supported {
                let mut stale = payload.clone();
                stale["connectionEpoch"] = json!(query.connection_epoch + 1);
                assert!(agent.thread_search(&shared, &stale, "stale").await.is_err());
                assert!(calls.lock().unwrap().is_empty());
                let result = agent
                    .thread_search(&shared, &payload, "read-once")
                    .await
                    .unwrap();
                assert_eq!(result["endpointId"], "titles");
                assert_eq!(result["items"][0]["title"], "设计");
                assert_eq!(
                    calls.lock().unwrap()[0],
                    (
                        json!({"query":"设计","limit":25,"visibility":"all","after":"opaque.cursor"}),
                        "read-once".into()
                    )
                );
                fail.store(true, std::sync::atomic::Ordering::SeqCst);
                assert!(
                    matches!(agent.thread_search(&shared, &payload, "stale-cursor").await, Err(Error::Remote(value)) if value["code"] == "cursor-stale")
                );
                assert_eq!(calls.lock().unwrap().len(), 2);
            } else {
                assert!(
                    agent
                        .thread_search(&shared, &payload, "unsupported")
                        .await
                        .is_err()
                );
                assert!(calls.lock().unwrap().is_empty());
            }
            shared.runtime.stop().await.unwrap();
        }
    }
}
