//! Private JSON-lines host protocol. The authenticated HTTP/NDJSON bridge lives
//! in the embedding host; these values describe only its child-process boundary.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
};
use tokio::sync::mpsc;

/// A parsed request keeps the original JSON intact for SharedAgent dispatch.
/// `id` identifies one transport operation; mutations use their separate
/// caller-owned `rpcId` in `value`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostRequest<'a> {
    pub id: &'a str,
    pub operation: &'a str,
    pub value: &'a Value,
}

impl<'a> HostRequest<'a> {
    /// Invalid requests are rejected at this line boundary and do not stop the
    /// multiplexed service. The caller cannot safely answer without a valid id.
    pub fn parse(value: &'a Value) -> Option<Self> {
        let object = value.as_object()?;
        let id = object.get("id")?.as_str()?;
        let operation = object.get("op")?.as_str()?;
        if id.is_empty() || operation.is_empty() {
            return None;
        }
        Some(Self {
            id,
            operation,
            value,
        })
    }

    pub fn cancels_observation(self) -> bool {
        self.operation == "cancelObservation"
    }

    pub fn follows(self) -> bool {
        self.operation == "follow"
    }

    /// DSH credential refresh is an embedding-host operation, never a browser API.
    pub fn configures_dsh(self) -> bool {
        self.operation == "configureDSH"
    }
}

/// One line in the existing host response stream. Errors deliberately retain
/// their legacy string representation for compatibility with the current bridge.
#[derive(Clone, Debug, PartialEq)]
pub enum HostResponse {
    Value { id: String, value: Value },
    Error { id: String, message: String },
    Done { id: String },
}

impl HostResponse {
    pub fn value(id: impl Into<String>, value: Value) -> Self {
        Self::Value {
            id: id.into(),
            value,
        }
    }

    pub fn error(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            id: id.into(),
            message: message.into(),
        }
    }

    pub fn done(id: impl Into<String>) -> Self {
        Self::Done { id: id.into() }
    }

    pub fn into_json(self) -> Value {
        match self {
            Self::Value { id, value } => json!({ "id": id, "value": value }),
            Self::Error { id, message } => json!({ "id": id, "error": message }),
            Self::Done { id } => json!({ "id": id, "done": true }),
        }
    }
}

/// Cancellation targets registered observation ids only. A matching unary
/// request has no observation entry and cannot be aborted by this operation.
pub fn abort_observation(
    id: &str,
    tasks: &mut BTreeMap<String, tokio::task::AbortHandle>,
    observations: &mut BTreeSet<String>,
) -> bool {
    if !observations.remove(id) {
        return false;
    }
    tasks.remove(id).is_some_and(|task| {
        task.abort();
        true
    })
}

/// Drain frames that a finite follow operation queued before returning. The
/// biased receive branch ensures a ready terminal result cannot overtake frames
/// already accepted by its bounded channel.
pub async fn forward_follow<F>(
    mut receiver: mpsc::Receiver<Value>,
    follow: F,
    id: String,
    output: mpsc::Sender<Value>,
) -> tekes_agent_contract::Result<()>
where
    F: Future<Output = tekes_agent_contract::Result<()>>,
{
    tokio::pin!(follow);
    loop {
        tokio::select! {
            biased;
            value = receiver.recv() => match value {
                Some(value) => {
                    if output.send(HostResponse::value(id.clone(), value).into_json()).await.is_err() {
                        return Ok(());
                    }
                }
                None => return follow.await,
            },
            result = &mut follow => {
                while let Ok(value) = receiver.try_recv() {
                    if output.send(HostResponse::value(id.clone(), value).into_json()).await.is_err() {
                        return Ok(());
                    }
                }
                return result;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_transport_and_mutation_ids_and_legacy_wire_envelopes() {
        let request = json!({
            "id": "transport-9",
            "rpcId": "mutation-4",
            "op": "call",
            "method": "session.create",
        });
        let parsed = HostRequest::parse(&request).unwrap();
        assert_eq!(parsed.id, "transport-9");
        assert_eq!(parsed.operation, "call");
        assert_eq!(parsed.value["rpcId"], "mutation-4");

        assert_eq!(
            HostResponse::value("transport-9", json!({"accepted": true})).into_json(),
            json!({"id":"transport-9","value":{"accepted":true}})
        );
        assert_eq!(
            HostResponse::error("transport-9", "rejected").into_json(),
            json!({"id":"transport-9","error":"rejected"})
        );
        assert_eq!(
            HostResponse::done("transport-9").into_json(),
            json!({"id":"transport-9","done":true})
        );
    }

    #[test]
    fn malformed_or_missing_request_identity_is_contained_at_the_line_boundary() {
        for request in [
            json!({"id":"", "op":"snapshot"}),
            json!({"id":"transport", "op":""}),
            json!({"id":"transport"}),
            json!({"op":"snapshot"}),
            json!(null),
        ] {
            assert!(HostRequest::parse(&request).is_none());
        }
    }

    #[test]
    fn host_only_operation_and_observation_cancellation_remain_explicit() {
        let dsh = json!({"id":"host", "op":"configureDSH"});
        let cancel = json!({"id":"observer-1", "op":"cancelObservation"});
        let mutation = json!({"id":"mutation-transport", "rpcId":"rpc-1", "op":"call"});
        assert!(HostRequest::parse(&dsh).unwrap().configures_dsh());
        let cancellation = HostRequest::parse(&cancel).unwrap();
        assert!(cancellation.cancels_observation());
        assert!(!cancellation.follows());
        let mutation = HostRequest::parse(&mutation).unwrap();
        assert!(!mutation.cancels_observation());
        assert_eq!(mutation.value["rpcId"], "rpc-1");
    }

    #[tokio::test]
    async fn finite_follow_delivers_accepted_frames_in_order_before_terminal_error() {
        let (frame_sender, frame_receiver) = mpsc::channel(4);
        let (output_sender, mut output_receiver) = mpsc::channel(8);
        let follow = async move {
            frame_sender.send(json!({"seq": 1})).await.unwrap();
            frame_sender.send(json!({"seq": 2})).await.unwrap();
            Err(tekes_agent_contract::Error::Protocol(
                "fixture terminal".into(),
            ))
        };

        let result = forward_follow(
            frame_receiver,
            follow,
            "watch-7".into(),
            output_sender.clone(),
        )
        .await;
        assert!(result.is_err());
        let terminal_message = result.unwrap_err().to_string();
        output_sender
            .send(HostResponse::error("watch-7", terminal_message).into_json())
            .await
            .unwrap();
        output_sender
            .send(HostResponse::done("watch-7").into_json())
            .await
            .unwrap();

        assert_eq!(
            output_receiver.recv().await.unwrap(),
            json!({"id":"watch-7","value":{"seq":1}})
        );
        assert_eq!(
            output_receiver.recv().await.unwrap(),
            json!({"id":"watch-7","value":{"seq":2}})
        );
        assert_eq!(
            output_receiver.recv().await.unwrap(),
            json!({"id":"watch-7","error":"protocol: fixture terminal"})
        );
        assert_eq!(
            output_receiver.recv().await.unwrap(),
            json!({"id":"watch-7","done":true})
        );
    }

    #[tokio::test]
    async fn cancelling_a_transport_id_only_aborts_registered_observations() {
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel::<()>();
        let unary = tokio::spawn(async move {
            let _ = finished_rx.await;
            42
        });
        let (observation_finished_tx, observation_finished_rx) =
            tokio::sync::oneshot::channel::<()>();
        let observation = tokio::spawn(async move {
            let _ = observation_finished_rx.await;
            43
        });
        let mut tasks = BTreeMap::from([
            ("mutation-1".to_owned(), unary.abort_handle()),
            ("watch-1".to_owned(), observation.abort_handle()),
        ]);
        let mut observations = BTreeSet::from(["watch-1".to_owned()]);

        assert!(!abort_observation(
            "mutation-1",
            &mut tasks,
            &mut observations
        ));
        assert!(abort_observation("watch-1", &mut tasks, &mut observations));
        finished_tx.send(()).unwrap();
        assert_eq!(unary.await.unwrap(), 42);
        assert!(observation.await.unwrap_err().is_cancelled());
        let _ = observation_finished_tx.send(());
    }
}
