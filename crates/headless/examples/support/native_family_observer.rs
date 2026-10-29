use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tekes_agent_contract::*;

pub fn monotonic_ns() -> u128 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_nanos()
}

#[derive(Clone, Debug)]
pub struct BoundedCall {
    pub connection_ordinal: u64,
    pub call_ordinal: u64,
    pub phase: String,
    pub intent_ns: u128,
    pub return_ns: u128,
    pub request: Value,
    pub response: std::result::Result<Value, Value>,
}

#[derive(Default)]
pub struct CallLog {
    next_call: AtomicU64,
    phase: Mutex<String>,
    calls: Mutex<Vec<BoundedCall>>,
}

impl CallLog {
    pub fn set_phase(&self, phase: impl Into<String>) {
        *self.phase.lock().expect("observer phase log poisoned") = phase.into();
    }

    pub fn drain(&self) -> Vec<BoundedCall> {
        std::mem::take(&mut *self.calls.lock().expect("observer source log poisoned"))
    }
}

pub struct ObservedConnector {
    pub inner: Arc<dyn EndpointConnector>,
    pub log: Arc<CallLog>,
    next_connection: AtomicU64,
    current_connection: AtomicU64,
}

pub fn validate_role_path(role: &str, path: &str, page20_enabled: bool) -> Result<()> {
    require(matches!(role, "baseline" | "candidate"), "unknown role")?;
    require(matches!(path, "common50" | "product20"), "unknown path")?;
    require(
        (path == "product20") == page20_enabled && (path != "product20" || role == "candidate"),
        "product20 requires candidate page20 feature",
    )
}

pub fn validate_page_cut(actual: i64, expected: i64) -> Result<()> {
    require(actual == expected, "history cut mismatch")
}

pub fn insert_raw_entry(
    union: &mut BTreeMap<i64, HistoryEntry>,
    entry: &HistoryEntry,
) -> Result<()> {
    if let Some(previous) = union.insert(entry.event.seq, entry.clone()) {
        require(previous == *entry, "same-sequence history content changed")?;
    }
    Ok(())
}

impl ObservedConnector {
    pub fn new(inner: Arc<dyn EndpointConnector>, log: Arc<CallLog>) -> Self {
        Self {
            inner,
            log,
            next_connection: AtomicU64::new(0),
            current_connection: AtomicU64::new(0),
        }
    }

    pub fn connection_ordinal(&self) -> u64 {
        self.current_connection.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl EndpointConnector for ObservedConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        let endpoint = self.inner.connect().await?;
        let ordinal = self.next_connection.fetch_add(1, Ordering::SeqCst) + 1;
        self.current_connection.store(ordinal, Ordering::SeqCst);
        Ok(Arc::new(ObservedEndpoint {
            inner: endpoint,
            log: self.log.clone(),
            connection_ordinal: ordinal,
        }))
    }
}

struct ObservedEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    log: Arc<CallLog>,
    connection_ordinal: u64,
}

#[async_trait::async_trait]
impl SessionEndpoint for ObservedEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }
    fn host(&self) -> &HostDescription {
        self.inner.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        self.inner.capabilities()
    }
    fn content_kind(&self) -> SessionContentKind {
        self.inner.content_kind()
    }
    fn supports_recovery(&self) -> bool {
        self.inner.supports_recovery()
    }
    async fn load_earlier_conversation(&self, session_id: &str, count: usize) -> Result<bool> {
        self.inner
            .load_earlier_conversation(session_id, count)
            .await
    }
    async fn locator(&self, session_id: &str) -> Result<SessionLocator> {
        self.inner.locator(session_id).await
    }
    async fn observe_files(&self, session_id: &str, path: &str) -> Result<FrameStream> {
        self.inner.observe_files(session_id, path).await
    }
    async fn file_stat(&self, session_id: &str, path: &str) -> Result<SessionFileStat> {
        self.inner.file_stat(session_id, path).await
    }
    async fn file_read_bytes(
        &self,
        session_id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        self.inner
            .file_read_bytes(session_id, path, offset, length)
            .await
    }
    async fn file_read_text(
        &self,
        session_id: &str,
        path: &str,
        offset: u64,
        limit: usize,
    ) -> Result<SessionFileTextPage> {
        self.inner
            .file_read_text(session_id, path, offset, limit)
            .await
    }
    fn supports_events(&self) -> bool {
        self.inner.supports_events()
    }
    async fn events(&self) -> Result<FrameStream> {
        self.inner.events().await
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }
    async fn respond(
        &self,
        actionable_id: &str,
        expected_revision: i64,
        outcome: Value,
    ) -> Result<()> {
        self.inner
            .respond(actionable_id, expected_revision, outcome)
            .await
    }
    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        let call_ordinal = if method == "subagents.history" {
            self.log.next_call.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            0
        };
        let intent_ns = monotonic_ns();
        let request = serde_json::json!({"method":method,"payload":payload,"rpcId":rpc_id});
        let phase = self
            .log
            .phase
            .lock()
            .expect("observer phase log poisoned")
            .clone();
        let result = self.inner.call(method, payload, rpc_id).await;
        let return_ns = monotonic_ns();
        if method == "subagents.history" {
            let response = match &result {
                Ok(value) => Ok(value.clone()),
                Err(Error::Protocol(message)) => {
                    Err(serde_json::json!({"kind":"protocol","message":message}))
                }
                Err(Error::Transport(message)) => {
                    Err(serde_json::json!({"kind":"transport","message":message}))
                }
                Err(Error::Remote(value)) => {
                    Err(serde_json::json!({"kind":"remote","value":value}))
                }
                Err(Error::Timeout) => Err(serde_json::json!({"kind":"timeout"})),
                Err(Error::Disconnected) => Err(serde_json::json!({"kind":"disconnected"})),
                Err(Error::Unsupported(message)) => {
                    Err(serde_json::json!({"kind":"unsupported","message":message}))
                }
                Err(Error::Json(error)) => {
                    Err(serde_json::json!({"kind":"json","message":error.to_string()}))
                }
            };
            {
                self.log
                    .calls
                    .lock()
                    .expect("observer source log poisoned")
                    .push(BoundedCall {
                        connection_ordinal: self.connection_ordinal,
                        call_ordinal,
                        phase,
                        intent_ns,
                        return_ns,
                        request,
                        response,
                    });
            }
        }
        result
    }
    async fn close(&self) {
        self.inner.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    struct FakeEndpoint {
        host: HostDescription,
        returned: Value,
    }
    #[async_trait::async_trait]
    impl SessionEndpoint for FakeEndpoint {
        fn identity(&self) -> &str {
            "native-test"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        async fn follow(&self, _: StreamTarget) -> Result<FrameStream> {
            Err(unsupported("test"))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("test"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Ok(())
        }
        async fn call(&self, _: &str, _: Value, _: &str) -> Result<Value> {
            Ok(self.returned.clone())
        }
        async fn close(&self) {}
    }

    #[tokio::test]
    async fn response_is_forwarded_unchanged_and_logged_verbatim() {
        let expected = json!({"parentSessionId":"p","childSessionId":"c","throughSequence":7,"historyRevision":"a".repeat(64),"entries":[{"event":{"seq":7,"type":"turn/end","time":1,"data":{}}}]});
        let host = HostDescription {
            protocol_version: 4,
            product: Product {
                name: "Native".into(),
                version: "0.9.3".into(),
            },
            capabilities: BTreeSet::new(),
            cwd: "/tmp".into(),
            home: "/tmp".into(),
            provider: None,
            model: None,
            attached_sessions: 0,
            can_open_path: false,
        };
        let log = Arc::new(CallLog::default());
        log.set_phase("test");
        let endpoint = ObservedEndpoint {
            inner: Arc::new(FakeEndpoint {
                host,
                returned: expected.clone(),
            }),
            log: log.clone(),
            connection_ordinal: 1,
        };
        let actual = endpoint
            .call("subagents.history", json!({"maxMessages":20}), "rpc")
            .await
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(log.drain()[0].response.as_ref().unwrap(), &expected);
    }

    #[test]
    fn role_path_configuration_is_strict() {
        assert!(validate_role_path("baseline", "common50", false).is_ok());
        assert!(validate_role_path("candidate", "product20", true).is_ok());
        assert!(validate_role_path("baseline", "product20", true).is_err());
        assert!(validate_role_path("candidate", "product20", false).is_err());
        assert!(validate_role_path("candidate", "common50", false).is_ok());
    }

    #[test]
    fn same_identity_with_changed_content_and_wrong_cut_are_not_equal() {
        let original: HistoryEntry = serde_json::from_value(
            json!({"event":{"seq":4,"type":"turn/end","time":1,"data":{"text":"old"}}}),
        )
        .unwrap();
        let changed: HistoryEntry = serde_json::from_value(
            json!({"event":{"seq":4,"type":"turn/end","time":1,"data":{"text":"new"}}}),
        )
        .unwrap();
        let mut union = BTreeMap::new();
        insert_raw_entry(&mut union, &original).unwrap();
        assert!(insert_raw_entry(&mut union, &changed).is_err());
        assert!(validate_page_cut(5, 4).is_err());
        assert!(validate_page_cut(4, 4).is_ok());
    }
}
