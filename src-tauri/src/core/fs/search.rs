//! Bounded recursive filename/path search without following symlinks.
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    path: String,
    relative_path: String,
}

#[derive(Debug, Serialize)]
pub struct Results {
    matches: Vec<Match>,
    truncated: bool,
    skipped: usize,
}

pub fn search(
    folder: &str,
    query: &str,
    show_hidden: bool,
    cancel: &AtomicBool,
) -> Result<Results, String> {
    if folder.len() > 8192 || folder.contains('\0') || !Path::new(folder).is_absolute() {
        return Err("Expected an absolute folder path (up to 8192 bytes)".into());
    }
    if query.is_empty() || query.len() > 1024 {
        return Err("Search requires 1–1024 bytes".into());
    }
    let root = std::fs::canonicalize(folder).map_err(|e| e.to_string())?;
    let initial = std::fs::read_dir(&root).map_err(|e| e.to_string())?;
    let mut stack = vec![(initial, 0)];
    let mut result = Results {
        matches: Vec::new(),
        truncated: false,
        skipped: 0,
    };
    let query = query.to_lowercase();
    let start = Instant::now();
    let mut scanned = 0;
    let mut bytes = 0;
    while let Some((directory, depth)) = stack.last_mut() {
        if cancel.load(Ordering::Acquire) {
            return Err("Search cancelled".into());
        }
        if scanned >= 100_000 || start.elapsed() >= Duration::from_secs(2) {
            result.truncated = true;
            break;
        }
        let Some(entry) = directory.next() else {
            stack.pop();
            continue;
        };
        scanned += 1;
        let Ok(entry) = entry else {
            result.skipped += 1;
            continue;
        };
        if !show_hidden && entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            result.skipped += 1;
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if *depth >= 64 {
                result.truncated = true;
                continue;
            }
            let next_depth = *depth + 1;
            match std::fs::read_dir(&path) {
                Ok(children) => stack.push((children, next_depth)),
                Err(_) => result.skipped += 1,
            }
        } else if kind.is_file() {
            let Some(relative) = path.strip_prefix(&root).ok().and_then(Path::to_str) else {
                result.skipped += 1;
                continue;
            };
            if !relative.to_lowercase().contains(&query) {
                continue;
            }
            let Some(full) = path.to_str() else {
                result.skipped += 1;
                continue;
            };
            bytes += full.len() + relative.len();
            if result.matches.len() >= 500 || bytes > 128 * 1024 {
                result.truncated = true;
                break;
            }
            result.matches.push(Match {
                path: full.to_owned(),
                relative_path: relative.to_owned(),
            });
        }
    }
    result
        .matches
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn result_limit_is_reported_without_dropping_all_matches() {
        let root =
            std::env::temp_dir().join(format!("ongleet-path-search-limit-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for index in 0..501 {
            std::fs::write(root.join(format!("match-{index}.txt")), "").unwrap();
        }
        let result = search(
            root.to_str().unwrap(),
            "match",
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.matches.len(), 500);
        assert!(result.truncated);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recursive_path_matching_respects_hidden_files_and_symlinks() {
        let root = std::env::temp_dir().join(format!("ongleet-path-search-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Nested/Components")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("Nested/Components/Button.tsx"), "unrelated text").unwrap();
        std::fs::write(root.join(".hidden/button.txt"), "").unwrap();
        std::fs::write(root.join("other.txt"), "button").unwrap();
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let cancel = AtomicBool::new(false);
        let folder = root.to_str().unwrap();
        let found = search(folder, "components/but", false, &cancel).unwrap();
        assert_eq!(found.matches.len(), 1);
        assert_eq!(
            found.matches[0].relative_path,
            "Nested/Components/Button.tsx"
        );
        assert!(!found.truncated);
        assert_eq!(
            search(folder, "BUTTON", false, &cancel)
                .unwrap()
                .matches
                .len(),
            1
        );
        assert_eq!(
            search(folder, "button", true, &cancel)
                .unwrap()
                .matches
                .len(),
            2
        );
        cancel.store(true, Ordering::Release);
        assert!(search(folder, "button", true, &cancel)
            .unwrap_err()
            .contains("cancelled"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
