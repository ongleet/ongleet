use super::*;
fn record(path: &Path, launch: &str, input: impl Read) -> Result<(), String> {
    super::record_with(path, launch, input, |_| {})
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("ong-codex-{}", nonce().unwrap()));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn path(&self) -> PathBuf {
        self.0.join("tab.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event(id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"hook_event_name": "SessionStart", "session_id": id}))
        .unwrap()
}

#[test]
fn records_changed_conversations_and_resumes_only_the_matching_tab() {
    let fixture = Fixture::new();
    let path = fixture.path();
    let first = begin(&path, false).unwrap();
    assert!(first.conversation.is_none());
    record(&path, &first.launch, event("conversation-a").as_slice()).unwrap();
    record(&path, &first.launch, event("conversation-b").as_slice()).unwrap();
    record(
        &path,
        &first.launch,
        br#"{"session_id":"conversation-b","hook_event_name":"UserPromptSubmit"}"#.as_slice(),
    )
    .unwrap();
    let next = begin(&path, false).unwrap();
    assert_eq!(next.conversation.as_deref(), Some("conversation-b"));
    let args = codex_args(
        &next,
        "/app path/it's executable",
        &["--model".into(), "example".into()],
    );
    assert!(args
        .windows(2)
        .any(|pair| pair == ["resume", "conversation-b"]));
    assert_eq!(&args[args.len() - 2..], ["--model", "example"]);
    assert!(begin(&fixture.0.join("other-tab.json"), false)
        .unwrap()
        .conversation
        .is_none());
    assert!(begin(&path, true).unwrap().conversation.is_none());
}

#[test]
fn stale_or_invalid_hooks_cannot_replace_the_saved_conversation() {
    let fixture = Fixture::new();
    let path = fixture.path();
    let old = begin(&path, false).unwrap();
    let current = begin(&path, false).unwrap();
    record(&path, &current.launch, event("current").as_slice()).unwrap();
    record(&path, &old.launch, event("stale").as_slice()).unwrap();
    assert!(record(&path, &current.launch, event("../bad").as_slice()).is_err());
    assert!(record(
        &path,
        &current.launch,
        br#"{"session_id":"bad","hook_event_name":"SubagentStart"}"#.as_slice()
    )
    .is_err());
    assert!(record(
        &path,
        &current.launch,
        vec![b'x'; LIMIT as usize + 1].as_slice()
    )
    .is_err());
    assert_eq!(
        read_state(&path).unwrap().conversation.as_deref(),
        Some("current")
    );
}

#[test]
fn large_tool_data_is_skipped_while_activity_metadata_is_recorded() {
    let fixture = Fixture::new();
    let path = fixture.path();
    let state = begin(&path, false).unwrap();
    record(
        &path,
        &state.launch,
        br#"{"session_id":"conversation","hook_event_name":"UserPromptSubmit","turn_id":"turn"}"#
            .as_slice(),
    )
    .unwrap();
    let payload = serde_json::to_vec(&serde_json::json!({
        "tool_input": {"content": "private-input".repeat(100_000)},
        "tool_response": {"output": "private-output".repeat(100_000)},
        "session_id": "conversation",
        "hook_event_name": "PermissionRequest",
        "turn_id": "turn",
        "tool_name": "Bash",
        "tool_use_id": "approval"
    }))
    .unwrap();
    record(&path, &state.launch, payload.as_slice()).unwrap();
    let saved = read_state(&path).unwrap();
    assert_eq!(saved.activity, Activity::Approval);
    assert!(saved.pending_approvals.contains("approval"));
    let mapping = fs::read(&path).unwrap();
    assert!(mapping.len() < 1024);
    assert!(!String::from_utf8(mapping.clone())
        .unwrap()
        .contains("private-"));
    let mut malformed = payload;
    malformed.extend_from_slice(b" trailing garbage");
    assert!(record(&path, &state.launch, malformed.as_slice()).is_err());
    assert_eq!(fs::read(&path).unwrap(), mapping);
}

#[test]
fn malformed_state_fails_without_resetting_the_file() {
    let fixture = Fixture::new();
    let path = fixture.path();
    fs::write(&path, b"broken").unwrap();
    assert!(begin(&path, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken");
}

fn hook(name: &str, turn: &str, tool: &str, call: &str) -> HookEvent {
    serde_json::from_value(serde_json::json!({
        "session_id": "conversation", "hook_event_name": name,
        "turn_id": turn, "tool_name": tool, "tool_use_id": call,
    }))
    .unwrap()
}

#[test]
fn activity_tracks_questions_approvals_and_ignores_unrelated_completions() {
    let mut state = State::default();
    apply_event(&mut state, &hook("UserPromptSubmit", "turn-1", "", "")).unwrap();
    assert_eq!(state.activity, Activity::Working);
    apply_event(
        &mut state,
        &hook("PreToolUse", "turn-1", "request_user_input", "question"),
    )
    .unwrap();
    assert_eq!(state.activity, Activity::Question);
    apply_event(
        &mut state,
        &hook("PermissionRequest", "turn-1", "Bash", "approval"),
    )
    .unwrap();
    apply_event(
        &mut state,
        &hook("PostToolUse", "turn-1", "Bash", "unrelated"),
    )
    .unwrap();
    assert_eq!(state.activity, Activity::Question);
    apply_event(
        &mut state,
        &hook("PostToolUse", "turn-1", "request_user_input", "question"),
    )
    .unwrap();
    assert_eq!(state.activity, Activity::Approval);
    apply_event(
        &mut state,
        &hook("PostToolUse", "turn-1", "Bash", "approval"),
    )
    .unwrap();
    assert_eq!(state.activity, Activity::Working);
    apply_event(&mut state, &hook("Stop", "turn-1", "", "")).unwrap();
    assert_eq!(state.activity, Activity::Idle);
    assert!(!apply_event(&mut state, &hook("PostToolUse", "turn-1", "Bash", "late")).unwrap());
    assert_eq!(state.activity, Activity::Idle);
    apply_event(&mut state, &hook("UserPromptSubmit", "turn-2", "", "")).unwrap();
    assert!(!apply_event(&mut state, &hook("Stop", "turn-1", "", "")).unwrap());
    assert_eq!(state.activity, Activity::Working);
    apply_event(&mut state, &hook("Interrupt", "turn-2", "", "")).unwrap();
    assert_eq!(state.activity, Activity::Idle);
    apply_event(&mut state, &hook("SessionEnd", "turn-2", "", "")).unwrap();
    assert_eq!(state.activity, Activity::Stopped);
}

#[test]
fn activity_reports_only_current_main_agent_and_keeps_mappings_separate() {
    let fixture = Fixture::new();
    let path = fixture.path();
    let old = begin(&path, false).unwrap();
    let launch = begin(&path, false).unwrap();
    let notifications = std::cell::Cell::new(0);
    let notify = |_: &str| notifications.set(notifications.get() + 1);
    let input = serde_json::to_vec(&serde_json::json!({"session_id":"conversation", "hook_event_name":"UserPromptSubmit", "turn_id":"turn"})).unwrap();
    record_with(&path, &old.launch, input.as_slice(), notify).unwrap();
    assert_eq!(notifications.get(), 0);
    let subagent = br#"{"session_id":"conversation","hook_event_name":"UserPromptSubmit","agent_id":"child","turn_id":"turn"}"#;
    record_with(&path, &launch.launch, subagent.as_slice(), notify).unwrap();
    assert_eq!(notifications.get(), 0);
    record_with(&path, &launch.launch, input.as_slice(), notify).unwrap();
    assert_eq!(notifications.get(), 1);
    assert_eq!(read_state(&path).unwrap().activity, Activity::Working);
    let other = fixture.0.join("codex-sessions/1");
    fs::create_dir_all(&other).unwrap();
    fs::copy(&path, other.join("2.json")).unwrap();
    assert_eq!(
        activity_for_launch(&fixture.0, 1, 2, &launch.launch).unwrap(),
        Some(Activity::Working)
    );
    assert_eq!(
        activity_for_launch(&fixture.0, 1, 2, &old.launch).unwrap(),
        None
    );
    assert_eq!(
        activity_for_launch(&fixture.0, 1, 3, &launch.launch).unwrap(),
        None
    );
}

#[test]
fn activity_hook_configuration_is_scoped_and_bounded() {
    let args = codex_args(&State::default(), "/app/ong-codex", &[]);
    for name in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "Interrupt",
        "SessionEnd",
    ] {
        assert!(args
            .iter()
            .any(|arg| arg.starts_with(&format!("hooks.{name}="))));
    }
    let pre = args
        .iter()
        .find(|arg| arg.starts_with("hooks.PreToolUse="))
        .unwrap();
    assert!(pre.contains("matcher=\"^request_user_input$\""));
    assert!(args
        .iter()
        .filter(|arg| arg.starts_with("hooks.Interrupt=") || arg.starts_with("hooks.SessionEnd="))
        .all(|arg| arg.contains("timeout=3")));
}

#[test]
fn hook_notifications_release_mapping_lock_before_waiting_for_gui() {
    let fixture = Fixture::new();
    let path = fixture.path();
    let state = begin(&path, false).unwrap();
    let prompt =
        br#"{"session_id":"conversation","hook_event_name":"UserPromptSubmit","turn_id":"turn"}"#;
    record_with(&path, &state.launch, prompt.as_slice(), |_| {
        let path = path.clone();
        let launch = state.launch.clone();
        std::thread::spawn(move || {
            record_with(&path, &launch, br#"{"session_id":"conversation","hook_event_name":"PreToolUse","turn_id":"turn","tool_name":"request_user_input","tool_use_id":"question"}"#.as_slice(), |_| {})
        }).join().unwrap().unwrap();
    }).unwrap();
    assert_eq!(read_state(&path).unwrap().activity, Activity::Question);
}
