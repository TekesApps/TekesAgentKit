//! Reusable `tekes-ui-app-host` containers and shared workbench coordinator for
//! TekesAgentKit desktop consumers.

pub mod artifact;
pub mod changes;
pub mod child_run;
pub mod container;
pub mod files;
pub mod process_components;
pub mod run;
pub mod settings;
pub mod sidebar;
pub mod sidechat;
pub mod summary;
pub mod tasks_queue;
pub mod workbench;

pub use artifact::{ArtifactViewerContainer, ArtifactViewerProvider};
pub use changes::WorkspaceChangesContainer;
pub use child_run::ChildRunContainer;
pub use container::{AgentContainerAdapter, ManagedAgentContainer, ResourceContextOps};
pub use files::WorkspaceFilesContainer;
pub use process_components::{AgentProcessComponentDescriptor, CORE_PROCESS_COMPONENTS};
pub use run::{
    AgentRunContainer, ComposerContainer, FOCUS_SWITCH_COMMAND, RUN_TOOLBAR_APPROVAL_CMD,
    RUN_TOOLBAR_EFFORT_CMD, RUN_TOOLBAR_HARNESS_CMD, RUN_TOOLBAR_MODEL_CMD, TranscriptContainer,
};
pub use settings::{
    AGENT_SETTINGS_SCHEMA_TOML, HarnessSettingsContainer, build_agent_settings_owner,
    build_agent_settings_owner_with_mode,
};
pub use sidebar::SessionSidebarContainer;
pub use sidechat::SideChatContainer;
pub use summary::SummaryContainer;
pub use tasks_queue::TasksQueueContainer;
pub use workbench::{AppHostWakeHub, SharedAgentWorkbench};
