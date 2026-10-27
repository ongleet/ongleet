use super::*;

fn parse(args: Vec<String>) -> Result<Invocation, RpcError> {
    super::parse_with_context(args, None, None)
}
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
#[test]
fn parser_requires_explicit_ids_and_confirmation_and_rejects_extra_input() {
    assert!(matches!(
        parse(args(&["session", "close", "12"])).unwrap(),
        Invocation::Request(Command::SessionClose {
            id: 12,
            confirmed: false
        })
    ));
    assert!(matches!(
        parse(args(&["session", "close", "12", "--confirm", "--json"])).unwrap(),
        Invocation::Request(Command::SessionClose {
            id: 12,
            confirmed: true
        })
    ));
    assert!(
        matches!(parse(args(&["session","create","--project","2","--name","Name with spaces"])).unwrap(),Invocation::Request(Command::SessionCreate {project_id:2,directory,..}) if directory.is_empty())
    );
    for values in [
        vec!["session", "close", "0"],
        vec!["session", "close", "-1"],
        vec!["session", "list", "--confirm"],
        vec!["session", "create", "--name", "Missing group"],
        vec!["session", "rename", "1", "--name", "A", "--name", "B"],
        vec!["app", "quit", "--unexpected", "yes"],
    ] {
        assert!(parse(args(&values)).is_err(), "{values:?}");
    }
}
#[test]
fn protocol_rejects_unknown_fields_and_accepts_activation_frame() {
    use crate::control::protocol::Request;
    assert!(matches!(
        serde_json::from_str::<Request>(r#"{"version":1,"command":"show"}"#)
            .unwrap()
            .command,
        Command::Show
    ));
    assert!(serde_json::from_str::<Request>(
        r#"{"version":1,"command":"session_list","unexpected":true}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Request>(
        r#"{"version":1,"command":"session_close","id":1,"confirmed":"yes"}"#
    )
    .is_err());
}

#[test]
fn socket_reader_bounds_bytes_and_wait_time_on_real_unix_streams() {
    use std::{
        io::Write,
        net::Shutdown,
        os::unix::net::UnixStream,
        time::{Duration, Instant},
    };
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    client::verify_peer(&reader).unwrap();
    writer.write_all(b"abcdef").unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    assert_eq!(
        client::read_message(&mut reader, 5, Duration::from_secs(1))
            .unwrap_err()
            .code,
        "message_too_large"
    );
    let (mut reader, _writer) = UnixStream::pair().unwrap();
    let start = Instant::now();
    assert_eq!(
        client::read_message(&mut reader, 5, Duration::from_millis(50))
            .unwrap_err()
            .code,
        "timeout"
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    writer.write_all("界".as_bytes()).unwrap();
    writer.shutdown(Shutdown::Write).unwrap();
    assert_eq!(
        client::read_message(&mut reader, 3, Duration::from_secs(1)).unwrap(),
        "界".as_bytes()
    );
}

#[test]
fn oversized_response_returns_a_bounded_structured_error() {
    use crate::control::protocol::{encode_response, RESPONSE_LIMIT};
    let frame = encode_response(&Response::success(Some(
        serde_json::json!({"name": "x".repeat(RESPONSE_LIMIT)}),
    )));
    assert!(frame.len() <= RESPONSE_LIMIT);
    let response: Response = serde_json::from_slice(&frame).unwrap();
    assert!(!response.ok);
    assert_eq!(response.error.unwrap().code, "response_too_large");
}

#[test]
fn scoped_help_needs_no_target_or_required_options() {
    for values in [
        vec!["session", "--help"],
        vec!["session", "create", "--help"],
        vec!["--json", "session", "close", "-h"],
        vec!["app", "quit", "--help"],
    ] {
        let Invocation::Help(text) = parse(args(&values)).unwrap() else {
            panic!("Help must never execute a request: {values:?}");
        };
        assert!(text.contains("ongleet automation --help"));
        assert!(!text.contains("AUTOMATION CONTRACT"));
    }
    let Invocation::Help(text) = parse(args(&["session", "create", "--help"])).unwrap() else {
        panic!("Expected help");
    };
    assert!(text.contains("session create [--project ID] --name NAME"));
    assert!(text.contains("poll session list"));
    assert!(!text.contains("session rename ID"));
    for values in [
        vec!["unknown", "--help"],
        vec!["session", "unknown", "--help"],
        vec!["app", "quit", "--help", "--confirm"],
    ] {
        assert!(parse(args(&values)).is_err(), "{values:?}");
    }
    // A literal name is not a help request.
    assert!(matches!(
        parse(args(&["session", "rename", "1", "--name", "-h"])).unwrap(),
        Invocation::Request(Command::SessionRename { name, .. }) if name == "-h"
    ));
}

#[test]
fn tab_commands_target_definitions_and_require_explicit_delete_confirmation() {
    assert!(matches!(
        parse(args(&["tab", "delete", "7"])).unwrap(),
        Invocation::Request(Command::TabDelete {
            id: 7,
            confirmed: false
        })
    ));
    assert!(matches!(
        parse(args(&["tab", "delete", "7", "--confirm"])).unwrap(),
        Invocation::Request(Command::TabDelete {
            id: 7,
            confirmed: true
        })
    ));
    assert!(matches!(
        parse(args(&["tab", "list", "--session", "3"])).unwrap(),
        Invocation::Request(Command::TabList { session_id: 3 })
    ));
    assert!(matches!(
        parse(args(&["tab", "create", "--session", "3", "--action", "bash"])).unwrap(),
        Invocation::Request(Command::TabCreate { session_id: 3, action_id, .. }) if action_id == "bash"
    ));
    assert!(matches!(
        parse(args(&["tab", "configure", "7", "--config", r#"{"executable":"/bin/bash"}"#])).unwrap(),
        Invocation::Request(Command::TabConfigure { id: 7, config }) if config["executable"] == "/bin/bash"
    ));
    for values in [
        vec!["tab", "delete", "0"],
        vec!["tab", "delete", "7", "--confirm", "--confirm"],
        vec!["tab", "configure", "7", "--config", "invalid"],
        vec!["tab", "create", "--session", "3"],
    ] {
        assert!(parse(args(&values)).is_err(), "{values:?}");
    }
}

#[test]
fn tab_discovery_and_creation_overrides_are_available_to_agents() {
    assert!(matches!(
        parse(args(&["tab", "types"])).unwrap(),
        Invocation::Request(Command::TabTypes)
    ));
    assert!(matches!(
        parse(args(&["tab", "actions", "--session", "3"])).unwrap(),
        Invocation::Request(Command::TabActions { session_id: 3 })
    ));
    assert!(matches!(
        parse(args(&["tab", "create", "--session", "3", "--action", "bash",
            "--name", "Agent task", "--config", r#"{"executable":"/usr/bin/python3","args":["-V"]}"#])).unwrap(),
        Invocation::Request(Command::TabCreate { session_id: 3, action_id, name: Some(name), config: Some(config) })
            if action_id == "bash" && name == "Agent task" && config["args"][0] == "-V"
    ));
    for values in [
        vec!["tab", "actions"],
        vec!["tab", "types", "--session", "3"],
        vec![
            "tab",
            "create",
            "--session",
            "3",
            "--action",
            "bash",
            "--config",
            "invalid",
        ],
    ] {
        assert!(parse(args(&values)).is_err(), "{values:?}");
    }
    let Invocation::Help(help) = parse(args(&["tab", "--help"])).unwrap() else {
        panic!("Expected tab help");
    };
    assert!(help.contains("tab types"));
    assert!(help.contains("tab actions [--session ID]"));
    assert!(help.contains("[--name NAME] [--config JSON]"));
}

#[test]
fn terminal_context_supplies_defaults_with_explicit_options_taking_precedence() {
    for verb in ["list", "actions", "create"] {
        let mut values = vec!["tab", verb];
        if verb == "create" {
            values.extend(["--action", "bash"]);
        }
        let invocation = super::parse_with_context(args(&values), Some("12"), Some("9")).unwrap();
        match invocation {
            Invocation::Request(
                Command::TabList { session_id }
                | Command::TabActions { session_id }
                | Command::TabCreate { session_id, .. },
            ) => assert_eq!(session_id, 12),
            _ => panic!("Expected session-targeted command"),
        }
        values.extend(["--session", "5"]);
        let invocation = super::parse_with_context(args(&values), Some("invalid"), None).unwrap();
        match invocation {
            Invocation::Request(
                Command::TabList { session_id }
                | Command::TabActions { session_id }
                | Command::TabCreate { session_id, .. },
            ) => assert_eq!(session_id, 5),
            _ => panic!("Expected explicit session"),
        }
    }
    assert!(matches!(
        super::parse_with_context(
            args(&["session", "create", "--name", "New"]),
            Some("12"),
            Some("9")
        )
        .unwrap(),
        Invocation::Request(Command::SessionCreate { project_id: 9, .. })
    ));
    assert!(matches!(
        super::parse_with_context(
            args(&["session", "create", "--name", "New", "--project", "2"]),
            None,
            Some("invalid")
        )
        .unwrap(),
        Invocation::Request(Command::SessionCreate { project_id: 2, .. })
    ));
    for value in [None, Some(""), Some("0"), Some("bad")] {
        assert!(super::parse_with_context(args(&["tab", "list"]), value, None).is_err());
    }
    // Context does not silently retarget destructive commands.
    assert!(super::parse_with_context(args(&["session", "close"]), Some("12"), Some("9")).is_err());
    assert!(super::parse_with_context(args(&["tab", "delete"]), Some("12"), Some("9")).is_err());
}

#[test]
fn help_is_progressive_and_keeps_automation_details_in_their_own_topic() {
    let help = |values: &[&str]| {
        let Invocation::Help(text) = parse(args(values)).unwrap() else {
            panic!("Help must not execute commands");
        };
        text
    };
    let root = help(&["--help"]);
    assert!(root.contains("RESOURCES"));
    assert!(!root.contains("--confirm"));
    assert!(!root.contains("EXIT CODES"));
    assert!(!root.contains("configSchema"));
    assert!(root.lines().count() <= 20);

    let tabs = help(&["tab", "--help"]);
    assert!(tabs.contains("tab delete ID [--confirm]"));
    assert!(!tabs.contains("session create"));
    assert!(!tabs.contains("retaining its definition"));
    assert!(!tabs.contains("EXIT CODES"));

    let create = help(&["tab", "create", "--help"]);
    assert!(create.contains("ONGLEET_SESSION_ID"));
    assert!(create.contains("tab types"));
    assert!(create.contains("before launch"));
    assert!(!create.contains("project close"));
    assert!(!create.contains("EXIT CODES"));
    assert!(create.lines().count() <= 20);

    let delete = help(&["tab", "delete", "--help"]);
    assert!(delete.contains("Archive a tab definition for 30 days"));
    assert!(delete.contains("require --confirm"));

    let automation = help(&["automation", "--help"]);
    assert!(automation.contains("AUTOMATION CONTRACT"));
    assert!(automation.contains("EXIT CODES"));
    assert!(automation.contains("confirmation required"));
    assert!(parse(args(&["automation", "unknown", "--help"])).is_err());
}

#[test]
fn human_output_preserves_ids_status_configuration_and_escapes_terminal_controls() {
    let response = Response::success(Some(serde_json::json!({
        "selectedTabId": 42,
        "tabs": [
            {"id": 42, "name": "Agent shell", "kind": "terminal",
             "status": {"state": "exited", "code": 23},
             "config": {"env": {"PATH": "/bin", "MixedCase_Key": "value"}},
             "isOpen": true},
            {"id": 81, "name": "bad\u{1b}[2J\nname", "status": {"state": "closed"}}
        ]
    })));
    let text = output::success(&response, false);
    assert!(text.contains("ID: 42"));
    assert!(text.contains("Selected tab: 42"));
    assert!(text.contains("Name: Agent shell"));
    assert!(text.contains("state: exited"));
    assert!(text.contains("code: 23"));
    assert!(text.contains("state: closed"));
    assert!(text.contains("Open: yes"));
    assert!(text.contains("PATH: /bin"));
    assert!(text.contains("MixedCase_Key: value"));
    assert!(!text.contains('\u{1b}'));
    assert!(text.contains(r"bad\u{1b}[2J\nname"));
    assert!(serde_json::from_str::<serde_json::Value>(&text).is_err());
    let json: serde_json::Value = serde_json::from_str(&output::success(&response, true)).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["tabs"][0]["status"]["code"], 23);
    assert_eq!(json["data"]["tabs"][1]["name"], "bad\u{1b}[2J\nname");
    assert_eq!(output::success(&Response::success(None), false), "Done.");
    assert!(output::success(
        &Response::success(Some(serde_json::json!({"tabs": []}))),
        false
    )
    .contains("(empty)"));
}

#[test]
fn output_mode_applies_to_errors_without_changing_exit_codes() {
    let error = || {
        RpcError::new(
            "confirmation_required",
            "Running terminal requires --confirm",
        )
    };
    assert_eq!(error().exit_code(), 5);
    assert_eq!(
        output::error(error(), false),
        "Error (confirmation_required): Running terminal requires --confirm"
    );
    let json: serde_json::Value = serde_json::from_str(&output::error(error(), true)).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "confirmation_required");
}

#[test]
fn configuration_commands_preserve_file_content_and_require_explicit_targets() {
    let path = std::env::temp_dir().join(format!("ongleet-cli-config-{}.yaml", std::process::id()));
    let yaml = "# Keep comments\nenv:\n  VALUE: 'literal $value'\nactions: []\n";
    std::fs::write(&path, yaml).unwrap();
    let path_arg = path.to_str().unwrap();
    let parsed = parse(args(&["project", "configure", "9", "--file", path_arg])).unwrap();
    assert!(
        matches!(parsed, Invocation::Request(Command::ProjectConfigure { id: 9, yaml: text }) if text == yaml)
    );
    std::fs::write(&path, vec![b'x'; 65537]).unwrap();
    assert!(parse(args(&["project", "configure", "9", "--file", path_arg])).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(parse(args(&["project", "configure", "9", "--file", path_arg])).is_err());
    assert!(
        matches!(parse(args(&["session", "configure", "12", "--name", "New name", "--project", "3"])).unwrap(),
        Invocation::Request(Command::SessionConfigure { id: 12, name: Some(name), directory: None, project_id: Some(3) }) if name == "New name")
    );
    assert!(
        matches!(parse(args(&["session", "pull-request", "12", "--target", "none"])).unwrap(),
        Invocation::Request(Command::SessionPullRequest { id: 12, target, number: None }) if target == "none")
    );
    for values in [
        vec!["session", "configure", "12"],
        vec!["session", "configure", "12", "--project", "0"],
        vec!["project", "configure", "9"],
        vec![
            "session",
            "pull-request",
            "12",
            "--target",
            "org/repo",
            "--number",
            "0",
        ],
        vec!["session", "config"],
        vec!["project", "config"],
    ] {
        assert!(
            super::parse_with_context(args(&values), Some("12"), Some("9")).is_err(),
            "{values:?}"
        );
    }
    let request = crate::control::protocol::Request {
        version: crate::control::protocol::VERSION,
        command: Command::ProjectConfigure {
            id: 9,
            yaml: "#\n".repeat(32768),
        },
    };
    let encoded = serde_json::to_vec(&request).unwrap();
    assert!(encoded.len() < crate::control::protocol::REQUEST_LIMIT);
    assert!(serde_json::from_slice::<crate::control::protocol::Request>(&encoded).is_ok());
}

#[test]
fn archive_commands_expose_recovery_and_require_confirmation_for_permanent_removal() {
    assert!(matches!(
        parse(args(&["session", "archive", "7", "--confirm"])).unwrap(),
        Invocation::Request(Command::SessionClose {
            id: 7,
            confirmed: true
        })
    ));
    assert!(matches!(
        parse(args(&["tab", "archive", "8"])).unwrap(),
        Invocation::Request(Command::TabDelete {
            id: 8,
            confirmed: false
        })
    ));
    assert!(matches!(
        parse(args(&["archive", "list"])).unwrap(),
        Invocation::Request(Command::ArchiveList)
    ));
    assert!(matches!(
        parse(args(&["archive", "restore", "9"])).unwrap(),
        Invocation::Request(Command::ArchiveRestore { id: 9 })
    ));
    assert!(matches!(
        parse(args(&["archive", "purge", "9", "--confirm"])).unwrap(),
        Invocation::Request(Command::ArchivePurge {
            id: 9,
            confirmed: true
        })
    ));
    assert!(matches!(
        parse(args(&["archive", "prune", "--confirm"])).unwrap(),
        Invocation::Request(Command::ArchivePrune { confirmed: true })
    ));
    assert!(parse(args(&["archive", "purge"])).is_err());
    assert!(parse(args(&["archive", "restore", "9", "--confirm"])).is_err());
    assert!(parse(args(&["archive", "restore", "9", "--session", "10"])).is_err());
    let Invocation::Help(help) = parse(args(&["archive", "purge", "--help"])).unwrap() else {
        panic!()
    };
    assert!(help.contains("--confirm"));
    assert!(help.contains("Working") || help.contains("working"));
}

#[test]
fn tag_commands_only_list_add_and_remove_with_explicit_targets() {
    assert!(matches!(
        parse(args(&["session", "tags", "12"])).unwrap(),
        Invocation::Request(Command::SessionTags { id: 12 })
    ));
    assert!(parse(args(&["session", "tags", "12", "--tags", "[]"])).is_err());
    assert!(
        matches!(parse(args(&["session", "tag-add", "12", "--name", "Ready", "--dot", "#22c55e", "--background", "#ffffff", "--priority", "50"])).unwrap(), Invocation::Request(Command::SessionTagAdd { id: 12, name, dot: Some(dot), background: Some(bg), priority: 50 }) if name == "Ready" && dot == "#22c55e" && bg == "#ffffff")
    );
    assert!(
        matches!(parse(args(&["session", "tag-remove", "12", "--name", "Ready"])).unwrap(), Invocation::Request(Command::SessionTagRemove { id: 12, name }) if name == "Ready")
    );
    for input in [
        vec!["session", "tag-add", "12"],
        vec!["session", "tag-remove", "12"],
        vec!["session", "tags"],
        vec![
            "session",
            "tag-add",
            "12",
            "--name",
            "Ready",
            "--priority",
            "1.5",
        ],
    ] {
        assert!(parse(args(&input)).is_err());
    }
}

#[test]
fn terminal_input_preserves_text_requires_explicit_enter_and_bounds_utf8() {
    for enter in [false, true] {
        let mut values = args(&["tab", "input", "12", "--text", "café 日本語; $(literal)"]);
        if enter {
            values.push("--enter".into());
        }
        assert!(
            matches!(parse(values).unwrap(), Invocation::Request(Command::TabInput { id: 12, text, enter: actual }) if text == "café 日本語; $(literal)" && actual == enter)
        );
    }
    assert!(matches!(
        parse(args(&["tab", "input", "12", "--text", "--literal"])).unwrap(),
        Invocation::Request(Command::TabInput { text, .. }) if text == "--literal"
    ));
    let mut values = args(&["tab", "input", "12", "--text"]);
    values.push("é".repeat(4096));
    assert!(parse(values.clone()).is_ok());
    values.push("--enter".into());
    assert!(parse(values).is_err());
    for values in [
        vec!["tab", "input", "12"],
        vec!["tab", "input", "0", "--text", "hello"],
        vec![
            "tab", "input", "12", "--text", "hello", "--enter", "--enter",
        ],
        vec!["tab", "select", "12", "--enter"],
    ] {
        assert!(parse(args(&values)).is_err());
    }
}
