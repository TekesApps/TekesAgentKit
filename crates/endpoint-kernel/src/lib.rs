//! Authenticated Kernel v4 HTTP / multiplexed WebSocket client. No Kernel-private storage access.
use async_trait::async_trait;
use futures_util::{SinkExt, Stream, StreamExt};
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tekes_agent_contract::*;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

const FRAME_LIMIT: usize = 8 * 1024 * 1024; // NativeSessionEndpoint's current finite ceiling.
const STREAM_BUFFER: usize = 256;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn id() -> String {
    format!("agentkit-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}
fn transport(error: impl std::fmt::Display) -> Error {
    Error::Transport(error.to_string())
}

/// Host supplies endpoint identity, URL and credential. This type deliberately does not implement Debug.
#[derive(Clone)]
pub struct KernelConnector {
    identity: String,
    url: Url,
    bearer: String,
    timeout: Duration,
}
impl KernelConnector {
    pub fn new(identity: impl Into<String>, url: &str, bearer: String) -> Result<Self> {
        let identity = identity.into();
        let url = Url::parse(url).map_err(transport)?;
        require(
            !identity.is_empty()
                && matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some(),
            "invalid endpoint identity or URL",
        )?;
        require(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "endpoint URL contains credentials/query/fragment",
        )?;
        require(
            !bearer.is_empty() && !bearer.contains(['\r', '\n']),
            "invalid bearer",
        )?;
        Ok(Self {
            identity,
            url,
            bearer,
            timeout: Duration::from_secs(30),
        })
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    pub async fn connect_kernel(&self) -> Result<NativeSessionEndpoint> {
        let mut url = self.url.clone();
        url.set_scheme(if self.url.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .map_err(|_| Error::Protocol("invalid websocket scheme".into()))?;
        url.set_path("/api/remote.mux");
        let mut request = url.as_str().into_client_request().map_err(transport)?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", self.bearer)
                .parse()
                .map_err(|_| Error::Protocol("invalid bearer header".into()))?,
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(FRAME_LIMIT))
            .max_frame_size(Some(FRAME_LIMIT));
        let (mut ws, _) = tokio::time::timeout(
            self.timeout,
            connect_async_with_config(request, Some(config), false),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(transport)?;
        let ready = tokio::time::timeout(self.timeout, ws.next())
            .await
            .map_err(|_| Error::Timeout)?
            .ok_or(Error::Disconnected)?
            .map_err(transport)?;
        let value: Value = serde_json::from_slice(&ready.into_data())?;
        require(
            string(&value, "type")? == "ready",
            "first frame is not ready",
        )?;
        require(
            !string(&value, "generation")?.is_empty(),
            "empty physical generation",
        )?;
        let host: HostDescription = serde_json::from_value(value["host"].clone())?;
        host.validate_kernel()?;
        let generation = string(&value, "generation")?.to_owned();
        let http = Client::builder()
            .timeout(self.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(transport)?;
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let mut streams: HashMap<String, mpsc::Sender<Result<Value>>> = HashMap::new();
            let mut replies: HashMap<String, oneshot::Sender<Result<Value>>> = HashMap::new();
            let failure = loop {
                tokio::select! {
                    command = receiver.recv() => {
                        let Some(command) = command else { break Error::Disconnected; };
                        let wire = match command {
                            Command::Open { id, target, output, ack } => {
                                streams.insert(id.clone(), output);
                                let wire = json!({"type":"open","streamId":id,"target":target});
                                match ws.send(Message::Text(wire.to_string().into())).await {
                                    Ok(()) => { let _ = ack.send(Ok(())); continue; }
                                    Err(e) => { let _ = ack.send(Err(Error::Disconnected)); break transport(e); }
                                }
                            }
                            Command::Close(id) => { streams.remove(&id); json!({"type":"close","streamId":id}) }
                            Command::Request { id, mut value, reply } => {
                                value["requestId"] = json!(id); replies.insert(id, reply); value
                            }
                            Command::Forget(id) => { replies.remove(&id); continue; }
                            Command::Shutdown => { let _ = ws.close(None).await; break Error::Disconnected; }
                        };
                        if let Err(e) = ws.send(Message::Text(wire.to_string().into())).await { break transport(e); }
                    }
                    message = ws.next() => {
                        let value: Value = match message {
                            Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) { Ok(v) => v, Err(e) => break Error::Json(e) },
                            Some(Ok(Message::Binary(bytes))) => match serde_json::from_slice(&bytes) { Ok(v) => v, Err(e) => break Error::Json(e) },
                            Some(Ok(Message::Ping(bytes))) => { if let Err(e) = ws.send(Message::Pong(bytes)).await { break transport(e); } continue; }
                            Some(Ok(Message::Pong(_))) => continue,
                            _ => break Error::Disconnected,
                        };
                        match value["type"].as_str() {
                            Some("stream") => {
                                let Some(stream_id) = value["streamId"].as_str() else { break Error::Protocol("missing streamId".into()); };
                                // Closed streams can still have in-flight frames. Discard without validating.
                                if let Some(output) = streams.get(stream_id) {
                                    let frame = value["frame"].clone();
                                    let g = frame.get("generation").or_else(|| frame.get("snapshot").and_then(|s| s.get("generation"))).and_then(Value::as_str);
                                    if g != Some(generation.as_str()) { break Error::Protocol("physical generation mismatch".into()); }

                                    match output.try_send(Ok(frame)) {
                                        Ok(()) => {},
                                        Err(mpsc::error::TrySendError::Closed(_)) => { streams.remove(stream_id); },
                                        Err(mpsc::error::TrySendError::Full(_)) => break Error::Protocol("stream buffer overflow; rebaseline required".into()),
                                    }
                                }
                            }
                            Some("error") => {
                                let error = value["error"].clone();
                                if let Some(reply) = value["requestId"].as_str().and_then(|id| replies.remove(id)) { let _ = reply.send(Err(Error::Remote(error))); }
                                else if let Some(stream_id) = value["streamId"].as_str() {
                                    if let Some(output) = streams.remove(stream_id) { let _ = output.try_send(Err(Error::Remote(error))); }
                                } else { break Error::Remote(error); }
                            }
                            Some("journal-page-result" | "actionable-response-result") => {
                                if let Some(reply) = value["requestId"].as_str().and_then(|id| replies.remove(id)) {
                                    let result = if value["type"] == "journal-page-result" { value["page"].clone() } else { value.clone() };
                                    let _ = reply.send(Ok(result));
                                }
                            }
                            _ => break Error::Protocol("unknown mux frame".into()),
                        }
                    }
                }
            };
            // Every stream terminates; runtime owns recovery. Never silently drop an event.
            let message = failure.to_string();
            for (_, output) in streams {
                let _ = output.try_send(Err(Error::Transport(message.clone())));
            }
            for (_, reply) in replies {
                let _ = reply.send(Err(Error::Disconnected));
            }
        });
        Ok(NativeSessionEndpoint {
            config: self.clone(),
            host,
            http,
            commands,
            task,
        })
    }
}
#[async_trait]
impl EndpointConnector for KernelConnector {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(self.connect_kernel().await?))
    }
}

enum Command {
    Open {
        id: String,
        target: StreamTarget,
        output: mpsc::Sender<Result<Value>>,
        ack: oneshot::Sender<Result<()>>,
    },
    Close(String),
    Request {
        id: String,
        value: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
    Forget(String),
    Shutdown,
}
pub struct NativeSessionEndpoint {
    config: KernelConnector,
    host: HostDescription,
    http: Client,
    commands: mpsc::UnboundedSender<Command>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for NativeSessionEndpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct LogicalStream {
    id: String,
    commands: mpsc::UnboundedSender<Command>,
    receiver: mpsc::Receiver<Result<Value>>,
}
impl Stream for LogicalStream {
    type Item = Result<Value>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}
impl Drop for LogicalStream {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Close(self.id.clone()));
    }
}
struct PendingRequest {
    id: String,
    commands: mpsc::UnboundedSender<Command>,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Forget(self.id.clone()));
    }
}
impl NativeSessionEndpoint {
    async fn mux_request(&self, value: Value) -> Result<Value> {
        let id = id();
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::Request {
                id: id.clone(),
                value,
                reply,
            })
            .map_err(|_| Error::Disconnected)?;
        let _cleanup = PendingRequest {
            id,
            commands: self.commands.clone(),
        };
        tokio::time::timeout(self.config.timeout, result)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)?
    }
}
#[async_trait]
impl SessionEndpoint for NativeSessionEndpoint {
    fn identity(&self) -> &str {
        &self.config.identity
    }
    fn host(&self) -> &HostDescription {
        &self.host
    }
    fn supports_recovery(&self) -> bool {
        true
    }
    async fn file_stat(&self, session_id: &str, path: &str) -> Result<SessionFileStat> {
        serde_json::from_value(
            self.call(
                "session.files.stat",
                json!({"sessionId":session_id,"path":path}),
                &id(),
            )
            .await?,
        )
        .map_err(Error::from)
    }
    async fn file_read_bytes(
        &self,
        session_id: &str,
        path: &str,
        offset: u64,
        length: usize,
    ) -> Result<SessionFileBytePage> {
        require(length > 0 && length <= 4194304, "invalid byte page length")?;
        serde_json::from_value(
            self.call(
                "session.files.readBytes",
                json!({"sessionId":session_id,"path":path,"offset":offset,"limit":length}),
                &id(),
            )
            .await?,
        )
        .map_err(Error::from)
    }
    async fn file_read_text(
        &self,
        session_id: &str,
        path: &str,
        offset: u64,
        limit: usize,
    ) -> Result<SessionFileTextPage> {
        require(
            offset >= 1 && limit > 0 && limit <= 10000,
            "invalid text page",
        )?;
        serde_json::from_value(
            self.call(
                "session.files.read",
                json!({"sessionId":session_id,"path":path,"offset":offset,"limit":limit}),
                &id(),
            )
            .await?,
        )
        .map_err(Error::from)
    }
    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        if matches!(target, StreamTarget::SessionConversation { .. }) {
            return Err(unsupported("Kernel native conversation stream"));
        }
        if let StreamTarget::SessionJournal { max_messages, .. } = &target {
            require(
                *max_messages > 0 && *max_messages <= 500,
                "invalid maxMessages",
            )?;
        }
        let id = id();
        let (output, receiver) = mpsc::channel(STREAM_BUFFER);
        let (ack, result) = oneshot::channel();
        let stream = LogicalStream {
            id: id.clone(),
            commands: self.commands.clone(),
            receiver,
        };
        self.commands
            .send(Command::Open {
                id,
                target,
                output,
                ack,
            })
            .map_err(|_| Error::Disconnected)?;
        tokio::time::timeout(self.config.timeout, result)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)??;
        Ok(Box::pin(stream))
    }
    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        request.validate()?;
        let mut value = serde_json::to_value(&request)?;
        value
            .as_object_mut()
            .expect("request object")
            .remove("sessionId");
        value["type"] = json!("journal-page");
        value["address"] = json!({"sessionId":request.session_id});
        let page: HistoryPage = serde_json::from_value(self.mux_request(value).await?)?;
        page.validate(&request)?;
        Ok(page)
    }
    async fn respond(
        &self,
        actionable_id: &str,
        expected_revision: i64,
        outcome: Value,
    ) -> Result<()> {
        require(expected_revision >= 0, "invalid expectedRevision")?;
        self.mux_request(json!({"type":"actionable-respond","actionableId":actionable_id,"expectedRevision":expected_revision,"outcome":outcome})).await?;
        Ok(())
    }
    async fn call(&self, method: &str, mut payload: Value, rpc_id: &str) -> Result<Value> {
        require(
            !rpc_id.is_empty()
                && !method.is_empty()
                && method.split('/').all(|p| !matches!(p, "." | ".."))
                && !method.contains(['?', '#']),
            "invalid unary request",
        )?;
        require(payload.is_object(), "unary payload must be object")?;
        if method == "session.create" && payload.get("execution").is_none() {
            let cwd = string(&payload, "cwd")?.to_owned();
            payload
                .as_object_mut()
                .expect("payload object")
                .remove("cwd");
            payload["execution"] = json!({"cwd":cwd,"read_roots":[],"writable_roots":[],"toolchain_roots":[],"network":false,"allowed_tools":[],"mcp":[]});
        }
        if method.starts_with("workspace.") {
            return Err(Error::Unsupported("Kernel v4 has no workspace RPC".into()));
        }
        let mut url = self.config.url.clone();
        url.set_path(&format!("/api/{method}"));
        let mut response = self
            .http
            .post(url)
            .bearer_auth(&self.config.bearer)
            .json(
                &json!({"type":"client-request","rpcId":rpc_id,"method":method,"payload":payload}),
            )
            .send()
            .await
            .map_err(|_| {
                Error::Transport("HTTP request failed; reconcile mutation receipt".into())
            })?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Transport("HTTP response incomplete".into()))?
        {
            require(
                bytes.len() + chunk.len() <= FRAME_LIMIT,
                "HTTP response too large",
            )?;
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        require(
            value["type"] == "server-response" && value["rpcId"] == rpc_id,
            "unary reply correlation mismatch",
        )?;
        match value["result"]["ok"].as_bool() {
            Some(false) => Err(Error::Remote(value["result"]["error"].clone())),
            Some(true) => value["result"]
                .get("value")
                .cloned()
                .ok_or_else(|| Error::Protocol("missing result value".into())),
            None => Err(Error::Protocol("invalid result envelope".into())),
        }
    }
    async fn close(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}
