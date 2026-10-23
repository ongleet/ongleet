use super::SessionRegistry;
use std::{fs, io::Read, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

const LIMIT: usize = 256 * 1024;

impl SessionRegistry {
    fn history_path(&self, tab_id: u64) -> Result<PathBuf, String> {
        let session = self
            .sessions
            .iter()
            .find(|s| s.tabs.iter().any(|t| t.id == tab_id))
            .ok_or("Tab is unavailable")?;
        Ok(self
            .data_directory
            .as_ref()
            .ok_or("History requires persisted application data")?
            .join("terminal-history")
            .join(session.id.to_string())
            .join(format!("{tab_id}.txt")))
    }

    pub fn terminal_history(&self, tab_id: u64) -> Result<String, String> {
        // Reattachment needs the original prefix, not a snapshot of this same
        // process: its current screen is reconstructed by raw replay afterward.
        if let Some(terminal) = self.terminals.get(&tab_id) {
            return Ok(terminal.previous_history.clone());
        }
        let path = self.history_path(tab_id)?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => return Err(format!("Cannot read terminal history: {error}")),
        };
        let mut text = String::new();
        file.take((LIMIT + 1) as u64)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > LIMIT {
            return Err("Saved terminal history exceeds 256 KiB".into());
        }
        Ok(text)
    }

    pub fn save_terminal_history(&self, tab_id: u64, id: u64, text: String) -> Result<(), String> {
        if self.terminals.get(&tab_id).map(|s| s.id) != Some(id) {
            return Err("Terminal history belongs to an unavailable process".into());
        }
        let path = self.history_path(tab_id)?;
        write_history(&path, &text)
    }
}

fn write_history(path: &std::path::Path, text: &str) -> Result<(), String> {
    if text.len() > LIMIT {
        return Err("Terminal history exceeds 256 KiB".into());
    }
    let directory = path.parent().unwrap();
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    fs::File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn history_survives_registry_restart_is_isolated_and_rejects_bad_data() {
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let directory = Directory(
            std::env::temp_dir().join(format!("terminal-history-test-{}", std::process::id())),
        );
        fs::create_dir(&directory.0).unwrap();
        let mut registry = SessionRegistry::open(&directory.0).unwrap();
        registry
            .create("First".into(), directory.0.to_string_lossy().into_owned())
            .unwrap();
        registry
            .create("Second".into(), directory.0.to_string_lossy().into_owned())
            .unwrap();
        let first = registry.sessions[0].tabs[0].id;
        let second = registry.sessions[1].tabs[0].id;
        let path = registry.history_path(first).unwrap();
        assert_eq!(registry.terminal_history(first).unwrap(), "");
        write_history(&path, "old command\ncafé 日本語 🙂").unwrap();
        assert!(registry
            .save_terminal_history(first, 999, "stale".into())
            .is_err());
        assert!(write_history(&path, &"x".repeat(LIMIT + 1)).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(registry);
        let registry = SessionRegistry::open(&directory.0).unwrap();
        assert_eq!(
            registry.terminal_history(first).unwrap(),
            "old command\ncafé 日本語 🙂"
        );
        assert_eq!(registry.terminal_history(second).unwrap(), "");
        assert!(registry.terminal_history(u64::MAX).is_err());
        fs::write(&path, vec![b'x'; LIMIT + 1]).unwrap();
        assert!(registry.terminal_history(first).is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), (LIMIT + 1) as u64);
    }
}
