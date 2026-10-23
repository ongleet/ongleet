use super::*;

#[test]
fn project_actions_are_independent_and_reloadable() {
    let directory =
        std::env::temp_dir().join(format!("project-actions-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();

    assert_eq!(load_project(&directory, 10).unwrap()[0].name, "Bash");
    assert_eq!(load_project(&directory, 11).unwrap()[0].name, "Bash");
    let path = project_path(&directory, 10);
    std::fs::write(&path, DEFAULTS.replace("name: Bash", "name: Group ten")).unwrap();
    assert_eq!(load_project(&directory, 10).unwrap()[0].name, "Group ten");
    assert_eq!(load_project(&directory, 11).unwrap()[0].name, "Bash");
    std::fs::write(&path, "not valid: [").unwrap();
    assert!(load_project(&directory, 10)
        .err()
        .unwrap()
        .contains("actions.yaml"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not valid: [");
    std::fs::write(&path, "x".repeat(65_537)).unwrap();
    assert!(load_project(&directory, 10)
        .err()
        .unwrap()
        .contains("64 KiB"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn templates_preserve_literal_arguments_and_reject_invalid_modes() {
    let types = parse(DEFAULTS).unwrap();
    assert_eq!(
        types[0].terminal_config().executable.as_deref(),
        Some("bash")
    );
    assert_eq!(
        types[1].terminal_config().executable.as_deref(),
        Some("dcodex")
    );
    let input = "actions:\n  - id: test\n    name: Test\n    kind: terminal\n    config:\n      executable: printf\n      args: ['a; echo literal', '日本語']\n      env: {PORT: '3000'}\n";
    let types = parse(input).unwrap();
    assert_eq!(
        types[0].terminal_config().args,
        ["a; echo literal", "日本語"]
    );
    assert!(parse(&input.replace(
        "executable: printf",
        "executable: printf\n      shellCommand: echo bad"
    ))
    .is_err());
    assert!(parse(&input.replace("kind: terminal", "kind: browser")).is_err());
    assert!(parse(&format!(
        "{input}{}",
        input.trim_start_matches("actions:\n")
    ))
    .is_err());
    assert!(parse(&input.replace("executable:", "executabl:")).is_err());
}

#[test]
fn action_editor_roundtrips_and_rejects_invalid_saves_without_overwriting() {
    let directory = std::env::temp_dir().join(format!("action-editor-test-{}", std::process::id()));
    load_project(&directory, 8).unwrap();
    load_project(&directory, 9).unwrap();
    let edited = DEFAULTS.replace("name: Bash", "name: Project shell");
    save_project_yaml(&directory, 8, &edited).unwrap();
    assert_eq!(read_project_yaml(&directory, 8).unwrap(), edited);
    assert_eq!(
        parse(&format(parse_config(&edited).unwrap()).unwrap()).unwrap()[0].name,
        "Project shell"
    );
    for invalid in [
        "actions: []".into(),
        "actions: [".into(),
        "x".repeat(65_537),
        DEFAULTS.replacen("actions:", "tab_types:", 1),
        DEFAULTS.replace("env: {}", "env: {'BAD=KEY': value}"),
        DEFAULTS.replace("args: []", "args: []\n      cwdOverride: relative/path"),
    ] {
        assert!(save_project_yaml(&directory, 8, &invalid).is_err());
        assert_eq!(read_project_yaml(&directory, 8).unwrap(), edited);
    }
    assert_eq!(load_project(&directory, 9).unwrap()[0].name, "Bash");
    let path = project_path(&directory, 8);
    std::fs::write(&path, "actions: [").unwrap();
    assert_eq!(read_project_yaml(&directory, 8).unwrap(), "actions: [");
    save_project_yaml(&directory, 8, &edited).unwrap();
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn project_config_migration_preserves_custom_yaml_and_refuses_conflicts() {
    let directory =
        std::env::temp_dir().join(format!("project-config-migration-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("workgroups/7")).unwrap();
    let custom = b"# custom\nactions: []\n# {{workspace.path}} must remain literal\n";
    std::fs::write(directory.join("workgroups/7/actions.yaml"), custom).unwrap();
    std::fs::create_dir_all(directory.join("projects")).unwrap();
    assert!(migrate_projects(&directory).unwrap_err().contains("Both"));
    assert_eq!(
        std::fs::read(directory.join("workgroups/7/actions.yaml")).unwrap(),
        custom
    );
    std::fs::remove_dir(directory.join("projects")).unwrap();
    migrate_projects(&directory).unwrap();
    migrate_projects(&directory).unwrap();
    assert_eq!(std::fs::read(project_path(&directory, 7)).unwrap(), custom);
    assert!(!directory.join("workgroups").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unavailable_project_actions_do_not_block_available_actions_or_common_environment() {
    let directory =
        std::env::temp_dir().join(format!("actions-unavailable-{}", std::process::id()));
    load_project(&directory, 1).unwrap();
    let path = project_path(&directory, 1);
    let mut config: serde_json::Value =
        serde_json::to_value(parse_config(DEFAULTS).unwrap()).unwrap();
    config["env"] = serde_json::json!({"COMMON":"retained"});
    config["actions"].as_array_mut().unwrap().push(serde_json::json!({"id":"removed","name":"Removed plugin","kind":"unavailable","configVersion":7,"config":{"opaque":[1,2,3]}}));
    let text = config.to_string();
    std::fs::write(&path, &text).unwrap();
    let loaded = load_project(&directory, 1).unwrap();
    assert_eq!(loaded[0].kind, "terminal");
    assert_eq!(
        loaded.last().unwrap().config["opaque"],
        serde_json::json!([1, 2, 3])
    );
    assert_eq!(
        parse_project_config(&text).unwrap().env["COMMON"],
        "retained"
    );
    assert!(save_project_yaml(&directory, 1, &text).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    std::fs::remove_dir_all(directory).unwrap();
}
