use super::*;
impl Context {
    pub(crate) async fn gate(&self, id: &str) -> Arc<tokio::sync::Semaphore> {
        self.gates
            .lock()
            .await
            .entry(id.into())
            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
    }
    async fn notify_queue(&self, id: &str) {
        if let Ok(row) = self.row(id).await {
            let _ = self
                .notifications
                .send(json!({"queue":{"sessionId":id,"items":row.queue}}));
        }
    }
    pub(crate) async fn enqueue(&self, payload: &Value, rpc_id: &str) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let digest = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown session"))?;
        if let Some(receipt) = row.receipts.get(rpc_id) {
            require(receipt.digest == digest, "prompt identity content conflict")?;
            require(
                receipt.accepted && !receipt.cancelled,
                "uncertain or cancelled prompt requires a new intent",
            )?;
            return Ok(json!({"accepted":true,"queued":receipt.queued,"duplicate":true}));
        }
        let item_id = uuid::Uuid::new_v4().to_string();
        row.queue.push(json!({"id":item_id,"rpcId":rpc_id,"placement":"queued","createdAt":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()*1000.0,"message":{"role":"user","content":payload["content"]},"payload":payload}));
        row.receipts.insert(
            rpc_id.into(),
            Receipt {
                digest,
                accepted: true,
                queued: true,
                cancelled: false,
            },
        );
        self.save(&manifest).await?;
        drop(manifest);
        self.notify_queue(id).await;
        Ok(json!({"accepted":true,"queued":true,"itemId":item_id}))
    }
    pub(crate) async fn update_queue(&self, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let item_id = string(payload, "itemId")?;
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown session"))?;
        let index = row
            .queue
            .iter()
            .position(|item| item["id"] == item_id)
            .ok_or_else(|| unsupported("queue item not found"))?;
        require(
            row.queue[index]["placement"] == "queued",
            "dispatching input outcome is uncertain",
        )?;
        let rpc_id = string(&row.queue[index], "rpcId")?.to_owned();
        match string(&payload["action"], "kind")? {
            "remove" => {
                row.queue.remove(index);
                let receipt = row.receipts.get_mut(&rpc_id).expect("queue receipt");
                receipt.cancelled = true;
                receipt.queued = false;
            }
            "edit" => {
                let content = array(&payload["action"], "content")?;
                require(
                    !content.is_empty()
                        && content
                            .iter()
                            .all(|part| part["type"] == "text" && part["text"].is_string()),
                    "queue edit requires text content",
                )?;
                let attachments: Vec<_> = array(&row.queue[index]["message"], "content")?
                    .iter()
                    .filter(|part| part["type"] != "text")
                    .cloned()
                    .collect();
                let mut next = content.clone();
                next.extend(attachments);
                row.queue[index]["message"]["content"] = json!(next);
                row.queue[index]["payload"]["content"] = json!(next);
                let digest = format!(
                    "{:x}",
                    Sha256::digest(row.queue[index]["payload"].to_string().as_bytes())
                );
                row.receipts.get_mut(&rpc_id).expect("queue receipt").digest = digest;
            }
            _ => return Err(unsupported("invalid queue action")),
        }
        self.save(&manifest).await?;
        drop(manifest);
        self.notify_queue(id).await;
        Ok(Value::Null)
    }
    pub(crate) async fn dispatch_next(self: &Arc<Self>, id: &str) -> Result<()> {
        self.dispatch_next_if_current(id, None).await
    }
    pub(crate) async fn dispatch_next_if_current(
        self: &Arc<Self>,
        id: &str,
        expected: Option<Arc<Rpc>>,
    ) -> Result<()> {
        let _guard = self
            .gate(id)
            .await
            .acquire_owned()
            .await
            .map_err(|_| Error::Disconnected)?;
        let rpc = self
            .processes
            .lock()
            .await
            .get(id)
            .cloned()
            .filter(|rpc| rpc.alive())
            .ok_or(Error::Disconnected)?;
        if let Some(expected) = expected
            && !Arc::ptr_eq(&expected, &rpc)
        {
            return Ok(());
        }
        if rpc.request("get_state", json!({})).await?["isStreaming"] != false {
            return Ok(());
        }
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown session"))?;
        let Some(first) = row.queue.first_mut() else {
            return Ok(());
        };
        require(
            first["placement"] == "queued",
            "uncertain queue dispatch requires reconciliation",
        )?;
        first["placement"] = json!("dispatching");
        let item = first.clone();
        self.save(&manifest).await?;
        drop(manifest);
        self.notify_queue(id).await;
        let (message, images) = self
            .materialize(id, array(&item["message"], "content")?)
            .await?;
        // Receipt is already persisted as dispatching. A timeout is never converted back into queued.
        let result = rpc
            .request("prompt", json!({"message":message,"images":images}))
            .await?;
        require(
            matches!(result["disposition"].as_str(), Some("started" | "handled")),
            "unexpected queue dispatch",
        )?;
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown session"))?;
        row.queue.retain(|current| current["id"] != item["id"]);
        if let Some(receipt) = row.receipts.get_mut(string(&item, "rpcId")?) {
            receipt.queued = false;
        }
        self.save(&manifest).await?;
        drop(manifest);
        self.notify_queue(id).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn queue_cancellation_cannot_be_readmitted_and_data_root_has_one_owner() {
        let root = std::env::temp_dir().join(format!("agentkit-queue-{}", uuid::Uuid::new_v4()));
        let config = PiConfiguration::new(PathBuf::from("/bin/false"), root.clone());
        let connector = PiConnector::new(config.clone()).await.unwrap();
        assert!(PiConnector::new(config).await.is_err());
        let id = uuid::Uuid::new_v4().to_string();
        let row = Row {
            id: id.clone(),
            cwd: root.clone(),
            file: root.join("sessions/test.jsonl"),
            title: None,
            archived: false,
            parent: None,
            ephemeral: false,
            receipts: BTreeMap::new(),
            workspace_id: None,
            queue: Vec::new(),
            control_revision: 0,
            goal_revision: 0,
            goal: None,
            feedback: BTreeMap::new(),
        };
        connector
            .context
            .manifest
            .lock()
            .await
            .sessions
            .insert(id.clone(), row);
        let payload =
            json!({"sessionId":id,"mode":"queue","content":[{"type":"text","text":"input"}]});
        let receipt = connector.context.enqueue(&payload, "key").await.unwrap();
        connector
            .context
            .update_queue(
                &json!({"sessionId":id,"itemId":receipt["itemId"],"action":{"kind":"remove"}}),
            )
            .await
            .unwrap();
        assert!(connector.context.enqueue(&payload, "key").await.is_err());
        assert!(connector.context.row(&id).await.unwrap().queue.is_empty());
        drop(connector);
        std::fs::remove_dir_all(root).unwrap();
    }
}
