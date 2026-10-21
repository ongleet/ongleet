use super::*;

#[test]
fn workgroup_actions_are_independent_reloadable_and_preserve_legacy_choices() {
    let directory =
        std::env::temp_dir().join(format!("workgroup-actions-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let legacy = DEFAULTS.replace("name: Bash", "name: Custom shell");
    std::fs::write(directory.join("tab-types.yaml"), &legacy).unwrap();
    assert_eq!(
        load_workgroup(&directory, 10).unwrap()[0].name,
        "Custom shell"
    );
    assert_eq!(
        load_workgroup(&directory, 11).unwrap()[0].name,
        "Custom shell"
    );
    let path = workgroup_path(&directory, 10);
    std::fs::write(&path, DEFAULTS.replace("name: Bash", "name: Group ten")).unwrap();
    assert_eq!(load_workgroup(&directory, 10).unwrap()[0].name, "Group ten");
    assert_eq!(
        load_workgroup(&directory, 11).unwrap()[0].name,
        "Custom shell"
    );
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
    assert_eq!(
        std::fs::read_to_string(directory.join("tab-types.yaml")).unwrap(),
        legacy
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn templates_preserve_literal_arguments_and_reject_invalid_modes() {
    let types = parse(DEFAULTS).unwrap();
    assert_eq!(types[0].config.executable.as_deref(), Some("bash"));
    assert_eq!(types[1].config.executable.as_deref(), Some("codex"));
    let input = "tab_types:\n  - id: test\n    name: Test\n    kind: terminal\n    config:\n      executable: printf\n      args: ['a; echo literal', '日本語']\n      env: {PORT: '3000'}\n";
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
        input.trim_start_matches("tab_types:\n")
    ))
    .is_err());
    assert!(parse(&input.replace("executable:", "executabl:")).is_err());
}
