//! User-visible status for Sidebar inventory observers.
//!
//! This component owns only presentation controls. The host must forward a
//! returned retry intent to the binding that owns the original observer.
use std::collections::BTreeMap;
use tekes_agent_presentation::{
    SidebarObserverEndpointInput, SidebarObserverIntent, SidebarObserverKey, SidebarObserverPhase,
    SidebarObserverSourceKey, SidebarObserverStatusInput,
};
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::{ComponentId, *},
    extended::{Axis, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

/// Localized copy for the observer status controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarObserverLabels {
    pub name: String,
    pub observing: String,
    pub failed: String,
    pub restarting: String,
    pub retry: String,
}

impl Default for SidebarObserverLabels {
    fn default() -> Self {
        Self {
            name: "会话列表监听状态".into(),
            observing: "正在同步会话列表…".into(),
            failed: "会话列表监听已停止".into(),
            restarting: "正在恢复会话列表监听…".into(),
            retry: "重试".into(),
        }
    }
}

#[derive(Clone, Debug)]
struct EndpointControls {
    key: SidebarObserverKey,
    row: ComponentId,
    status: ComponentId,
    retry: ComponentId,
    pending: bool,
}

/// Status rows for the original observer source. It does not own observer IO.
#[derive(Debug)]
pub struct SidebarObserverStatus {
    root: ComponentId,
    labels: SidebarObserverLabels,
    body: StackContent,
    source: Option<SidebarObserverSourceKey>,
    displayed: Option<SidebarObserverStatusInput>,
    controls: BTreeMap<String, EndpointControls>,
    /// Retained after an endpoint disappears so an older generation cannot
    /// become current again if that endpoint is later reintroduced.
    seen: BTreeMap<String, SidebarObserverEndpointInput>,
    next: u64,
    end: u64,
}

impl SidebarObserverStatus {
    /// Construct a standalone component in a 32-bit ID namespace.
    pub fn new(namespace: u32, labels: SidebarObserverLabels) -> Self {
        let base = (namespace as u64) << 32;
        Self::from_id_range(ComponentId(base), base + 1, base + u32::MAX as u64, labels)
            .expect("valid standalone Sidebar observer ID range")
    }

    /// Construct inside an ID range reserved by a parent. `end` is exclusive.
    pub(crate) fn from_id_range(
        root: ComponentId,
        next: u64,
        end: u64,
        labels: SidebarObserverLabels,
    ) -> Result<Self, &'static str> {
        if next > end {
            return Err("invalid Sidebar observer ID range");
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
            source: None,
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

    /// Replace the displayed source. A new source retires every old control
    /// while the monotonically increasing allocator prevents raw-ID reuse.
    pub fn replace_source(
        &mut self,
        input: &SidebarObserverStatusInput,
    ) -> Result<(), &'static str> {
        self.validate_input(input)?;
        if self.source.as_ref() == Some(&input.source) {
            self.validate_revision(input)?;
            return self.install(input);
        }

        self.ensure_capacity(input.endpoints.len())?;
        self.controls.clear();
        self.seen.clear();
        self.source = Some(input.source.clone());
        for endpoint in &input.endpoints {
            self.create_controls(endpoint)?;
            self.seen
                .insert(endpoint.key.endpoint_id().to_owned(), endpoint.clone());
        }
        self.displayed = Some(input.clone());
        self.rebuild();
        Ok(())
    }

    /// Update the active source. Source replacement must be explicit.
    pub fn update_from(&mut self, input: &SidebarObserverStatusInput) -> Result<(), &'static str> {
        self.validate_input(input)?;
        if self.source.as_ref() != Some(&input.source) {
            return Err("Sidebar observer source changed; replace source explicitly");
        }
        self.validate_revision(input)?;
        self.install(input)
    }

    pub fn displayed(&self) -> Option<&SidebarObserverStatusInput> {
        self.displayed.as_ref()
    }

    pub fn source(&self) -> Option<&SidebarObserverSourceKey> {
        self.source.as_ref()
    }

    /// Return a retry button only for the currently displayed failed attempt.
    pub fn retry_id(&self, endpoint_id: &str) -> Option<ComponentId> {
        let endpoint = self
            .displayed
            .as_ref()?
            .endpoints
            .iter()
            .find(|endpoint| endpoint.key.endpoint_id() == endpoint_id)?;
        if endpoint.observation.phase != SidebarObserverPhase::Failed {
            return None;
        }
        let controls = self.controls.get(endpoint_id)?;
        (controls.key == endpoint.key && !controls.pending).then_some(controls.retry)
    }

    /// Capture a retry intent only from the original button for the displayed
    /// failed generation. A host rejection leaves the control pending until a
    /// newer publication or explicit source replacement arrives.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<SidebarObserverIntent> {
        let Some(displayed) = self.displayed.as_ref() else {
            return Vec::new();
        };
        let mut intents = Vec::new();
        for action in actions {
            if action.kind != ContentActionKind::Activate {
                continue;
            }
            let Some((endpoint_id, controls)) = self
                .controls
                .iter_mut()
                .find(|(_, controls)| controls.retry == action.target)
            else {
                continue;
            };
            if controls.pending {
                continue;
            }
            let Some(endpoint) = displayed
                .endpoints
                .iter()
                .find(|endpoint| endpoint.key.endpoint_id() == endpoint_id)
            else {
                continue;
            };
            if controls.key != endpoint.key
                || endpoint.observation.phase != SidebarObserverPhase::Failed
            {
                continue;
            }
            controls.pending = true;
            intents.push(SidebarObserverIntent::Retry {
                key: endpoint.key.clone(),
            });
        }
        if !intents.is_empty() {
            self.rebuild();
        }
        intents
    }

    fn validate_input(&self, input: &SidebarObserverStatusInput) -> Result<(), &'static str> {
        input
            .validate()
            .map_err(|_| "invalid Sidebar observer status input")
    }

    fn validate_revision(&self, input: &SidebarObserverStatusInput) -> Result<(), &'static str> {
        let Some(current) = self.displayed.as_ref() else {
            return Ok(());
        };
        if input.revision < current.revision {
            return Err("Sidebar observer revision moved backwards");
        }
        if input.revision == current.revision && input != current {
            return Err("Sidebar observer revision was reused");
        }
        for endpoint in &input.endpoints {
            let endpoint_id = endpoint.key.endpoint_id();
            let Some(previous) = self.seen.get(endpoint_id) else {
                continue;
            };
            if endpoint.key.generation() < previous.key.generation()
                || (endpoint.key.generation() == previous.key.generation()
                    && endpoint.key != previous.key)
            {
                return Err("retired Sidebar observer generation");
            }
            if endpoint.revision < previous.revision
                || (endpoint.revision == previous.revision && endpoint != previous)
            {
                return Err("Sidebar observer endpoint revision was reused");
            }
        }
        Ok(())
    }

    fn ensure_capacity(&self, count: usize) -> Result<(), &'static str> {
        let required = (count as u64)
            .checked_mul(3)
            .ok_or("Sidebar observer ID range exhausted")?;
        let next_end = self
            .next
            .checked_add(required)
            .ok_or("Sidebar observer ID range exhausted")?;
        if next_end > self.end {
            return Err("Sidebar observer ID range exhausted");
        }
        Ok(())
    }

    fn install(&mut self, input: &SidebarObserverStatusInput) -> Result<(), &'static str> {
        let mut needed = 0usize;
        for endpoint in &input.endpoints {
            let endpoint_id = endpoint.key.endpoint_id();
            if self
                .controls
                .get(endpoint_id)
                .is_none_or(|controls| controls.key != endpoint.key)
            {
                needed += 1;
            }
        }
        let required = (needed as u64)
            .checked_mul(3)
            .ok_or("Sidebar observer ID range exhausted")?;
        let next_end = self
            .next
            .checked_add(required)
            .ok_or("Sidebar observer ID range exhausted")?;
        if next_end > self.end {
            return Err("Sidebar observer ID range exhausted");
        }

        for endpoint in &input.endpoints {
            let endpoint_id = endpoint.key.endpoint_id();
            let replace = self
                .controls
                .get(endpoint_id)
                .is_none_or(|controls| controls.key != endpoint.key);
            if replace {
                self.create_controls(endpoint)?;
            }
            self.seen.insert(endpoint_id.to_owned(), endpoint.clone());
        }
        let present: std::collections::BTreeSet<_> = input
            .endpoints
            .iter()
            .map(|endpoint| endpoint.key.endpoint_id())
            .collect();
        self.controls
            .retain(|endpoint_id, _| present.contains(endpoint_id.as_str()));
        self.displayed = Some(input.clone());
        self.rebuild();
        Ok(())
    }

    fn create_controls(
        &mut self,
        endpoint: &SidebarObserverEndpointInput,
    ) -> Result<(), &'static str> {
        let row = ComponentId(self.allocate_id()?);
        let status = ComponentId(self.allocate_id()?);
        let retry = ComponentId(self.allocate_id()?);
        self.controls.insert(
            endpoint.key.endpoint_id().to_owned(),
            EndpointControls {
                key: endpoint.key.clone(),
                row,
                status,
                retry,
                pending: false,
            },
        );
        Ok(())
    }

    fn allocate_id(&mut self) -> Result<u64, &'static str> {
        if self.next >= self.end {
            return Err("Sidebar observer ID range exhausted");
        }
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Sidebar observer ID range exhausted")?;
        Ok(id)
    }

    fn rebuild(&mut self) {
        let Some(displayed) = self.displayed.as_ref() else {
            self.body.children.clear();
            return;
        };
        let mut old: BTreeMap<_, _> = std::mem::take(&mut self.body.children)
            .into_iter()
            .map(|node| (node.id(), node))
            .collect();
        let mut children = Vec::new();
        for endpoint in &displayed.endpoints {
            let Some(controls) = self.controls.get(endpoint.key.endpoint_id()) else {
                continue;
            };
            if endpoint.observation.phase == SidebarObserverPhase::Observing
                && endpoint.observation.converged
            {
                continue;
            }
            let mut message = match endpoint.observation.phase {
                SidebarObserverPhase::Observing => self.labels.observing.clone(),
                SidebarObserverPhase::Restarting => self.labels.restarting.clone(),
                SidebarObserverPhase::Failed => self.labels.failed.clone(),
            };
            if let Some(error) = &endpoint.error
                && !error.is_empty()
            {
                message = format!("{error}\n{message}");
            }
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
            let mut status = parts
                .remove(&controls.status)
                .unwrap_or_else(|| Content::new(controls.status, TextContent::new("")));
            status.component_mut::<TextContent>().unwrap().text =
                format!("{}: {message}", endpoint.key.endpoint_id());
            row.children.push(status);
            if endpoint.observation.phase == SidebarObserverPhase::Failed {
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

impl Component for SidebarObserverStatus {
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
                top: 0.0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tekes_agent_presentation::{
        SidebarObservation, SidebarObserverKey, SidebarObserverSourceKey,
    };
    use tekes_ui_controls::{ButtonTextAlign, ButtonTextRenderer};
    use tekes_ui_scene::Scene;
    use tekes_ui_tokens::{Rgba, Theme, proposal};

    #[derive(Default)]
    struct Text;

    impl ButtonTextRenderer for Text {
        fn measure(&mut self, text: &str, _: proposal::TextRole, max_width: f32) -> Size {
            Size::new((text.chars().count() as f32 * 8.0).min(max_width), 20.0)
        }

        fn draw(
            &mut self,
            _: &mut Scene,
            _: &str,
            _: proposal::TextRole,
            _: Rect,
            _: ButtonTextAlign,
            _: Rgba,
        ) {
        }
    }

    fn endpoint(
        source_owner: &Arc<()>,
        observer_owner: &Arc<()>,
        endpoint_id: &str,
        generation: u64,
        revision: u64,
        lifecycle: (SidebarObserverPhase, bool),
        error: Option<&str>,
    ) -> SidebarObserverEndpointInput {
        SidebarObserverEndpointInput {
            key: SidebarObserverKey::from_owners(
                source_owner,
                observer_owner,
                endpoint_id,
                generation,
            )
            .unwrap(),
            revision,
            observation: SidebarObservation {
                generation,
                phase: lifecycle.0,
                converged: lifecycle.1,
            },
            error: error.map(str::to_owned),
        }
    }

    fn input(
        source_owner: &Arc<()>,
        revision: u64,
        endpoints: Vec<SidebarObserverEndpointInput>,
    ) -> SidebarObserverStatusInput {
        SidebarObserverStatusInput {
            source: SidebarObserverSourceKey::from_owner(source_owner),
            revision,
            endpoints,
        }
    }

    fn activate(id: ComponentId) -> ContentAction {
        ContentAction {
            target: id,
            kind: ContentActionKind::Activate,
            source: ContentActionSource::Pointer,
        }
    }

    fn failed_endpoint(
        owner: &Arc<()>,
        observer: &Arc<()>,
        generation: u64,
    ) -> SidebarObserverEndpointInput {
        endpoint(
            owner,
            observer,
            "native",
            generation,
            1,
            (SidebarObserverPhase::Failed, true),
            Some("读取失败"),
        )
    }

    #[test]
    fn retry_is_single_use_and_pending_survives_other_endpoint_updates() {
        let owner = Arc::new(());
        let native = Arc::new(());
        let pi = Arc::new(());
        let a = failed_endpoint(&owner, &native, 1);
        let b = endpoint(
            &owner,
            &pi,
            "pi",
            1,
            1,
            (SidebarObserverPhase::Observing, true),
            None,
        );
        let mut component = SidebarObserverStatus::new(22, SidebarObserverLabels::default());
        component
            .replace_source(&input(&owner, 1, vec![a.clone(), b.clone()]))
            .unwrap();
        let retry = component.retry_id("native").unwrap();
        let intent = component.take_intents(&[activate(retry), activate(retry)]);
        assert_eq!(intent.len(), 1);
        assert!(component.retry_id("native").is_none());

        let b_updated = endpoint(
            &owner,
            &pi,
            "pi",
            1,
            2,
            (SidebarObserverPhase::Observing, true),
            Some("仍在同步"),
        );
        component
            .update_from(&input(&owner, 2, vec![a, b_updated]))
            .unwrap();
        assert!(component.retry_id("native").is_none());
        assert!(component.take_intents(&[activate(retry)]).is_empty());
    }

    #[test]
    fn a_new_source_retires_old_raw_button_ids() {
        let first_owner = Arc::new(());
        let second_owner = Arc::new(());
        let observer = Arc::new(());
        let mut component = SidebarObserverStatus::new(23, SidebarObserverLabels::default());
        component
            .replace_source(&input(
                &first_owner,
                1,
                vec![failed_endpoint(&first_owner, &observer, 1)],
            ))
            .unwrap();
        let old_id = component.retry_id("native").unwrap();
        assert!(
            component
                .update_from(&input(
                    &second_owner,
                    1,
                    vec![failed_endpoint(&second_owner, &observer, 1)],
                ))
                .is_err()
        );
        component
            .replace_source(&input(
                &second_owner,
                1,
                vec![failed_endpoint(&second_owner, &observer, 1)],
            ))
            .unwrap();
        let new_id = component.retry_id("native").unwrap();
        assert_ne!(old_id, new_id);
        assert!(component.take_intents(&[activate(old_id)]).is_empty());
        assert_eq!(component.take_intents(&[activate(new_id)]).len(), 1);
    }

    #[test]
    fn retired_generations_and_reused_endpoint_revisions_are_rejected() {
        let owner = Arc::new(());
        let observer = Arc::new(());
        let mut component = SidebarObserverStatus::new(24, SidebarObserverLabels::default());
        let current = failed_endpoint(&owner, &observer, 2);
        component
            .replace_source(&input(&owner, 4, vec![current.clone()]))
            .unwrap();

        let retired = failed_endpoint(&owner, &observer, 1);
        assert!(
            component
                .update_from(&input(&owner, 5, vec![retired]))
                .is_err()
        );

        let changed_same_revision = endpoint(
            &owner,
            &observer,
            "native",
            2,
            1,
            (SidebarObserverPhase::Failed, true),
            Some("不同错误内容"),
        );
        assert!(
            component
                .update_from(&input(&owner, 5, vec![changed_same_revision]))
                .is_err()
        );
    }

    #[test]
    fn a_new_generation_gets_new_controls_and_retires_the_old_button() {
        let owner = Arc::new(());
        let observer = Arc::new(());
        let mut component = SidebarObserverStatus::new(27, SidebarObserverLabels::default());
        component
            .replace_source(&input(
                &owner,
                1,
                vec![failed_endpoint(&owner, &observer, 1)],
            ))
            .unwrap();
        let old_id = component.retry_id("native").unwrap();

        let mut next_generation = endpoint(
            &owner,
            &observer,
            "native",
            2,
            1,
            (SidebarObserverPhase::Failed, true),
            Some("新一代监听失败"),
        );
        assert!(
            component
                .update_from(&input(&owner, 2, vec![next_generation.clone()]))
                .is_err(),
            "new generation cannot reset the original child publication revision"
        );
        next_generation.revision = 2;
        component
            .update_from(&input(&owner, 2, vec![next_generation]))
            .unwrap();
        let current_id = component.retry_id("native").unwrap();
        assert_ne!(old_id, current_id);
        assert!(component.take_intents(&[activate(old_id)]).is_empty());
        assert_eq!(component.take_intents(&[activate(current_id)]).len(), 1);
    }

    #[test]
    fn same_aggregate_revision_with_different_content_is_rejected() {
        let owner = Arc::new(());
        let observer = Arc::new(());
        let mut component = SidebarObserverStatus::new(25, SidebarObserverLabels::default());
        component
            .replace_source(&input(
                &owner,
                1,
                vec![failed_endpoint(&owner, &observer, 1)],
            ))
            .unwrap();
        let changed = endpoint(
            &owner,
            &observer,
            "native",
            1,
            2,
            (SidebarObserverPhase::Failed, true),
            Some("不同内容"),
        );
        assert!(
            component
                .update_from(&input(&owner, 1, vec![changed]))
                .is_err()
        );
    }

    #[test]
    fn standalone_component_mounts_and_lays_out_with_sdk_controls() {
        let owner = Arc::new(());
        let observer = Arc::new(());
        let mut component = SidebarObserverStatus::new(26, SidebarObserverLabels::default());
        component
            .replace_source(&input(
                &owner,
                1,
                vec![failed_endpoint(&owner, &observer, 1)],
            ))
            .unwrap();
        let mut host = component.into_host().unwrap();
        let mut scene = Scene::new();
        let bounds = Rect::new(0.0, 0.0, 320.0, 90.0);
        host.render(&mut scene, &mut Text, bounds, bounds, Theme::LIGHT.into())
            .unwrap();
        let actions = host.drain_actions();
        assert!(actions.is_empty());
    }
}
