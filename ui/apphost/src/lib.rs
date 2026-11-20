//! Reusable `tekes-ui-app-host` containers and shared workbench coordinator for
//! TekesAgentKit desktop consumers.

pub mod container;
pub mod workbench;

pub use container::{AgentContainerAdapter, ManagedAgentContainer, ResourceContextOps};
pub use workbench::{AppHostWakeHub, SharedAgentWorkbench};
