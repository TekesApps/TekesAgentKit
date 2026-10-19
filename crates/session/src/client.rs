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
            !data.is_empty() && data.len() <= MAX_UPLOAD_BYTES,
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

impl SessionRuntime {
    pub async fn subagent_catalog(&self, parent: &str) -> Result<SubagentCatalog> {
        require(!parent.is_empty(), "empty parent session")?;
        Ok(serde_json::from_value(
            self.call_optional(
                "subagents.list",
                json!({"parentSessionId":parent}),
                "subagent-catalog",
            )
            .await?,
        )?)
    }
    /// Re-read ownership immediately before control. Never infer permission from a cached panel.
    pub async fn prompt_child(
        &self,
        parent: &str,
        child: &str,
        mode: PromptMode,
        content: Vec<Value>,
        rpc_id: &str,
    ) -> Result<Value> {
        require(
            parent != child && !child.is_empty(),
            "invalid child identity",
        )?;
        require(
            !content.iter().any(|v| v["type"] == "file"),
            "SUBAGENT_FILE_UNSUPPORTED",
        )?;
        self.subagent_catalog(parent)
            .await?
            .require_continuable(child)?;
        self.call_optional(
            "subagents.prompt",
            json!({"parentSessionId":parent,"childSessionId":child,"mode":mode,"content":content}),
            rpc_id,
        )
        .await
    }
    pub async fn cancel_child(&self, parent: &str, child: &str, rpc_id: &str) -> Result<()> {
        require(
            parent != child && !child.is_empty(),
            "invalid child identity",
        )?;
        self.subagent_catalog(parent)
            .await?
            .require_continuable(child)?;
        self.call_optional(
            "subagents.cancel",
            json!({"parentSessionId":parent,"childSessionId":child}),
            rpc_id,
        )
        .await?;
        Ok(())
    }
}

impl SessionRuntime {
    pub async fn create_child(
        &self,
        parent: &str,
        label: Option<&str>,
        request_key: &str,
    ) -> Result<SessionCreated> {
        let value = self
            .call_optional(
                "subagents.create",
                json!({"parentSessionId":parent,"label":label,"requestKey":request_key}),
                request_key,
            )
            .await?;
        let id = string(&value, "childSessionId")?;
        require(
            !id.is_empty() && id != parent,
            "invalid child creation receipt",
        )?;
        Ok(SessionCreated {
            session_id: id.into(),
        })
    }
    pub async fn read_child(&self, parent: &str, child: &str) -> Result<Value> {
        self.subagent_catalog(parent)
            .await?
            .require_continuable(child)?;
        self.call_optional(
            "subagents.read",
            json!({"parentSessionId":parent,"childSessionId":child}),
            "child-read",
        )
        .await
    }
    pub async fn bounded_child_history(
        &self,
        request: BoundedChildHistoryRequest,
    ) -> Result<BoundedChildHistoryPage> {
        request.validate()?;
        let epoch = self.inspect(|s| s.connection_epoch);
        let value = self
            .call_optional(
                "subagents.history",
                serde_json::to_value(&request)?,
                "bounded-child-history",
            )
            .await?;
        let page: BoundedChildHistoryPage = serde_json::from_value(value)?;
        page.validate(&request)?;
        require(
            self.inspect(|s| s.connection_epoch == epoch),
            "bounded history connection changed during read",
        )?;
        Ok(page)
    }
}
