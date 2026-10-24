//! Real Pi public-API probe for ordered image/file/image attachments and queue reconciliation.
//!
//! The endpoint configuration must be one existing Pi endpoint with two distinct session IDs.
//! The provider-side runner holds `PI_MIXED_QUEUE_HOLD` until it sees and validates the
//! `AGENTKIT_PI_MIXED_QUEUE_READY {json}` line, then releases that request. All three prompts
//! must receive the exact text `ENDPOINT_REPLY_OK`.
//!
//! Usage:
//! cargo run -p tekes-agent-ui-demo --example pi_mixed_file_probe -- \
//!   --endpoint-config /absolute/pi-config.json --receipt /absolute/new/receipt.json
//!
//! The receipt contains generated image bytes in canonical prompt blocks and the small uploaded
//! text fixture. Treat it as test data. Credentials are resolved by EndpointConfiguration from
//! the named process environment variables and are never copied into the receipt.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tekes_agent_contract::{Error, PromptMode, Result, require};
use tekes_agent_presentation::{
    ComposerDestination, RunScope, SubmissionPlan, SubmissionPreparation,
    TranscriptAttachmentDisplayItem, TranscriptItem, TranscriptItemKind, TranscriptSourcePosition,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{
    AttachmentFile, AttachmentOutcome, InlineImageInput, InlineImageOutcome, PaneSnapshot,
    RunBinding,
};
use tekes_agent_ui_demo::endpoint_config::EndpointConfiguration;

const DIRECT_TEXT: &str = "PI_MIXED_DIRECT";
const HOLD_TEXT: &str = "PI_MIXED_QUEUE_HOLD";
const QUEUED_TEXT: &str = "PI_MIXED_QUEUED";
const EXPECTED_REPLY: &str = "ENDPOINT_REPLY_OK";
const FILE_NAME: &str = "中文混合.txt";
const FILE_BYTES: &[u8] = b"AgentKit Pi mixed file proof\n";
const WAIT: Duration = Duration::from_secs(180);

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
    scope: RunScope,
    binding: Arc<RunBinding>,
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
            .map_err(|_| Error::Protocol("could not encode PNG fixture".into()))?;
        writer
            .write_image_data(&rgb.repeat(4))
            .map_err(|_| Error::Protocol("could not encode PNG fixture pixels".into()))?;
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

fn item_json(item: &TranscriptItem) -> Value {
    json!(item)
}

fn display_order_json(order: Vec<TranscriptAttachmentDisplayItem>) -> Vec<Value> {
    order
        .into_iter()
        .map(|item| match item {
            TranscriptAttachmentDisplayItem::MessageAttachment { attachment_index } => {
                json!({"kind":"image","attachmentIndex":attachment_index})
            }
            TranscriptAttachmentDisplayItem::AdmittedFile { file_index } => {
                json!({"kind":"file","fileIndex":file_index})
            }
        })
        .collect()
}

async fn fresh_snapshot(target: &Target) -> Result<Arc<tekes_agent_presentation::RunSnapshot>> {
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
            return Err(Error::Protocol(
                "Pi main session snapshot or scope unavailable".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn transcript<'a>(
    _target: &Target,
    snapshot: &'a tekes_agent_presentation::RunSnapshot,
) -> Result<&'a Vec<TranscriptItem>> {
    require(!snapshot.transcript.is_stale, "transcript is stale")?;
    Ok(&snapshot.transcript.items)
}

fn same_source_identity(before: &TranscriptItem, after: &TranscriptItem) -> bool {
    matches!(
        (&before.source_position, &after.source_position),
        (
            Some(TranscriptSourcePosition::Native { entry_id: left, .. }),
            Some(TranscriptSourcePosition::Native { entry_id: right, .. })
        ) if left == right
    ) && before.id == after.id
        && before.record_id == after.record_id
        && before.reconciliation_id == after.reconciliation_id
        && before.turn_id == after.turn_id
}

async fn wait_round_trip(target: &Target, text: &str, rpc_id: &str) -> Result<(Value, Value)> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_snapshot(target).await?;
        for user in transcript(target, &snapshot)?.iter().filter(|item| {
            item.kind == TranscriptItemKind::User
                && (item.text == text
                    || item
                        .text
                        .starts_with(&format!("{text}\n\nAttached file: \"{FILE_NAME}\" (path: ")))
                && item.reconciliation_id.as_deref() == Some(rpc_id)
                && !item.is_provisional
                && (item.source_position.is_some() || item.source_sequence.is_some())
        }) {
            let assistant = transcript(target, &snapshot)?.iter().find(|item| {
                item.kind == TranscriptItemKind::Assistant
                    && item.turn_id == user.turn_id
                    && item.is_final
                    && !item.is_provisional
                    && !item.text.trim().is_empty()
                    && (item.source_position.is_some() || item.source_sequence.is_some())
            });
            if let Some(assistant) = assistant {
                require(
                    assistant.text.trim() == EXPECTED_REPLY,
                    "provider reply differs from fixture",
                )?;
                return Ok((item_json(user), item_json(assistant)));
            }
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn image_count(attachments: &[Value]) -> usize {
    attachments
        .iter()
        .filter(|item| item["type"] == "image")
        .count()
}

fn assert_attachment_contract(
    item: &TranscriptItem,
    receipt_id: &str,
    image_a: &str,
    image_b: &str,
) -> Result<Value> {
    require(
        item.kind == TranscriptItemKind::User,
        "expected user transcript item",
    )?;
    require(
        image_count(&item.attachments) == 2,
        "transcript must retain two raw images",
    )?;
    require(
        item.attachments[0]["type"] == "image"
            && item.attachments[0]["data"] == image_a
            && item.attachments[1]["type"] == "image"
            && item.attachments[1]["data"] == image_b,
        "raw image order or bytes changed",
    )?;
    let admitted = item.admitted_file_attachments();
    require(
        admitted.len() == 1,
        "expected one receipt-backed admitted file",
    )?;
    let file = &admitted[0];
    require(
        file.content_index == 2 && file.preceding_image_count == 1,
        "admitted file content position differs",
    )?;
    require(
        file.file.attachment_id == receipt_id
            && file.file.name == FILE_NAME
            && file.file.bytes == FILE_BYTES.len() as u64,
        "admitted file identity or bytes differ from upload",
    )?;
    let order = item.attachment_display_order();
    require(
        order
            == vec![
                TranscriptAttachmentDisplayItem::MessageAttachment {
                    attachment_index: 0,
                },
                TranscriptAttachmentDisplayItem::AdmittedFile { file_index: 0 },
                TranscriptAttachmentDisplayItem::MessageAttachment {
                    attachment_index: 1,
                },
            ],
        "attachment display order must be image, file, image",
    )?;
    Ok(json!({
        "files": admitted,
        "attachmentDisplayOrder": display_order_json(order),
    }))
}

async fn upload_file(target: &Target, name: &str, bytes: &[u8]) -> Result<Value> {
    let destination = ComposerDestination::run(target.scope.clone());
    let request = target.binding.begin_attachments(&destination)?;
    let file = AttachmentFile::new(
        name.to_owned(),
        "text/plain; charset=utf-8".into(),
        Arc::from(bytes),
    )?;
    let report = request.upload(&[file]).await?;
    require(
        report.outcome == AttachmentOutcome::Attached && report.receipts.len() == 1,
        "Pi file upload did not attach exactly one receipt",
    )?;
    let receipt = &report.receipts[0];
    require(
        receipt.file.name == name && receipt.file.bytes == bytes.len() as u64,
        "Pi upload receipt differs from selected file",
    )?;
    Ok(json!({
        "requestId": report.request_id,
        "fileReceipt": {
            "receiptId": receipt.receipt_id,
            "file": {
                "attachmentId": receipt.file.attachment_id,
                "name": receipt.file.name,
                "bytes": receipt.file.bytes,
            }
        },
    }))
}

async fn attach_image(target: &Target, name: &str, bytes: Vec<u8>) -> Result<String> {
    let destination = ComposerDestination::run(target.scope.clone());
    let selection = target.binding.begin_inline_images(&destination)?;
    let encoded = base64(&bytes);
    require(
        selection
            .attach(&[InlineImageInput::new(name.to_owned(), Arc::from(bytes))?])
            .await?
            == InlineImageOutcome::Attached,
        "Pi inline image selection detached",
    )?;
    Ok(encoded)
}

fn assert_prepared_mixed(
    ticket: &tekes_agent_presentation::PreparedSubmission,
    expected_text: &str,
    first_image: (&str, usize, &str),
    file_receipt: &Value,
    last_image: (&str, usize, &str),
) -> Result<Value> {
    let (image_a, image_a_bytes, image_a_name) = first_image;
    let (image_b, image_b_bytes, image_b_name) = last_image;
    let SubmissionPlan::Prompt {
        content,
        display_text,
    } = ticket.plan()
    else {
        return Err(Error::Protocol("mixed probe must prepare a prompt".into()));
    };
    require(display_text == expected_text, "frozen display text differs")?;
    require(
        content.len() == 4,
        "frozen prompt must contain text,image,file,image",
    )?;
    require(
        content[0]["type"] == "text" && content[0]["text"] == expected_text,
        "frozen prompt text differs",
    )?;
    require(
        content[1]["type"] == "image"
            && content[1]["data"] == image_a
            && content[1]["mediaType"] == "image/png"
            && content[1]["name"] == image_a_name
            && content[2]["type"] == "file"
            && content[2]["receiptId"] == file_receipt["receiptId"]
            && content[3]["type"] == "image"
            && content[3]["data"] == image_b
            && content[3]["mediaType"] == "image/png"
            && content[3]["name"] == image_b_name,
        "frozen attachment order or receipt differs",
    )?;
    let facts = ticket.attachment_facts();
    require(facts.len() == 3, "frozen attachment facts count differs")?;
    let first = facts[0]
        .facts()
        .ok_or_else(|| Error::Protocol("first image facts missing".into()))?;
    let file = facts[1]
        .facts()
        .ok_or_else(|| Error::Protocol("file facts missing".into()))?;
    let last = facts[2]
        .facts()
        .ok_or_else(|| Error::Protocol("second image facts missing".into()))?;
    require(
        first.byte_count == image_a_bytes as u64
            && first.media_type.as_deref() == Some("image/png")
            && file.byte_count == FILE_BYTES.len() as u64
            && file.attachment_id.as_deref() == file_receipt["file"]["attachmentId"].as_str()
            && last.byte_count == image_b_bytes as u64
            && last.media_type.as_deref() == Some("image/png"),
        "frozen attachment facts differ from captured bytes or receipt",
    )?;
    Ok(
        json!({"plan": content, "displayText": display_text, "attachmentFacts": facts.iter().map(|fact| json!({"artifactId":fact.artifact_id(),"facts":fact.facts()})).collect::<Vec<_>>() }),
    )
}

async fn verify_uploaded_file(target: &Target, upload: &Value) -> Result<Value> {
    let read = target
        .runtime
        .call(
            "session.fileAttachment",
            json!({
                "sessionId": target.scope.session_id,
                "attachmentId": upload["file"]["attachmentId"],
            }),
            &format!("pi-mixed-file-read-{}", target.scope.session_id),
        )
        .await?;
    require(
        read["attachment"]["attachmentId"] == upload["file"]["attachmentId"]
            && read["attachment"]["name"] == FILE_NAME
            && read["attachment"]["bytes"] == FILE_BYTES.len()
            && read["data"] == base64(FILE_BYTES),
        "public fileAttachment read differs from uploaded bytes",
    )?;
    Ok(read)
}

fn matching_queue_fact(
    queue: &[Value],
    rpc_id: &str,
    expected_text: &str,
    image_a: &str,
    receipt_id: &str,
    image_b: &str,
) -> Option<Value> {
    queue
        .iter()
        .find(|item| {
            let content = item["message"]["content"].as_array();
            item["rpcId"].as_str() == Some(rpc_id)
                && item["placement"] == "queued"
                && item["message"]["role"] == "user"
                && content.is_some_and(|content| {
                    content.len() == 4
                        && content[0]["type"] == "text"
                        && content[0]["text"] == expected_text
                        && content[1]["type"] == "image"
                        && content[1]["data"] == image_a
                        && content[2]["type"] == "file"
                        && content[2]["receiptId"] == receipt_id
                        && content[3]["type"] == "image"
                        && content[3]["data"] == image_b
                })
        })
        .cloned()
}

async fn wait_queue_fact(
    target: &Target,
    rpc_id: &str,
    image_a: &str,
    receipt_id: &str,
    image_b: &str,
) -> Result<(Value, u64, u64, u64)> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_snapshot(target).await?;
        if let Some(fact) = matching_queue_fact(
            snapshot
                .run
                .as_ref()
                .map(|run| run.queue.as_slice())
                .unwrap_or(&[]),
            rpc_id,
            QUEUED_TEXT,
            image_a,
            receipt_id,
            image_b,
        ) {
            return Ok((
                fact,
                snapshot.revision,
                snapshot.connection_epoch,
                snapshot
                    .run
                    .as_ref()
                    .map_or(0, |run| run.queue.len() as u64),
            ));
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Protocol(
                "public RunSnapshot.queue never contained the prepared ticket".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn native_source_evidence(
    runtime: &SessionRuntime,
    scope: &RunScope,
    input: &Value,
) -> Result<Value> {
    let projection = runtime.snapshot();
    let session = projection
        .sessions
        .get(&scope.session_id)
        .ok_or_else(|| Error::Protocol("public Pi runtime projection lacks session".into()))?;
    let conversation = session
        .native_conversation
        .as_ref()
        .ok_or_else(|| Error::Protocol("public Pi native conversation missing".into()))?;
    let rpc_id = input["reconciliationId"]
        .as_str()
        .ok_or_else(|| Error::Protocol("transcript reconciliation ID missing".into()))?;
    let record_id = input["recordId"]
        .as_str()
        .ok_or_else(|| Error::Protocol("transcript record ID missing".into()))?;
    let receipt = conversation.entries.iter().find(|entry| {
        matches!(
            entry["customType"].as_str(),
            Some("agentkit.inputReceipt.v1" | "tekes.inputReceipt.v1")
        ) && entry["data"]["rpcId"] == rpc_id
    });
    let message = conversation.entries.iter().find(|entry| {
        entry["id"] == record_id && entry["type"] == "message" && entry["message"]["role"] == "user"
    });
    let receipt = receipt
        .ok_or_else(|| Error::Protocol("matching public native inputReceipt missing".into()))?;
    let message = message
        .ok_or_else(|| Error::Protocol("matching public native user message missing".into()))?;
    Ok(json!({"nativeReceipt":receipt,"nativeMessage":message}))
}

async fn wait_draft_empty(target: &Target) -> Result<()> {
    let started = Instant::now();
    loop {
        if target
            .binding
            .current_run()
            .is_some_and(|run| run.draft.text.is_empty() && run.draft.artifacts.is_empty())
        {
            return Ok(());
        }
        if started.elapsed() >= Duration::from_secs(10) {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn execute(config_path: &Path) -> Result<Value> {
    let config: Value = serde_json::from_slice(
        &fs::read(config_path)
            .map_err(|_| Error::Protocol("cannot read endpoint config".into()))?,
    )
    .map_err(|_| Error::Protocol("endpoint config must be JSON".into()))?;
    require(config["kind"] == "pi", "endpoint config must be Pi")?;
    let configuration = EndpointConfiguration::parse(&config, |name| env::var(name).ok())?;
    let configured = configuration.into_connector().await?;
    require(configured.kind == "pi", "probe accepts only a Pi endpoint")?;
    let scope_a = configured.scopes[0].clone();
    let scope_b = configured.scopes[1].clone();
    require(
        scope_a.session_id != scope_b.session_id,
        "two distinct Pi sessions required",
    )?;
    let runtime = Arc::new(SessionRuntime::spawn(
        &scope_a.endpoint_id,
        configured.connector,
    ));
    let execution = async {
        eprintln!("PI_MIXED_PHASE runtime-ready");
        runtime.wait_ready(Duration::from_secs(30)).await?;
        eprintln!("PI_MIXED_PHASE open-bindings");
        let target_a = Target {
            runtime: runtime.clone(),
            scope: scope_a.clone(),
            binding: Arc::new(RunBinding::open_main(runtime.clone(), scope_a.clone()).await?),
        };
        let target_b = Target {
            runtime: runtime.clone(),
            scope: scope_b.clone(),
            binding: Arc::new(RunBinding::open_main(runtime.clone(), scope_b.clone()).await?),
        };
        require(target_a.binding.supports_inline_images(), "Pi inline image policy unavailable")?;
        require(target_a.binding.supports_attachments(), "Pi file upload capability unavailable")?;
        require(target_b.binding.supports_inline_images(), "Pi second session image policy unavailable")?;
        require(target_b.binding.supports_attachments(), "Pi second session file capability unavailable")?;
        wait_draft_empty(&target_a).await?;
        wait_draft_empty(&target_b).await?;

        // Prove the original second-session draft remains scoped while the first draft changes.
        let destination_b = ComposerDestination::run(scope_b.clone());
        target_b.binding.edit_text(&destination_b, "PI_MIXED_SCOPE_SENTINEL".into())?;
        require(
            target_b.binding.current_run().is_some_and(|run| run.draft.text == "PI_MIXED_SCOPE_SENTINEL")
                && target_a.binding.current_run().is_some_and(|run| run.draft.text.is_empty() && run.draft.artifacts.is_empty()),
            "Composer draft crossed Pi session scopes",
        )?;
        let image_a_bytes = encode_png([235, 36, 24])?;
        let image_b_bytes = encode_png([18, 88, 228])?;
        require(image_a_bytes != image_b_bytes, "generated PNG fixtures must differ")?;
        let image_a = base64(&image_a_bytes);
        let image_b = base64(&image_b_bytes);
        let (image_a_len, image_b_len) = (image_a_bytes.len(), image_b_bytes.len());
        let destination_a = ComposerDestination::run(scope_a.clone());
        target_a.binding.edit_text(&destination_a, DIRECT_TEXT.into())?;
        let captured_a = attach_image(&target_a, "第一张.png", image_a_bytes).await?;
        require(captured_a == image_a, "first canonical image encoding changed")?;
        let upload_a = upload_file(&target_a, FILE_NAME, FILE_BYTES).await?;
        let captured_b = attach_image(&target_a, "第二张.png", image_b_bytes).await?;
        require(captured_b == image_b, "second canonical image encoding changed")?;
        let baseline_a = target_a
            .binding
            .current_run()
            .map(|run| run.transcript.items.iter().map(|item| item.id.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        eprintln!("PI_MIXED_PHASE direct-prepare");
        let ticket_a = target_a.binding.prepare_send(&destination_a, PromptMode::Steer)?;
        let frozen_a = assert_prepared_mixed(&ticket_a, DIRECT_TEXT, (&image_a, image_a_len, "第一张.png"), &upload_a["fileReceipt"], (&image_b, image_b_len, "第二张.png"))?;
        let rpc_a = ticket_a.id().to_owned();
        target_a.binding.dispatch_prepared_with(ticket_a, &NoPreparation).await?;
        eprintln!("PI_MIXED_PHASE direct-round-trip");
        let (user_a, assistant_a) = wait_round_trip(&target_a, DIRECT_TEXT, &rpc_a).await?;
        require(
            !baseline_a.iter().any(|id| Some(id.as_str()) == user_a["id"].as_str()),
            "direct transcript input existed before probe",
        )?;
        let direct_item = transcript(&target_a, fresh_snapshot(&target_a).await?.as_ref())?
            .iter()
            .find(|item| item.id == user_a["id"])
            .cloned()
            .ok_or_else(|| Error::Protocol("direct user input disappeared".into()))?;
        eprintln!("PI_MIXED_PHASE direct-file-contract");
        let direct_files = assert_attachment_contract(&direct_item, upload_a["fileReceipt"]["file"]["attachmentId"].as_str().unwrap_or_default(), &image_a, &image_b)?;
        let public_file_read = verify_uploaded_file(&target_a, &upload_a["fileReceipt"]).await?;
        let direct_native_source = native_source_evidence(&runtime, &scope_a, &user_a)?;
        require(
            target_b
                .binding
                .current_run()
                .is_some_and(|run| run.draft.text == "PI_MIXED_SCOPE_SENTINEL")
                && target_a
                    .binding
                    .current_run()
                    .is_some_and(|run| run.draft.text.is_empty() && run.draft.artifacts.is_empty()),
            "direct send changed another session draft",
        )?;
        target_b.binding.edit_text(&destination_b, String::new())?;
        wait_draft_empty(&target_b).await?;

        // The provider runner holds this direct request while the real public queue projection
        // exposes the second mixed ticket. Spawn dispatch because the endpoint may await settlement.
        let destination_b = ComposerDestination::run(scope_b.clone());
        target_b.binding.edit_text(&destination_b, HOLD_TEXT.into())?;
        let _ = fresh_snapshot(&target_b).await?;
        eprintln!("PI_MIXED_PHASE hold-prepare");
        let hold_ticket = target_b.binding.prepare_send(&destination_b, PromptMode::Steer)?;
        eprintln!("PI_MIXED_PHASE hold-ticket-prepared");
        let hold_rpc = hold_ticket.id().to_owned();
        let hold_binding = target_b.binding.clone();
        let hold_dispatch = tokio::spawn(async move {
            hold_binding
                .dispatch_prepared_with(hold_ticket, &NO_PREPARATION)
                .await
        });
        eprintln!("PI_MIXED_PHASE hold-dispatch-spawned");
        let hold_started = Instant::now();
        loop {
            let snapshot = fresh_snapshot(&target_b).await?;
            if snapshot.transcript.items.iter().any(|item| {
                item.kind == TranscriptItemKind::User
                    && item.text == HOLD_TEXT
                    && item.reconciliation_id.as_deref() == Some(hold_rpc.as_str())
                    && !item.is_provisional
                    && item.source_position.is_some()
            }) {
                break;
            }
            if hold_started.elapsed() >= WAIT {
                return Err(Error::Protocol("held provider input did not appear in public transcript".into()));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        eprintln!("PI_MIXED_PHASE hold-user-committed");
        wait_draft_empty(&target_b).await?;

        let queued_image_a = encode_png([24, 196, 42])?;
        let queued_image_b = encode_png([212, 42, 174])?;
        require(queued_image_a != queued_image_b, "queued PNG fixtures must differ")?;
        let queued_a = base64(&queued_image_a);
        let queued_b = base64(&queued_image_b);
        let (queued_image_a_len, queued_image_b_len) = (queued_image_a.len(), queued_image_b.len());
        target_b.binding.edit_text(&destination_b, QUEUED_TEXT.into())?;
        let queued_capture_a = attach_image(&target_b, "排队第一张.png", queued_image_a).await?;
        require(queued_capture_a == queued_a, "queued first PNG encoding changed")?;
        let queued_upload = upload_file(&target_b, FILE_NAME, FILE_BYTES).await?;
        let queued_capture_b = attach_image(&target_b, "排队第二张.png", queued_image_b).await?;
        require(queued_capture_b == queued_b, "queued second PNG encoding changed")?;
        eprintln!("PI_MIXED_PHASE queue-prepare");
        let queued_ticket = target_b.binding.prepare_send(&destination_b, PromptMode::Queue)?;
        let queued_frozen = assert_prepared_mixed(&queued_ticket, QUEUED_TEXT, (&queued_a, queued_image_a_len, "排队第一张.png"), &queued_upload["fileReceipt"], (&queued_b, queued_image_b_len, "排队第二张.png"))?;
        let queued_rpc = queued_ticket.id().to_owned();
        let queued_binding = target_b.binding.clone();
        let queued_dispatch = tokio::spawn(async move {
            queued_binding
                .dispatch_prepared_with(queued_ticket, &NO_PREPARATION)
                .await
        });

        eprintln!("PI_MIXED_PHASE queue-public-fact");
        let (queue_fact, revision, epoch, queue_len) = wait_queue_fact(
            &target_b,
            &queued_rpc,
            &queued_a,
            queued_upload["fileReceipt"]["receiptId"].as_str().unwrap_or_default(),
            &queued_b,
        )
        .await?;
        let ready = json!({
            "status":"queued",
            "scope":scope_b,
            "ticketId":queued_rpc,
            "rpcId":queued_rpc,
            "queueItemId":queue_fact["id"],
            "mode":"queue",
            "snapshotRevision":revision,
            "connectionEpoch":epoch,
            "queueLength":queue_len,
            "queueFact":queue_fact,
            "providerProtocol":{"holdPrompt":HOLD_TEXT,"queuedPrompt":QUEUED_TEXT,"reply":EXPECTED_REPLY},
        });
        println!("AGENTKIT_PI_MIXED_QUEUE_READY {ready}");
        std::io::stdout()
            .flush()
            .map_err(|_| Error::Protocol("could not flush queue-ready receipt".into()))?;

        // The external runner now verifies its held-provider state and releases the hold.
        eprintln!("PI_MIXED_PHASE queue-round-trips");
        let (hold_user, hold_assistant) = wait_round_trip(&target_b, HOLD_TEXT, &hold_rpc).await?;
        let (queued_user, queued_assistant) = wait_round_trip(&target_b, QUEUED_TEXT, &queued_rpc).await?;
        let queued_item = transcript(&target_b, fresh_snapshot(&target_b).await?.as_ref())?
            .iter()
            .find(|item| item.id == queued_user["id"])
            .cloned()
            .ok_or_else(|| Error::Protocol("queued user input disappeared".into()))?;
        let queued_files = assert_attachment_contract(
            &queued_item,
            queued_upload["fileReceipt"]["file"]["attachmentId"].as_str().unwrap_or_default(),
            &queued_a,
            &queued_b,
        )?;
        let queued_file_read = verify_uploaded_file(&target_b, &queued_upload["fileReceipt"]).await?;
        let hold_native_source = native_source_evidence(&runtime, &scope_b, &hold_user)?;
        let queued_native_source = native_source_evidence(&runtime, &scope_b, &queued_user)?;
        let (hold_dispatch_result, queued_dispatch_result) = tokio::join!(hold_dispatch, queued_dispatch);
        hold_dispatch_result
            .map_err(|_| Error::Protocol("held dispatch task failed".into()))??;
        queued_dispatch_result
            .map_err(|_| Error::Protocol("queued dispatch task failed".into()))??;

        // Preserve an original raw source identity across the runtime's public reconnect path.
        let before_epoch = runtime.inspect(|state| state.connection_epoch);
        drop(target_a);
        drop(target_b);
        eprintln!("PI_MIXED_PHASE reconnect");
        runtime.reconnect().await?;
        let after_epoch = runtime.inspect(|state| state.connection_epoch);
        require(after_epoch > before_epoch, "Pi reconnect did not advance epoch")?;
        let recovered_a = Target {
            runtime: runtime.clone(),
            scope: scope_a.clone(),
            binding: Arc::new(RunBinding::open_main(runtime.clone(), scope_a.clone()).await?),
        };
        let recovered_b = Target {
            runtime: runtime.clone(),
            scope: scope_b.clone(),
            binding: Arc::new(RunBinding::open_main(runtime.clone(), scope_b.clone()).await?),
        };
        eprintln!("PI_MIXED_PHASE recovered-source");
        let direct_after = find_by_id(&recovered_a, user_a["id"].as_str().unwrap_or_default()).await?;
        let queued_after = find_by_id(&recovered_b, queued_user["id"].as_str().unwrap_or_default()).await?;
        let hold_after = find_by_id(&recovered_b, hold_user["id"].as_str().unwrap_or_default()).await?;
        let direct_assistant_after = find_by_id(&recovered_a, assistant_a["id"].as_str().unwrap_or_default()).await?;
        let queued_assistant_after = find_by_id(&recovered_b, queued_assistant["id"].as_str().unwrap_or_default()).await?;
        let original_direct_assistant: TranscriptItem = serde_json::from_value(assistant_a.clone())?;
        let original_queued_assistant: TranscriptItem = serde_json::from_value(queued_assistant.clone())?;
        require(
            same_source_identity(&original_direct_assistant, &direct_assistant_after)
                && same_source_identity(&original_queued_assistant, &queued_assistant_after)
                && direct_assistant_after.text == EXPECTED_REPLY
                && queued_assistant_after.text == EXPECTED_REPLY
                && direct_assistant_after.is_final && queued_assistant_after.is_final,
            "Pi reconnect changed final assistant identity or reply",
        )?;
        let recovered_direct_read = verify_uploaded_file(&recovered_a, &upload_a["fileReceipt"]).await?;
        let recovered_queued_read = verify_uploaded_file(&recovered_b, &queued_upload["fileReceipt"]).await?;
        require(
            same_source_identity(&direct_item, &direct_after)
                && same_source_identity(&queued_item, &queued_after),
            "Pi reconnect changed original raw source identity",
        )?;
        let recovered_direct_files = assert_attachment_contract(&direct_after, upload_a["fileReceipt"]["file"]["attachmentId"].as_str().unwrap_or_default(), &image_a, &image_b)?;
        let recovered_queued_files = assert_attachment_contract(&queued_after, queued_upload["fileReceipt"]["file"]["attachmentId"].as_str().unwrap_or_default(), &queued_a, &queued_b)?;
        let recovered_direct_native_source = native_source_evidence(&runtime, &scope_a, &item_json(&direct_after))?;
        let recovered_queued_native_source = native_source_evidence(&runtime, &scope_b, &item_json(&queued_after))?;
        Ok(json!({
            "status":"completed",
            "endpointKind":"pi",
            "sessions":{"direct":scope_a,"queued":scope_b},
            "draftIsolation":{"verified":true,"sentinel":"PI_MIXED_SCOPE_SENTINEL"},
            "direct":{"scope":scope_a,"rpcId":rpc_a,"imageInputs":[{"type":"image","mediaType":"image/png","name":"第一张.png","data":image_a},{"type":"image","mediaType":"image/png","name":"第二张.png","data":image_b}],"prepared":frozen_a,"fileReceipt":upload_a["fileReceipt"],"input":user_a,"assistant":assistant_a,"files":direct_files["files"],"attachmentDisplayOrder":direct_files["attachmentDisplayOrder"],"fileRead":public_file_read,"nativeSource":direct_native_source,"recovered":{"input":item_json(&direct_after),"assistant":item_json(&direct_assistant_after),"fileRead":recovered_direct_read,"files":recovered_direct_files["files"],"attachmentDisplayOrder":recovered_direct_files["attachmentDisplayOrder"],"reconciliationId":direct_after.reconciliation_id,"epochBefore":before_epoch,"epochAfter":after_epoch,"nativeSource":recovered_direct_native_source}},
            "queued":{"scope":scope_b,"hold":{"rpcId":hold_rpc,"input":hold_user,"assistant":hold_assistant,"nativeSource":hold_native_source},"rpcId":queued_rpc,"imageInputs":[{"type":"image","mediaType":"image/png","name":"排队第一张.png","data":queued_a},{"type":"image","mediaType":"image/png","name":"排队第二张.png","data":queued_b}],"prepared":queued_frozen,"fileReceipt":queued_upload["fileReceipt"],"queueFact":queue_fact,"queueItemId":queue_fact["id"],"readyFrame":ready,"input":queued_user,"assistant":queued_assistant,"files":queued_files["files"],"attachmentDisplayOrder":queued_files["attachmentDisplayOrder"],"fileRead":queued_file_read,"nativeSource":queued_native_source,"recovered":{"input":item_json(&queued_after),"assistant":item_json(&queued_assistant_after),"fileRead":recovered_queued_read,"files":recovered_queued_files["files"],"attachmentDisplayOrder":recovered_queued_files["attachmentDisplayOrder"],"reconciliationId":queued_after.reconciliation_id,"epochBefore":before_epoch,"epochAfter":after_epoch,"nativeSource":recovered_queued_native_source},"recoveredHoldInput":item_json(&hold_after)},
            "reconnect":{"epochBefore":before_epoch,"epochAfter":after_epoch,"sameDirectSourceIdentity":true,"sameQueuedSourceIdentity":true},
            "providerProtocol":{"directPrompt":DIRECT_TEXT,"holdPrompt":HOLD_TEXT,"queuedPrompt":QUEUED_TEXT,"requiredReply":EXPECTED_REPLY,"releaseHoldOnlyAfterQueueReadyValidation":true},
            "desktop":"not run",
            "providerObservation":"external runner must verify provider payload, hold/release, and three replies",
        }))
    }
    .await;
    let stopped = runtime.stop().await;
    match (execution, stopped) {
        (Ok(receipt), Ok(())) => Ok(receipt),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

async fn find_by_id(target: &Target, id: &str) -> Result<TranscriptItem> {
    let started = Instant::now();
    loop {
        let snapshot = fresh_snapshot(target).await?;
        if let Some(item) = transcript(target, &snapshot)?
            .iter()
            .find(|item| item.id == id)
        {
            return Ok(item.clone());
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Protocol(
                "reconnected transcript source identity missing".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
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
    let parsed = parse_args();
    let (config_path, receipt_path) = match parsed {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("Pi mixed probe arguments invalid: {error}");
            std::process::exit(2);
        }
    };
    let document = match execute(&config_path).await {
        Ok(value) => value,
        Err(error) => {
            let known = match &error {
                Error::Protocol(message)
                    if matches!(
                        message.as_str(),
                        "Pi main session snapshot or scope unavailable"
                            | "transcript is stale"
                            | "Run is not ready for input"
                            | "pane closing or outcome unknown"
                            | "pane pointer missing or changed"
                            | "pane destination mismatch"
                            | "empty submission"
                            | "submission requires reconciliation"
                    ) =>
                {
                    Some(message.as_str())
                }
                _ => None,
            };
            json!({
                "status":"failed",
                "failure":"Pi mixed-file probe operation failed; endpoint details omitted",
                "knownProtocolFailure":known,
                "errorKind":match &error {
                    Error::Protocol(_) => "protocol", Error::Transport(_) => "transport",
                    Error::Remote(_) => "remote", Error::Timeout => "timeout",
                    Error::Disconnected => "disconnected", Error::Unsupported(_) => "unsupported",
                    Error::Json(_) => "json",
                },
                "timeout":matches!(error, Error::Timeout),
                "desktop":"not run"
            })
        }
    };
    if let Err(error) = write_receipt(&receipt_path, &document) {
        eprintln!("Pi mixed probe could not write receipt: {error}");
        std::process::exit(1);
    }
    if document["status"] != "completed" {
        eprintln!(
            "Pi mixed probe failed; receipt written to {}",
            receipt_path.display()
        );
        std::process::exit(1);
    }
    println!("Pi mixed probe receipt: {}", receipt_path.display());
}
