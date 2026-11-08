//! Opt-in Codex conversation restoration. No daemon or global Codex config edits.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const WRAPPER_FLAG: &str = "--internal-codex-wrapper";
const RECORD_FLAG: &str = "--internal-codex-record";
const LIMIT: u64 = 64 * 1024;

#[derive(Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
struct State {
    launch: String,
    conversation: Option<String>,
    #[serde(default)]
    activity: Activity,
    #[serde(default)]
    pending_questions: std::collections::BTreeSet<String>,
    #[serde(default)]
    pending_approvals: std::collections::BTreeSet<String>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    active_turn: bool,
}

fn nonce(system: &crate::system::SystemServices) -> Result<String, String> {
    system.files.nonce()
}

fn read_state(path: &Path) -> Result<State, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT as usize {
        return Err("Codex session file is too large".into());
    }
    let state: State =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Codex session file: {e}"))?;
    if state.conversation.as_ref().is_some_and(|id| !valid_id(id)) {
        return Err("Invalid saved Codex conversation ID".into());
    }
    Ok(state)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn write_state(
    system: &crate::system::SystemServices,
    path: &Path,
    state: &State,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT as usize {
        return Err("Codex session file is too large".into());
    }
    let temp = path.with_extension(format!("{}.tmp", nonce(system)?));
    let result = (|| {
        let mut file = system
            .files
            .open_private(&temp, crate::system::PrivateOpen::New)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        system
            .files
            .atomic_replace(&temp, path)
            .map_err(|e| e.to_string())?;
        system
            .files
            .sync_directory(path.parent().unwrap())
            .map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(temp);
    result
}

// Serialize generation changes and hook writes, so a stale hook cannot race a
// newer launch between reading its generation and replacing the file.
fn locked<T>(
    system: &crate::system::SystemServices,
    path: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    locked_for(system, path, std::time::Duration::ZERO, operation)
}
fn locked_for<T>(
    system: &crate::system::SystemServices,
    path: &Path,
    timeout: std::time::Duration,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let deadline = std::time::Instant::now() + timeout;
    let _guard = loop {
        match system.files.try_lock(&path.with_extension("lock")) {
            Ok(lock) => break lock,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(_) => return Err("Codex session mapping is busy; retry the launch".into()),
        }
    };
    operation()
}

fn begin(
    system: &crate::system::SystemServices,
    path: &Path,
    fresh: bool,
) -> Result<State, String> {
    locked(system, path, || {
        let old = read_state(path)?;
        let state = State {
            launch: nonce(system)?,
            conversation: if fresh { None } else { old.conversation },
            ..State::default()
        };
        write_state(system, path, &state)?;
        Ok(state)
    })
}

fn record(
    system: &crate::system::SystemServices,
    path: &Path,
    launch: &str,
    input: impl Read,
) -> Result<(), String> {
    record_with(system, path, launch, input, |launch| {
        report_activity(system, launch)
    })
}
fn record_with(
    system: &crate::system::SystemServices,
    path: &Path,
    launch: &str,
    input: impl Read,
    notify: impl Fn(&str),
) -> Result<(), String> {
    // Deserialize directly from the pipe: serde skips unknown fields without
    // retaining prompt/tool content. The mapping size limit must not constrain
    // hook input, which can include arbitrarily large tool responses.
    let event: HookEvent = serde_json::from_reader(std::io::BufReader::new(input))
        .map_err(|e| format!("Invalid Codex hook event: {e}"))?;
    if !valid_id(&event.session_id) {
        return Err("Invalid Codex session_id".into());
    }
    let changed = locked_for(system, path, std::time::Duration::from_millis(500), || {
        let mut state = read_state(path)?;
        if state.launch != launch || event.agent_id.is_some() {
            return Ok(false);
        }
        if !apply_event(&mut state, &event)? {
            return Ok(false);
        }
        write_state(system, path, &state)?;
        Ok(true)
    })?;
    // Never hold the mapping lock while waiting for the GUI. Every notification
    // reads the latest committed state, so concurrent delivery cannot regress it.
    if changed {
        notify(launch);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Activity {
    Working,
    Question,
    Approval,
    #[default]
    Idle,
    Stopped,
}

#[derive(Deserialize)]
struct HookEvent {
    session_id: String,
    hook_event_name: String,
    agent_id: Option<String>,
    turn_id: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
}

fn tool_wait_key(event: &HookEvent) -> Option<String> {
    event.tool_use_id.clone().or_else(|| {
        event
            .tool_name
            .as_ref()
            .map(|name| format!("\0tool:{name}"))
    })
}

fn apply_event(state: &mut State, event: &HookEvent) -> Result<bool, String> {
    let name = event.hook_event_name.as_str();
    if !matches!(
        name,
        "SessionStart"
            | "UserPromptSubmit"
            | "PreToolUse"
            | "PostToolUse"
            | "PermissionRequest"
            | "Stop"
            | "Interrupt"
            | "SessionEnd"
    ) {
        return Err("Unsupported Codex hook event".into());
    }
    if matches!(name, "PreToolUse" | "PostToolUse" | "PermissionRequest") && !state.active_turn {
        return Ok(false);
    }
    if name == "UserPromptSubmit" {
        state.active_turn = true;
        if state.turn != event.turn_id {
            state.pending_questions.clear();
            state.pending_approvals.clear();
        }
        state.turn = event.turn_id.clone();
    } else if event.turn_id.is_some() && state.turn.is_some() && event.turn_id != state.turn {
        return Ok(false);
    }
    match name {
        "SessionStart" => {
            state.conversation = Some(event.session_id.clone());
        }
        "UserPromptSubmit" => {
            state.conversation = Some(event.session_id.clone());
            state.activity = Activity::Working;
        }
        "PreToolUse" if event.tool_name.as_deref() == Some("request_user_input") => {
            if let Some(key) = tool_wait_key(event) {
                state.pending_questions.insert(key);
            }
        }
        "PermissionRequest" => {
            // PermissionRequest has no tool_use_id in Codex's hook schema.
            // Coalesce unidentified waits by tool until that tool completes.
            if let Some(key) = tool_wait_key(event) {
                state.pending_approvals.insert(key);
            }
        }
        "PostToolUse" => {
            if let Some(id) = &event.tool_use_id {
                state.pending_questions.remove(id);
                state.pending_approvals.remove(id);
            }
            if let Some(name) = &event.tool_name {
                let key = format!("\0tool:{name}");
                state.pending_questions.remove(&key);
                state.pending_approvals.remove(&key);
            }
            state.activity = Activity::Working;
        }
        "Stop" | "Interrupt" | "SessionEnd" => {
            state.active_turn = false;
            state.pending_questions.clear();
            state.pending_approvals.clear();
            state.activity = if name == "SessionEnd" {
                Activity::Stopped
            } else {
                Activity::Idle
            };
        }
        _ => {}
    }
    if !state.pending_questions.is_empty() {
        state.activity = Activity::Question;
    } else if !state.pending_approvals.is_empty() {
        state.activity = Activity::Approval;
    }
    Ok(true)
}

// Read only our bounded mapping format, never Codex's private transcript.
#[allow(dead_code)] // The standalone wrapper only sends updates; the GUI reads them.
pub fn activity_for_launch(
    data: &Path,
    session: u64,
    tab: u64,
    launch: &str,
) -> Result<Option<Activity>, String> {
    let state = read_state(
        &data
            .join("codex-sessions")
            .join(session.to_string())
            .join(format!("{tab}.json")),
    )?;
    Ok((state.launch == launch).then_some(state.activity))
}

fn report_activity(system: &crate::system::SystemServices, launch: &str) {
    let result = (|| -> Result<(), String> {
        let data = PathBuf::from(required("ONGLEET_CODEX_DATA")?);
        let socket = system.paths.control_endpoint(&data)?;
        let id = |key| {
            required(key)?
                .parse::<u64>()
                .map_err(|_| format!("Invalid {key}"))
        };
        let response = crate::control::client::call_with_timeout(
            system.transport.as_ref(),
            &socket,
            crate::control::protocol::Command::CodexActivity {
                session_id: id("ONGLEET_SESSION_ID")?,
                tab_id: id("ONGLEET_TAB_ID")?,
                launch: launch.into(),
            },
            std::time::Duration::from_secs(1),
        )
        .map_err(|e| e.message)?;
        if let Some(error) = response.error {
            return Err(error.message);
        }
        Ok(())
    })();
    if std::env::var_os("ONGLEET_DEBUG").is_some_and(|v| v == "1") {
        if let Err(error) = result {
            eprintln!("Codex activity update: {error}");
        }
    }
}

fn stopped(
    system: &crate::system::SystemServices,
    path: &Path,
    launch: &str,
) -> Result<(), String> {
    let changed = locked_for(system, path, std::time::Duration::from_millis(500), || {
        let mut state = read_state(path)?;
        if state.launch != launch {
            return Ok(false);
        }
        state.active_turn = false;
        state.activity = Activity::Stopped;
        state.pending_questions.clear();
        state.pending_approvals.clear();
        write_state(system, path, &state)?;
        Ok(true)
    })?;
    if changed {
        report_activity(system, launch);
    }
    Ok(())
}

fn required(name: &str) -> Result<String, String> {
    std::env::var(name)
        .or_else(|error| {
            if name == "ONGLEET_SESSION_ID" {
                std::env::var("ONGLEET_WORKSPACE_ID")
            } else {
                Err(error)
            }
        })
        .map_err(|_| format!("{name} is required; launch ong-codex from a configured terminal tab"))
}

fn session_path(system: &crate::system::SystemServices) -> Result<PathBuf, String> {
    let data = PathBuf::from(required("ONGLEET_CODEX_DATA")?);
    if !data.is_absolute() {
        return Err("Codex data directory must be absolute".into());
    }
    let id = |key| -> Result<u64, String> {
        required(key)?
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| format!("{key} must be a positive integer"))
    };
    let directory = data
        .join("codex-sessions")
        .join(id("ONGLEET_SESSION_ID")?.to_string());
    system
        .files
        .private_directory(&directory, true)
        .map_err(|e| e.to_string())?;
    Ok(directory.join(format!("{}.json", id("ONGLEET_TAB_ID")?)))
}

const ONGLEET_INSTRUCTIONS: &str = "You are launched through ong-codex, the Ongleet Codex wrapper. Ongleet is a native terminal application organized into projects, sessions and tabs. A Ongleet session may contain multiple terminal or other plugin tabs; it is distinct from your Codex conversation. Read ONGLEET_SESSION_ID, ONGLEET_PROJECT_ID and ONGLEET_TAB_ID from the environment for your launch context when available; this context does not follow later project moves or GUI selection changes. The ongleet CLI can inspect and manage the running application: start with ongleet --help, then resource or command --help. Use --json for structured output. Inspect configuration with project config ID or session config ID; project configure ID --file PATH saves actions/env YAML, session configure ID updates properties, and session pull-request ID sets or clears the linked PR. Discover options with command --help. Tab list/actions/create default to the launch session, and session create defaults to the launch project; explicit --session/--project override these defaults. The CLI requires a running app and does not start it automatically. Use it only as needed for the user's task; closing your own tab or session or quitting the app can terminate your process. Wait for the user's task before taking action.";

fn codex_args(state: &State, command: &str, extra: &[String]) -> Vec<String> {
    // Serialize the native hook command as a TOML basic string.
    let command = serde_json::to_string(command).unwrap();
    let mut args = vec![
        "--no-daemon".into(),
        // Session-flag hooks have no durable source trust hash. This is also
        // AO's launch strategy; it applies to all enabled hooks, not just ours.
        "--dangerously-bypass-hook-trust".into(),
        "-c".into(),
        "features.hooks=true".into(),
        "-c".into(),
        format!(
            "developer_instructions={}",
            serde_json::to_string(ONGLEET_INSTRUCTIONS).unwrap()
        ),
    ];
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "Interrupt",
        "SessionEnd",
    ] {
        let matcher = if event == "PreToolUse" {
            "matcher=\"^request_user_input$\","
        } else {
            ""
        };
        let timeout = if matches!(event, "Interrupt" | "SessionEnd") {
            3
        } else {
            5
        };
        args.extend([
            "-c".into(),
            format!("hooks.{event}=[{{{matcher}hooks=[{{type=\"command\",command={command},timeout={timeout}}}]}}]"),
        ]);
    }
    if let Some(id) = &state.conversation {
        args.extend(["resume".into(), id.clone()]);
    }
    args.extend_from_slice(extra);
    args
}

pub fn run_if_requested(system: &crate::system::SystemServices) -> bool {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        return false;
    };
    if flag != WRAPPER_FLAG && flag != RECORD_FLAG {
        return false;
    }
    run(system, &flag, args.collect());
    true
}

fn log_hook_error(system: &crate::system::SystemServices, error: &str) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "ongleet-codex-hook-error-{}-{}.log",
        std::process::id(),
        nonce(system).map_err(std::io::Error::other)?
    ));
    let mut file = system
        .files
        .open_private(&path, crate::system::PrivateOpen::New)?;
    // Bound diagnostics and never copy the hook's stdin or inherited environment.
    let entry = serde_json::json!({
        "timestampUnixSeconds": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        "pid": std::process::id(),
        "executable": system.paths.application_executable().ok(),
        "error": error.chars().take(4096).collect::<String>(),
    });
    writeln!(file, "{entry}")?;
    Ok(path)
}

pub fn run(system: &crate::system::SystemServices, flag: &str, args: Vec<String>) {
    let result = (|| {
        let mut extra = args;
        if extra.first().is_some_and(|a| a == "--help" || a == "-h") {
            println!("ong-codex [--session-id ID --tab-id ID] [--fresh] [--no-restart] [-- CODEX_OPTIONS...]\nResume this tab's saved Codex conversation, or start one if absent.\nConfigured terminal tabs supply their IDs automatically.\n--fresh explicitly starts a new conversation.\nAutomatically restart after exit, resuming when an ID is saved; close the tab to stop. --no-restart runs once.\nConfigure the tab executable as ong-codex.\nThis wrapper passes --dangerously-bypass-hook-trust for this invocation; all enabled hooks may run without review. Global Codex config is not changed.");
            return Ok(());
        }
        let path = session_path(system)?;
        if flag == RECORD_FLAG {
            return record(
                system,
                &path,
                &required("ONGLEET_CODEX_LAUNCH")?,
                std::io::stdin().lock(),
            );
        }
        let mut fresh = false;
        let mut restart = true;
        while let Some(option) = extra.first().map(String::as_str) {
            match option {
                "--fresh" => fresh = true,
                "--no-restart" => restart = false,
                _ => break,
            }
            extra.remove(0);
        }
        if extra.first().is_some_and(|a| a == "--") {
            extra.remove(0);
        }
        let hook = system
            .paths
            .hook_command(&required("ONGLEET_CODEX_HOOK_EXE")?, &[RECORD_FLAG])?;
        let mut state = begin(system, &path, fresh)?;
        // Keep terminal-generated interrupts directed at Codex without killing
        // its supervisor. The child restores normal dispositions before exec.
        system.processes.prepare_supervisor();
        let mut rapid_exits = 0;
        loop {
            if let Some(id) = &state.conversation {
                eprintln!("Resuming Codex conversation {id}. If unavailable, use --fresh to start a new conversation.");
            }
            report_activity(system, &state.launch);
            let started = std::time::Instant::now();
            let status = system
                .processes
                .spawn(
                    crate::system::ProcessLaunch {
                        executable: "codex".into(),
                        args: codex_args(&state, &hook, &extra),
                        env: std::collections::BTreeMap::from([(
                            "ONGLEET_CODEX_LAUNCH".into(),
                            state.launch.clone(),
                        )]),
                        cwd: None,
                    },
                    crate::system::ProcessMode::Inherited,
                )
                .and_then(|mut resources| resources.child.wait());
            let _ = stopped(system, &path, &state.launch);
            let status = status.map_err(|error| format!("Cannot start Codex: {error}"))?;
            if !restart {
                std::process::exit(status.exit_code);
            }
            rapid_exits = if started.elapsed() < std::time::Duration::from_secs(10) {
                rapid_exits + 1
            } else {
                0
            };
            if rapid_exits >= 5 {
                return Err("Codex exited too quickly five times; automatic restart stopped. Fix the reported error and reopen the tab.".into());
            }
            if read_state(&path)?.conversation.is_some() {
                eprintln!("Codex exited ({status}); resuming in 2 seconds. Close the tab to stop.");
            } else {
                eprintln!("Codex exited ({status}) without a saved conversation ID; starting a new conversation in 2 seconds. Close the tab to stop.");
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
            state = begin(system, &path, false)?;
        }
    })();
    if let Err(error) = result {
        if flag == RECORD_FLAG && std::env::var("ONGLEET_DEBUG").as_deref() == Ok("1") {
            match log_hook_error(system, &error) {
                Ok(path) => eprintln!("Codex hook error log: {}", path.display()),
                Err(log_error) => eprintln!("Cannot write Codex hook error log: {log_error}"),
            }
        }
        eprintln!("Codex conversation restoration: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
