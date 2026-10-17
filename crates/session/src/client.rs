use crate::SessionRuntime;
use serde_json::{Value, json};
use tekes_agent_contract::*;
impl SessionRuntime {
    pub async fn capabilities(&self) -> Result<EndpointCapabilities> {
        Ok(serde_json::from_value(
            self.call("endpoint.capabilities", json!({}), "capabilities")
                .await?,
        )?)
    }
    /// Checks the concrete endpoint/host method authority before invoking an optional extension.
    pub async fn call_optional(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        if !self.capabilities().await?.supports(method) {
            return Err(unsupported(method));
        }
        self.call(method, payload, rpc_id).await
    }
    pub async fn create(
        &self,
        request: CreateSessionRequest,
        rpc_id: &str,
    ) -> Result<SessionCreated> {
        Ok(serde_json::from_value(
            self.call_optional("session.create", serde_json::to_value(request)?, rpc_id)
                .await?,
        )?)
    }
    pub async fn submit(&self, request: PromptRequest, rpc_id: &str) -> Result<Value> {
        self.call_optional("session.prompt", serde_json::to_value(request)?, rpc_id)
            .await
    }
    pub async fn fork(&self, request: ForkRequest, rpc_id: &str) -> Result<SessionCreated> {
        request.validate()?;
        Ok(serde_json::from_value(
            self.call_optional("session.fork", serde_json::to_value(request)?, rpc_id)
                .await?,
        )?)
    }
    pub async fn discard(&self, id: &str, rpc_id: &str) -> Result<()> {
        self.call_optional("session.discard", json!({"sessionId":id}), rpc_id)
            .await?;
        Ok(())
    }
    pub async fn rename(&self, id: &str, title: &str, rpc_id: &str) -> Result<Value> {
        self.call_optional(
            "session.rename",
            json!({"sessionId":id,"title":title}),
            rpc_id,
        )
        .await
    }
    pub async fn archive(&self, id: &str, rpc_id: &str) -> Result<Value> {
        self.call_optional("session.archive", json!({"sessionId":id}), rpc_id)
            .await
    }
    pub async fn unarchive(&self, id: &str, rpc_id: &str) -> Result<Value> {
        self.call_optional("session.unarchive", json!({"sessionId":id}), rpc_id)
            .await
    }
    pub async fn update_queue(
        &self,
        id: &str,
        item: &str,
        action: QueueAction,
        rpc_id: &str,
    ) -> Result<Value> {
        self.call_optional(
            "session.updateQueue",
            json!({"sessionId":id,"itemId":item,"action":action}),
            rpc_id,
        )
        .await
    }
    pub async fn upload_bytes(
        &self,
        id: &str,
        name: &str,
        media_type: &str,
        data: &[u8],
        rpc_id: &str,
    ) -> Result<UploadReceipt> {
        use base64::Engine;
        require(
            !data.is_empty() && data.len() <= 8 * 1024 * 1024,
            "upload limit",
        )?;
        let value=self.call_optional("session.uploadFile",json!({"sessionId":id,"name":name,"mediaType":media_type,"data":base64::engine::general_purpose::STANDARD.encode(data)}),rpc_id).await?;
        let receipt: UploadReceipt = serde_json::from_value(value)?;
        require(
            receipt.file.bytes == data.len() as u64 && !receipt.receipt_id.is_empty(),
            "upload receipt mismatch",
        )?;
        Ok(receipt)
    }
    pub async fn attachment_policy(&self, id: Option<&str>) -> Result<AttachmentPolicy> {
        Ok(serde_json::from_value(
            self.call_optional(
                "attachments.policy",
                json!({"sessionId":id}),
                "attachment-policy",
            )
            .await?,
        )?)
    }
    pub async fn goal(&self, id: &str) -> Result<Option<GoalView>> {
        let value = self
            .call_optional("goals.get", json!({"sessionId":id}), "goal-read")
            .await?;
        Ok(serde_json::from_value(
            value.get("goal").cloned().unwrap_or(value),
        )?)
    }
}
