use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use serde_json::{Value, json};
use tekes_agent_contract::{
    EndpointCapabilities, EndpointConnector, Error, FrameStream, HistoryPage, HistoryRequest,
    HostDescription, Result, SessionEndpoint, StreamTarget,
};
use tekes_agent_presentation::{
    ComposerArtifact, ComposerContextBaseline, ComposerDestination, PreparedSubmission, RunScope,
    SubmissionPlan, SubmissionPreparation,
};
use tekes_agent_session::SessionRuntime;
use tekes_agent_ui_bindings::{AttachmentFile, InlineImageInput, InlineImageOutcome, RunBinding};
use tekes_agent_ui_demo::fixture::Fixture;
use tokio::sync::Semaphore;

const ONE_PIXEL_PNG: [u8; 69] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0,
    0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 96, 96, 96, 0, 0, 0, 4, 0,
    1, 246, 23, 56, 85, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[derive(Clone)]
struct PolicyHost {
    fixture: Fixture,
    policy: Arc<Mutex<Value>>,
    calls: Arc<AtomicUsize>,
    prompts: Arc<Mutex<Vec<Value>>>,
    gate: Option<Arc<Semaphore>>,
    hide_upload: bool,
}

struct PolicyEndpoint {
    inner: Arc<dyn SessionEndpoint>,
    host: PolicyHost,
}

#[async_trait]
impl EndpointConnector for PolicyHost {
    async fn connect(&self) -> Result<Arc<dyn SessionEndpoint>> {
        Ok(Arc::new(PolicyEndpoint {
            inner: self.fixture.connect().await?,
            host: self.clone(),
        }))
    }
}

#[async_trait]
impl SessionEndpoint for PolicyEndpoint {
    fn identity(&self) -> &str {
        self.inner.identity()
    }

    fn host(&self) -> &HostDescription {
        self.inner.host()
    }

    fn capabilities(&self) -> EndpointCapabilities {
        let mut capabilities = self.inner.capabilities();
        capabilities.methods.insert("attachments.policy".into());
        if self.host.hide_upload {
            capabilities.methods.remove("session.uploadFile");
            capabilities.methods.remove("session.fileAttachment");
        }
        capabilities
    }

    async fn follow(&self, target: StreamTarget) -> Result<FrameStream> {
        self.inner.follow(target).await
    }

    async fn history_page(&self, request: HistoryRequest) -> Result<HistoryPage> {
        self.inner.history_page(request).await
    }

    async fn respond(&self, id: &str, revision: i64, outcome: Value) -> Result<()> {
        self.inner.respond(id, revision, outcome).await
    }

    async fn call(&self, method: &str, payload: Value, rpc_id: &str) -> Result<Value> {
        if method == "attachments.policy" {
            self.host.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.host.gate {
                gate.acquire()
                    .await
                    .map_err(|_| Error::Disconnected)?
                    .forget();
            }
            let policy = self.host.policy.lock().unwrap().clone();
            return Ok(policy);
        }
        if method == "session.prompt" {
            self.host.prompts.lock().unwrap().push(payload.clone());
        }
        self.inner.call(method, payload, rpc_id).await
    }

    async fn close(&self) {
        self.inner.close().await;
    }
}

fn policy(max_message_bytes: u64) -> Value {
    json!({
        "inlineMediaTypes": ["image/png", "image/jpeg", "image/gif", "image/webp"],
        "maxAttachmentBytes": 4_194_304,
        "maxAttachmentsPerMessage": 8,
        "maxMessageAttachmentBytes": max_message_bytes,
        "maxImagePixels": 33_554_432,
        "maxImageDimension": 8_192
    })
}

fn make_host(
    fixture: Fixture,
    value: Value,
    gate: Option<Arc<Semaphore>>,
    hide_upload: bool,
) -> PolicyHost {
    PolicyHost {
        fixture,
        policy: Arc::new(Mutex::new(value)),
        calls: Arc::new(AtomicUsize::new(0)),
        prompts: Arc::new(Mutex::new(Vec::new())),
        gate,
        hide_upload,
    }
}

async fn setup(
    fixture: Fixture,
    policy_value: Value,
    gate: Option<Arc<Semaphore>>,
    hide_upload: bool,
) -> (Arc<SessionRuntime>, Arc<RunBinding>, PolicyHost) {
    let endpoint = make_host(fixture, policy_value, gate, hide_upload);
    let runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(endpoint.clone())));
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    let binding = Arc::new(
        RunBinding::open_main(runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    (runtime, binding, endpoint)
}

fn destination(binding: &RunBinding) -> ComposerDestination {
    ComposerDestination::run(binding.scope().clone())
}

fn image() -> InlineImageInput {
    InlineImageInput::new("中文图片.png".into(), Arc::from(ONE_PIXEL_PNG.as_slice())).unwrap()
}

const PNG_DATA: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNgYGAAAAAEAAH2FzhVAAAAAElFTkSuQmCC";
fn assert_original_image_content(content: &Value) {
    assert_eq!(
        content["data"], PNG_DATA,
        "standard base64 must preserve the exact original PNG bytes"
    );
}

async fn wait_policy(host: &PolicyHost, count: usize) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while host.calls.load(Ordering::SeqCst) < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn chinese_image_keeps_original_bytes_and_order_without_generic_upload_and_sends_alone() {
    let fixture = Fixture::default();
    let (runtime, binding, host) = setup(fixture.clone(), policy(1024), None, true).await;
    assert!(binding.supports_inline_images());
    assert!(!binding.supports_attachments());
    let target = destination(&binding);
    binding.edit_text(&target, "先写中文".into()).unwrap();
    let request = binding.begin_inline_images(&target).unwrap();
    assert_eq!(
        request.attach(&[image()]).await.unwrap(),
        InlineImageOutcome::Attached
    );

    let snapshot = binding.current_run().unwrap();
    assert_eq!(snapshot.draft.text, "先写中文");
    let ComposerArtifact::Attachment { content, facts, .. } = &snapshot.draft.artifacts[0] else {
        panic!("image should be the first Composer artifact")
    };
    assert_eq!(content["type"], "image");
    assert_eq!(content["mediaType"], "image/png");
    assert_eq!(content["name"], "中文图片.png");
    assert_original_image_content(content);
    assert_eq!(
        facts.as_ref().unwrap().byte_count,
        ONE_PIXEL_PNG.len() as u64
    );
    assert_eq!(
        facts.as_ref().unwrap().media_type.as_deref(),
        Some("image/png")
    );
    let SubmissionPlan::Prompt {
        content: mixed_content,
        ..
    } = snapshot.draft.compile(false).unwrap()
    else {
        panic!("text and image should compile to a prompt")
    };
    assert_eq!(mixed_content[0], json!({"type":"text","text":"先写中文"}));
    assert_eq!(mixed_content[1]["type"], "image");
    assert_original_image_content(&mixed_content[1]);

    binding.edit_text(&target, String::new()).unwrap();
    let ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    let SubmissionPlan::Prompt { content, .. } = ticket.plan() else {
        panic!("image-only Composer should compile to a prompt")
    };
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "image");
    assert_original_image_content(&content[0]);
    binding
        .dispatch_prepared_with(ticket, &NoPreparation)
        .await
        .unwrap();
    let prompt = host.prompts.lock().unwrap()[0].clone();
    assert_eq!(prompt["content"][0]["mediaType"], "image/png");
    assert_original_image_content(&prompt["content"][0]);
    assert_eq!(fixture.upload_calls(), 0);
    assert_eq!(
        host.calls.load(Ordering::SeqCst),
        2,
        "capture and image-ticket validation each read policy"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn invalid_second_image_rejects_the_batch_and_a_selection_is_single_use() {
    let (runtime, binding, _) = setup(Fixture::default(), policy(1024), None, true).await;
    let target = destination(&binding);
    binding.edit_text(&target, "原文字".into()).unwrap();
    let before = binding.current_run().unwrap().draft.clone();
    let request = binding.begin_inline_images(&target).unwrap();
    let mut damaged = ONE_PIXEL_PNG;
    damaged[45] ^= 0xff;
    let corrupt = InlineImageInput::new("坏图.png".into(), Arc::from(damaged.as_slice())).unwrap();
    assert!(request.attach(&[image(), corrupt]).await.is_err());
    assert_eq!(binding.current_run().unwrap().draft, before);
    assert!(
        request.attach(&[image()]).await.is_err(),
        "selection cannot be replayed after failed validation"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn context_aba_during_policy_read_detaches_without_writing_to_the_new_context() {
    let gate = Arc::new(Semaphore::new(0));
    let (runtime, binding, host) =
        setup(Fixture::default(), policy(1024), Some(gate.clone()), true).await;
    let target = destination(&binding);
    binding.edit_text(&target, "原草稿".into()).unwrap();
    let request = Arc::new(binding.begin_inline_images(&target).unwrap());
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.attach(&[image()]).await.unwrap() })
    };
    wait_policy(&host, 1).await;

    let original = ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap();
    let marker = ComposerArtifact::Reference {
        id: "aba-marker".into(),
        mention: Some("暂时变更".into()),
        path: None,
        excerpt: None,
    };
    let mut changed = original.artifacts.clone();
    changed.push(marker);
    assert!(
        binding
            .patch_context(&original, changed, original.annotations.clone())
            .unwrap()
    );
    let changed = ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap();
    assert!(
        binding
            .patch_context(
                &changed,
                original.artifacts.clone(),
                original.annotations.clone()
            )
            .unwrap()
    );

    gate.add_permits(1);
    assert_eq!(task.await.unwrap(), InlineImageOutcome::Detached);
    assert!(binding.current_run().unwrap().draft.artifacts.is_empty());
    assert_eq!(binding.current_run().unwrap().draft.text, "原草稿");
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn text_edits_during_policy_read_survive_and_reconnect_detaches_old_capture() {
    let gate = Arc::new(Semaphore::new(0));
    let (runtime, binding, host) =
        setup(Fixture::default(), policy(1024), Some(gate.clone()), true).await;
    let target = destination(&binding);
    binding.edit_text(&target, "初始文本".into()).unwrap();
    let request = Arc::new(binding.begin_inline_images(&target).unwrap());
    let task = {
        let request = request.clone();
        tokio::spawn(async move { request.attach(&[image()]).await.unwrap() })
    };
    wait_policy(&host, 1).await;
    binding
        .edit_text(&target, "等待期间的新中文".into())
        .unwrap();
    gate.add_permits(1);
    assert_eq!(task.await.unwrap(), InlineImageOutcome::Attached);
    assert_eq!(
        binding.current_run().unwrap().draft.text,
        "等待期间的新中文"
    );
    assert_eq!(binding.current_run().unwrap().draft.artifacts.len(), 1);

    let reconnect_gate = Arc::new(Semaphore::new(0));
    // A new connector uses the same wrapped fixture authority but a fresh, held policy call.
    let endpoint = make_host(
        Fixture::default(),
        policy(1024),
        Some(reconnect_gate.clone()),
        true,
    );
    let reconnect_runtime = Arc::new(SessionRuntime::spawn("fixture", Arc::new(endpoint.clone())));
    reconnect_runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    let reconnect_binding = Arc::new(
        RunBinding::open_main(reconnect_runtime.clone(), RunScope::main("fixture", "a"))
            .await
            .unwrap(),
    );
    let reconnect_target = destination(&reconnect_binding);
    let pending = Arc::new(
        reconnect_binding
            .begin_inline_images(&reconnect_target)
            .unwrap(),
    );
    let pending_task = {
        let pending = pending.clone();
        tokio::spawn(async move { pending.attach(&[image()]).await.unwrap() })
    };
    wait_policy(&endpoint, 1).await;
    reconnect_runtime.reconnect().await.unwrap();
    reconnect_runtime
        .wait_ready(Duration::from_secs(3))
        .await
        .unwrap();
    reconnect_gate.add_permits(1);
    assert_eq!(pending_task.await.unwrap(), InlineImageOutcome::Detached);
    assert!(
        reconnect_binding
            .current_run()
            .unwrap()
            .draft
            .artifacts
            .is_empty()
    );
    runtime.stop().await.unwrap();
    reconnect_runtime.stop().await.unwrap();
}

#[tokio::test]
async fn pre_send_policy_revalidates_frozen_mixed_ticket_and_restores_original_attachments() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, binding, host) = setup(fixture.clone(), policy(1024), None, false).await;
    let target = destination(&binding);
    binding.edit_text(&target, "旧文字".into()).unwrap();
    let upload = binding.begin_attachments(&target).unwrap();
    assert!(matches!(
        upload
            .upload(&[AttachmentFile::new(
                "旧附件.txt".into(),
                "text/plain".into(),
                Arc::from(&b"known file bytes"[..]),
            )
            .unwrap(),])
            .await
            .unwrap()
            .outcome,
        tekes_agent_ui_bindings::AttachmentOutcome::Attached
    ));
    assert_eq!(
        binding
            .begin_inline_images(&target)
            .unwrap()
            .attach(&[image()])
            .await
            .unwrap(),
        InlineImageOutcome::Attached
    );
    binding
        .edit_text(&target, "发送时保留的新文字".into())
        .unwrap();
    let frozen_draft = binding.current_run().unwrap().draft.clone();
    let ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    assert!(matches!(ticket.plan(), SubmissionPlan::Prompt { .. }));
    binding
        .edit_text(&target, "准备期间的新中文".into())
        .unwrap();
    let result = binding
        .dispatch_prepared_with(
            ticket,
            &LowerMessageLimit {
                host: host.clone(),
                max_bytes: ONE_PIXEL_PNG.len() as u64 + 7,
            },
        )
        .await;
    assert!(
        result.is_err(),
        "lowered maxMessageAttachmentBytes must reject frozen image content"
    );
    assert!(
        host.prompts.lock().unwrap().is_empty(),
        "no prompt RPC may be attempted"
    );
    let after = binding.current_run().unwrap().draft.clone();
    assert_eq!(after.text, "准备期间的新中文");
    assert_eq!(
        after.artifacts, frozen_draft.artifacts,
        "original mixed attachments are restored after pre-send rejection"
    );
    assert_eq!(after.artifacts.len(), 2);
    assert_eq!(
        host.calls.load(Ordering::SeqCst),
        3,
        "file upload, image capture and pre-send validation each read policy"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn plain_text_and_legacy_file_ticket_do_not_add_image_policy_reads() {
    let fixture = Fixture::default().with_uploads();
    let (runtime, binding, host) = setup(fixture.clone(), policy(1024), None, false).await;
    let target = destination(&binding);
    binding.edit_text(&target, "普通文字".into()).unwrap();
    let text_ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    binding
        .dispatch_prepared_with(text_ticket, &NoPreparation)
        .await
        .unwrap();
    assert_eq!(host.calls.load(Ordering::SeqCst), 0);
    runtime.stop().await.unwrap();
    // Each public fixture admits one active prompt. Use an independent settled
    // file-only Run rather than racing the previous streaming response.
    let (runtime, binding, host) =
        setup(Fixture::default().with_uploads(), policy(1024), None, false).await;
    let target = destination(&binding);
    let request = binding.begin_attachments(&target).unwrap();
    let uploaded = request
        .upload(&[AttachmentFile::new(
            "file.txt".into(),
            "text/plain".into(),
            Arc::from(&b"legacy file"[..]),
        )
        .unwrap()])
        .await
        .unwrap();
    assert!(matches!(
        uploaded.outcome,
        tekes_agent_ui_bindings::AttachmentOutcome::Attached
    ));
    let after_capture = host.calls.load(Ordering::SeqCst);
    assert_eq!(
        after_capture, 1,
        "generic file selection performs its own policy read"
    );
    let file_ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    binding
        .dispatch_prepared_with(file_ticket, &NoPreparation)
        .await
        .unwrap();
    assert_eq!(
        host.calls.load(Ordering::SeqCst),
        after_capture,
        "file-only ticket has no extra inline-image policy read"
    );
    runtime.stop().await.unwrap();
}

#[tokio::test]
async fn image_ticket_with_missing_legacy_file_facts_is_rejected_when_total_limit_applies() {
    let fixture = Fixture::default();
    let (runtime, binding, host) = setup(fixture.clone(), policy(1024), None, true).await;
    let target = destination(&binding);
    binding.edit_text(&target, "带旧附件的输入".into()).unwrap();
    let baseline = ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap();
    let legacy_file = ComposerArtifact::Attachment {
        id: "legacy-file-no-byte-facts".into(),
        name: "旧文件.txt".into(),
        content: json!({"type":"file","receiptId":"old-file-receipt"}),
        facts: None,
    };
    let image_artifact = ComposerArtifact::Attachment {
        id: "legacy-image-no-byte-facts".into(),
        name: "旧图片.png".into(),
        content: json!({
            "type":"image",
            "mediaType":"image/png",
            "data": PNG_DATA,
            "name":"旧图片.png"
        }),
        facts: None,
    };
    assert!(
        binding
            .patch_context(
                &baseline,
                vec![legacy_file, image_artifact],
                baseline.annotations.clone(),
            )
            .unwrap()
    );
    let before_send = binding.current_run().unwrap().draft.clone();
    let ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    assert!(
        binding
            .dispatch_prepared_with(ticket, &NoPreparation)
            .await
            .is_err()
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    assert!(
        host.prompts.lock().unwrap().is_empty(),
        "no prompt RPC may be attempted"
    );
    assert_eq!(binding.current_run().unwrap().draft, before_send);
    runtime.stop().await.unwrap();
}

struct NoPreparation;

#[async_trait]
impl SubmissionPreparation for NoPreparation {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        Ok(())
    }
}

struct LowerMessageLimit {
    host: PolicyHost,
    max_bytes: u64,
}

#[async_trait]
impl SubmissionPreparation for LowerMessageLimit {
    async fn prepare(
        &self,
        _: &SessionRuntime,
        _: &PreparedSubmission,
        _: &RunScope,
    ) -> Result<()> {
        *self.host.policy.lock().unwrap() = policy(self.max_bytes);
        Ok(())
    }
}

#[tokio::test]
async fn legacy_image_without_facts_is_decoded_but_false_mime_never_reaches_prompt() {
    for (mime, admitted) in [("image/png", true), ("image/jpeg", false)] {
        let (runtime, binding, host) = setup(Fixture::default(), policy(1024), None, true).await;
        let target = destination(&binding);
        let baseline = ComposerContextBaseline::read(&binding.current_run().unwrap()).unwrap();
        let image_artifact = ComposerArtifact::Attachment {
            id: "legacy-image".into(),
            name: "中文.png".into(),
            content: json!({"type":"image","mediaType":mime,"data":PNG_DATA}),
            facts: None,
        };
        assert!(
            binding
                .patch_context(&baseline, vec![image_artifact], vec![])
                .unwrap()
        );
        if admitted {
            assert_eq!(
                binding
                    .begin_inline_images(&target)
                    .unwrap()
                    .attach(&[image()])
                    .await
                    .unwrap(),
                InlineImageOutcome::Attached,
                "adding to a legacy image draft derives its real bytes instead of requiring missing metadata"
            );
        }
        let ticket = binding
            .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
            .unwrap();
        let outcome = binding.dispatch_prepared_with(ticket, &NoPreparation).await;
        assert_eq!(outcome.is_ok(), admitted);
        assert_eq!(host.prompts.lock().unwrap().len(), usize::from(admitted));
        runtime.stop().await.unwrap();
    }
}

#[tokio::test]
async fn reconnect_during_pre_send_policy_read_rejects_before_any_prompt_and_restores_image() {
    let gate = Arc::new(Semaphore::new(1));
    let (runtime, binding, host) =
        setup(Fixture::default(), policy(1024), Some(gate.clone()), true).await;
    let target = destination(&binding);
    assert_eq!(
        binding
            .begin_inline_images(&target)
            .unwrap()
            .attach(&[image()])
            .await
            .unwrap(),
        InlineImageOutcome::Attached
    );
    let original = binding.current_run().unwrap().draft.clone();
    let ticket = binding
        .prepare_send(&target, tekes_agent_contract::PromptMode::Queue)
        .unwrap();
    let task = {
        let binding = binding.clone();
        tokio::spawn(async move { binding.dispatch_prepared_with(ticket, &NoPreparation).await })
    };
    wait_policy(&host, 2).await;
    runtime.reconnect().await.unwrap();
    runtime.wait_ready(Duration::from_secs(3)).await.unwrap();
    gate.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert!(host.prompts.lock().unwrap().is_empty());
    assert_eq!(
        binding.current_run().unwrap().draft.artifacts,
        original.artifacts
    );
    runtime.stop().await.unwrap();
}
