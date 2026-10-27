//! One-time directory rename, preserving SQLite sidecars and Codex mappings.
use crate::control::paths::LEGACY_ID;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

mod yaml;
pub use yaml::migrate as migrate_yaml;

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
        "Close the previous application instance before migrating Ongleet data".to_string()
    })?;
    Ok(Lock(file))
}

fn exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
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
    // Never merge or overwrite independently created Ongleet state.
    if exists(target)? {
        return Err(format!("Both legacy and Ongleet directories exist: {} and {}. Resolve the conflict explicitly; neither was changed.", source.display(), target.display()));
    }
    Ok(true)
}

/// Preflight every directory before moving any state. Renames preserve sidecars.
pub fn migrate(data: &Path, config: &Path) -> Result<(), String> {
    fs::create_dir_all(data.parent().ok_or("Data directory has no parent")?)
        .map_err(|e| e.to_string())?;
    let _migration = lock(&data.with_file_name(".ongleet-migration.lock"))?;
    let mut moves = Vec::new();
    for target in [
        data.to_path_buf(),
        config.to_path_buf(),
        data.with_file_name("io.local.ongleet"),
        config.with_file_name("io.local.ongleet"),
    ] {
        if fs::symlink_metadata(&target).is_ok_and(|m| !m.is_dir()) {
            return Err(format!(
                "Application destination is not a directory: {}",
                target.display()
            ));
        }
        let sources: Vec<_> = if target.file_name().is_some_and(|n| n == "io.local.ongleet") {
            vec![target.with_file_name("io.local.devcenter")]
        } else {
            vec![
                target.with_file_name("devcenter"),
                target.with_file_name(LEGACY_ID),
            ]
        };
        let mut existing = Vec::new();
        for source in sources {
            if exists(&source)? {
                existing.push(source);
            }
        }
        if existing.len() > 1 {
            return Err(format!(
                "Ambiguous legacy sources for {}; neither was changed",
                target.display()
            ));
        }
        if let Some(source) = existing.first() {
            if check(source, &target)? {
                moves.push((source.clone(), target));
            }
        }
    }
    let profile = data.with_file_name("io.local.ongleet");
    let profile_source = moves
        .iter()
        .find(|(_, target)| target == data)
        .map(|(source, _)| source.as_path())
        .unwrap_or(data);
    let profile_names = [
        "localstorage",
        "storage",
        "databases",
        "CacheStorage",
        "WebKitCache",
        "mediakeys",
        "hsts-storage.sqlite",
        "hsts-storage.sqlite-wal",
        "hsts-storage.sqlite-shm",
    ];
    let pending_profile = profile_source.join(".ongleet-prototype-webkit.pending");
    let resume_profile = pending_profile.exists();
    let legacy_profile = profile_source != data
        && (profile_source.file_name().is_some_and(|n| n == LEGACY_ID)
            || profile_source.join("localstorage").is_dir());
    let mut profile_moves = Vec::new();
    // Old prototype state may already have been moved by a preceding DevCenter
    // upgrade. Explicit profile children never include metadata or user files.
    for name in profile_names
        .into_iter()
        .filter(|_| resume_profile || legacy_profile)
    {
        let source = profile_source.join(name);
        if exists(&source)? {
            let target = profile.join(name);
            if exists(&target)? || moves.iter().any(|(_, p)| p == &profile) {
                return Err(format!(
                    "Conflicting WebKit profile component {}; neither was changed",
                    target.display()
                ));
            }
            if fs::symlink_metadata(&source)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Legacy WebKit profile component is a symbolic link".into());
            }
            profile_moves.push(name);
        }
    }
    // Hold all legacy writer locks until every rename has completed.
    let mut writers = Vec::new();
    for name in [
        "devcenter",
        LEGACY_ID,
        crate::control::paths::APPLICATION_ID,
    ] {
        let old = data.with_file_name(name);
        if old.is_dir() {
            writers.push(lock(&old.join("metadata.lock"))?);
        }
    }
    if !profile_moves.is_empty() && !resume_profile {
        use std::io::Write;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending_profile)
            .and_then(|mut file| file.write_all(b"1"))
            .map_err(|e| e.to_string())?;
    }
    for (source, target) in moves {
        fs::rename(&source, &target)
            .map_err(|e| format!("Cannot migrate {}: {e}", source.display()))?;
    }
    if !profile_moves.is_empty() {
        fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
        for name in profile_moves {
            fs::rename(data.join(name), profile.join(name))
                .map_err(|e| format!("Cannot migrate prototype WebKit profile: {e}"))?;
        }
    }
    let pending = data.join(".ongleet-prototype-webkit.pending");
    if pending.exists() {
        fs::remove_file(pending).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Only exact bare executable names are automatic migration candidates.
pub fn executable(value: &str) -> Option<&'static str> {
    match value {
        "dcodex" | "devcenter-codex" => Some("ong-codex"),
        "devcenter" | "terminal-workspace" => Some("ongleet"),
        "devcenter-gui" => Some("ongleet-gui"),
        _ => None,
    }
}

pub fn namespaces(value: &mut serde_json::Value) -> Result<(), String> {
    if let Some(object) = value.as_object_mut() {
        if let Some(old) = object.get("devcenter.session-tags").cloned() {
            if object
                .get("ongleet.session-tags")
                .is_some_and(|new| new != &old)
            {
                return Err("Conflicting session tag namespaces".into());
            }
            object.insert("ongleet.session-tags".into(), old);
            object.remove("devcenter.session-tags");
        }
    }
    Ok(())
}

pub fn terminal(value: &mut serde_json::Value) {
    if let Some(config) = value.as_object_mut() {
        if let Some(new) = config
            .get("executable")
            .and_then(|v| v.as_str())
            .and_then(executable)
        {
            config.insert("executable".into(), new.into());
        }
    }
    if serde_json::to_string(value).is_ok_and(|s| {
        ["devcenter", "dcodex", "terminal-workspace", "DEVCENTER_"]
            .iter()
            .any(|old| s.contains(old))
    }) {
        eprintln!("Saved terminal retains legacy references in custom paths, shell commands or arguments; review its configuration manually");
    }
}

pub fn archived(value: &mut serde_json::Value) -> Result<(), String> {
    if let Some(object) = value.as_object_mut() {
        if let Some(data) = object.get_mut("pluginData") {
            namespaces(data)?;
        }
        if object.get("kind").and_then(|v| v.as_str()) == Some("terminal") {
            if let Some(config) = object.get_mut("config") {
                terminal(config);
            }
        }
        for key in ["item", "session", "tab"] {
            if let Some(child) = object.get_mut(key) {
                archived(child)?;
            }
        }
        if let Some(tabs) = object.get_mut("tabs").and_then(|v| v.as_array_mut()) {
            for tab in tabs {
                archived(tab)?;
            }
        }
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
                "ongleet-migration-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("data")).unwrap();
            fs::create_dir_all(path.join("config")).unwrap();
            Self(path)
        }
        fn paths(&self) -> (std::path::PathBuf, std::path::PathBuf) {
            (self.0.join("data/ongleet"), self.0.join("config/ongleet"))
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

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    #[test]
    fn devcenter_profile_and_partial_directory_migration_resume() {
        let root =
            std::env::temp_dir().join(format!("ongleet-branding-resume-{}", std::process::id()));
        let data = root.join("data/ongleet");
        let config = root.join("config/ongleet");
        fs::create_dir_all(data.with_file_name("devcenter")).unwrap();
        // Simulate interruption after the configuration directory was renamed.
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("actions.yaml"), b"# keep me").unwrap();
        let profile = data.with_file_name("io.local.devcenter");
        fs::create_dir_all(profile.join("localstorage")).unwrap();
        fs::write(
            profile.join("localstorage/state"),
            b"drafts and preferences",
        )
        .unwrap();
        migrate(&data, &config).unwrap();
        migrate(&data, &config).unwrap();
        assert_eq!(
            fs::read(
                data.with_file_name("io.local.ongleet")
                    .join("localstorage/state")
            )
            .unwrap(),
            b"drafts and preferences"
        );
        assert_eq!(fs::read(config.join("actions.yaml")).unwrap(), b"# keep me");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ambiguous_sources_and_profile_conflicts_change_neither_tree() {
        let root =
            std::env::temp_dir().join(format!("ongleet-branding-conflict-{}", std::process::id()));
        let data = root.join("data/ongleet");
        let config = root.join("config/ongleet");
        let old = data.with_file_name("devcenter");
        fs::create_dir_all(&old).unwrap();
        let prototype = data.with_file_name(LEGACY_ID);
        fs::create_dir_all(&prototype).unwrap();
        assert!(migrate(&data, &config).unwrap_err().contains("Ambiguous"));
        fs::remove_dir(prototype).unwrap();
        fs::create_dir_all(data.with_file_name("io.local.devcenter")).unwrap();
        fs::create_dir_all(data.with_file_name("io.local.ongleet")).unwrap();
        assert!(migrate(&data, &config).is_err());
        assert!(old.exists() && !data.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fresh_install_creates_no_state_and_is_repeatable() {
        let root =
            std::env::temp_dir().join(format!("ongleet-branding-fresh-{}", std::process::id()));
        let data = root.join("data/ongleet");
        let config = root.join("config/ongleet");
        migrate(&data, &config).unwrap();
        migrate(&data, &config).unwrap();
        assert!(!data.exists() && !config.exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod prototype_profile_tests {
    use super::*;
    #[test]
    fn moves_prototype_webkit_children_and_resumes_partial_profile_move() {
        let root =
            std::env::temp_dir().join(format!("ongleet-prototype-webkit-{}", std::process::id()));
        let data = root.join("data/ongleet");
        let config = root.join("config/ongleet");
        let old = data.with_file_name(LEGACY_ID);
        fs::create_dir_all(old.join("localstorage")).unwrap();
        fs::create_dir_all(old.join("storage")).unwrap();
        fs::write(old.join("localstorage/drafts"), b"draft").unwrap();
        fs::write(old.join("storage/preferences"), b"wrap").unwrap();
        fs::write(old.join("user-file"), b"keep").unwrap();
        migrate(&data, &config).unwrap();
        let profile = data.with_file_name("io.local.ongleet");
        assert_eq!(
            fs::read(profile.join("localstorage/drafts")).unwrap(),
            b"draft"
        );
        assert_eq!(fs::read(data.join("user-file")).unwrap(), b"keep");
        // Simulate interruption between individual WebKit component renames.
        fs::rename(profile.join("storage"), data.join("storage")).unwrap();
        fs::write(data.join(".ongleet-prototype-webkit.pending"), b"1").unwrap();
        migrate(&data, &config).unwrap();
        migrate(&data, &config).unwrap();
        assert_eq!(
            fs::read(profile.join("storage/preferences")).unwrap(),
            b"wrap"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
