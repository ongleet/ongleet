//! One-time directory rename, preserving SQLite sidecars and Codex mappings.
use crate::control::paths::LEGACY_ID;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn lock(path: &Path) -> Result<Lock, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("Cannot lock migration: {e}"))?;
    file.try_lock().map_err(|_| {
        "Close the previous application instance before migrating DevCenter data".to_string()
    })?;
    Ok(Lock(file))
}

fn check(source: &Path, target: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    if !metadata.is_dir() {
        return Err(format!(
            "Legacy application path is not a directory: {}",
            source.display()
        ));
    }
    // Never merge or overwrite independently created DevCenter state.
    if target.try_exists().map_err(|e| e.to_string())? {
        return Err(format!("Both legacy and DevCenter directories exist: {} and {}. Resolve the conflict explicitly; neither was changed.", source.display(), target.display()));
    }
    Ok(true)
}

pub fn migrate(data: &Path, config: &Path) -> Result<(), String> {
    let legacy_data = data.with_file_name(LEGACY_ID);
    let legacy_config = config.with_file_name(LEGACY_ID);
    fs::create_dir_all(data.parent().ok_or("Data directory has no parent")?)
        .map_err(|e| e.to_string())?;
    let _migration = lock(&data.with_file_name(".devcenter-migration.lock"))?;
    let move_data = check(&legacy_data, data)?;
    let move_config = check(&legacy_config, config)?;
    // Check the same writer lock used by the old application before moving
    // either directory. Moving a live SQLite database would strand its writer.
    let _legacy = if move_data {
        Some(lock(&legacy_data.join("metadata.lock"))?)
    } else {
        None
    };
    if move_config {
        fs::rename(&legacy_config, config)
            .map_err(|e| format!("Cannot migrate DevCenter config: {e}"))?;
    }
    if move_data {
        // Keep the complete directory together: database, WAL/SHM, mappings and
        // lock inode. A failure is reported, and a subsequent launch can retry.
        fs::rename(&legacy_data, data)
            .map_err(|e| format!("Cannot migrate DevCenter data: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "devcenter-migration-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("data")).unwrap();
            fs::create_dir_all(path.join("config")).unwrap();
            Self(path)
        }
        fn paths(&self) -> (std::path::PathBuf, std::path::PathBuf) {
            (
                self.0.join("data/devcenter"),
                self.0.join("config/devcenter"),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn moves_complete_state_and_is_idempotent() {
        let fixture = Fixture::new();
        let (data, config) = fixture.paths();
        let old = data.with_file_name(LEGACY_ID);
        let old_config = config.with_file_name(LEGACY_ID);
        fs::create_dir_all(old.join("codex-sessions/1")).unwrap();
        fs::write(old.join("codex-sessions/1/2.json"), b"conversation").unwrap();
        let db = rusqlite::Connection::open(old.join("workspaces.sqlite3")).unwrap();
        db.execute_batch(
            "CREATE TABLE marker (value TEXT); INSERT INTO marker VALUES ('preserved');",
        )
        .unwrap();
        drop(db);
        fs::write(old.join("workspaces.sqlite3-wal"), b"sidecar").unwrap();
        fs::create_dir_all(old_config.join("projects/1")).unwrap();
        fs::write(
            old_config.join("projects/1/actions.yaml"),
            b"custom actions",
        )
        .unwrap();
        migrate(&data, &config).unwrap();
        migrate(&data, &config).unwrap();
        assert!(!old.exists() && !old_config.exists());
        assert_eq!(
            fs::read(data.join("codex-sessions/1/2.json")).unwrap(),
            b"conversation"
        );
        assert_eq!(
            fs::read(data.join("workspaces.sqlite3-wal")).unwrap(),
            b"sidecar"
        );
        assert_eq!(
            fs::read(config.join("projects/1/actions.yaml")).unwrap(),
            b"custom actions"
        );
        fs::remove_file(data.join("workspaces.sqlite3-wal")).unwrap();
        let db = rusqlite::Connection::open(data.join("workspaces.sqlite3")).unwrap();
        assert_eq!(
            db.query_row("SELECT value FROM marker", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "preserved"
        );
    }

    #[test]
    fn refuses_a_running_legacy_writer_or_conflicting_destination() {
        let fixture = Fixture::new();
        let (data, config) = fixture.paths();
        let old = data.with_file_name(LEGACY_ID);
        let old_config = config.with_file_name(LEGACY_ID);
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&old_config).unwrap();
        let held = lock(&old.join("metadata.lock")).unwrap();
        assert!(migrate(&data, &config)
            .unwrap_err()
            .contains("Close the previous"));
        assert!(old.exists() && old_config.exists() && !data.exists() && !config.exists());
        drop(held);
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("keep"), b"new config").unwrap();
        assert!(migrate(&data, &config).unwrap_err().contains("Both legacy"));
        assert!(old.exists() && old_config.exists() && !data.exists());
        assert_eq!(fs::read(config.join("keep")).unwrap(), b"new config");
    }
}
