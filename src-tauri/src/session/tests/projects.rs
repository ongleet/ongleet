use super::*;

#[test]
fn project_reordering_persists_without_changing_sessions_or_selection() {
    let data = Directory::new();
    let root = Directory::new();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    let first = registry.projects[0].id;
    registry
        .create_project("Second".into(), root.path())
        .unwrap();
    let second = registry.projects[1].id;
    registry
        .create_project("Third".into(), root.path())
        .unwrap();
    let third = registry.projects[2].id;
    registry
        .create_in_project(second, "Keep".into(), root.path())
        .unwrap();
    registry.fold_project(second, true).unwrap();
    let sessions = serde_json::to_value(&registry.snapshot().sessions).unwrap();
    let selected = registry.selected_id;
    registry.reorder_project(third, 0).unwrap();
    registry.reorder_project(first, 2).unwrap();
    assert!(registry.reorder_project(second, 3).is_err());
    assert!(registry.reorder_project(u64::MAX, 0).is_err());
    drop(registry);
    let restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(
        restored
            .projects
            .iter()
            .map(|p| (p.id, p.display_order))
            .collect::<Vec<_>>(),
        vec![(third, 0), (second, 1), (first, 2)]
    );
    assert!(restored.project(second).unwrap().collapsed);
    assert_eq!(restored.selected_id, selected);
    assert_eq!(
        serde_json::to_value(&restored.snapshot().sessions).unwrap(),
        sessions
    );
}

#[test]
fn projects_persist_defaults_membership_and_folding_without_touching_directories_or_ptys() {
    let data = Directory::new();
    let first = Directory::new();
    let second = Directory::new();
    fs::write(first.0.join("keep"), "first").unwrap();
    fs::write(second.0.join("keep"), "second").unwrap();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    let group = registry
        .create_project("Projects".into(), first.path())
        .unwrap()
        .projects
        .last()
        .unwrap()
        .id;
    let session = registry
        .create_in_project(group, "Default path".into(), String::new())
        .unwrap()
        .selected_id
        .unwrap();
    assert_eq!(registry.sessions[0].base_directory, first.path());
    let tab = registry.sessions[0].selected_tab_id.unwrap();
    registry.configure_tab(tab, TabConfig { executable: Some("/usr/bin/python3".into()), args: vec!["-u".into(), "-c".into(), "import os,time; print(os.getcwd(),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into()], ..Default::default() }).unwrap();
    let (channel, receive) = output();
    let terminal_id = registry.start(tab, 80, 24, channel).unwrap();
    assert_eq!(read_cwd(&registry, tab, &receive), first.path());
    registry.fold_project(group, true).unwrap();
    registry
        .update_project(group, "Renamed".into(), second.path())
        .unwrap();
    assert_eq!(registry.terminals[&tab].id, terminal_id);
    assert!(matches!(
        registry.status(terminal_id).unwrap(),
        Status::Running
    ));
    assert!(registry.close_project(group).is_err());
    registry.terminals[&tab]
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(read_cwd(&registry, tab, &receive), first.path());
    let other = registry
        .create_in_project(group, "New root".into(), String::new())
        .unwrap()
        .selected_id
        .unwrap();
    assert_eq!(
        registry
            .sessions
            .iter()
            .find(|w| w.id == other)
            .unwrap()
            .base_directory,
        second.path()
    );
    registry
        .update_in_project(
            other,
            registry.projects[0].id,
            "Override".into(),
            first.path(),
        )
        .unwrap();
    registry.fold_project(group, true).unwrap();
    registry.select(session).unwrap();
    let expected = serde_json::to_value(registry.snapshot()).unwrap();
    registry.shutdown(true).unwrap();
    drop(registry);
    let mut restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(serde_json::to_value(restored.snapshot()).unwrap(), expected);
    assert!(restored.project(group).unwrap().collapsed);
    restored.close(session, false).unwrap();
    restored.close_project(group).unwrap();
    assert_eq!(fs::read_to_string(first.0.join("keep")).unwrap(), "first");
    assert_eq!(fs::read_to_string(second.0.join("keep")).unwrap(), "second");
    assert_eq!(fs::read_dir(&first.0).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&second.0).unwrap().count(), 1);
    let next = restored
        .create_project("Next".into(), first.path())
        .unwrap()
        .projects
        .last()
        .unwrap()
        .id;
    assert!(next > group, "Deleted IDs must not reuse old action files");
}

#[test]
fn version_one_database_migrates_sessions_and_tabs_into_default_project() {
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
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(registry.projects[0].name, "Default");
    assert_eq!(registry.projects[0].root_directory, directory.path());
    assert_eq!(registry.sessions[0].project_id, registry.projects[0].id);
    assert_eq!(registry.selected_id, Some(7));
    let tab = &registry.sessions[0].tabs[0];
    assert_eq!(tab.id, 12);
    assert!(tab.is_open && tab.resume_on_start);
    assert_eq!(tab.config.args, ["literal; 日本語"]);
    assert_eq!(tab.config.env["PORT"], "1234");
    registry.fold_project(1, true).unwrap();
    drop(registry);
    let restored = SessionRegistry::open(&data.0).unwrap();
    assert!(restored.projects[0].collapsed);
    assert_eq!(restored.next_id, 7);
    assert_eq!(restored.next_tab_id, 12);
}

#[test]
fn failed_project_migration_rolls_back_schema_and_keeps_existing_rows() {
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
    assert!(SessionRegistry::open(&data.0)
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
fn new_session_uses_first_configured_action_and_rejects_invalid_yaml_atomically() {
    let data = Directory::new();
    let config = Directory::new();
    let root = Directory::new();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    let group = registry.projects[0].id;
    crate::actions::load_project(&config.0, group).unwrap();
    let path = crate::actions::project_path(&config.0, group);
    let action = serde_json::json!({"id":"custom", "name":"Project shell", "kind":"terminal", "config":{
        "executable":"/usr/bin/python3", "args":["-u","-c","import os,time; print(os.getcwd(),flush=True); time.sleep(60)"], "env":{"ACTION_TEST":"literal value"}
    }});
    fs::write(&path, serde_json::to_vec(&serde_json::json!({"actions":[action, {"id":"second","name":"Second action","kind":"terminal","config":{"executable":"/bin/false"}}]})).unwrap()).unwrap();
    let snapshot = registry
        .create_from_first_action(Some(group), "Project".into(), root.path(), &config.0)
        .unwrap();
    let session = &snapshot.sessions[0];
    assert_eq!(session.tabs.len(), 1);
    let tab = &session.tabs[0];
    assert_eq!(tab.name, "Project shell");
    assert_eq!(session.selected_tab_id, Some(tab.id));
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
    let restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(restored.sessions[0].tabs.len(), 1);
    assert_eq!(restored.sessions[0].tabs[0].name, "Project shell");
    assert_eq!(
        restored.sessions[0].tabs[0].config.env["ACTION_TEST"],
        "literal value"
    );
}

#[test]
fn common_environment_is_loaded_for_new_launches_and_action_overrides_win() {
    let data = Directory::new();
    let config = Directory::new();
    let root = Directory::new();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    registry.set_actions_directory(config.0.clone());
    let group = registry.projects[0].id;
    crate::actions::load_project(&config.0, group).unwrap();
    let mut document = serde_json::json!({"env": {"COMMON": "first $HOME; a", "SHARED": "group", "EMPTY": "", "HOME": "/common-home"}, "actions": [{"id":"probe", "name":"Probe", "kind":"terminal", "config": {
        "executable":"/usr/bin/python3", "args":["-u","-c","import os,sys,json,time,tty; tty.setraw(0); report=lambda: print(json.dumps([os.getenv('COMMON'),os.getenv('SHARED'),os.getenv('EMPTY'),os.getenv('HOME'),'PATH' in os.environ]),flush=True); report(); sys.stdin.readline(); report(); time.sleep(60)"], "env":{"SHARED":"action"}
    }}]});
    crate::actions::save_project_yaml(&config.0, group, &document.to_string()).unwrap();
    let session = registry
        .create_from_first_action(Some(group), "Probe session".into(), root.path(), &config.0)
        .unwrap()
        .selected_id
        .unwrap();
    let tab = registry.sessions[0].selected_tab_id.unwrap();
    let (channel, receive) = output();
    let id = registry.start(tab, 80, 24, channel).unwrap();
    let expected = serde_json::json!(["first $HOME; a", "action", "", "/common-home", true]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read_line(&registry, tab, &receive)).unwrap(),
        expected
    );
    document["env"]["COMMON"] = "second".into();
    crate::actions::save_project_yaml(&config.0, group, &document.to_string()).unwrap();
    let saved = registry.sessions[0].tabs[0].config.clone();
    assert_eq!(
        saved.env.len(),
        1,
        "Do not persist merged/inherited environments in tab metadata"
    );
    registry.terminals[&tab]
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read_line(&registry, tab, &receive)).unwrap(),
        expected
    );
    let second = registry
        .create_typed_tab(session, "Another".into(), saved.clone())
        .unwrap()
        .sessions[0]
        .selected_tab_id
        .unwrap();
    let (channel, second_output) = output();
    registry.start(second, 80, 24, channel).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&read_line(&registry, second, &second_output))
            .unwrap(),
        serde_json::json!(["second", "action", "", "/common-home", true])
    );
    assert!(matches!(registry.status(id).unwrap(), Status::Running));
    registry.shutdown(true).unwrap();
    drop(registry);
    let restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(restored.sessions[0].tabs[0].config.env, saved.env);
    assert_eq!(
        crate::actions::parse_config(&crate::actions::read_project_yaml(&config.0, group).unwrap())
            .unwrap()
            .env["COMMON"],
        "second"
    );
}

#[test]
fn external_actions_resolve_context_without_tabs_and_default_skips_commands() {
    let data = Directory::new();
    let config = Directory::new();
    let root = Directory::new();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    let group = registry.projects[0].id;
    let document = serde_json::json!({"env":{"COMMON":"{{session.name}}","OVERRIDE":"group"},"actions":[
        {"id":"editor","name":"Editor","kind":"command","config":{"executable":"code","cwdOverride":"{{session.path}}","args":["--new-window","{{session.path}}","{{session.name}}"],"env":{"OVERRIDE":"action"}}},
        {"id":"bash","name":"Bash","kind":"terminal","config":{"executable":"bash"}}
    ]});
    crate::actions::save_project_yaml(&config.0, group, &document.to_string()).unwrap();
    let snapshot = registry
        .create_from_first_action(
            Some(group),
            "A ; $(literal) {{braces}}".into(),
            root.path(),
            &config.0,
        )
        .unwrap();
    let id = snapshot.selected_id.unwrap();
    assert_eq!(snapshot.sessions[0].tabs[0].name, "Bash");
    let prepared = registry.prepare_external(id, "editor", &config.0).unwrap();
    assert_eq!(
        prepared.config.args,
        vec![
            "--new-window".to_string(),
            root.path(),
            "A ; $(literal) {{braces}}".into()
        ]
    );
    assert_eq!(prepared.config.env["COMMON"], "A ; $(literal) {{braces}}");
    assert_eq!(prepared.config.env["OVERRIDE"], "action");
    assert_eq!(prepared.config.env["SESSION_PATH"], root.path());
    assert_eq!(registry.sessions[0].tabs.len(), 1);
    assert!(registry.terminals.is_empty());
    assert!(registry.prepare_external(id, "bash", &config.0).is_err());
    assert!(registry.prepare_external(id, "absent", &config.0).is_err());
    let mut only_external = document;
    only_external["actions"].as_array_mut().unwrap().pop();
    crate::actions::save_project_yaml(&config.0, group, &only_external.to_string()).unwrap();
    assert!(registry
        .create_from_first_action(Some(group), "No default".into(), root.path(), &config.0)
        .is_err());
    assert_eq!(registry.sessions.len(), 1);
}
