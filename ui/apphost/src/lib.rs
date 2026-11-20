//! Reusable `tekes-ui-app-host` containers and shared workbench coordinator for
//! TekesAgentKit desktop consumers.

pub mod container;
pub mod process_components;
pub mod run;
pub mod sidebar;
pub mod summary;
pub mod workbench;

pub use container::{AgentContainerAdapter, ManagedAgentContainer, ResourceContextOps};
pub use process_components::{AgentProcessComponentDescriptor, CORE_PROCESS_COMPONENTS};
pub use run::{AgentRunContainer, ComposerContainer, FOCUS_SWITCH_COMMAND, TranscriptContainer};
pub use sidebar::SessionSidebarContainer;
pub use summary::SummaryContainer;
pub use workbench::{AppHostWakeHub, SharedAgentWorkbench};
