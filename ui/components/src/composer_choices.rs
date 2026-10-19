//! Agent model/preset content over public controls. The host supplies the catalog
//! and mounts this content in its chosen public panel/popover/layout.
use std::collections::{BTreeMap, BTreeSet};
use tekes_agent_presentation::ComposerDestination;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerChoiceKind {
    Model,
    Preset,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerChoiceRequest {
    pub generation: u64,
    pub destination: ComposerDestination,
    pub kind: ComposerChoiceKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerChoice {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerCatalogPhase {
    Loading,
    Ready,
    Failed(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerChoiceCatalog {
    pub revision: u64,
    pub phase: ComposerCatalogPhase,
    pub choices: Vec<ComposerChoice>,
    pub selected: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerChoiceSelection {
    pub request: ComposerChoiceRequest,
    pub catalog_revision: u64,
    pub choice: ComposerChoice,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerChoiceIntent {
    Select(Box<ComposerChoiceSelection>),
    Dismiss(ComposerChoiceRequest),
}
#[derive(Clone, Debug)]
pub struct ComposerChoiceLabels {
    pub group: String,
    pub model: String,
    pub preset: String,
    pub loading: String,
    pub empty: String,
    pub pending: String,
    pub dismiss: String,
    pub selected: String,
    pub current: String,
}
impl Default for ComposerChoiceLabels {
    fn default() -> Self {
        Self {
            group: "Composer 选项".into(),
            model: "选择模型".into(),
            preset: "选择预设".into(),
            loading: "正在读取选项".into(),
            empty: "没有可用选项".into(),
            pending: "正在应用选择".into(),
            dismiss: "关闭".into(),
            selected: "已选择".into(),
            current: "当前选择".into(),
        }
    }
}
/// Catalog facts and mutation authority stay outside this component. Only the
/// open gesture and pending click are local. Drain actions before every update.
#[derive(Debug)]
pub struct ComposerChoices {
    root: ComponentId,
    dismiss: ComponentId,
    next: u64,
    generation: u64,
    request: Option<ComposerChoiceRequest>,
    catalog: Option<ComposerChoiceCatalog>,
    pending: Option<ComposerChoiceSelection>,
    error: Option<String>,
    rows: BTreeMap<ComponentId, ComposerChoice>,
    body: StackContent,
    labels: ComposerChoiceLabels,
}
impl ComposerChoices {
    pub fn new(namespace: u32, labels: ComposerChoiceLabels) -> Self {
        let base = (namespace as u64) << 32;
        Self {
            root: ComponentId(base),
            dismiss: ComponentId(base + 1),
            next: 16,
            generation: 0,
            request: None,
            catalog: None,
            pending: None,
            error: None,
            rows: BTreeMap::new(),
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            labels,
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn dismiss_id(&self) -> ComponentId {
        self.dismiss
    }
    pub fn request(&self) -> Option<&ComposerChoiceRequest> {
        self.request.as_ref()
    }
    /// Host navigation/cancellation retires only its captured gesture.
    pub fn dismiss(&mut self, request: &ComposerChoiceRequest) -> bool {
        if self.request.as_ref() != Some(request) {
            return false;
        }
        self.close();
        self.rebuild();
        true
    }
    pub fn choice_id(&self, choice: &str) -> Option<ComponentId> {
        self.rows
            .iter()
            .find_map(|(id, row)| (row.id == choice).then_some(*id))
    }
    pub fn open(
        &mut self,
        destination: ComposerDestination,
        kind: ComposerChoiceKind,
    ) -> Result<ComposerChoiceRequest, &'static str> {
        destination
            .validate()
            .map_err(|_| "invalid choice destination")?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("choice generation exhausted")?;
        // Dismiss IDs also change across A -> B -> A so late AX callbacks cannot
        // dismiss a newer gesture. Namespace allocation never reuses an ID.
        self.reserve(1)?;
        self.dismiss = self.allocate();
        self.generation = generation;
        let request = ComposerChoiceRequest {
            generation,
            destination,
            kind,
        };
        self.request = Some(request.clone());
        self.catalog = None;
        self.pending = None;
        self.error = None;
        self.rows.clear();
        self.rebuild();
        Ok(request)
    }
    pub fn update(
        &mut self,
        request: &ComposerChoiceRequest,
        catalog: ComposerChoiceCatalog,
    ) -> Result<bool, &'static str> {
        if self.request.as_ref() != Some(request)
            || self
                .catalog
                .as_ref()
                .is_some_and(|old| catalog.revision <= old.revision)
        {
            return Ok(false);
        }
        if catalog.revision == 0 || catalog.choices.len() > 4096 {
            return Err("invalid choice catalog size/revision");
        }
        let mut ids = BTreeSet::new();
        if catalog
            .choices
            .iter()
            .any(|choice| choice.id.is_empty() || !ids.insert(&choice.id))
        {
            return Err("empty or duplicate choice identity");
        }
        self.reserve(catalog.choices.len() as u64)?;
        self.rows.clear();
        for choice in &catalog.choices {
            let id = self.allocate();
            self.rows.insert(id, choice.clone());
        }
        self.catalog = Some(catalog);
        self.error = None;
        self.rebuild();
        Ok(true)
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<ComposerChoiceIntent> {
        let mut result = vec![];
        for action in actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
        {
            let Some(request) = self.request.clone() else {
                break;
            };
            if action.target == self.dismiss {
                self.close();
                result.push(ComposerChoiceIntent::Dismiss(request));
                break;
            }
            if self.pending.is_some() {
                continue;
            }
            let Some(catalog) = &self.catalog else {
                continue;
            };
            let Some(choice) = self.rows.get(&action.target) else {
                continue;
            };
            if catalog.phase != ComposerCatalogPhase::Ready || !choice.enabled {
                continue;
            }
            let selection = ComposerChoiceSelection {
                request,
                catalog_revision: catalog.revision,
                choice: choice.clone(),
            };
            self.pending = Some(selection.clone());
            result.push(ComposerChoiceIntent::Select(Box::new(selection)));
        }
        if !result.is_empty() {
            self.rebuild();
        }
        result
    }
    pub fn complete(
        &mut self,
        selection: &ComposerChoiceSelection,
        result: Result<(), String>,
    ) -> bool {
        if self.request.as_ref() != Some(&selection.request)
            || self.pending.as_ref() != Some(selection)
        {
            return false;
        }
        self.pending = None;
        match result {
            Ok(()) => self.close(),
            Err(error) => self.error = Some(error),
        }
        self.rebuild();
        true
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    fn reserve(&self, count: u64) -> Result<(), &'static str> {
        self.next
            .checked_add(count)
            .filter(|next| *next < u32::MAX as u64)
            .map(|_| ())
            .ok_or("choice namespace exhausted")
    }
    fn allocate(&mut self) -> ComponentId {
        let id = ComponentId(self.root.0 + self.next);
        self.next += 1;
        id
    }
    fn close(&mut self) {
        self.request = None;
        self.pending = None;
        self.catalog = None;
        self.error = None;
        self.rows.clear();
    }
    fn rebuild(&mut self) {
        self.body.children.clear();
        let Some(request) = &self.request else { return };
        let title = match request.kind {
            ComposerChoiceKind::Model => &self.labels.model,
            ComposerChoiceKind::Preset => &self.labels.preset,
        };
        self.body.children.push(Content::new(
            ComponentId(self.root.0 + 2),
            TextContent::new(title),
        ));
        let ready = self
            .catalog
            .as_ref()
            .is_some_and(|catalog| catalog.phase == ComposerCatalogPhase::Ready);
        let status = if self.pending.is_some() {
            self.labels.pending.clone()
        } else if let Some(error) = &self.error {
            error.clone()
        } else {
            match self.catalog.as_ref().map(|catalog| &catalog.phase) {
                Some(ComposerCatalogPhase::Failed(error)) => error.clone(),
                Some(ComposerCatalogPhase::Ready) if self.rows.is_empty() => {
                    self.labels.empty.clone()
                }
                Some(ComposerCatalogPhase::Ready) => String::new(),
                _ => self.labels.loading.clone(),
            }
        };
        self.body.children.push(Content::new(
            ComponentId(self.root.0 + 3),
            TextContent::new(status),
        ));
        if let Some(selected) = self
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.selected.as_ref())
            && !self.rows.values().any(|choice| &choice.id == selected)
        {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 4),
                TextContent::new(format!("{}: {selected}", self.labels.current)),
            ));
        }
        for (id, choice) in &self.rows {
            let mut label = choice.title.clone();
            if self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.selected.as_ref())
                == Some(&choice.id)
            {
                label.push_str(&format!(" · {}", self.labels.selected));
            }
            for detail in [Some(&choice.detail), choice.disabled_reason.as_ref()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
            {
                label.push_str(" · ");
                label.push_str(detail);
            }
            let mut button = ButtonContent::new(label);
            button.button.state.selected = self
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.selected.as_ref())
                == Some(&choice.id);
            button.button.state.disabled = !ready || !choice.enabled || self.pending.is_some();
            self.body.children.push(Content::new(*id, button));
        }
        self.body.children.push(Content::new(
            self.dismiss,
            ButtonContent::new(&self.labels.dismiss),
        ));
    }
}
impl Component for ComposerChoices {
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
        self.body.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
