//! Demo host composition; canonical upload/draft behavior stays in the binding.
use super::*;
use tekes_agent_components_ui::{ComposerControlGate, ComposerUploads};
use tekes_agent_ui_bindings::{AttachmentFile, AttachmentReport, AttachmentSelection};
pub(super) struct UploadGesture {
    pub binding: Arc<RunBinding>,
    pub request: Arc<AttachmentSelection>,
    pub root: ComponentId,
    pub files: Vec<AttachmentFile>,
    pub report: Option<AttachmentReport>,
    pub local_error: Option<String>,
}
impl ComponentDemo {
    pub fn set_file_picker_available(&mut self, available: bool) {
        self.file_picker_available = available;
    }
    pub fn upload_pending(&self, binding: &Arc<RunBinding>) -> bool {
        self.uploads.get(binding.scope()).is_some_and(|g| {
            Arc::ptr_eq(&g.binding, binding) && g.request.snapshot().phase.is_pending()
        })
    }
    pub fn begin_file_selection(
        &mut self,
        binding: Arc<RunBinding>,
        destination: ComposerDestination,
    ) -> Result<Arc<AttachmentSelection>> {
        require(
            self.file_picker_available
                && !self.composing()
                && Arc::ptr_eq(&binding, self.binding())
                && !self.uploads.contains_key(binding.scope()),
            "file selection unavailable or unresolved",
        )?;
        let request = Arc::new(binding.begin_attachments(&destination)?);
        let namespace = self.next_upload_namespace;
        self.next_upload_namespace = namespace
            .checked_add(1)
            .filter(|n| *n < 3_000_000)
            .ok_or_else(|| unsupported("upload namespaces exhausted"))?;
        let mut status = ComposerUploads::new(namespace, Default::default());
        status.update(request.snapshot()).map_err(unsupported)?;
        let root = status.id();
        self.tree
            .insert(Self::pair_id(self.selected), Content::new(root, status))
            .map_err(|_| unsupported("mount upload status"))?;
        self.uploads.insert(
            binding.scope().clone(),
            UploadGesture {
                binding,
                request: request.clone(),
                root,
                files: vec![],
                report: None,
                local_error: None,
            },
        );
        self.sync()?;
        Ok(request)
    }
    /// Called with the original request even when another run is now visible.
    /// Keep byte buffers and receipts for explicit host reconciliation/dismissal.
    pub fn complete_file_selection(
        &mut self,
        request: &Arc<AttachmentSelection>,
        files: Vec<AttachmentFile>,
        result: std::result::Result<Option<AttachmentReport>, String>,
    ) -> Result<bool> {
        let Some(g) = self
            .uploads
            .values_mut()
            .find(|g| Arc::ptr_eq(&g.request, request))
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
            Err(error) => {
                g.local_error = Some(error);
            }
        }
        self.sync()?;
        Ok(true)
    }
    pub fn upload_report(&self, scope: &RunScope) -> Option<&AttachmentReport> {
        self.uploads.get(scope)?.report.as_ref()
    }
    pub(super) fn sync_upload_status(&mut self) -> Result<()> {
        if self.selection_cleared {
            return Ok(());
        }
        let binding = self.binding().clone();
        let Some(g) = self
            .uploads
            .get(binding.scope())
            .filter(|g| Arc::ptr_eq(&g.binding, &binding))
        else {
            return Ok(());
        };
        let composing = self.composing();
        if let Some(node) = self.tree.get_mut(g.root) {
            let status = node
                .component_mut::<ComposerUploads>()
                .expect("upload status");
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
        if self.selection_cleared || self.composing() {
            return Ok(());
        }
        let binding = self.binding().clone();
        let Some(g) = self
            .uploads
            .get(binding.scope())
            .filter(|g| Arc::ptr_eq(&g.binding, &binding))
        else {
            return Ok(());
        };
        let root = g.root;
        let Some(node) = self.tree.get_mut(root) else {
            return Ok(());
        };
        let intents = node
            .component_mut::<ComposerUploads>()
            .unwrap()
            .take_intents(actions);
        if !intents.is_empty() {
            self.tree
                .remove(root)
                .map_err(|_| unsupported("unmount upload status"))?;
            self.uploads.remove(binding.scope());
        }
        Ok(())
    }
}
