use super::*;

#[test]
fn workgroups_persist_defaults_membership_and_folding_without_touching_directories_or_ptys() {
    let data = Directory::new();
    let first = Directory::new();
    let second = Directory::new();
    fs::write(first.0.join("keep"), "first").unwrap();
    fs::write(second.0.join("keep"), "second").unwrap();
    let mut registry = WorkspaceRegistry::open(&data.0).unwrap();
    let group = registry
        .create_workgroup("Projects".into(), first.path())
        .unwrap()
        .workgroups
        .last()
        .unwrap()
        .id;
    let workspace = registry
        .create_in_workgroup(group, "Default path".into(), String::new())
        .unwrap()
        .selected_id
        .unwrap();
    assert_eq!(registry.workspaces[0].base_directory, first.path());
    let tab = registry.workspaces[0].selected_tab_id.unwrap();
    registry.configure_tab(tab, TabConfig { executable: Some("/usr/bin/python3".into()), args: vec!["-u".into(), "-c".into(), "import os,time; print(os.getcwd(),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into()], ..Default::default() }).unwrap();
    let (channel, receive) = output();
    let session = registry.start(tab, 80, 24, channel).unwrap();
    assert_eq!(read_cwd(&registry, tab, &receive), first.path());
    registry.fold_workgroup(group, true).unwrap();
    registry
        .update_workgroup(group, "Renamed".into(), second.path())
        .unwrap();
    assert_eq!(registry.sessions[&tab].id, session);
    assert!(matches!(registry.status(session).unwrap(), Status::Running));
    assert!(registry.close_workgroup(group).is_err());
    registry.sessions[&tab]
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(read_cwd(&registry, tab, &receive), first.path());
    let other = registry
        .create_in_workgroup(group, "New root".into(), String::new())
        .unwrap()
        .selected_id
        .unwrap();
    assert_eq!(
        registry
            .workspaces
            .iter()
            .find(|w| w.id == other)
            .unwrap()
            .base_directory,
        second.path()
    );
    registry
        .update_in_workgroup(
            other,
            registry.workgroups[0].id,
            "Override".into(),
            first.path(),
        )
        .unwrap();
    registry.fold_workgroup(group, true).unwrap();
    registry.select(workspace).unwrap();
    let expected = serde_json::to_value(registry.snapshot()).unwrap();
    registry.shutdown(true).unwrap();
    drop(registry);
    let mut restored = WorkspaceRegistry::open(&data.0).unwrap();
    assert_eq!(serde_json::to_value(restored.snapshot()).unwrap(), expected);
    assert!(restored.workgroup(group).unwrap().collapsed);
    restored.close(workspace, false).unwrap();
    restored.close_workgroup(group).unwrap();
    assert_eq!(fs::read_to_string(first.0.join("keep")).unwrap(), "first");
    assert_eq!(fs::read_to_string(second.0.join("keep")).unwrap(), "second");
    assert_eq!(fs::read_dir(&first.0).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&second.0).unwrap().count(), 1);
    let next = restored
        .create_workgroup("Next".into(), first.path())
        .unwrap()
        .workgroups
        .last()
        .unwrap()
        .id;
    assert!(next > group, "Deleted IDs must not reuse old action files");
}

#[test]
fn version_one_database_migrates_workspaces_and_tabs_into_default_workgroup() {
    let data = Directory::new();
    let directory = Directory::new();
    {
        let database = rusqlite::Connection::open(data.0.join("workspaces.sqlite3")).unwrap();
        database
            .execute_batch(include_str!("../../../migrations/001_metadata.sql"))
            .unwrap();
        database
            .execute(
                "INSERT INTO workspaces VALUES (7,'Original',?1,0,NULL)",
                [directory.path()],
            )
            .unwrap();
        database
            .execute(
                "INSERT INTO tab_definitions VALUES (12,7,'Saved','terminal',?1,0)",
                [r#"{"executable":"printf","args":["literal; 日本語"],"env":{"PORT":"1234"}}"#],
            )
            .unwrap();
        database.execute_batch("INSERT INTO open_tabs VALUES (12,1); UPDATE workspaces SET selected_tab_id=12; UPDATE app_state SET selected_workspace_id=7,workspace_sequence=7,tab_sequence=12;").unwrap();
    }
    let mut registry = WorkspaceRegistry::open(&data.0).unwrap();
    assert_eq!(registry.workgroups[0].name, "Default");
    assert_eq!(registry.workgroups[0].root_directory, directory.path());
    assert_eq!(
        registry.workspaces[0].workgroup_id,
        registry.workgroups[0].id
    );
    assert_eq!(registry.selected_id, Some(7));
    let tab = &registry.workspaces[0].tabs[0];
    assert_eq!(tab.id, 12);
    assert!(tab.is_open && tab.resume_on_start);
    assert_eq!(tab.config.args, ["literal; 日本語"]);
    assert_eq!(tab.config.env["PORT"], "1234");
    registry.fold_workgroup(1, true).unwrap();
    drop(registry);
    let restored = WorkspaceRegistry::open(&data.0).unwrap();
    assert!(restored.workgroups[0].collapsed);
    assert_eq!(restored.next_id, 7);
    assert_eq!(restored.next_tab_id, 12);
}

#[test]
fn failed_workgroup_migration_rolls_back_schema_and_keeps_existing_rows() {
    let data = Directory::new();
    {
        let database = rusqlite::Connection::open(data.0.join("workspaces.sqlite3")).unwrap();
        database
            .execute_batch(include_str!("../../../migrations/001_metadata.sql"))
            .unwrap();
        // A broken legacy relationship must be reported, never silently reset.
        database
            .execute_batch(
                "PRAGMA foreign_keys=OFF; INSERT INTO workspaces VALUES (7,'Keep me','/missing/saved/directory',0,999);",
            )
            .unwrap();
    }
    assert!(WorkspaceRegistry::open(&data.0)
        .err()
        .unwrap()
        .contains("invalid metadata relationships"));
    let database = rusqlite::Connection::open(data.0.join("workspaces.sqlite3")).unwrap();
    assert_eq!(
        database
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .query_row("SELECT name FROM workspaces WHERE id=7", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Keep me"
    );
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='workgroups'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn new_workspace_uses_first_configured_action_and_rejects_invalid_yaml_atomically() {
    let data = Directory::new();
    let config = Directory::new();
    let root = Directory::new();
    let mut registry = WorkspaceRegistry::open(&data.0).unwrap();
    let group = registry.workgroups[0].id;
    crate::actions::load_workgroup(&config.0, group).unwrap();
    let path = crate::actions::workgroup_path(&config.0, group);
    let action = serde_json::json!({"id":"custom", "name":"Project shell", "kind":"terminal", "config":{
        "executable":"/usr/bin/python3", "args":["-u","-c","import os,time; print(os.getcwd(),flush=True); time.sleep(60)"], "env":{"ACTION_TEST":"literal value"}
    }});
    fs::write(&path, serde_json::to_vec(&serde_json::json!({"actions":[action, {"id":"second","name":"Second action","kind":"terminal","config":{"executable":"/bin/false"}}]})).unwrap()).unwrap();
    let snapshot = registry
        .create_from_first_action(Some(group), "Project".into(), root.path(), &config.0)
        .unwrap();
    let workspace = &snapshot.workspaces[0];
    assert_eq!(workspace.tabs.len(), 1);
    let tab = &workspace.tabs[0];
    assert_eq!(tab.name, "Project shell");
    assert_eq!(workspace.selected_tab_id, Some(tab.id));
    assert!(tab.is_open && tab.resume_on_start);
    assert_eq!(tab.config.env["ACTION_TEST"], "literal value");
    let (channel, receive) = output();
    registry
        .connect(tab.id, 80, 24, channel, tab.resume_on_start)
        .unwrap()
        .unwrap();
    assert_eq!(read_cwd(&registry, tab.id, &receive), root.path());
    fs::write(&path, "actions: []").unwrap();
    let expected = serde_json::to_value(registry.snapshot()).unwrap();
    assert!(registry
        .create_from_first_action(Some(group), "Rejected".into(), root.path(), &config.0)
        .is_err());
    assert_eq!(serde_json::to_value(registry.snapshot()).unwrap(), expected);
    registry.shutdown(true).unwrap();
    drop(registry);
    let restored = WorkspaceRegistry::open(&data.0).unwrap();
    assert_eq!(restored.workspaces[0].tabs.len(), 1);
    assert_eq!(restored.workspaces[0].tabs[0].name, "Project shell");
    assert_eq!(
        restored.workspaces[0].tabs[0].config.env["ACTION_TEST"],
        "literal value"
    );
}
