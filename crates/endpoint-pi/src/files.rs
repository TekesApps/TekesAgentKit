use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use tekes_agent_contract::*;
/// Local Pi only: explicit host-owned workspace capability, never a remote-path fallback.
pub(crate) fn resolve(root: &Path, path: &str) -> Result<PathBuf> {
    require(!path.contains('\0'), "invalid session file path")?;
    let root = root
        .canonicalize()
        .map_err(|_| unsupported("session directory unavailable"))?;
    let target = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    let target = target
        .canonicalize()
        .map_err(|_| unsupported("session file not found"))?;
    require(target.starts_with(&root), "session file escapes read root")?;
    Ok(target)
}
pub(crate) fn read(
    root: &Path,
    path: &str,
    offset: u64,
    length: usize,
) -> Result<SessionFileBytePage> {
    require(
        length > 0 && length <= 4 * 1024 * 1024,
        "invalid byte page length",
    )?;
    let path = resolve(root, path)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| unsupported("session directory unavailable"))?;
    let relative = path
        .strip_prefix(&canonical_root)
        .map_err(|_| unsupported("session file escapes read root"))?;
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&canonical_root, cap_std::ambient_authority())
            .map_err(|_| unsupported("session directory open"))?;
    let mut file = directory
        .open(relative)
        .map_err(|_| unsupported("session file open outside capability"))?
        .into_std();
    let before = file
        .metadata()
        .map_err(|_| unsupported("session file stat"))?;
    require(
        before.is_file() && offset <= before.len(),
        "invalid file or offset",
    )?;
    // Hash one descriptor for a stable version; do not reopen between version and payload reads.
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| unsupported("session file read"))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| unsupported("session file seek"))?;
    let mut data = vec![0; length.min((before.len() - offset) as usize)];
    file.read_exact(&mut data)
        .map_err(|_| unsupported("session file changed during read"))?;
    let after = file
        .metadata()
        .map_err(|_| unsupported("session file stat"))?;
    require(
        before.len() == after.len() && before.modified().ok() == after.modified().ok(),
        "session file changed during read",
    )?;
    Ok(SessionFileBytePage {
        absolute_path: path.to_string_lossy().into(),
        version: format!("sha256:{:x}", digest.finalize()),
        bytes: Some(before.len()),
        offset,
        eof: offset + data.len() as u64 == before.len(),
        data: STANDARD.encode(data),
    })
}
pub(crate) fn text(
    root: &Path,
    path: &str,
    offset: u64,
    limit: usize,
) -> Result<SessionFileTextPage> {
    require(
        offset >= 1 && limit > 0 && limit <= 10000,
        "invalid text page",
    )?;
    let target = resolve(root, path)?;
    let size = target
        .metadata()
        .map_err(|_| unsupported("session file stat"))?
        .len();
    require(
        size <= 4 * 1024 * 1024,
        "text file exceeds bounded preview; use bytes",
    )?;
    let page = read(root, path, 0, 4 * 1024 * 1024)?;
    let bytes = STANDARD
        .decode(&page.data)
        .map_err(|_| Error::Protocol("invalid byte page".into()))?;
    let text = String::from_utf8(bytes).map_err(|_| unsupported("file is not UTF-8 text"))?;
    let lines: Vec<_> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    };
    let start = usize::try_from(offset - 1).map_err(|_| unsupported("invalid offset"))?;
    require(start <= lines.len(), "line offset outside file")?;
    let end = start.saturating_add(limit).min(lines.len());
    Ok(SessionFileTextPage {
        absolute_path: page.absolute_path,
        version: page.version,
        bytes: page.bytes,
        offset,
        text: lines[start..end].join("\n"),
        lines: end - start,
        eof: end == lines.len(),
    })
}

fn fingerprints(root: &Path, path: &Path) -> Result<std::collections::BTreeMap<PathBuf, String>> {
    let mut pending = vec![(path.to_owned(), 0)];
    let mut values = std::collections::BTreeMap::new();
    while let Some((path, depth)) = pending.pop() {
        require(
            values.len() < 4096 && depth <= 16,
            "file observation coverage exceeds bound",
        )?;
        let metadata = match path.symlink_metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(unsupported("file observation stat")),
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        require(path.starts_with(root), "observation escapes root")?;
        values.insert(
            path.clone(),
            format!("poll:{}:{:?}", metadata.len(), metadata.modified().ok()),
        );
        if metadata.is_dir() {
            for entry in
                std::fs::read_dir(&path).map_err(|_| unsupported("file observation list"))?
            {
                let entry = entry.map_err(|_| unsupported("file observation entry"))?;
                if entry.file_name() == ".git" {
                    continue;
                }
                pending.push((entry.path(), depth + 1));
            }
        }
    }
    Ok(values)
}
pub(crate) async fn observe(
    root: PathBuf,
    path: &str,
    close: tokio::sync::watch::Receiver<bool>,
) -> Result<FrameStream> {
    use serde_json::json;
    let target = resolve(&root, path)?;
    let root = root.canonicalize().map_err(|_| unsupported("read root"))?;
    let r = root.clone();
    let p = target.clone();
    let initial = tokio::task::spawn_blocking(move || fingerprints(&r, &p))
        .await
        .map_err(|_| Error::Disconnected)??;
    let updates = futures_util::stream::try_unfold(
        (
            root,
            target,
            close,
            initial,
            std::collections::VecDeque::new(),
        ),
        |(root, target, mut close, mut previous, mut pending)| async move {
            loop {
                if let Some(frame) = pending.pop_front() {
                    return Ok(Some((frame, (root, target, close, previous, pending))));
                }
                tokio::select! {_=close.changed()=>return Ok(None),_=tokio::time::sleep(std::time::Duration::from_millis(250))=>{}}
                let r = root.clone();
                let p = target.clone();
                let current = tokio::task::spawn_blocking(move || fingerprints(&r, &p))
                    .await
                    .map_err(|_| Error::Disconnected)??;
                for (path, version) in &current {
                    if previous.get(path) != Some(version) {
                        pending.push_back(json!({"kind":"change","change":{"absolutePath":path,"version":version}}));
                    }
                }
                for path in previous.keys() {
                    if !current.contains_key(path) {
                        pending.push_back(
                            json!({"kind":"change","change":{"absolutePath":path,"absent":true}}),
                        );
                    }
                }
                previous = current;
            }
        },
    );
    use futures_util::StreamExt;
    Ok(Box::pin(
        futures_util::stream::once(async { Ok(json!({"kind":"ready"})) }).chain(updates),
    ))
}

pub(crate) fn candidates(root: &Path, query: &str) -> Result<serde_json::Value> {
    use serde_json::json;
    let root = root
        .canonicalize()
        .map_err(|_| unsupported("workspace unavailable"))?;
    let mut paths = vec![(root.clone(), 0)];
    let mut result = Vec::new();
    let mut inspected = 0;
    while let Some((directory, depth)) = paths.pop() {
        if depth > 16 {
            continue;
        }
        for entry in std::fs::read_dir(directory).map_err(|_| unsupported("file reference list"))? {
            inspected += 1;
            if inspected > 10000 || result.len() >= 100 {
                return Ok(json!(result));
            }
            let entry = entry.map_err(|_| unsupported("file reference entry"))?;
            let kind = entry
                .file_type()
                .map_err(|_| unsupported("file reference type"))?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| unsupported("reference outside root"))?
                .to_string_lossy()
                .to_string();
            if kind.is_dir() {
                if ![".git", "node_modules", "target"]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    paths.push((path, depth + 1));
                }
                continue;
            }
            if relative.to_lowercase().contains(&query.to_lowercase()) {
                result.push(json!({"path":path,"relativePath":relative,"label":relative,"name":entry.file_name(),"isDirectory":false}));
            }
        }
    }
    Ok(json!(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn observation_reports_external_change_absence_and_close() {
        use futures_util::StreamExt;
        let root = std::env::temp_dir().join(format!("agentkit-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("proof"), "before").unwrap();
        let (close, receiver) = tokio::sync::watch::channel(false);
        let mut changes = observe(root.clone(), "proof", receiver).await.unwrap();
        assert_eq!(changes.next().await.unwrap().unwrap()["kind"], "ready");
        std::fs::write(root.join("proof"), "changed contents").unwrap();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), changes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(frame["change"]["version"].is_string());
        std::fs::remove_file(root.join("proof")).unwrap();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), changes.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(frame["change"]["absent"], true);
        close.send(true).unwrap();
        assert!(changes.next().await.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn file_pages_keep_versions_and_reject_escape() {
        let root = std::env::temp_dir().join(format!("agentkit-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("proof"), "你好\nsecond\n").unwrap();
        let first = read(&root, "proof", 0, 3).unwrap();
        first.validate(0, 3).unwrap();
        assert_eq!(STANDARD.decode(&first.data).unwrap(), "你".as_bytes());
        let second = read(&root, "proof", 3, 1024).unwrap();
        assert_eq!(first.version, second.version);
        assert!(second.eof);
        assert_eq!(text(&root, "proof", 2, 1).unwrap().text, "second");
        assert!(read(&root, "../", 0, 1).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.parent().unwrap(), root.join("outside")).unwrap();
            assert!(read(&root, "outside", 0, 1).is_err());
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
