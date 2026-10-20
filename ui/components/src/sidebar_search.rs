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

/// Capture the source, endpoint epoch and query attempt, including ABA queries.
/// `owner` identifies one search source lifetime; generations are never recycled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSearchKey {
    pub owner: String,
    pub endpoint_id: String,
    pub connection_epoch: u64,
    pub generation: u64,
    pub query: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SidebarSearchPhase {
    #[default]
    Idle,
    Searching,
    Complete {
        has_more: bool,
    },
    Failed {
        message: String,
        has_more: bool,
        retryable: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSearchInput {
    /// Monotonic display publication, independent from inventory publications.
    pub revision: u64,
    /// None means search was cleared. No result or callback survives clearing.
    pub key: Option<SidebarSearchKey>,
    pub phase: SidebarSearchPhase,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarSearchIntent {
    Retry { key: SidebarSearchKey },
}
#[derive(Clone, Debug)]
pub struct SidebarSearchLabels {
    pub name: String,
    pub searching: String,
    pub retry: String,
    pub more: String,
}
impl Default for SidebarSearchLabels {
    fn default() -> Self {
        Self {
            name: "会话搜索状态".into(),
            searching: "正在搜索…".into(),
            retry: "重试".into(),
            more: "还有更多匹配结果，请缩小搜索范围。".into(),
        }
    }
}
impl SidebarSearchInput {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
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
    pub(crate) fn notice(&self, labels: &SidebarSearchLabels) -> Option<String> {
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
    pub(crate) fn retry_key(&self) -> Option<&SidebarSearchKey> {
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
