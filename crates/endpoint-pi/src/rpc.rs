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
    ) -> Result<Arc<Self>> {
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
            .env("PI_CODING_AGENT_DIR", config.data_root.join("agent"))
            .env("AGENTKIT_CONTROL_TOKEN", &control_token)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
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
            let mut pending = HashMap::<String, oneshot::Sender<Result<Value>>>::new();
            loop {
                tokio::select! {
                    command = receive.recv() => {
                        let value = match command {
                            Some(Request::Call(value, reply)) => {
                                let id = value["id"].as_str().unwrap_or("").to_owned();
                                pending.insert(id, reply); value
                            }
                            Some(Request::Ui(value)) => value,
                            Some(Request::Forget(id)) => { pending.remove(&id); continue; }
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
                            if let Some(id) = value["id"].as_str() && let Some(reply) = pending.remove(id) {
                                let result = match value["success"].as_bool() {
                                    Some(true) => Ok(value.get("data").cloned().unwrap_or(Value::Null)),
                                    Some(false) => Err(Error::Remote(json!({"code":"pi/command-failed","message":value["error"]}))),
                                    None => Err(Error::Protocol("invalid Pi response".into())),
                                };
                                let _ = reply.send(result);
                            }
                        } else { let _ = event_sender.send(value); }
                    }
                }
            }
            running.store(false, Ordering::Release);
            for (_, reply) in pending {
                let _ = reply.send(Err(Error::Disconnected));
            }
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
    pub(crate) async fn request(&self, command: &str, mut fields: Value) -> Result<Value> {
        require(self.alive(), "Pi process is not running")?;
        let id = uuid::Uuid::new_v4().to_string();
        let (reply, response) = oneshot::channel();
        fields["type"] = json!(command);
        fields["id"] = json!(id);
        self.commands
            .send(Request::Call(fields, reply))
            .map_err(|_| Error::Disconnected)?;
        let result = tokio::time::timeout(Duration::from_secs(30), response).await;
        let _ = self.commands.send(Request::Forget(id));
        result
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)?
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
