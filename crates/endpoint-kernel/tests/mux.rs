use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tekes_agent_contract::*;
use tekes_agent_endpoint_kernel::KernelConnector;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
fn host(version: i64) -> Value {
    json!({"type":"ready","generation":"g","host":{"protocolVersion":version,"product":{"name":"TekesKernel","version":"test"},"capabilities":["session-inventory-sync","session-control-sync","actionable-sync","session-journal"],"cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false}})
}
async fn send<S>(ws: &mut S, value: Value)
where
    S: futures_util::Sink<Message> + Unpin,
    S::Error: std::fmt::Debug,
{
    ws.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
#[tokio::test]
async fn rejects_old_kernel_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(socket).await.unwrap();
        send(&mut ws, host(3)).await;
    });
    assert!(
        KernelConnector::new("k", &url, "synthetic".into())
            .unwrap()
            .connect_kernel()
            .await
            .is_err()
    );
    task.await.unwrap();
}
#[tokio::test]
async fn mux_close_discards_inflight_frames_and_correlates_pages_and_actionables() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(socket).await.unwrap();
        send(&mut ws, host(4)).await;
        let open: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        let id = open["streamId"].clone();
        assert_eq!(open["target"]["kind"], "session-inventory");
        send(&mut ws,json!({"type":"stream","streamId":id,"frame":{"type":"baseline","generation":"g","snapshot":{"items":[]}}})).await;
        let close: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        assert_eq!(close["type"], "close");
        // Invalid payload belongs to a closed logical stream and must not kill this connection.
        send(
            &mut ws,
            json!({"type":"stream","streamId":id,"frame":{"bad":"late"}}),
        )
        .await;
        let page: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        assert_eq!(page["type"], "journal-page");
        assert_eq!(page["throughSequence"], 3);
        assert_eq!(page["beforeSequence"], 2);
        send(&mut ws,json!({"type":"journal-page-result","requestId":page["requestId"],"page":{"sessionId":"s","throughSequence":3,"entries":[],"hasMoreBefore":false}})).await;
        let response: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        assert_eq!(response["expectedRevision"], 2);
        send(&mut ws,json!({"type":"actionable-response-result","requestId":response["requestId"],"actionableId":"a","revision":2})).await;
        let _ = ws.next().await;
    });
    let endpoint = KernelConnector::new("k", &url, "synthetic".into())
        .unwrap()
        .connect_kernel()
        .await
        .unwrap();
    let mut stream = endpoint
        .follow(StreamTarget::SessionInventory)
        .await
        .unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap()["type"], "baseline");
    drop(stream);
    let page = endpoint
        .history_page(HistoryRequest {
            session_id: "s".into(),
            through_sequence: 3,
            before_sequence: Some(2),
            max_messages: 50,
        })
        .await
        .unwrap();
    assert!(page.entries.is_empty());
    endpoint
        .respond("a", 2, json!({"answer":"yes"}))
        .await
        .unwrap();
    endpoint.close().await;
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn unary_envelope_authentication_errors_and_no_mutation_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let ws_task = tokio::spawn(async move {
            let mut ws = accept_async(socket).await.unwrap();
            send(&mut ws, host(4)).await;
            let _ = ws.next().await;
        });
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let header_end = loop {
            let mut byte = [0];
            socket.read_exact(&mut byte).await.unwrap();
            bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") {
                break bytes.len();
            }
        };
        let headers = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            headers
                .to_lowercase()
                .contains("authorization: bearer synthetic")
        );
        assert!(headers.starts_with("POST /api/session.prompt "));
        let len: usize = headers
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap();
        bytes.resize(header_end + len, 0);
        socket.read_exact(&mut bytes[header_end..]).await.unwrap();
        let request: Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
        assert_eq!(request["type"], "client-request");
        assert_eq!(request["rpcId"], "stable-request");
        assert_eq!(request["payload"]["sessionId"], "s");
        let body=json!({"type":"server-response","rpcId":"stable-request","result":{"ok":false,"error":{"code":"busy","message":"synthetic","details":{}}}}).to_string();
        socket.write_all(format!("HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        ws_task.await.unwrap();
    });
    let endpoint = KernelConnector::new("k", &url, "synthetic".into())
        .unwrap()
        .with_timeout(Duration::from_secs(2))
        .connect_kernel()
        .await
        .unwrap();
    assert!(matches!(
        endpoint
            .call("session.prompt", json!({"sessionId":"s"}), "stable-request")
            .await,
        Err(Error::Remote(_))
    ));
    assert!(matches!(
        endpoint.call("workspace.create", json!({}), "x").await,
        Err(Error::Unsupported(_))
    ));
    endpoint.close().await;
    let closed = endpoint
        .call("session.prompt", json!({"sessionId":"s"}), "after-close")
        .await
        .unwrap_err();
    assert!(closed.to_string().contains("closed"));
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn physical_generation_mismatch_terminates_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(socket).await.unwrap();
        send(&mut ws, host(4)).await;
        let open: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        send(&mut ws,json!({"type":"stream","streamId":open["streamId"],"frame":{"type":"reset","generation":"wrong"}})).await;
    });
    let endpoint = KernelConnector::new("k", &url, "synthetic".into())
        .unwrap()
        .connect_kernel()
        .await
        .unwrap();
    let mut frames = endpoint.follow(StreamTarget::Actionables).await.unwrap();
    assert!(frames.next().await.unwrap().is_err());
    task.await.unwrap();
}

#[tokio::test]
#[allow(clippy::result_large_err)] // tungstenite fixes the callback error type.
async fn file_observation_is_authenticated_and_closes_with_endpoint() {
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut main = accept_async(socket).await.unwrap();
        send(&mut main, host(4)).await;
        let (socket, _) = listener.accept().await.unwrap();
        let mut files =
            tokio_tungstenite::accept_hdr_async(socket, |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/api/session.files.changes");
                assert_eq!(request.uri().query(), Some("sessionId=s"));
                assert_eq!(request.headers()["Authorization"], "Bearer synthetic");
                Ok(response)
            })
            .await
            .unwrap();
        send(&mut files, json!({"kind":"ready"})).await;
        send(
            &mut files,
            json!({"kind":"change","change":{"absolutePath":"/work/file","version":"v"}}),
        )
        .await;
        let _ = main.next().await;
    });
    let endpoint = KernelConnector::new("k", &url, "synthetic".into())
        .unwrap()
        .connect_kernel()
        .await
        .unwrap();
    let mut changes = endpoint.observe_files("s", ".").await.unwrap();
    assert_eq!(changes.next().await.unwrap().unwrap()["kind"], "ready");
    assert_eq!(
        changes.next().await.unwrap().unwrap()["change"]["version"],
        "v"
    );
    endpoint.close().await;
    assert!(
        tokio::time::timeout(Duration::from_secs(1), changes.next())
            .await
            .unwrap()
            .is_none()
    );
    task.await.unwrap();
    assert!(
        endpoint
            .call("session.prompt", json!({}), "after-close")
            .await
            .is_err()
    );
}
