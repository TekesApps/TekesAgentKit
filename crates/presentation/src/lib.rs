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
