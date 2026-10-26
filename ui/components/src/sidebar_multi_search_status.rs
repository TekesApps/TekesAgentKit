//! Endpoint-owned status rows for an aggregate Sidebar search publication.
//! This component presents the original child state and only emits fenced
//! retry intents; it does not own search or retry IO.
use crate::SidebarSearchLabels;
use std::collections::BTreeMap;
use tekes_agent_presentation::{
    SidebarMultiSearchIntent, SidebarMultiSearchKey, SidebarMultiSearchPublication,
    SidebarSearchKey, SidebarSearchPhase,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::{ComponentId, *},
    extended::{Axis, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AttemptIdentity {
    aggregate: Option<SidebarMultiSearchKey>,
    child: Option<SidebarSearchKey>,
}

#[derive(Clone, Debug)]
struct EndpointControls {
    identity: AttemptIdentity,
    row: ComponentId,
    status: ComponentId,
    retry: ComponentId,
    pending: bool,
}

/// A small view of the per-endpoint state in one validated aggregate
/// publication. The host remains responsible for forwarding returned intents
/// to the original controller.
#[derive(Debug)]
pub struct SidebarMultiSearchStatus {
    root: ComponentId,
    labels: SidebarSearchLabels,
    body: StackContent,
    owner: Option<String>,
    displayed: Option<SidebarMultiSearchPublication>,
    controls: BTreeMap<String, EndpointControls>,
    seen: BTreeMap<String, SidebarSearchKey>,
    next: u64,
    end: u64,
}

impl SidebarMultiSearchStatus {
    /// Construct a standalone status component in its own 32-bit namespace.
    pub fn new(namespace: u32, labels: SidebarSearchLabels) -> Self {
        let base = (namespace as u64) << 32;
        Self::from_id_range(ComponentId(base), base + 1, base + u32::MAX as u64, labels)
            .expect("valid standalone Sidebar multi-search ID range")
    }

    /// Construct inside a range reserved by a parent component. `end` is
    /// exclusive; IDs are allocated monotonically and are never reused.
    pub(crate) fn from_id_range(
        root: ComponentId,
        next: u64,
        end: u64,
        labels: SidebarSearchLabels,
    ) -> Result<Self, &'static str> {
        if next > end {
            return Err("invalid Sidebar multi-search ID range");
        }
        Ok(Self {
            root,
            labels,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: Vec::new(),
            },
            owner: None,
            displayed: None,
            controls: BTreeMap::new(),
            seen: BTreeMap::new(),
            next,
            end,
        })
    }

    pub fn id(&self) -> ComponentId {
        self.root
    }

    pub(crate) fn next_id(&self) -> u64 {
        self.next
    }

    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }

    /// Accept an explicitly new source owner. Replacing the owner starts a
    /// fresh publication revision sequence while retiring every prior control.
    /// Reusing the current owner still observes the normal revision fence.
    pub fn replace_source(
        &mut self,
        owner: &str,
        publication: &SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        self.validate_source(owner, publication)?;
        let is_new_owner = self.owner.as_deref() != Some(owner);
        if !is_new_owner {
            self.validate_revision(publication)?;
        }

        let aggregate = publication.input.key.clone();
        let mut required = 0u64;
        for child in &publication.children {
            let identity = AttemptIdentity {
                aggregate: aggregate.clone(),
                child: child.publication.input.key.clone(),
            };
            if is_new_owner
                || self
                    .controls
                    .get(&child.endpoint_id)
                    .is_none_or(|controls| controls.identity != identity)
            {
                required = required
                    .checked_add(3)
                    .ok_or("Sidebar multi-search ID range exhausted")?;
            }
        }
        let next_end = self
            .next
            .checked_add(required)
            .ok_or("Sidebar multi-search ID range exhausted")?;
        if next_end > self.end {
            return Err("Sidebar multi-search ID range exhausted");
        }

        if is_new_owner {
            self.controls.clear();
            self.seen.clear();
        }
        self.owner = Some(owner.to_owned());
        for child in &publication.children {
            let identity = AttemptIdentity {
                aggregate: aggregate.clone(),
                child: child.publication.input.key.clone(),
            };
            let replace = self
                .controls
                .get(&child.endpoint_id)
                .is_none_or(|controls| controls.identity != identity);
            if replace {
                let row = ComponentId(self.allocate_id()?);
                let status = ComponentId(self.allocate_id()?);
                let retry = ComponentId(self.allocate_id()?);
                self.controls.insert(
                    child.endpoint_id.clone(),
                    EndpointControls {
                        identity,
                        row,
                        status,
                        retry,
                        pending: false,
                    },
                );
            }
        }
        self.controls.retain(|endpoint, _| {
            publication
                .children
                .iter()
                .any(|child| &child.endpoint_id == endpoint)
        });
        for child in &publication.children {
            if let Some(key) = &child.publication.input.key {
                self.seen.insert(child.endpoint_id.clone(), key.clone());
            }
        }
        self.displayed = Some(publication.clone());
        self.rebuild();
        Ok(())
    }

    /// Update the active owner. Same-revision updates must be byte-for-byte
    /// model-equal; lower revisions and owner changes are rejected.
    pub fn update_from(
        &mut self,
        owner: &str,
        publication: &SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        self.validate_source(owner, publication)?;
        if self.owner.as_deref() != Some(owner) {
            return Err("Sidebar multi-search owner changed; replace source explicitly");
        }
        self.validate_revision(publication)?;
        self.replace_source(owner, publication)
    }

    pub fn displayed(&self) -> Option<&SidebarMultiSearchPublication> {
        self.displayed.as_ref()
    }

    pub fn source_owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn set_labels(&mut self, labels: SidebarSearchLabels) -> Result<(), &'static str> {
        self.labels = labels;
        self.rebuild();
        Ok(())
    }

    /// Return the original SDK button ID only while this endpoint currently
    /// publishes a retryable failure.
    pub fn retry_id(&self, endpoint: &str) -> Option<ComponentId> {
        let publication = self.displayed.as_ref()?;
        let child = publication
            .children
            .iter()
            .find(|child| child.endpoint_id == endpoint)?;
        if !matches!(
            &child.publication.input.phase,
            SidebarSearchPhase::Failed {
                retryable: true,
                ..
            }
        ) {
            return None;
        }
        self.controls.get(endpoint).map(|controls| controls.retry)
    }

    /// Capture only actions delivered to the current original button and only
    /// while its source is still the retryable failed child attempt.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<SidebarMultiSearchIntent> {
        let Some(publication) = self.displayed.as_ref() else {
            return Vec::new();
        };
        let aggregate_key = publication.input.key.clone();
        let mut intents = Vec::new();
        for action in actions {
            if action.kind != ContentActionKind::Activate {
                continue;
            }
            let Some((endpoint, controls)) = self
                .controls
                .iter_mut()
                .find(|(_, controls)| controls.retry == action.target)
            else {
                continue;
            };
            if controls.pending {
                continue;
            }
            let Some(child) = publication
                .children
                .iter()
                .find(|child| &child.endpoint_id == endpoint)
            else {
                continue;
            };
            let SidebarSearchPhase::Failed {
                retryable: true, ..
            } = &child.publication.input.phase
            else {
                continue;
            };
            let (Some(aggregate_key), Some(child_key)) =
                (aggregate_key.clone(), child.publication.input.key.clone())
            else {
                continue;
            };
            if controls.identity.aggregate.as_ref() != Some(&aggregate_key)
                || controls.identity.child.as_ref() != Some(&child_key)
            {
                continue;
            }
            controls.pending = true;
            intents.push(SidebarMultiSearchIntent::Retry {
                aggregate_key,
                child_key,
            });
        }
        if !intents.is_empty() {
            self.rebuild();
        }
        intents
    }

    fn validate_source(
        &self,
        owner: &str,
        publication: &SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        publication
            .validate()
            .map_err(|_| "invalid Sidebar multi-search publication")?;
        if owner.trim().is_empty() || publication.input.owner != owner {
            return Err("Sidebar multi-search owner mismatch");
        }
        Ok(())
    }

    fn validate_revision(
        &self,
        publication: &SidebarMultiSearchPublication,
    ) -> Result<(), &'static str> {
        let Some(current) = self.displayed.as_ref() else {
            return Ok(());
        };
        if publication.input.revision < current.input.revision {
            return Err("Sidebar multi-search revision moved backwards");
        }
        if publication.input.revision == current.input.revision && publication != current {
            return Err("Sidebar multi-search revision was reused");
        }
        if publication.input.generation < current.input.generation
            || (publication.input.generation == current.input.generation
                && publication.input.key != current.input.key)
            || publication.children.len() != current.children.len()
        {
            return Err("retired Sidebar multi-search query");
        }
        for child in &publication.children {
            let previous = current
                .children
                .iter()
                .find(|old| old.endpoint_id == child.endpoint_id)
                .ok_or("multi-search endpoint set changed")?;
            if child.publication.input.revision < previous.publication.input.revision
                || (child.publication.input.revision == previous.publication.input.revision
                    && child.publication != previous.publication)
            {
                return Err("retired multi-search child publication");
            }
            if let (Some(key), Some(seen)) = (
                &child.publication.input.key,
                self.seen.get(&child.endpoint_id),
            ) && (key.owner != seen.owner
                || key.connection_epoch < seen.connection_epoch
                || key.generation < seen.generation
                || (key.generation == seen.generation && key != seen))
            {
                return Err("retired multi-search child attempt");
            }
        }
        Ok(())
    }

    fn allocate_id(&mut self) -> Result<u64, &'static str> {
        if self.next >= self.end {
            return Err("Sidebar multi-search ID range exhausted");
        }
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Sidebar multi-search ID range exhausted")?;
        Ok(id)
    }

    fn rebuild(&mut self) {
        let Some(publication) = self.displayed.as_ref() else {
            self.body.children.clear();
            return;
        };
        let mut old: BTreeMap<_, _> = std::mem::take(&mut self.body.children)
            .into_iter()
            .map(|node| (node.id(), node))
            .collect();
        let mut children = Vec::new();
        for child in &publication.children {
            let Some(controls) = self.controls.get(&child.endpoint_id) else {
                continue;
            };
            let status = match &child.publication.input.phase {
                SidebarSearchPhase::Idle => None,
                SidebarSearchPhase::Searching => Some(self.labels.searching.clone()),
                SidebarSearchPhase::Complete { has_more: true } => Some(self.labels.more.clone()),
                SidebarSearchPhase::Complete { has_more: false }
                    if !child.publication.unresolved.is_empty() =>
                {
                    Some(self.labels.pending_results.clone())
                }
                SidebarSearchPhase::Complete { has_more: false } => None,
                SidebarSearchPhase::Failed {
                    message, has_more, ..
                } => Some(if *has_more {
                    format!("{message}\n{}", self.labels.more)
                } else {
                    message.clone()
                }),
            };
            let Some(status) = status else { continue };
            let mut content = old.remove(&controls.row).unwrap_or_else(|| {
                Content::new(
                    controls.row,
                    StackContent {
                        model: Stack {
                            axis: Axis::Horizontal,
                            gap: Spacing::Regular.value(),
                        },
                        flexible_child: Some(controls.status),
                        children: vec![],
                    },
                )
            });
            let row = content.component_mut::<StackContent>().unwrap();
            let mut parts: BTreeMap<_, _> = std::mem::take(&mut row.children)
                .into_iter()
                .map(|node| (node.id(), node))
                .collect();
            let mut text = parts
                .remove(&controls.status)
                .unwrap_or_else(|| Content::new(controls.status, TextContent::new("")));
            text.component_mut::<TextContent>().unwrap().text =
                format!("{}: {status}", child.endpoint_id);
            row.children.push(text);
            if matches!(
                &child.publication.input.phase,
                SidebarSearchPhase::Failed {
                    retryable: true,
                    ..
                }
            ) {
                let mut button = parts
                    .remove(&controls.retry)
                    .unwrap_or_else(|| Content::new(controls.retry, ButtonContent::new("")));
                let model = button.component_mut::<ButtonContent>().unwrap();
                model.label = self.labels.retry.clone();
                model.button.state.disabled = controls.pending;
                row.children.push(button);
            }
            children.push(content);
        }
        self.body.children = children;
    }
}

impl Component for SidebarMultiSearchStatus {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn active(&self) -> bool {
        !self.body.children.is_empty()
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if self.body.children.is_empty() {
            return MeasuredChild::new(Size::default());
        }
        let inset = Spacing::Regular.value();
        let inner = self.body.measure(
            text,
            Size::new((available.width - 2.0 * inset).max(0.0), available.height),
        );
        MeasuredChild::new(Size::new(
            inner.size.width + 2.0 * inset,
            inner.size.height + inset,
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let inset = Spacing::Regular.value();
        let layout = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets {
                left: inset,
                right: inset,
                top: 0.,
                bottom: inset,
            },
            Some(clip),
        );
        self.body.arrange(
            text,
            layout.content(),
            layout.visible_clip().unwrap_or_default(),
        );
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
