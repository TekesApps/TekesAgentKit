//! Read-only workspace projection from a Harness's own public file service.
//! Remote paths are never interpreted as paths on the embedding host.
use crate::*;
use base64::{Engine, engine::general_purpose::STANDARD};

const PREVIEW_BYTES: usize = 1024 * 1024;
const COMPLETE_BYTES: usize = 4 * 1024 * 1024;
const PAGE_BYTES: usize = 256 * 1024;

pub(crate) fn supports_harness_workspace(kind: &str, caps: &EndpointCapabilities) -> bool {
    kind == "dsh"
        && caps.supports("workspaceFiles/list")
        && caps.supports("session.files.stat")
        && caps.supports("session.files.readBytes")
}

impl SharedAgent {
    pub(crate) async fn harness_workspace_read(
        &self,
        shared: &SharedEndpoint,
        payload: &Value,
        rpc_id: &str,
    ) -> Result<Value> {
        let id = string(payload, "sessionId")?;
        let epoch = payload["connectionEpoch"].as_u64().ok_or_else(|| {
            Error::Protocol("workspace connectionEpoch must be an unsigned integer".into())
        })?;
        let method = string(payload, "method")?;
        require(
            matches!(method, "capabilities" | "filesList" | "filesRead"),
            "Harness workspace method is unavailable",
        )?;
        let fields = payload
            .as_object()
            .ok_or_else(|| Error::Protocol("workspace payload must be an object".into()))?;
        require(
            fields.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "sessionId" | "connectionEpoch" | "method" | "request"
                )
            }),
            "unknown Harness workspace field",
        )?;
        let request = payload["request"]
            .as_object()
            .ok_or_else(|| Error::Protocol("workspace request must be an object".into()))?;
        let allowed: &[&str] = match method {
            "capabilities" => &[],
            "filesList" => &["path", "limit"],
            "filesRead" => &["path", "maxBytes"],
            _ => unreachable!(),
        };
        require(
            request.keys().all(|key| allowed.contains(&key.as_str())),
            "unknown Harness workspace request field",
        )?;
        let endpoint = shared.connected()?;
        require(
            supports_harness_workspace(&shared.kind, &endpoint.capabilities()),
            "Harness workspace reading is unavailable",
        )?;
        shared
            .runtime
            .inspect(|state| validate_scope(state, id, epoch))?;
        let result = match method {
            "capabilities" => Ok(
                json!({"version":1,"methods":["capabilities","filesList","filesRead"],"mutations":false,"allowedGitOperations":[],"turnChanges":false}),
            ),
            "filesList" => {
                let path = optional_path(&payload["request"], true)?;
                let limit = optional_bound(&payload["request"], "limit", 10000)?;
                match shared
                    .runtime
                    .call_at_epoch(
                        epoch,
                        "workspaceFiles/list",
                        json!({"workspaceFileScopeId":id,"path":path}),
                        rpc_id,
                    )
                    .await
                {
                    Ok(raw) => normalize_listing(id, raw, limit),
                    Err(error) => Err(error),
                }
            }
            "filesRead" => {
                let path = optional_path(&payload["request"], false)?;
                let maximum = payload["request"]
                    .get("maxBytes")
                    .map(|value| {
                        value.as_u64().ok_or_else(|| {
                            Error::Protocol("maxBytes must be a nonnegative integer".into())
                        })
                    })
                    .transpose()?
                    .unwrap_or(PREVIEW_BYTES as u64);
                require(
                    maximum <= COMPLETE_BYTES as u64,
                    "Harness workspace read exceeds the 4 MiB bound",
                )?;
                read_file(shared, id, epoch, path, maximum as usize, rpc_id).await
            }
            _ => unreachable!(),
        };
        // A late read, including a failure, cannot be attributed to a new connection.
        shared
            .runtime
            .inspect(|state| validate_scope(state, id, epoch))?;
        let mut response = json!({"provider":"harness","endpointId":endpoint.identity(),"sessionId":id,"workspaceId":id,"connectionEpoch":epoch,"method":method});
        match result {
            Ok(value) => response["result"] = value,
            Err(Error::Remote(error)) => response["error"] = error,
            Err(error) => return Err(error),
        }
        Ok(response)
    }
}

fn validate_scope(state: &MemoryProjectionStore, id: &str, epoch: u64) -> Result<()> {
    require(
        state.connection_epoch == epoch,
        "Harness workspace connection epoch is stale",
    )?;
    require(
        matches!(state.connection, ConnectionState::Ready) && state.inventory_ready,
        "Harness workspace requires a ready inventory",
    )?;
    require(
        state
            .sessions
            .get(id)
            .is_some_and(|session| session.summary.is_some()),
        "Harness workspace session is unavailable",
    )
}

fn relative_path(path: &str, allow_empty: bool) -> Result<()> {
    require(
        (allow_empty || !path.is_empty())
            && !path.starts_with('/')
            && !path.contains(['\\', '\0'])
            && !path.split('/').any(|part| part == "..")
            && !path.split('/').next().unwrap_or("").contains(':'),
        "Harness workspace path must be relative",
    )
}
fn optional_path(request: &Value, allow_empty: bool) -> Result<&str> {
    let path = match request.get("path") {
        None if allow_empty => "",
        Some(Value::String(path)) => path.as_str(),
        _ => return Err(Error::Protocol("workspace path must be text".into())),
    };
    relative_path(path, allow_empty)?;
    Ok(path)
}
fn optional_bound(request: &Value, field: &str, maximum: usize) -> Result<Option<usize>> {
    request
        .get(field)
        .map(|value| {
            let value = value
                .as_u64()
                .ok_or_else(|| Error::Protocol(format!("{field} must be a positive integer")))?;
            require(
                value > 0 && value <= maximum as u64,
                &format!("invalid workspace {field}"),
            )?;
            Ok(value as usize)
        })
        .transpose()
}

fn normalize_listing(id: &str, raw: Value, limit: Option<usize>) -> Result<Value> {
    let path = string(&raw, "path")?;
    relative_path(path, true)?;
    let rows = array(&raw, "entries")?;
    let upstream_truncated = raw["truncated"]
        .as_bool()
        .ok_or_else(|| Error::Protocol("workspace listing has no truncation fact".into()))?;
    let maximum = limit.unwrap_or(rows.len());
    let mut entries = Vec::new();
    for row in rows {
        let name = string(row, "name")?;
        require(
            !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\\', '\0']),
            "workspace listing entry is not a basename",
        )?;
        let kind = string(row, "type")?;
        require(
            matches!(kind, "file" | "directory" | "other"),
            "unknown Harness workspace file kind",
        )?;
        if let Some(size) = row.get("size") {
            require(
                size.as_u64().is_some(),
                "invalid Harness workspace file size",
            )?;
        }
        if entries.len() < maximum {
            let child = if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            };
            let mut entry = json!({"name":name,"path":child,"kind":kind,"hidden":child.split('/').any(|part|part.starts_with('.'))});
            if let Some(size) = row.get("size") {
                entry["size"] = size.clone();
            }
            entries.push(entry);
        }
    }
    Ok(
        json!({"workspaceId":id,"path":path,"entries":entries,"truncated":upstream_truncated || rows.len()>maximum}),
    )
}

async fn read_file(
    shared: &SharedEndpoint,
    id: &str,
    epoch: u64,
    path: &str,
    maximum: usize,
    rpc_id: &str,
) -> Result<Value> {
    let complete = maximum == 0;
    let maximum = if complete { COMPLETE_BYTES } else { maximum };
    let stat: SessionFileStat = serde_json::from_value(
        shared
            .runtime
            .call_at_epoch(
                epoch,
                "session.files.stat",
                json!({"sessionId":id,"path":path}),
                &format!("{rpc_id}:stat"),
            )
            .await?,
    )?;
    require(
        !stat.absolute_path.is_empty() && !stat.version.is_empty(),
        "Harness file stat identity is missing",
    )?;
    if complete && stat.bytes.is_some_and(|size| size > maximum as u64) {
        return Err(Error::Remote(
            json!({"code":"workspace-file/too-large","message":"Complete Harness file exceeds the 4 MiB read bound"}),
        ));
    }
    let mut data = Vec::new();
    let mut total = stat.bytes;
    let mut eof = false;
    while data.len() < maximum && !eof {
        let offset = data.len() as u64;
        let length = PAGE_BYTES.min(maximum - data.len());
        let page: SessionFileBytePage = serde_json::from_value(
            shared
                .runtime
                .call_at_epoch(
                    epoch,
                    "session.files.readBytes",
                    json!({"sessionId":id,"path":path,"offset":offset,"limit":length}),
                    &format!("{rpc_id}:bytes:{offset}"),
                )
                .await?,
        )?;
        page.validate(offset, length)?;
        require(
            page.absolute_path == stat.absolute_path
                && page.version == stat.version
                && (total.is_none() || page.bytes == total),
            "Harness file changed while reading",
        )?;
        if total.is_none() {
            total = page.bytes;
        }
        let bytes = STANDARD
            .decode(&page.data)
            .map_err(|_| Error::Protocol("invalid Harness file bytes".into()))?;
        data.extend(bytes);
        eof = page.eof;
    }
    if complete && !eof {
        return Err(Error::Remote(
            json!({"code":"workspace-file/too-large","message":"Complete Harness file exceeds the 4 MiB read bound"}),
        ));
    }
    let size = total
        .or_else(|| eof.then_some(data.len() as u64))
        .ok_or_else(|| Error::Protocol("Harness preview has no complete size fact".into()))?;
    Ok(
        json!({"workspaceId":id,"path":path,"encoding":"base64","content":STANDARD.encode(data),"size":size,"truncated":!eof,"revision":stat.version}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Clone)]
    struct Connector {
        calls: Arc<Mutex<Vec<(String, Value)>>>,
        fail_list: bool,
        block_list: bool,
        entered: Arc<tokio::sync::Notify>,
        finish: Arc<tokio::sync::Notify>,
        version_changed: Arc<AtomicBool>,
    }
    struct Endpoint {
        connector: Connector,
        host: HostDescription,
    }
    #[async_trait]
    impl EndpointConnector for Connector {
        async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
            Ok(Arc::new(Endpoint {
                connector: self.clone(),
                host: serde_json::from_value(
                    json!({"protocolVersion":4,"product":{"name":"DSH fixture","version":"1"},"capabilities":["session-journal","session-inventory-sync","session-control-sync","actionable-sync"],"cwd":"/remote/project","home":"/remote","attachedSessions":0,"canOpenPath":false}),
                )?,
            }))
        }
    }
    #[async_trait]
    impl SessionEndpoint for Endpoint {
        fn identity(&self) -> &str {
            "dsh-files"
        }
        fn host(&self) -> &HostDescription {
            &self.host
        }
        fn capabilities(&self) -> EndpointCapabilities {
            EndpointCapabilities::methods(&[
                "workspaceFiles/list",
                "session.files.stat",
                "session.files.readBytes",
            ])
        }
        async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
            let value = match target {
                StreamTarget::SessionInventory => {
                    json!({"type":"baseline","generation":"g","snapshot":{"items":[{"sessionId":"session","running":false,"blank":false,"updatedAt":1}]}})
                }
                StreamTarget::SessionControl => {
                    json!({"type":"baseline","generation":"g","snapshot":{"queues":[],"jobs":[],"projections":[]}})
                }
                StreamTarget::Actionables => json!({"type":"reset","generation":"g"}),
                _ => {
                    return Err(unsupported(
                        "unexpected observation; file reads must not open an agent journal",
                    ));
                }
            };
            Ok(Box::pin(
                stream::once(async { Ok(value) }).chain(stream::pending()),
            ))
        }
        async fn history_page(&self, _: HistoryRequest) -> Result<HistoryPage> {
            Err(unsupported("fixture history"))
        }
        async fn respond(&self, _: &str, _: i64, _: Value) -> Result<()> {
            Err(unsupported("fixture response"))
        }
        async fn file_stat(&self, id: &str, path: &str) -> Result<SessionFileStat> {
            self.connector
                .calls
                .lock()
                .unwrap()
                .push(("typed-stat".into(), json!({"sessionId":id,"path":path})));
            Ok(SessionFileStat {
                absolute_path: "/remote/project/proof.bin".into(),
                version: "remote-version".into(),
                bytes: Some(4),
            })
        }
        async fn file_read_bytes(
            &self,
            id: &str,
            path: &str,
            offset: u64,
            length: usize,
        ) -> Result<SessionFileBytePage> {
            self.connector.calls.lock().unwrap().push((
                "typed-bytes".into(),
                json!({"sessionId":id,"path":path,"offset":offset,"length":length}),
            ));
            let bytes = [0_u8, 255, 4, 5];
            let end = bytes.len().min(offset as usize + length);
            Ok(SessionFileBytePage {
                absolute_path: "/remote/project/proof.bin".into(),
                version: if self.connector.version_changed.load(Ordering::SeqCst) {
                    "changed"
                } else {
                    "remote-version"
                }
                .into(),
                bytes: Some(4),
                offset,
                data: STANDARD.encode(&bytes[offset as usize..end]),
                eof: end == bytes.len(),
            })
        }
        async fn call(&self, method: &str, payload: Value, _: &str) -> Result<Value> {
            self.connector
                .calls
                .lock()
                .unwrap()
                .push((method.into(), payload));
            if method != "workspaceFiles/list" {
                return Err(unsupported("unexpected method"));
            }
            self.connector.entered.notify_one();
            if self.connector.block_list {
                self.connector.finish.notified().await;
            }
            if self.connector.fail_list {
                return Err(Error::Remote(
                    json!({"code":"workspace-file/not-directory","message":"Remote refused directory","details":{"path":"proof"}}),
                ));
            }
            Ok(
                json!({"path":"remote","entries":[{"name":"proof.bin","type":"file","size":4},{"name":"folder","type":"directory"}],"truncated":false}),
            )
        }
        async fn close(&self) {}
    }
    async fn fixture(
        fail_list: bool,
        block_list: bool,
    ) -> (Arc<SharedAgent>, Arc<SharedEndpoint>, Connector) {
        let connector = Connector {
            calls: Arc::new(Mutex::new(Vec::new())),
            fail_list,
            block_list,
            entered: Arc::new(tokio::sync::Notify::new()),
            finish: Arc::new(tokio::sync::Notify::new()),
            version_changed: Arc::new(AtomicBool::new(false)),
        };
        let mut agent = SharedAgent::default();
        agent
            .register(
                "dsh-files".into(),
                "dsh".into(),
                "DSH files".into(),
                Arc::new(connector.clone()),
            )
            .unwrap();
        let shared = agent.endpoint("dsh-files").unwrap();
        shared
            .runtime
            .wait_ready(std::time::Duration::from_secs(2))
            .await
            .unwrap();
        (Arc::new(agent), shared, connector)
    }
    fn request(epoch: u64, method: &str, body: Value) -> Value {
        json!({"op":"call","endpoint":"dsh-files","method":"agent.workspace.read","rpcId":"read","payload":{"sessionId":"session","connectionEpoch":epoch,"method":method,"request":body}})
    }

    #[tokio::test]
    async fn public_reader_uses_remote_scope_and_typed_binary_without_opening_an_agent() {
        let (agent, shared, connector) = fixture(false, false).await;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let caps = agent
            .request(&request(epoch, "capabilities", json!({})))
            .await
            .unwrap();
        assert_eq!(caps["provider"], "harness");
        assert_eq!(caps["result"]["mutations"], false);
        let listed = agent
            .request(&request(
                epoch,
                "filesList",
                json!({"path":"remote","limit":1}),
            ))
            .await
            .unwrap();
        assert_eq!(listed["result"]["entries"][0]["path"], "remote/proof.bin");
        assert_eq!(listed["result"]["truncated"], true);
        let read = agent
            .request(&request(
                epoch,
                "filesRead",
                json!({"path":"proof.bin","maxBytes":0}),
            ))
            .await
            .unwrap();
        assert_eq!(read["workspaceId"], "session");
        assert_eq!(read["connectionEpoch"], epoch);
        assert_eq!(
            read["result"]["content"],
            STANDARD.encode([0_u8, 255, 4, 5])
        );
        assert_eq!(read["result"]["revision"], "remote-version");
        assert_eq!(read["result"]["truncated"], false);
        let calls = connector.calls.lock().unwrap();
        assert_eq!(
            calls[0],
            (
                "workspaceFiles/list".into(),
                json!({"workspaceFileScopeId":"session","path":"remote"})
            )
        );
        assert_eq!(calls[1].0, "typed-stat");
        assert_eq!(calls[2].0, "typed-bytes");
        assert_eq!(calls.len(), 3);
        drop(calls);
        shared.runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn remote_failure_is_preserved_and_invalid_scope_never_falls_back() {
        let (agent, shared, connector) = fixture(true, false).await;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let reply = agent
            .request(&request(epoch, "filesList", json!({"path":"remote"})))
            .await
            .unwrap();
        assert_eq!(
            reply["error"],
            json!({"code":"workspace-file/not-directory","message":"Remote refused directory","details":{"path":"proof"}})
        );
        for invalid in [
            request(epoch + 1, "filesList", json!({"path":"remote"})),
            request(epoch, "filesRead", json!({"path":"../private"})),
            request(epoch, "filesRead", json!({"path":"/local/proof"})),
            request(epoch, "filesWrite", json!({"path":"proof"})),
            request(epoch, "filesList", json!({"path":"","root":"/host"})),
        ] {
            assert!(agent.request(&invalid).await.is_err());
        }
        assert_eq!(connector.calls.lock().unwrap().len(), 1);
        shared.runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn preview_preserves_truncation_and_rejects_a_changed_file_version() {
        let (agent, shared, connector) = fixture(false, false).await;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let read = agent
            .request(&request(
                epoch,
                "filesRead",
                json!({"path":"proof.bin","maxBytes":2}),
            ))
            .await
            .unwrap();
        assert_eq!(read["result"]["size"], 4);
        assert_eq!(read["result"]["truncated"], true);
        assert_eq!(read["result"]["content"], STANDARD.encode([0_u8, 255]));
        connector.version_changed.store(true, Ordering::SeqCst);
        assert!(
            agent
                .request(&request(
                    epoch,
                    "filesRead",
                    json!({"path":"proof.bin","maxBytes":0})
                ))
                .await
                .is_err()
        );
        shared.runtime.stop().await.unwrap();
    }

    #[tokio::test]
    async fn a_pending_read_cannot_cross_a_reconnected_epoch() {
        let (agent, shared, connector) = fixture(false, true).await;
        let epoch = shared.runtime.snapshot().connection_epoch;
        let pending = {
            let agent = agent.clone();
            tokio::spawn(async move {
                agent
                    .request(&request(epoch, "filesList", json!({"path":"remote"})))
                    .await
            })
        };
        connector.entered.notified().await;
        shared.runtime.reconnect().await.unwrap();
        shared
            .runtime
            .wait_ready(std::time::Duration::from_secs(2))
            .await
            .unwrap();
        connector.finish.notify_one();
        assert!(pending.await.unwrap().is_err());
        assert!(shared.runtime.snapshot().connection_epoch > epoch);
        shared.runtime.stop().await.unwrap();
    }
}
