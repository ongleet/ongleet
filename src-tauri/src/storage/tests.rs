use super::*;

#[test]
fn dropping_store_releases_lock_while_a_fork_duplicate_is_still_open() {
    let directory =
        std::env::temp_dir().join(format!("metadata-lock-release-{}", std::process::id()));
    let store = MetadataStore::open(&directory).unwrap();
    // A concurrent PTY fork temporarily inherits this same open description
    // before CLOEXEC takes effect. It must not extend the store's ownership.
    let inherited = store._lock.0.try_clone().unwrap();
    assert!(MetadataStore::open(&directory).is_err());
    drop(store);
    let replacement = MetadataStore::open(&directory).unwrap();
    drop(inherited);
    assert!(MetadataStore::open(&directory).is_err());
    drop(replacement);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn version_two_rename_preserves_metadata_and_reopens() {
    let directory = std::env::temp_dir().join(format!("metadata-v3-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    {
        let db = Connection::open(directory.join("workspaces.sqlite3")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        db.execute_batch(include_str!("../../migrations/001_metadata.sql"))
            .unwrap();
        db.execute_batch("INSERT INTO workspaces VALUES (7,'Saved session','/missing/preserved',0,NULL); INSERT INTO tab_definitions VALUES (12,7,'Codex','terminal','{\"executable\":\"dcodex\",\"args\":[\"literal; 日本語\"],\"env\":{\"KEEP\":\"yes\"}}',0); INSERT INTO open_tabs VALUES (12,1); UPDATE workspaces SET selected_tab_id=12; UPDATE app_state SET selected_workspace_id=7,workspace_sequence=30,tab_sequence=40;").unwrap();
        db.execute_batch(include_str!("../../migrations/002_workgroups.sql"))
            .unwrap();
        db.execute_batch("UPDATE workgroups SET name='Saved project',collapsed=1; UPDATE app_state SET workgroup_sequence=20;").unwrap();
    }
    let store = MetadataStore::open(&directory).unwrap();
    let metadata = store.load().unwrap();
    assert_eq!(metadata.project_sequence, 20);
    assert_eq!(metadata.session_sequence, 30);
    assert_eq!(metadata.tab_sequence, 40);
    assert_eq!(metadata.snapshot.selected_id, Some(7));
    let project = &metadata.snapshot.projects[0];
    assert_eq!(project.name, "Saved project");
    assert!(project.collapsed);
    let session = &metadata.snapshot.sessions[0];
    assert_eq!(session.id, 7);
    assert_eq!(session.project_id, project.id);
    assert_eq!(session.base_directory, "/missing/preserved");
    assert_eq!(session.selected_tab_id, Some(12));
    let tab = &session.tabs[0];
    assert_eq!(tab.id, 12);
    assert!(tab.is_open && tab.resume_on_start);
    assert_eq!(tab.config.executable.as_deref(), Some("dcodex"));
    assert_eq!(tab.config.args, ["literal; 日本語"]);
    assert_eq!(tab.config.env["KEEP"], "yes");
    assert_eq!(
        store
            .connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        0
    );
    let expected = serde_json::to_value(&metadata.snapshot).unwrap();
    drop(store);
    let mut reopened = MetadataStore::open(&directory).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.load().unwrap().snapshot).unwrap(),
        expected
    );
    reopened.save(&metadata).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.load().unwrap().snapshot).unwrap(),
        expected
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_version_two_relationship_rolls_back_rename() {
    let directory =
        std::env::temp_dir().join(format!("metadata-v3-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let db = Connection::open(directory.join("workspaces.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    db.execute_batch(include_str!("../../migrations/001_metadata.sql"))
        .unwrap();
    db.execute_batch(include_str!("../../migrations/002_workgroups.sql"))
        .unwrap();
    db.execute_batch("INSERT INTO workspaces VALUES (7,'Keep','/missing',0,NULL,999);")
        .unwrap();
    assert!(MetadataStore::open(&directory)
        .err()
        .unwrap()
        .contains("invalid metadata relationships"));
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT name FROM workspaces WHERE id=7", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "Keep"
    );
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}
