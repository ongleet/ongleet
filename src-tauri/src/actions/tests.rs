use super::*;

#[test]
fn workgroup_actions_are_independent_and_reloadable() {
    let directory =
        std::env::temp_dir().join(format!("workgroup-actions-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();

    assert_eq!(load_workgroup(&directory, 10).unwrap()[0].name, "Bash");
    assert_eq!(load_workgroup(&directory, 11).unwrap()[0].name, "Bash");
    let path = workgroup_path(&directory, 10);
    std::fs::write(&path, DEFAULTS.replace("name: Bash", "name: Group ten")).unwrap();
    assert_eq!(load_workgroup(&directory, 10).unwrap()[0].name, "Group ten");
    assert_eq!(load_workgroup(&directory, 11).unwrap()[0].name, "Bash");
    std::fs::write(&path, "not valid: [").unwrap();
    assert!(load_workgroup(&directory, 10)
        .err()
        .unwrap()
        .contains("actions.yaml"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not valid: [");
    std::fs::write(&path, "x".repeat(65_537)).unwrap();
    assert!(load_workgroup(&directory, 10)
        .err()
        .unwrap()
        .contains("64 KiB"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn templates_preserve_literal_arguments_and_reject_invalid_modes() {
    let types = parse(DEFAULTS).unwrap();
    assert_eq!(types[0].config.executable.as_deref(), Some("bash"));
    assert_eq!(types[1].config.executable.as_deref(), Some("codex"));
    let input = "actions:\n  - id: test\n    name: Test\n    kind: terminal\n    config:\n      executable: printf\n      args: ['a; echo literal', '日本語']\n      env: {PORT: '3000'}\n";
    let types = parse(input).unwrap();
    assert_eq!(types[0].config.args, ["a; echo literal", "日本語"]);
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
    load_workgroup(&directory, 8).unwrap();
    load_workgroup(&directory, 9).unwrap();
    let edited = DEFAULTS.replace("name: Bash", "name: Project shell");
    save_workgroup_yaml(&directory, 8, &edited).unwrap();
    assert_eq!(read_workgroup_yaml(&directory, 8).unwrap(), edited);
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
        assert!(save_workgroup_yaml(&directory, 8, &invalid).is_err());
        assert_eq!(read_workgroup_yaml(&directory, 8).unwrap(), edited);
    }
    assert_eq!(load_workgroup(&directory, 9).unwrap()[0].name, "Bash");
    let path = workgroup_path(&directory, 8);
    std::fs::write(&path, "actions: [").unwrap();
    assert_eq!(read_workgroup_yaml(&directory, 8).unwrap(), "actions: [");
    save_workgroup_yaml(&directory, 8, &edited).unwrap();
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    std::fs::remove_dir_all(directory).unwrap();
}
