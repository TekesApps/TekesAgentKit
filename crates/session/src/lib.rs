//! Headless session runtime and in-memory projection; no database or UI dependencies.
mod projection;
mod runtime;
pub use projection::*;
pub use runtime::*;
