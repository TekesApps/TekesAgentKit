use super::*;
impl Context {
    pub(crate) async fn workspace_snapshot(&self) -> Value {
        let manifest = self.manifest.lock().await;
        let items: Vec<_> = manifest
            .workspaces
            .values()
            .map(|workspace| {
                let mut value = workspace.clone();
                value["sessionIds"] = json!(
                    manifest
                        .sessions
                        .values()
                        .filter(|row| !row.ephemeral
                            && !row.archived
                            && row.workspace_id.as_deref() == workspace["workspaceId"].as_str())
                        .map(|row| row.id.clone())
                        .collect::<Vec<_>>()
                );
                value
            })
            .collect();
        json!({"items":items,"archivedSessionIds":manifest.sessions.values().filter(|row|row.archived).map(|row|row.id.clone()).collect::<Vec<_>>()})
    }
    pub(crate) async fn notify_workspaces(&self) {
        let _ = self
            .notifications
            .send(json!({"workspaces":self.workspace_snapshot().await}));
    }
}
impl PiSessionEndpoint {
    pub(crate) async fn management(&self, method: &str, payload: &Value) -> Option<Result<Value>> {
        match method {
            "session.initialPresets" => Some(Ok(
                json!({"presets":[{"id":"default","title":"Default","configurationText":null}],"defaultID":"default"}),
            )),
            "workspace.create" | "workspace.rename" | "workspace.relocate" => {
                Some(self.workspace_call(method, payload).await)
            }
            "session.fork" => Some(self.fork(payload).await),
            "session.discard" => Some(self.discard(payload).await),
            "commands/list" | "commands.list" => Some(
                async {
                    let rpc = self.context.process(string(payload, "sessionId")?).await?;
                    let result = rpc.request("get_commands", json!({})).await?;
                    Ok(result)
                }
                .await,
            ),
            "commands/run" | "commands/execute" | "commands.execute" => Some(
                async {
                    let rpc = self.context.process(string(payload, "sessionId")?).await?;
                    let name = string(payload, "name")?;
                    require(
                        !name.is_empty()
                            && name != "agentkit-control"
                            && !name.contains(char::is_whitespace)
                            && !name.contains('/'),
                        "invalid command name",
                    )?;
                    let arguments = payload["arguments"].as_str().unwrap_or("");
                    let mut content =
                        vec![json!({"type":"text","text":format!("/{name} {arguments}")})];
                    if let Some(attachments) = payload["attachments"].as_array() {
                        content.extend(attachments.iter().cloned());
                    }
                    let materialized = self
                        .context
                        .materialize(string(payload, "sessionId")?, &content)
                        .await?;
                    // Commands retain their existing direct prompt path. They do not call
                    // input.prepare and must not create an AgentKit input admission/receipt;
                    // structured file metadata is therefore explicitly not covered here.
                    let result = rpc
                        .request(
                            "prompt",
                            json!({"message":materialized.message,"images":materialized.images}),
                        )
                        .await?;
                    require(
                        result["disposition"] == "handled",
                        "command was not handled",
                    )?;
                    rpc.refresh();
                    Ok(json!({"commandId":uuid::Uuid::new_v4().to_string(),"native":result}))
                }
                .await,
            ),
            "settings.describe" | "settings.get" | "skills/list" | "skills.list" | "tools.list" => {
                Some(
                    async {
                        let rpc = self.context.process(string(payload, "sessionId")?).await?;
                        let native = match method {
                            "skills/list" | "skills.list" => "skills.list",
                            "tools.list" => "tools.list",
                            _ => "settings.get",
                        };
                        let value = rpc.control(native, json!({})).await?;
                        Ok(if native == "skills.list" {
                            json!({"skills":value})
                        } else {
                            value
                        })
                    }
                    .await,
                )
            }
            "questions.cancel" => Some(
                async {
                    self.respond(
                        string(payload, "actionableId")?,
                        integer(payload, "expectedRevision")?,
                        json!({"cancelled":true}),
                    )
                    .await?;
                    Ok(Value::Null)
                }
                .await,
            ),
            _ => None,
        }
    }
    async fn workspace_call(&self, method: &str, payload: &Value) -> Result<Value> {
        let mut manifest = self.context.manifest.lock().await;
        let now = chrono::Utc::now().to_rfc3339();
        let (id, created) = if method == "workspace.create" {
            let path = Path::new(string(payload, "path")?)
                .canonicalize()
                .map_err(io_error)?;
            require(path.is_dir(), "workspace must be directory")?;
            if let Some(workspace) = manifest
                .workspaces
                .values()
                .find(|workspace| workspace["path"].as_str() == path.to_str())
            {
                (string(workspace, "workspaceId")?.to_owned(), false)
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                manifest.workspaces.insert(id.clone(),json!({"workspaceId":id,"path":path,"title":path.file_name().and_then(|s|s.to_str()).unwrap_or("Workspace"),"sessionIds":[],"createdAt":now,"updatedAt":now}));
                (id, true)
            }
        } else {
            let id = string(payload, "workspaceId")?.to_owned();
            let workspace = manifest
                .workspaces
                .get_mut(&id)
                .ok_or_else(|| unsupported("unknown workspace"))?;
            if method == "workspace.rename" {
                let title = string(payload, "title")?;
                require(!title.trim().is_empty(), "empty title")?;
                workspace["title"] = json!(title);
            } else {
                require(
                    workspace["path"] == payload["previousPath"],
                    "workspace path changed",
                )?;
                let path = Path::new(string(payload, "path")?)
                    .canonicalize()
                    .map_err(io_error)?;
                require(path.is_dir(), "workspace must be directory")?;
                workspace["path"] = json!(path);
            }
            workspace["updatedAt"] = json!(now);
            (id, false)
        };
        let workspace = manifest.workspaces[&id].clone();
        let mut relocated = Vec::new();
        if method == "workspace.relocate" {
            for row in manifest
                .sessions
                .values_mut()
                .filter(|row| row.workspace_id.as_deref() == Some(&id))
            {
                row.cwd = PathBuf::from(string(&workspace, "path")?);
                relocated.push(row.id.clone());
            }
        }
        self.context.save(&manifest).await?;
        drop(manifest);
        for id in relocated {
            if let Some(rpc) = self.context.processes.lock().await.remove(&id) {
                rpc.close();
            }
            self.context.notify_row(&id).await;
        }
        self.context.notify_workspaces().await;
        Ok(json!({"workspace":workspace,"created":created}))
    }
    async fn fork(&self, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let source = self.context.row(id).await?;
        let live = self.context.process(id).await?;
        require(
            live.request("get_state", json!({})).await?["isStreaming"] == false,
            "cannot fork a running session",
        )?;
        let history = live.request("get_entries", json!({})).await?;
        let (branch, _) = active_branch(&history, usize::MAX)?;
        let boundary = if let Some(entry) = payload["entryId"].as_str() {
            require(
                branch.iter().any(|value| value["id"] == entry),
                "fork entry is not in active branch",
            )?;
            Some(entry.to_owned())
        } else if !payload["atSeq"].is_null() {
            Some(completed_fork_entry(&history, &payload["atSeq"])?)
        } else {
            None
        };
        let temporary = Rpc::start(&self.context.config, &source.cwd, Some(&source.file)).await?;
        let result = if let Some(entry) = boundary {
            temporary.control("fork.at", json!({"entryId":entry})).await
        } else {
            temporary.request("clone", json!({})).await
        };
        let result = match result {
            Ok(value) => value,
            Err(error) => {
                temporary.close();
                return Err(error);
            }
        };
        if result["cancelled"] == true {
            temporary.close();
            return Err(Error::Remote(json!({"code":"pi/fork-cancelled"})));
        }
        let state = temporary.request("get_state", json!({})).await;
        let metadata = temporary.request("get_entries", json!({})).await;
        temporary.close();
        let state = state?;
        let (goal_revision, goal, feedback) = extensions::recover_metadata(&metadata?)?;
        let file = PathBuf::from(string(&state, "sessionFile")?)
            .canonicalize()
            .map_err(io_error)?;
        require(
            file.starts_with(self.context.config.data_root.join("sessions")) && file != source.file,
            "fork file outside owned storage",
        )?;
        let child = uuid::Uuid::new_v4().to_string();
        let ephemeral = payload["ephemeral"].as_bool().unwrap_or(false);
        let row = Row {
            id: child.clone(),
            cwd: source.cwd,
            file,
            title: source.title,
            archived: false,
            parent: Some(id.into()),
            subagent_parent: None,
            model_selection: None,
            ephemeral,
            receipts: BTreeMap::new(),
            materialized: true,
            native_session_id: Some(string(&state, "sessionId")?.to_owned()),
            queue: Vec::new(),
            control_revision: 0,
            goal_revision,
            goal,
            feedback,
            workspace_id: if ephemeral { None } else { source.workspace_id },
        };
        let old_uploads = self.context.upload_directory(id);
        if old_uploads.exists() {
            let target = self.context.upload_directory(&child);
            tokio::fs::create_dir_all(&target).await.map_err(io_error)?;
            let mut entries = tokio::fs::read_dir(old_uploads).await.map_err(io_error)?;
            while let Some(entry) = entries.next_entry().await.map_err(io_error)? {
                require(
                    entry.file_type().await.map_err(io_error)?.is_file(),
                    "invalid upload storage member",
                )?;
                tokio::fs::copy(entry.path(), target.join(entry.file_name()))
                    .await
                    .map_err(io_error)?;
            }
        }
        let mut manifest = self.context.manifest.lock().await;
        manifest.sessions.insert(child.clone(), row);
        self.context.save(&manifest).await?;
        drop(manifest);
        self.context.notify_row(&child).await;
        self.context.notify_workspaces().await;
        Ok(json!({"sessionId":child}))
    }
    async fn discard(&self, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let row = self.context.row(id).await?;
        require(row.ephemeral, "only scratch sessions can be discarded")?;
        require(
            row.file
                .starts_with(self.context.config.data_root.join("sessions")),
            "scratch outside owned storage",
        )?;
        if let Some(rpc) = self.context.processes.lock().await.remove(id) {
            rpc.close();
        }
        // A scratch file is owned by this endpoint; adopted files can never be discarded.
        if row.file.exists() {
            tokio::fs::remove_file(&row.file).await.map_err(io_error)?;
        }
        let uploads = self.context.upload_directory(id);
        if uploads.exists() {
            tokio::fs::remove_dir_all(uploads).await.map_err(io_error)?;
        }
        let mut manifest = self.context.manifest.lock().await;
        manifest.sessions.remove(id);
        self.context.save(&manifest).await?;
        drop(manifest);
        let _ = self.context.notifications.send(json!({"removed":id}));
        self.context.notify_workspaces().await;
        Ok(json!({"sessionId":id}))
    }
}

// ClientKit sequences address get_entries order, including inactive branches, not ancestry ordinals.
fn completed_fork_entry(history: &Value, sequence: &Value) -> Result<String> {
    let sequence = sequence
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| unsupported("invalid fork sequence"))?;
    let entries = array(history, "entries")?;
    let target = entries
        .get(usize::try_from(sequence - 1).map_err(|_| unsupported("invalid fork sequence"))?)
        .ok_or_else(|| unsupported("invalid fork sequence"))?;
    let id = string(target, "id")?;
    let (branch, _) = active_branch(history, usize::MAX)?;
    require(
        branch.iter().any(|entry| entry["id"] == id),
        "fork sequence is outside active branch",
    )?;
    require(
        matches!(
            target["customType"].as_str(),
            Some("tekes.run.v1" | "agentkit.run.v1")
        ) && target["data"]["phase"] == "settled",
        "fork sequence must identify a completed turn; use entryId for a native entry boundary",
    )?;
    Ok(id.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_fork_uses_full_native_order_and_rejects_nonsettled_or_inactive_entries() {
        let history = json!({"leafId":"tail","entries":[
            {"id":"root","parentId":null,"type":"message"},
            {"id":"other","parentId":"root","customType":"tekes.run.v1","data":{"phase":"settled"}},
            {"id":"cut","parentId":"root","customType":"tekes.run.v1","data":{"phase":"settled"}},
            {"id":"tail","parentId":"cut","type":"message"}
        ]});
        assert_eq!(completed_fork_entry(&history, &json!(3)).unwrap(), "cut");
        for invalid in [json!(0), json!(-1), json!(1), json!(2), json!(4), json!(5)] {
            assert!(completed_fork_entry(&history, &invalid).is_err());
        }
    }
}
