use super::*;
impl Context {
    pub(crate) fn schedule_goal(self: &Arc<Self>, id: String, rpc: Arc<Rpc>) {
        let context = self.clone();
        tokio::spawn(async move {
            if let Err(error) = context.drive_goal(&id, &rpc).await {
                context.armed_goals.lock().await.remove(&id);
                let _=context.notifications.send(json!({"goalActivation":{"sessionId":id,"activation":"disarmed","error":error.to_string()}}));
            }
        });
    }
    async fn drive_goal(&self, id: &str, rpc: &Arc<Rpc>) -> Result<()> {
        let _guard = self
            .gate(id)
            .await
            .acquire_owned()
            .await
            .map_err(|_| Error::Disconnected)?;
        if !self.armed_goals.lock().await.contains(id) {
            return Ok(());
        }
        let current = self.processes.lock().await.get(id).cloned();
        if !current.is_some_and(|current| Arc::ptr_eq(&current, rpc)) || !rpc.alive() {
            return Ok(());
        }
        if rpc.request("get_state", json!({})).await?["isStreaming"] != false {
            return Ok(());
        }
        let history = rpc.request("get_entries", json!({})).await?;
        let (native_revision, native_goal, _) = extensions::recover_metadata(&history)?;
        let mut manifest = self.manifest.lock().await;
        let row = manifest
            .sessions
            .get_mut(id)
            .ok_or_else(|| unsupported("unknown goal owner"))?;
        if !row.queue.is_empty() {
            return Ok(());
        }
        if native_revision > row.goal_revision {
            row.goal_revision = native_revision;
            row.goal = native_goal;
        }
        let Some(mut goal) = row.goal.clone() else {
            self.armed_goals.lock().await.remove(id);
            return Ok(());
        };
        if goal["phase"] != "active" {
            self.save(&manifest).await?;
            drop(manifest);
            self.armed_goals.lock().await.remove(id);
            self.publish_projection(id, "goal", goal).await?;
            self.goal_activation(id, "disarmed");
            return Ok(());
        }
        let rounds = goal["roundsStarted"]
            .as_u64()
            .ok_or_else(|| unsupported("invalid goal rounds"))?;
        let bound = goal["maxGoalRounds"]
            .as_u64()
            .ok_or_else(|| unsupported("invalid goal bound"))?;
        row.goal_revision = row
            .goal_revision
            .checked_add(1)
            .ok_or_else(|| unsupported("goal revision exhausted"))?;
        goal["revision"] = json!(row.goal_revision);
        goal["updatedAt"] = json!(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
                * 1000.0
        );
        let exhausted = rounds >= bound;
        if exhausted {
            goal["phase"] = json!("blocked");
            goal["blockedReason"] = json!({"code":"max-goal-rounds"});
        } else {
            goal["roundsStarted"] = json!(rounds + 1);
        }
        // A crash/uncertain transport consumes its reserved round and never re-arms on reconnect.
        row.goal = Some(goal.clone());
        self.save(&manifest).await?;
        drop(manifest);
        rpc.control(
            "entry.append",
            json!({"type":"agentkit.goal.v1","data":{"revision":goal["revision"],"value":goal}}),
        )
        .await?;
        self.publish_projection(id, "goal", goal.clone()).await?;
        if exhausted {
            self.armed_goals.lock().await.remove(id);
            self.goal_activation(id, "disarmed");
            rpc.refresh();
            return Ok(());
        }
        let message = format!(
            "Continue the active user goal: {}\nCheck the result. Use agentkit_goal to record completion or a blocking condition.",
            string(&goal, "objective")?
        );
        let response = rpc.request("prompt", json!({"message":message})).await?;
        require(
            response["disposition"] == "started",
            "goal round was not started",
        )?;
        Ok(())
    }
    pub(crate) fn goal_activation(&self, id: &str, activation: &str) {
        let _ = self
            .notifications
            .send(json!({"goalActivation":{"sessionId":id,"activation":activation}}));
    }
}
