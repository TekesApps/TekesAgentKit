use crate::PiConfiguration;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tekes_agent_contract::*;
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{broadcast, mpsc, oneshot},
};
use tokio_util::codec::{FramedRead, LinesCodec};
const MAX_RECORD: usize = 64 * 1024 * 1024;
enum Request {
    Call(Value, oneshot::Sender<Result<Value>>),
    Ui(Value),
    Forget(String),
    Close,
}

#[derive(Default)]
struct PendingRequests {
    requests: HashMap<String, oneshot::Sender<Result<Value>>>,
}

impl PendingRequests {
    fn insert(&mut self, id: String, reply: oneshot::Sender<Result<Value>>) {
        self.requests.insert(id, reply);
    }

    fn forget(&mut self, id: &str) {
        self.requests.remove(id);
    }

    fn respond(&mut self, value: &Value) {
        let Some(id) = value["id"].as_str() else {
            return;
        };
        let Some(reply) = self.requests.remove(id) else {
            return;
        };
        let result = match value["success"].as_bool() {
            Some(true) => Ok(value.get("data").cloned().unwrap_or(Value::Null)),
            Some(false) => Err(Error::Remote(
                json!({"code":"pi/command-failed","message":value["error"]}),
            )),
            None => Err(Error::Protocol("invalid Pi response".into())),
        };
        let _ = reply.send(result);
    }

    fn disconnect(&mut self) {
        for (_, reply) in self.requests.drain() {
            let _ = reply.send(Err(Error::Disconnected));
        }
    }
}

struct PendingRequestGuard {
    commands: mpsc::UnboundedSender<Request>,
    id: String,
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        let _ = self.commands.send(Request::Forget(self.id.clone()));
    }
}
pub(crate) struct Rpc {
    commands: mpsc::UnboundedSender<Request>,
    events: broadcast::Sender<Value>,
    alive: Arc<AtomicBool>,
    control_token: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Rpc {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Rpc {
    pub(crate) async fn start(
        config: &PiConfiguration,
        cwd: &Path,
        session: Option<&Path>,
        permission_file: Option<&Path>,
    ) -> Result<Arc<Self>> {
        require(
            session.is_none() || permission_file.is_some(),
            "Pi session launch requires its owned approval policy path",
        )?;
        let control_token = uuid::Uuid::new_v4().to_string();
        let mut command = Command::new(&config.executable);
        command
            .args(&config.arguments)
            .args(["--mode", "rpc", "--no-extensions"]);
        command
            .arg("-e")
            .arg(config.data_root.join("agentkit-control.mjs"));
        command
            .arg("--session-dir")
            .arg(config.data_root.join("sessions"));
        for extension in &config.extensions {
            command.arg("-e").arg(extension);
        }
        if let Some(session) = session {
            command.arg("--session").arg(session);
        } else {
            command.arg("--no-session");
        }
        command
            .current_dir(cwd)
            .env_clear()
            .envs(&config.environment)
            // This is an AgentKit-owned, session-scoped authority. A host-wide
            // environment setting must never select another session's policy.
            .env_remove("AGENTKIT_PERMISSION_FILE")
            .env("PI_CODING_AGENT_DIR", config.data_root.join("agent"))
            .env("AGENTKIT_CONTROL_TOKEN", &control_token)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(permission_file) = permission_file {
            command.env("AGENTKIT_PERMISSION_FILE", permission_file);
        }
        let mut child = command
            .spawn()
            .map_err(|e| Error::Transport(format!("Pi launch: {e}")))?;
        let mut input = child.stdin.take().ok_or(Error::Disconnected)?;
        let mut output = FramedRead::new(
            child.stdout.take().ok_or(Error::Disconnected)?,
            LinesCodec::new_with_max_length(MAX_RECORD),
        );
        let mut stderr = child.stderr.take().ok_or(Error::Disconnected)?;
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
        let (commands, mut receive) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(1024);
        let event_sender = events.clone();
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        let reading_token = control_token.clone();
        let task = tokio::spawn(async move {
            let mut reading = crate::reading::ReadingTracker::default();
            let mut pending = PendingRequests::default();
            loop {
                tokio::select! {
                    command = receive.recv() => {
                        let value = match command {
                            Some(Request::Call(value, reply)) => {
                                let id = value["id"].as_str().unwrap_or("").to_owned();
                                pending.insert(id, reply); value
                            }
                            Some(Request::Ui(value)) => value,
                            Some(Request::Forget(id)) => { pending.forget(&id); continue; }
                            Some(Request::Close) | None => break,
                        };
                        let mut bytes = value.to_string().into_bytes(); bytes.push(b'\n');
                        if bytes.len() > MAX_RECORD || input.write_all(&bytes).await.is_err() { break; }
                    }
                    record = output.next() => {
                        let value: Value = match record {
                            Some(Ok(line)) => match serde_json::from_str(&line) { Ok(v) => v, Err(_) => break },
                            _ => break,
                        };
                        let value=match reading.receive(value,&reading_token){Ok(Some(value))=>value,Ok(None)=>continue,Err(_)=>break};
                        if value["type"] == "response" {
                            pending.respond(&value);
                        } else { let _ = event_sender.send(value); }
                    }
                }
            }
            running.store(false, Ordering::Release);
            pending.disconnect();
            let _ = event_sender.send(json!({"type":"agentkit/transport-closed"}));
            let _ = input.shutdown().await;
            let _ = child.start_kill();
            let _ = child.wait().await;
        });
        Ok(Arc::new(Self {
            commands,
            events,
            alive,
            control_token,
            task,
        }))
    }
    pub(crate) async fn control(&self, method: &str, params: Value) -> Result<Value> {
        use base64::Engine;
        let id = uuid::Uuid::new_v4().to_string();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(serde_json::to_vec(
            &json!({"id":id,"token":self.control_token,"method":method,"params":params}),
        )?);
        let mut events = self.events();
        let response = self
            .request(
                "prompt",
                json!({"message":format!("/agentkit-control {encoded}")}),
            )
            .await?;
        require(
            response["disposition"] == "handled",
            "host control not handled",
        )?;
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let event = events.recv().await.map_err(|_| Error::Disconnected)?;
                if let Some(message) = event["message"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("agentkit-control/v1:"))
                {
                    let value: Value = serde_json::from_str(message)?;
                    if value["id"] != id {
                        continue;
                    }
                    return if value["success"] == true {
                        Ok(value.get("data").cloned().unwrap_or(Value::Null))
                    } else {
                        Err(Error::Remote(
                            json!({"code":"pi/host-control","message":value["error"]}),
                        ))
                    };
                }
            }
        })
        .await
        .map_err(|_| Error::Timeout)?
    }
    pub(crate) fn host_request(&self, event: &Value) -> Option<Value> {
        let text = event["message"]
            .as_str()?
            .strip_prefix("agentkit-host/v1:")?;
        if text.len() > 128 * 1024 {
            return None;
        }
        let value: Value = serde_json::from_str(text).ok()?;
        (value["token"] == self.control_token
            && value["requestId"].as_str().is_some_and(|s| !s.is_empty())
            && value["method"]
                .as_str()
                .is_some_and(|s| s.starts_with("subagents.")))
        .then_some(value)
    }
    pub(crate) fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub(crate) fn events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    pub(crate) async fn request(&self, command: &str, fields: Value) -> Result<Value> {
        self.request_with_timeout(command, fields, Duration::from_secs(30))
            .await
    }

    async fn request_with_timeout(
        &self,
        command: &str,
        mut fields: Value,
        timeout: Duration,
    ) -> Result<Value> {
        require(self.alive(), "Pi process is not running")?;
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, response) = oneshot::channel();
        fields["type"] = json!(command);
        fields["id"] = json!(id);
        self.commands
            .send(Request::Call(fields, reply))
            .map_err(|_| Error::Disconnected)?;
        let _pending = PendingRequestGuard {
            commands: self.commands.clone(),
            id,
        };
        tokio::time::timeout(timeout, response)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)?
    }

    pub(crate) async fn session_state(&self) -> Result<(Value, Value)> {
        // Pi 1.3 captures get_entries synchronously before handling the next
        // input line. Submit it before the fresh identity read, but wait for all
        // three replies concurrently. Full history stays on its standard wire.
        let (mut entries, state, identities) = tokio::try_join!(
            biased;
            self.request("get_entries", json!({})),
            self.request("get_state", json!({})),
            self.control("reading.identities", json!({}))
        )?;
        entries
            .as_object_mut()
            .ok_or_else(|| Error::Protocol("Pi entries snapshot must be an object".into()))?
            .insert("readingIdentities".into(), identities);
        Ok((entries, state))
    }

    pub(crate) fn ui_response(&self, id: &str, mut fields: Value) -> Result<()> {
        fields["type"] = json!("extension_ui_response");
        fields["id"] = json!(id);
        self.commands
            .send(Request::Ui(fields))
            .map_err(|_| Error::Disconnected)
    }
    pub(crate) fn refresh(&self) {
        let _ = self.events.send(json!({"type":"agentkit/refresh"}));
    }
    pub(crate) fn close(&self) {
        let _ = self.commands.send(Request::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct TestActor {
        pending: Arc<Mutex<PendingRequests>>,
        task: tokio::task::JoinHandle<()>,
    }

    fn test_rpc() -> (Arc<Rpc>, mpsc::UnboundedReceiver<Value>, TestActor) {
        let (commands, mut receive) = mpsc::unbounded_channel();
        let (calls, call_receive) = mpsc::unbounded_channel();
        let pending = Arc::new(Mutex::new(PendingRequests::default()));
        let actor_pending = pending.clone();
        let alive = Arc::new(AtomicBool::new(true));
        let actor_alive = alive.clone();
        let actor = tokio::spawn(async move {
            while let Some(command) = receive.recv().await {
                match command {
                    Request::Call(value, reply) => {
                        let id = value["id"].as_str().unwrap_or("").to_owned();
                        actor_pending.lock().unwrap().insert(id, reply);
                        let _ = calls.send(value);
                    }
                    Request::Forget(id) => actor_pending.lock().unwrap().forget(&id),
                    Request::Close => break,
                    Request::Ui(_) => {}
                }
            }
            actor_alive.store(false, Ordering::Release);
            actor_pending.lock().unwrap().disconnect();
        });
        let rpc = Arc::new(Rpc {
            commands,
            events: broadcast::channel(8).0,
            alive,
            control_token: String::new(),
            task: tokio::spawn(async {}),
        });
        (
            rpc,
            call_receive,
            TestActor {
                pending,
                task: actor,
            },
        )
    }

    async fn next_call(calls: &mut mpsc::UnboundedReceiver<Value>) -> Value {
        tokio::time::timeout(Duration::from_secs(1), calls.recv())
            .await
            .expect("request was enqueued")
            .expect("test actor remains connected")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn session_rpc_binds_its_owned_permission_file_over_global_environment() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("agentkit-rpc-env-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let executable = root.join("fake-pi");
        tokio::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s' \"$AGENTKIT_PERMISSION_FILE\" > \"$AGENTKIT_CAPTURE\"\nexec /bin/sleep 5\n",
        )
        .await
        .unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();

        let mut config = PiConfiguration::new(executable, root.join("data"));
        config.environment.insert(
            "AGENTKIT_PERMISSION_FILE".into(),
            "/global/override.json".into(),
        );
        assert!(
            Rpc::start(&config, &root, Some(&root.join("missing.jsonl")), None)
                .await
                .is_err(),
            "a session RPC cannot launch without its owned policy path"
        );
        let mut sessions = Vec::new();
        for index in 1..=2 {
            let session_file = root.join(format!("session-{index}.jsonl"));
            let policy_file = root.join(format!("owned-{index}.json"));
            let capture_file = root.join(format!("captured-{index}.txt"));
            config.environment.insert(
                "AGENTKIT_CAPTURE".into(),
                capture_file.display().to_string(),
            );
            let rpc = Rpc::start(&config, &root, Some(&session_file), Some(&policy_file))
                .await
                .unwrap();
            sessions.push((rpc, capture_file, policy_file));
        }

        for (rpc, capture, expected) in &sessions {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if tokio::fs::read_to_string(capture)
                        .await
                        .is_ok_and(|value| value == expected.display().to_string())
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("fake Pi recorded its startup environment");
            assert_eq!(
                tokio::fs::read_to_string(capture).await.unwrap(),
                expected.display().to_string()
            );
            rpc.close();
        }
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    async fn wait_for_pending(actor: &TestActor, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if actor.pending.lock().unwrap().requests.len() == expected {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending request table converged");
    }

    fn respond(actor: &TestActor, call: &Value, success: bool, data: Value) {
        actor.pending.lock().unwrap().respond(&json!({
            "id":call["id"], "success":success, "data":data,
            "error":if success {Value::Null} else {json!("test failure")}
        }));
    }

    fn reading_control_id(call: &Value, method: &str) -> String {
        use base64::Engine;
        assert_eq!(call["type"], "prompt");
        let encoded = call["message"]
            .as_str()
            .unwrap()
            .strip_prefix("/agentkit-control ")
            .unwrap();
        let value: Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["method"], method);
        value["id"].as_str().unwrap().to_owned()
    }

    fn control_reply(rpc: &Rpc, id: &str, data: Value) {
        let value = json!({"id":id,"success":true,"data":data});
        let _ = rpc.events.send(json!({
            "type":"extension_ui_request",
            "method":"notify",
            "message":format!("agentkit-control/v1:{value}")
        }));
    }

    #[tokio::test]
    async fn session_state_enqueues_entries_state_identities_before_waiting_for_replies() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        assert_eq!(entries_call["type"], "get_entries");
        assert_eq!(state_call["type"], "get_state");
        let control_id = reading_control_id(&identity_call, "reading.identities");
        assert_ne!(entries_call["id"], state_call["id"]);
        assert_ne!(state_call["id"], identity_call["id"]);
        assert_eq!(actor.pending.lock().unwrap().requests.len(), 3);

        // The final request can answer first, including its notification before
        // the handled acknowledgement. Unrelated controls cannot bind a read.
        control_reply(&rpc, "unrelated-control", json!([{"entryId":"wrong"}]));
        control_reply(
            &rpc,
            &control_id,
            json!([{"streamId":"stream","entryId":"leaf"}]),
        );
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        respond(&actor, &state_call, true, json!({"isStreaming":true}));
        assert!(!request.is_finished(), "entries remain required");
        respond(
            &actor,
            &entries_call,
            true,
            json!({"leafId":"leaf","entries":[{"id":"leaf"}]}),
        );
        let (entries, state) = request.await.unwrap().unwrap();
        assert_eq!(entries["leafId"], "leaf");
        assert_eq!(entries["readingIdentities"][0]["entryId"], "leaf");
        assert_eq!(state["isStreaming"], true);
        wait_for_pending(&actor, 0).await;
        assert!(rpc.alive());
        assert!(calls.try_recv().is_err(), "each read is sent only once");
        actor.task.abort();
    }

    #[tokio::test]
    async fn full_entries_use_standard_response_not_control_notification() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        assert_eq!(entries_call["type"], "get_entries");
        let control_id = reading_control_id(&identity_call, "reading.identities");
        let text = "\"".repeat(1_000_000);
        respond(
            &actor,
            &entries_call,
            true,
            json!({"entries":[{"id":"large","text":text}],"leafId":"large"}),
        );
        respond(&actor, &state_call, true, json!({"isStreaming":false}));
        control_reply(&rpc, &control_id, json!([]));
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        let (entries, _) = request.await.unwrap().unwrap();
        assert_eq!(
            entries["entries"][0]["text"].as_str().unwrap().len(),
            1_000_000
        );
        assert_eq!(entries["readingIdentities"], json!([]));
        wait_for_pending(&actor, 0).await;
        assert!(calls.try_recv().is_err());
        actor.task.abort();
    }

    #[tokio::test]
    async fn standard_entries_failure_forgets_both_siblings_without_retry() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        respond(&actor, &entries_call, false, Value::Null);
        assert!(request.await.unwrap().is_err());
        wait_for_pending(&actor, 0).await;
        assert!(rpc.alive());
        respond(&actor, &state_call, true, json!({}));
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        assert!(
            calls.try_recv().is_err(),
            "failure is not retried or replaced"
        );
        actor.task.abort();
    }

    #[tokio::test]
    async fn identity_control_failure_is_not_retried_or_replaced_with_empty_identities() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        respond(
            &actor,
            &entries_call,
            true,
            json!({"entries":[],"leafId":null}),
        );
        respond(&actor, &state_call, true, json!({}));
        respond(&actor, &identity_call, false, Value::Null);
        assert!(matches!(request.await.unwrap(), Err(Error::Remote(_))));
        wait_for_pending(&actor, 0).await;
        assert!(calls.try_recv().is_err());
        actor.task.abort();
    }

    #[tokio::test]
    async fn malformed_standard_entries_is_not_replaced_with_a_control_snapshot() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        let control_id = reading_control_id(&identity_call, "reading.identities");
        respond(&actor, &entries_call, true, json!([]));
        respond(&actor, &state_call, true, json!({}));
        control_reply(&rpc, &control_id, json!([]));
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        assert!(matches!(request.await.unwrap(), Err(Error::Protocol(_))));
        wait_for_pending(&actor, 0).await;
        assert!(calls.try_recv().is_err());
        actor.task.abort();
    }

    #[tokio::test]
    async fn state_error_after_control_ack_does_not_adopt_late_identities() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let first = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        let old_control = reading_control_id(&identity_call, "reading.identities");
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        wait_for_pending(&actor, 2).await;
        respond(&actor, &state_call, false, Value::Null);
        assert!(first.await.unwrap().is_err());
        wait_for_pending(&actor, 0).await;
        respond(
            &actor,
            &entries_call,
            true,
            json!({"entries":[],"leafId":"old"}),
        );

        let reader = rpc.clone();
        let next = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        let current_control = reading_control_id(&identity_call, "reading.identities");
        assert_ne!(old_control, current_control);
        control_reply(&rpc, &old_control, json!([{"entryId":"stale"}]));
        respond(
            &actor,
            &entries_call,
            true,
            json!({"entries":[],"leafId":"current"}),
        );
        respond(&actor, &state_call, true, json!({"isStreaming":false}));
        respond(
            &actor,
            &identity_call,
            true,
            json!({"disposition":"handled"}),
        );
        control_reply(&rpc, &current_control, json!([]));
        let (snapshot, _) = next.await.unwrap().unwrap();
        assert_eq!(snapshot["leafId"], "current");
        assert_eq!(snapshot["readingIdentities"], json!([]));
        wait_for_pending(&actor, 0).await;
        assert!(rpc.alive());
        assert!(calls.try_recv().is_err());
        actor.task.abort();
    }

    #[tokio::test]
    async fn cancelling_outer_read_forgets_all_three_pending_requests() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let entries_call = next_call(&mut calls).await;
        let state_call = next_call(&mut calls).await;
        let identity_call = next_call(&mut calls).await;
        assert_eq!(actor.pending.lock().unwrap().requests.len(), 3);
        request.abort();
        let _ = request.await;
        wait_for_pending(&actor, 0).await;
        assert!(rpc.alive());
        for call in [&entries_call, &state_call, &identity_call] {
            respond(&actor, call, true, json!({}));
        }
        assert!(actor.pending.lock().unwrap().requests.is_empty());
        assert!(calls.try_recv().is_err());
        actor.task.abort();
    }

    #[tokio::test]
    async fn timeout_forgets_request_from_actor_pending_table() {
        let (rpc, mut calls, actor) = test_rpc();
        let reader = rpc.clone();
        let request = tokio::spawn(async move {
            reader
                .request_with_timeout("get_state", json!({}), Duration::from_millis(1))
                .await
        });
        let _call = next_call(&mut calls).await;
        assert!(matches!(request.await.unwrap(), Err(Error::Timeout)));
        wait_for_pending(&actor, 0).await;
        assert!(rpc.alive(), "timeout cleanup keeps the actor alive");
        assert!(calls.try_recv().is_err(), "timeout does not retry the read");
        actor.task.abort();
    }

    #[tokio::test]
    async fn actor_disconnect_finishes_all_reads_without_publishing_a_frame() {
        let (rpc, mut calls, actor) = test_rpc();
        let closer = rpc.clone();
        let reader = rpc.clone();
        let request = tokio::spawn(async move { reader.session_state().await });
        let _entries_call = next_call(&mut calls).await;
        let _state_call = next_call(&mut calls).await;
        let _identity_call = next_call(&mut calls).await;
        assert_eq!(actor.pending.lock().unwrap().requests.len(), 3);

        closer.close();
        assert!(matches!(request.await.unwrap(), Err(Error::Disconnected)));
        wait_for_pending(&actor, 0).await;
        actor.task.abort();
    }
}
