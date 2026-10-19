//! Host layout/retention for upload operations. Canonical authority stays in the
//! original binding; statuses do not live in cached/discardable pane bodies.
use super::*;
use tekes_agent_components_ui::{ComposerControlGate, ComposerUploads};
use tekes_agent_ui_bindings::{AttachmentFile, AttachmentReport, AttachmentSelection};
pub(super) struct UploadGesture {
    pub binding: Arc<RunBinding>,
    pub request: Arc<AttachmentSelection>,
    pub root: ComponentId,
    wrapper: ComponentId,
    header: ComponentId,
    source: String,
    retained: Option<Content>,
    mounted: Option<ComponentId>,
    pub files: Vec<AttachmentFile>,
    pub report: Option<AttachmentReport>,
    pub local_error: Option<String>,
}
impl ComponentDemo {
    pub fn set_file_picker_available(&mut self, available: bool) {
        self.file_picker_available = available;
    }
    pub fn upload_pending(&self, binding: &Arc<RunBinding>) -> bool {
        self.uploads
            .values()
            .any(|g| Arc::ptr_eq(&g.binding, binding) && g.request.snapshot().phase.is_pending())
    }
    pub(super) fn has_upload(&self, binding: &Arc<RunBinding>) -> bool {
        self.uploads
            .values()
            .any(|g| Arc::ptr_eq(&g.binding, binding))
    }
    pub(super) fn attachment_affordance(&self, binding: &Arc<RunBinding>) -> bool {
        let reading_child = !Arc::ptr_eq(binding, self.binding())
            && matches!(
                self.panel_selections.get(self.binding().scope()),
                Some(SidePanelTabKind::Subagent { .. })
            )
            && !self.panel_child_controls_ready();
        self.file_picker_available
            && !reading_child
            && binding.supports_attachments()
            && !self.has_upload(binding)
    }
    fn active_upload_host(&self, binding: &Arc<RunBinding>) -> Option<ComponentId> {
        if self.selection_cleared {
            return None;
        }
        if Arc::ptr_eq(binding, self.binding()) {
            return Some(MAIN_UPLOAD_HOST);
        }
        self.active_panel_run()
            .filter(|run| Arc::ptr_eq(&run.binding, binding))
            .filter(|run| self.tree.get(run.ids.root).is_some())
            .map(|_| PANEL_UPLOAD_HOST)
    }
    fn upload_source(&self, binding: &Arc<RunBinding>) -> String {
        if Arc::ptr_eq(binding, self.binding()) {
            return self
                .sidebar_source
                .as_ref()
                .and_then(|source| {
                    source
                        .rows
                        .iter()
                        .find(|row| {
                            row.address.endpoint_id == binding.scope().endpoint_id
                                && row.address.session_id == binding.scope().session_id
                        })
                        .map(|row| row.title.clone())
                })
                .unwrap_or_else(|| match self.selected {
                    0 => "会话 A".into(),
                    1 => "会话 B".into(),
                    _ => "当前会话".into(),
                });
        }
        let parent = self.binding().scope();
        match self.panel_selections.get(parent) {
            Some(SidePanelTabKind::SideChat { session_id }) => self
                .side_chats
                .as_ref()
                .and_then(|s| s.snapshot().chats.get(session_id).map(|c| c.title.clone()))
                .unwrap_or_else(|| "SideChat".into()),
            Some(SidePanelTabKind::Task { task_id }) => self
                .task_sources
                .get(&(parent.clone(), task_id.clone()))
                .and_then(|s| s.source.snapshot().value.map(|t| t.name))
                .unwrap_or_else(|| "任务".into()),
            Some(SidePanelTabKind::Subagent { child_id }) => self
                .child_panes
                .get(&(parent.clone(), child_id.clone()))
                .map(|p| p.title.clone())
                .unwrap_or_else(|| "子代理".into()),
            _ => "原会话".into(),
        }
    }
    pub fn begin_file_selection(
        &mut self,
        binding: Arc<RunBinding>,
        destination: ComposerDestination,
    ) -> Result<Arc<AttachmentSelection>> {
        require(
            self.attachment_affordance(&binding)
                && !self.composing()
                && self.active_upload_host(&binding).is_some()
                && !self.has_upload(&binding),
            "file selection unavailable or unresolved",
        )?;
        let request = Arc::new(binding.begin_attachments(&destination)?);
        let namespace = self.next_upload_namespace;
        self.next_upload_namespace = namespace
            .checked_add(2)
            .filter(|n| *n < 3_000_000)
            .ok_or_else(|| unsupported("upload namespaces exhausted"))?;
        let mut status = ComposerUploads::new(namespace, Default::default());
        status.update(request.snapshot()).map_err(unsupported)?;
        let root = status.id();
        let wrapper = ComponentId(((namespace + 1) as u64) << 32);
        let header = ComponentId(wrapper.0 + 1);
        let source = self.upload_source(&binding);
        let retained = Content::new(
            wrapper,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![
                    Content::new(header, TextContent::new(format!("附件 · {source}"))),
                    Content::new(root, status),
                ],
            },
        );
        self.uploads.insert(
            request.request_id().into(),
            UploadGesture {
                binding,
                request: request.clone(),
                root,
                wrapper,
                header,
                source,
                retained: Some(retained),
                mounted: None,
                files: vec![],
                report: None,
                local_error: None,
            },
        );
        if let Err(error) = self.sync() {
            request.fail_selection(error.to_string());
            return Err(error);
        }
        Ok(request)
    }
    /// Original request authority, never the current tab or a same-address replacement.
    pub fn complete_file_selection(
        &mut self,
        request: &Arc<AttachmentSelection>,
        files: Vec<AttachmentFile>,
        result: std::result::Result<Option<AttachmentReport>, String>,
    ) -> Result<bool> {
        let Some(g) = self
            .uploads
            .get_mut(request.request_id())
            .filter(|g| Arc::ptr_eq(&g.request, request))
        else {
            return Ok(false);
        };
        require(
            !request.snapshot().phase.is_pending(),
            "upload completion arrived before terminal facts",
        )?;
        if let Ok(Some(report)) = &result {
            require(
                report.request_id == request.request_id()
                    && &report.destination == request.destination(),
                "upload report source mismatch",
            )?;
        }
        g.files = files;
        match result {
            Ok(report) => g.report = report,
            Err(error) => g.local_error = Some(error),
        };
        self.sync()?;
        Ok(true)
    }
    /// Address-only lookup is intentionally ambiguous across distinct authorities.
    pub fn upload_report(&self, scope: &RunScope) -> Option<&AttachmentReport> {
        let mut found = self.uploads.values().filter(|g| g.binding.scope() == scope);
        let first = found.next()?;
        if found.next().is_some() {
            return None;
        }
        first.report.as_ref()
    }
    pub fn upload_report_for_request(&self, request_id: &str) -> Option<&AttachmentReport> {
        self.uploads.get(request_id)?.report.as_ref()
    }
    pub(super) fn sync_upload_status(&mut self) -> Result<()> {
        let composing = self.composing();
        let plan: Vec<_> = self
            .uploads
            .iter()
            .map(|(id, g)| {
                let retired = g.binding.access() == PaneAccess::Missing
                    || g.binding.current_run().is_some_and(|s| {
                        s.run.as_ref().is_some_and(|r| {
                            r.phase == tekes_agent_presentation::RunPhase::Archived
                        })
                    });
                let host = self
                    .active_upload_host(&g.binding)
                    .or_else(|| retired.then_some(MAIN_UPLOAD_HOST));
                (id.clone(), host, retired)
            })
            .collect();
        for (id, host, retired) in plan {
            let g = self.uploads.get_mut(&id).expect("planned operation");
            if host != g.mounted {
                if g.mounted.take().is_some() {
                    g.retained = Some(
                        self.tree
                            .remove(g.wrapper)
                            .map_err(|_| unsupported("retain upload status"))?,
                    );
                }
                if let Some(parent) = host {
                    self.tree
                        .insert(parent, g.retained.take().expect("retained upload status"))
                        .map_err(|_| unsupported("mount scoped upload status"))?;
                    g.mounted = Some(parent);
                }
            }
            let node = if g.mounted.is_some() {
                self.tree.get_mut(g.wrapper).expect("mounted status")
            } else {
                g.retained.as_mut().expect("retained status")
            };
            let body = node
                .component_mut::<StackContent>()
                .expect("host upload group");
            body.children
                .iter_mut()
                .find(|n| n.id() == g.header)
                .unwrap()
                .component_mut::<TextContent>()
                .unwrap()
                .text = format!(
                "附件 · {}{}",
                g.source,
                if retired {
                    "（原来源已关闭，未转交其他草稿）"
                } else {
                    ""
                }
            );
            let status = body
                .children
                .iter_mut()
                .find(|n| n.id() == g.root)
                .unwrap()
                .component_mut::<ComposerUploads>()
                .unwrap();
            status.set_interactions_enabled(!composing);
            status.update(g.request.snapshot()).map_err(unsupported)?;
        }
        Ok(())
    }
    pub(super) fn upload_gate(&self, binding: &Arc<RunBinding>) -> ComposerControlGate {
        let mut gate = binding.access().composer_gate();
        if self.upload_pending(binding) {
            gate.send = false;
            gate.entries = false;
        }
        gate
    }
    pub(super) fn drain_uploads(&mut self, actions: &[ContentAction]) -> Result<()> {
        if self.composing() {
            return Ok(());
        }
        let mut closed = vec![];
        for (id, g) in &mut self.uploads {
            if g.mounted.is_none() {
                continue;
            }
            let intents = self
                .tree
                .get_mut(g.root)
                .expect("mounted upload status")
                .component_mut::<ComposerUploads>()
                .unwrap()
                .take_intents(actions);
            if !intents.is_empty() {
                closed.push(id.clone());
            }
        }
        for id in closed {
            let g = self.uploads.remove(&id).unwrap();
            self.tree
                .remove(g.wrapper)
                .map_err(|_| unsupported("dismiss upload status"))?;
        }
        Ok(())
    }
}
