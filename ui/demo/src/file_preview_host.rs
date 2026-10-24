//! Host-only private staging for an explicit system viewer callback.
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tekes_agent_contract::{Result, require};
use tekes_agent_ui_bindings::FilePreview;
pub struct StagedFilePreview {
    directory: PathBuf,
    path: PathBuf,
}
impl StagedFilePreview {
    pub fn stage(preview: &FilePreview) -> Result<Self> {
        let name = preview.name();
        require(
            !name.trim().is_empty()
                && name != "."
                && name != ".."
                && !name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\'),
            "preview requires basename",
        )?;
        let directory = std::env::temp_dir().join(format!(
            "agentkit-file-preview-{}-{}-{}",
            std::process::id(),
            preview.request_id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&directory).map_err(io_error)?;
        let staged = Self {
            path: directory.join(name),
            directory,
        };
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&staged.path).map_err(io_error)?;
        file.write_all(preview.bytes()).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        Ok(staged)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for StagedFilePreview {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}
fn io_error(error: std::io::Error) -> tekes_agent_contract::Error {
    tekes_agent_contract::Error::Transport(format!("host preview staging: {error}"))
}
