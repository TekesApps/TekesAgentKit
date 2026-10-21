//! Sidebar-owned search footer. Host search data is an explicit input; generic
//! text, buttons, focus and painting remain TekesUI controls.
use tekes_ui_controls::{
    ButtonTextRenderer,
    composition::*,
    extended::{Axis, Stack},
    layout::{ContainerLayout, Insets, MeasuredChild},
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

pub use tekes_agent_presentation::{
    SidebarSearchInput, SidebarSearchIntent, SidebarSearchKey, SidebarSearchPhase,
};
#[derive(Clone, Debug)]
pub struct SidebarSearchLabels {
    pub name: String,
    pub searching: String,
    pub retry: String,
    pub more: String,
    pub no_results: String,
    pub pending_results: String,
}
impl Default for SidebarSearchLabels {
    fn default() -> Self {
        Self {
            no_results: "没有匹配的会话".into(),
            pending_results: "搜索结果尚未就绪".into(),
            name: "会话搜索状态".into(),
            searching: "正在搜索…".into(),
            retry: "重试".into(),
            more: "还有更多匹配结果，请缩小搜索范围。".into(),
        }
    }
}
pub(crate) trait SearchInputView {
    fn validate(&self) -> Result<(), &'static str>;
    fn notice(&self, labels: &SidebarSearchLabels) -> Option<String>;
    fn retry_key(&self) -> Option<&SidebarSearchKey>;
}
impl SearchInputView for SidebarSearchInput {
    fn validate(&self) -> Result<(), &'static str> {
        if self.revision == 0 {
            return Err("unversioned sidebar search input");
        }
        match &self.key {
            None if self.phase != SidebarSearchPhase::Idle => {
                return Err("search status has no source");
            }
            Some(key)
                if key.owner.is_empty()
                    || key.endpoint_id.is_empty()
                    || key.generation == 0
                    || key.query.is_empty() =>
            {
                return Err("invalid sidebar search identity");
            }
            _ => {}
        }
        Ok(())
    }
    fn notice(&self, labels: &SidebarSearchLabels) -> Option<String> {
        self.key.as_ref()?;
        match &self.phase {
            SidebarSearchPhase::Idle => None,
            SidebarSearchPhase::Searching => Some(labels.searching.clone()),
            SidebarSearchPhase::Complete { has_more } => has_more.then(|| labels.more.clone()),
            SidebarSearchPhase::Failed {
                message, has_more, ..
            } => Some(if *has_more {
                format!("{message}\n{}", labels.more)
            } else {
                message.clone()
            }),
        }
    }
    fn retry_key(&self) -> Option<&SidebarSearchKey> {
        matches!(
            self.phase,
            SidebarSearchPhase::Failed {
                retryable: true,
                ..
            }
        )
        .then_some(self.key.as_ref())
        .flatten()
    }
}

#[derive(Debug)]
pub(crate) struct SearchFooter {
    pub body: StackContent,
    pub visible: bool,
    pub name: String,
}
impl SearchFooter {
    pub fn new(name: String) -> Self {
        Self {
            body: StackContent {
                model: Stack {
                    axis: Axis::Vertical,
                    gap: Spacing::Regular.value(),
                },
                flexible_child: None,
                children: vec![],
            },
            visible: false,
            name,
        }
    }
}
impl Component for SearchFooter {
    fn active(&self) -> bool {
        self.visible
    }
    fn children(&self) -> &[Content] {
        &self.body.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.body.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        if !self.visible {
            return MeasuredChild::new(Size::default());
        }
        let inset = Spacing::Regular.value();
        let inner = self.body.measure(
            text,
            Size::new((available.width - 2. * inset).max(0.), available.height),
        );
        MeasuredChild::new(Size::new(
            inner.size.width + 2. * inset,
            inner.size.height + inset,
        ))
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if !self.visible {
            return;
        }
        let inset = Spacing::Regular.value();
        let layout = ContainerLayout::new(
            frame,
            Insets::ZERO,
            Insets {
                left: inset,
                right: inset,
                bottom: inset,
                top: 0.,
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
        ContentSemantics::new(ContentRole::Group, &self.name)
    }
}

/// Query-scoped help on the original session action owner, part of the same
/// versioned SidebarInput as its row membership. No visible subtitle is added.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarSearchHelp {
    /// Search-result coherence is separate from inventory restoration. Pins
    /// remain available while a query or an unknown-hit inventory join waits.
    pub results_ready: bool,
    pub source: SidebarSearchInput,
    pub snippets: std::collections::BTreeMap<tekes_agent_contract::SessionAddress, String>,
}
/// The default published SDK cannot declare help on an original Content owner.
/// An explicit fixed candidate adapter enables this without a fallback engine.
pub fn sidebar_row_help_supported() -> bool {
    false
}
