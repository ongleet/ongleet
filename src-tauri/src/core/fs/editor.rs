//! Bounded UTF-8 editing with optimistic conflict detection and atomic replacement.
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

const LIMIT: usize = 1024 * 1024;
static EDIT_LOCK: Mutex<()> = Mutex::new(());
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub path: String,
    pub text: String,
    pub revision: String,
    pub line_ending: String,
    pub bom: bool,
}

fn read(path: &Path) -> Result<(Vec<u8>, std::fs::Metadata), String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Only regular files can be edited".into());
    }
    if metadata.len() > LIMIT as u64 {
        return Err("Text editor limit is 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Text editor limit is 1 MiB".into());
    }
    Ok((bytes, metadata))
}
fn revision(bytes: &[u8], metadata: &std::fs::Metadata) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    (
        metadata.dev(),
        metadata.ino(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.mode(),
        metadata.uid(),
        metadata.gid(),
    )
        .hash(&mut hash);
    format!("{:016x}", hash.finish())
}
fn document(
    path: &Path,
    bytes: &[u8],
    metadata: &std::fs::Metadata,
) -> Result<Option<Document>, String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
    {
        return Ok(None);
    }
    let bom = text.starts_with('\u{feff}');
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Keep the exact original bytes until Save. Uniform CRLF survives editing.
    let line_ending = if text.contains("\r\n") && !text.replace("\r\n", "").contains(['\n', '\r']) {
        "CRLF"
    } else {
        "LF"
    };
    if text.replace("\r\n", "").contains('\r') || (text.contains("\r\n") && line_ending == "LF") {
        return Err("Mixed or legacy line endings are not supported by the text editor".into());
    }
    Ok(Some(Document {
        path: path.to_str().ok_or("File path is not UTF-8")?.into(),
        text: text.replace("\r\n", "\n"),
        revision: revision(bytes, metadata),
        line_ending: line_ending.into(),
        bom,
    }))
}
pub fn load(path: &str) -> Result<Option<Document>, String> {
    let _guard = EDIT_LOCK.lock().map_err(|e| e.to_string())?;
    if !Path::new(path).is_absolute() {
        return Err("Choose an absolute file path".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let (bytes, metadata) = read(&path)?;
    document(&path, &bytes, &metadata)
}
pub fn save(path: &str, text: &str, expected_revision: &str) -> Result<Document, String> {
    let _guard = EDIT_LOCK.lock().map_err(|e| e.to_string())?;
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Choose an absolute file path".into());
    }
    if text.len() > LIMIT {
        return Err("Text editor limit is 1 MiB".into());
    }
    let (bytes, metadata) = read(path)?;
    if revision(&bytes, &metadata) != expected_revision {
        return Err(
            "File changed on disk. Reload it before saving; your edits have been kept.".into(),
        );
    }
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o222 == 0
    {
        return Err("Saving requires a writable file owned by you with a single hard link".into());
    }
    let original = document(path, &bytes, &metadata)?.ok_or("File is not UTF-8 text")?;
    let mut output = if original.bom {
        "\u{feff}".to_owned()
    } else {
        String::new()
    };
    output.push_str(&if original.line_ending == "CRLF" {
        text.replace('\n', "\r\n")
    } else {
        text.into()
    });
    if output.len() > LIMIT {
        return Err("Text editor limit is 1 MiB".into());
    }
    // Validate input before writing (including unsupported control characters).
    document(path, output.as_bytes(), &metadata)?.ok_or("Content is not supported text")?;
    let parent = path.parent().ok_or("File has no parent directory")?;
    let temporary = parent.join(format!(
        ".devcenter-edit-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        created = true;
        file.write_all(output.as_bytes())
            .map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.gid() != metadata.gid()
            && unsafe { libc::fchown(file.as_raw_fd(), !0, metadata.gid()) } != 0
        {
            return Err(format!(
                "Cannot preserve file group: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Preserve ordinary access permissions; do not create a privileged executable.
        file.set_permissions(std::fs::Permissions::from_mode(metadata.mode() & 0o777))
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        let (current, current_metadata) = read(path)?;
        if revision(&current, &current_metadata) != expected_revision {
            return Err("File changed on disk while saving; your edits have been kept".into());
        }
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        document(path, output.as_bytes(), &metadata)?.ok_or("Content is not supported text".into())
    })();
    if created {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_rejects_read_only_hardlinked_and_retargeted_files_without_modification() {
        let root =
            std::env::temp_dir().join(format!("devcenter-editor-reject-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        std::fs::write(&target, "original").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let loaded = load(link.to_str().unwrap()).unwrap().unwrap();
        assert_eq!(loaded.path, target.to_str().unwrap());
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
        let loaded = load(&loaded.path).unwrap().unwrap();
        assert!(save(&loaded.path, "changed", &loaded.revision).is_err());
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let hardlink = root.join("hardlink.txt");
        std::fs::hard_link(&target, &hardlink).unwrap();
        let loaded = load(target.to_str().unwrap()).unwrap().unwrap();
        assert!(save(&loaded.path, "changed", &loaded.revision).is_err());
        assert_eq!(std::fs::read_to_string(&hardlink).unwrap(), "original");
        std::fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&hardlink, &target).unwrap();
        assert!(save(&loaded.path, "changed", &loaded.revision).is_err());
        assert_eq!(std::fs::read_to_string(&hardlink).unwrap(), "original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editing_preserves_format_permissions_and_rejects_conflicts_and_binary_files() {
        let root = std::env::temp_dir().join(format!("devcenter-editor-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("test.txt");
        let name = path.to_str().unwrap();
        std::fs::write(&path, b"\xef\xbb\xbfone\r\ntwo\r\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let loaded = load(name).unwrap().unwrap();
        assert_eq!(loaded.text, "one\ntwo\n");
        let saved = save(name, "edited\n", &loaded.revision).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"\xef\xbb\xbfedited\r\n");
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
        assert!(save(name, "stale", &loaded.revision).is_err());
        std::fs::write(&path, "external edit").unwrap();
        assert!(save(name, "overwrite", &saved.revision).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external edit");
        std::fs::write(&path, b"binary\0data").unwrap();
        assert!(load(name).unwrap().is_none());
        std::fs::write(&path, vec![b'a'; LIMIT + 1]).unwrap();
        assert!(load(name).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
