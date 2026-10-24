//! Existing annotation comments over TekesUI editing; provenance is immutable.
use crate::{ComposerContextAction, ComposerContextIntent, ComposerContextItem};
use std::sync::Arc;
use tekes_agent_presentation::{ComposerAnnotation, ComposerContextBaseline, RunSnapshot};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnnotationEditorIds {
    pub root: ComponentId,
    pub editor: ComponentId,
    pub close: ComponentId,
    source: ComponentId,
    pub excerpt: ComponentId,
    status: ComponentId,
}
impl AnnotationEditorIds {
    fn new(namespace: u32) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            root,
            editor: ComponentId(root.0 + 1),
            close: ComponentId(root.0 + 2),
            source: ComponentId(root.0 + 3),
            excerpt: ComponentId(root.0 + 4),
            status: ComponentId(root.0 + 5),
        }
    }
}
#[derive(Clone, Debug)]
pub struct AnnotationEditorLabels {
    pub group: String,
    pub comment: String,
    pub excerpt: String,
    pub close: String,
    pub pending: String,
    pub invalid: String,
    pub stale: String,
}
impl Default for AnnotationEditorLabels {
    fn default() -> Self {
        Self {
            group: "编辑批注评论".into(),
            comment: "批注评论".into(),
            excerpt: "引用（只读）".into(),
            close: "关闭".into(),
            pending: "正在更新批注".into(),
            invalid: "批注评论无效或过长，尚未写入草稿".into(),
            stale: "草稿内容或权限已变化，编辑文字保留但未继续写入；请关闭后重新打开批注".into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationEditChange {
    pub generation: u64,
    pub baseline: Arc<ComposerContextBaseline>,
    pub annotation: ComposerAnnotation,
    pub comment: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationEditorIntent {
    Change(Box<AnnotationEditChange>),
    Dismiss,
}
#[derive(Debug)]
struct Target {
    baseline: Arc<ComposerContextBaseline>,
    annotation: ComposerAnnotation,
}
/// Only a transient field/gesture. Committed comments live in shared drafts;
/// selection, composition, editing and undo belong to the existing TextSurface.
#[derive(Debug)]
pub struct AnnotationEditor {
    ids: AnnotationEditorIds,
    labels: AnnotationEditorLabels,
    body: StackContent,
    generation: u64,
    revision: u64,
    observed: Option<(tekes_agent_presentation::SelectionToken, u64)>,
    editable: bool,
    target: Option<Target>,
    pending: Option<AnnotationEditChange>,
    stale: bool,
    status: String,
}
impl AnnotationEditor {
    pub fn new(namespace: u32, labels: AnnotationEditorLabels) -> Self {
        Self {
            ids: AnnotationEditorIds::new(namespace),
            labels,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            generation: 0,
            revision: 0,
            observed: None,
            editable: true,
            target: None,
            pending: None,
            stale: false,
            status: String::new(),
        }
    }
    pub fn ids(&self) -> AnnotationEditorIds {
        self.ids
    }
    pub fn is_open(&self) -> bool {
        self.target.is_some()
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn editor(&self) -> Option<&TextSurface> {
        self.body
            .children
            .iter()
            .find(|node| node.id() == self.ids.editor)?
            .component::<TextFieldContent>()
            .map(|field| &field.surface)
    }
    pub fn is_composing(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.input.marked.is_some())
    }
    pub fn open(&mut self, intent: &ComposerContextIntent) -> Result<(), &'static str> {
        if self.is_composing() {
            return Err("finish annotation composition before changing source");
        }
        let ComposerContextItem::Annotation(annotation) = &intent.item else {
            return Err("not an annotation");
        };
        if intent.action != ComposerContextAction::EditAnnotation
            || annotation.validate().is_err()
            || intent
                .baseline
                .annotations
                .iter()
                .filter(|a| a.id == annotation.id)
                .count()
                != 1
            || !intent.baseline.annotations.contains(annotation)
        {
            return Err("annotation edit unavailable");
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("annotation generation exhausted")?;
        let offset = generation
            .checked_mul(8)
            .filter(|offset| *offset < u32::MAX as u64 - 8)
            .ok_or("annotation namespace exhausted")?;
        self.generation = generation;
        self.ids.editor = ComponentId(self.ids.root.0 + offset + 1);
        self.ids.close = ComponentId(self.ids.root.0 + offset + 2);
        self.ids.source = ComponentId(self.ids.root.0 + offset + 3);
        self.ids.excerpt = ComponentId(self.ids.root.0 + offset + 4);
        self.ids.status = ComponentId(self.ids.root.0 + offset + 5);
        let mut field = TextFieldContent::new(&self.labels.comment, &annotation.comment);
        field.surface = TextSurface::new(&annotation.comment, true, false);
        field.viewport = TextViewport::Grow {
            min: field.surface.minimum_outer_height(1.),
            max: field.surface.role.style().line_height * 6.
                + 2. * tekes_ui_tokens::proposal::SCROLL_CONTENT_PADDING,
        };
        field.submit_policy = TextSubmitPolicy::None;
        let mut excerpt = TextFieldContent::new(&self.labels.excerpt, &annotation.excerpt);
        excerpt.surface = TextSurface::new(&annotation.excerpt, true, true);
        excerpt.viewport = TextViewport::Grow {
            min: excerpt.surface.minimum_outer_height(1.),
            max: excerpt.surface.role.style().line_height * 3.
                + 2. * tekes_ui_tokens::proposal::SCROLL_CONTENT_PADDING,
        };
        self.body.children = vec![
            Content::new(self.ids.source, TextContent::new(&annotation.source)),
            Content::new(self.ids.excerpt, excerpt),
            Content::new(self.ids.editor, field),
            Content::new(self.ids.close, ButtonContent::new(&self.labels.close)),
            Content::new(self.ids.status, TextContent::new("")),
        ];
        self.target = Some(Target {
            baseline: intent.baseline.clone(),
            annotation: annotation.clone(),
        });
        self.pending = None;
        self.stale = false;
        self.status.clear();
        self.revision = 0;
        self.observed = Some((
            intent.baseline.selection.clone(),
            intent.baseline.context_revision,
        ));
        self.editable = true;
        Ok(())
    }
    /// Feed the original controller's snapshot, not a newly selected binding.
    /// Invalidation never replaces text, marked text, selection or undo.
    pub fn update_authority(
        &mut self,
        snapshot: &RunSnapshot,
        editable: bool,
    ) -> Result<(), &'static str> {
        if snapshot.revision == 0 || snapshot.revision < self.revision {
            return Err("stale annotation snapshot");
        }
        self.revision = snapshot.revision;
        self.observed = snapshot
            .selection
            .clone()
            .map(|selection| (selection, snapshot.draft_context_revision));
        self.editable = editable;
        if self
            .target
            .as_ref()
            .is_some_and(|target| !editable || !target.baseline.matches_snapshot(snapshot))
        {
            self.stale = true;
            self.status = self.labels.stale.clone();
        }
        self.refresh();
        Ok(())
    }
    pub fn dismiss(&mut self) -> Result<bool, &'static str> {
        if self.is_composing() {
            return Err("finish annotation composition before dismissal");
        }
        let open = self.is_open();
        self.target = None;
        self.pending = None;
        self.body.children.clear();
        Ok(open)
    }
    pub fn invalidate(&mut self) {
        self.stale = true;
        self.editable = false;
        self.status = self.labels.stale.clone();
        self.refresh();
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<AnnotationEditorIntent> {
        self.refresh();
        if self.target.is_none() || self.is_composing() {
            return vec![];
        }
        let mut intents = vec![];
        if !self.stale && self.pending.is_none() {
            let target = self.target.as_ref().unwrap();
            let comment = self.editor().unwrap().input.text.clone();
            if comment != target.annotation.comment {
                let mut updated = target.annotation.clone();
                updated.comment = comment.clone();
                if updated.validate().is_err() {
                    self.status = self.labels.invalid.clone();
                } else {
                    let change = AnnotationEditChange {
                        generation: self.generation,
                        baseline: target.baseline.clone(),
                        annotation: target.annotation.clone(),
                        comment,
                    };
                    self.pending = Some(change.clone());
                    self.status = self.labels.pending.clone();
                    intents.push(AnnotationEditorIntent::Change(Box::new(change)));
                }
            }
        }
        if self.pending.is_none()
            && actions.iter().any(|action| {
                action.target == self.ids.close && action.kind == ContentActionKind::Activate
            })
        {
            self.dismiss().expect("unmarked editor");
            intents.push(AnnotationEditorIntent::Dismiss);
        }
        self.refresh();
        intents
    }
    /// Some is the original controller's post-write snapshot; None invalidates
    /// this capture. Its publication revision also fences late pre-write input.
    /// Drain again after a successful acknowledgement if newer typing arrived
    /// while awaiting it. Failed/stale captures never automatically retry.
    pub fn complete(
        &mut self,
        change: &AnnotationEditChange,
        result: Result<Option<Arc<RunSnapshot>>, String>,
    ) -> bool {
        if self.generation != change.generation || self.pending.as_ref() != Some(change) {
            return false;
        }
        self.pending = None;
        let mut expected = change.annotation.clone();
        expected.comment = change.comment.clone();
        let acknowledgement = result.map(|snapshot| {
            snapshot.and_then(|snapshot| {
                if snapshot.revision == 0 {
                    return None;
                }
                Some((ComposerContextBaseline::read(&snapshot)?, snapshot.revision))
            })
        });
        match acknowledgement {
            Ok(Some((baseline, revision)))
                if baseline.selection.destination == change.baseline.selection.destination
                    && baseline.context_revision > change.baseline.context_revision
                    && baseline
                        .annotations
                        .iter()
                        .filter(|a| a.id == expected.id)
                        .count()
                        == 1
                    && baseline.annotations.contains(&expected) =>
            {
                self.revision = self.revision.max(revision);
                let current = self.editable
                    && self.observed.as_ref().is_some_and(|(selection, revision)| {
                        selection.destination == baseline.selection.destination
                            && *revision <= baseline.context_revision
                            && (!matches!(
                                &selection.destination,
                                tekes_agent_presentation::ComposerDestination::NewThread { .. }
                            ) || selection.generation == baseline.selection.generation)
                    });
                self.target = Some(Target {
                    baseline: Arc::new(baseline),
                    annotation: expected,
                });
                self.stale = !current;
                self.status = if current {
                    String::new()
                } else {
                    self.labels.stale.clone()
                };
            }
            Ok(_) => {
                self.stale = true;
                self.status = self.labels.stale.clone();
            }
            Err(error) => {
                self.stale = true;
                self.status = format!("{} · {error}", self.labels.stale);
            }
        }
        self.refresh();
        true
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.ids.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    fn refresh(&mut self) {
        if !self.is_open() {
            return;
        }
        let blocked = self.is_composing() || self.pending.is_some();
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.close)
            .unwrap()
            .component_mut::<ButtonContent>()
            .unwrap()
            .button
            .state
            .disabled = blocked;
        self.body
            .children
            .iter_mut()
            .find(|node| node.id() == self.ids.status)
            .unwrap()
            .component_mut::<TextContent>()
            .unwrap()
            .text = self.status.clone();
    }
}
impl Component for AnnotationEditor {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.refresh();
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip)
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.group)
    }
}
