//! Headless session runtime and in-memory projection; no database or UI dependencies.
mod client;
mod inventory_refresh;
mod native_assistant;
mod projection;
pub use native_assistant::NativeAssistantProjection;
mod runtime;
pub use projection::*;
pub use runtime::*;

mod registry;
pub use registry::SessionEndpointRegistry;
