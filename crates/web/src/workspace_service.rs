//! Stateless public WSE transport. A host registers the connection; session
//! Harness identity does not imply that a Workspace Service exists.
use async_trait::async_trait;
use reqwest::{
    Client, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use std::time::Duration;
use tekes_agent_contract::{
    Error, Result, WorkspaceMethod, WorkspaceServiceEndpoint, require, validate_workspace_request,
    validate_workspace_response,
};

/// Host-only construction. Credentials and the service URL never appear in
/// browser descriptions or responses. Replace this connection on host rebind.
pub struct HTTPWorkspaceServiceEndpoint {
    connection_id: String,
    base_url: Url,
    headers: HeaderMap,
    client: Client,
}
impl HTTPWorkspaceServiceEndpoint {
    pub fn new(
        connection_id: impl Into<String>,
        base_url: &str,
        authorization: Option<(String, String)>,
    ) -> Result<Self> {
        let connection_id = connection_id.into();
        let mut base_url = Url::parse(base_url)
            .map_err(|_| Error::Protocol("invalid workspace service URL".into()))?;
        require(
            !connection_id.is_empty()
                && matches!(base_url.scheme(), "http" | "https")
                && base_url.host_str().is_some()
                && base_url.username().is_empty()
                && base_url.password().is_none()
                && base_url.query().is_none()
                && base_url.fragment().is_none(),
            "invalid workspace service connection",
        )?;
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let mut headers = HeaderMap::new();
        if let Some((name, value)) = authorization {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| Error::Protocol("invalid workspace authorization header".into()))?;
            require(
                matches!(name.as_str(), "authorization" | "cookie"),
                "workspace authorization must be an Authorization or Cookie header",
            )?;
            let mut value = HeaderValue::from_str(&value)
                .map_err(|_| Error::Protocol("invalid workspace authorization value".into()))?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| Error::Transport("workspace HTTP client initialization failed".into()))?;
        Ok(Self {
            connection_id,
            base_url,
            headers,
            client,
        })
    }
}

fn uncertain_mutation() -> Error {
    Error::Remote(json!({
        "code":"outcome-unknown",
        "message":"Workspace mutation outcome is unknown; reconcile before retrying"
    }))
}

fn decode_reply(
    method: WorkspaceMethod,
    payload: &Value,
    rpc_id: &str,
    reply: Value,
) -> Result<Value> {
    require(
        reply["type"] == "server-response" && reply["rpcId"] == rpc_id,
        "workspace reply correlation mismatch",
    )?;
    let result = &reply["result"];
    match result["ok"].as_bool() {
        Some(false) => {
            let error = &result["error"];
            require(
                error["code"].as_str().is_some_and(|s| !s.is_empty())
                    && error["message"].as_str().is_some_and(|s| !s.is_empty())
                    && result.get("value").is_none(),
                "invalid workspace failure envelope",
            )?;
            Err(Error::Remote(error.clone()))
        }
        Some(true) => {
            require(
                result.get("error").is_none(),
                "workspace success contains an error",
            )?;
            let value = result
                .get("value")
                .ok_or_else(|| Error::Protocol("missing workspace result value".into()))?;
            validate_workspace_response(method, payload, value)?;
            Ok(value.clone())
        }
        None => Err(Error::Protocol("invalid workspace result envelope".into())),
    }
}

#[async_trait]
impl WorkspaceServiceEndpoint for HTTPWorkspaceServiceEndpoint {
    fn connection_id(&self) -> &str {
        &self.connection_id
    }

    async fn call(&self, method: WorkspaceMethod, payload: Value, rpc_id: &str) -> Result<Value> {
        validate_workspace_request(method, &payload)?;
        require(
            !rpc_id.is_empty() && rpc_id.len() <= 4096,
            "invalid workspace RPC identity",
        )?;
        let name = format!("tekesWorkspace.{}", method.name());
        let url = self
            .base_url
            .join(&format!("api/{name}"))
            .map_err(|_| Error::Protocol("invalid workspace request URL".into()))?;
        let response = self
            .client
            .post(url)
            .headers(self.headers.clone())
            .timeout(if method == WorkspaceMethod::GitPush {
                Duration::from_secs(150)
            } else {
                Duration::from_secs(30)
            })
            .json(&json!({"type":"client-request","rpcId":rpc_id,"method":name,"payload":payload}))
            .send()
            .await
            .map_err(|error| {
                if method.is_mutation() {
                    uncertain_mutation()
                } else if error.is_timeout() {
                    Error::Timeout
                } else {
                    Error::Transport("workspace HTTP request failed".into())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            if method.is_mutation() && status.is_server_error() {
                return Err(uncertain_mutation());
            }
            return Err(Error::Remote(
                json!({"code":format!("http-{}",status.as_u16()),"message":format!("Workspace service returned HTTP {}",status.as_u16())}),
            ));
        }
        let reply: Value = response.json().await.map_err(|_| {
            if method.is_mutation() {
                uncertain_mutation()
            } else {
                Error::Protocol("invalid workspace JSON response".into())
            }
        })?;
        match decode_reply(method, &payload, rpc_id, reply) {
            Err(Error::Protocol(_) | Error::Json(_)) if method.is_mutation() => {
                Err(uncertain_mutation())
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn rejects_wrong_rpc_identity_and_preserves_failure_codes() {
        let payload = json!({"workspaceId":"w","path":""});
        assert!(
            decode_reply(
                WorkspaceMethod::FilesList,
                &payload,
                "one",
                json!({"type":"server-response","rpcId":"two","result":{"ok":true,"value":{}}})
            )
            .is_err()
        );
        let error = decode_reply(WorkspaceMethod::FilesWrite,&payload,"one",json!({"type":"server-response","rpcId":"one","result":{"ok":false,"error":{"code":"revision-conflict","message":"File changed"}}})).unwrap_err();
        assert!(matches!(error,Error::Remote(value) if value["code"]=="revision-conflict"));
    }

    async fn read_http(stream: &mut tokio::net::TcpStream) -> (String, Value) {
        let mut bytes = Vec::new();
        let (header_end, length) = loop {
            let mut buffer = [0_u8; 4096];
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < header_end + length {
            let mut buffer = [0_u8; 4096];
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
        }
        (
            String::from_utf8(bytes[..header_end].to_vec()).unwrap(),
            serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap(),
        )
    }

    #[tokio::test]
    async fn sends_the_public_envelope_once_and_does_not_follow_auth_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let service = HTTPWorkspaceServiceEndpoint::new(
            "fixture",
            &base,
            Some(("Authorization".into(), "Bearer fixture-secret".into())),
        )
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (headers, request) = read_http(&mut stream).await;
            assert!(headers.starts_with("POST /api/tekesWorkspace.filesWrite HTTP/1.1"));
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-secret")
            );
            assert_eq!(request["rpcId"], "stable-save");
            assert_eq!(request["payload"]["expectedRevision"], "old");
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = service
            .call(
                WorkspaceMethod::FilesWrite,
                json!({"workspaceId":"w","path":"proof","content":"YWJj","expectedRevision":"old"}),
                "stable-save",
            )
            .await
            .unwrap_err();
        assert!(matches!(error,Error::Remote(value) if value["code"]=="http-302"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn lost_mutation_reply_is_unknown_and_is_never_automatically_replayed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let service = HTTPWorkspaceServiceEndpoint::new("fixture", &base, None).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (_, request) = read_http(&mut stream).await;
            assert_eq!(request["method"], "tekesWorkspace.gitCommit");
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = service
            .call(
                WorkspaceMethod::GitCommit,
                json!({"workspaceId":"w","message":"commit","paths":["proof"]}),
                "one-commit",
            )
            .await
            .unwrap_err();
        assert!(matches!(error,Error::Remote(value) if value["code"]=="outcome-unknown"));
        server.await.unwrap();
    }
}
