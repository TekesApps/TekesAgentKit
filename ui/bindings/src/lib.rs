//! Platform-free and renderer-free UI bindings connecting TekesAgentKit presentation
//! and contract services (`SessionRuntime`, `RunController`, `SideChatController`,
//! `WorkspaceServiceEndpoint`, normalized settings) with desktop hosts.

pub mod fixture_sep;
pub mod run;

pub use fixture_sep::{
    CHILD_SUBAGENT_SESSION_ID, DEFAULT_MAIN_SESSION_ID, FixtureSessionEndpoint,
    SECONDARY_SESSION_ID,
};
pub use run::{
    AVAILABLE_MODELS, AVAILABLE_PRESETS, RunWorkbenchBinding, SidebarFilterMode, SidebarSessionRow,
};

use tekes_agent_contract::{ArtifactSelectionReference, SessionAddress};
use tekes_agent_presentation::{ComposerDestination, RunScope};

/// Background UI wake bridge implemented by host shells (`AppHost` registers `WindowWake`).
pub trait UiWakeNotifier: Send + Sync {
    fn wake(&self);
}

/// No-op wake notifier for purely synchronous unit tests.
#[derive(Debug, Default)]
pub struct NoopWakeNotifier;

impl UiWakeNotifier for NoopWakeNotifier {
    fn wake(&self) {}
}

/// Shared request to inspect an artifact file inside an `Others` viewer instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactOpenRequest {
    pub workspace_id: String,
    pub path: String,
    pub revision: Option<String>,
    pub line_hint: Option<u64>,
    pub source_scope: Option<RunScope>,
}

/// Request to open or focus a child/subagent run trace viewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildRunOpenRequest {
    pub scope: RunScope,
    pub task_id: String,
    pub child_session_id: String,
    pub title: String,
}

/// Request to open or focus a temporary side-chat branch panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideChatOpenRequest {
    pub parent_scope: RunScope,
    pub sidechat_id: String,
    pub title: String,
}

/// Request to insert a validated `ArtifactSelectionReference` excerpt into a Composer draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposerQuoteRequest {
    pub destination: Option<ComposerDestination>,
    pub reference: ArtifactSelectionReference,
    pub comment: String,
}

/// Typed cross-container intent enqueued by one container and consumed on subsequent wake/render
/// without borrowing sibling `ContentHost` instances directly.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkbenchCommand {
    SelectSession(SessionAddress),
    OpenArtifact(ArtifactOpenRequest),
    OpenChildRun(ChildRunOpenRequest),
    OpenSideChat(SideChatOpenRequest),
    QuoteToComposer(ComposerQuoteRequest),
    AttachFileToComposer {
        destination: Option<ComposerDestination>,
        path: String,
    },
    RequestReconnect,
    RequestLoadEarlier(RunScope),
}
