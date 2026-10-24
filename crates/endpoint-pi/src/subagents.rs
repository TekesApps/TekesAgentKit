//! Native Pi child sessions, owned by the adapter; no fork-as-subagent shortcut.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ChildAdmission {
    parent: String,
    child: String,
    label: Option<String>,
}
impl Context {
    async fn child_catalog(&self, parent: &str) -> Result<SubagentCatalog> {
        let manifest = self.manifest.lock().await;
        let available = manifest
            .sessions
            .get(parent)
            .is_some_and(|p| !p.archived && !p.ephemeral);
        let entries = if available {
            manifest
                .sessions
                .values()
                .filter(|c| c.subagent_parent.as_deref() == Some(parent) && !c.archived)
                .map(|c| SubagentEntry {
                    kind: "child".into(),
                    id: c.id.clone(),
                    mode: Some("continuable".into()),
                    plane: None,
                    label: c.title.clone(),
                    reason: None,
                    has_children: Some(
                        manifest
                            .sessions
                            .values()
                            .any(|v| v.subagent_parent.as_deref() == Some(&c.id) && !v.archived),
                    ),
                })
                .collect()
        } else {
            vec![]
        };
        Ok(SubagentCatalog {
            entries,
            parent_available: available,
        })
    }
    async fn owned_child(&self, parent: &str, child: &str) -> Result<Row> {
        require(parent != child, "invalid child identity")?;
        self.child_catalog(parent)
            .await?
            .require_continuable(child)?;
        self.row(child).await
    }
    async fn create_child(&self, payload: &Value, rpc_id: &str) -> Result<Value> {
        let parent = string(payload, "parentSessionId")?;
        let label = payload
            .get("label")
            .filter(|v| !v.is_null())
            .map(|v| v.as_str().ok_or_else(|| unsupported("invalid child label")))
            .transpose()?;
        require(
            label.is_none_or(|s| s.len() <= 512),
            "child label exceeds limit",
        )?;
        let key = payload["requestKey"].as_str().unwrap_or(rpc_id);
        require(
            !key.is_empty() && key.len() <= 512,
            "invalid child creation key",
        )?;
        let key = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([parent, key]))?)
        );
        // An active parent may have changed model through a native command. Inherit the
        // authoritative RPC selection; dormant parents use their saved selection/default.
        let rpc = self
            .processes
            .lock()
            .await
            .get(parent)
            .cloned()
            .filter(|r| r.alive());
        let live_selection = if let Some(rpc) = rpc {
            let state = rpc.request("get_state", json!({})).await?;
            state["model"]
                .is_object()
                .then(|| models::selection(&state))
        } else {
            None
        };
        let mut manifest = self.manifest.lock().await;
        let source = manifest
            .sessions
            .get(parent)
            .filter(|p| !p.archived && !p.ephemeral)
            .cloned()
            .ok_or_else(|| unsupported("subagent parent unavailable"))?;
        if let Some(admission) = manifest.child_admissions.get(&key) {
            require(
                admission.parent == parent && admission.label.as_deref() == label,
                "child creation content conflict",
            )?;
            require(
                manifest
                    .sessions
                    .get(&admission.child)
                    .is_some_and(|c| c.subagent_parent.as_deref() == Some(parent) && !c.archived),
                "created child unavailable",
            )?;
            return Ok(
                json!({"childSessionId":admission.child,"sessionId":admission.child,"duplicate":true}),
            );
        }
        let id = uuid::Uuid::new_v4().to_string();
        let row = Row {
            id: id.clone(),
            cwd: source.cwd,
            file: self
                .config
                .data_root
                .join("sessions")
                .join(format!("{id}.jsonl")),
            title: label.map(str::to_owned),
            archived: false,
            parent: None,
            subagent_parent: Some(parent.into()),
            model_selection: live_selection.or(source.model_selection),
            ephemeral: false,
            receipts: BTreeMap::new(),
            materialized: false,
            native_session_id: None,
            workspace_id: source.workspace_id,
            queue: vec![],
            control_revision: 0,
            goal_revision: 0,
            goal: None,
            feedback: BTreeMap::new(),
        };
        let mut next = manifest.clone();
        next.sessions.insert(id.clone(), row);
        next.child_admissions.insert(
            key,
            ChildAdmission {
                parent: parent.into(),
                child: id.clone(),
                label: label.map(str::to_owned),
            },
        );
        self.save(&next).await?;
        *manifest = next;
        drop(manifest);
        self.notify_row(&id).await;
        self.notify_workspaces().await;
        Ok(json!({"childSessionId":id,"sessionId":id}))
    }
    pub(crate) async fn subagent_call(
        self: &Arc<Self>,
        method: &str,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let parent = string(payload, "parentSessionId")?;
        if method == "subagents.list" {
            return Ok(serde_json::to_value(self.child_catalog(parent).await?)?);
        }
        if method == "subagents.create" {
            return self.create_child(payload, rpc_id).await;
        }
        let child = string(payload, "childSessionId")?;
        self.owned_child(parent, child).await?;
        let gate = self.gate(child).await;
        let _guard = gate
            .acquire_owned()
            .await
            .map_err(|_| Error::Disconnected)?;
        self.owned_child(parent, child).await?;
        match method {
            "subagents.prompt" => {
                let content = array(payload, "content")?;
                require(
                    !content.is_empty() && !content.iter().any(|v| v["type"] == "file"),
                    "SUBAGENT_FILE_UNSUPPORTED",
                )?;
                let mode = payload["mode"].as_str().unwrap_or("queue");
                require(matches!(mode, "queue" | "steer"), "invalid child delivery")?;
                let rpc = self.process(child).await?;
                // Validate/materialize before durable admission; only supported public prompt blocks.
                let materialized = self.materialize(child, content).await?;
                if mode == "steer" {
                    return self.direct_prompt(&rpc, &json!({"sessionId":child,"mode":"steer","content":content,"clientTimeZone":payload["clientTimeZone"]}), rpc_id).await;
                }
                let input = json!({"sessionId":child,"mode":"queue","content":content,"clientTimeZone":payload["clientTimeZone"]});
                let receipt = self.enqueue(&input, rpc_id).await?;
                drop(_guard);
                let context = self.clone();
                let child = child.to_owned();
                tokio::spawn(async move {
                    let _ = context.dispatch_next_if_current(&child, Some(rpc)).await;
                });
                // `file` blocks are rejected above: the general materialized file projection
                // does not grant subagents file support.
                let _ = materialized;
                Ok(receipt)
            }
            "subagents.cancel" => {
                self.armed_goals.lock().await.remove(child);
                let mut manifest = self.manifest.lock().await;
                let row = manifest.sessions.get_mut(child).expect("owned child");
                for item in &row.queue {
                    if let Some(receipt) = item["rpcId"]
                        .as_str()
                        .and_then(|id| row.receipts.get_mut(id))
                    {
                        receipt.cancelled = true;
                        receipt.queued = false;
                    }
                }
                row.queue.clear();
                self.save(&manifest).await?;
                drop(manifest);
                self.notify_queue(child).await;
                if let Some(rpc) = self
                    .processes
                    .lock()
                    .await
                    .get(child)
                    .cloned()
                    .filter(|r| r.alive())
                {
                    rpc.request("abort", json!({})).await?;
                }
                Ok(json!({"cancelled":true}))
            }
            "subagents.read" => {
                let rpc = self.process(child).await?;
                let state = rpc.request("get_state", json!({})).await?;
                let entries = rpc.request("get_entries", json!({})).await?;
                let pending = self.row(child).await?.queue.len();
                let busy = state["isStreaming"] == true
                    || pending != 0
                    || self.armed_goals.lock().await.contains(child);
                self.owned_child(parent, child).await?;
                let value = json!({"running":busy,"settled":!busy,"pendingInputs":pending,"entries":entries["entries"]});
                require(
                    serde_json::to_vec(&value)?.len() <= 4 * 1024 * 1024,
                    "child result exceeds limit",
                )?;
                Ok(value)
            }
            _ => Err(unsupported("unknown child operation")),
        }
    }
}

impl Context {
    pub(crate) async fn direct_prompt(
        &self,
        rpc: &Rpc,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let id = string(payload, "sessionId")?;

        require(!rpc_id.is_empty(), "prompt rpcId required")?;
        let materialized = self.materialize(id, array(payload, "content")?).await?;
        if payload["mode"] != "steer"
            && rpc.request("get_state", json!({})).await?["isStreaming"] == true
        {
            return self.enqueue(payload, rpc_id).await;
        }
        let digest = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
        let mut manifest = self.manifest.lock().await;
        let row = manifest.sessions.get_mut(id).expect("owned row");
        if let Some(receipt) = row.receipts.get(rpc_id) {
            require(receipt.digest == digest, "prompt identity content conflict")?;
            require(!receipt.cancelled, "prompt was cancelled; use a new rpcId")?;
            return if receipt.accepted {
                Ok(json!({"accepted":true,"duplicate":true,"queued":receipt.queued}))
            } else {
                Err(unsupported(
                    "uncertain Pi prompt; reconcile native history before retry",
                ))
            };
        }
        row.receipts.insert(
            rpc_id.into(),
            Receipt {
                digest,
                accepted: false,
                queued: false,
                cancelled: false,
            },
        );
        self.save(&manifest).await?;
        drop(manifest);
        rpc.control(
            "input.prepare",
            json!({"id":rpc_id,"text":materialized.message.clone(),"hasImages":!materialized.images.is_empty(),"files":materialized.files.clone()}),
        )
        .await?;
        let result = rpc
            .request(
                "prompt",
                json!({
                    "message":materialized.message,"images":materialized.images,
                    "streamingBehavior":if payload["mode"]=="steer" {"steer"} else {"followUp"}
                }),
            )
            .await?;
        let disposition = result["disposition"].as_str().unwrap_or("");
        require(
            matches!(disposition, "started" | "queued" | "handled"),
            "Pi did not admit prompt",
        )?;
        if disposition == "handled" {
            rpc.control("input.cancel", json!({"id":rpc_id})).await?;
        }
        let state = rpc.request("get_state", json!({})).await?;
        let mut manifest = self.manifest.lock().await;
        let row = manifest.sessions.get_mut(id).expect("owned row");
        if row.file.is_file() || matches!(disposition, "started" | "queued") {
            row.materialized = true;
            row.native_session_id = Some(string(&state, "sessionId")?.to_owned());
        }
        row.receipts.get_mut(rpc_id).expect("receipt").accepted = true;
        self.save(&manifest).await?;
        Ok(json!({"accepted":true,"native":result}))
    }
}

impl Context {
    pub(crate) fn handle_host_request(
        self: Arc<Self>,
        parent: String,
        rpc: Arc<Rpc>,
        request: Value,
    ) -> futures_util::future::BoxFuture<'static, ()> {
        Box::pin(async move {
            let current = self
                .processes
                .lock()
                .await
                .get(&parent)
                .is_some_and(|p| Arc::ptr_eq(p, &rpc));
            if !current {
                return;
            }
            let mut params = request["params"].clone();
            params["parentSessionId"] = json!(parent);
            let result = async {
                let method = string(&request, "method")?;
                if method == "subagents.prompt" {
                    let text = string(&params, "text")?;
                    require(
                        !text.is_empty() && text.len() <= 65536,
                        "invalid child input",
                    )?;
                    params["content"] = json!([{"type":"text","text":text}]);
                    params["mode"] = json!("queue");
                }
                self.subagent_call(method, &params, string(&request["params"], "rpcId")?)
                    .await
            }
            .await;
            let reply = match result {
                Ok(data) => json!({"requestId":request["requestId"],"success":true,"data":data}),
                Err(error) => {
                    json!({"requestId":request["requestId"],"success":false,"error":error.to_string()})
                }
            };
            let _ = rpc.control("host.reply", reply).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn child_creation_ownership_idempotency_and_fork_distinction_survive_restart() {
        let root = std::env::temp_dir().join(format!("agentkit-children-{}", uuid::Uuid::new_v4()));
        let config = PiConfiguration::new(PathBuf::from("/bin/false"), root.clone());
        let connector = PiConnector::new(config.clone()).await.unwrap();
        let root = root.canonicalize().unwrap();
        let parent = uuid::Uuid::new_v4().to_string();
        let other = uuid::Uuid::new_v4().to_string();
        let row:Row=serde_json::from_value(json!({"id":parent,"cwd":root,"file":root.join("sessions/parent.jsonl"),"title":null,"archived":false,"parent":null,"ephemeral":false,"receipts":{},"model_selection":{"provider":"p","model":"m"}})).unwrap();
        let mut alternate = row.clone();
        alternate.id = other.clone();
        let mut manifest = connector.context.manifest.lock().await;
        manifest.sessions.insert(parent.clone(), row);
        manifest.sessions.insert(other.clone(), alternate);
        connector.context.save(&manifest).await.unwrap();
        drop(manifest);
        let request = json!({"parentSessionId":parent,"label":"Child","requestKey":"stable"});
        let (first, replay) = tokio::join!(
            connector.context.create_child(&request, "rpc"),
            connector.context.create_child(&request, "rpc")
        );
        let first = first.unwrap();
        assert_eq!(first["childSessionId"], replay.unwrap()["childSessionId"]);
        let child = first["childSessionId"].as_str().unwrap().to_owned();
        let child_row = connector.context.row(&child).await.unwrap();
        assert_eq!(child_row.model_selection.unwrap()["model"], "m");
        assert!(child_row.parent.is_none());
        assert_eq!(child_row.subagent_parent.as_deref(), Some(parent.as_str()));
        assert!(
            connector
                .context
                .create_child(
                    &json!({"parentSessionId":parent,"label":"Conflict","requestKey":"stable"}),
                    "rpc"
                )
                .await
                .is_err()
        );
        let mut fork = connector.context.row(&child).await.unwrap();
        fork.id = uuid::Uuid::new_v4().to_string();
        fork.parent = Some(parent.clone());
        fork.subagent_parent = None;
        connector
            .context
            .manifest
            .lock()
            .await
            .sessions
            .insert(fork.id.clone(), fork);
        assert_eq!(
            connector
                .context
                .child_catalog(&parent)
                .await
                .unwrap()
                .entries
                .len(),
            1
        );
        assert!(connector.context.owned_child(&other, &child).await.is_err());
        assert!(
            connector
                .context
                .validate_pointer(&connector.context.row(&child).await.unwrap())
                .await
                .is_ok()
        );
        let input =
            json!({"sessionId":child,"mode":"queue","content":[{"type":"text","text":"pending"}]});
        connector
            .context
            .enqueue(&input, "cancel-me")
            .await
            .unwrap();
        connector
            .context
            .subagent_call(
                "subagents.cancel",
                &json!({"parentSessionId":parent,"childSessionId":child}),
                "cancel",
            )
            .await
            .unwrap();
        assert!(
            connector
                .context
                .enqueue(&input, "cancel-me")
                .await
                .is_err()
        );
        assert!(
            connector
                .context
                .row(&child)
                .await
                .unwrap()
                .queue
                .is_empty()
        );
        drop(connector);
        let restored = PiConnector::new(config).await.unwrap();
        assert_eq!(
            restored
                .context
                .create_child(&request, "rpc")
                .await
                .unwrap()["childSessionId"],
            child
        );
        let catalog = restored.context.child_catalog(&parent).await.unwrap();
        assert_eq!(catalog.entries[0].label.as_deref(), Some("Child"));
        restored
            .context
            .manifest
            .lock()
            .await
            .sessions
            .get_mut(&parent)
            .unwrap()
            .archived = true;
        assert!(
            !restored
                .context
                .child_catalog(&parent)
                .await
                .unwrap()
                .parent_available
        );
        assert!(restored.context.owned_child(&parent, &child).await.is_err());
        drop(restored);
        std::fs::remove_dir_all(root).unwrap();
    }
}
