//! Demo host filesystem adapter. No path/file access enters Kit components or bindings.
use std::{io::Read, path::PathBuf, sync::Arc};
use tekes_agent_contract::{MAX_UPLOAD_BYTES, Result, require};
use tekes_agent_ui_bindings::AttachmentFile;
/// Host memory limits, additional to the endpoint's own policy.
pub const MAX_SELECTED_FILES: usize = 32;
pub const MAX_SELECTED_BYTES: usize = 32 * 1024 * 1024;
pub fn read_selected_files(paths: &[PathBuf]) -> Result<Vec<AttachmentFile>> {
    require(
        !paths.is_empty() && paths.len() <= MAX_SELECTED_FILES,
        "host file count limit",
    )?;
    let mut files = vec![];
    let mut total = 0usize;
    for path in paths {
        require(
            std::fs::metadata(path).map_err(file_error)?.is_file(),
            "select regular files only",
        )?;
        let file = std::fs::File::open(path).map_err(file_error)?;
        require(
            file.metadata().map_err(file_error)?.is_file(),
            "selected file changed type",
        )?;
        require(
            file.metadata().map_err(file_error)?.len() <= MAX_UPLOAD_BYTES as u64,
            "upload limit",
        )?;
        let mut bytes = vec![];
        file.take(MAX_UPLOAD_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(file_error)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| tekes_agent_contract::unsupported("host byte count overflow"))?;
        require(total <= MAX_SELECTED_BYTES, "host selection byte limit")?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| tekes_agent_contract::unsupported("file basename is not UTF-8"))?;
        // Generic files need no image decoding or extension-based MIME claim.
        files.push(AttachmentFile::new(
            name.into(),
            "application/octet-stream".into(),
            Arc::from(bytes),
        )?);
    }
    Ok(files)
}

fn file_error(error: std::io::Error) -> tekes_agent_contract::Error {
    tekes_agent_contract::Error::Transport(format!("host file read: {error}"))
}
