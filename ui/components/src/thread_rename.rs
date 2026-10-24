//! Thread rename gesture over shared action definitions and upstream text input.
use tekes_agent_contract::{SessionAddress, ThreadActionDefinition};
use tekes_ui_controls::{
    ButtonTextRenderer, TextSurface,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq)]
pub struct RenameSubmission {
    pub generation: u64,
    pub attempt: u64,
    pub address: SessionAddress,
    pub definition: ThreadActionDefinition,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum RenameIntent {
    Submit(Box<RenameSubmission>),
    Dismiss,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameFailure {
    pub message: String,
    pub uncertain: bool,
}
impl RenameFailure {
    pub fn from_error(error: &tekes_agent_contract::Error) -> Self {
        Self {
            message: error.to_string(),
            uncertain: !matches!(
                error,
                tekes_agent_contract::Error::Remote(_)
                    | tekes_agent_contract::Error::Unsupported(_)
            ),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenameIds {
    pub root: ComponentId,
    pub editor: ComponentId,
    pub submit: ComponentId,
    pub dismiss: ComponentId,
    status: ComponentId,
}
impl RenameIds {
    pub fn new(namespace: u32) -> Self {
        let base = (namespace as u64) << 32;
        Self {
            root: ComponentId(base),
            editor: ComponentId(base + 1),
            submit: ComponentId(base + 2),
            dismiss: ComponentId(base + 3),
            status: ComponentId(base + 4),
        }
    }
}
#[derive(Clone, Debug)]
pub struct RenameLabels {
    pub name: String,
    pub title: String,
    pub submit: String,
    pub dismiss: String,
    pub pending: String,
}
impl Default for RenameLabels {
    fn default() -> Self {
        Self {
            name: "重命名会话".into(),
            title: "会话名称".into(),
            submit: "重命名".into(),
            dismiss: "关闭".into(),
            pending: "正在重命名".into(),
        }
    }
}
#[derive(Debug)]
struct Target {
    address: SessionAddress,
    definition: ThreadActionDefinition,
}
/// Owns only this transient rename gesture, not session titles or remote action
/// policy. The host owns RPC IDs and dispatch; inventory remains authoritative.
#[derive(Debug)]
pub struct ThreadRename {
    ids: RenameIds,
    labels: RenameLabels,
    body: StackContent,
    target: Option<Target>,
    generation: u64,
    sequence: u64,
    last: Option<RenameSubmission>,
    pending: bool,
}
impl ThreadRename {
    pub fn new(namespace: u32, labels: RenameLabels) -> Self {
        Self {
            ids: RenameIds::new(namespace),
            labels,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            target: None,
            generation: 0,
            sequence: 0,
            last: None,
            pending: false,
        }
    }
    pub fn ids(&self) -> RenameIds {
        self.ids
    }
    pub fn is_open(&self) -> bool {
        self.target.is_some()
    }
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    pub fn editor(&self) -> Option<&TextSurface> {
        self.body
            .children
            .first()?
            .component::<TextFieldContent>()
            .map(|field| &field.surface)
    }
    pub fn is_composing(&self) -> bool {
        self.editor()
            .is_some_and(|editor| editor.input.marked.is_some())
    }
    pub fn open(
        &mut self,
        address: SessionAddress,
        definition: ThreadActionDefinition,
        title: &str,
    ) -> Result<(), &'static str> {
        if self.is_composing() {
            return Err("finish rename composition before changing target");
        }
        if address.endpoint_id.is_empty()
            || address.session_id.is_empty()
            || definition.interaction != "rename"
            || !definition.enabled
        {
            return Err("rename action unavailable");
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("rename generation exhausted")?;
        let offset = self
            .generation
            .checked_mul(8)
            .filter(|offset| *offset < u32::MAX as u64 - 8)
            .ok_or("rename namespace exhausted")?;
        self.ids.editor = ComponentId(self.ids.root.0 + offset + 1);
        self.ids.submit = ComponentId(self.ids.root.0 + offset + 2);
        self.ids.dismiss = ComponentId(self.ids.root.0 + offset + 3);
        self.ids.status = ComponentId(self.ids.root.0 + offset + 4);
        self.target = Some(Target {
            address,
            definition,
        });
        self.pending = false;
        self.last = None;
        let mut editor = TextFieldContent::new(self.labels.title.clone(), title);
        editor.submit_policy = TextSubmitPolicy::Enter;
        self.body.children = vec![
            Content::new(self.ids.editor, editor),
            Content::new(
                self.ids.submit,
                ButtonContent::new(self.labels.submit.clone()),
            ),
            Content::new(
                self.ids.dismiss,
                ButtonContent::new(self.labels.dismiss.clone()),
            ),
            Content::new(self.ids.status, TextContent::new("")),
        ];
        self.refresh_controls();
        Ok(())
    }
    pub fn is_current(&self, ticket: &RenameSubmission) -> bool {
        self.target.is_some()
            && self.generation == ticket.generation
            && self.last.as_ref() == Some(ticket)
    }
    pub fn complete(
        &mut self,
        ticket: &RenameSubmission,
        result: Result<(), RenameFailure>,
    ) -> bool {
        if !self.is_current(ticket) {
            return false;
        }
        self.pending = false;
        match result {
            Ok(()) => {
                self.target = None;
                self.body.children.clear();
            }
            Err(error) => {
                if !error.uncertain {
                    self.last = None;
                }
                self.body.children[0]
                    .component_mut::<TextFieldContent>()
                    .unwrap()
                    .surface
                    .input
                    .disabled = false;
                self.body.children[3]
                    .component_mut::<TextContent>()
                    .unwrap()
                    .text = error.message;
            }
        }
        self.refresh_controls();
        true
    }
    /// A repeated exact failed payload retains its attempt identity. Editing its
    /// title or opening a new target creates a new attempt; no automatic retry.
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<RenameIntent> {
        self.refresh_controls();
        if self.target.is_none() || self.is_composing() {
            return vec![];
        }
        let mut intents = vec![];
        for action in actions {
            if action.target == self.ids.dismiss && action.kind == ContentActionKind::Activate {
                self.target = None;
                self.body.children.clear();
                self.pending = false;
                intents.push(RenameIntent::Dismiss);
                break;
            }
            if self.pending {
                continue;
            }
            let submit = (action.target == self.ids.submit
                && action.kind == ContentActionKind::Activate)
                || (action.target == self.ids.editor && action.kind == ContentActionKind::Submit);
            if !submit {
                continue;
            }
            let title = self.editor().unwrap().input.text.trim().to_owned();
            if title.is_empty() {
                continue;
            }
            let target = self.target.as_ref().unwrap();
            let ticket = if let Some(last) = self.last.as_ref().filter(|last| last.title == title) {
                last.clone()
            } else {
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .expect("rename attempt exhausted");
                RenameSubmission {
                    generation: self.generation,
                    attempt: self.sequence,
                    address: target.address.clone(),
                    definition: target.definition.clone(),
                    title,
                }
            };
            self.last = Some(ticket.clone());
            self.pending = true;
            self.body.children[0]
                .component_mut::<TextFieldContent>()
                .unwrap()
                .surface
                .input
                .disabled = true;
            self.body.children[3]
                .component_mut::<TextContent>()
                .unwrap()
                .text = self.labels.pending.clone();
            intents.push(RenameIntent::Submit(Box::new(ticket)));
        }
        self.refresh_controls();
        intents
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let mut host = ContentHost::new(Content::new(self.ids.root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    fn refresh_controls(&mut self) {
        if self.target.is_some() {
            let disabled = self.pending
                || self.is_composing()
                || self.editor().unwrap().input.text.trim().is_empty();
            self.body.children[1]
                .component_mut::<ButtonContent>()
                .unwrap()
                .button
                .state
                .disabled = disabled;
        }
    }
}
impl Component for ThreadRename {
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.refresh_controls();
        self.body.measure(text, available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        self.body.arrange(text, frame, clip);
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.labels.name)
    }
}
