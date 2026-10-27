//! Opt-in Codex conversation restoration. No daemon or global Codex config edits.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, OpenOptionsExt},
            process::CommandExt,
            process::ExitStatusExt,
        },
    },
    path::{Path, PathBuf},
    process::Command,
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

fn nonce() -> Result<String, String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
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

fn write_state(path: &Path, state: &State) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT as usize {
        return Err("Codex session file is too large".into());
    }
    let temp = path.with_extension(format!("{}.tmp", nonce()?));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        File::open(path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(temp);
    result
}

// Serialize generation changes and hook writes, so a stale hook cannot race a
// newer launch between reading its generation and replacing the file.
fn locked<T>(path: &Path, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    locked_for(path, std::time::Duration::ZERO, operation)
}
fn locked_for<T>(
    path: &Path,
    timeout: std::time::Duration,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    struct Lock(File);
    impl Drop for Lock {
        fn drop(&mut self) {
            // Explicitly unlock even if a concurrent fork inherited the fd.
            unsafe {
                libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + timeout;
    while unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if std::time::Instant::now() >= deadline {
            return Err("Codex session mapping is busy; retry the launch".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _guard = Lock(lock);
    operation()
}

fn begin(path: &Path, fresh: bool) -> Result<State, String> {
    locked(path, || {
        let old = read_state(path)?;
        let state = State {
            launch: nonce()?,
            conversation: if fresh { None } else { old.conversation },
            ..State::default()
        };
        write_state(path, &state)?;
        Ok(state)
    })
}

fn record(path: &Path, launch: &str, input: impl Read) -> Result<(), String> {
    record_with(path, launch, input, report_activity)
}
fn record_with(
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
    let changed = locked_for(path, std::time::Duration::from_millis(500), || {
        let mut state = read_state(path)?;
        if state.launch != launch || event.agent_id.is_some() {
            return Ok(false);
        }
        if !apply_event(&mut state, &event)? {
            return Ok(false);
        }
        write_state(path, &state)?;
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
            state
                .pending_questions
                .insert(event.tool_use_id.clone().ok_or("Missing tool_use_id")?);
        }
        "PermissionRequest" => {
            state
                .pending_approvals
                .insert(event.tool_use_id.clone().ok_or("Missing tool_use_id")?);
        }
        "PostToolUse" => {
            if let Some(id) = &event.tool_use_id {
                state.pending_questions.remove(id);
                state.pending_approvals.remove(id);
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

fn report_activity(launch: &str) {
    let result = (|| -> Result<(), String> {
        let data = PathBuf::from(required("ONGLEET_CODEX_DATA")?);
        let socket = crate::control::paths::socket_path(&data)?;
        let id = |key| {
            required(key)?
                .parse::<u64>()
                .map_err(|_| format!("Invalid {key}"))
        };
        let response = crate::control::client::call_with_timeout(
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

fn stopped(path: &Path, launch: &str) -> Result<(), String> {
    let changed = locked_for(path, std::time::Duration::from_millis(500), || {
        let mut state = read_state(path)?;
        if state.launch != launch {
            return Ok(false);
        }
        state.active_turn = false;
        state.activity = Activity::Stopped;
        state.pending_questions.clear();
        state.pending_approvals.clear();
        write_state(path, &state)?;
        Ok(true)
    })?;
    if changed {
        report_activity(launch);
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

fn session_path() -> Result<PathBuf, String> {
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
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    Ok(directory.join(format!("{}.json", id("ONGLEET_TAB_ID")?)))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

const ONGLEET_INSTRUCTIONS: &str = "You are launched through ong-codex, the Ongleet Codex wrapper. Ongleet is a Linux terminal application organized into projects, sessions and tabs. A Ongleet session may contain multiple terminal or other plugin tabs; it is distinct from your Codex conversation. Read ONGLEET_SESSION_ID, ONGLEET_PROJECT_ID and ONGLEET_TAB_ID from the environment for your launch context when available; this context does not follow later project moves or GUI selection changes. The ongleet CLI can inspect and manage the running application: start with ongleet --help, then resource or command --help. Use --json for structured output. Inspect configuration with project config ID or session config ID; project configure ID --file PATH saves actions/env YAML, session configure ID updates properties, and session pull-request ID sets or clears the linked PR. Discover options with command --help. Tab list/actions/create default to the launch session, and session create defaults to the launch project; explicit --session/--project override these defaults. The CLI requires a running app and does not start it automatically. Use it only as needed for the user's task; closing your own tab or session or quitting the app can terminate your process. Wait for the user's task before taking action.";

fn codex_args(state: &State, hook: &str, extra: &[String]) -> Vec<String> {
    let command = format!("{} {RECORD_FLAG}", quote(hook));
    // JSON strings are also valid TOML basic strings for these ASCII commands.
    let command = serde_json::to_string(&command).unwrap();
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

pub fn run_if_requested() -> bool {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        return false;
    };
    if flag != WRAPPER_FLAG && flag != RECORD_FLAG {
        return false;
    }
    run(&flag, args.collect());
    true
}

pub fn run(flag: &str, args: Vec<String>) {
    let result = (|| {
        let mut extra = args;
        if extra.first().is_some_and(|a| a == "--help" || a == "-h") {
            println!("ong-codex [--session-id ID --tab-id ID] [--fresh] [--no-restart] [-- CODEX_OPTIONS...]\nResume this tab's saved Codex conversation, or start one if absent.\nConfigured terminal tabs supply their IDs automatically.\n--fresh explicitly starts a new conversation.\nAutomatically restart after exit, resuming when an ID is saved; close the tab to stop. --no-restart runs once.\nConfigure the tab executable as ong-codex.\nThis wrapper passes --dangerously-bypass-hook-trust for this invocation; all enabled hooks may run without review. Global Codex config is not changed.");
            return Ok(());
        }
        let path = session_path()?;
        if flag == RECORD_FLAG {
            return record(
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
        let hook = required("ONGLEET_CODEX_HOOK_EXE")?;
        let mut state = begin(&path, fresh)?;
        // Keep terminal-generated interrupts directed at Codex without killing
        // its supervisor. The child restores normal dispositions before exec.
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
            libc::signal(libc::SIGQUIT, libc::SIG_IGN);
        }
        let mut rapid_exits = 0;
        loop {
            if let Some(id) = &state.conversation {
                eprintln!("Resuming Codex conversation {id}. If unavailable, use --fresh to start a new conversation.");
            }
            report_activity(&state.launch);
            let started = std::time::Instant::now();
            let mut command = Command::new("codex");
            command
                .args(codex_args(&state, &hook, &extra))
                .env("ONGLEET_CODEX_LAUNCH", &state.launch);
            unsafe {
                command.pre_exec(|| {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    Ok(())
                });
            }
            let status = command.status();
            let _ = stopped(&path, &state.launch);
            let status = status.map_err(|error| format!("Cannot start Codex: {error}"))?;
            if !restart {
                std::process::exit(
                    status
                        .code()
                        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
                );
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
            state = begin(&path, false)?;
        }
    })();
    if let Err(error) = result {
        eprintln!("Codex conversation restoration: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
