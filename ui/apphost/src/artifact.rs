//! Multi-instance `ArtifactViewerContainer` with `ContainerFactory` (`Multiple`), GPU bitmap
//! `ImageContent` resource upload in `prepare_resources`, UTF-16 `ArtifactSelectionReference`
//! quote capture/resolution, and pluggable host `ArtifactViewerProvider`.

use crate::{
    AgentContainerAdapter, FOCUS_SWITCH_COMMAND, ManagedAgentContainer, SharedAgentWorkbench,
};
use anyhow::Result;
use std::sync::Arc;
use tekes_agent_components_ui::{
    ARTIFACT_SHELL_ID, ArtifactIntent, ArtifactViewKind, ArtifactViewModel, ArtifactViewerSurface,
    ResponsiveWidthMode,
};
use tekes_agent_contract::{
    ArtifactSelectionReference, ArtifactSelectionResolution, Utf16SelectionRange,
};
use tekes_agent_ui_bindings::{
    ComposerQuoteRequest, LoadedArtifact, LoadedArtifactContent, WorkbenchCommand, WorkspaceBinding,
};
use tekes_ui_app_host::{
    ContainerFactory, ContainerMultiplicity, ContainerRenderContext, ContainerToolbarHost,
    ContainerToolbarPosition, LocalContainer,
};
use tekes_ui_app_protocol::ToolbarItem;
use tekes_ui_controls::composition::{Content, ContentActionRecord, ContentHost, ImageLoadOutcome};
use tekes_ui_scene::Size;

/// Host-supplied custom viewer extension (e.g., CAD specs, schematics, waveform inspectors).
pub trait ArtifactViewerProvider: Send + Sync {
    fn supports_path(&self, path: &str) -> bool;
    fn build_custom_content(&self, artifact: &LoadedArtifact) -> Option<(Content, String)>;
}

struct ArtifactViewerAdapter {
    instance_id: String,
    surface: ArtifactViewerSurface,
    provider: Option<Arc<dyn ArtifactViewerProvider>>,
    loaded_once: bool,
}

impl ArtifactViewerAdapter {
    fn new(
        instance_id: impl Into<String>,
        provider: Option<Arc<dyn ArtifactViewerProvider>>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            surface: ArtifactViewerSurface::new(),
            provider,
            loaded_once: false,
        }
    }

    fn resolve_target_path(&mut self, workbench: &SharedAgentWorkbench) -> Option<String> {
        if let Some(path) = workbench.artifact_target(&self.instance_id) {
            return Some(path);
        }
        if !self.loaded_once {
            self.loaded_once = true;
            if let Some(req) = workbench.pop_pending_artifact() {
                workbench.set_artifact_target(&self.instance_id, req.path.clone());
                return Some(req.path);
            }
        }
        workbench
            .workspace_binding()
            .and_then(|ws| ws.selected_file())
    }

    fn build_model(
        &self,
        workbench: &SharedAgentWorkbench,
        ws: &WorkspaceBinding,
        path: Option<&str>,
    ) -> (ArtifactViewModel, Option<Content>) {
        let Some(path) = path else {
            return (
                ArtifactViewModel {
                    workspace_id: ws.workspace_id(),
                    path: String::new(),
                    revision: String::new(),
                    quote_status_badge: None,
                    kind: ArtifactViewKind::Empty,
                },
                None,
            );
        };

        let artifact = ws
            .get_artifact(path)
            .unwrap_or_else(|| ws.load_artifact_blocking(path));

        let quote_badge = workbench.latest_quote().and_then(|q| {
            if q.artifact_id == path {
                let status = ws.resolve_quote_reference(&q);
                Some(match status {
                    ArtifactSelectionResolution::Exact => "引用校验: Exact".to_owned(),
                    ArtifactSelectionResolution::Stale => "引用校验: Stale (文件已修订)".to_owned(),
                    ArtifactSelectionResolution::Unavailable => "引用校验: Unavailable".to_owned(),
                })
            } else {
                None
            }
        });

        if let Some(provider) = &self.provider
            && provider.supports_path(&artifact.path)
            && let Some((custom_content, summary_label)) = provider.build_custom_content(&artifact)
        {
            return (
                ArtifactViewModel {
                    workspace_id: artifact.workspace_id,
                    path: artifact.path,
                    revision: artifact.revision,
                    quote_status_badge: quote_badge,
                    kind: ArtifactViewKind::CustomHandled { summary_label },
                },
                Some(custom_content),
            );
        }

        let kind = match artifact.content {
            LoadedArtifactContent::Loading => ArtifactViewKind::Loading,
            LoadedArtifactContent::CodeText { text, language } => {
                ArtifactViewKind::CodeText { text, language }
            }
            LoadedArtifactContent::Markdown(md) => ArtifactViewKind::Markdown(md),
            LoadedArtifactContent::Image { width, height, .. } => {
                ArtifactViewKind::Image { width, height }
            }
            LoadedArtifactContent::Binary { size } => ArtifactViewKind::Binary { size },
            LoadedArtifactContent::Truncated { preview_text, size } => {
                ArtifactViewKind::Truncated { preview_text, size }
            }
            LoadedArtifactContent::Error(err) => ArtifactViewKind::Error(err),
        };

        (
            ArtifactViewModel {
                workspace_id: artifact.workspace_id,
                path: artifact.path,
                revision: artifact.revision,
                quote_status_badge: quote_badge,
                kind,
            },
            None,
        )
    }
}

impl AgentContainerAdapter for ArtifactViewerAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        let Some(ws) = workbench.workspace_binding() else {
            return;
        };
        let target_path = self.resolve_target_path(workbench);
        let (model, custom) = self.build_model(workbench, &ws, target_path.as_deref());
        self.surface
            .sync_with_custom_body(host, &model, custom, width_mode);
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        let Some(ws) = workbench.workspace_binding() else {
            return;
        };
        let target_path = self.resolve_target_path(workbench);
        let (model, _) = self.build_model(workbench, &ws, target_path.as_deref());

        for intent in self.surface.handle_actions(host, &model, &records) {
            match intent {
                ArtifactIntent::QuoteSelection {
                    path,
                    revision,
                    utf16_start,
                    utf16_len,
                    field_text,
                } => {
                    let digest = ws.get_artifact(&path).map(|a| a.digest).unwrap_or_default();
                    if let Ok(reference) = ArtifactSelectionReference::capture(
                        format!("{}:{path}", ws.workspace_id()),
                        path.clone(),
                        "file".to_owned(),
                        vec!["content".into()],
                        revision,
                        digest,
                        &field_text,
                        Utf16SelectionRange {
                            location: utf16_start,
                            length: utf16_len,
                        },
                        32,
                        32,
                    ) {
                        workbench.set_latest_quote(reference.clone());
                        let req = ComposerQuoteRequest {
                            destination: workbench
                                .run_binding()
                                .and_then(|r| r.current_destination()),
                            reference,
                            comment: format!("引用文件 {path} 选区"),
                        };
                        if let Some(run) = workbench.run_binding() {
                            run.apply_quote(req);
                        } else {
                            workbench.enqueue_command(WorkbenchCommand::QuoteToComposer(req));
                        }
                    }
                }
                ArtifactIntent::AttachFile(path) => {
                    if let Some(run) = workbench.run_binding() {
                        run.attach_file_path(None, path);
                    } else {
                        workbench.enqueue_command(WorkbenchCommand::AttachFileToComposer {
                            destination: None,
                            path,
                        });
                    }
                }
                ArtifactIntent::Reload(path) => {
                    ws.load_artifact_blocking(&path);
                }
            }
        }
    }

    fn prepare_resources(
        &mut self,
        host: &mut ContentHost,
        context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        let renderer_id = context.renderer.resource_identity();
        let revision_key = format!("bitmap-renderer-{renderer_id}");
        if !self
            .surface
            .needs_bitmap_upload(host, renderer_id, &revision_key)
        {
            return Ok(());
        }
        // Check if current artifact in surface header belongs to an image already loaded in host?
        // Look up the target path via static fallback won't happen if no workspace, so query header text path if needed,
        // wait: store last loaded artifact inside adapter directly? Even better: resolve via helper field if set!
        Ok(())
    }
}

/// Separate wrapper storing last rendered `LoadedArtifact` so `prepare_resources` uploads RGBA8
/// directly without needing `SharedAgentWorkbench`!
struct ResourceAwareArtifactAdapter {
    inner: ArtifactViewerAdapter,
    last_image_artifact: Option<LoadedArtifact>,
}

impl ResourceAwareArtifactAdapter {
    fn new(
        instance_id: impl Into<String>,
        provider: Option<Arc<dyn ArtifactViewerProvider>>,
    ) -> Self {
        Self {
            inner: ArtifactViewerAdapter::new(instance_id, provider),
            last_image_artifact: None,
        }
    }
}

impl AgentContainerAdapter for ResourceAwareArtifactAdapter {
    fn sync_from_workbench(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        width_mode: ResponsiveWidthMode,
    ) {
        if let Some(ws) = workbench.workspace_binding() {
            let target_path = self.inner.resolve_target_path(workbench);
            if let Some(p) = &target_path {
                let art = ws
                    .get_artifact(p)
                    .unwrap_or_else(|| ws.load_artifact_blocking(p));
                if matches!(art.content, LoadedArtifactContent::Image { .. }) {
                    self.last_image_artifact = Some(art);
                } else {
                    self.last_image_artifact = None;
                }
            }
            let (model, custom) = self
                .inner
                .build_model(workbench, &ws, target_path.as_deref());
            self.inner
                .surface
                .sync_with_custom_body(host, &model, custom, width_mode);
        }
    }

    fn handle_actions(
        &mut self,
        host: &mut ContentHost,
        workbench: &SharedAgentWorkbench,
        records: Vec<ContentActionRecord>,
    ) {
        self.inner.handle_actions(host, workbench, records);
    }

    fn prepare_resources(
        &mut self,
        host: &mut ContentHost,
        context: &mut ContainerRenderContext<'_>,
    ) -> Result<()> {
        let Some(art) = &self.last_image_artifact else {
            return Ok(());
        };
        let LoadedArtifactContent::Image {
            rgba8_premul,
            width,
            height,
            ..
        } = &art.content
        else {
            return Ok(());
        };
        let renderer_id = context.renderer.resource_identity();
        if !self
            .inner
            .surface
            .needs_bitmap_upload(host, renderer_id, &art.revision)
        {
            return Ok(());
        }
        let lease = context
            .renderer
            .register_bitmap_image(rgba8_premul, *width, *height)?;
        self.inner.surface.complete_bitmap_upload(
            host,
            renderer_id,
            &art.revision,
            ImageLoadOutcome::Ready {
                lease,
                logical_size: Size::new(*width as f32, *height as f32),
            },
        );
        Ok(())
    }
}

pub struct ArtifactViewerContainer;

impl ArtifactViewerContainer {
    pub fn build(
        instance_id: impl Into<String>,
        workbench: SharedAgentWorkbench,
        provider: Option<Arc<dyn ArtifactViewerProvider>>,
    ) -> Result<LocalContainer> {
        let id = instance_id.into();
        let adapter = ResourceAwareArtifactAdapter::new(id.clone(), provider);
        let container = ManagedAgentContainer::build_local(
            id,
            ARTIFACT_SHELL_ID,
            ArtifactViewerSurface::build_content(),
            workbench,
            adapter,
            false,
        )?;
        Ok(container
            .title("产物预览")
            .switcher_icon("tekes-file-code")
            .toolbar_host(ContainerToolbarHost::Container {
                position: ContainerToolbarPosition::Top,
            })
            .toolbar(vec![ToolbarItem {
                id: FOCUS_SWITCH_COMMAND,
                title: "Focus 切换".into(),
                enabled: true,
                geometry_dependent: false,
            }])
            .focus_switch(FOCUS_SWITCH_COMMAND))
    }

    /// Registers a `ContainerMultiplicity::Multiple` factory for dynamic artifact tabs/inspectors.
    /// Callers MUST NOT pre-declare `id` inside `OthersOptions.groups`.
    pub fn factory(
        id: impl Into<String>,
        workbench: SharedAgentWorkbench,
        provider: Option<Arc<dyn ArtifactViewerProvider>>,
    ) -> ContainerFactory {
        let factory_id = id.into();
        ContainerFactory::local(
            factory_id,
            "产物查看",
            ContainerMultiplicity::Multiple,
            move |instance_id| Self::build(instance_id, workbench.clone(), provider.clone()),
        )
        .switcher_icon("tekes-file-code")
    }
}
