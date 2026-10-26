use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use tekes_agent_contract::*;
use tekes_agent_endpoint_kernel::KernelConnector;
use tekes_agent_session::MemoryProjectionStore;
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
async fn remote_stream_error_does_not_close_mux_and_streams_close_once() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let ws_task = tokio::spawn(async move {
            let mut ws = accept_async(socket).await.unwrap();
            send(&mut ws, host(4)).await;

            let mut ids = std::collections::HashMap::new();
            for _ in 0..4 {
                let open: Value =
                    serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
                assert_eq!(open["type"], "open");
                ids.insert(
                    open["target"]["kind"].as_str().unwrap().to_owned(),
                    open["streamId"].clone(),
                );
            }
            let abandoned_id = ids["actionables"].clone();
            let bad_id = ids["session-journal"].clone();
            let healthy_id = ids["session-inventory"].clone();
            let control_id = ids["session-control"].clone();

            // A consumer dropped before a frame still needs exactly one peer Close.
            send(
                &mut ws,
                json!({"type":"stream","streamId":abandoned_id,"frame":{"generation":"g"}}),
            )
            .await;
            let close: Value =
                serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
            assert_eq!(close, json!({"type":"close","streamId":abandoned_id}));

            // This is Kernel's generated V4 per-stream error envelope. The logical stream is
            // already retired at the peer, so a later local Drop must not send Close for it.
            send(&mut ws, json!({
                "type":"error",
                "streamId":bad_id,
                "error":{"category":"transport","code":"protocol-error","message":"journal unavailable","details":{}}
            })).await;
            send(&mut ws, json!({"type":"stream","streamId":healthy_id,"frame":{"type":"baseline","generation":"g","snapshot":{"items":[]}}})).await;
            send(&mut ws, json!({"type":"stream","streamId":control_id,"frame":{"type":"baseline","generation":"g","snapshot":{"items":[]}}})).await;

            // The next Close must belong to the healthy stream explicitly dropped by the
            // consumer. A stale Close for `bad_id` would arrive first and fail this assertion.
            let close: Value =
                serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
            assert_eq!(close, json!({"type":"close","streamId":healthy_id}));

            // The control stream continues on this same physical WebSocket after the error,
            // and after the HTTP unary request is served on the same listener.
            send(&mut ws, json!({"type":"stream","streamId":control_id,"frame":{"type":"delta","generation":"g","sequence":1}})).await;
            let frame = ws.next().await.unwrap().unwrap();
            assert!(
                matches!(frame, Message::Close(_)),
                "unexpected logical frame after stream retirement"
            );
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
        assert!(headers.starts_with("POST /api/models.list "));
        let len: usize = headers
            .lines()
            .find_map(|line| {
                line.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().parse().unwrap())
            })
            .unwrap();
        bytes.resize(header_end + len, 0);
        socket.read_exact(&mut bytes[header_end..]).await.unwrap();
        let request: Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
        assert_eq!(request["type"], "client-request");
        assert_eq!(request["rpcId"], "same-transport");
        let body = json!({
            "type":"server-response",
            "rpcId":"same-transport",
            "result":{"ok":true,"value":{"sessions":[]}}
        })
        .to_string();
        socket
            .write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            ).as_bytes())
            .await
            .unwrap();
        ws_task.await.unwrap();
    });

    let endpoint = KernelConnector::new("k", &url, "synthetic".into())
        .unwrap()
        .with_timeout(Duration::from_secs(3))
        .connect_kernel()
        .await
        .unwrap();
    let abandoned = endpoint.follow(StreamTarget::Actionables).await.unwrap();
    let mut bad = endpoint
        .follow(StreamTarget::SessionJournal {
            address: JournalAddress {
                session_id: "s".into(),
            },
            max_messages: 10,
        })
        .await
        .unwrap();
    let mut healthy = endpoint
        .follow(StreamTarget::SessionInventory)
        .await
        .unwrap();
    let mut control = endpoint.follow(StreamTarget::SessionControl).await.unwrap();
    drop(abandoned);

    let error = tokio::time::timeout(Duration::from_secs(3), bad.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, Error::Remote(_)));
    assert!(error.to_string().contains("journal unavailable"));
    drop(bad);
    assert_eq!(healthy.next().await.unwrap().unwrap()["type"], "baseline");
    assert_eq!(control.next().await.unwrap().unwrap()["type"], "baseline");

    assert_eq!(
        endpoint
            .call("models.list", json!({}), "same-transport")
            .await
            .unwrap(),
        json!({"sessions":[]})
    );
    drop(healthy);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), control.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()["type"],
        "delta"
    );
    endpoint.close().await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn native_inventory_wire_preserves_fork_and_ephemeral_facts() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(socket).await.unwrap();
        send(
            &mut ws,
            json!({
                "type":"ready",
                "generation":"g",
                "host": {
                    "protocolVersion":4,
                    "product":{"name":"TekesKernel","version":"0.9.3"},
                    "capabilities":["session-inventory-sync","session-control-sync","actionable-sync","session-journal"],
                    "cwd":"/work","home":"/home","attachedSessions":0,"canOpenPath":false
                }
            }),
        )
        .await;

        let open: Value =
            serde_json::from_slice(&ws.next().await.unwrap().unwrap().into_data()).unwrap();
        assert_eq!(open["type"], "open");
        assert_eq!(open["target"]["kind"], "session-inventory");

        send(
            &mut ws,
            json!({
                "type":"stream",
                "streamId":open["streamId"],
                "frame": {
                    "type":"baseline",
                    "generation":"g",
                    "snapshot":{"items":[
                        {
                            "sessionId":"fork-ephemeral",
                            "updatedAt":1,"running":false,"blank":true,
                            "parentSessionId":"parent",
                            "origin":"fork",
                            "ephemeral":true
                        },
                        {
                            "sessionId":"fork-durable",
                            "updatedAt":2,"running":false,"blank":false,
                            "parentSessionId":"parent",
                            "origin":"fork"
                        },
                        {
                            "sessionId":"ordinary",
                            "updatedAt":3,"running":false,"blank":true
                        }
                    ]}
                }
            }),
        )
        .await;
        let _ = ws.next().await;
    });

    let endpoint = KernelConnector::new("native", &url, "synthetic".into())
        .unwrap()
        .connect_kernel()
        .await
        .unwrap();
    let mut inventory = endpoint
        .follow(StreamTarget::SessionInventory)
        .await
        .unwrap();
    let baseline = tokio::time::timeout(Duration::from_secs(3), inventory.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let mut store = MemoryProjectionStore::new(endpoint.identity());
    store.begin_connection(endpoint.host().clone());
    store.apply("inventory", baseline).unwrap();

    let ephemeral_fork = store
        .session(&SessionAddress {
            endpoint_id: "native".into(),
            session_id: "fork-ephemeral".into(),
        })
        .unwrap()
        .summary
        .as_ref()
        .unwrap();
    assert_eq!(ephemeral_fork["parentSessionId"], "parent");
    assert_eq!(ephemeral_fork["origin"], "fork");
    assert_eq!(ephemeral_fork["ephemeral"], true);
    assert_ne!(ephemeral_fork["origin"], "subagent");

    let durable_fork = store
        .session(&SessionAddress {
            endpoint_id: "native".into(),
            session_id: "fork-durable".into(),
        })
        .unwrap()
        .summary
        .as_ref()
        .unwrap();
    assert_eq!(durable_fork["parentSessionId"], "parent");
    assert_eq!(durable_fork["origin"], "fork");
    assert!(!durable_fork.as_object().unwrap().contains_key("ephemeral"));
    assert!(!durable_fork["ephemeral"].as_bool().unwrap_or(false));

    let ordinary = store
        .session(&SessionAddress {
            endpoint_id: "native".into(),
            session_id: "ordinary".into(),
        })
        .unwrap()
        .summary
        .as_ref()
        .unwrap();
    assert!(
        !ordinary
            .as_object()
            .unwrap()
            .contains_key("parentSessionId")
    );
    assert!(!ordinary.as_object().unwrap().contains_key("origin"));
    assert!(!ordinary.as_object().unwrap().contains_key("ephemeral"));

    drop(inventory);
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
