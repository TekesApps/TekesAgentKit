use super::*;
pub(crate) const MODES: &[&str] = &["allow-tools", "ask-tools", "deny-tools"];
fn policy() -> Value {
    json!({"threadLevel":[{"value":"allow-tools","title":"Allow Tools"},{"value":"ask-tools","title":"Ask for Each Tool"},{"value":"deny-tools","title":"Block Tools"}],"serverLevel":{"options":[],"currentValue":null}})
}
impl Context {
    pub(crate) fn permission_file(&self, id: &str) -> PathBuf {
        self.config
            .data_root
            .join("permissions")
            .join(format!("{id}.json"))
    }
    pub(crate) async fn permission_mode(&self, id: &str) -> Result<String> {
        let path = self.permission_file(id);
        let metadata = match tokio::fs::symlink_metadata(&path).await {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok("allow-tools".into());
            }
            Err(error) => return Err(io_error(error)),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 {
            return Ok("deny-tools".into());
        }
        let value = tokio::fs::read(&path).await.map_err(io_error)?;
        let parsed = serde_json::from_slice::<Value>(&value).ok();
        Ok(parsed
            .as_ref()
            .and_then(|value| value["mode"].as_str())
            .filter(|mode| MODES.contains(mode))
            .unwrap_or("deny-tools")
            .into())
    }
    pub(crate) async fn write_permission(&self, id: &str, mode: &str) -> Result<()> {
        require(MODES.contains(&mode), "Unsupported Pi approval mode")?;
        let path = self.permission_file(id);
        tokio::fs::create_dir_all(path.parent().expect("permission parent"))
            .await
            .map_err(io_error)?;
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).await.map_err(io_error)?;
        file.write_all(&serde_json::to_vec(&json!({"mode":mode}))?)
            .await
            .map_err(io_error)?;
        file.sync_all().await.map_err(io_error)?;
        drop(file);
        tokio::fs::rename(&temporary, &path)
            .await
            .map_err(io_error)?;
        Ok(())
    }
}
impl PiSessionEndpoint {
    pub(crate) async fn permission_call(&self, method: &str, payload: &Value) -> Result<Value> {
        if method == "approvals.policy" {
            return Ok(policy());
        }
        let id = string(payload, "sessionId")?;
        let row = self.context.row(id).await?;
        if method == "approvals.select" {
            require(!row.archived, "session is archived")?;
            let mode = string(payload, "mode")?;
            self.context.write_permission(id, mode).await?;
            self.context
                .publish_projection(
                    id,
                    "permissions",
                    json!({"options":policy()["threadLevel"],"currentValue":mode}),
                )
                .await?;
        }
        Ok(json!({"mode":self.context.permission_mode(id).await?}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn permissions_persist_fail_closed_and_do_not_launch_pi() {
        let root =
            std::env::temp_dir().join(format!("agentkit-permissions-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let endpoint = PiSessionEndpoint {
            context: connector.context.clone(), generation: "test".into(),
            host: serde_json::from_value(json!({"protocolVersion":3,"product":{"name":"Pi","version":"1.3.0"},"capabilities":[],"cwd":root,"home":root,"attachedSessions":0,"canOpenPath":false})).unwrap(),
            windows: Arc::new(Mutex::new(BTreeMap::new())), lifetime: tokio::sync::watch::channel(false).0,
        };
        let created = endpoint
            .call("session.create", json!({"cwd":root}), "create")
            .await
            .unwrap();
        let id = created["sessionId"].as_str().unwrap();
        let catalog = endpoint
            .call("approvals.policy", json!({}), "policy")
            .await
            .unwrap();
        let typed: PermissionPolicy = serde_json::from_value(catalog).unwrap();
        assert_eq!(
            typed
                .thread_level
                .iter()
                .map(|o| o.value.as_str())
                .collect::<Vec<_>>(),
            MODES
        );
        for mode in MODES {
            endpoint
                .call(
                    "approvals.select",
                    json!({"sessionId":id,"mode":mode}),
                    "select",
                )
                .await
                .unwrap();
            assert_eq!(
                endpoint
                    .call("approvals.mode", json!({"sessionId":id}), "read")
                    .await
                    .unwrap()["mode"],
                *mode
            );
        }
        assert!(
            endpoint
                .call(
                    "approvals.select",
                    json!({"sessionId":id,"mode":"danger-full-access"}),
                    "invalid"
                )
                .await
                .is_err()
        );
        assert!(connector.context.processes.lock().await.is_empty());
        assert!(!connector.context.row(id).await.unwrap().file.exists());
        let file = connector.context.permission_file(id);
        tokio::fs::write(&file, "invalid policy").await.unwrap();
        assert_eq!(
            connector.context.permission_mode(id).await.unwrap(),
            "deny-tools"
        );
        drop(endpoint);
        drop(connector);
        let reopened = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        assert_eq!(
            reopened.context.permission_mode(id).await.unwrap(),
            "deny-tools"
        );
        drop(reopened);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
