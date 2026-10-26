//! Attachment operation status over generic controls; no transport or file IO.
use std::sync::Arc;
use tekes_agent_presentation::{AttachmentTransferPhase, AttachmentTransferSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;
#[derive(Clone, Debug)]
pub struct ComposerUploadLabels {
    pub group: String,
    pub selecting: String,
    pub uploading: String,
    pub attached: String,
    pub detached: String,
    pub failed: String,
    pub uncertain: String,
    pub cancelled: String,
    pub dismiss: String,
}
impl Default for ComposerUploadLabels {
    fn default() -> Self {
        Self {
            group: "附件上传".into(),
            selecting: "正在选择或检查文件".into(),
            uploading: "正在上传".into(),
            attached: "附件已加入原会话草稿".into(),
            detached: "草稿或来源已变化，附件未加入草稿".into(),
            failed: "附件未加入草稿".into(),
            uncertain: "上传结果可能未知，需显式对账；不会自动重传".into(),
            cancelled: "已取消文件选择".into(),
            dismiss: "关闭附件状态".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerUploadDismiss {
    pub snapshot: Arc<AttachmentTransferSnapshot>,
}
/// One instance belongs to one request authority. A new request requires a new
/// component and unused namespace; old actions cannot dismiss a later upload.
#[derive(Debug)]
pub struct ComposerUploads {
    root: ComponentId,
    dismiss: ComponentId,
    body: StackContent,
    labels: ComposerUploadLabels,
    snapshot: Option<Arc<AttachmentTransferSnapshot>>,
    interactions: bool,
    dismissed: bool,
}
impl ComposerUploads {
    pub fn new(namespace: u32, labels: ComposerUploadLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            root,
            dismiss: ComponentId(root.0 + 1),
            labels,
            snapshot: None,
            interactions: true,
            dismissed: false,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn dismiss_id(&self) -> ComponentId {
        self.dismiss
    }
    pub fn update(
        &mut self,
        snapshot: Arc<AttachmentTransferSnapshot>,
    ) -> Result<bool, &'static str> {
        if snapshot.revision == 0 || snapshot.request_id.is_empty() {
            return Err("invalid upload snapshot");
        }
        snapshot
            .destination
            .validate()
            .map_err(|_| "invalid upload destination")?;
        if let AttachmentTransferPhase::Uploading { index, total } = snapshot.phase
            && (index == 0 || index > total)
        {
            return Err("invalid upload progress");
        }
        if let Some(old) = &self.snapshot {
            if old.request_id != snapshot.request_id
                || old.destination != snapshot.destination
                || snapshot.revision < old.revision
            {
                return Ok(false);
            }
            if snapshot.revision == old.revision {
                return if **old == *snapshot {
                    Ok(false)
                } else {
                    Err("conflicting upload revision")
                };
            }
            if !old.phase.is_pending() {
                return Ok(false);
            }
        }
        self.snapshot = Some(snapshot);
        self.rebuild();
        Ok(true)
    }
    pub fn set_interactions_enabled(&mut self, enabled: bool) {
        self.interactions = enabled;
        self.rebuild();
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<ComposerUploadDismiss> {
        let Some(snapshot) = &self.snapshot else {
            return vec![];
        };
        if self.dismissed || !self.interactions || snapshot.phase.is_pending() {
            return vec![];
        }
        if actions
            .iter()
            .any(|a| a.kind == ContentActionKind::Activate && a.target == self.dismiss)
        {
            self.dismissed = true;
            let intent = ComposerUploadDismiss {
                snapshot: snapshot.clone(),
            };
            self.rebuild();
            vec![intent]
        } else {
            vec![]
        }
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    fn rebuild(&mut self) {
        self.body.children.clear();
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let text = match &snapshot.phase {
            AttachmentTransferPhase::Selecting => self.labels.selecting.clone(),
            AttachmentTransferPhase::Uploading { index, total } => {
                format!("{} {index}/{total}", self.labels.uploading)
            }
            AttachmentTransferPhase::Attached { count } => {
                format!("{} · {count}", self.labels.attached)
            }
            AttachmentTransferPhase::Detached { completed } => {
                format!("{} · 已返回 {completed} 个回执", self.labels.detached)
            }
            AttachmentTransferPhase::Failed {
                completed,
                uncertain,
                message,
            } => format!(
                "{} · 已返回 {completed} 个回执 · {message}{}",
                self.labels.failed,
                if *uncertain {
                    format!(" · {}", self.labels.uncertain)
                } else {
                    String::new()
                }
            ),
            AttachmentTransferPhase::Cancelled => self.labels.cancelled.clone(),
        };
        self.body.children.push(Content::new(
            ComponentId(self.root.0 + 2),
            TextContent::new(text),
        ));
        let mut dismiss = ButtonContent::new(&self.labels.dismiss);
        dismiss.button.state.disabled =
            snapshot.phase.is_pending() || !self.interactions || self.dismissed;
        self.body.children.push(Content::new(self.dismiss, dismiss));
    }
}
impl Component for ComposerUploads {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip)
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
