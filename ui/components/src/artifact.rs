//! Artifact Viewer surface supporting Code/Text with UTF-16 selection capture for
//! `ArtifactSelectionReference`, Markdown (`RichContent`), Bitmap (`ImageContent` + Metal lease),
//! Loading/Binary/Truncated/Error diagnostics, and pluggable host `Custom` Content providers.

use crate::ResponsiveWidthMode;
use crate::ids::*;
use std::ops::Range;
use tekes_ui_controls::{
    ButtonVariant, ScrollArea,
    composition::{
        Button, Content, ContentActionKind, ContentActionRecord, ContentHost, ImageContent,
        ImageLoadOutcome, ImageLoadRequest, ImageLoadStatus, ImageSemantics, RichContent,
        RichDocument, ScrollContent, StackContent, TextContent, TextFieldContent, TextViewport,
    },
    extended::{Axis, Stack},
};
use tekes_ui_scene::Size;
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactViewKind {
    Empty,
    Loading,
    CodeText { text: String, language: String },
    Markdown(String),
    Image { width: u32, height: u32 },
    Binary { size: u64 },
    Truncated { preview_text: String, size: u64 },
    Error(String),
    CustomHandled { summary_label: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactViewModel {
    pub workspace_id: String,
    pub path: String,
    pub revision: String,
    pub quote_status_badge: Option<String>,
    pub kind: ArtifactViewKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactIntent {
    QuoteSelection {
        path: String,
        revision: String,
        utf16_start: u64,
        utf16_len: u64,
        field_text: String,
    },
    AttachFile(String),
    Reload(String),
}

pub struct ArtifactViewerSurface {
    last_render_sig: String,
    pending_image_request: Option<ImageLoadRequest>,
    last_image_identity: Option<(String, u64)>,
}

impl std::fmt::Debug for ArtifactViewerSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactViewerSurface")
            .field("last_render_sig", &self.last_render_sig)
            .finish_non_exhaustive()
    }
}

impl Default for ArtifactViewerSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactViewerSurface {
    pub fn new() -> Self {
        Self {
            last_render_sig: String::new(),
            pending_image_request: None,
            last_image_identity: None,
        }
    }

    pub fn build_content() -> Content {
        let mut quote_btn = Button::new("引用选区到对话");
        quote_btn.variant = ButtonVariant::Primary;
        let attach_btn = Button::new("附加文件");
        let mut reload_btn = Button::new("重新加载");
        reload_btn.variant = ButtonVariant::Quiet;

        let actions = Content::new(
            ARTIFACT_ACTIONS_ROW_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(ARTIFACT_QUOTE_BTN_ID, quote_btn),
                    Content::new(ARTIFACT_ATTACH_BTN_ID, attach_btn),
                    Content::new(ARTIFACT_RELOAD_BTN_ID, reload_btn),
                ],
            },
        );

        let body_slot = Content::new(
            ARTIFACT_BODY_SLOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(ARTIFACT_BODY_SCROLL_ID),
                children: vec![Content::new(
                    ARTIFACT_DIAG_TEXT_ID,
                    TextContent::new(
                        "从 Workspace Files 选择文件或在 Summary 中点击产物打开预览。",
                    ),
                )],
            },
        );

        Content::new(
            ARTIFACT_ROOT_ID,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: Some(ARTIFACT_BODY_SLOT_ID),
                children: vec![
                    Content::new(ARTIFACT_HEADER_ID, TextContent::new("产物查看器")),
                    actions,
                    body_slot,
                ],
            },
        )
    }

    pub fn sync_with_custom_body(
        &mut self,
        host: &mut ContentHost,
        model: &ArtifactViewModel,
        custom_body: Option<Content>,
        _width_mode: ResponsiveWidthMode,
    ) {
        if let Some(header) = host
            .get_mut(ARTIFACT_HEADER_ID)
            .and_then(|c| c.component_mut::<TextContent>())
        {
            let badge = model
                .quote_status_badge
                .as_deref()
                .map(|b| format!(" · {b}"))
                .unwrap_or_default();
            if model.path.is_empty() {
                header.text = "产物查看器 · 未打开文件".to_owned();
            } else {
                header.text = format!(
                    "产物 · {} ({} · {}){badge}",
                    model.path, model.revision, model.workspace_id
                );
            }
        }

        let sig = match &model.kind {
            ArtifactViewKind::Empty => "empty".to_owned(),
            ArtifactViewKind::Loading => format!("loading:{}", model.path),
            ArtifactViewKind::CodeText { text, language } => {
                format!(
                    "code:{}:{}:{language}:{}",
                    model.path,
                    model.revision,
                    text.len()
                )
            }
            ArtifactViewKind::Markdown(md) => {
                format!("md:{}:{}:{}", model.path, model.revision, md.len())
            }
            ArtifactViewKind::Image { width, height } => {
                format!("img:{}:{}:{width}x{height}", model.path, model.revision)
            }
            ArtifactViewKind::Binary { size } => {
                format!("bin:{}:{}:{size}", model.path, model.revision)
            }
            ArtifactViewKind::Truncated { size, preview_text } => {
                format!(
                    "trunc:{}:{}:{size}:{}",
                    model.path,
                    model.revision,
                    preview_text.len()
                )
            }
            ArtifactViewKind::Error(err) => format!("err:{}:{err}", model.path),
            ArtifactViewKind::CustomHandled { summary_label } => {
                format!("custom:{}:{}:{summary_label}", model.path, model.revision)
            }
        };

        if sig != self.last_render_sig {
            self.last_render_sig = sig;
            self.pending_image_request = None;
            self.last_image_identity = None;

            if let Some(custom) = custom_body {
                let scroll = Content::new(
                    ARTIFACT_BODY_SCROLL_ID,
                    ScrollContent::new(ScrollArea::default(), custom),
                );
                replace_stack_children(host, ARTIFACT_BODY_SLOT_ID, vec![scroll]);
            } else {
                let body_children = match &model.kind {
                    ArtifactViewKind::Empty => vec![Content::new(
                        ARTIFACT_DIAG_TEXT_ID,
                        TextContent::new("从 Workspace Files 或 Summary 打开文件以预览内容。"),
                    )],
                    ArtifactViewKind::Loading => vec![Content::new(
                        ARTIFACT_DIAG_TEXT_ID,
                        TextContent::new(format!("正在加载 {} …", model.path)),
                    )],
                    ArtifactViewKind::CodeText { text, .. } => {
                        let mut field = TextFieldContent::read_only_content(
                            "源码预览",
                            text,
                            tekes_ui_tokens::proposal::TextRole::Code,
                        );
                        field.show_metadata = false;
                        field.viewport = TextViewport::Fill;
                        vec![Content::new(
                            ARTIFACT_BODY_SCROLL_ID,
                            ScrollContent::new(
                                ScrollArea::default(),
                                Content::new(ARTIFACT_CODE_EDITOR_ID, field),
                            ),
                        )]
                    }
                    ArtifactViewKind::Markdown(md) => {
                        let doc = RichDocument::from_markdown(md);
                        let rich = RichContent::new(format!("artifact-md-{}", model.path), 1, doc)
                            .expect("valid artifact markdown RichContent");
                        vec![Content::new(
                            ARTIFACT_BODY_SCROLL_ID,
                            ScrollContent::new(
                                ScrollArea::default(),
                                Content::new(ARTIFACT_MARKDOWN_RICH_ID, rich),
                            ),
                        )]
                    }
                    ArtifactViewKind::Image { width, height } => {
                        let w = (*width).max(1) as f32;
                        let h = (*height).max(1) as f32;
                        let mut img = ImageContent::loading(
                            Size::new(w, h),
                            ImageSemantics::Alternative(format!("图像预览: {}", model.path)),
                        )
                        .expect("valid ImageContent size");
                        self.pending_image_request = img.begin_load().ok();
                        vec![Content::new(
                            ARTIFACT_BODY_SCROLL_ID,
                            ScrollContent::new(
                                ScrollArea::default(),
                                Content::new(ARTIFACT_IMAGE_ID, img),
                            ),
                        )]
                    }
                    ArtifactViewKind::Binary { size } => vec![Content::new(
                        ARTIFACT_DIAG_TEXT_ID,
                        TextContent::new(format!(
                            "二进制文件（不可文本预览）\n路径: {}\n大小: {} bytes",
                            model.path, size
                        )),
                    )],
                    ArtifactViewKind::Truncated { preview_text, size } => vec![Content::new(
                        ARTIFACT_DIAG_TEXT_ID,
                        TextContent::new(format!(
                            "[文件已截断预览 · 总大小 {} bytes]\n\n{}",
                            size, preview_text
                        )),
                    )],
                    ArtifactViewKind::Error(err) => vec![Content::new(
                        ARTIFACT_DIAG_TEXT_ID,
                        TextContent::new(format!("加载产物失败：{err}")),
                    )],
                    ArtifactViewKind::CustomHandled { summary_label } => {
                        vec![Content::new(
                            ARTIFACT_DIAG_TEXT_ID,
                            TextContent::new(summary_label.clone()),
                        )]
                    }
                };
                replace_stack_children(host, ARTIFACT_BODY_SLOT_ID, body_children);
            }
        }

        let quotable = matches!(
            model.kind,
            ArtifactViewKind::CodeText { .. }
                | ArtifactViewKind::Markdown(_)
                | ArtifactViewKind::CustomHandled { .. }
        ) && !model.path.is_empty();
        if let Some(btn) = host
            .get_mut(ARTIFACT_QUOTE_BTN_ID)
            .and_then(|c| c.component_mut::<Button>())
        {
            btn.button.state.disabled = !quotable;
        }
        for id in [ARTIFACT_ATTACH_BTN_ID, ARTIFACT_RELOAD_BTN_ID] {
            if let Some(btn) = host.get_mut(id).and_then(|c| c.component_mut::<Button>()) {
                btn.button.state.disabled = model.path.is_empty();
            }
        }
    }

    pub fn needs_bitmap_upload(
        &self,
        host: &ContentHost,
        renderer_id: u64,
        revision: &str,
    ) -> bool {
        let Some(img) = host
            .get(ARTIFACT_IMAGE_ID)
            .and_then(|c| c.component::<ImageContent>())
        else {
            return false;
        };
        img.status() == ImageLoadStatus::Loading
            || self.last_image_identity.as_ref() != Some(&(revision.to_owned(), renderer_id))
    }

    pub fn complete_bitmap_upload(
        &mut self,
        host: &mut ContentHost,
        renderer_id: u64,
        revision: &str,
        outcome: ImageLoadOutcome,
    ) -> bool {
        let Some(img) = host
            .get_mut(ARTIFACT_IMAGE_ID)
            .and_then(|c| c.component_mut::<ImageContent>())
        else {
            return false;
        };
        let req = match self.pending_image_request.take() {
            Some(r) => r,
            None => match img.begin_load() {
                Ok(r) => r,
                Err(_) => return false,
            },
        };
        if img.complete(req, outcome).unwrap_or(false) {
            self.last_image_identity = Some((revision.to_owned(), renderer_id));
            true
        } else {
            false
        }
    }

    pub fn current_utf16_selection(host: &ContentHost) -> Option<Range<usize>> {
        host.get(ARTIFACT_CODE_EDITOR_ID)
            .and_then(|c| c.component::<TextFieldContent>())
            .map(|f| f.surface.input().selection())
    }

    pub fn handle_actions(
        &self,
        host: &ContentHost,
        model: &ArtifactViewModel,
        records: &[ContentActionRecord],
    ) -> Vec<ArtifactIntent> {
        let mut out = Vec::new();
        for record in records {
            if !matches!(record.action.kind, ContentActionKind::Activate) {
                continue;
            }
            match record.action.target {
                id if id == ARTIFACT_QUOTE_BTN_ID && !model.path.is_empty() => {
                    let full_text = match &model.kind {
                        ArtifactViewKind::CodeText { text, .. } => Some(text.clone()),
                        ArtifactViewKind::Markdown(md) => Some(md.clone()),
                        ArtifactViewKind::CustomHandled { summary_label } => {
                            Some(summary_label.clone())
                        }
                        _ => None,
                    };
                    if let Some(field_text) = full_text {
                        let total_utf16 = field_text.encode_utf16().count();
                        let sel = Self::current_utf16_selection(host)
                            .filter(|r| !r.is_empty() && r.end <= total_utf16)
                            .unwrap_or_else(|| 0..total_utf16.min(120));
                        if !sel.is_empty() {
                            out.push(ArtifactIntent::QuoteSelection {
                                path: model.path.clone(),
                                revision: model.revision.clone(),
                                utf16_start: sel.start as u64,
                                utf16_len: (sel.end - sel.start) as u64,
                                field_text,
                            });
                        }
                    }
                }
                id if id == ARTIFACT_ATTACH_BTN_ID && !model.path.is_empty() => {
                    out.push(ArtifactIntent::AttachFile(model.path.clone()));
                }
                id if id == ARTIFACT_RELOAD_BTN_ID && !model.path.is_empty() => {
                    out.push(ArtifactIntent::Reload(model.path.clone()));
                }
                _ => {}
            }
        }
        out
    }
}
