use crate::*;
use std::sync::Arc;
/// Actual optional method authority for one connection, separate from the synchronization wire.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointCapabilities {
    pub methods: BTreeSet<String>,
    pub supports_archive_session: bool,
    pub supports_unarchive_session: bool,
    pub supports_ephemeral_fork: bool,
    pub can_fork_completed_turn: bool,
    pub can_edit_historical_input: bool,
    pub supports_file_observation: bool,
    /// `respond` accepts the canonical question-only cancellation outcome
    /// `{sessionId, cancelQuestion: true}`. This does not cancel the Run.
    /// Adapters with another wire shape must normalize it before advertising this.
    #[serde(default)]
    pub supports_question_cancellation: bool,
}
impl EndpointCapabilities {
    pub fn methods(methods: &[&str]) -> Self {
        Self {
            methods: methods.iter().map(|s| (*s).into()).collect(),
            ..Self::default()
        }
    }
    pub fn supports(&self, method: &str) -> bool {
        self.methods.contains(method)
    }
}
/// OS/application-owned services (credentials, installation, plugin packages, Git, permissions).
/// Methods are callable only when explicitly advertised by the supplied backend.
#[async_trait]
pub trait HostServices: Send + Sync {
    fn methods(&self) -> BTreeSet<String>;
    async fn call(
        &self,
        endpoint_id: &str,
        method: &str,
        payload: Value,
        rpc_id: &str,
    ) -> Result<Value>;
}
/// Public adapter composition: hosts can add product capabilities without forking any endpoint.
pub struct HostedConnector {
    pub endpoint: Arc<dyn EndpointConnector>,
    pub services: Arc<dyn HostServices>,
}
#[async_trait]
impl EndpointConnector for HostedConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(HostedEndpoint {
            endpoint: self.endpoint.connect().await?,
            services: self.services.clone(),
        }))
    }
}
struct HostedEndpoint {
    endpoint: Arc<dyn SessionEndpoint>,
    services: Arc<dyn HostServices>,
}
#[async_trait]
impl SessionEndpoint for HostedEndpoint {
    fn identity(&self) -> &str {
        self.endpoint.identity()
    }
    fn host(&self) -> &HostDescription {
        self.endpoint.host()
    }
    fn capabilities(&self) -> EndpointCapabilities {
        let mut caps = self.endpoint.capabilities();
        caps.methods.extend(self.services.methods());
        caps
    }
    fn sequenced_queue_projection_key(&self) -> Option<&'static str> {
        self.endpoint.sequenced_queue_projection_key()
    }
    fn content_kind(&self) -> SessionContentKind {
        self.endpoint.content_kind()
    }
    fn supports_recovery(&self) -> bool {
        self.endpoint.supports_recovery()
    }
    fn supports_events(&self) -> bool {
        self.endpoint.supports_events()
    }
    async fn events(&self) -> Result<FrameStream> {
        self.endpoint.events().await
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.endpoint.follow(target).await
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.endpoint.history_page(request).await
    }
    async fn load_earlier_conversation(&self, id: &str, count: usize) -> Result<bool> {
        self.endpoint.load_earlier_conversation(id, count).await
    }
    async fn locator(&self, id: &str) -> Result<SessionLocator> {
        self.endpoint.locator(id).await
    }
    async fn observe_files(&self, id: &str, path: &str) -> Result<FrameStream> {
        self.endpoint.observe_files(id, path).await
    }
    async fn file_stat(&self, id: &str, path: &str) -> Result<SessionFileStat> {
        self.endpoint.file_stat(id, path).await
    }
    async fn file_read_bytes(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        self.endpoint
            .file_read_bytes(id, path, offset, length)
            .await
    }
    async fn file_read_text(
        &self,
        id: &str,
        path: &str,
        offset: u64,
        limit: usize,
    ) -> Result<SessionFileTextPage> {
        self.endpoint.file_read_text(id, path, offset, limit).await
    }
    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        self.endpoint.respond(id, revision, outcome).await
    }
    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        if self.services.methods().contains(method) {
            self.services
                .call(self.identity(), method, payload, rpc_id)
                .await
        } else {
            self.endpoint.call(method, payload, rpc_id).await
        }
    }
    async fn close(&self) {
        self.endpoint.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_capability_documents_do_not_advertise_question_cancellation() {
        let mut document = serde_json::to_value(EndpointCapabilities::default()).unwrap();
        document
            .as_object_mut()
            .unwrap()
            .remove("supportsQuestionCancellation");
        let restored: EndpointCapabilities = serde_json::from_value(document).unwrap();
        assert!(!restored.supports_question_cancellation);
    }
}
