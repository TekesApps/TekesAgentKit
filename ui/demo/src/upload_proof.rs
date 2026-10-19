//! Explicit acceptance probe using the real public UI/binding path. No private
//! harness files, credentials, paid providers or system-dialog emulation.
use crate::{
    attachment_files::read_selected_files,
    component_demo::{ComponentDemo, DemoCommand},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tekes_agent_contract::{Result, require};
use tekes_agent_presentation::{
    ComposerDestination, TranscriptItem, TranscriptItemKind, TranscriptSnapshot,
};
use tekes_agent_session::{ConnectionState, SessionRuntime};
use tekes_agent_ui_bindings::{AttachmentOutcome, ModelSelectionPreparation, RunBinding};
use tekes_ui_controls::composition::AccessibilityAction;
const MARKER: &str = "AgentKit attachment UI proof";
const BYTES: &[u8] = b"AgentKit attachment UI proof\n";
struct OwnedFile(PathBuf);
impl OwnedFile {
    fn create() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "agentkit-upload-proof-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&dir).map_err(io_error)?;
        let path = dir.join("中文+a.txt");
        if let Err(error) = std::fs::write(&path, BYTES) {
            let _ = std::fs::remove_dir(&dir);
            return Err(io_error(error));
        }
        Ok(Self(path))
    }
}
impl Drop for OwnedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}
fn io_error(error: std::io::Error) -> tekes_agent_contract::Error {
    tekes_agent_contract::Error::Transport(format!("owned upload proof file: {error}"))
}
fn users(binding: &RunBinding) -> Vec<tekes_agent_presentation::TranscriptItem> {
    binding
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|item| {
            item.kind == TranscriptItemKind::User
                && item.text.contains(MARKER)
                && !item.is_provisional
        })
        .cloned()
        .collect()
}
fn file_only_inputs(
    binding: &RunBinding,
    body_input_id: &str,
) -> Vec<tekes_agent_presentation::TranscriptItem> {
    binding
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|item| file_input(item, body_input_id))
        .cloned()
        .collect()
}
fn file_input(item: &TranscriptItem, body_input_id: &str) -> bool {
    item.kind == TranscriptItemKind::User
        && !item.is_provisional
        && item.id != body_input_id
        && (item.text.contains("中文+a.txt")
            || item
                .attachments
                .iter()
                .any(|a| a.to_string().contains("中文+a.txt")))
}
/// Require a new public journal baseline, not an old Ready UI watch value.
async fn fresh_history(
    runtime: &SessionRuntime,
    binding: &RunBinding,
    old_epoch: u64,
) -> Result<TranscriptSnapshot> {
    let mut changes = runtime.subscribe();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let candidate: Result<Option<TranscriptSnapshot>> = runtime.inspect(|store| {
                if store.connection_epoch == old_epoch || store.connection != ConnectionState::Ready
                {
                    return Ok(None);
                }
                let Some(session) = store
                    .sessions
                    .get(&binding.scope().session_id)
                    .filter(|s| s.journal_ready)
                else {
                    return Ok(None);
                };
                TranscriptSnapshot::read(binding.scope(), session).map(Some)
            });
            if let Some(history) = candidate? {
                return Ok(history);
            }
            changes
                .changed()
                .await
                .map_err(|_| tekes_agent_contract::Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| tekes_agent_contract::Error::Timeout)?
}
async fn send(view: &mut ComponentDemo, expected: &Arc<RunBinding>) -> Result<()> {
    let commands = view.accessibility_action(view.ids().send, None, AccessibilityAction::Press)?;
    let [DemoCommand::Send { binding, ticket }]: [DemoCommand; 1] = commands
        .try_into()
        .map_err(|_| tekes_agent_contract::unsupported("public Send did not capture admission"))?
    else {
        return Err(tekes_agent_contract::unsupported("wrong Send command"));
    };
    require(
        Arc::ptr_eq(&binding, expected),
        "Send changed original binding",
    )?;
    binding
        .dispatch_prepared_with(ticket, &ModelSelectionPreparation)
        .await
}
async fn settled(binding: &RunBinding, previous: usize) -> Result<()> {
    let mut changes = binding.subscribe_run().expect("writable run");
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            if snapshot.can_submit
                && !snapshot.transcript.is_stale
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .filter(|i| i.kind == TranscriptItemKind::Assistant && i.is_final)
                    .count()
                    > previous
            {
                return Ok(());
            }
            changes
                .changed()
                .await
                .map_err(|_| tekes_agent_contract::Error::Disconnected)?;
        }
    })
    .await
    .map_err(|_| tekes_agent_contract::Error::Timeout)?
}
/// Source A is captured before host file IO; B remains independently editable.
/// Receipt reads use public method capability, never image APIs or private paths.
pub async fn run(view: &mut ComponentDemo, runtime: &Arc<SessionRuntime>) -> Result<Value> {
    require(
        view.selected() == 0,
        "upload proof starts at original source",
    )?;
    let a = view.bindings()[0].clone();
    let b = view.bindings()[1].clone();
    let destination = ComposerDestination::run(a.scope().clone());
    let other = ComposerDestination::run(b.scope().clone());
    require(
        a.supports_attachments(),
        "required upload/policy capabilities unavailable",
    )?;
    require(
        a.current_run().unwrap().draft.artifacts.is_empty(),
        "isolated upload draft must start empty",
    )?;
    let previous = a
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|i| i.kind == TranscriptItemKind::Assistant && i.is_final)
        .count();
    view.sync()?;
    let commands =
        view.accessibility_action(view.ids().attachment, None, AccessibilityAction::Press)?;
    let [DemoCommand::SelectFiles { request }]: [DemoCommand; 1] =
        commands.try_into().map_err(|_| {
            tekes_agent_contract::unsupported("public attachment entry did not capture selection")
        })?
    else {
        return Err(tekes_agent_contract::unsupported(
            "wrong attachment command",
        ));
    };
    require(
        view.upload_pending(&a) && !view.upload_pending(&b),
        "upload gate leaked to another source",
    )?;
    // This is a host file provider for offscreen acceptance, not the OS picker.
    let owned = OwnedFile::create()?;
    let files = read_selected_files(std::slice::from_ref(&owned.0))?;
    a.edit_text(&destination, MARKER.into())?;
    view.select(1)?;
    b.edit_text(&other, "Other session upload-proof draft".into())?;
    let report = request.upload(&files).await?;
    require(
        report.outcome == AttachmentOutcome::Attached && report.receipts.len() == 1,
        "upload did not attach exactly once",
    )?;
    require(
        report.receipts[0].file.name == "中文+a.txt"
            && report.receipts[0].file.bytes == BYTES.len() as u64,
        "file metadata mismatch",
    )?;
    require(
        request.upload(&files).await.is_err(),
        "upload capture repeated a mutation",
    )?;
    let receipt = report.receipts[0].clone();
    view.complete_file_selection(&request, files, Ok(Some(report)))?;
    require(
        view.selected() == 1 && b.current_run().unwrap().draft.artifacts.is_empty(),
        "late upload retargeted another session",
    )?;
    require(
        b.current_run().unwrap().draft.text == "Other session upload-proof draft",
        "other draft changed",
    )?;
    view.select(0)?;
    view.sync()?;
    require(
        view.tree
            .semantics()
            .iter()
            .any(|n| n.semantics.name.contains("中文+a.txt")),
        "uploaded card absent",
    )?;
    let can_read = runtime.inspect(|s| {
        s.endpoint_capabilities
            .methods
            .contains("session.fileAttachment")
    });
    require(
        a.supports_file_preview() == can_read,
        "file preview capability drift",
    )?;
    if can_read {
        let baseline = Arc::new(
            tekes_agent_presentation::ComposerContextBaseline::read(&a.current_run().unwrap())
                .unwrap(),
        );
        let intent = tekes_agent_components_ui::ComposerContextIntent {
            item: tekes_agent_components_ui::ComposerContextItem::Artifact(
                baseline.artifacts[0].clone(),
            ),
            baseline,
            action: tekes_agent_components_ui::ComposerContextAction::Open,
        };
        let preview = a.begin_file_preview(&intent)?.read().await?;
        require(
            preview.bytes() == BYTES,
            "public binding file bytes mismatch",
        )?;
        require(
            preview.name() == "中文+a.txt",
            "public binding filename mismatch",
        )?;
        require(runtime.call("session.fileAttachment",json!({"sessionId":b.scope().session_id,"attachmentId":receipt.file.attachment_id}),"ui-upload-read-foreign").await.is_err(),"file read escaped original session")?;
    }
    send(view, &a).await?;
    settled(&a, previous).await?;
    let input = users(&a);
    require(input.len() == 1, "durable input missing or repeated")?;
    let restored_file = !input[0].attachments.is_empty();
    let restored_reference = input[0].text.contains("中文+a.txt");
    require(
        restored_file || restored_reference,
        "history dropped file metadata and normalized reference",
    )?;
    require(
        a.current_run().unwrap().draft.artifacts.is_empty()
            && a.current_run().unwrap().draft.text.is_empty(),
        "Send did not clear original controlled draft",
    )?;
    let input_id = input[0].id.clone();
    // A separate user gesture, not a retry: exercise the shared compiler's
    // attachment-only admission and visible durable history on each harness.
    view.sync()?;
    let dismiss = view
        .tree
        .semantics()
        .iter()
        .find(|n| n.semantics.name == "关闭附件状态")
        .ok_or_else(|| tekes_agent_contract::unsupported("terminal upload status missing"))?
        .id;
    require(
        view.accessibility_action(dismiss, None, AccessibilityAction::Press)?
            .is_empty(),
        "dismiss must not execute transport",
    )?;
    let commands =
        view.accessibility_action(view.ids().attachment, None, AccessibilityAction::Press)?;
    let [DemoCommand::SelectFiles { request }]: [DemoCommand; 1] = commands
        .try_into()
        .map_err(|_| tekes_agent_contract::unsupported("second attachment entry missing"))?
    else {
        return Err(tekes_agent_contract::unsupported(
            "wrong second attachment command",
        ));
    };
    let files = read_selected_files(std::slice::from_ref(&owned.0))?;
    let report = request.upload(&files).await?;
    require(
        report.outcome == AttachmentOutcome::Attached && report.receipts.len() == 1,
        "attachment-only upload failed",
    )?;
    view.complete_file_selection(&request, files, Ok(Some(report)))?;
    require(
        a.current_run().unwrap().draft.text.is_empty()
            && a.current_run().unwrap().draft.artifacts.len() == 1,
        "attachment-only draft changed",
    )?;
    let previous = a
        .current_run()
        .unwrap()
        .transcript
        .items
        .iter()
        .filter(|i| i.kind == TranscriptItemKind::Assistant && i.is_final)
        .count();
    send(view, &a).await?;
    settled(&a, previous).await?;
    let file_only = file_only_inputs(&a, &input_id);
    require(
        file_only.len() == 1 && file_only[0].is_visible && !file_only[0].text.contains(MARKER),
        "attachment-only durable input missing, hidden or duplicated",
    )?;
    require(
        a.current_run().unwrap().draft.text.is_empty()
            && a.current_run().unwrap().draft.artifacts.is_empty(),
        "attachment-only Send did not clear draft",
    )?;
    let file_only_id = file_only[0].id.clone();
    let mut changes = a.subscribe_run().expect("run watch");
    let old_epoch = runtime.inspect(|s| s.connection_epoch);
    let old_revision = a.current_run().unwrap().revision;
    runtime.reconnect().await?;
    runtime.wait_ready(Duration::from_secs(10)).await?;
    let history = fresh_history(runtime, &a, old_epoch).await?;
    require(
        history
            .items
            .iter()
            .filter(|i| i.kind == TranscriptItemKind::User && i.text.contains(MARKER))
            .count()
            == 1
            && history
                .items
                .iter()
                .filter(|i| file_input(i, &input_id))
                .count()
                == 1
            && history.items.iter().any(|i| i.id == input_id)
            && history.items.iter().any(|i| i.id == file_only_id),
        "fresh reconnect journal dropped or duplicated submitted inputs",
    )?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let snapshot = changes.borrow_and_update().clone();
            if snapshot.can_submit
                && snapshot.revision > old_revision
                && !snapshot.transcript.is_stale
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .any(|i| i.id == input_id && !i.is_provisional)
                && snapshot
                    .transcript
                    .items
                    .iter()
                    .any(|i| i.id == file_only_id && !i.is_provisional)
            {
                break;
            }
            changes.changed().await.expect("run publication");
        }
    })
    .await
    .map_err(|_| tekes_agent_contract::Error::Timeout)?;
    require(users(&a).len() == 1, "reconnect duplicated submitted input")?;
    require(
        file_only_inputs(&a, &input_id).len() == 1,
        "reconnect duplicated attachment-only input",
    )?;
    require(
        b.current_run().unwrap().draft.text == "Other session upload-proof draft"
            && b.current_run().unwrap().draft.artifacts.is_empty(),
        "reconnect changed another draft",
    )?;
    view.sync()?;
    Ok(
        json!({"publicEntry":true,"hostBoundedFileRead":true,"receiptValidated":true,"originalDraft":true,"otherDraftPreserved":true,"oneShotUpload":true,"componentSend":true,"promptSettled":true,"durableInputCount":1,"historyFileMetadata":restored_file,"historyFileReference":restored_reference,"publicByteRead":can_read,"foreignSessionReadRejected":can_read.then_some(true),"readUnavailable":!can_read,"attachmentOnlySend":true,"attachmentOnlyVisible":true,"attachmentOnlyInputCount":1,"freshJournalAfterReconnect":true,"reconnectNoDuplicate":true,"nativeInteraction":false,"inlineImage":false}),
    )
}
