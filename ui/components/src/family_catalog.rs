//! Read-only navigation for the children published by a parent's catalog.
//!
//! This component owns presentation identities only. The host remains responsible
//! for validating parent authority and opening the requested child history.
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tekes_agent_contract::SubagentCatalog;
use tekes_agent_presentation::RunScope;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Scene, Size};
use tekes_ui_tokens::Spacing;

static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyCatalogLabels {
    pub name: String,
    pub loading: String,
    pub refreshing: String,
    pub empty: String,
    pub failed: String,
    pub mode_bounded: String,
    pub mode_continuable: String,
    pub mode_one_shot: String,
    pub mode_unknown: String,
    pub parent_unavailable: String,
}

impl Default for FamilyCatalogLabels {
    fn default() -> Self {
        Self {
            name: "关联会话".into(),
            loading: "正在读取关联会话…".into(),
            refreshing: "正在刷新关联会话…".into(),
            empty: "没有关联会话".into(),
            failed: "无法读取关联会话".into(),
            mode_bounded: "有界历史".into(),
            mode_continuable: "可继续".into(),
            mode_one_shot: "单次运行".into(),
            mode_unknown: "能力未知".into(),
            parent_unavailable: "父会话不可用".into(),
        }
    }
}

/// Opaque identity of one catalog adapter owner. Clones retain identity;
/// allocating another key retires captures even when counters and data coincide.
/// This is a display fence, never an endpoint capability or control grant.
#[derive(Clone, Debug, Default)]
pub struct FamilyCatalogSourceKey(Arc<()>);
impl PartialEq for FamilyCatalogSourceKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for FamilyCatalogSourceKey {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyCatalogInput {
    pub source: FamilyCatalogSourceKey,
    pub parent: RunScope,
    pub connection_epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub refreshing: bool,
    pub catalog: Option<SubagentCatalog>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyCatalogIntent {
    pub source: FamilyCatalogSourceKey,
    pub parent: RunScope,
    pub child_id: String,
    pub connection_epoch: u64,
    pub generation: u64,
    pub revision: u64,
    widget_id: u64,
    frame_id: u64,
}

impl FamilyCatalogIntent {
    pub fn belongs_to_widget(&self, widget: &FamilyCatalog) -> bool {
        self.widget_id == widget.widget_id
    }
}

#[derive(Clone, Debug)]
struct ChildControl {
    id: ComponentId,
    entry: tekes_agent_contract::SubagentEntry,
}

/// Shows catalog children with TekesUI's existing stack, text, and button
/// components. It does not create a child, fetch a catalog, or grant send access.
#[derive(Debug)]
pub struct FamilyCatalog {
    root: ComponentId,
    body: StackContent,
    labels: FamilyCatalogLabels,
    widget_id: u64,
    next_id: u64,
    displayed: Option<FamilyCatalogInput>,
    controls: BTreeMap<String, ChildControl>,
    frame_id: u64,
}

impl FamilyCatalog {
    pub fn new(namespace: u32, labels: FamilyCatalogLabels) -> Self {
        let base = (namespace as u64) << 32;
        Self {
            root: ComponentId(base),
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
            labels,
            widget_id: NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed),
            next_id: base + 1,
            displayed: None,
            controls: BTreeMap::new(),
            frame_id: 0,
        }
    }

    pub fn id(&self) -> ComponentId {
        self.root
    }

    pub fn displayed_input(&self) -> Option<&FamilyCatalogInput> {
        self.displayed.as_ref()
    }

    pub fn child_button_id(&self, child_id: &str) -> Option<ComponentId> {
        self.controls.get(child_id).map(|control| control.id)
    }

    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }

    /// Accepts monotonically newer publications. A repeated version must be an
    /// identical replay within one parent. Parent switches retire all controls;
    /// independent parent sources do not share version counters.
    pub fn update(&mut self, input: &FamilyCatalogInput) -> Result<(), &'static str> {
        input
            .parent
            .validate()
            .map_err(|_| "invalid family parent scope")?;
        Self::validate_catalog(input.catalog.as_ref())?;
        if let Some(current) = &self.displayed {
            let old_version = (
                current.connection_epoch,
                current.generation,
                current.revision,
            );
            let new_version = (input.connection_epoch, input.generation, input.revision);
            let same_source = current.source == input.source && current.parent == input.parent;
            if same_source && new_version < old_version {
                return Err("stale family catalog publication");
            }
            if same_source && new_version == old_version {
                return if current == input {
                    Ok(())
                } else {
                    Err("conflicting family catalog publication")
                };
            }
            if !same_source
                || (current.connection_epoch, current.generation)
                    != (input.connection_epoch, input.generation)
            {
                // Retire old controls without reusing their IDs across authority.
                self.controls.clear();
            }
        }

        self.frame_id = self.frame_id.saturating_add(1);
        self.displayed = Some(input.clone());
        self.rebuild(input)?;
        Ok(())
    }

    fn validate_catalog(catalog: Option<&SubagentCatalog>) -> Result<(), &'static str> {
        let Some(catalog) = catalog else {
            return Ok(());
        };
        let mut seen = std::collections::BTreeSet::new();
        for entry in catalog.entries.iter().filter(|entry| entry.kind == "child") {
            if entry.id.trim().is_empty() {
                return Err("family catalog has an empty child ID");
            }
            if !seen.insert(entry.id.as_str()) {
                return Err("family catalog has duplicate child IDs");
            }
        }
        Ok(())
    }

    fn mode_label(&self, mode: Option<&str>) -> (&str, bool) {
        match mode {
            Some("bounded") => (&self.labels.mode_bounded, true),
            Some("continuable") => (&self.labels.mode_continuable, true),
            Some("one-shot") => (&self.labels.mode_one_shot, true),
            _ => (&self.labels.mode_unknown, false),
        }
    }

    fn alloc_id(&mut self) -> Result<ComponentId, &'static str> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("family catalog ID space exhausted")?;
        Ok(ComponentId(id))
    }

    fn rebuild(&mut self, input: &FamilyCatalogInput) -> Result<(), &'static str> {
        let mut children = Vec::new();
        if let Some(error) = &input.error {
            self.controls.clear();
            let message = if error.trim().is_empty() {
                self.labels.failed.clone()
            } else {
                format!("{}: {error}", self.labels.failed)
            };
            children.push(Content::new(self.alloc_id()?, TextContent::new(message)));
        } else {
            if input.refreshing {
                children.push(Content::new(
                    self.alloc_id()?,
                    TextContent::new(&self.labels.refreshing),
                ));
            }
            match input.catalog.as_ref() {
                None => {
                    self.controls.clear();
                    children.push(Content::new(
                        self.alloc_id()?,
                        TextContent::new(if input.refreshing {
                            &self.labels.refreshing
                        } else {
                            &self.labels.loading
                        }),
                    ));
                }
                Some(catalog) => {
                    let entries: Vec<_> = catalog
                        .entries
                        .iter()
                        .filter(|entry| entry.kind == "child")
                        .cloned()
                        .collect();
                    let wanted: std::collections::BTreeSet<_> =
                        entries.iter().map(|entry| entry.id.clone()).collect();
                    self.controls.retain(|id, _| wanted.contains(id));
                    if entries.is_empty() {
                        children.push(Content::new(
                            self.alloc_id()?,
                            TextContent::new(&self.labels.empty),
                        ));
                    } else {
                        for entry in entries {
                            let label = entry
                                .label
                                .as_deref()
                                .filter(|label| !label.trim().is_empty())
                                .unwrap_or(&entry.id);
                            let (mode, known_mode) = self.mode_label(entry.mode.as_deref());
                            let mut title = format!("{label} · {mode}");
                            if !catalog.parent_available {
                                title.push_str(" · ");
                                title.push_str(&self.labels.parent_unavailable);
                            }
                            let id = if let Some(control) = self.controls.get_mut(&entry.id) {
                                control.entry = entry.clone();
                                control.id
                            } else {
                                let id = self.alloc_id()?;
                                self.controls.insert(
                                    entry.id.clone(),
                                    ChildControl {
                                        id,
                                        entry: entry.clone(),
                                    },
                                );
                                id
                            };
                            let mut button = Button::new(title);
                            button.button.state.disabled = !known_mode || input.refreshing;
                            children.push(Content::new(id, button).preserving_focus());
                        }
                    }
                }
            }
        }
        self.body.children = children;
        Ok(())
    }

    /// Converts original SDK button activations into captured navigation intents.
    /// Host code must still validate parent authority and connection ownership.
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<FamilyCatalogIntent> {
        let Some(input) = self.displayed.as_ref() else {
            return Vec::new();
        };
        if input.refreshing || input.error.is_some() {
            return Vec::new();
        }
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| {
                let (child_id, control) = self
                    .controls
                    .iter()
                    .find(|(_, control)| control.id == action.target)?;
                let (_, known_mode) = self.mode_label(control.entry.mode.as_deref());
                if !known_mode
                    || input
                        .catalog
                        .as_ref()
                        .and_then(|catalog| catalog.child(child_id))
                        != Some(&control.entry)
                {
                    return None;
                }
                Some(FamilyCatalogIntent {
                    source: input.source.clone(),
                    parent: input.parent.clone(),
                    child_id: child_id.clone(),
                    connection_epoch: input.connection_epoch,
                    generation: input.generation,
                    revision: input.revision,
                    widget_id: self.widget_id,
                    frame_id: self.frame_id,
                })
            })
            .collect()
    }

    /// Rejects intents captured by another widget, source, or publication.
    pub fn accepts(&self, intent: &FamilyCatalogIntent) -> bool {
        let Some(input) = self.displayed.as_ref() else {
            return false;
        };
        intent.widget_id == self.widget_id
            && intent.frame_id == self.frame_id
            && intent.source == input.source
            && intent.parent == input.parent
            && intent.connection_epoch == input.connection_epoch
            && intent.generation == input.generation
            && intent.revision == input.revision
            && !input.refreshing
            && input.error.is_none()
            && input.catalog.as_ref().is_some_and(|catalog| {
                catalog
                    .child(&intent.child_id)
                    .is_some_and(|entry| self.mode_label(entry.mode.as_deref()).1)
            })
    }
}

impl Component for FamilyCatalog {
    fn children(&self) -> &[Content] {
        self.body.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        self.body.children_mut()
    }

    fn fixed_slots(&self) -> bool {
        self.body.fixed_slots()
    }

    fn validate(&self) -> Result<(), CompositionError> {
        self.body.validate()
    }

    fn child_theme(&self, theme: ContentTheme) -> ContentTheme {
        self.body.child_theme(theme)
    }

    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.body.measure(text, available)
    }

    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip)
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        self.body.paint(scene, text, frame, clip, theme)
    }

    fn event(&mut self, event: &ContentEvent) -> ContentResponse {
        self.body.event(event)
    }

    fn accessibility_action(
        &mut self,
        part: Option<usize>,
        action: &AccessibilityAction,
    ) -> ContentResponse {
        self.body.accessibility_action(part, action)
    }

    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
