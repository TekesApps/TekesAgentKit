//! Reusable Agent waiting-interaction content over public TekesUI controls.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tekes_agent_presentation::ActionablePrompt;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;
type ResponseReceipt = Arc<Mutex<Option<Result<(), String>>>>;
#[derive(Clone, Debug)]
pub struct ActionableResponseIntent {
    pub prompt: Arc<ActionablePrompt>,
    pub choice_id: String,
    pub epoch: u64,
    receipt: ResponseReceipt,
}
impl ActionableResponseIntent {
    /// Completes only this local operation, including while its component is
    /// retained offscreen. It never resolves a shared actionable or navigates.
    pub fn complete(&self, result: Result<(), String>) -> bool {
        let mut receipt = self.receipt.lock().expect("response receipt");
        if receipt.is_some() {
            return false;
        }
        *receipt = Some(result);
        true
    }
}
#[derive(Clone, Debug)]
pub struct ActionableLabels {
    pub group: String,
    pub unsupported: String,
    pub pending: String,
    pub uncertain: String,
}
impl Default for ActionableLabels {
    fn default() -> Self {
        Self {
            group: "等待处理".into(),
            unsupported: "此交互暂未支持；不会代答".into(),
            pending: "正在提交回答，等待状态更新".into(),
            uncertain: "回答结果未确认，请先核对；不会自动重试".into(),
        }
    }
}
/// One instance belongs to one mounted Run authority. New/retired prompts receive
/// fresh action IDs. Completion never resolves or removes canonical actionables.
#[derive(Debug)]
pub struct ActionableCard {
    root: ComponentId,
    next: u64,
    publication: u64,
    epoch: u64,
    prompt: Option<Arc<ActionablePrompt>>,
    controls: bool,
    attempted: bool,
    receipt: Option<ResponseReceipt>,
    rendered_receipt: Option<Result<(), String>>,
    rows: BTreeMap<ComponentId, String>,
    body: StackContent,
    labels: ActionableLabels,
}
impl ActionableCard {
    pub fn new(namespace: u32, labels: ActionableLabels) -> Self {
        Self {
            root: ComponentId((namespace as u64) << 32),
            next: 16,
            publication: 0,
            epoch: 0,
            prompt: None,
            controls: false,
            attempted: false,
            receipt: None,
            rendered_receipt: None,
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
    pub fn choice_id(&self, id: &str) -> Option<ComponentId> {
        self.rows
            .iter()
            .find_map(|(key, value)| (value == id).then_some(*key))
    }
    pub fn update(
        &mut self,
        publication: u64,
        epoch: u64,
        prompt: Option<Arc<ActionablePrompt>>,
        controls: bool,
    ) -> Result<bool, &'static str> {
        self.check_update(publication, epoch, prompt.as_ref())?;
        let changed = self.prompt != prompt || self.epoch != epoch;
        if changed {
            self.rows.clear();
            self.attempted = false;
            self.receipt = None;
            if let Some(prompt) = &prompt {
                for choice in &prompt.choices {
                    let id = ComponentId(self.root.0 + self.next);
                    self.next += 1;
                    self.rows.insert(id, choice.id.clone());
                }
            }
        }
        let result = self
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.lock().expect("response receipt").clone());
        let rebuild = changed || self.controls != controls || self.rendered_receipt != result;
        self.epoch = epoch;
        self.publication = publication;
        self.prompt = prompt;
        self.controls = controls;
        if rebuild {
            self.rebuild();
        }
        Ok(rebuild)
    }
    pub(crate) fn check_update(
        &self,
        publication: u64,
        epoch: u64,
        prompt: Option<&Arc<ActionablePrompt>>,
    ) -> Result<(), &'static str> {
        if publication == 0 || publication < self.publication {
            return Err("stale actionable publication");
        }
        let changed = self.prompt.as_ref() != prompt || self.epoch != epoch;
        if publication == self.publication && changed {
            return Err("conflicting actionable publication");
        }
        if changed
            && self
                .next
                .checked_add(prompt.map_or(0, |p| p.choices.len()) as u64)
                .is_none_or(|n| n >= u32::MAX as u64)
        {
            return Err("actionable namespace exhausted");
        }
        Ok(())
    }
    pub fn take_intents(&mut self, actions: &[ContentAction]) -> Vec<ActionableResponseIntent> {
        if !self.controls || self.attempted {
            return vec![];
        }
        let Some(prompt) = &self.prompt else {
            return vec![];
        };
        let selected = actions
            .iter()
            .filter(|a| a.kind == ContentActionKind::Activate)
            .find_map(|a| self.rows.get(&a.target));
        let Some(choice) = selected else {
            return vec![];
        };
        let receipt = Arc::new(Mutex::new(None));
        let intent = ActionableResponseIntent {
            prompt: prompt.clone(),
            choice_id: choice.clone(),
            epoch: self.epoch,
            receipt: receipt.clone(),
        };
        self.receipt = Some(receipt);
        self.attempted = true;
        self.rebuild();
        vec![intent]
    }
    pub fn complete(
        &mut self,
        intent: &ActionableResponseIntent,
        result: Result<(), String>,
    ) -> bool {
        if !self.attempted || self.prompt.as_ref() != Some(&intent.prompt) {
            return false;
        }
        if self
            .receipt
            .as_ref()
            .is_none_or(|receipt| !Arc::ptr_eq(receipt, &intent.receipt))
        {
            return false;
        }
        let accepted = intent.complete(result);
        self.rebuild();
        accepted
    }

    pub fn into_host(self) -> Result<ContentHost, &'static str> {
        let root = self.root;
        let mut host =
            ContentHost::new(Content::new(root, self)).map_err(|_| "actionable card mount")?;
        host.set_action_events(true);
        Ok(host)
    }
    fn rebuild(&mut self) {
        self.rendered_receipt = self
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.lock().expect("response receipt").clone());
        self.body.children.clear();
        let Some(prompt) = &self.prompt else { return };
        self.body.children.push(Content::new(
            ComponentId(self.root.0 + 1),
            TextContent::new(&prompt.title),
        ));
        if !prompt.detail.is_empty() {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 2),
                TextContent::new(&prompt.detail),
            ));
        }
        if prompt.choices.is_empty() {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 3),
                TextContent::new(&self.labels.unsupported),
            ));
        }
        if self.attempted {
            self.body.children.push(Content::new(
                ComponentId(self.root.0 + 4),
                TextContent::new(
                    if let Some(Err(error)) = self
                        .receipt
                        .as_ref()
                        .and_then(|receipt| receipt.lock().expect("response receipt").clone())
                    {
                        format!("{}：{error}", self.labels.uncertain)
                    } else {
                        self.labels.pending.clone()
                    },
                ),
            ));
        }
        for (id, choice) in &self.rows {
            if let Some(choice) = prompt.choice(choice) {
                let mut button = ButtonContent::new(&choice.label);
                button.button.state.disabled = !self.controls || self.attempted;
                self.body.children.push(Content::new(*id, button));
            }
        }
    }
}
impl Component for ActionableCard {
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
