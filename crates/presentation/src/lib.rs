//! Renderer-independent Run, Transcript, Composer and connection state.
//! Harness facts come exclusively from the public session projection; local intent never gains authority.
mod annotations;
mod attachment_transfer;
pub use attachment_transfer::*;
mod composer;
pub use annotations::*;
mod thread_actions;
pub use thread_actions::*;
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

mod bounded_history;

mod thread_preferences;
pub use thread_preferences::*;

mod thread_activity;
pub use thread_activity::*;
mod actionable;
pub use actionable::*;

mod transcript_document;
pub use transcript_document::*;
