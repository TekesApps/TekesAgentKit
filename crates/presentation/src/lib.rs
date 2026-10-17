//! Renderer-independent Run, Transcript, Composer and connection state.
//! Harness facts come exclusively from the public session projection; local intent never gains authority.
mod composer;
mod controller;
mod run;
mod scope;
mod transcript;
pub use composer::*;
pub use controller::*;
pub use run::*;
pub use scope::*;
pub use transcript::*;

mod child;
mod sidechat;
mod summary;
mod task;
pub use child::*;
pub use sidechat::*;
pub use summary::*;
pub use task::*;
