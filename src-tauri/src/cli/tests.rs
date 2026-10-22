use super::*;
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
#[test]
fn parser_requires_explicit_ids_and_confirmation_and_rejects_extra_input() {
    assert!(matches!(
        parse(args(&["workspace", "close", "12"])).unwrap(),
        Invocation::Request(Command::WorkspaceClose {
            id: 12,
            confirmed: false
        })
    ));
    assert!(matches!(
        parse(args(&["workspace", "close", "12", "--confirm", "--json"])).unwrap(),
        Invocation::Request(Command::WorkspaceClose {
            id: 12,
            confirmed: true
        })
    ));
    assert!(
        matches!(parse(args(&["workspace","create","--workgroup","2","--name","Name with spaces"])).unwrap(),Invocation::Request(Command::WorkspaceCreate {workgroup_id:2,directory,..}) if directory.is_empty())
    );
    for values in [
        vec!["workspace", "close", "0"],
        vec!["workspace", "close", "-1"],
        vec!["workspace", "list", "--confirm"],
        vec!["workspace", "create", "--name", "Missing group"],
        vec!["workspace", "rename", "1", "--name", "A", "--name", "B"],
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
        r#"{"version":1,"command":"workspace_list","unexpected":true}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Request>(
        r#"{"version":1,"command":"workspace_close","id":1,"confirmed":"yes"}"#
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
        vec!["workspace", "--help"],
        vec!["workspace", "create", "--help"],
        vec!["--json", "workspace", "close", "-h"],
        vec!["app", "quit", "--help"],
    ] {
        let Invocation::Help(text) = parse(args(&values)).unwrap() else {
            panic!("Help must never execute a request: {values:?}");
        };
        assert!(text.contains("AUTOMATION CONTRACT"));
    }
    let Invocation::Help(text) = parse(args(&["workspace", "create", "--help"])).unwrap() else {
        panic!("Expected help");
    };
    assert!(text.contains("workspace create --workgroup ID --name NAME"));
    assert!(text.contains("poll workspace list"));
    assert!(!text.contains("workspace rename ID"));
    for values in [
        vec!["unknown", "--help"],
        vec!["workspace", "unknown", "--help"],
        vec!["app", "quit", "--help", "--confirm"],
    ] {
        assert!(parse(args(&values)).is_err(), "{values:?}");
    }
    // A literal name is not a help request.
    assert!(matches!(
        parse(args(&["workspace", "rename", "1", "--name", "-h"])).unwrap(),
        Invocation::Request(Command::WorkspaceRename { name, .. }) if name == "-h"
    ));
}
