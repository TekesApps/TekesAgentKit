//! Agent draft content cards; paths/media/system operations are host callbacks.
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_presentation::{
    ComposerAnnotation, ComposerArtifact, ComposerContextBaseline, RunSnapshot,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq)]
pub enum ComposerContextItem {
    Artifact(ComposerArtifact),
    Annotation(ComposerAnnotation),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerContextAction {
    Remove,
    Open,
    EditAnnotation,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerContextIntent {
    pub baseline: Arc<ComposerContextBaseline>,
    pub item: ComposerContextItem,
    pub action: ComposerContextAction,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposerContextAffordances {
    pub preview_artifacts: bool,
    pub reveal_annotations: bool,
    pub edit_annotations: bool,
}
#[derive(Clone, Debug)]
pub struct ComposerContextLabels {
    pub group: String,
    pub attachment: String,
    pub command: String,
    pub skill: String,
    pub reference: String,
    pub annotation: String,
    pub remove: String,
    pub open: String,
    pub edit: String,
}
impl Default for ComposerContextLabels {
    fn default() -> Self {
        Self {
            group: "草稿内容".into(),
            attachment: "附件".into(),
            command: "命令".into(),
            skill: "技能".into(),
            reference: "引用".into(),
            annotation: "批注".into(),
            remove: "移除".into(),
            open: "查看来源".into(),
            edit: "编辑批注".into(),
        }
    }
}
#[derive(Debug)]
pub struct ComposerContext {
    root: ComponentId,
    next: u64,
    baseline: Option<Arc<ComposerContextBaseline>>,
    revision: u64,
    editable: bool,
    blocked: bool,
    affordances: ComposerContextAffordances,
    labels: ComposerContextLabels,
    body: StackContent,
    actions: BTreeMap<ComponentId, (ContextRow, ComposerContextAction)>,
}
#[derive(Clone, Copy, Debug)]
enum ContextRow {
    Artifact(usize),
    Annotation(usize),
}
impl ComposerContext {
    pub fn new(namespace: u32, labels: ComposerContextLabels) -> Self {
        Self {
            root: ComponentId((namespace as u64) << 32),
            next: 16,
            baseline: None,
            revision: 0,
            editable: false,
            blocked: false,
            affordances: Default::default(),
            labels,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            actions: BTreeMap::new(),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn action_id(
        &self,
        item: &ComposerContextItem,
        action: ComposerContextAction,
    ) -> Option<ComponentId> {
        self.actions.iter().find_map(|(id, (row, kind))| {
            (self.matches_item(*row, item) && *kind == action).then_some(*id)
        })
    }
    pub fn update(
        &mut self,
        snapshot: &RunSnapshot,
        editable: bool,
        affordances: ComposerContextAffordances,
    ) -> Result<(), &'static str> {
        if snapshot.revision == 0 || snapshot.revision < self.revision {
            return Err("stale context snapshot");
        }
        let source_changed = match (&self.baseline, &snapshot.selection) {
            (None, None) => false,
            (Some(old), Some(_)) => !old.matches_snapshot(snapshot),
            _ => true,
        };
        let changed = source_changed || affordances != self.affordances;
        if changed {
            let count = snapshot
                .draft
                .artifacts
                .len()
                .checked_add(snapshot.draft.annotations.len())
                .and_then(|n| n.checked_mul(8))
                .ok_or("context size exhausted")?;
            self.next
                .checked_add(count as u64)
                .filter(|next| *next < u32::MAX as u64)
                .ok_or("context namespace exhausted")?;
        }
        self.revision = snapshot.revision;
        self.editable = editable;
        self.affordances = affordances;
        if changed {
            if source_changed {
                self.baseline = ComposerContextBaseline::read(snapshot).map(Arc::new);
            }
            self.actions.clear();
            self.body.children.clear();
            if let Some(baseline) = self.baseline.clone() {
                for i in 0..baseline.artifacts.len() {
                    self.add_card(ContextRow::Artifact(i), &baseline);
                }
                for i in 0..baseline.annotations.len() {
                    self.add_card(ContextRow::Annotation(i), &baseline);
                }
            }
        }
        self.refresh_enabled();
        Ok(())
    }
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<ComposerContextIntent> {
        if self.blocked {
            return vec![];
        }
        let Some(baseline) = &self.baseline else {
            return vec![];
        };
        actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
            .filter_map(|a| {
                let (item, action) = self.actions.get(&a.target)?;
                if matches!(
                    action,
                    ComposerContextAction::Remove | ComposerContextAction::EditAnnotation
                ) && !self.editable
                {
                    return None;
                }
                Some(ComposerContextIntent {
                    baseline: baseline.clone(),
                    item: self.item(*item)?,
                    action: *action,
                })
            })
            .collect()
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub(crate) fn set_blocked(&mut self, blocked: bool) {
        self.blocked = blocked;
        self.refresh_enabled();
    }
    fn allocate(&mut self) -> ComponentId {
        let id = ComponentId(self.root.0 + self.next);
        self.next += 1;
        id
    }
    fn matches_item(&self, row: ContextRow, item: &ComposerContextItem) -> bool {
        let Some(baseline) = &self.baseline else {
            return false;
        };
        match (row, item) {
            (ContextRow::Artifact(i), ComposerContextItem::Artifact(item)) => {
                baseline.artifacts.get(i) == Some(item)
            }
            (ContextRow::Annotation(i), ComposerContextItem::Annotation(item)) => {
                baseline.annotations.get(i) == Some(item)
            }
            _ => false,
        }
    }
    fn item(&self, row: ContextRow) -> Option<ComposerContextItem> {
        let baseline = self.baseline.as_ref()?;
        match row {
            ContextRow::Artifact(i) => baseline
                .artifacts
                .get(i)
                .cloned()
                .map(ComposerContextItem::Artifact),
            ContextRow::Annotation(i) => baseline
                .annotations
                .get(i)
                .cloned()
                .map(ComposerContextItem::Annotation),
        }
    }
    fn add_card(&mut self, item: ContextRow, baseline: &ComposerContextBaseline) {
        let (title, detail) = match item {
            ContextRow::Artifact(i) => match &baseline.artifacts[i] {
                ComposerArtifact::Attachment { name, facts, .. } => (
                    format!("{} · {name}", self.labels.attachment),
                    facts
                        .as_ref()
                        .map(|facts| format!("{} B", facts.byte_count))
                        .unwrap_or_default(),
                ),
                ComposerArtifact::Command { name, .. } => {
                    (format!("{} · /{name}", self.labels.command), String::new())
                }
                ComposerArtifact::Skill { name, .. } => {
                    (format!("{} · {name}", self.labels.skill), String::new())
                }
                ComposerArtifact::Reference {
                    mention,
                    path,
                    excerpt,
                    ..
                } => (
                    format!(
                        "{} · {}",
                        self.labels.reference,
                        mention.as_deref().or(path.as_deref()).unwrap_or("")
                    ),
                    // Match the shared compiler's mention -> path -> excerpt
                    // precedence. Hidden preview metadata is not presented as
                    // quoted text that the model will receive.
                    if mention.is_none() && path.is_none() {
                        excerpt.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                ),
            },
            ContextRow::Annotation(i) => {
                let annotation = &baseline.annotations[i];
                (
                    format!("{} · {}", self.labels.annotation, annotation.source),
                    format!("{}\n{}", annotation.excerpt, annotation.comment),
                )
            }
        };
        let mut children = vec![Content::new(self.allocate(), TextContent::new(title))];
        if !detail.is_empty() {
            children.push(Content::new(self.allocate(), TextContent::new(detail)));
        }
        let mut controls = vec![];
        let mut kinds = vec![(ComposerContextAction::Remove, self.labels.remove.clone())];
        let annotation = matches!(item, ContextRow::Annotation(_));
        if (annotation && self.affordances.reveal_annotations)
            || (!annotation && self.affordances.preview_artifacts)
        {
            kinds.push((ComposerContextAction::Open, self.labels.open.clone()));
        }
        if annotation && self.affordances.edit_annotations {
            kinds.push((
                ComposerContextAction::EditAnnotation,
                self.labels.edit.clone(),
            ));
        }
        for (kind, label) in kinds {
            let id = self.allocate();
            controls.push(Content::new(id, ButtonContent::new(label)));
            self.actions.insert(id, (item, kind));
        }
        children.push(Content::new(
            self.allocate(),
            StackContent {
                model: Stack {
                    axis: Axis::Horizontal,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: controls,
            },
        ));
        let row = self.allocate();
        self.body.children.push(Content::new(
            row,
            StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children,
            },
        ));
    }
    fn refresh_enabled(&mut self) {
        for row in &mut self.body.children {
            let children = row.component_mut::<StackContent>().unwrap();
            for button in &mut children
                .children
                .last_mut()
                .unwrap()
                .component_mut::<StackContent>()
                .unwrap()
                .children
            {
                let (_, kind) = self.actions.get(&button.id()).unwrap();
                button
                    .component_mut::<ButtonContent>()
                    .unwrap()
                    .button
                    .state
                    .disabled = self.blocked
                    || (!self.editable
                        && matches!(
                            kind,
                            ComposerContextAction::Remove | ComposerContextAction::EditAnnotation
                        ));
            }
        }
    }
}
impl Component for ComposerContext {
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
