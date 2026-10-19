//! Host gesture/navigation fences; bytes and read authority are owned by Kit.
use super::*;
use tekes_agent_components_ui::ComposerContextIntent;
use tekes_agent_ui_bindings::FilePreviewRequest;
pub(super) struct FilePreviewGesture {
    request: Arc<FilePreviewRequest>,
    navigation: u64,
    panel_navigation: u64,
    notice: String,
}
impl ComponentDemo {
    pub fn set_file_preview_available(&mut self, available: bool) {
        self.file_preview_available = available;
    }
    pub fn file_preview_current(&self, request: &Arc<FilePreviewRequest>) -> bool {
        self.preview_gesture.as_ref().is_some_and(|g| {
            self.file_preview_available
                && Arc::ptr_eq(&g.request, request)
                && g.navigation == self.navigation_revision
                && g.panel_navigation == self.panel_navigation_revision
                && !self.composing()
                && request.is_current()
        })
    }
    pub(super) fn sync_file_preview(&mut self) {
        if let Some(g) = &self.preview_gesture
            && !self.file_preview_current(&g.request)
        {
            let notice = g.notice.clone();
            self.preview_gesture = None;
            if self.notice_text() == notice {
                self.set_notice("附件预览已取消；草稿保留");
            }
        }
    }
    fn notice_text(&self) -> &str {
        &self
            .tree
            .get(NOTICE)
            .expect("notice")
            .component::<TextContent>()
            .unwrap()
            .text
    }
    pub fn begin_file_preview(
        &mut self,
        binding: Arc<RunBinding>,
        intent: &ComposerContextIntent,
    ) -> Result<Arc<FilePreviewRequest>> {
        require(
            self.file_preview_available
                && self.preview_gesture.is_none()
                && !self.selection_cleared
                && !self.composing(),
            "preview unavailable or pending",
        )?;
        let visible = Arc::ptr_eq(&binding, self.binding())
            || self.active_panel_run().is_some_and(|r| {
                Arc::ptr_eq(&r.binding, &binding) && self.tree.get(r.ids.root).is_some()
            });
        require(visible, "preview source not visible")?;
        let request = Arc::new(binding.begin_file_preview(intent)?);
        let notice = format!("正在读取附件预览：{}", request.name());
        self.set_notice(notice.clone());
        self.preview_gesture = Some(FilePreviewGesture {
            request: request.clone(),
            navigation: self.navigation_revision,
            panel_navigation: self.panel_navigation_revision,
            notice,
        });
        self.sync()?;
        Ok(request)
    }
    /// Call on the UI thread immediately before the host viewer, with no async
    /// hop afterwards. Old navigation/IME/source results never open a window.
    pub fn complete_file_preview(
        &mut self,
        request: &Arc<FilePreviewRequest>,
        error: Option<&str>,
    ) -> Result<bool> {
        if !self.file_preview_current(request) {
            self.sync_file_preview();
            return Ok(false);
        }
        let gesture = self.preview_gesture.take().expect("current preview");
        if self.notice_text() == gesture.notice {
            self.set_notice(match error {
                Some(error) => format!("附件预览失败：{error}"),
                None => "附件已读取，正在打开系统预览".into(),
            });
        }
        self.sync()?;
        Ok(error.is_none())
    }
    pub(super) fn context_affordances(
        &self,
        binding: &Arc<RunBinding>,
    ) -> tekes_agent_components_ui::ComposerContextAffordances {
        tekes_agent_components_ui::ComposerContextAffordances {
            edit_annotations: true,
            preview_file_attachments: self.file_preview_available
                && self.preview_gesture.is_none()
                && binding.supports_file_preview(),
            ..Default::default()
        }
    }
}
