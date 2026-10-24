pub mod endpoint_config;
pub mod feedback_read_proof;
pub mod feedback_write_proof;
pub mod fixture;
pub mod mixed_tool_proof;
pub mod sidechat_proof;
pub mod transcript_coverage;

pub mod actionable_proof;
pub mod attachment_files;
pub mod component_demo;
pub mod file_preview_host;
pub mod notifications;
pub mod sidebar_delivery_receipt;
pub mod sidebar_host;
pub mod upload_proof;

pub mod history_proof;

pub mod turn_footnote_proof;

pub mod readonly_child_proof;

pub mod feedback_controls_proof;

pub mod feedback_transcript_proof;

#[cfg(target_os = "macos")]
pub mod label_host;

pub mod footnote_host;

pub mod child_run_proof;
