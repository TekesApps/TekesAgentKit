use super::*;
impl Context {
    pub(crate) async fn publish_projection(
        &self,
        id: &str,
        key: &str,
        value: Value,
    ) -> Result<i64> {
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown session"))?;
        row.control_revision = row
            .control_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("projection clock exhausted"))?;
        let revision = row.control_revision;
        self.save(&manifest).await?;
        drop(manifest);
        let _=self.notifications.send(json!({"projection":{"sessionId":id,"projections":{"asOfSeq":revision,"values":{key:value}}}}));
        Ok(revision)
    }
}
impl PiSessionEndpoint {
    pub(crate) async fn extension_call(
        &self,
        method: &str,
        payload: &Value,
    ) -> Option<Result<Value>> {
        match method {
            "references.files"=>Some(async {
                let root=self.context.row(string(payload,"sessionId")?).await?.cwd;let query=string(payload,"query")?.to_owned();
                tokio::task::spawn_blocking(move||files::candidates(&root,&query)).await.map_err(|_|Error::Disconnected)?
            }.await),
            "goals.get"|"goals.create"|"goals.edit"|"goals.clear"|"goals.pause"|"goals.resume"=>Some(self.goal_call(method,payload).await),
            "feedback.list"|"feedback.put"|"feedback.delete"=>Some(self.feedback_call(method,payload).await),
            "references.sessions"|"session.search"=>Some(async {
                let query=string(payload,"query")?.to_lowercase();
                require(query.len()<=4096,"search query limit")?;
                let rows:Vec<_>=self.context.manifest.lock().await.sessions.values().filter(|row|!row.archived && !row.ephemeral).cloned().collect();
                let mut items=Vec::new();
                for row in rows {
                    let title=row.title.clone().unwrap_or_else(||row.id.clone());
                    let mut text=title.clone();
                    if method=="session.search" {
                        let rpc=self.context.process(&row.id).await?;
                        let entries=rpc.request("get_entries",json!({})).await?;
                        let (branch,_)=active_branch(&entries,usize::MAX)?;
                        text.push_str(&serde_json::to_string(&branch)?);
                    }
                    if text.to_lowercase().contains(&query) {items.push(json!({"sessionId":row.id,"title":title,"label":title,"cwd":row.cwd,"snippet":text.chars().take(240).collect::<String>()}));}
                    if items.len()>=20 {break;}
                }
                Ok(if method=="session.search" {json!({"items":items})} else {json!(items)})
            }.await),
            "sessions.recover"=>Some(async {
                let ids=array(payload,"sessionIds")?;
                for id in ids {let id=id.as_str().ok_or_else(||unsupported("invalid session id"))?;self.context.process(id).await?;self.context.dispatch_next(id).await?;}
                Ok(Value::Null)
            }.await),
            _=>None,
        }
    }
    async fn goal_call(&self, method: &str, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        self.context.row(id).await?;
        if method == "goals.get" {
            let _guard = self
                .context
                .gate(id)
                .await
                .acquire_owned()
                .await
                .map_err(|_| Error::Disconnected)?;
            let rpc = self.context.process(id).await?;
            let history = rpc.request("get_entries", json!({})).await?;
            let (revision, value, _) = recover_metadata(&history)?;
            let mut manifest = self.context.manifest.lock().await;
            let row = manifest.sessions.get_mut(id).expect("owned row");
            if revision > row.goal_revision {
                row.goal_revision = revision;
                row.goal = value;
                self.context.save(&manifest).await?;
            }
            let mut goal = manifest.sessions[id].goal.clone().unwrap_or(Value::Null);
            drop(manifest);
            if !goal.is_null() {
                if goal["phase"] != "active" {
                    self.context.armed_goals.lock().await.remove(id);
                }
                if self.context.armed_goals.lock().await.contains(id) {
                    goal["activation"] = json!("armed");
                }
            }
            return Ok(goal);
        }
        let _guard = self
            .context
            .gate(id)
            .await
            .acquire_owned()
            .await
            .map_err(|_| Error::Disconnected)?;
        let mut manifest = self.context.manifest.lock().await;
        let row = manifest.sessions.get_mut(id).expect("owned row");
        let next = row
            .goal_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("goal revision exhausted"))?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0;
        let result = if method == "goals.create" {
            require(row.goal.is_none(), "goal already exists")?;
            let objective = string(payload, "objective")?;
            require(
                !objective.trim().is_empty() && objective.len() <= 65536,
                "invalid objective",
            )?;
            let rounds = payload["maxGoalRounds"].as_u64().unwrap_or(8);
            require((1..=10000).contains(&rounds), "invalid goal round bound")?;
            json!({"id":uuid::Uuid::new_v4().to_string(),"revision":next,"objective":objective,"phase":"active","maxGoalRounds":rounds,"roundsStarted":0,"createdAt":now,"updatedAt":now,"activation":"disarmed","blockedReason":null})
        } else {
            let old = row.goal.as_ref().ok_or_else(|| unsupported("no goal"))?;
            require(
                payload["ref"]["id"] == old["id"] && payload["ref"]["revision"] == old["revision"],
                "stale goal reference",
            )?;
            if method == "goals.clear" {
                json!({"id":old["id"],"revision":next})
            } else {
                let mut goal = old.clone();
                goal["revision"] = json!(next);
                goal["updatedAt"] = json!(now);
                match method {
                    "goals.edit" => {
                        let objective = string(payload, "objective")?;
                        require(
                            !objective.trim().is_empty() && objective.len() <= 65536,
                            "invalid objective",
                        )?;
                        goal["objective"] = json!(objective);
                    }
                    "goals.pause" => goal["phase"] = json!("paused"),
                    "goals.resume" => {
                        require(goal["phase"] != "complete", "completed goal cannot resume")?;
                        goal["phase"] = json!("active");
                    }
                    _ => unreachable!(),
                }
                goal
            }
        };
        row.goal_revision = next;
        row.goal = if method == "goals.clear" {
            None
        } else {
            Some(result.clone())
        };
        let record = json!({"revision":next,"value":row.goal});
        self.context.save(&manifest).await?;
        drop(manifest);
        let rpc = self.context.process(id).await?;
        rpc.control(
            "entry.append",
            json!({"type":"agentkit.goal.v1","data":record}),
        )
        .await?;
        self.context
            .publish_projection(id, "goal", record["value"].clone())
            .await?;
        rpc.refresh();
        let mut result = result;
        if method == "goals.resume" {
            self.context.armed_goals.lock().await.insert(id.into());
            self.context.goal_activation(id, "armed");
            self.context.schedule_goal(id.into(), rpc);
            result["activation"] = json!("armed");
        } else if matches!(method, "goals.pause" | "goals.clear") {
            self.context.armed_goals.lock().await.remove(id);
            self.context.goal_activation(id, "disarmed");
        }
        Ok(result)
    }
    async fn feedback_call(&self, method: &str, payload: &Value) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        if method == "feedback.list" {
            return Ok(
                json!({"items":self.context.row(id).await?.feedback.values().collect::<Vec<_>>()}),
            );
        }
        let _guard = self
            .context
            .gate(id)
            .await
            .acquire_owned()
            .await
            .map_err(|_| Error::Disconnected)?;
        let message = string(payload, "messageId")?;
        let rpc = self.context.process(id).await?;
        let entries = rpc.request("get_entries", json!({})).await?;
        require(
            array(&entries, "entries")?
                .iter()
                .any(|entry| entry["id"] == message && entry["message"]["role"] == "assistant"),
            "feedback must address a native assistant entry",
        )?;
        let mut manifest = self.context.manifest.lock().await;
        let row = manifest.sessions.get_mut(id).expect("owned row");
        let current = row.feedback.get(message);
        require(
            current.map(|value| &value["version"])
                == payload.get("ifVersion").filter(|value| !value.is_null()),
            "feedback version conflict",
        )?;
        let result = if method == "feedback.delete" {
            row.feedback.remove(message);
            Value::Null
        } else {
            let rating = string(payload, "rating")?;
            require(
                ["positive", "negative", "up", "down"].contains(&rating),
                "invalid feedback rating",
            )?;
            let value = json!({"messageId":message,"rating":rating,"note":payload["note"],"version":uuid::Uuid::new_v4().to_string()});
            row.feedback.insert(message.into(), value.clone());
            value
        };
        self.context.save(&manifest).await?;
        drop(manifest);
        rpc.control(
            "entry.append",
            json!({"type":"agentkit.feedback.v1","data":{"messageId":message,"value":result}}),
        )
        .await?;
        rpc.refresh();
        Ok(result)
    }
}

/// Recover only adapter-owned records from the public native tree, never from a private log parser.
pub(crate) fn recover_metadata(
    history: &Value,
) -> Result<(i64, Option<Value>, BTreeMap<String, Value>)> {
    let (branch, _) = active_branch(history, usize::MAX)?;
    let mut revision = 0;
    let mut goal = None;
    let mut feedback = BTreeMap::new();
    for entry in branch {
        match entry["customType"].as_str() {
            Some("agentkit.goal.v1" | "tekes.goal.v1") => {
                let record = &entry["data"];
                let next = integer(record, "revision")?;
                require(next >= revision, "goal metadata revision moved backwards")?;
                revision = next;
                goal = if record["value"].is_null() {
                    None
                } else {
                    let mut value = record["value"].clone();
                    value["activation"] = json!("disarmed");
                    let parsed: GoalView = serde_json::from_value(value.clone())?;
                    require(
                        parsed.revision == next && !parsed.id.is_empty(),
                        "invalid native goal metadata",
                    )?;
                    Some(value)
                };
            }
            Some("agentkit.feedback.v1") => {
                let id = string(&entry["data"], "messageId")?.to_owned();
                if entry["data"]["value"].is_null() {
                    feedback.remove(&id);
                } else {
                    feedback.insert(id, entry["data"]["value"].clone());
                }
            }
            _ => {}
        }
    }
    Ok((revision, goal, feedback))
}
