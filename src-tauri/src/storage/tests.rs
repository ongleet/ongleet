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
    assert_eq!(tab.terminal_config().executable.as_deref(), Some("dcodex"));
    assert_eq!(tab.terminal_config().args, ["literal; 日本語"]);
    assert_eq!(tab.terminal_config().env["KEEP"], "yes");
    assert_eq!(
        store
            .connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        6
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

#[test]
fn plugin_migration_rolls_back_all_payloads_when_one_legacy_payload_is_invalid() {
    let directory =
        std::env::temp_dir().join(format!("metadata-plugin-rollback-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("workspaces.sqlite3");
    {
        let db = Connection::open(&path).unwrap();
        db.execute_batch(include_str!("../../migrations/001_metadata.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/002_workgroups.sql"))
            .unwrap();
        db.execute_batch(include_str!("../../migrations/003_projects_sessions.sql"))
            .unwrap();
        db.execute_batch("INSERT INTO sessions VALUES (1,'Saved','/missing',0,NULL,1); INSERT INTO tab_definitions VALUES (1,1,'Valid','terminal','{\"executable\":\"bash\"}',0),(2,1,'Invalid','terminal','invalid-json',1); INSERT INTO open_tabs VALUES (1,1),(2,0); UPDATE sessions SET selected_tab_id=2 WHERE id=1; UPDATE app_state SET selected_session_id=1,session_sequence=1,tab_sequence=2;").unwrap();
    }
    assert!(MetadataStore::open(&directory).is_err());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row(
            "SELECT config_json FROM tab_definitions WHERE id=1",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        r#"{"executable":"bash"}"#
    );
    assert_eq!(
        db.query_row(
            "SELECT config_json FROM tab_definitions WHERE id=2",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "invalid-json"
    );
    assert_eq!(
        db.query_row("SELECT SUM(resume_on_start) FROM open_tabs", [], |row| row
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT selected_tab_id FROM sessions WHERE id=1",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        2
    );
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn both_schema_four_variants_upgrade_without_rewrapping_or_changing_metadata() {
    for existing_column in [false, true] {
        let directory = std::env::temp_dir().join(format!(
            "metadata-v4-{}-{existing_column}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("workspaces.sqlite3");
        let payload = r#"{"version":1,"config":{"executable":"bash","args":["literal"],"env":{"KEEP":"yes"}}}"#;
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(include_str!("../../migrations/001_metadata.sql"))
                .unwrap();
            db.execute_batch(include_str!("../../migrations/002_workgroups.sql"))
                .unwrap();
            db.execute_batch(include_str!("../../migrations/003_projects_sessions.sql"))
                .unwrap();
            db.execute_batch("INSERT INTO sessions VALUES (7,'Saved','/missing',0,NULL,1); UPDATE app_state SET selected_session_id=7,session_sequence=9,tab_sequence=20; PRAGMA user_version=4;").unwrap();
            db.execute(
                "INSERT INTO tab_definitions VALUES (12,7,'Saved terminal','terminal',?1,0)",
                [payload],
            )
            .unwrap();
            db.execute_batch(
                "INSERT INTO open_tabs VALUES (12,1); UPDATE sessions SET selected_tab_id=12;",
            )
            .unwrap();
            if existing_column {
                db.execute_batch("ALTER TABLE sessions ADD COLUMN pull_request_json TEXT; UPDATE sessions SET pull_request_json='{\"repository\":\"example/repo\",\"number\":42}';").unwrap();
            }
        }
        let mut store = MetadataStore::open(&directory).unwrap();
        let metadata = store.load().unwrap();
        assert_eq!(metadata.snapshot.selected_id, Some(7));
        assert_eq!(metadata.session_sequence, 9);
        assert_eq!(metadata.tab_sequence, 20);
        let session = &metadata.snapshot.sessions[0];
        assert_eq!(session.selected_tab_id, Some(12));
        assert_eq!(
            session.pull_request.as_ref().map(|pr| pr.number),
            existing_column.then_some(42)
        );
        let tab = &session.tabs[0];
        assert!(tab.is_open && tab.resume_on_start);
        assert_eq!(tab.config_version, 1);
        assert_eq!(tab.config["args"][0], "literal");
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT config_json FROM tab_definitions WHERE id=12",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            payload
        );
        assert_eq!(
            store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            6
        );
        store.save(&metadata).unwrap();
        drop(store);
        let reopened = MetadataStore::open(&directory).unwrap();
        assert_eq!(
            reopened.load().unwrap().snapshot.sessions[0].tabs[0].config["env"]["KEEP"],
            "yes"
        );
        drop(reopened);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn archive_migration_failure_rolls_back_and_keeps_existing_metadata() {
    let directory =
        std::env::temp_dir().join(format!("metadata-archive-migration-{}", std::process::id()));
    let store = MetadataStore::open(&directory).unwrap();
    store.connection.execute_batch("DROP TABLE archives; DROP TABLE archive_state; CREATE TABLE archives(conflicting_column TEXT); PRAGMA user_version=5;").unwrap();
    drop(store);
    assert!(MetadataStore::open(&directory).is_err());
    let db = Connection::open(directory.join("workspaces.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('archive_state')",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM app_state", [], |row| row
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}
