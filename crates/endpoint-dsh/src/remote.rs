use crate::{DshConfiguration, multipart};
use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll},
};
use tekes_agent_contract::*;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};
const LIMIT: usize = 100 * 1024 * 1024;
enum Command {
    Open {
        id: String,
        endpoint: String,
        args: Value,
        output: mpsc::Sender<Result<Value>>,
        ack: oneshot::Sender<Result<()>>,
    },
    Cancel(String),
    Close,
}
pub(crate) struct Remote {
    config: DshConfiguration,
    client: reqwest::Client,
    commands: mpsc::UnboundedSender<Command>,
    task: tokio::task::JoinHandle<()>,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct Logical {
    id: String,
    commands: mpsc::UnboundedSender<Command>,
    receiver: mpsc::Receiver<Result<Value>>,
}
impl Stream for Logical {
    type Item = Result<Value>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}
impl Drop for Logical {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Cancel(self.id.clone()));
    }
}
impl Remote {
    pub(crate) async fn connect(config: DshConfiguration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Transport("DSH HTTP client".into()))?;
        let mut url = config
            .base_url
            .join("api/remote.mux")
            .map_err(|_| Error::Protocol("invalid DSH URL".into()))?;
        url.set_scheme(if config.base_url.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .map_err(|_| Error::Protocol("invalid DSH URL".into()))?;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|_| Error::Protocol("invalid DSH request".into()))?;
        if let Some((name, value)) = &config.authorization {
            request.headers_mut().insert(
                name.parse::<tokio_tungstenite::tungstenite::http::HeaderName>()
                    .map_err(|_| Error::Protocol("invalid header".into()))?,
                value
                    .parse()
                    .map_err(|_| Error::Protocol("invalid authorization".into()))?,
            );
        }
        let (mut socket, _) = tokio::time::timeout(
            config.timeout,
            connect_async_with_config(
                request,
                Some(
                    WebSocketConfig::default()
                        .max_message_size(Some(LIMIT))
                        .max_frame_size(Some(LIMIT)),
                ),
                false,
            ),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(|_| Error::Transport("DSH websocket handshake failed".into()))?;
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_closed = closed.clone();
        let task = tokio::spawn(async move {
            let mut streams = HashMap::<String, mpsc::Sender<Result<Value>>>::new();
            loop {
                tokio::select! {
                    command=receiver.recv()=>{
                        let value=match command{
                            Some(Command::Open{id,endpoint,args,output,ack})=>{
                                streams.insert(id.clone(),output);
                                let result=socket.send(Message::Text(json!({"type":"open","streamId":id,"endpoint":endpoint,"payload":{"args":args}}).to_string().into())).await;
                                let okay=result.is_ok();let _=ack.send(if okay{Ok(())}else{Err(Error::Disconnected)});if !okay{break;}continue;
                            }
                            Some(Command::Cancel(id))=>{streams.remove(&id);json!({"type":"cancel","streamId":id})}
                            Some(Command::Close)|None=>{let _=socket.close(None).await;break;}
                        };
                        if socket.send(Message::Text(value.to_string().into())).await.is_err(){break;}
                    }
                    message=socket.next()=>{
                        let frame:Value=match message{
                            Some(Ok(Message::Text(text)))=>match serde_json::from_str(&text){Ok(v)=>v,Err(_)=>break},
                            Some(Ok(Message::Ping(bytes)))=>{if socket.send(Message::Pong(bytes)).await.is_err(){break;}continue;}
                            Some(Ok(Message::Pong(_)))=>continue,
                            _=>break,
                        };
                        let Some(id)=frame["streamId"].as_str()else{break;};
                        let Some(output)=streams.get(id)else{continue;}; // A cancelled stream's in-flight frames are harmless.
                        match frame["type"].as_str(){
                            Some("item")=>{
                                if frame.get("error").is_some(){break;}
                                if let Some(value)=frame.get("value").filter(|v|!v.is_null()){
                                    match output.try_send(Ok(value.clone())) {
                                        Ok(()) => {},
                                        Err(mpsc::error::TrySendError::Closed(_)) => { streams.remove(id); },
                                        Err(mpsc::error::TrySendError::Full(_)) => break,
                                    }
                                }
                            }
                            Some("error")=>{if let Some(output)=streams.remove(id){let _=output.try_send(Err(Error::Remote(frame["error"].clone())));}}
                            Some("end")=>{streams.remove(id);}
                            _=>break,
                        }
                    }
                }
            }
            task_closed.store(true, std::sync::atomic::Ordering::Release);
            for (_, output) in streams {
                let _ = output.try_send(Err(Error::Disconnected));
            }
        });
        Ok(Self {
            config,
            client,
            commands,
            task,
            closed,
        })
    }
    pub(crate) async fn open(&self, endpoint: &str, args: Value) -> Result<FrameStream> {
        let id = uuid::Uuid::new_v4().to_string();
        let (output, receiver) = mpsc::channel(256);
        let (ack, response) = oneshot::channel();
        let stream = Logical {
            id: id.clone(),
            commands: self.commands.clone(),
            receiver,
        };
        self.commands
            .send(Command::Open {
                id,
                endpoint: endpoint.into(),
                args,
                output,
                ack,
            })
            .map_err(|_| Error::Disconnected)?;
        tokio::time::timeout(self.config.timeout, response)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Disconnected)??;
        Ok(Box::pin(stream))
    }
    pub(crate) async fn call(&self, endpoint: &str, args: Value, rpc_id: &str) -> Result<Value> {
        require(
            !self.closed.load(std::sync::atomic::Ordering::Acquire),
            "DSH endpoint is closed",
        )?;
        require(
            !rpc_id.is_empty()
                && !endpoint.is_empty()
                && !endpoint.contains(['?', '#'])
                && endpoint.split('/').all(|s| s != ".." && s != "."),
            "invalid DSH call",
        )?;
        let url = self
            .config
            .base_url
            .join(&format!("api/{endpoint}"))
            .map_err(|_| Error::Protocol("invalid DSH URL".into()))?;
        let mut request=self.client.post(url).json(&json!({"type":"client-request","rpcId":rpc_id,"method":endpoint,"payload":{"args":args}}));
        if let Some((name, value)) = &self.config.authorization {
            request = request.header(name, value);
        }
        let response = request.send().await.map_err(|_| {
            Error::Transport("DSH HTTP request failed; reconcile mutation before retry".into())
        })?;
        require(
            response.status() == reqwest::StatusCode::OK,
            "DSH HTTP status is not 200",
        )?;
        let value = self.decode(response).await?;
        require(
            value["type"] == "server-response" && value["rpcId"] == rpc_id,
            "DSH reply correlation mismatch",
        )?;
        match value["result"]["ok"].as_bool() {
            Some(true) => Ok(value["result"].get("value").cloned().unwrap_or(Value::Null)),
            Some(false) => Err(Error::Remote(value["result"]["error"].clone())),
            _ => Err(Error::Protocol("invalid DSH result".into())),
        }
    }
    pub(crate) async fn probe_version(&self) -> Result<String> {
        let mut request = self
            .client
            .get(
                self.config
                    .base_url
                    .join("api/tekes/harness-version")
                    .map_err(|_| Error::Protocol("invalid DSH URL".into()))?,
            )
            .header("Cache-Control", "no-cache");
        if let Some((name, value)) = &self.config.authorization {
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .map_err(|_| Error::Transport("DSH version probe unavailable".into()))?;
        require(
            response.status() == reqwest::StatusCode::OK,
            "DSH official version probe unavailable",
        )?;
        let value = self.decode(response).await?;
        version_evidence(&value, &self.config.supported_versions)
    }

    async fn decode(&self, mut response: reqwest::Response) -> Result<Value> {
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Disconnected)? {
            require(
                bytes.len() + chunk.len() <= LIMIT,
                "DSH response exceeds limit",
            )?;
            bytes.extend_from_slice(&chunk);
        }
        multipart::decode(&bytes, &content_type)
    }
    pub(crate) async fn upload(&self, id: &str, name: &str, data: Vec<u8>) -> Result<Value> {
        require(
            !self.closed.load(std::sync::atomic::Ordering::Acquire),
            "DSH endpoint is closed",
        )?;
        let mut url = self
            .config
            .base_url
            .join("api/session/uploadFileBinary")
            .map_err(|_| Error::Protocol("invalid upload URL".into()))?;
        url.query_pairs_mut()
            .append_pair("sessionId", id)
            .append_pair("name", name);
        let mut request = self
            .client
            .post(url)
            .header("Content-Type", "application/octet-stream")
            .body(data);
        if let Some((name, value)) = &self.config.authorization {
            request = request.header(name, value);
        }
        let response = request.send().await.map_err(|_| Error::Disconnected)?;
        require(
            response.status() == reqwest::StatusCode::OK,
            "DSH upload HTTP status",
        )?;
        let result = self.decode(response).await?;
        if result["ok"] == true {
            Ok(result["value"].clone())
        } else {
            Err(Error::Remote(result["error"].clone()))
        }
    }
    pub(crate) fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = self.commands.send(Command::Close);
    }
}

fn version_evidence(
    value: &Value,
    supported: &std::collections::BTreeSet<String>,
) -> Result<String> {
    require(
        value["product"] == "@deepseek-ai/dsh"
            && value["source"] == "@deepseek-ai/dsh-app-boot#getDshRuntimeVersion",
        "invalid DSH version evidence",
    )?;
    let version = string(value, "version")?.to_owned();
    require(
        supported.contains(&version),
        "unsupported DSH runtime version",
    )?;
    Ok(version)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_versions_require_official_evidence() {
        let config = DshConfiguration::new("dsh", "http://localhost:1/").unwrap();
        for version in ["0.2.0-rc.2", "0.2.1-rc.2"] {
            let evidence = json!({"product":"@deepseek-ai/dsh","source":"@deepseek-ai/dsh-app-boot#getDshRuntimeVersion","version":version});
            assert_eq!(
                version_evidence(&evidence, &config.supported_versions).unwrap(),
                version
            );
        }
        for (version, source) in [
            (
                "0.2.1-rc.3",
                "@deepseek-ai/dsh-app-boot#getDshRuntimeVersion",
            ),
            ("0.2.1-rc.2", "caller supplied"),
        ] {
            assert!(
                version_evidence(
                    &json!({"product":"@deepseek-ai/dsh","source":source,"version":version}),
                    &config.supported_versions
                )
                .is_err()
            );
        }
    }
}
