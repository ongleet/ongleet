use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("devcenter-codex-{}", nonce().unwrap()));
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
fn malformed_state_fails_without_resetting_the_file() {
    let fixture = Fixture::new();
    let path = fixture.path();
    fs::write(&path, b"broken").unwrap();
    assert!(begin(&path, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken");
}
