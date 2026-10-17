//! Headless session runtime and in-memory projection; no database or UI dependencies.
mod client;
mod projection;
mod runtime;
pub use projection::*;
pub use runtime::*;

mod registry;
pub use registry::SessionEndpointRegistry;
