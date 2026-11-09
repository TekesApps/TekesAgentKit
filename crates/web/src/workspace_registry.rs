//! Host-injected Workspace Service composition, independent of SessionRuntime.
use crate::*;
use serde::Serialize;

/// Public identities only. The embedding host owns the service URL, filesystem
/// root, credentials and grants; none are accepted from a browser request.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegistration {
    pub connection_id: String,
    pub workspace_id: String,
    pub title: String,
}

#[derive(Default)]
pub(crate) struct WorkspaceServices {
    connections: BTreeMap<String, Arc<dyn WorkspaceServiceEndpoint>>,
    registrations: BTreeMap<(String, String), WorkspaceRegistration>,
}

impl SharedAgent {
    /// Called by an embedding host before sharing this agent. It is not a wire
    /// operation: a browser cannot install a transport or change its authority.
    pub fn register_workspace_service(
        &mut self,
        service: Arc<dyn WorkspaceServiceEndpoint>,
    ) -> Result<()> {
        let id = service.connection_id().to_owned();
        require(!id.is_empty(), "empty workspace connection identity")?;
        require(
            !self.workspace_services.connections.contains_key(&id),
            "duplicate workspace connection identity",
        )?;
        self.workspace_services.connections.insert(id, service);
        Ok(())
    }

    /// Expose only workspaces explicitly registered by the owning host. A
    /// workspace ID is a server identity, never a user-selected filesystem root.
    pub fn register_workspace(
        &mut self,
        connection_id: &str,
        workspace_id: &str,
        title: &str,
    ) -> Result<()> {
        require(
            self.workspace_services
                .connections
                .contains_key(connection_id),
            "unknown workspace connection",
        )?;
        require(
            !workspace_id.is_empty() && !title.is_empty(),
            "invalid workspace registration",
        )?;
        let key = (connection_id.to_owned(), workspace_id.to_owned());
        require(
            !self.workspace_services.registrations.contains_key(&key),
            "duplicate workspace registration",
        )?;
        self.workspace_services.registrations.insert(
            key,
            WorkspaceRegistration {
                connection_id: connection_id.into(),
                workspace_id: workspace_id.into(),
                title: title.into(),
            },
        );
        Ok(())
    }

    pub fn workspace_inventory(&self) -> Value {
        json!({"workspaces":self.workspace_services.registrations.values().collect::<Vec<_>>()})
    }

    pub(crate) async fn workspace_request(&self, request: &Value) -> Result<Value> {
        let object = request
            .as_object()
            .ok_or_else(|| Error::Protocol("workspace request must be an object".into()))?;
        if request["op"] == "workspace.list" {
            require(
                object.keys().all(|key| matches!(key.as_str(), "id" | "op")),
                "unknown workspace list field",
            )?;
            return Ok(self.workspace_inventory());
        }
        require(
            object.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "id" | "op" | "connectionId" | "workspaceId" | "method" | "request" | "rpcId"
                )
            }),
            "unknown workspace call field",
        )?;
        let connection_id = string(request, "connectionId")?;
        let workspace_id = string(request, "workspaceId")?;
        require(
            self.workspace_services
                .registrations
                .contains_key(&(connection_id.into(), workspace_id.into())),
            "workspace is not registered for this connection",
        )?;
        let method: WorkspaceMethod = serde_json::from_value(request["method"].clone())?;
        let mut payload = request["request"]
            .as_object()
            .ok_or_else(|| Error::Protocol("workspace request body must be an object".into()))?
            .clone();
        require(
            !payload.contains_key("workspaceId"),
            "workspace identity must use the registered address",
        )?;
        if method != WorkspaceMethod::Capabilities {
            payload.insert("workspaceId".into(), json!(workspace_id));
        }
        let payload = Value::Object(payload);
        validate_workspace_request(method, &payload)?;
        let rpc_id = string(request, "rpcId")?;
        require(
            !rpc_id.is_empty() && rpc_id.len() <= 4096,
            "invalid workspace RPC identity",
        )?;
        let service = self
            .workspace_services
            .connections
            .get(connection_id)
            .ok_or_else(|| unsupported("workspace connection"))?;
        match service.call(method, payload, rpc_id).await {
            Ok(value) => Ok(json!({"result":value})),
            Err(Error::Remote(error)) => Ok(json!({"error":error})),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Service {
        calls: Mutex<Vec<(WorkspaceMethod, Value, String)>>,
    }
    #[async_trait]
    impl WorkspaceServiceEndpoint for Service {
        fn connection_id(&self) -> &str {
            "host-connection"
        }
        async fn call(
            &self,
            method: WorkspaceMethod,
            payload: Value,
            rpc_id: &str,
        ) -> Result<Value> {
            self.calls
                .lock()
                .unwrap()
                .push((method, payload.clone(), rpc_id.into()));
            if method == WorkspaceMethod::FilesWrite {
                Err(Error::Remote(
                    json!({"code":"permission-denied","message":"Host has not granted writes"}),
                ))
            } else {
                Ok(
                    json!({"workspaceId":payload["workspaceId"],"path":"","entries":[],"truncated":false}),
                )
            }
        }
    }
    fn agent() -> (SharedAgent, Arc<Service>) {
        let service = Arc::new(Service {
            calls: Mutex::new(Vec::new()),
        });
        let mut agent = SharedAgent::default();
        agent.register_workspace_service(service.clone()).unwrap();
        agent
            .register_workspace("host-connection", "workspace", "Project")
            .unwrap();
        (agent, service)
    }
    fn call(method: &str, body: Value) -> Value {
        json!({"op":"workspace.call","connectionId":"host-connection","workspaceId":"workspace","method":method,"request":body,"rpcId":"one-operation"})
    }

    #[tokio::test]
    async fn public_consumer_uses_registered_identity_and_preserves_host_denial() {
        let (agent, service) = agent();
        assert_eq!(
            agent
                .request(&json!({"op":"workspace.list"}))
                .await
                .unwrap(),
            json!({"workspaces":[{"connectionId":"host-connection","workspaceId":"workspace","title":"Project"}]})
        );
        let read = agent
            .request(&call("filesList", json!({"path":""})))
            .await
            .unwrap();
        assert_eq!(read["result"]["workspaceId"], "workspace");
        let saved = agent
            .request(&call(
                "filesWrite",
                json!({"path":"proof","content":"YWJj","expectedRevision":"old"}),
            ))
            .await
            .unwrap();
        assert_eq!(saved["error"]["code"], "permission-denied");
        let calls = service.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, json!({"workspaceId":"workspace","path":""}));
        assert_eq!(calls[1].2, "one-operation");
    }

    #[tokio::test]
    async fn browser_cannot_register_or_override_workspace_authority() {
        let (agent, service) = agent();
        for (key, value) in [
            ("connectionId", "other"),
            ("workspaceId", "other"),
            ("method", "workspace.register"),
        ] {
            let mut request = call("filesList", json!({"path":""}));
            request[key] = json!(value);
            assert!(agent.request(&request).await.is_err());
        }
        for key in ["root", "url", "authorization", "workspaceId"] {
            let mut body = json!({"path":""});
            body[key] = json!("other");
            assert!(agent.request(&call("filesList", body)).await.is_err());
        }
        assert!(service.calls.lock().unwrap().is_empty());
    }
}
