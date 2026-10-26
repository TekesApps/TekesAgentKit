use std::{fs, path::PathBuf};
use tekes_agent_contract::MAX_UPLOAD_BYTES;
use tekes_agent_ui_demo::attachment_files::*;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Owned(PathBuf);
impl Owned {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "agentkit-file-read-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn host_reads_exact_bytes_with_basename_and_no_inline_image_assumption() {
    let dir = Owned::new();
    let path = dir.0.join("中文.png");
    fs::write(&path, b"not decoded as an image").unwrap();
    let files = read_selected_files(&[path]).unwrap();
    assert_eq!(files[0].name(), "中文.png");
    assert_eq!(files[0].media_type(), "application/octet-stream");
    assert_eq!(
        files[0].byte_count(),
        b"not decoded as an image".len() as u64
    );
}
#[test]
fn host_rejects_empty_nonregular_oversized_and_excessive_selection() {
    let dir = Owned::new();
    let empty = dir.0.join("empty");
    fs::write(&empty, []).unwrap();
    assert!(read_selected_files(&[empty]).is_err());
    assert!(read_selected_files(std::slice::from_ref(&dir.0)).is_err());
    assert!(read_selected_files(&[]).is_err());
    assert!(read_selected_files(&vec![dir.0.join("missing"); MAX_SELECTED_FILES + 1]).is_err());
    let large = dir.0.join("large");
    fs::File::create(&large)
        .unwrap()
        .set_len(MAX_UPLOAD_BYTES as u64 + 1)
        .unwrap();
    assert!(read_selected_files(&[large]).is_err());
}
