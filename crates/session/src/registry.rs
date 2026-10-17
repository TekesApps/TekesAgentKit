use crate::SessionRuntime;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tekes_agent_contract::*;
/// A host-owned list of explicit endpoint connections. Session IDs are never resolved across endpoints.
#[derive(Default)]
pub struct SessionEndpointRegistry {
    endpoints: BTreeMap<String, SessionRuntime>,
}
impl SessionEndpointRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(
        &mut self,
        id: impl Into<String>,
        connector: Arc<dyn EndpointConnector>,
    ) -> Result<()> {
        let id = id.into();
        require(
            !id.is_empty() && !self.endpoints.contains_key(&id),
            "duplicate or empty endpoint identity",
        )?;
        self.endpoints
            .insert(id.clone(), SessionRuntime::spawn(id, connector));
        Ok(())
    }
    pub fn endpoint(&self, id: &str) -> Result<&SessionRuntime> {
        self.endpoints
            .get(id)
            .ok_or_else(|| unsupported("endpoint is not registered"))
    }
    pub fn session(&self, address: &SessionAddress) -> Result<&SessionRuntime> {
        self.endpoint(&address.endpoint_id)
    }
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let results = futures_util::future::join_all(
            self.endpoints
                .values()
                .map(|runtime| runtime.wait_ready(timeout)),
        )
        .await;
        for result in results {
            result?;
        }
        Ok(())
    }
    pub async fn open(&self, address: &SessionAddress) -> Result<()> {
        self.session(address)?
            .open_session(&address.session_id)
            .await
    }
    pub async fn locator(&self, address: &SessionAddress) -> Result<SessionLocator> {
        self.session(address)?.locator(&address.session_id).await
    }
    pub async fn file_read_all(
        &self,
        address: &SessionAddress,
        path: &str,
        maximum: usize,
    ) -> Result<Vec<u8>> {
        self.session(address)?
            .file_read_all(&address.session_id, path, maximum)
            .await
    }
    pub async fn stop(&self) -> Result<()> {
        let results =
            futures_util::future::join_all(self.endpoints.values().map(SessionRuntime::stop)).await;
        for result in results {
            result?;
        }
        Ok(())
    }
}
