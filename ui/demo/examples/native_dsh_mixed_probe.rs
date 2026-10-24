//! Headless Native/DSH public-API proof for ordered image/file/image submissions.
//!
//! Usage:
//! cargo run -p tekes-agent-ui-demo --example native_dsh_mixed_probe -- \
//!   --endpoint-config /absolute/endpoint-config.json \
//!   --receipt /absolute/new/receipt.json
//!
//! The endpoint config is the existing single Native or DSH configuration with two
//! distinct sessions. Credentials remain in the process environment and are never
//! copied to the receipt. The receipt contains captured image and file bytes.

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tekes_agent_contract::{Error, PromptMode, Result};
use tekes_agent_presentation::{
    ComposerDestination, RunScope, SubmissionPlan, SubmissionPreparation, TranscriptItem,
    TranscriptItemKind,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    AttachmentFile, AttachmentOutcome, InlineImageInput, InlineImageOutcome, PaneSnapshot,
    RunBinding,
};
use tekes_agent_ui_demo::endpoint_config::EndpointConfiguration;

const EXPECTED_REPLY: &str = "ENDPOINT_REPLY_OK";
const BUSY_HOLD_TEXT: &str = "NATIVE_DSH_BUSY_HOLD";
const FILE_NAME: &str = "中文混合.txt";
const FILE_BYTES: &[u8] = b"AgentKit Pi mixed file proof\n";
const WAIT: Duration = Duration::from_secs(180);

fn require(condition: bool, message: &'static str) -> Result<()> {
    if !condition {
        eprintln!("NATIVE_DSH_MIXED_CHECK_FAILED {message}");
    }
    tekes_agent_contract::require(condition, message)
}

struct NoPreparation;
static NO_PREPARATION: NoPreparation = NoPreparation;

#[async_trait]
impl SubmissionPreparation for NoPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        ticket: &tekes_agent_presentation::PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        require(
            ticket.settings().is_empty(),
            "probe requires empty settings",
        )
    }
}

struct Target {
    runtime: Arc<SessionRuntime>,
    kind: &'static str,
    scope: RunScope,
    binding: Arc<RunBinding>,
}

struct Scenario {
    authored_text: &'static str,
    first_name: &'static str,
    first_rgb: [u8; 3],
    last_name: &'static str,
    last_rgb: [u8; 3],
}

fn parse_args() -> Result<(PathBuf, PathBuf)> {
    let mut args = env::args().skip(1);
    let mut config = None;
    let mut receipt = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--endpoint-config" if config.is_none() => {
                config = Some(PathBuf::from(
                    args.next()
                        .ok_or(Error::Protocol("missing endpoint config".into()))?,
                ));
            }
            "--receipt" if receipt.is_none() => {
                receipt = Some(PathBuf::from(
                    args.next()
                        .ok_or(Error::Protocol("missing receipt path".into()))?,
                ));
            }
            _ => {
                return Err(Error::Protocol(
                    "expected one --endpoint-config and one --receipt".into(),
                ));
            }
        }
    }
    let config = config.ok_or(Error::Protocol("missing --endpoint-config".into()))?;
    require(
        config.is_absolute(),
        "endpoint config path must be absolute",
    )?;
    let receipt = receipt.ok_or(Error::Protocol("missing --receipt".into()))?;
    require(
        receipt.is_absolute() && !receipt.exists(),
        "receipt must be an absolute new path",
    )?;
    Ok((config, receipt))
}

fn encode_png(rgb: [u8; 3]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| Error::Protocol("PNG fixture encoding failed".into()))?;
        writer
            .write_image_data(&rgb.repeat(4))
            .map_err(|_| Error::Protocol("PNG fixture pixel encoding failed".into()))?;
    }
    Ok(bytes)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn decoded_base64_len(encoded: &str) -> usize {
    let padding = encoded
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    encoded.len() / 4 * 3 - padding
}

fn public_item(item: &TranscriptItem) -> Value {
    json!(item)
}

async fn wait_committed_user(target: &Target, rpc_id: &str) -> Result<TranscriptItem> {
    let started = Instant::now();
    let mut diagnostic_count = 0;
    loop {
        let snapshot = fresh_run(target).await?;
        if diagnostic_count < 3 && started.elapsed().as_secs() >= diagnostic_count * 5 {
            diagnostic_count += 1;
            let public_state = target.runtime.inspect(|state| state.sessions.get(&target.scope.session_id).map(|session| json!({
                "runOpen":session.run_open,"summary":session.summary,
                "entryCount":session.entries.len(),"nativeConversation":session.native_conversation.is_some(),
                "eventKinds":session.entries.values().map(|entry| entry.event.kind.clone()).collect::<Vec<_>>(),
            })));
            eprintln!(
                "NATIVE_DSH_BUSY_PUBLIC_HOLD_STATE {}",
                json!({"rpcId":rpc_id,"run":snapshot.run,"items":snapshot.transcript.items,"state":public_state})
            );
        }
        if let Some(input) = snapshot.transcript.items.iter().find(|item| {
            item.kind == TranscriptItemKind::User
                && item.reconciliation_id.as_deref() == Some(rpc_id)
                && !item.is_provisional
                && (item.source_position.is_some() || item.source_sequence.is_some())
        }) {
            require(
                !input.id.is_empty() && !input.turn_id.is_empty() && input.text == BUSY_HOLD_TEXT,
                "busy hold input identity or content differs",
            )?;
            let running = target.runtime.inspect(|state| {
                state
                    .sessions
                    .get(&target.scope.session_id)
                    .is_some_and(|session| session.run_open)
            });
            if running {
                return Ok(input.clone());
            }
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn queue_source_rpc<'a>(kind: &str, fact: &'a Value) -> Option<&'a str> {
    let message = if kind == "native" {
        fact.get("message")?
    } else {
        fact
    };
    message.get("source")?.get("rpcId")?.as_str()
}

struct BusyQueueContext<'a> {
    rpc_id: &'a str,
    authored_text: &'a str,
    prepared: &'a Value,
    upload: &'a Value,
    images: &'a [Value],
    image_bytes: [&'a [u8]; 2],
    hold_rpc_id: &'a str,
    hold_input: &'a TranscriptItem,
}

fn matches_public_queue_content(
    kind: &str,
    message: &Value,
    context: &BusyQueueContext<'_>,
) -> Result<()> {
    let content = message["content"]
        .as_array()
        .ok_or_else(|| Error::Protocol("public queued message content missing".into()))?;
    require(
        content.len() == 4 && content[0] == json!({"type":"text","text":context.authored_text}),
        "public queued authored text or block count differs",
    )?;
    let first_id = image_attachment_id(kind, &content[1], &context.images[0])?;
    let last_id = image_attachment_id(kind, &content[3], &context.images[1])?;
    require(
        first_id != last_id
            && first_id == format!("sha256:{:x}", Sha256::digest(context.image_bytes[0]))
            && last_id == format!("sha256:{:x}", Sha256::digest(context.image_bytes[1])),
        "public queued images differ from original captured PNG bytes",
    )?;
    let mut expected_file = json!({"type":"file","attachment":context.upload["file"]});
    if kind == "native" {
        expected_file["mediaType"] = json!("text/plain");
    }
    require(
        content[2] == expected_file,
        "public queued file differs from original upload",
    )?;
    // The independent runner repeats this check before releasing its provider hold.
    Ok(())
}

async fn wait_busy_queue(target: &Target, context: &BusyQueueContext<'_>) -> Result<Value> {
    let rpc_id = context.rpc_id;
    let prepared = context.prepared;
    let upload = context.upload;
    let images = context.images;
    let hold_rpc_id = context.hold_rpc_id;
    let started = Instant::now();
    let mut diagnostic_count = 0;
    loop {
        let snapshot = fresh_run(target).await?;
        if diagnostic_count < 3 && started.elapsed().as_secs() >= diagnostic_count * 5 {
            diagnostic_count += 1;
            eprintln!(
                "NATIVE_DSH_BUSY_PUBLIC_QUEUE_STATE {}",
                json!({"scope":target.scope,"epoch":snapshot.connection_epoch,"revision":snapshot.revision,"run":snapshot.run,"submissions":snapshot.submissions})
            );
        }
        let observed = target.runtime.inspect(|state| {
            let session = state.sessions.get(&target.scope.session_id)?;
            let native_run = snapshot.run.as_ref()?;
            let epoch = state.connection_epoch;
            match target.kind {
                "native" => native_run.queue
                    .iter()
                    .find(|item| queue_source_rpc("native", item) == Some(rpc_id))
                    .map(|item| {
                        let item = item.clone();
                        (epoch, snapshot.revision, None, item.clone(), json!({"kind":"control","items":native_run.queue}), item["message"].clone())
                    }),
                "dsh" => session.projections.get("inbox").and_then(|projection| {
                    projection.value["next-turn"].as_array()?.iter().find(|message| {
                        queue_source_rpc("dsh", message) == Some(rpc_id)
                    }).map(|message| {
                        let value = projection.value.clone();
                        let message = message.clone();
                        (epoch, snapshot.revision, Some(projection.sequence), message.clone(), json!({"kind":"inbox","projection":{"sequence":projection.sequence,"value":value}}), message)
                    })
                }),
                _ => None,
            }
        });
        if let Some((
            epoch,
            snapshot_revision,
            _projection_sequence,
            queue_fact,
            queue_witness,
            message,
        )) = observed
        {
            require(
                snapshot
                    .run
                    .as_ref()
                    .is_some_and(|run| run.scope == target.scope),
                "queue witness scope differs",
            )?;
            let common_queue = &snapshot.run.as_ref().ok_or(Error::Disconnected)?.queue;
            let expected_common = if target.kind == "native" {
                queue_fact.clone()
            } else {
                json!({"id":queue_fact["id"],"placement":"queued","target":"next-turn","message":queue_fact})
            };
            let submission = snapshot
                .submissions
                .iter()
                .find(|submission| submission.id == rpc_id);
            if !common_queue.contains(&expected_common)
                || !submission.is_some_and(|submission| submission.reconciled)
                || snapshot.transcript.items.iter().any(|item| {
                    item.is_provisional && item.reconciliation_id.as_deref() == Some(rpc_id)
                })
            {
                if started.elapsed() >= WAIT {
                    return Err(Error::Protocol(
                        "common queue did not reconcile the original ticket".into(),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            require(
                queue_source_rpc(target.kind, &queue_fact) == Some(rpc_id)
                    && message["role"] == "user"
                    && message["source"]["kind"] == "user"
                    && (target.kind != "native" || queue_fact["placement"] == "queued")
                    && snapshot.connection_epoch == epoch,
                "public queue fact RPC, role, placement, or epoch differs",
            )?;
            matches_public_queue_content(target.kind, &message, context)?;
            return Ok(json!({
                "status":"queued",
                "endpointKind":target.kind,
                "scope":target.scope,
                "rpcId":rpc_id,
                "ticketId":rpc_id,
                "snapshotRevision":snapshot_revision,
                "connectionEpoch":epoch,
                "queueFact":queue_fact,
                "queueWitness":queue_witness,
                "commonQueue":common_queue,
                "submission":submission,
                "prepared":prepared,
                "uploadReceipt":upload,
                "imageInputs":images,
                "holdRpcId":hold_rpc_id,
                "holdInput":public_item(context.hold_input),
            }));
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn baseline(binding: &RunBinding) -> Vec<String> {
    binding.current_run().map_or_else(Vec::new, |run| {
        run.transcript
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect()
    })
}

async fn fresh_run(target: &Target) -> Result<Arc<tekes_agent_presentation::RunSnapshot>> {
    let started = Instant::now();
    loop {
        if let PaneSnapshot::Run { snapshot, .. } = target.binding.snapshot().await?
            && !snapshot.transcript.is_stale
            && snapshot
                .run
                .as_ref()
                .is_some_and(|run| run.scope == target.scope)
        {
            return Ok(snapshot);
        }
        if started.elapsed() >= Duration::from_secs(30) {
            return Err(Error::Protocol("public Run snapshot unavailable".into()));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_round_trip(
    target: &Target,
    rpc_id: &str,
    excluded_ids: &[String],
) -> Result<(TranscriptItem, TranscriptItem)> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_run(target).await?;
        let items = &snapshot.transcript.items;
        for user in items.iter().filter(|item| {
            item.kind == TranscriptItemKind::User
                && item.reconciliation_id.as_deref() == Some(rpc_id)
                && !item.is_provisional
                && !excluded_ids.contains(&item.id)
                && (item.source_position.is_some() || item.source_sequence.is_some())
        }) {
            require(
                !user.id.is_empty() && !user.turn_id.is_empty(),
                "input source identity missing",
            )?;
            let assistant = items.iter().find(|item| {
                item.kind == TranscriptItemKind::Assistant
                    && item.turn_id == user.turn_id
                    && item.is_final
                    && !item.is_provisional
                    && (item.source_position.is_some() || item.source_sequence.is_some())
            });
            if let Some(assistant) = assistant {
                require(
                    assistant.text == EXPECTED_REPLY && !assistant.id.is_empty(),
                    "committed assistant differs from fixed endpoint reply",
                )?;
                return Ok((user.clone(), assistant.clone()));
            }
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn find_by_id(target: &Target, id: &str) -> Result<TranscriptItem> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_run(target).await?;
        if let Some(item) = snapshot.transcript.items.iter().find(|item| item.id == id) {
            return Ok(item.clone());
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Protocol(
                "reconnected source identity missing".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn upload_file(target: &Target) -> Result<Value> {
    let destination = ComposerDestination::run(target.scope.clone());
    let request = target.binding.begin_attachments(&destination)?;
    let file = AttachmentFile::new(
        FILE_NAME.to_owned(),
        "text/plain".into(),
        Arc::from(FILE_BYTES),
    )?;
    let report = request.upload(&[file]).await?;
    require(
        report.outcome == AttachmentOutcome::Attached && report.receipts.len() == 1,
        "public file upload did not attach exactly one receipt",
    )?;
    let receipt = &report.receipts[0];
    require(
        receipt.file.name == FILE_NAME && receipt.file.bytes == FILE_BYTES.len() as u64,
        "public file upload metadata differs",
    )?;
    Ok(json!({
        "receiptId": receipt.receipt_id,
        "file": {
            "attachmentId": receipt.file.attachment_id,
            "name": receipt.file.name,
            "bytes": receipt.file.bytes,
        }
    }))
}

async fn attach_image(target: &Target, name: &str, bytes: &[u8]) -> Result<String> {
    let destination = ComposerDestination::run(target.scope.clone());
    let selection = target.binding.begin_inline_images(&destination)?;
    let encoded = base64(bytes);
    require(
        selection
            .attach(&[InlineImageInput::new(name.to_owned(), Arc::from(bytes))?])
            .await?
            == InlineImageOutcome::Attached,
        "captured image was detached",
    )?;
    Ok(encoded)
}

fn canonical_image(target: &Target, expected_data: &str, expected_name: &str) -> Result<Value> {
    let snapshot = target.binding.current_run().ok_or(Error::Disconnected)?;
    snapshot
        .draft
        .artifacts
        .iter()
        .find_map(|artifact| match artifact {
            tekes_agent_presentation::ComposerArtifact::Attachment { content, .. }
                if content["type"] == "image"
                    && content["name"] == expected_name
                    && content["data"] == expected_data =>
            {
                Some(content.clone())
            }
            _ => None,
        })
        .ok_or_else(|| Error::Protocol("captured image missing from canonical draft".into()))
}

fn prepared_receipt(
    ticket: &tekes_agent_presentation::PreparedSubmission,
    authored_text: &str,
    first: &Value,
    upload: &Value,
    last: &Value,
) -> Result<Value> {
    let SubmissionPlan::Prompt {
        content,
        display_text,
    } = ticket.plan()
    else {
        return Err(Error::Protocol(
            "prepared plan is not a canonical prompt".into(),
        ));
    };
    require(
        display_text == authored_text,
        "prepared display text differs",
    )?;
    let offset = usize::from(!authored_text.is_empty());
    require(
        content.len() == offset + 3,
        "prepared canonical content count differs",
    )?;
    if offset == 1 {
        require(
            content[0]["type"] == "text" && content[0]["text"] == authored_text,
            "prepared authored text differs",
        )?;
    }
    require(
        content[offset] == *first
            && content[offset + 1]["type"] == "file"
            && content[offset + 1]["receiptId"] == upload["receiptId"]
            && content[offset + 2] == *last,
        "prepared canonical content or attachment order differs",
    )?;
    let facts = ticket.attachment_facts();
    require(facts.len() == 3, "prepared attachment facts count differs")?;
    require(
        facts[0].facts().is_some_and(|facts| {
            facts.byte_count
                == decoded_base64_len(first["data"].as_str().unwrap_or_default()) as u64
                && facts.media_type.as_deref() == Some("image/png")
        }) && facts[1].facts().is_some_and(|facts| {
            facts.byte_count == FILE_BYTES.len() as u64
                && facts.attachment_id.as_deref() == upload["file"]["attachmentId"].as_str()
        }) && facts[2].facts().is_some_and(|facts| {
            facts.byte_count == decoded_base64_len(last["data"].as_str().unwrap_or_default()) as u64
                && facts.media_type.as_deref() == Some("image/png")
        }),
        "prepared attachment facts differ from captured upload",
    )?;
    Ok(json!({"content": content, "displayText": display_text}))
}

fn raw_attachments(item: &TranscriptItem) -> &[Value] {
    item.attachments.as_slice()
}

fn image_attachment_id(kind: &str, raw: &Value, expected: &Value) -> Result<String> {
    require(raw["type"] == "image", "raw image position differs")?;
    match kind {
        "native" => {
            require(
                raw["mime"] == "image/png" && raw["name"] == expected["name"],
                "Native public image history metadata differs",
            )?;
            let asset = raw["asset"]
                .as_str()
                .ok_or_else(|| Error::Protocol("Native public image asset missing".into()))?;
            let digest = asset
                .strip_prefix("sha256-")
                .ok_or_else(|| Error::Protocol("Native public image asset ID malformed".into()))?;
            require(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
                "Native public image digest malformed",
            )?;
            Ok(format!("sha256:{digest}"))
        }
        "dsh" => {
            let descriptor = &raw["attachment"];
            require(
                descriptor["mediaType"] == "image/png"
                    && descriptor["name"] == expected["name"]
                    && descriptor["bytes"] == expected["byteCount"]
                    && descriptor["width"] == 2
                    && descriptor["height"] == 2,
                "DSH public image descriptor differs",
            )?;
            descriptor["attachmentId"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::Protocol("DSH public image ID missing".into()))
        }
        _ => Err(Error::Unsupported(
            "endpoint kind is not Native or DSH".into(),
        )),
    }
}

async fn verify_images(
    target: &Target,
    item: &TranscriptItem,
    images: &[Value],
    upload: &Value,
) -> Result<Vec<Value>> {
    let attachments = raw_attachments(item);
    require(
        attachments.len() == 3,
        "raw transcript must retain image/file/image",
    )?;
    let (file_index, first_index, last_index) = match target.kind {
        "native" | "dsh" => (1, 0, 2),
        _ => {
            return Err(Error::Unsupported(
                "endpoint kind is not Native or DSH".into(),
            ));
        }
    };
    let file = &attachments[file_index];
    require(
        file["type"] == "file",
        "raw transcript file position differs",
    )?;
    match target.kind {
        "native" => {
            require(
                file["mediaType"] == "text/plain"
                    && file["attachment"]["name"] == FILE_NAME
                    && file["attachment"]["bytes"] == FILE_BYTES.len()
                    && file["attachment"]["attachmentId"] == upload["file"]["attachmentId"],
                "Native public projected file metadata differs from upload receipt",
            )?;
        }
        "dsh" => {
            let descriptor = &file["attachment"];
            require(
                descriptor["name"] == FILE_NAME
                    && descriptor["bytes"] == FILE_BYTES.len()
                    && descriptor["attachmentId"] == upload["file"]["attachmentId"],
                "DSH public file reference metadata differs",
            )?;
        }
        _ => unreachable!(),
    }
    let mut reads = Vec::new();
    for (index, expected) in [(first_index, &images[0]), (last_index, &images[1])] {
        let raw = &attachments[index];
        let id = image_attachment_id(target.kind, raw, expected)?;
        let read = target
            .runtime
            .call(
                "session.attachment",
                json!({"sessionId":target.scope.session_id,"attachmentId":id}),
                &format!(
                    "native-dsh-mixed-image-read-{}-{index}",
                    target.scope.session_id
                ),
            )
            .await?;
        require(
            read["attachment"]["attachmentId"] == id
                && read["attachment"]["mediaType"] == "image/png"
                && read["attachment"]["bytes"] == expected["byteCount"]
                && read["attachment"]["width"] == 2
                && read["attachment"]["height"] == 2
                && read["data"] == expected["data"]
                && (target.kind == "native" || read["attachment"]["name"] == expected["name"]),
            "public image read differs from captured image bytes or metadata",
        )?;
        reads.push(json!({"rawTranscriptAttachment":raw,"attachmentId":id,"read":read}));
    }
    Ok(reads)
}

async fn file_read(target: &Target, upload: &Value) -> Result<Value> {
    let supported = target.runtime.inspect(|state| {
        state
            .endpoint_capabilities
            .methods
            .contains("session.fileAttachment")
    });
    if target.kind == "native" {
        require(
            supported,
            "Native session.fileAttachment capability unavailable",
        )?;
    } else {
        require(
            !supported,
            "DSH unexpectedly exposes session.fileAttachment",
        )?;
    }
    if !supported {
        return Ok(json!({"supported":false,"read":null}));
    }
    let attachment_id = upload["file"]["attachmentId"]
        .as_str()
        .ok_or_else(|| Error::Protocol("upload receipt attachment ID missing".into()))?;
    let read = target
        .runtime
        .call(
            "session.fileAttachment",
            json!({"sessionId":target.scope.session_id,"attachmentId":attachment_id}),
            &format!("native-dsh-mixed-file-read-{}", target.scope.session_id),
        )
        .await?;
    require(
        read["attachment"]["attachmentId"] == attachment_id
            && read["attachment"]["name"] == FILE_NAME
            && read["attachment"]["bytes"] == FILE_BYTES.len()
            && read["data"] == base64(FILE_BYTES),
        "Native public file read differs from uploaded bytes",
    )?;
    Ok(json!({"supported":true,"read":read}))
}

fn same_identity(before: &TranscriptItem, after: &TranscriptItem) -> bool {
    before.id == after.id
        && before.record_id == after.record_id
        && before.reconciliation_id == after.reconciliation_id
        && before.source_position == after.source_position
        && before.source_sequence == after.source_sequence
        && before.turn_id == after.turn_id
        && before.kind == after.kind
        && before.text == after.text
        && before.attachments == after.attachments
        && before.content_blocks == after.content_blocks
        && before.is_final == after.is_final
        && before.is_provisional == after.is_provisional
}

async fn run_scenario(
    runtime: &Arc<SessionRuntime>,
    kind: &'static str,
    scope: RunScope,
    scenario: &Scenario,
) -> Result<Value> {
    eprintln!("NATIVE_DSH_MIXED_PHASE open-scenario");
    let binding = Arc::new(RunBinding::open_main(runtime.clone(), scope.clone()).await?);
    require(
        binding.supports_inline_images(),
        "inline image capability unavailable",
    )?;
    require(
        binding.supports_attachments(),
        "file upload capability unavailable",
    )?;
    let target = Target {
        runtime: runtime.clone(),
        kind,
        scope: scope.clone(),
        binding,
    };
    let destination = ComposerDestination::run(scope.clone());
    let run = target.binding.current_run().ok_or(Error::Disconnected)?;
    require(
        run.draft.text.is_empty() && run.draft.artifacts.is_empty(),
        "probe Composer draft must start empty",
    )?;
    let excluded_ids = baseline(&target.binding);
    if !scenario.authored_text.is_empty() {
        target
            .binding
            .edit_text(&destination, scenario.authored_text.to_owned())?;
    }
    let first_bytes = encode_png(scenario.first_rgb)?;
    let last_bytes = encode_png(scenario.last_rgb)?;
    require(
        first_bytes != last_bytes,
        "mixed image fixtures must be different",
    )?;
    let first_data = attach_image(&target, scenario.first_name, &first_bytes).await?;
    let first = canonical_image(&target, &first_data, scenario.first_name)?;
    eprintln!("NATIVE_DSH_MIXED_PHASE upload");
    let upload = upload_file(&target).await?;
    let last_data = attach_image(&target, scenario.last_name, &last_bytes).await?;
    let last = canonical_image(&target, &last_data, scenario.last_name)?;
    let images = vec![
        json!({"type":"image","mediaType":"image/png","name":scenario.first_name,"data":first_data,"byteCount":first_bytes.len(),"width":2,"height":2}),
        json!({"type":"image","mediaType":"image/png","name":scenario.last_name,"data":last_data,"byteCount":last_bytes.len(),"width":2,"height":2}),
    ];
    eprintln!("NATIVE_DSH_MIXED_PHASE prepare");
    let ticket = target
        .binding
        .prepare_send(&destination, PromptMode::Queue)?;
    let rpc_id = ticket.id().to_owned();
    let prepared = prepared_receipt(&ticket, scenario.authored_text, &first, &upload, &last)?;
    target
        .binding
        .dispatch_prepared_with(ticket, &NO_PREPARATION)
        .await?;
    eprintln!("NATIVE_DSH_MIXED_PHASE wait-committed");
    let (input, assistant) = wait_round_trip(&target, &rpc_id, &excluded_ids).await?;
    require(
        input.text == scenario.authored_text,
        "committed input text differs",
    )?;
    eprintln!("NATIVE_DSH_MIXED_PHASE image-reads");
    eprintln!(
        "NATIVE_DSH_MIXED_PUBLIC_ATTACHMENTS {}",
        json!(input.attachments)
    );
    let image_reads = verify_images(&target, &input, &images, &upload).await?;
    eprintln!("NATIVE_DSH_MIXED_PHASE file-read");
    let file_read = file_read(&target, &upload).await?;
    Ok(json!({
        "scope":scope,
        "rpcId":rpc_id,
        "authoredText":scenario.authored_text,
        "prepared":prepared,
        "uploadReceipt":upload,
        "imageInputs":images,
        "input":public_item(&input),
        "assistant":public_item(&assistant),
        "imageReads":image_reads,
        "fileRead":file_read,
    }))
}

async fn run_busy_scenario(
    runtime: &Arc<SessionRuntime>,
    kind: &'static str,
    scope: RunScope,
    scenario: &Scenario,
) -> Result<(Value, Value)> {
    eprintln!("NATIVE_DSH_MIXED_PHASE open-busy-scenario");
    let binding = Arc::new(RunBinding::open_main(runtime.clone(), scope.clone()).await?);
    require(
        binding.supports_inline_images() && binding.supports_attachments(),
        "busy mixed attachments unavailable",
    )?;
    let target = Target {
        runtime: runtime.clone(),
        kind,
        scope: scope.clone(),
        binding,
    };
    let destination = ComposerDestination::run(scope.clone());
    let initial = target.binding.current_run().ok_or(Error::Disconnected)?;
    require(
        initial.draft.text.is_empty() && initial.draft.artifacts.is_empty(),
        "busy scenario Composer draft must start empty",
    )?;
    let excluded_ids = baseline(&target.binding);
    target
        .binding
        .edit_text(&destination, BUSY_HOLD_TEXT.to_owned())?;
    let hold_ticket = target
        .binding
        .prepare_send(&destination, PromptMode::Queue)?;
    let hold_rpc_id = hold_ticket.id().to_owned();
    target
        .binding
        .dispatch_prepared_with(hold_ticket, &NO_PREPARATION)
        .await?;
    eprintln!("NATIVE_DSH_MIXED_PHASE wait-held-running");
    let hold_input = wait_committed_user(&target, &hold_rpc_id).await?;

    let first_bytes = encode_png(scenario.first_rgb)?;
    let last_bytes = encode_png(scenario.last_rgb)?;
    require(
        first_bytes != last_bytes,
        "mixed image fixtures must be different",
    )?;
    target
        .binding
        .edit_text(&destination, scenario.authored_text.to_owned())?;
    let first_data = attach_image(&target, scenario.first_name, &first_bytes).await?;
    let first = canonical_image(&target, &first_data, scenario.first_name)?;
    eprintln!("NATIVE_DSH_MIXED_PHASE busy-upload");
    let upload = upload_file(&target).await?;
    let last_data = attach_image(&target, scenario.last_name, &last_bytes).await?;
    let last = canonical_image(&target, &last_data, scenario.last_name)?;
    let images = vec![
        json!({"type":"image","mediaType":"image/png","name":scenario.first_name,"data":first_data,"byteCount":first_bytes.len(),"width":2,"height":2}),
        json!({"type":"image","mediaType":"image/png","name":scenario.last_name,"data":last_data,"byteCount":last_bytes.len(),"width":2,"height":2}),
    ];
    eprintln!("NATIVE_DSH_MIXED_PHASE busy-prepare");
    let ticket = target
        .binding
        .prepare_send(&destination, PromptMode::Queue)?;
    let rpc_id = ticket.id().to_owned();
    let prepared = prepared_receipt(&ticket, scenario.authored_text, &first, &upload, &last)?;
    target
        .binding
        .dispatch_prepared_with(ticket, &NO_PREPARATION)
        .await?;
    eprintln!("NATIVE_DSH_MIXED_PHASE wait-public-queue-fact");
    let queue_ready = wait_busy_queue(
        &target,
        &BusyQueueContext {
            rpc_id: &rpc_id,
            authored_text: scenario.authored_text,
            prepared: &prepared,
            upload: &upload,
            images: &images,
            image_bytes: [&first_bytes, &last_bytes],
            hold_rpc_id: &hold_rpc_id,
            hold_input: &hold_input,
        },
    )
    .await?;
    let mut line = serde_json::to_string(&queue_ready)
        .map_err(|_| Error::Protocol("busy queue readiness JSON encoding failed".into()))?;
    line.insert_str(0, "AGENTKIT_NATIVE_DSH_BUSY_QUEUE_READY ");
    println!("{line}");
    std::io::stdout()
        .flush()
        .map_err(|_| Error::Protocol("busy queue readiness flush failed".into()))?;

    eprintln!("NATIVE_DSH_MIXED_PHASE wait-held-final");
    let (held_user, hold_assistant) = wait_round_trip(&target, &hold_rpc_id, &excluded_ids).await?;
    require(
        same_identity(&hold_input, &held_user),
        "held input identity changed before final",
    )?;
    eprintln!("NATIVE_DSH_MIXED_PHASE wait-queued-final");
    let mut queue_excluded = excluded_ids;
    queue_excluded.push(held_user.id.clone());
    let (input, assistant) = wait_round_trip(&target, &rpc_id, &queue_excluded).await?;
    require(
        input.text == scenario.authored_text,
        "queued mixed input text differs",
    )?;
    eprintln!("NATIVE_DSH_MIXED_PHASE busy-image-reads");
    let image_reads = verify_images(&target, &input, &images, &upload).await?;
    eprintln!("NATIVE_DSH_MIXED_PHASE busy-file-read");
    let file_read = file_read(&target, &upload).await?;
    let round_trip = json!({
        "scope":scope,
        "rpcId":rpc_id,
        "authoredText":scenario.authored_text,
        "prepared":prepared,
        "uploadReceipt":upload,
        "imageInputs":images,
        "input":public_item(&input),
        "assistant":public_item(&assistant),
        "imageReads":image_reads,
        "fileRead":file_read,
    });
    let drained = wait_queue_drained(&target).await?;
    let busy = json!({
        "drained":drained,
        "recoveredDrained":null,
        "holdRpcId":hold_rpc_id,
        "holdInput":public_item(&held_user),
        "holdAssistant":public_item(&hold_assistant),
        "queueReady":queue_ready,
        "recoveredHoldInput":null,
        "recoveredHoldAssistant":null,
    });
    Ok((round_trip, busy))
}

async fn wait_queue_drained(target: &Target) -> Result<Value> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_run(target).await?;
        let common_empty = snapshot
            .run
            .as_ref()
            .is_some_and(|run| run.queue.is_empty());
        let witness = target.runtime.inspect(|state| {
            if state.connection_epoch != snapshot.connection_epoch { return None; }
            let session = state.sessions.get(&target.scope.session_id)?;
            if target.kind == "dsh" {
                let projection = session.projections.get("inbox")?;
                if !["next-turn", "next-step"].iter().all(|key| projection.value[*key].as_array().is_some_and(Vec::is_empty)) { return None; }
                Some(json!({"kind":"inbox","projection":{"sequence":projection.sequence,"value":projection.value}}))
            } else if session.queue.is_empty() {
                Some(json!({"kind":"control","items":session.queue}))
            } else { None }
        });
        if common_empty && let Some(witness) = witness {
            return Ok(
                json!({"scope":target.scope,"connectionEpoch":snapshot.connection_epoch,"snapshotRevision":snapshot.revision,"commonQueue":[],"publicQueue":witness}),
            );
        }
        require(
            started.elapsed() < WAIT,
            "consumed busy queue did not drain",
        )?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn verify_recovered(
    runtime: &Arc<SessionRuntime>,
    kind: &'static str,
    scope: RunScope,
    original: &Value,
    epoch_before: u64,
    epoch_after: u64,
) -> Result<Value> {
    let binding = Arc::new(RunBinding::open_main(runtime.clone(), scope.clone()).await?);
    let target = Target {
        runtime: runtime.clone(),
        kind,
        scope,
        binding,
    };
    let original_input: TranscriptItem = serde_json::from_value(original["input"].clone())?;
    let original_assistant: TranscriptItem = serde_json::from_value(original["assistant"].clone())?;
    let input = find_by_id(&target, &original_input.id).await?;
    let assistant = find_by_id(&target, &original_assistant.id).await?;
    require(
        same_identity(&original_input, &input)
            && same_identity(&original_assistant, &assistant)
            && assistant.text == EXPECTED_REPLY
            && assistant.is_final
            && !assistant.is_provisional,
        "runtime reconnect changed committed transcript identity or attachments",
    )?;
    let images = original["imageInputs"]
        .as_array()
        .ok_or_else(|| Error::Protocol("receipt image inputs missing".into()))?;
    let upload = &original["uploadReceipt"];
    let image_reads = verify_images(&target, &input, images, upload).await?;
    let file_read = file_read(&target, upload).await?;
    let mut recovered = original.clone();
    recovered["scope"] = json!(target.scope);
    recovered["epochBefore"] = json!(epoch_before);
    recovered["epochAfter"] = json!(epoch_after);
    recovered["input"] = public_item(&input);
    recovered["assistant"] = public_item(&assistant);
    recovered["imageReads"] = json!(image_reads);
    recovered["fileRead"] = file_read;
    Ok(recovered)
}

async fn execute(config_path: &Path) -> Result<Value> {
    let mut config: Value = serde_json::from_slice(
        &fs::read(config_path)
            .map_err(|_| Error::Protocol("endpoint config read failed".into()))?,
    )
    .map_err(|_| Error::Protocol("endpoint config JSON invalid".into()))?;
    let busy_queue = match config
        .as_object_mut()
        .and_then(|object| object.remove("busyQueue"))
    {
        None => false,
        Some(Value::Bool(enabled)) => enabled,
        Some(_) => return Err(Error::Protocol("busyQueue must be boolean".into())),
    };
    let configuration = EndpointConfiguration::parse(&config, |name| env::var(name).ok())?;
    let configured = configuration.into_connector().await?;
    require(
        matches!(configured.kind, "native" | "dsh"),
        "endpoint config must select Native or DSH",
    )?;
    let kind = configured.kind;
    let scopes = configured.scopes;
    require(
        scopes[0].session_id != scopes[1].session_id,
        "two distinct sessions required",
    )?;
    let observer_endpoint = if busy_queue && kind == "native" {
        Some(configured.connector.connect().await?)
    } else {
        None
    };
    let observer_task = if let Some(endpoint) = &observer_endpoint {
        let mut frames = endpoint
            .follow(tekes_agent_contract::StreamTarget::SessionControl)
            .await?;
        Some(tokio::spawn(async move {
            while let Some(frame) = frames.next().await {
                match frame {
                    Ok(frame) if frame["type"] == "baseline" => eprintln!(
                        "NATIVE_BUSY_PUBLIC_CONTROL_FRAME {}",
                        json!({"type":"baseline","generation":frame["generation"],"queues":frame["snapshot"]["queues"]})
                    ),
                    Ok(frame) if frame["type"] == "queue" => {
                        eprintln!("NATIVE_BUSY_PUBLIC_CONTROL_FRAME {frame}")
                    }
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!("NATIVE_BUSY_PUBLIC_CONTROL_OBSERVER_ERROR");
                        break;
                    }
                }
            }
        }))
    } else {
        None
    };
    let runtime = Arc::new(SessionRuntime::spawn(
        &scopes[0].endpoint_id,
        configured.connector,
    ));
    let execution = async {
        runtime.wait_ready(Duration::from_secs(30)).await?;
        let first = Scenario {
            authored_text: "NATIVE_DSH_MIXED",
            first_name: "第一张.png",
            first_rgb: [235, 36, 24],
            last_name: "第二张.png",
            last_rgb: [18, 88, 228],
        };
        let second = Scenario {
            authored_text: "",
            first_name: "空正文第一张.png",
            first_rgb: [24, 196, 42],
            last_name: "空正文第二张.png",
            last_rgb: [212, 42, 174],
        };
        let (round_trip_a, mut busy_evidence) = if busy_queue {
            let (round_trip, evidence) =
                run_busy_scenario(&runtime, kind, scopes[0].clone(), &first).await?;
            (round_trip, Some(evidence))
        } else {
            (run_scenario(&runtime, kind, scopes[0].clone(), &first).await?, None)
        };
        let round_trip_b = run_scenario(&runtime, kind, scopes[1].clone(), &second).await?;
        let epoch_before = runtime.inspect(|state| state.connection_epoch);
        eprintln!("NATIVE_DSH_MIXED_PHASE reconnect");
        runtime.reconnect().await?;
        let epoch_after = runtime.inspect(|state| state.connection_epoch);
        require(epoch_after > epoch_before, "runtime reconnect did not advance epoch")?;
        let recovered_a = verify_recovered(
            &runtime,
            kind,
            scopes[0].clone(),
            &round_trip_a,
            epoch_before,
            epoch_after,
        )
        .await?;
        let recovered_b = verify_recovered(
            &runtime,
            kind,
            scopes[1].clone(),
            &round_trip_b,
            epoch_before,
            epoch_after,
        )
        .await?;
        if let Some(evidence) = &mut busy_evidence {
            let original_input: TranscriptItem = serde_json::from_value(evidence["holdInput"].clone())?;
            let original_assistant: TranscriptItem = serde_json::from_value(evidence["holdAssistant"].clone())?;
            let hold_input = find_by_id(
                &Target {
                    runtime: runtime.clone(),
                    kind,
                    scope: scopes[0].clone(),
                    binding: Arc::new(RunBinding::open_main(runtime.clone(), scopes[0].clone()).await?),
                },
                &original_input.id,
            )
            .await?;
            let hold_assistant = find_by_id(
                &Target {
                    runtime: runtime.clone(),
                    kind,
                    scope: scopes[0].clone(),
                    binding: Arc::new(RunBinding::open_main(runtime.clone(), scopes[0].clone()).await?),
                },
                &original_assistant.id,
            )
            .await?;
            require(
                same_identity(&original_input, &hold_input)
                    && same_identity(&original_assistant, &hold_assistant),
                "reconnected busy hold identity changed",
            )?;
            evidence["recoveredHoldInput"] = public_item(&hold_input);
            evidence["recoveredHoldAssistant"] = public_item(&hold_assistant);
            let recovered_target = Target {
                runtime: runtime.clone(), kind, scope: scopes[0].clone(),
                binding: Arc::new(RunBinding::open_main(runtime.clone(), scopes[0].clone()).await?),
            };
            evidence["recoveredDrained"] = wait_queue_drained(&recovered_target).await?;
        }
        let mut receipt = json!({
            "status":"completed",
            "endpointKind":kind,
            "roundTrips":[round_trip_a,round_trip_b],
            "reconnect":{
                "epochBefore":epoch_before,
                "epochAfter":epoch_after,
                "recovered":[recovered_a,recovered_b],
            },
            "providerObservation":"not observed by this consumer; verify independently at the provider",
            "desktop":"not run",
        });
        if let Some(evidence) = busy_evidence {
            receipt["busyQueue"] = evidence;
        }
        Ok(receipt)
    }
    .await;
    let stopped = runtime.stop().await;
    if let Some(task) = observer_task {
        task.abort();
        let _ = task.await;
    }
    if let Some(endpoint) = observer_endpoint {
        endpoint.close().await;
    }
    match (execution, stopped) {
        (Ok(receipt), Ok(())) => Ok(receipt),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn write_receipt(path: &Path, value: &Value) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("receipt parent missing"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)
}

#[tokio::main]
async fn main() {
    let (config_path, receipt_path) = match parse_args() {
        Ok(paths) => paths,
        Err(_) => {
            eprintln!("native/dsh mixed probe failed: phase=arguments kind=invalid");
            std::process::exit(2);
        }
    };
    let document = match execute(&config_path).await {
        Ok(value) => value,
        Err(error) => json!({
            "status":"failed",
            "failureKind":match error {
                Error::Protocol(_) => "protocol", Error::Transport(_) => "transport",
                Error::Remote(_) => "remote", Error::Timeout => "timeout",
                Error::Disconnected => "disconnected", Error::Unsupported(_) => "unsupported",
                Error::Json(_) => "json",
            },
            "error":{"phase":"endpoint_or_contract","kind":"public_api_failure"},
            "desktop":"not run",
            "providerObservation":"not observed by this consumer",
        }),
    };
    if write_receipt(&receipt_path, &document).is_err() {
        eprintln!("native/dsh mixed probe failed: phase=receipt kind=write");
        std::process::exit(1);
    }
    if document["status"] != "completed" {
        eprintln!("native/dsh mixed probe failed: phase=execution kind=public_api_failure");
        std::process::exit(1);
    }
    println!("native/dsh mixed probe: status=completed");
}
