//! Filesystem browsing and bounded text editing; no PTY or terminal-engine dependency.
pub mod editor;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub symlink: bool,
    #[serde(skip)]
    modified: Option<std::time::SystemTime>,
    #[serde(skip)]
    size: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub folder: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
    pub next_offset: Option<usize>,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    NameAscending,
    NameDescending,
    DateAscending,
    DateDescending,
    SizeAscending,
    SizeDescending,
}

pub fn list(
    folder: &str,
    offset: usize,
    order: SortOrder,
    show_hidden: bool,
    query: &str,
) -> Result<Listing, String> {
    if query.len() > 8192 {
        return Err("Search exceeds 8192-byte limit".into());
    }
    if folder.len() > 8192 || folder.contains('\0') || !Path::new(folder).is_absolute() {
        return Err("Expected an absolute folder path (up to 8192 bytes)".into());
    }
    if offset > 1_000_000 {
        return Err("Directory page offset exceeds limit".into());
    }
    let path = std::fs::canonicalize(folder).map_err(|e| format!("Cannot browse {folder}: {e}"))?;
    let folder = path.to_str().ok_or("Folder path is not UTF-8")?.to_owned();
    let mut entries = Vec::new();
    let mut bytes = 0;
    // Bound the scan before sorting the complete directory, then paginate.
    let directory = std::fs::read_dir(&path).map_err(|e| format!("Cannot browse {folder}: {e}"))?;
    for entry in directory {
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
        if entries.len() == 100_000 || bytes > 16 * 1024 * 1024 {
            return Err(
                "Directory exceeds sorting limit (100,000 entries or 16 MiB of paths)".into(),
            );
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let metadata = entry.metadata().ok();
        entries.push(Entry {
            name,
            path: entry_path_text,
            directory: if kind.is_symlink() {
                entry_path.is_dir()
            } else {
                kind.is_dir()
            },
            symlink: kind.is_symlink(),
            modified: metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok()),
            size: metadata.as_ref().map(|metadata| metadata.len()),
        });
    }
    if !show_hidden {
        entries.retain(|entry| !entry.name.starts_with('.'));
    }
    let query = query.to_lowercase();
    if !query.is_empty() {
        entries.retain(|entry| entry.name.to_lowercase().contains(&query));
    }
    entries.sort_by(|left, right| {
        let name = || {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        };
        let comparison = match order {
            SortOrder::NameAscending | SortOrder::NameDescending => name(),
            SortOrder::DateAscending | SortOrder::DateDescending => {
                left.modified.cmp(&right.modified).then_with(name)
            }
            SortOrder::SizeAscending | SortOrder::SizeDescending => {
                left.size.cmp(&right.size).then_with(name)
            }
        };
        let comparison = match order {
            SortOrder::NameDescending | SortOrder::DateDescending | SortOrder::SizeDescending => {
                comparison.reverse()
            }
            _ => comparison,
        };
        right.directory.cmp(&left.directory).then(comparison)
    });
    let mut page = Vec::new();
    let mut bytes = 0;
    let mut next_offset = None;
    for (index, entry) in entries.into_iter().enumerate().skip(offset) {
        bytes += entry.name.len() + entry.path.len();
        if page.len() == 200 || bytes > 128 * 1024 {
            next_offset = Some(index);
            break;
        }
        page.push(entry);
    }
    Ok(Listing {
        parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
        folder,
        entries: page,
        next_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn search_matches_names_case_insensitively_before_pagination() {
        let root = std::env::temp_dir().join(format!("plugin-files-search-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for index in 0..210 {
            std::fs::write(root.join(format!("noise-{index}")), b"").unwrap();
        }
        std::fs::create_dir(root.join("Report-folder")).unwrap();
        std::fs::write(root.join("z-REPORT.txt"), b"").unwrap();
        std::fs::write(root.join(".report"), b"").unwrap();
        let page = list(
            root.to_str().unwrap(),
            0,
            SortOrder::default(),
            false,
            "report",
        )
        .unwrap();
        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["Report-folder", "z-REPORT.txt"]
        );
        assert_eq!(page.next_offset, None);
        assert_eq!(
            list(
                root.to_str().unwrap(),
                0,
                SortOrder::default(),
                true,
                "REPORT"
            )
            .unwrap()
            .entries
            .len(),
            3
        );
        assert!(list(
            root.to_str().unwrap(),
            0,
            SortOrder::default(),
            true,
            "missing"
        )
        .unwrap()
        .entries
        .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hidden_entries_are_filtered_before_pagination_and_can_be_shown() {
        let root = std::env::temp_dir().join(format!("plugin-files-hidden-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join(".config")).unwrap();
        for index in 0..210 {
            std::fs::write(root.join(format!(".hidden-{index:03}")), b"").unwrap();
        }
        std::fs::write(root.join("visible.txt"), b"").unwrap();
        let hidden = list(root.to_str().unwrap(), 0, SortOrder::default(), false, "").unwrap();
        assert_eq!(hidden.entries.len(), 1);
        assert_eq!(hidden.entries[0].name, "visible.txt");
        assert_eq!(hidden.next_offset, None);
        let shown = list(root.to_str().unwrap(), 0, SortOrder::default(), true, "").unwrap();
        assert_eq!(shown.entries.len(), 200);
        assert_eq!(shown.entries[0].name, ".config");
        let last = list(
            root.to_str().unwrap(),
            shown.next_offset.unwrap(),
            SortOrder::default(),
            true,
            "",
        )
        .unwrap();
        assert_eq!(last.entries.len(), 12);
        assert_eq!(last.entries.last().unwrap().name, "visible.txt");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sorts_by_size_and_modified_date_in_both_directions_with_folders_first() {
        let root = std::env::temp_dir().join(format!("plugin-files-sort-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("z-folder")).unwrap();
        for (name, seconds, size) in [("z.rs", 100, 10), ("a.txt", 300, 0), ("b.rs", 200, 30)] {
            let file = std::fs::File::create(root.join(name)).unwrap();
            file.set_len(size).unwrap();
            file.set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)),
            )
            .unwrap();
        }
        for (order, expected) in [
            (
                SortOrder::SizeAscending,
                ["z-folder", "a.txt", "z.rs", "b.rs"],
            ),
            (
                SortOrder::SizeDescending,
                ["z-folder", "b.rs", "z.rs", "a.txt"],
            ),
            (
                SortOrder::DateAscending,
                ["z-folder", "z.rs", "b.rs", "a.txt"],
            ),
            (
                SortOrder::DateDescending,
                ["z-folder", "a.txt", "b.rs", "z.rs"],
            ),
        ] {
            let page = list(root.to_str().unwrap(), 0, order, false, "").unwrap();
            assert_eq!(
                page.entries
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

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
        let mut ordered = Vec::new();
        loop {
            let page = list(
                root.to_str().unwrap(),
                offset,
                SortOrder::default(),
                false,
                "",
            )
            .unwrap();
            assert!(page.entries.len() <= 200);
            for entry in page.entries {
                ordered.push(entry.name.clone());
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
        assert_eq!(&ordered[..4], &["child", "linked", "entry-0", "entry-1"]);
        assert_eq!(ordered.last().unwrap(), "entry-99");
        let descending = list(
            root.to_str().unwrap(),
            0,
            SortOrder::NameDescending,
            false,
            "",
        )
        .unwrap();
        assert_eq!(descending.entries[0].name, "linked");
        assert_eq!(descending.entries[1].name, "child");
        assert_eq!(descending.entries[2].name, "entry-99");
        assert_eq!(
            list(
                root.join("linked").to_str().unwrap(),
                0,
                SortOrder::default(),
                false,
                ""
            )
            .unwrap()
            .folder,
            child.to_str().unwrap()
        );
        let before = std::fs::read(root.join("entry-0")).unwrap();
        assert!(list(
            root.join("missing").to_str().unwrap(),
            0,
            SortOrder::default(),
            false,
            ""
        )
        .is_err());
        assert!(!root.join("missing").exists());
        assert!(list(
            root.join("entry-0").to_str().unwrap(),
            0,
            SortOrder::default(),
            false,
            ""
        )
        .is_err());
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert!(list(child.to_str().unwrap(), 0, SortOrder::default(), false, "").is_err());
        }
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(std::fs::read(root.join("entry-0")).unwrap(), before);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 452);
        std::fs::remove_dir_all(root).unwrap();
    }
}
