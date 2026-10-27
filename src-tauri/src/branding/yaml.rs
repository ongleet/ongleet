//! Preserve YAML comments by editing scalar tokens and verifying the entire document.
use std::{fs, path::Path};

fn expected(value: &mut serde_yaml_ng::Value) {
    if let Some(actions) = value.get_mut("actions").and_then(|v| v.as_sequence_mut()) {
        for action in actions {
            if matches!(
                action.get("kind").and_then(|v| v.as_str()),
                Some("terminal" | "command")
            ) {
                if let Some(config) = action.get_mut("config") {
                    if let Some(new) = config
                        .get("executable")
                        .and_then(|v| v.as_str())
                        .and_then(super::executable)
                    {
                        config["executable"] = new.into();
                    }
                }
            }
        }
    }
}

fn rewrite(text: &str) -> Option<String> {
    let mut desired: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).ok()?;
    let original = desired.clone();
    expected(&mut desired);
    if desired == original {
        return Some(text.into());
    }
    // Locate complete scalar tokens, including quoted JSON/flow YAML values.
    // Semantic comparison below rejects any edit to comments' neighbouring data,
    // shell commands, custom plugin payloads, aliases or unrelated scalar values.
    let bytes = text.as_bytes();
    let mut edits = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let quote = if matches!(bytes[i], b'\'' | b'"') {
            Some(bytes[i])
        } else {
            None
        };
        let start = i;
        let content_start = i + usize::from(quote.is_some());
        if let Some(q) = quote {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && q == b'"' {
                    i += 2;
                    continue;
                }
                if bytes[i] == q {
                    break;
                }
                i += 1;
            }
        } else {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !b":,{}[]#".contains(&bytes[i])
            {
                i += 1;
            }
        }
        let end = i.min(bytes.len());
        let token = &text[content_start.min(end)..end];
        if quote.is_some() && i < bytes.len() {
            i += 1;
        }
        let mut next = i;
        while next < bytes.len() && bytes[next].is_ascii_whitespace() {
            next += 1;
        }
        if token == "executable" && bytes.get(next) == Some(&b':') {
            next += 1;
            while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                next += 1;
            }
            let q = bytes
                .get(next)
                .copied()
                .filter(|b| matches!(b, b'\'' | b'"'));
            let a = next + usize::from(q.is_some());
            let mut b = a;
            while b < bytes.len()
                && match q {
                    Some(q) => bytes[b] != q,
                    None => !bytes[b].is_ascii_whitespace() && !b",}#]".contains(&bytes[b]),
                }
            {
                b += 1;
            }
            if let Some(new) = super::executable(&text[a..b]) {
                edits.push((a, b, new));
            }
        }
        if i == start {
            i += 1;
        }
    }
    let mut output = text.to_string();
    for (a, b, new) in edits.into_iter().rev() {
        output.replace_range(a..b, new);
    }
    let actual: serde_yaml_ng::Value = serde_yaml_ng::from_str(&output).ok()?;
    (actual == desired).then_some(output)
}

pub fn migrate(directory: &Path) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            migrate(&entry.path())?;
        } else if kind.is_file() && entry.file_name() == "actions.yaml" {
            let path = entry.path();
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            match rewrite(&text) {
                Some(updated) if updated != text => {
                    let backup = path.with_extension("yaml.before-ongleet");
                    if !backup.exists() {
                        fs::copy(&path, &backup).map_err(|e| e.to_string())?;
                    }
                    let temporary = path.with_extension("yaml.ongleet-migration");
                    fs::copy(&path, &temporary).map_err(|e| e.to_string())?;
                    fs::write(&temporary, updated).map_err(|e| e.to_string())?;
                    fs::rename(temporary, &path).map_err(|e| e.to_string())?;
                }
                None => eprintln!("Manual migration required for YAML: {}", path.display()),
                _ => {}
            }
            // Report paths only: never expose command arguments or credentials.
            let remaining = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            if ["devcenter", "dcodex", "terminal-workspace", "DEVCENTER_"]
                .iter()
                .any(|s| remaining.contains(s))
            {
                eprintln!(
                    "Legacy references remain; review manually: {}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_comments_arguments_and_shell_strings() {
        let text = "# custom\nactions:\n  - kind: terminal\n    config:\n      executable: 'dcodex' # wrapper\n      args: [dcodex, 'devcenter app quit']\n  - kind: command\n    config: {shellCommand: 'dcodex --fresh'}\n";
        let result = rewrite(text).unwrap();
        assert_eq!(
            result,
            text.replacen("'dcodex' # wrapper", "'ong-codex' # wrapper", 1)
        );
        assert_eq!(rewrite(&result).unwrap(), result);
        assert_eq!(rewrite(r#"{"actions":[{"kind":"terminal","config":{"executable":"devcenter-codex","args":[]}}]}"#).unwrap(), r#"{"actions":[{"kind":"terminal","config":{"executable":"ong-codex","args":[]}}]}"#);
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn migration_keeps_verbatim_backup_and_is_safe_to_repeat() {
        let root = std::env::temp_dir().join(format!("ongleet-yaml-backup-{}", std::process::id()));
        fs::create_dir_all(root.join("projects/7")).unwrap();
        let path = root.join("projects/7/actions.yaml");
        let original = "# personal comment\nactions:\n- kind: terminal\n  config: {executable: dcodex, args: ['devcenter'], env: {KEEP: yes}}\n";
        fs::write(&path, original).unwrap();
        migrate(&root).unwrap();
        migrate(&root).unwrap();
        assert_eq!(
            fs::read_to_string(path.with_extension("yaml.before-ongleet")).unwrap(),
            original
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            original.replace("executable: dcodex", "executable: ong-codex")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
