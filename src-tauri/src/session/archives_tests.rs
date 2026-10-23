use super::*;
use crate::session::SessionRegistry;
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("devcenter-archive-{name}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> SessionRegistry {
        SessionRegistry::open(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn archiving_restoring_and_purging_preserves_files_and_plugin_state_across_restarts() {
    let fixture = Fixture::new("restore");
    let work = fixture.0.join("work");
    std::fs::create_dir(&work).unwrap();
    let file = work.join("unchanged.txt");
    std::fs::write(&file, "keep my file").unwrap();
    let mut registry = fixture.open();
    registry
        .create("Saved session".into(), work.to_str().unwrap().into())
        .unwrap();
    let session = registry.sessions[0].id;
    let project = registry.sessions[0].project_id;
    registry
        .create_plugin_tab(
            session,
            "Files".into(),
            "files".into(),
            1,
            serde_json::json!({"folder":work,"editorFile":file}),
        )
        .unwrap();
    let tab = registry.next_tab_id;
    registry.delete_tab(tab, false).unwrap();
    assert!(!registry.sessions[0]
        .tabs
        .iter()
        .any(|entry| entry.id == tab));
    let tab_archive = registry.archives[0].id;
    registry.close(session, false).unwrap();
    assert!(registry.sessions.is_empty());
    let session_archive = registry.archives[1].id;
    drop(registry);
    let mut registry = fixture.open();
    assert_eq!(registry.archives.len(), 2);
    assert!(registry
        .restore_archive(tab_archive)
        .err()
        .unwrap()
        .contains("parent session"));
    registry.close_project(project).unwrap();
    registry.restore_archive(session_archive).unwrap();
    assert_eq!(registry.sessions[0].id, session);
    assert_eq!(registry.projects[0].id, project);
    registry.restore_archive(tab_archive).unwrap();
    let restored = registry.sessions[0]
        .tabs
        .iter()
        .find(|entry| entry.id == tab)
        .unwrap();
    assert_eq!(restored.config["editorFile"], file.to_str().unwrap());
    assert_eq!(restored.config["folder"], work.to_str().unwrap());
    assert!(registry.archives.is_empty());
    assert!(registry.terminals.is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep my file");
}

#[test]
fn retention_uses_archive_time_and_purges_only_matching_application_artifacts() {
    let fixture = Fixture::new("retention");
    let mut registry = fixture.open();
    registry
        .create(
            "Very old name but active".into(),
            fixture.0.to_str().unwrap().into(),
        )
        .unwrap();
    let session = registry.sessions[0].id;
    let tab = registry.sessions[0].tabs[0].id;
    let history = fixture.0.join("terminal-history").join(session.to_string());
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(history.join(format!("{tab}.txt")), "old output").unwrap();
    std::fs::write(history.join("999999.txt"), "unrelated output").unwrap();
    registry.delete_tab(tab, false).unwrap();
    let archived = registry.archives[0].archived_at;
    let id = registry.archives[0].id;
    assert!(registry.purge_archive(id, false).is_err());
    assert_eq!(
        registry
            .purge_expired_archives(archived + RETENTION_SECONDS - 1)
            .unwrap(),
        0
    );
    assert!(history.join(format!("{tab}.txt")).exists());
    assert_eq!(
        registry
            .purge_expired_archives(archived + RETENTION_SECONDS)
            .unwrap(),
        1
    );
    assert!(!history.join(format!("{tab}.txt")).exists());
    assert_eq!(
        std::fs::read_to_string(history.join("999999.txt")).unwrap(),
        "unrelated output"
    );
    assert_eq!(registry.sessions[0].id, session);
    assert!(registry.archives.is_empty());
    drop(registry);
    let mut registry = fixture.open();
    assert!(registry.archives.is_empty());
    registry.close(session, false).unwrap();
    assert!(
        registry.archives[0].id > id,
        "Purged archive IDs must never be reused"
    );
}

#[test]
fn archive_transaction_failure_rolls_back_active_metadata_and_cleanup_is_retryable() {
    let fixture = Fixture::new("rollback");
    let mut registry = fixture.open();
    registry
        .create("Keep".into(), fixture.0.to_str().unwrap().into())
        .unwrap();
    let id = registry.sessions[0].id;
    registry.store.as_ref().unwrap().reject_writes();
    assert!(registry.close(id, false).is_err());
    assert_eq!(registry.sessions[0].id, id);
    assert!(registry.archives.is_empty());
    drop(registry);
    let mut registry = fixture.open();
    registry.close(id, false).unwrap();
    let archive = registry.archives[0].id;
    // A symlink under application data must not redirect cleanup into user files.
    let work = fixture.0.join("work");
    std::fs::create_dir(&work).unwrap();
    std::fs::write(work.join("1.txt"), "untouched").unwrap();
    std::os::unix::fs::symlink(&work, fixture.0.join("terminal-history")).unwrap();
    assert!(registry.purge_archive(archive, true).is_err());
    assert!(registry.archives[0].purging);
    assert!(registry.restore_archive(archive).is_err());
    drop(registry);
    let mut registry = fixture.open();
    assert!(registry.archive_cleanup_error.is_some());
    assert_eq!(
        std::fs::read_to_string(work.join("1.txt")).unwrap(),
        "untouched"
    );
    std::fs::remove_file(fixture.0.join("terminal-history")).unwrap();
    registry.purge_archive(archive, true).unwrap();
    assert!(registry.archives.is_empty());
}

#[test]
fn startup_and_new_archives_prune_expired_entries_without_touching_recent_or_active_items() {
    let fixture = Fixture::new("startup");
    let mut registry = fixture.open();
    registry
        .create("First".into(), fixture.0.to_str().unwrap().into())
        .unwrap();
    let first = registry.sessions[0].id;
    registry.close(first, false).unwrap();
    registry.archives[0].archived_at = now().unwrap() - RETENTION_SECONDS;
    registry.persist().unwrap();
    drop(registry);
    let mut registry = fixture.open();
    assert!(registry.archives.is_empty());
    registry
        .create("Second".into(), fixture.0.to_str().unwrap().into())
        .unwrap();
    let second = registry.sessions[0].id;
    registry.close(second, false).unwrap();
    registry.archives[0].archived_at = now().unwrap() - RETENTION_SECONDS;
    registry.persist().unwrap();
    registry
        .create("Still active".into(), fixture.0.to_str().unwrap().into())
        .unwrap();
    let active = registry.sessions[0].id;
    let tab = registry.sessions[0].tabs[0].id;
    registry.delete_tab(tab, false).unwrap();
    assert_eq!(registry.archives.len(), 1);
    assert!(matches!(
        registry.archives[0].item,
        ArchivedItem::Tab { .. }
    ));
    assert_eq!(registry.sessions[0].id, active);
}
