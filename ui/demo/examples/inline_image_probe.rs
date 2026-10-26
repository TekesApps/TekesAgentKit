//! Headless, real-endpoint inline image round trip for the published demo API.
//!
//! Usage:
//! cargo run -p tekes-agent-ui-demo --example inline_image_probe -- \
//!   --endpoint-config /absolute/endpoint-config.json \
//!   --receipt /absolute/new/receipt.json
//!
//! Credentials are named by the config and read from the process environment. The
//! receipt contains the submitted fixture image, so treat it as user data.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tekes_agent_contract::{Error, PromptMode, Result};
use tekes_agent_presentation::{
    ComposerDestination, RunScope, SubmissionPreparation, TranscriptItemKind,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{InlineImageInput, InlineImageOutcome, RunBinding};
use tekes_agent_ui_demo::endpoint_config::EndpointConfiguration;

const ONE_PIXEL_PNG: [u8; 69] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 96, 96, 96, 0, 0, 0, 4, 0,
    1, 246, 23, 56, 85, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];
const IMAGE_NAME: &str = "中文图片.png";
const IMAGE_DATA: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC";
const EXPECTED_REPLY: &str = "ENDPOINT_REPLY_OK";
const WAIT: Duration = Duration::from_secs(120);

struct NoPreparation;
#[async_trait]
impl SubmissionPreparation for NoPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        ticket: &tekes_agent_presentation::PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        if ticket.settings().is_empty() {
            Ok(())
        } else {
            Err(Error::Protocol(
                "probe requires an empty settings draft".into(),
            ))
        }
    }
}

struct Target {
    runtime: Arc<SessionRuntime>,
    kind: &'static str,
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
                config = Some(
                    args.next()
                        .ok_or(Error::Protocol("missing endpoint config".into()))?,
                );
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
    let config_path =
        PathBuf::from(config.ok_or(Error::Protocol("missing --endpoint-config".into()))?);
    if !config_path.is_absolute() {
        return Err(Error::Protocol(
            "endpoint config path must be absolute".into(),
        ));
    }
    let receipt = receipt.ok_or(Error::Protocol("missing --receipt".into()))?;
    if !receipt.is_absolute() || receipt.exists() {
        return Err(Error::Protocol(
            "receipt must be an absolute new path".into(),
        ));
    }
    Ok((config_path, receipt))
}

fn item_receipt(item: &tekes_agent_presentation::TranscriptItem) -> Value {
    json!({
        "id": item.id,
        "recordId": item.record_id,
        "kind": item.kind,
        "text": item.text,
        "attachments": item.attachments,
        "isFinal": item.is_final,
        "isProvisional": item.is_provisional,
        "sourceSequence": item.source_sequence,
        "sourcePosition": item.source_position,
        "turnId": item.turn_id,
    })
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

async fn wait_for_round_trip(
    target: &Target,
    excluded_ids: &[String],
    expected_text: &str,
    expected_item_id: Option<&str>,
) -> Result<(Value, Value)> {
    let started = Instant::now();
    loop {
        let snapshot = target.binding.snapshot().await?;
        let tekes_agent_ui_bindings::PaneSnapshot::Run { snapshot: run, .. } = snapshot else {
            return Err(Error::Protocol(
                "main session transcript unavailable".into(),
            ));
        };
        // Native source positions are journal entry identities/ordinals, not
        // necessarily numeric sequences. Keep the original public position.
        if !run.transcript.is_stale {
            for user in run.transcript.items.iter().filter(|item| {
                item.kind == TranscriptItemKind::User
                    && item.text == expected_text
                    && !item.is_provisional
                    && !excluded_ids.contains(&item.id)
                    && expected_item_id.is_none_or(|id| item.id == id)
                    && (item.source_position.is_some() || item.source_sequence.is_some())
            }) {
                if user.id.is_empty() || user.turn_id.is_empty() {
                    return Err(Error::Protocol(
                        "transcript input lacks source identity".into(),
                    ));
                }
                let assistant = run.transcript.items.iter().find(|item| {
                    item.kind == TranscriptItemKind::Assistant
                        && item.turn_id == user.turn_id
                        && item.is_final
                        && !item.is_provisional
                        && !excluded_ids.contains(&item.id)
                        && (item.source_position.is_some() || item.source_sequence.is_some())
                        && !item.text.trim().is_empty()
                });
                if let Some(assistant) = assistant {
                    if assistant.id.is_empty() || assistant.text.trim() != EXPECTED_REPLY {
                        return Err(Error::Protocol(
                            "assistant differs from the endpoint fixture".into(),
                        ));
                    }
                    return Ok((item_receipt(user), item_receipt(assistant)));
                }
            }
        }
        if started.elapsed() >= WAIT {
            return Err(Error::Protocol(format!(
                "round trip timeout; public items: {}",
                json!(
                    run.transcript
                        .items
                        .iter()
                        .map(item_receipt)
                        .collect::<Vec<_>>()
                )
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn verify_image(target: &Target, input: &Value, phase: &str) -> Result<Value> {
    const DIGEST: &str = "c47dd9465c00e9a0c8b85e9ea58d3034a0d23b9cf926113602f3460752a4eb96";
    let attachments = input["attachments"]
        .as_array()
        .ok_or_else(|| Error::Protocol("public input has no attachment list".into()))?;
    if attachments.len() != 1 || attachments[0]["type"] != "image" {
        return Err(Error::Protocol(format!(
            "public image attachment differs: {attachments:?}"
        )));
    }
    let raw = &attachments[0];
    let id = match target.kind {
        "native" => {
            if raw["asset"] != format!("sha256-{DIGEST}")
                || raw["mime"] != "image/png"
                || raw["name"] != IMAGE_NAME
            {
                return Err(Error::Protocol(
                    "Native public asset reference differs".into(),
                ));
            }
            format!("sha256:{DIGEST}")
        }
        "pi" => {
            if raw["mimeType"] != "image/png" || raw["data"] != IMAGE_DATA {
                return Err(Error::Protocol("Pi public inline image differs".into()));
            }
            DIGEST.to_owned()
        }
        "dsh" => {
            let descriptor = &raw["attachment"];
            if descriptor["attachmentId"] != format!("sha256:{DIGEST}")
                || descriptor["mediaType"] != "image/png"
                || descriptor["name"] != IMAGE_NAME
                || descriptor["bytes"] != ONE_PIXEL_PNG.len()
                || descriptor["width"] != 1
                || descriptor["height"] != 1
            {
                return Err(Error::Protocol(
                    "DSH public image descriptor differs".into(),
                ));
            }
            descriptor["attachmentId"]
                .as_str()
                .ok_or_else(|| Error::Protocol("DSH image ID missing".into()))?
                .to_owned()
        }
        _ => return Err(Error::Unsupported("unknown image endpoint".into())),
    };
    let read = target
        .runtime
        .call(
            "session.attachment",
            json!({"sessionId":target.scope.session_id,"attachmentId":id}),
            &format!("image-probe-read-{phase}-{}", target.scope.session_id),
        )
        .await?;
    if read["data"] != IMAGE_DATA
        || read["attachment"]["attachmentId"] != id
        || read["attachment"]["mediaType"] != "image/png"
        || (target.kind != "native" && read["attachment"]["name"] != IMAGE_NAME)
        || read["attachment"]["bytes"] != ONE_PIXEL_PNG.len()
        || read["attachment"]["width"] != 1
        || read["attachment"]["height"] != 1
    {
        return Err(Error::Protocol(
            "public attachment read differs from source bytes and metadata".into(),
        ));
    }
    Ok(json!({"rawTranscriptAttachment":raw,"publicByteRead":true,"attachmentId":id,"read":read}))
}

async fn send_one(target: &Target, text: &str, label: &str) -> Result<Value> {
    let destination = ComposerDestination::run(target.scope.clone());
    let before = baseline(&target.binding);
    let initial = target.binding.current_run().ok_or(Error::Disconnected)?;
    if !initial.draft.text.is_empty() || !initial.draft.artifacts.is_empty() {
        return Err(Error::Protocol(
            "probe session has a pre-existing Composer draft".into(),
        ));
    }
    target.binding.edit_text(&destination, text.to_owned())?;
    let selection = target.binding.begin_inline_images(&destination)?;
    if selection
        .attach(&[InlineImageInput::new(
            IMAGE_NAME.to_owned(),
            Arc::from(ONE_PIXEL_PNG.as_slice()),
        )?])
        .await?
        != InlineImageOutcome::Attached
    {
        return Err(Error::Protocol("captured image was detached".into()));
    }
    let snapshot = target.binding.current_run().ok_or(Error::Disconnected)?;
    let attachment = snapshot
        .draft
        .artifacts
        .iter()
        .find_map(|artifact| match artifact {
            tekes_agent_presentation::ComposerArtifact::Attachment { content, .. }
                if content["type"] == "image" =>
            {
                Some(content)
            }
            _ => None,
        })
        .ok_or(Error::Protocol("image missing from canonical draft".into()))?;
    if attachment["data"] != IMAGE_DATA
        || attachment["mediaType"] != "image/png"
        || attachment["name"] != IMAGE_NAME
    {
        return Err(Error::Protocol(
            "canonical image bytes or metadata changed".into(),
        ));
    }
    let ticket = target
        .binding
        .prepare_send(&destination, PromptMode::Queue)?;
    target
        .binding
        .dispatch_prepared_with(ticket, &NoPreparation)
        .await?;
    let (input, output) = wait_for_round_trip(target, &before, text, None).await?;
    let input_text = input["text"].as_str().unwrap_or_default();
    if input_text != text {
        return Err(Error::Protocol(
            "public transcript input does not match probe".into(),
        ));
    }
    if !input["sourcePosition"].is_object() && input["sourceSequence"].as_i64().is_none() {
        return Err(Error::Protocol(
            "transcript input has no source position".into(),
        ));
    }
    let image_evidence = verify_image(target, &input, label).await?;
    Ok(json!({
        "label": label,
        "scope": target.scope,
        "excludedSourceIdsBefore": before,
        "input": input,
        "imageEvidence": image_evidence,
        "assistant": output,
        "image": {"name": IMAGE_NAME, "mediaType": "image/png", "data": IMAGE_DATA, "byteCount": ONE_PIXEL_PNG.len()},
    }))
}

async fn execute(config_path: &Path) -> Result<Value> {
    let config: Value = serde_json::from_slice(
        &fs::read(config_path)
            .map_err(|_| Error::Protocol("cannot read endpoint config file".into()))?,
    )
    .map_err(|_| Error::Protocol("endpoint config file must contain one JSON object".into()))?;
    let configuration = EndpointConfiguration::parse(&config, |name| env::var(name).ok())?;
    let Configured {
        connector,
        scopes,
        kind,
    } = {
        let configured = configuration.into_connector().await?;
        Configured {
            connector: configured.connector,
            scopes: configured.scopes,
            kind: configured.kind,
        }
    };
    let runtime = Arc::new(SessionRuntime::spawn(&scopes[0].endpoint_id, connector));
    let execution = async {
        runtime.wait_ready(Duration::from_secs(30)).await?;
        // Keep one active harness session at a time. Some harnesses expose a
        // process-wide active session even though the runtime stores both scopes.
        let mut sent = Vec::new();
        for (index, scope) in scopes.iter().cloned().enumerate() {
            let binding = Arc::new(RunBinding::open_main(runtime.clone(), scope.clone()).await?);
            if !binding.supports_inline_images() {
                return Err(Error::Protocol("endpoint does not advertise usable inline image policy".into()));
            }
            let target = Target { runtime: runtime.clone(), kind, scope, binding };
            let (text, label) = if index == 0 {
                ("请描述这张图片。", "text_and_image")
            } else {
                ("", "image_only")
            };
            sent.push(send_one(&target, text, label).await?);
        }
        let mixed = sent.remove(0);
        let image_only = sent.remove(0);
        let before_reconnect_epoch = runtime.inspect(|state| state.connection_epoch);
        runtime.reconnect().await?;
        let after_reconnect_epoch = runtime.inspect(|state| state.connection_epoch);
        if after_reconnect_epoch <= before_reconnect_epoch {
            return Err(Error::Protocol("reconnect did not advance connection epoch".into()));
        }
        let mut recovered = Vec::new();
        for (index, scope) in scopes.iter().cloned().enumerate() {
            let binding = Arc::new(RunBinding::open_main(runtime.clone(), scope.clone()).await?);
            let target = Target { runtime: runtime.clone(), kind, scope: scope.clone(), binding };
            let expected_text = if index == 0 { "请描述这张图片。" } else { "" };
            let expected_input_id = if index == 0 {
                mixed["input"]["id"].as_str()
            } else {
                image_only["input"]["id"].as_str()
            }
            .ok_or_else(|| Error::Protocol("initial transcript source ID missing".into()))?;
            let (input, output) =
                wait_for_round_trip(&target, &[], expected_text, Some(expected_input_id)).await?;
            let expected_output_id = if index == 0 {
                mixed["assistant"]["id"].as_str()
            } else {
                image_only["assistant"]["id"].as_str()
            }
            .ok_or_else(|| Error::Protocol("initial assistant source ID missing".into()))?;
            if output["id"] != expected_output_id {
                return Err(Error::Protocol(
                    "reopened transcript assistant source identity changed".into(),
                ));
            }
            let image_evidence = verify_image(&target, &input, "reconnected").await?;
            recovered.push(json!({
                "scope": scope,
                "reopenedConnectionEpoch": after_reconnect_epoch,
                "imageEvidence": image_evidence,
                "input": input,
                "assistant": output,
                "transcriptItemCount": target.binding.current_run().map(|run| run.transcript.items.len()),
            }));
        }
        Ok(json!({
            "status": "completed",
            "endpointKind": kind,
            "sessions": [mixed["scope"].clone(), image_only["scope"].clone()],
            "roundTrips": [mixed, image_only],
            "reconnect": {"epochBefore": before_reconnect_epoch, "epochAfter": after_reconnect_epoch, "recovered": recovered},
            "providerPromptCount": null,
            "providerObservation": "not observed by this consumer; verify independently at the provider",
            "desktop": "not run",
        }))
    }.await;
    let stopped = runtime.stop().await;
    match (execution, stopped) {
        (Ok(receipt), Ok(())) => Ok(receipt),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

struct Configured {
    connector: Arc<dyn tekes_agent_contract::EndpointConnector>,
    scopes: [RunScope; 2],
    kind: &'static str,
}

fn write_receipt(path: &Path, value: &Value) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("receipt parent missing"))?;
    fs::create_dir_all(parent)?;
    use std::io::Write;
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
        Ok(values) => values,
        Err(error) => {
            eprintln!("inline image probe: {error}");
            std::process::exit(2);
        }
    };
    let result = execute(&config_path).await;
    let document = match result {
        Ok(value) => value,
        Err(error) => json!({"status":"failed", "error":error.to_string(), "desktop":"not run"}),
    };
    if let Err(error) = write_receipt(&receipt_path, &document) {
        eprintln!("inline image probe failed to write receipt: {error}");
        std::process::exit(1);
    }
    if document["status"] != "completed" {
        eprintln!(
            "inline image probe failed; receipt written to {}",
            receipt_path.display()
        );
        std::process::exit(1);
    }
    println!("inline image probe receipt: {}", receipt_path.display());
}
