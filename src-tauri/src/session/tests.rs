use super::*;
use crate::terminal_session::{Output, Status};
use std::collections::BTreeMap;
use std::{
    fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::ipc::Channel;
use tauri::ipc::InvokeResponseBody;

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "session-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> String {
        self.0.to_str().unwrap().into()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

mod projects;

#[test]
fn codex_wrapper_receives_stable_context_and_records_from_a_contained_pty() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new();
    let fake = directory.0.join("codex");
    fs::write(&fake, r#"#!/usr/bin/python3
import json, os, subprocess, sys, tomllib
args = sys.argv[1:]
config = next(tomllib.loads(args[i+1]) for i, a in enumerate(args) if a == '-c' and args[i+1].startswith('hooks.'))
command = config['hooks']['SessionStart'][0]['hooks'][0]['command']
subprocess.run(command, shell=True, input=json.dumps({'session_id': 'pty-conversation', 'hook_event_name': 'SessionStart'}), text=True, check=True)
print('CONTEXT:' + os.environ['DEVCENTER_SESSION_ID'] + ':' + os.environ['DEVCENTER_TAB_ID'], flush=True)
"#).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let mut registry = SessionRegistry::open(&directory.0).unwrap();
    registry.create("Codex".into(), directory.path()).unwrap();
    let session = registry.sessions[0].id;
    let tab = registry.sessions[0].tabs[0].id;
    registry.sessions[0].tabs[0].config = serde_json::to_value(TabConfig {
        executable: Some("dcodex".into()),
        args: vec!["--no-restart".into()],
        env: BTreeMap::from([
            ("PATH".into(), format!("{}:/usr/bin:/bin", directory.path())),
            ("DEVCENTER_TAB_ID".into(), "999".into()),
        ]),
        ..Default::default()
    })
    .unwrap();
    let (channel, receive) = output();
    let terminal_id = registry.start(tab, 80, 24, channel).unwrap();
    let mut bytes = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Ok(Output::Data { seq, bytes: chunk }) =
            receive.recv_timeout(Duration::from_millis(50))
        {
            bytes.extend(chunk);
            registry.terminals[&tab].acknowledge(seq).unwrap();
        }
        if matches!(registry.status(terminal_id).unwrap(), Status::Exited { .. }) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains(&format!("CONTEXT:{session}:{tab}")), "{text}");
    let saved: serde_json::Value = serde_json::from_slice(
        &fs::read(
            directory
                .0
                .join(format!("codex-sessions/{session}/{tab}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["conversation"], "pty-conversation");
    registry.shutdown(true).unwrap();
}

#[test]
fn metadata_operations_preserve_identity_order_selection_and_directories() {
    let directory = Directory::new();
    fs::write(directory.0.join("keep.txt"), b"untouched").unwrap();
    let other = Directory::new();
    let mut registry = SessionRegistry::default();
    let first = registry
        .create("First".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let second = registry
        .create("Second".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    assert_ne!(first, second);
    registry.select(first).unwrap();
    let state = registry.reorder(first, 1).unwrap();
    assert_eq!(state.selected_id, Some(first));
    assert_eq!(
        state
            .sessions
            .iter()
            .map(|w| (w.id, w.display_order))
            .collect::<Vec<_>>(),
        vec![(second, 0), (first, 1)]
    );
    let state = registry
        .update(first, "Renamed".into(), other.path())
        .unwrap();
    assert_eq!(state.sessions[1].id, first);
    assert_eq!(state.sessions[1].name, "Renamed");
    assert_eq!(state.sessions[1].base_directory, other.path());
    let state = registry.close(first, false).unwrap();
    assert_eq!(state.selected_id, Some(second));
    assert_eq!(state.sessions[0].display_order, 0);
    assert!(registry.close(second, false).unwrap().selected_id.is_none());
    assert_eq!(
        fs::read(directory.0.join("keep.txt")).unwrap(),
        b"untouched"
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    assert!(other.0.is_dir());
}

#[test]
fn invalid_updates_are_atomic_and_missing_directory_does_not_prevent_rename() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::default();
    assert!(registry.create(" ".into(), directory.path()).is_err());
    let id = registry
        .create("Keep".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let missing = directory.0.join("missing").to_str().unwrap().to_owned();
    assert!(registry.create("Invalid".into(), missing.clone()).is_err());
    assert!(registry
        .update(id, "Wrong".into(), missing.clone())
        .is_err());
    assert!(registry.reorder(id, 8).is_err());
    assert!(registry.select(999).is_err());
    assert_eq!(registry.snapshot().selected_id, Some(id));
    assert_eq!(registry.snapshot().sessions[0].name, "Keep");
    assert!(!std::path::Path::new(&missing).exists());
    fs::remove_dir(&directory.0).unwrap();
    let state = registry
        .update(id, "Renamed".into(), directory.path())
        .unwrap();
    assert_eq!(state.sessions[0].name, "Renamed");
    assert_eq!(state.sessions[0].base_directory, directory.path());
    assert!(!directory.0.exists());
}

fn start_test(
    registry: &mut SessionRegistry,
    tab: u64,
    directory: String,
    channel: Channel<Output>,
) -> u64 {
    registry.configure_tab(tab, TabConfig {
            executable: Some("/usr/bin/python3".into()),
            args: vec!["-u".into(), "-c".into(), "import os,time; print(os.getcwd(),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into()],
            cwd_override: Some(directory), ..Default::default()
        }).unwrap();
    registry.start(tab, 80, 24, channel).unwrap()
}

fn output() -> (Channel<Output>, mpsc::Receiver<Output>) {
    let (send, receive) = mpsc::channel();
    let channel = Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            let _ = send.send(serde_json::from_str(&json).unwrap());
        }
        Ok(())
    });
    (channel, receive)
}

fn read_cwd(registry: &SessionRegistry, session: u64, receive: &mpsc::Receiver<Output>) -> String {
    let mut collected = Vec::new();
    loop {
        let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected working directory")
        };
        collected.extend(bytes);
        registry
            .terminals
            .get(&session)
            .unwrap()
            .acknowledge(seq)
            .unwrap();
        if collected.ends_with(b"\n") {
            if let Some(path) = String::from_utf8_lossy(&collected)
                .lines()
                .find(|line| line.starts_with('/'))
            {
                return path.trim().to_owned();
            }
        }
    }
}

#[test]
fn session_navigation_keeps_independent_ptys_and_closure_requires_confirmation() {
    let first_directory = Directory::new();
    let second_directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let first = registry
        .create("First".into(), first_directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let second = registry
        .create("Second".into(), second_directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let first_tab = registry.sessions[0].tabs[0].id;
    let second_tab = registry.sessions[1].tabs[0].id;
    let (channel, receive) = output();
    let first_session = start_test(&mut registry, first_tab, first_directory.path(), channel);
    assert_eq!(
        read_cwd(&registry, first_tab, &receive),
        first_directory.path()
    );
    let session_id = registry.terminals.get(&first_tab).unwrap().id;
    registry.select(second).unwrap();
    registry
        .update(first, "Renamed".into(), second_directory.path())
        .unwrap();
    registry
        .terminals
        .get(&first_tab)
        .unwrap()
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(
        read_cwd(&registry, first_tab, &receive),
        first_directory.path(),
        "Editing session metadata changed a running process's cwd"
    );
    let (channel, _) = output();
    let second_session = start_test(&mut registry, second_tab, second_directory.path(), channel);
    assert_ne!(first_session, second_session);
    registry.select(first).unwrap();
    assert_eq!(registry.terminals.get(&first_tab).unwrap().id, session_id);
    assert!(matches!(
        registry
            .terminals
            .get_mut(&first_tab)
            .unwrap()
            .status()
            .unwrap(),
        Status::Running
    ));
    assert!(registry.close(first, false).is_err());
    assert!(registry.shutdown(false).is_err());
    assert_eq!(registry.snapshot().sessions.len(), 2);
    registry.close(first, true).unwrap();
    assert!(matches!(
        registry
            .terminals
            .get_mut(&second_tab)
            .unwrap()
            .status()
            .unwrap(),
        Status::Running
    ));
    assert_eq!(registry.snapshot().selected_id, Some(second));
    registry.shutdown(true).unwrap();
    assert!(registry.terminals.is_empty());
    assert!(first_directory.0.is_dir() && second_directory.0.is_dir());
}

#[test]
fn tab_order_names_selection_and_last_tab_close_are_session_local() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let first = registry
        .create("First".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let a = registry.sessions[0].tabs[0].id;
    registry.create_tab(first).unwrap();
    let b = registry.sessions[0].selected_tab_id.unwrap();
    let second = registry
        .create("Second".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let c = registry.sessions[1].selected_tab_id.unwrap();
    assert!(a != b && b != c && a != c);
    registry.rename_tab(a, "Shell".into()).unwrap();
    assert!(registry.rename_tab(a, " ".into()).is_err());
    registry.select_tab(a).unwrap();
    registry.reorder_tab(a, 1).unwrap();
    assert_eq!(
        registry.sessions[0]
            .tabs
            .iter()
            .map(|t| (t.id, t.display_order))
            .collect::<Vec<_>>(),
        vec![(b, 0), (a, 1)]
    );
    assert_eq!(registry.sessions[0].selected_tab_id, Some(a));
    assert_eq!(registry.sessions[1].selected_tab_id, Some(c));
    assert_eq!(registry.selected_id, Some(second));
    registry.select(first).unwrap();
    assert_eq!(registry.sessions[0].tabs[1].name, "Shell");
    assert!(registry.reorder_tab(a, 9).is_err());
    registry.close_tab(a, false).unwrap();
    assert_eq!(registry.sessions[0].selected_tab_id, Some(b));
    registry.close_tab(b, false).unwrap();
    assert!(registry.sessions[0].tabs.iter().all(|tab| !tab.is_open));
    assert_eq!(
        registry.sessions[0].tabs.len(),
        2,
        "Closing tabs must retain their definitions"
    );
    assert_eq!(registry.sessions[0].selected_tab_id, None);
    assert_eq!(registry.sessions[1].tabs[0].id, c);
    registry.create_tab(first).unwrap();
    assert!(registry.sessions[0].selected_tab_id.unwrap() > c);
}

#[test]
fn sibling_ptys_survive_tab_changes_and_session_close_preflights_all_sessions() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let session = registry
        .create("Session".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let a = registry.sessions[0].tabs[0].id;
    registry.create_tab(session).unwrap();
    let b = registry.sessions[0].selected_tab_id.unwrap();
    let (channel, receive_a) = output();
    let session_a = start_test(&mut registry, a, directory.path(), channel);
    assert_eq!(read_cwd(&registry, a, &receive_a), directory.path());
    let (channel, receive_b) = output();
    let session_b = start_test(&mut registry, b, directory.path(), channel);
    assert_eq!(read_cwd(&registry, b, &receive_b), directory.path());
    assert_ne!(session_a, session_b);
    registry.rename_tab(a, "Live shell".into()).unwrap();
    registry.reorder_tab(a, 1).unwrap();
    registry.select_tab(a).unwrap();
    let (channel, _) = output();
    assert!(registry.start(a, 80, 24, channel).is_err());
    assert!(registry.close(session, false).is_err());
    assert!(registry.close_tab(a, false).is_err());
    for (tab, session) in [(a, session_a), (b, session_b)] {
        assert_eq!(registry.terminals[&tab].id, session);
        assert!(matches!(
            registry.terminals.get_mut(&tab).unwrap().status().unwrap(),
            Status::Running
        ));
    }
    registry.close_tab(a, true).unwrap();
    assert!(!registry.terminals.contains_key(&a));
    assert!(matches!(
        registry.terminals.get_mut(&b).unwrap().status().unwrap(),
        Status::Running
    ));
    registry.close(session, true).unwrap();
    assert!(registry.terminals.is_empty());
    assert!(registry.sessions.is_empty());
    assert!(directory.0.is_dir());
}

fn read_line(registry: &SessionRegistry, tab: u64, receive: &mpsc::Receiver<Output>) -> String {
    let mut collected = Vec::new();
    loop {
        let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected fixture output")
        };
        collected.extend(bytes);
        registry.terminals[&tab].acknowledge(seq).unwrap();
        if collected.ends_with(b"\n") {
            return String::from_utf8(collected).unwrap().trim().to_owned();
        }
    }
}

#[test]
fn tab_config_launches_direct_args_overrides_default_directory_and_explicit_shell() {
    let old_directory = Directory::new();
    let new_directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let session = registry
        .create("Session".into(), old_directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let a = registry.sessions[0].tabs[0].id;
    let mut config = TabConfig {
            executable: Some("/usr/bin/python3".into()),
            args: vec!["-u".into(), "-c".into(), "import os,sys,json,time; print(json.dumps([os.getcwd(),sys.argv[1],os.environ['TAB_VALUE'],'PATH' in os.environ]),flush=True); input(); print(os.getcwd(),flush=True); time.sleep(60)".into(), "literal; echo $(shell-syntax)".into()],
            env: BTreeMap::from([("TAB_VALUE".into(), "explicit".into())]), ..Default::default()
        };
    registry.configure_tab(a, config.clone()).unwrap();
    let (channel, receive) = output();
    registry.start(a, 80, 24, channel).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(&read_line(&registry, a, &receive)).unwrap();
    assert_eq!(
        payload,
        serde_json::json!([
            old_directory.path(),
            "literal; echo $(shell-syntax)",
            "explicit",
            true
        ])
    );
    registry
        .update(session, "Updated".into(), new_directory.path())
        .unwrap();
    registry.terminals[&a]
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(read_cwd(&registry, a, &receive), old_directory.path());
    registry.create_tab(session).unwrap();
    let b = registry.sessions[0].selected_tab_id.unwrap();
    registry.configure_tab(b, config.clone()).unwrap();
    let (channel, receive) = output();
    registry.start(b, 80, 24, channel).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(&read_line(&registry, b, &receive)).unwrap();
    assert_eq!(payload[0], new_directory.path());
    config.cwd_override = Some(old_directory.path());
    registry.create_tab(session).unwrap();
    let c = registry.sessions[0].selected_tab_id.unwrap();
    registry.configure_tab(c, config).unwrap();
    let (channel, receive) = output();
    registry.start(c, 80, 24, channel).unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(&read_line(&registry, c, &receive)).unwrap();
    assert_eq!(payload[0], old_directory.path());
    assert_eq!(registry.sessions[0].tabs[0].terminal_config().env.len(), 1);
    registry.create_tab(session).unwrap();
    let d = registry.sessions[0].selected_tab_id.unwrap();
    registry
        .configure_tab(
            d,
            TabConfig {
                shell_command: Some("printf '%s\\n' explicit-shell".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let (channel, receive) = output();
    registry.start(d, 80, 24, channel).unwrap();
    assert_eq!(read_line(&registry, d, &receive), "explicit-shell");
    assert!(registry
        .configure_tab(
            d,
            TabConfig {
                env: BTreeMap::from([("INVALID=KEY".into(), "value".into())]),
                ..Default::default()
            }
        )
        .is_err());
    registry.shutdown(true).unwrap();
}

#[test]
fn new_tab_names_avoid_open_sibling_collisions() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let session = registry
        .create("Names".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let create = |registry: &mut SessionRegistry, session| {
        let state = registry
            .create_typed_tab(session, "Bash".into(), TabConfig::default())
            .unwrap();
        let session = state.sessions.iter().find(|w| w.id == session).unwrap();
        let tab = session
            .tabs
            .iter()
            .find(|t| Some(t.id) == session.selected_tab_id)
            .unwrap();
        (tab.id, tab.name.clone())
    };
    let (first, name) = create(&mut registry, session);
    assert_eq!(name, "Bash");
    registry.rename_tab(first, "Bash 2".into()).unwrap();
    assert_eq!(create(&mut registry, session).1, "Bash");
    assert_eq!(create(&mut registry, session).1, "Bash 3");
    registry.close_tab(first, false).unwrap();
    assert_eq!(create(&mut registry, session).1, "Bash 2");
    let other = registry
        .create("Other".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    assert_eq!(create(&mut registry, other).1, "Bash");
}

#[test]
fn reload_connect_reuses_live_session_and_never_starts_stopped_tabs() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::default();
    let session = registry
        .create("Reload".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let tab = registry.snapshot().sessions[0].selected_tab_id.unwrap();
    let (channel, _) = output();
    assert!(registry
        .connect(tab, 80, 24, channel, false)
        .unwrap()
        .is_none());
    let (channel, receive) = output();
    let first = start_test(&mut registry, tab, directory.path(), channel);
    let initial = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(initial, Output::Data { .. }));
    let (channel, _) = output();
    let connection = registry
        .connect(tab, 90, 30, channel, true)
        .unwrap()
        .unwrap();
    assert_eq!(connection.id, first);
    assert!(connection.replay);
    assert_eq!(registry.terminals.len(), 1);
    assert_eq!(registry.snapshot().selected_id, Some(session));
    registry.shutdown(true).unwrap();
}

#[test]
fn yaml_types_reload_without_changing_existing_openings() {
    let data = Directory::new();
    let config_directory = Directory::new();
    let directory = Directory::new();
    let mut registry = SessionRegistry::open(&data.0).unwrap();
    let session = registry
        .create("Types".into(), directory.path())
        .unwrap()
        .selected_id
        .unwrap();
    let types = crate::actions::load(&config_directory.0).unwrap();
    let state = registry
        .create_typed_tab(session, types[0].name.clone(), types[0].terminal_config())
        .unwrap();
    let first = state.sessions[0].selected_tab_id.unwrap();
    let config_path = config_directory.0.join("actions.yaml");
    let updated = "actions:\n  - id: bash\n    name: Custom\n    kind: terminal\n    config:\n      executable: /bin/echo\n      args: ['literal; text']\n";
    fs::write(&config_path, updated).unwrap();
    let types = crate::actions::load(&config_directory.0).unwrap();
    let state = registry
        .create_typed_tab(session, types[0].name.clone(), types[0].terminal_config())
        .unwrap();
    let second = state.sessions[0].selected_tab_id.unwrap();
    assert_ne!(first, second);
    let original = state.sessions[0]
        .tabs
        .iter()
        .find(|t| t.id == first)
        .unwrap();
    assert_eq!(
        original.terminal_config().executable.as_deref(),
        Some("bash")
    );
    assert!(original.is_open && original.resume_on_start);
    fs::write(&config_path, "invalid: [").unwrap();
    assert!(crate::actions::load(&config_directory.0).is_err());
    assert_eq!(fs::read_to_string(&config_path).unwrap(), "invalid: [");
    drop(registry);
    let restored = SessionRegistry::open(&data.0).unwrap();
    let saved = restored.sessions[0]
        .tabs
        .iter()
        .find(|t| t.id == second)
        .unwrap();
    assert_eq!(saved.terminal_config().args, ["literal; text"]);
    assert_eq!(saved.name, "Custom");
}

#[test]
fn sqlite_roundtrip_restores_definitions_openings_selection_and_only_running_resume_flags() {
    let data = Directory::new();
    let directory = Directory::new();
    let (expected, session, live, stopped, closed);
    {
        let mut registry = SessionRegistry::open(&data.0).unwrap();
        session = registry
            .create("Saved session".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        live = registry.sessions[0].selected_tab_id.unwrap();
        registry
            .rename_tab(live, "Development server".into())
            .unwrap();
        let (channel, receive) = output();
        start_test(&mut registry, live, directory.path(), channel);
        assert_eq!(read_cwd(&registry, live, &receive), directory.path());
        registry.create_tab(session).unwrap();
        stopped = registry.sessions[0].selected_tab_id.unwrap();
        registry
            .configure_tab(
                stopped,
                TabConfig {
                    executable: Some("test-command".into()),
                    args: vec!["literal;argument".into()],
                    env: BTreeMap::from([("PORT".into(), "3456".into())]),
                    ..Default::default()
                },
            )
            .unwrap();
        registry.create_tab(session).unwrap();
        closed = registry.sessions[0].selected_tab_id.unwrap();
        registry
            .rename_tab(closed, "Available later".into())
            .unwrap();
        registry.close_tab(closed, false).unwrap();
        registry.select_tab(live).unwrap();
        registry.reorder_tab(live, 1).unwrap();
        registry
            .create("Other session".into(), directory.path())
            .unwrap();
        registry.select(session).unwrap();
        registry.reorder(session, 1).unwrap();
        registry.shutdown(true).unwrap();
        expected = serde_json::to_value(registry.snapshot()).unwrap();
    }
    let mut restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(serde_json::to_value(restored.snapshot()).unwrap(), expected);
    assert!(
        restored.terminals.is_empty(),
        "Loading metadata must not itself spawn disconnected PTYs"
    );
    let saved = restored.sessions.iter().find(|w| w.id == session).unwrap();
    assert_eq!(saved.selected_tab_id, Some(live));
    assert!(
        saved
            .tabs
            .iter()
            .find(|t| t.id == live)
            .unwrap()
            .resume_on_start
    );
    assert!(
        !saved
            .tabs
            .iter()
            .find(|t| t.id == stopped)
            .unwrap()
            .resume_on_start
    );
    let definition = saved.tabs.iter().find(|t| t.id == closed).unwrap();
    assert!(!definition.is_open && !definition.resume_on_start);
    assert_eq!(definition.name, "Available later");
    assert_eq!(
        saved
            .tabs
            .iter()
            .find(|t| t.id == stopped)
            .unwrap()
            .terminal_config()
            .env
            .len(),
        1
    );
    restored.open_tab(closed).unwrap();
    assert!(
        restored
            .sessions
            .iter()
            .find(|w| w.id == session)
            .unwrap()
            .tabs
            .iter()
            .find(|t| t.id == closed)
            .unwrap()
            .resume_on_start
    );
    let old_max = restored.next_tab_id;
    restored.close(session, false).unwrap();
    restored.create_tab(restored.sessions[0].id).unwrap();
    assert!(
        restored.next_tab_id > old_max,
        "Stable IDs cannot be reused after deletion and restart"
    );
}

#[test]
fn sqlite_stop_and_natural_exit_are_not_relaunched_and_missing_directories_are_retained() {
    let data = Directory::new();
    let directory = Directory::new();
    let (session, stopped, exited);
    {
        let mut registry = SessionRegistry::open(&data.0).unwrap();
        session = registry
            .create("Missing later".into(), directory.path())
            .unwrap()
            .selected_id
            .unwrap();
        stopped = registry.sessions[0].selected_tab_id.unwrap();
        let (channel, receive) = output();
        let session = start_test(&mut registry, stopped, directory.path(), channel);
        read_cwd(&registry, stopped, &receive);
        registry.close_terminal_session(session, true).unwrap();
        registry.create_tab(session).unwrap();
        exited = registry.sessions[0].selected_tab_id.unwrap();
        registry
            .configure_tab(
                exited,
                TabConfig {
                    executable: Some("/usr/bin/python3".into()),
                    args: vec!["-c".into(), "raise SystemExit(9)".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        let (channel, _) = output();
        let session = registry.start(exited, 80, 24, channel).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Status::Exited { code } = registry.status(session).unwrap() {
                assert_eq!(code, 9);
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        registry.shutdown(true).unwrap();
    }
    fs::remove_dir(&directory.0).unwrap();
    let mut restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(restored.sessions[0].base_directory, directory.path());
    assert!(restored.sessions[0].tabs.iter().all(|t| !t.resume_on_start));
    assert!(restored.sessions[0].tabs.iter().any(|t| t.id == stopped));
    let (channel, _) = output();
    assert!(restored.start(exited, 80, 24, channel).is_err());
    assert!(!directory.0.exists());
    restored
        .update(session, "Still here".into(), directory.path())
        .unwrap();
    assert_eq!(restored.sessions[0].name, "Still here");
}

#[test]
fn sqlite_failed_writes_preserve_previous_state_and_exclusive_lock_prevents_second_writer() {
    let data = Directory::new();
    let directory = Directory::new();
    let expected;
    {
        let mut registry = SessionRegistry::open(&data.0).unwrap();
        registry.create("Keep me".into(), directory.path()).unwrap();
        assert!(SessionRegistry::open(&data.0).is_err());
        let id = registry.sessions[0].id;
        expected = serde_json::to_value(registry.snapshot()).unwrap();
        registry.store.as_ref().unwrap().reject_writes();
        let group = registry.projects[0].id;
        assert!(registry.fold_project(group, true).is_err());
        assert!(registry
            .update_project(group, "Failed rename".into(), directory.path())
            .is_err());
        assert_eq!(serde_json::to_value(registry.snapshot()).unwrap(), expected);
        assert!(registry
            .update(id, "Should roll back".into(), directory.path())
            .is_err());
        assert_eq!(serde_json::to_value(registry.snapshot()).unwrap(), expected);
        assert!(registry
            .create("Should not appear".into(), directory.path())
            .is_err());
        assert_eq!(serde_json::to_value(registry.snapshot()).unwrap(), expected);
    }
    let restored = SessionRegistry::open(&data.0).unwrap();
    assert_eq!(serde_json::to_value(restored.snapshot()).unwrap(), expected);
}

#[test]
fn sqlite_transaction_rolls_back_after_partial_replacement_and_future_schema_is_not_reset() {
    let data = Directory::new();
    let directory = Directory::new();
    {
        let mut registry = SessionRegistry::open(&data.0).unwrap();
        registry.create("Keep".into(), directory.path()).unwrap();
        let expected = serde_json::to_value(registry.snapshot()).unwrap();
        let mut invalid = registry.last_saved.clone().unwrap();
        invalid.snapshot.sessions[0].name = "Must roll back".into();
        invalid
            .snapshot
            .sessions
            .push(invalid.snapshot.sessions[0].clone());
        let store = registry.store.as_mut().unwrap();
        assert!(
            store.save(&invalid).is_err(),
            "Duplicate identity should fail after earlier statements executed"
        );
        assert_eq!(
            serde_json::to_value(store.load().unwrap().snapshot).unwrap(),
            expected
        );
    }
    {
        let connection = rusqlite::Connection::open(data.0.join("workspaces.sqlite3")).unwrap();
        connection.execute_batch("PRAGMA user_version=99;").unwrap();
    }
    assert!(SessionRegistry::open(&data.0).is_err());
    let connection = rusqlite::Connection::open(data.0.join("workspaces.sqlite3")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT name FROM sessions", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "Keep"
    );
}

#[test]
fn plugin_tabs_restore_mixed_openings_and_unavailable_configuration_without_starting_ptys() {
    let directory = Directory::new();
    let mut registry = SessionRegistry::open(&directory.0).unwrap();
    registry.create("Mixed".into(), directory.path()).unwrap();
    let session = registry.sessions[0].id;
    let terminal = registry.sessions[0].tabs[0].id;
    registry
        .create_plugin_tab(
            session,
            "Files".into(),
            "files".into(),
            1,
            serde_json::json!({}),
        )
        .unwrap();
    let files = registry.next_tab_id;
    registry
        .create_plugin_tab(
            session,
            "PR".into(),
            "pull-request".into(),
            1,
            serde_json::json!({}),
        )
        .unwrap();
    let pr = registry.next_tab_id;
    registry
        .associate_pull_request(
            pr,
            Some(
                crate::plugins::pull_request::Association::parse(
                    "https://github.com/example/repository/pull/42",
                    None,
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(registry
        .create_plugin_tab(
            session,
            "Invalid".into(),
            "files".into(),
            1,
            serde_json::json!({"folder": "relative"})
        )
        .is_err());
    assert!(registry
        .create_plugin_tab(
            session,
            "Future".into(),
            "terminal".into(),
            2,
            serde_json::json!({})
        )
        .is_err());
    assert_eq!(registry.sessions[0].tabs.len(), 3);
    assert!(!registry.sessions[0].tabs.iter().any(|t| t.resume_on_start));
    assert!(registry.terminals.is_empty());
    registry
        .configure_plugin_tab(files, serde_json::json!({"folder":"/missing/retained"}))
        .unwrap();
    assert!(registry
        .configure_plugin_tab(files, serde_json::json!({"executable":"bash"}))
        .is_err());
    registry.reorder_tab(files, 0).unwrap();
    registry.close_tab(pr, false).unwrap();
    registry.select_tab(files).unwrap();
    // Simulate a plugin removed from a subsequent build, retaining opaque payloads.
    let tab = registry.sessions[0]
        .tabs
        .iter_mut()
        .find(|t| t.id == terminal)
        .unwrap();
    tab.kind = "removed-plugin".into();
    tab.config_version = 7;
    tab.config = serde_json::json!({"privateSetting":["preserved"]});
    registry.persist().unwrap();
    drop(registry);
    let mut restored = SessionRegistry::open(&directory.0).unwrap();
    let session = &restored.sessions[0];
    assert_eq!(session.selected_tab_id, Some(files));
    assert_eq!(session.tabs[0].id, files);
    assert_eq!(session.tabs[0].config["folder"], "/missing/retained");
    assert_eq!(
        session.pull_request.as_ref().unwrap().repository,
        "example/repository"
    );
    assert_eq!(session.pull_request.as_ref().unwrap().number, 42);
    let unknown = session.tabs.iter().find(|t| t.id == terminal).unwrap();
    assert_eq!(unknown.config_version, 7);
    assert_eq!(unknown.config["privateSetting"][0], "preserved");
    assert!(!session.tabs.iter().find(|t| t.id == pr).unwrap().is_open);
    assert!(restored.terminals.is_empty());
    assert!(restored.open_tab(terminal).is_err());
    restored.open_tab(pr).unwrap();
    assert!(
        !restored.sessions[0]
            .tabs
            .iter()
            .find(|t| t.id == pr)
            .unwrap()
            .resume_on_start
    );
    restored.close_tab(files, false).unwrap();
    assert!(
        restored.sessions[0]
            .tabs
            .iter()
            .find(|t| t.id == terminal)
            .unwrap()
            .is_open
    );
}
