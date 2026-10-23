//! Read-only filesystem operations; no PTY or terminal-engine dependency.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {
    pub folder: Option<String>,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .folder
            .as_ref()
            .is_some_and(|p| p.len() > 8192 || p.contains('\0') || !Path::new(p).is_absolute())
        {
            return Err("File browser folder must be an absolute path (up to 8192 bytes)".into());
        }
        Ok(())
    }
}
pub fn initialize(config: serde_json::Value, directory: &str) -> Result<serde_json::Value, String> {
    let mut config: Config = serde_json::from_value(config).map_err(|e| e.to_string())?;
    if config.folder.is_none() {
        config.folder = Some(directory.into());
    }
    serde_json::to_value(config).map_err(|e| e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub symlink: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub folder: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
    pub next_offset: Option<usize>,
}

pub fn list(folder: &str, offset: usize) -> Result<Listing, String> {
    Config {
        folder: Some(folder.into()),
    }
    .validate()?;
    if offset > 1_000_000 {
        return Err("Directory page offset exceeds limit".into());
    }
    let path = std::fs::canonicalize(folder).map_err(|e| format!("Cannot browse {folder}: {e}"))?;
    let folder = path.to_str().ok_or("Folder path is not UTF-8")?.to_owned();
    let mut entries = Vec::new();
    let mut bytes = 0;
    let mut next_offset = None;
    // Stream directory order, rather than collecting/sorting an unbounded directory.
    let directory = std::fs::read_dir(&path).map_err(|e| format!("Cannot browse {folder}: {e}"))?;
    for (index, entry) in directory.enumerate().skip(offset) {
        let entry = entry.map_err(|e| format!("Cannot read directory entry: {e}"))?;
        let entry_path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Directory contains a non-UTF-8 filename")?;
        let entry_path_text = entry_path
            .to_str()
            .ok_or("File path is not UTF-8")?
            .to_owned();
        bytes += name.len() + entry_path_text.len();
        if entries.len() == 200 || bytes > 128 * 1024 {
            next_offset = Some(index);
            break;
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        entries.push(Entry {
            name,
            path: entry_path_text,
            directory: if kind.is_symlink() {
                entry_path.is_dir()
            } else {
                kind.is_dir()
            },
            symlink: kind.is_symlink(),
        });
    }
    Ok(Listing {
        parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
        folder,
        entries,
        next_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn paginated_read_only_browsing_follows_symlinks_and_reports_failures() {
        let root = std::env::temp_dir().join(format!("plugin-files-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for index in 0..450 {
            std::fs::write(root.join(format!("entry-{index}")), b"unchanged").unwrap();
        }
        let child = root.join("child");
        std::fs::create_dir(&child).unwrap();
        symlink(&child, root.join("linked")).unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut offset = 0;
        loop {
            let page = list(root.to_str().unwrap(), offset).unwrap();
            assert!(page.entries.len() <= 200);
            for entry in page.entries {
                assert!(seen.insert(entry.name));
            }
            match page.next_offset {
                Some(next) => {
                    assert!(next > offset);
                    offset = next;
                }
                None => break,
            }
        }
        assert_eq!(seen.len(), 452);
        assert_eq!(
            list(root.join("linked").to_str().unwrap(), 0)
                .unwrap()
                .folder,
            child.to_str().unwrap()
        );
        let before = std::fs::read(root.join("entry-0")).unwrap();
        assert!(list(root.join("missing").to_str().unwrap(), 0).is_err());
        assert!(!root.join("missing").exists());
        assert!(list(root.join("entry-0").to_str().unwrap(), 0).is_err());
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert!(list(child.to_str().unwrap(), 0).is_err());
        }
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(std::fs::read(root.join("entry-0")).unwrap(), before);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 452);
        std::fs::remove_dir_all(root).unwrap();
    }
}
