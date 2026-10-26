//! Agent Summary custom-row ownership. Generic layout/input stays with Content.
use crate::SummaryOwner;
use std::sync::Arc;
use tekes_ui_controls::{ButtonTextRenderer, composition::*, layout::MeasuredChild};
use tekes_ui_scene::{Rect, Scene, Size};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryCustomRowSpec {
    pub section: String,
    pub row: String,
    /// Semantic identity of the supplied host control. Change to retire it.
    pub content_key: String,
}
#[derive(Clone, Debug)]
pub struct SummaryCustomRowToken {
    pub(crate) summary: Arc<()>,
    pub(crate) owner: SummaryOwner,
    pub(crate) revision: u64,
    pub(crate) disclosure_epoch: u64,
    pub(crate) id: ComponentId,
    pub(crate) spec: SummaryCustomRowSpec,
}
impl SummaryCustomRowToken {
    pub fn owner(&self) -> &SummaryOwner {
        &self.owner
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn spec(&self) -> &SummaryCustomRowSpec {
        &self.spec
    }
    pub fn row_id(&self) -> ComponentId {
        self.id
    }
}
#[derive(Debug)]
pub(crate) struct SummaryCustomRow {
    pub children: Vec<Content>,
    pub placeholder: TextContent,
}
impl SummaryCustomRow {
    pub fn new(text: String) -> Self {
        Self {
            children: vec![],
            placeholder: TextContent::new(text),
        }
    }
}
impl Component for SummaryCustomRow {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, size: Size) -> MeasuredChild {
        let mut measured = if let Some(child) = self.children.first_mut() {
            child.measure(text, size)
        } else {
            self.placeholder.measure(text, size)
        };
        measured.size.width = size.width;
        measured
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        if let Some(child) = self.children.first_mut() {
            child.arrange(text, frame, clip);
        }
    }
    fn paint(
        &mut self,
        scene: &mut Scene,
        text: &mut dyn ButtonTextRenderer,
        frame: Rect,
        clip: Rect,
        theme: ContentTheme,
    ) {
        if self.children.is_empty() {
            self.placeholder.paint(scene, text, frame, clip, theme);
        }
    }
    fn semantics(&self) -> ContentSemantics {
        if self.children.is_empty() {
            self.placeholder.semantics()
        } else {
            ContentSemantics::new(ContentRole::Group, &self.placeholder.text)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SummaryCustomRowError {
    Agent(&'static str),
    Composition(CompositionError),
}
/// Install into the original host using the SDK's atomic tree/ID validation
/// and lifecycle. This does not build a second tree validator in AgentKit.
pub fn install_summary_custom_row(
    host: &mut ContentHost,
    root: ComponentId,
    token: &SummaryCustomRowToken,
    content: Content,
) -> Result<(), SummaryCustomRowError> {
    let view = host
        .get(root)
        .and_then(|node| node.component::<crate::SummaryView>())
        .ok_or(SummaryCustomRowError::Agent("Summary is not mounted"))?;
    view.check_custom_install(token, content.id())
        .map_err(SummaryCustomRowError::Agent)?;
    host.insert(token.id, content)
        .map_err(SummaryCustomRowError::Composition)?;
    host.get_mut(root)
        .unwrap()
        .component_mut::<crate::SummaryView>()
        .unwrap()
        .invalidate_custom_layout(token.id);
    Ok(())
}

/// Preflight the complete Summary display input, then retire removed/replaced
/// custom roots through the SDK lifecycle before publishing the new row graph.
/// Unchanged custom controls are never detached. No yield or product operation
/// occurs between structural preflight and application.
pub fn publish_summary_context_update(
    host: &mut ContentHost,
    root: ComponentId,
    input: &crate::SummaryContextInput,
) -> Result<(), SummaryCustomRowError> {
    let prepared = host
        .get(root)
        .and_then(|node| node.component::<crate::SummaryView>())
        .ok_or(SummaryCustomRowError::Agent("Summary is not mounted"))?
        .prepare_context(input)
        .map_err(SummaryCustomRowError::Agent)?;
    for id in &prepared.retiring {
        // Slot roots are direct children of our non-fixed non-action container;
        // the same-turn preflight proved their original membership.
        host.remove(*id)
            .expect("preflighted original Summary custom root");
    }
    host.get_mut(root)
        .unwrap()
        .component_mut::<crate::SummaryView>()
        .unwrap()
        .apply_context(input, prepared)
        .expect("complete same-turn Summary preflight");
    Ok(())
}
