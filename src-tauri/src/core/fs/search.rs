//! Bounded recursive path and UTF-8 content search without following symlinks.
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    path: String,
    relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Results {
    matches: Vec<Match>,
    truncated: bool,
    skipped: usize,
}

// Slashless patterns match basenames at any depth; paths are root-relative.
fn patterns(value: &str) -> Result<Vec<glob::Pattern>, String> {
    if value.len() > 4096 {
        return Err("Search filters exceed 4096 bytes".into());
    }
    let parts: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() > 32 {
        return Err("Search allows at most 32 patterns per filter".into());
    }
    parts
        .into_iter()
        .map(|p| glob::Pattern::new(p).map_err(|e| format!("Invalid search pattern: {e}")))
        .collect()
}

fn matches_patterns(patterns: &[glob::Pattern], path: &str) -> bool {
    let options = glob::MatchOptions {
        require_literal_separator: true,
        ..Default::default()
    };
    patterns.iter().any(|pattern| {
        let candidate = if pattern.as_str().contains('/') {
            path
        } else {
            path.rsplit('/').next().unwrap_or(path)
        };
        pattern.matches_with(candidate, options)
    })
}

#[cfg(test)]
pub fn search(
    folder: &str,
    query: &str,
    show_hidden: bool,
    cancel: &AtomicBool,
) -> Result<Results, String> {
    search_with_options(folder, query, show_hidden, false, false, ("", ""), cancel)
}

pub fn search_with_options(
    folder: &str,
    query: &str,
    show_hidden: bool,
    content: bool,
    case_sensitive: bool,
    filters: (&str, &str),
    cancel: &AtomicBool,
) -> Result<Results, String> {
    use std::io::Read;
    let include = patterns(filters.0)?;
    let exclude = patterns(filters.1)?;
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
    let query = if content && case_sensitive {
        query.to_owned()
    } else {
        query.to_lowercase()
    };
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
        let Some(relative) = path.strip_prefix(&root).ok().and_then(Path::to_str) else {
            result.skipped += 1;
            continue;
        };
        let relative = relative
            .split(std::path::MAIN_SEPARATOR)
            .collect::<Vec<_>>()
            .join("/");
        if matches_patterns(&exclude, &relative)
            || (kind.is_dir() && matches_patterns(&exclude, &format!("{relative}/")))
        {
            continue;
        }
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
            if !include.is_empty() && !matches_patterns(&include, &relative) {
                continue;
            }
            if !content && !relative.to_lowercase().contains(&query) {
                continue;
            }
            let Some(full) = path.to_str() else {
                result.skipped += 1;
                continue;
            };
            let lines = if content {
                let mut data = Vec::new();
                let read = std::fs::File::open(&path)
                    .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut data));
                if read.is_err() || data.len() > 1024 * 1024 || data.contains(&0) {
                    result.skipped += 1;
                    continue;
                }
                let Ok(text) = String::from_utf8(data) else {
                    result.skipped += 1;
                    continue;
                };
                text.lines()
                    .enumerate()
                    .filter_map(|(index, line)| {
                        let haystack = if case_sensitive {
                            line.to_owned()
                        } else {
                            line.to_lowercase()
                        };
                        haystack.contains(&query).then(|| {
                            (
                                Some(index + 1),
                                Some(line.chars().take(500).collect::<String>()),
                            )
                        })
                    })
                    .take(501)
                    .collect::<Vec<_>>()
            } else {
                vec![(None, None)]
            };
            for (line, text) in lines {
                if cancel.load(Ordering::Acquire) {
                    return Err("Search cancelled".into());
                }
                bytes += full.len() + relative.len() + text.as_ref().map_or(0, String::len);
                if result.matches.len() >= 500
                    || bytes > 128 * 1024
                    || start.elapsed() >= Duration::from_secs(2)
                {
                    result.truncated = true;
                    break;
                }
                result.matches.push(Match {
                    path: full.to_owned(),
                    relative_path: relative.to_owned(),
                    line,
                    text,
                });
            }
            if result.truncated {
                break;
            }
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
    fn filters_include_nested_files_and_prune_excluded_directories() {
        let root =
            std::env::temp_dir().join(format!("ongleet-search-filters-{}", std::process::id()));
        for directory in ["src/nested", "node_modules", "src/node_modules", "docs"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in [
            "src/main.ts",
            "src/nested/view.tsx",
            "src/nested/view.test.ts",
            "docs/readme.md",
        ] {
            std::fs::write(root.join(file), "needle").unwrap();
        }
        // Excluded trees must never be read, even for skipped-file accounting.
        for file in ["node_modules/binary", "src/node_modules/binary"] {
            std::fs::write(root.join(file), b"needle\0").unwrap();
        }
        let cancel = AtomicBool::new(false);
        let folder = root.to_str().unwrap();
        let result = search_with_options(
            folder,
            "needle",
            false,
            true,
            false,
            ("*.ts, *.tsx", "**/node_modules/**, *.test.ts"),
            &cancel,
        )
        .unwrap();
        assert_eq!(
            result
                .matches
                .iter()
                .map(|m| m.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.ts", "src/nested/view.tsx"]
        );
        assert_eq!(result.skipped, 0);
        let result = search_with_options(
            folder,
            "needle",
            false,
            true,
            false,
            ("docs/**", ""),
            &cancel,
        )
        .unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].relative_path, "docs/readme.md");
        assert!(
            search_with_options(folder, "needle", false, true, false, ("[", ""), &cancel).is_err()
        );
        assert!(patterns(&vec!["*.ts"; 33].join(",")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_search_returns_lines_and_skips_binary_and_oversized_files() {
        let root =
            std::env::temp_dir().join(format!("ongleet-content-search-{}", std::process::id()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(
            root.join("nested/source.txt"),
            "first\nNeedle here\nneedle again\n",
        )
        .unwrap();
        std::fs::write(root.join("binary"), b"needle\0data").unwrap();
        std::fs::write(root.join("large"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
        std::fs::write(root.join(".hidden"), "needle").unwrap();
        let cancel = AtomicBool::new(false);
        let folder = root.to_str().unwrap();
        let found =
            search_with_options(folder, "needle", false, true, false, ("", ""), &cancel).unwrap();
        assert_eq!(found.matches.len(), 2);
        assert_eq!(found.matches[0].relative_path, "nested/source.txt");
        assert_eq!(found.matches[0].line, Some(2));
        assert_eq!(found.matches[0].text.as_deref(), Some("Needle here"));
        assert_eq!(found.skipped, 2);
        let sensitive =
            search_with_options(folder, "needle", false, true, true, ("", ""), &cancel).unwrap();
        assert_eq!(sensitive.matches.len(), 1);
        assert_eq!(sensitive.matches[0].line, Some(3));
        assert_eq!(
            search_with_options(folder, "needle", true, true, false, ("", ""), &cancel)
                .unwrap()
                .matches
                .len(),
            3
        );
        std::fs::remove_dir_all(root).unwrap();
    }

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
        #[cfg(unix)]
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
