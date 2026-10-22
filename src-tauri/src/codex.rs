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
    let temp = path.with_extension(format!("{}.tmp", nonce()?));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(state).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
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
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Codex session mapping is busy; retry the launch".into());
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
        };
        write_state(path, &state)?;
        Ok(state)
    })
}

fn record(path: &Path, launch: &str, input: impl Read) -> Result<(), String> {
    let mut bytes = Vec::new();
    input
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT as usize {
        return Err("Codex hook payload is too large".into());
    }
    #[derive(Deserialize)]
    struct Event {
        session_id: String,
        hook_event_name: String,
    }
    let event: Event =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Codex hook event: {e}"))?;
    if !matches!(
        event.hook_event_name.as_str(),
        "SessionStart" | "UserPromptSubmit"
    ) || !valid_id(&event.session_id)
    {
        return Err("Expected SessionStart or UserPromptSubmit with a valid session_id".into());
    }
    locked(path, || {
        let mut state = read_state(path)?;
        if state.launch != launch {
            return Ok(());
        }
        state.conversation = Some(event.session_id);
        write_state(path, &state)
    })
}

fn required(name: &str) -> Result<String, String> {
    std::env::var(name)
        .or_else(|error| {
            if name == "DEVCENTER_SESSION_ID" {
                std::env::var("DEVCENTER_WORKSPACE_ID")
            } else {
                Err(error)
            }
        })
        .map_err(|_| format!("{name} is required; launch dcodex from a configured terminal tab"))
}

fn session_path() -> Result<PathBuf, String> {
    let data = PathBuf::from(required("DEVCENTER_CODEX_DATA")?);
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
        .join(id("DEVCENTER_SESSION_ID")?.to_string());
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    Ok(directory.join(format!("{}.json", id("DEVCENTER_TAB_ID")?)))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

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
    ];
    for event in ["SessionStart", "UserPromptSubmit"] {
        args.extend([
            "-c".into(),
            format!("hooks.{event}=[{{hooks=[{{type=\"command\",command={command},timeout=5}}]}}]"),
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
            println!("dcodex [--session-id ID --tab-id ID] [--fresh] [-- CODEX_OPTIONS...]\nResume this tab's saved Codex conversation, or start one if absent.\nConfigured terminal tabs supply their IDs automatically.\n--fresh explicitly starts a new conversation. Configure the tab executable as dcodex.\nThis wrapper passes --dangerously-bypass-hook-trust for this invocation; all enabled hooks may run without review. Global Codex config is not changed.");
            return Ok(());
        }
        let path = session_path()?;
        if flag == RECORD_FLAG {
            return record(
                &path,
                &required("DEVCENTER_CODEX_LAUNCH")?,
                std::io::stdin().lock(),
            );
        }
        let fresh = extra.first().is_some_and(|a| a == "--fresh");
        if fresh {
            extra.remove(0);
        }
        if extra.first().is_some_and(|a| a == "--") {
            extra.remove(0);
        }
        let hook = required("DEVCENTER_CODEX_HOOK_EXE")?;
        let state = begin(&path, fresh)?;
        if let Some(id) = &state.conversation {
            eprintln!("Resuming Codex conversation {id}. If unavailable, use --fresh to start a new conversation.");
        }
        let error = Command::new("codex")
            .args(codex_args(&state, &hook, &extra))
            .env("DEVCENTER_CODEX_LAUNCH", &state.launch)
            .exec();
        Err(format!("Cannot start Codex: {error}"))
    })();
    if let Err(error) = result {
        eprintln!("Codex conversation restoration: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
