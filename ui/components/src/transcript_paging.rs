//! Public history-page controls. Reading anchors and page merging are owned elsewhere.
use tekes_agent_presentation::RunScope;
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptPageRead {
    Idle,
    Loading,
    Failed(String),
    Interrupted,
}
/// Facts from the original binding and its canonical transcript; no local history cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptPagingFacts {
    pub scope: RunScope,
    pub supported: bool,
    pub has_more_before: bool,
    pub is_stale: bool,
    pub source_ready: bool,
    pub read: TranscriptPageRead,
}
impl TranscriptPagingFacts {
    pub fn can_load(&self) -> bool {
        self.supported
            && self.has_more_before
            && !self.is_stale
            && self.source_ready
            && self.read != TranscriptPageRead::Loading
    }
}
#[derive(Clone, Debug)]
pub struct TranscriptPagingLabels {
    pub group: String,
    pub earlier: String,
    pub retry: String,
    pub loading: String,
    pub interrupted: String,
    pub waiting: String,
}
impl Default for TranscriptPagingLabels {
    fn default() -> Self {
        Self {
            group: "历史分页".into(),
            earlier: "加载更早记录".into(),
            retry: "重试加载".into(),
            loading: "正在加载历史".into(),
            interrupted: "历史读取已中断".into(),
            waiting: "历史待同步".into(),
        }
    }
}
/// Mount per binding, using a unique namespace. It cannot be retargeted to another scope.
/// Activation returns the captured scope; immediately capture a HistoryPageRequest
/// on that same binding before handing work to an executor. No retry is automatic.
#[derive(Debug)]
pub struct TranscriptPaging {
    root: ComponentId,
    button: ComponentId,
    labels: TranscriptPagingLabels,
    scope: RunScope,
    facts: Option<TranscriptPagingFacts>,
    body: StackContent,
}
impl TranscriptPaging {
    pub fn new(namespace: u32, scope: RunScope, labels: TranscriptPagingLabels) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            root,
            button: ComponentId(root.0 + 1),
            labels,
            scope,
            facts: None,
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Tight.value(),
                },
                flexible_child: None,
                children: vec![],
            },
        }
    }
    pub fn button_id(&self) -> ComponentId {
        self.button
    }
    pub fn facts(&self) -> Option<&TranscriptPagingFacts> {
        self.facts.as_ref()
    }
    pub fn update(&mut self, facts: TranscriptPagingFacts) -> Result<(), &'static str> {
        if facts.scope != self.scope {
            return Err("history control scope mismatch");
        }
        if self.facts.as_ref() == Some(&facts) {
            return Ok(());
        }
        self.body.children.clear();
        if facts.supported {
            let status = if !facts.source_ready || facts.is_stale {
                Some(self.labels.waiting.clone())
            } else {
                match &facts.read {
                    TranscriptPageRead::Loading => Some(self.labels.loading.clone()),
                    TranscriptPageRead::Failed(message) => Some(message.clone()),
                    TranscriptPageRead::Interrupted => Some(self.labels.interrupted.clone()),
                    TranscriptPageRead::Idle => None,
                }
            };
            if let Some(status) = status {
                self.body.children.push(Content::new(
                    ComponentId(self.root.0 + 2),
                    TextContent::new(status),
                ));
            }
            if facts.has_more_before {
                let retry = facts.source_ready
                    && matches!(
                        facts.read,
                        TranscriptPageRead::Failed(_) | TranscriptPageRead::Interrupted
                    );
                let mut button = Button::new(if retry {
                    &self.labels.retry
                } else {
                    &self.labels.earlier
                });
                button.button.state.disabled = !facts.can_load();
                self.body.children.push(Content::new(self.button, button));
            }
        }
        self.facts = Some(facts);
        Ok(())
    }
    pub fn activation(&self, actions: &[ContentAction]) -> Option<&RunScope> {
        (self
            .facts
            .as_ref()
            .is_some_and(TranscriptPagingFacts::can_load)
            && actions.iter().any(|action| {
                action.target == self.button && action.kind == ContentActionKind::Activate
            }))
        .then_some(&self.scope)
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.root;
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
}
impl Component for TranscriptPaging {
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
