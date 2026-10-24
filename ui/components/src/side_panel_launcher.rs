//! Agent pane destinations. Original SDK controls own input, focus, AX and paint.
use crate::RunScope;
use std::collections::BTreeMap;
use std::sync::Arc;
use tekes_ui_controls::{
    ButtonTextRenderer, composition::*, extended::Grid, layout::MeasuredChild,
};
use tekes_ui_scene::{Rect, Size};
use tekes_ui_tokens::Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SidePanelDestination {
    Summary,
    Review,
    Terminal,
    Browser,
    Files,
    SideChat,
}
impl SidePanelDestination {
    pub const ALL: [Self; 6] = [
        Self::Summary,
        Self::Review,
        Self::Terminal,
        Self::Browser,
        Self::Files,
        Self::SideChat,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Review => "Review",
            Self::Terminal => "Terminal",
            Self::Browser => "Browser",
            Self::Files => "Files",
            Self::SideChat => "SideChat",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidePanelLauncherOwner {
    Run(RunScope),
    Workspace { id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidePanelDestinationItem {
    pub destination: SidePanelDestination,
    pub title: String,
    pub available: bool,
    pub enabled: bool,
}
impl SidePanelDestinationItem {
    /// Availability is opt-in. A label never grants a product capability.
    pub fn unavailable(destination: SidePanelDestination) -> Self {
        Self {
            destination,
            title: destination.title().into(),
            available: false,
            enabled: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidePanelLauncherInput {
    pub revision: u64,
    pub owner: SidePanelLauncherOwner,
    /// Order/slots are explicit. Unavailable entries keep their Source grid slot.
    pub destinations: Vec<SidePanelDestinationItem>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidePanelLauncherIntent {
    pub owner: SidePanelLauncherOwner,
    pub destination: SidePanelDestination,
    root: ComponentId,
    revision: u64,
    target: ComponentId,
    authority: LauncherAuthority,
}
#[derive(Clone, Debug)]
struct LauncherAuthority(Arc<()>);
impl PartialEq for LauncherAuthority {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for LauncherAuthority {}
/// Empty pane launcher. No Run, navigation, permission or execution state machine.
#[derive(Debug)]
pub struct SidePanelLauncher {
    root: ComponentId,
    name: String,
    input: Option<SidePanelLauncherInput>,
    children: Vec<Content>,
    targets: BTreeMap<SidePanelDestination, ComponentId>,
    next: u64,
    row_height: f32,
    metric: ButtonContent,
    authority: LauncherAuthority,
}
impl SidePanelLauncher {
    pub fn new(namespace: u32, name: impl Into<String>) -> Self {
        let root = ComponentId((namespace as u64) << 32);
        Self {
            root,
            name: name.into(),
            input: None,
            children: vec![],
            targets: BTreeMap::new(),
            next: root.0 + 1,
            row_height: 0.,
            metric: ButtonContent::new("M"),
            authority: LauncherAuthority(Arc::new(())),
        }
    }
    pub fn id(&self) -> ComponentId {
        self.root
    }
    pub fn displayed_input(&self) -> Option<&SidePanelLauncherInput> {
        self.input.as_ref()
    }
    pub fn button_id(&self, destination: SidePanelDestination) -> Option<ComponentId> {
        self.targets.get(&destination).copied()
    }
    pub(crate) fn retire_controls(&mut self) {
        self.children.clear();
        self.targets.clear();
    }
    pub(crate) fn update_hidden(
        &mut self,
        input: &SidePanelLauncherInput,
    ) -> Result<(), &'static str> {
        self.validate_input(input)?;
        self.retire_controls();
        self.input = Some(input.clone());
        Ok(())
    }
    pub fn into_host(self) -> Result<ContentHost, CompositionError> {
        let root = self.id();
        let mut host = ContentHost::new(Content::new(root, self))?;
        host.set_action_events(true);
        Ok(host)
    }
    pub fn validate_input(&self, input: &SidePanelLauncherInput) -> Result<(), &'static str> {
        if input.revision == 0
            || self.input.as_ref().is_some_and(|old| {
                input.revision < old.revision || (input.revision == old.revision && input != old)
            })
        {
            return Err("stale launcher input");
        }
        match &input.owner {
            SidePanelLauncherOwner::Run(scope) => {
                scope.validate().map_err(|_| "invalid launcher Run owner")?
            }
            SidePanelLauncherOwner::Workspace { id } if id.trim().is_empty() => {
                return Err("empty launcher workspace owner");
            }
            _ => {}
        }
        let mut seen = std::collections::BTreeSet::new();
        if input.destinations.len() > 6
            || input
                .destinations
                .iter()
                .any(|item| item.title.trim().is_empty() || !seen.insert(item.destination))
        {
            return Err("invalid or duplicate launcher destinations");
        }
        let changed = self
            .input
            .as_ref()
            .is_none_or(|old| old.owner != input.owner);
        let needed = input
            .destinations
            .iter()
            .filter(|item| {
                item.available && (changed || !self.targets.contains_key(&item.destination))
            })
            .count() as u64;
        self.next
            .checked_add(needed)
            .filter(|next| *next <= (self.root.0 | u32::MAX as u64))
            .ok_or("launcher namespace exhausted")?;
        Ok(())
    }
    pub fn update(&mut self, input: &SidePanelLauncherInput) -> Result<(), &'static str> {
        self.validate_input(input)?;
        let changed = self
            .input
            .as_ref()
            .is_none_or(|old| old.owner != input.owner);
        let mut old: BTreeMap<_, _> = std::mem::take(&mut self.children)
            .into_iter()
            .map(|child| (child.id(), child))
            .collect();
        let mut targets = BTreeMap::new();
        for item in input.destinations.iter().filter(|item| item.available) {
            let id = if !changed && let Some(id) = self.targets.get(&item.destination) {
                *id
            } else {
                let id = ComponentId(self.next);
                self.next += 1;
                id
            };
            let mut child = old
                .remove(&id)
                .unwrap_or_else(|| Content::new(id, ButtonContent::new(&item.title)));
            let button = child.component_mut::<ButtonContent>().unwrap();
            button.label = item.title.clone();
            button.button.state.disabled = !item.enabled;
            self.children.push(child);
            targets.insert(item.destination, id);
        }
        self.targets = targets;
        self.input = Some(input.clone());
        Ok(())
    }
    pub fn accepts(&self, intent: &SidePanelLauncherIntent) -> bool {
        self.authority == intent.authority
            && self.root == intent.root
            && self.input.as_ref().is_some_and(|input| {
                input.owner == intent.owner
                    && input.revision == intent.revision
                    && input.destinations.iter().any(|item| {
                        item.destination == intent.destination && item.available && item.enabled
                    })
            })
            && self.button_id(intent.destination) == Some(intent.target)
    }
    /// Drain original SDK actions before updating this controlled input.
    pub fn take_intents(&self, actions: &[ContentAction]) -> Vec<SidePanelLauncherIntent> {
        let Some(input) = &self.input else {
            return vec![];
        };
        actions
            .iter()
            .filter(|action| action.kind == ContentActionKind::Activate)
            .filter_map(|action| {
                let item = input.destinations.iter().find(|item| {
                    item.available
                        && item.enabled
                        && self.button_id(item.destination) == Some(action.target)
                })?;
                Some(SidePanelLauncherIntent {
                    owner: input.owner.clone(),
                    destination: item.destination,
                    root: self.root,
                    revision: input.revision,
                    target: action.target,
                    authority: self.authority.clone(),
                })
            })
            .collect()
    }
}
impl Component for SidePanelLauncher {
    fn children(&self) -> &[Content] {
        &self.children
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Content>> {
        Some(&mut self.children)
    }
    fn measure(&mut self, text: &mut dyn ButtonTextRenderer, available: Size) -> MeasuredChild {
        self.row_height = self.metric.measure(text, available).size.height;
        for child in &mut self.children {
            child.measure(text, available);
        }
        MeasuredChild::new(available)
    }
    fn arrange(&mut self, text: &mut dyn ButtonTextRenderer, frame: Rect, clip: Rect) {
        let gap = Spacing::Relaxed.value();
        let available = (frame.size.width - Spacing::Section.value() * 2.).max(0.);
        let width = (self.row_height * 4.).min(((available - gap * 2.) / 3.).max(0.));
        let height = (self.row_height * 3.).min(((frame.size.height - gap) / 2.).max(0.));
        let x = frame.min_x() + (frame.size.width - width * 3. - gap * 2.) / 2.;
        let y = frame.min_y() + ((frame.size.height - height * 2. - gap) / 2.).max(0.);
        if let Some(input) = &self.input {
            // AgentKit supplies Source bounds/caps; the public SDK owns cell
            // placement and gap normalization, including hidden grid slots.
            let cells = Grid {
                columns: 3,
                gap_x: gap,
                gap_y: gap,
            }
            .layout(
                Rect::new(x, y, width * 3. + gap * 2., height * 2. + gap),
                input.destinations.len(),
                height,
            );
            for (index, item) in input.destinations.iter().enumerate() {
                if let Some(id) = self.targets.get(&item.destination)
                    && let Some(child) = self.children.iter_mut().find(|child| child.id() == *id)
                {
                    child.arrange(text, cells[index], clip);
                }
            }
        }
    }
    fn semantics(&self) -> ContentSemantics {
        ContentSemantics::new(ContentRole::Group, &self.name)
    }
}
