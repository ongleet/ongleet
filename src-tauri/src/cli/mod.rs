use crate::control::{
    client, paths,
    protocol::{Command, Response, RpcError},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::{Command as ProcessCommand, Stdio},
    time::{Duration, Instant},
};

mod help;
mod output;

#[derive(Debug)]
pub enum Invocation {
    Help(String),
    Request(Command),
    Start(Option<PathBuf>),
}
fn invalid(message: impl Into<String>) -> RpcError {
    RpcError::new("invalid_argument", message)
}
fn positive(value: &str) -> Result<u64, RpcError> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("IDs must be positive integers"))
}
fn absolute(value: String) -> Result<String, RpcError> {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|e| invalid(e.to_string()))?
            .join(path)
    };
    path.into_os_string()
        .into_string()
        .map_err(|_| invalid("Directory must be valid UTF-8"))
}

pub fn parse(args: Vec<String>) -> Result<Invocation, RpcError> {
    let session = std::env::var_os("DEVCENTER_SESSION_ID");
    let project = std::env::var_os("DEVCENTER_PROJECT_ID");
    parse_with_context(
        args,
        session.as_ref().map(|s| s.to_string_lossy()).as_deref(),
        project.as_ref().map(|s| s.to_string_lossy()).as_deref(),
    )
}

fn parse_with_context(
    args: Vec<String>,
    session: Option<&str>,
    project: Option<&str>,
) -> Result<Invocation, RpcError> {
    let mut args = args;
    if args.first().is_some_and(|arg| arg == "--json") {
        args.remove(0);
    }
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(Invocation::Help(help::render(&[])?));
    }
    // Only recognize help in the command prefix: option values may literally be "-h".
    if let Some(index) = args
        .iter()
        .take(3)
        .position(|arg| arg == "--help" || arg == "-h")
    {
        if index != args.len() - 1 {
            return Err(invalid(
                "Help must follow the resource or command, without extra arguments",
            ));
        }
        return Ok(Invocation::Help(help::render(&args[..index])?));
    }
    if args.len() < 2 {
        return Err(invalid("Expected a resource and command; use --help"));
    }
    let resource = args[0].as_str();
    let verb = args[1].as_str();
    let needs_id = matches!(
        (resource, verb),
        (
            "tab",
            "select" | "rename" | "reorder" | "open" | "close" | "delete" | "archive" | "configure"
        ) | (
            "session",
            "rename"
                | "directory"
                | "move"
                | "reorder"
                | "select"
                | "close"
                | "archive"
                | "config"
                | "configure"
                | "pull-request"
        ) | (
            "project",
            "rename" | "directory" | "fold" | "unfold" | "close" | "config" | "configure"
        )
    );
    let needs_id = needs_id || matches!((resource, verb), ("archive", "restore" | "purge"));
    let mut offset = 2;
    let id = if needs_id {
        let id = positive(args.get(2).ok_or_else(|| invalid("Missing target ID"))?)?;
        offset = 3;
        id
    } else {
        0
    };
    let mut options = BTreeMap::new();
    let mut confirmed = false;
    while offset < args.len() {
        let key = &args[offset];
        if key == "--json" {
            offset += 1;
            continue;
        }
        if key == "--confirm" {
            if confirmed {
                return Err(invalid("Duplicate --confirm"));
            }
            confirmed = true;
            offset += 1;
            continue;
        }
        if !key.starts_with("--") {
            return Err(invalid(format!("Unexpected argument: {key}")));
        }
        let value = args
            .get(offset + 1)
            .ok_or_else(|| invalid(format!("Missing value for {key}")))?;
        if value.starts_with("--") {
            return Err(invalid(format!("Missing value for {key}")));
        }
        if options.insert(key.clone(), value.clone()).is_some() {
            return Err(invalid(format!("Duplicate option: {key}")));
        }
        offset += 2;
    }
    if confirmed
        && !matches!(
            (resource, verb),
            ("session", "close" | "archive")
                | ("tab", "close" | "delete" | "archive")
                | ("archive", "purge" | "prune")
                | ("app", "quit")
        )
    {
        return Err(invalid(
            "--confirm applies only to tab close/delete/archive, session close/archive, archive purge/prune or app quit",
        ));
    }
    let context = match (resource, verb) {
        ("tab", "list" | "actions" | "create") => {
            Some(("--session", "DEVCENTER_SESSION_ID", session))
        }
        ("session", "create") => Some(("--project", "DEVCENTER_PROJECT_ID", project)),
        _ => None,
    };
    if let Some((option, variable, fallback)) = context {
        if !options.contains_key(option) {
            let value = fallback.ok_or_else(|| {
                invalid(format!(
                    "Required option: {option} (or {variable} inside a DevCenter terminal)"
                ))
            })?;
            positive(value).map_err(|_| invalid(format!("{variable} must be a positive integer; pass {option} explicitly to override it")))?;
            options.insert(option.into(), value.into());
        }
    }
    let mut take = |key: &str| {
        options
            .remove(key)
            .ok_or_else(|| invalid(format!("Required option: {key}")))
    };
    let command = match (resource, verb) {
        ("session", "config") => Command::SessionConfig { id },
        ("session", "configure") => {
            let name = options.remove("--name");
            let directory = options.remove("--directory").map(absolute).transpose()?;
            let project_id = options
                .remove("--project")
                .map(|v| positive(&v))
                .transpose()?;
            if name.is_none() && directory.is_none() && project_id.is_none() {
                return Err(invalid("Supply --name, --directory or --project"));
            }
            Command::SessionConfigure {
                id,
                name,
                directory,
                project_id,
            }
        }
        ("session", "pull-request") => Command::SessionPullRequest {
            id,
            target: take("--target")?,
            number: options
                .remove("--number")
                .map(|v| positive(&v))
                .transpose()?,
        },
        ("session", "list") => Command::SessionList,
        ("session", "create") => {
            let project_id = positive(&take("--project")?)?;
            let name = take("--name")?;
            let directory = options
                .remove("--directory")
                .map(absolute)
                .transpose()?
                .unwrap_or_default();
            Command::SessionCreate {
                project_id,
                name,
                directory,
            }
        }
        ("session", "rename") => Command::SessionRename {
            id,
            name: take("--name")?,
        },
        ("session", "directory") => Command::SessionDirectory {
            id,
            directory: absolute(take("--directory")?)?,
        },
        ("session", "move") => Command::SessionMove {
            id,
            project_id: positive(&take("--project")?)?,
        },
        ("session", "reorder") => Command::SessionReorder {
            id,
            position: take("--position")?
                .parse()
                .map_err(|_| invalid("Position must be a nonnegative integer"))?,
        },
        ("session", "select") => Command::SessionSelect { id },
        ("archive", "list") => Command::ArchiveList,
        ("archive", "restore") => Command::ArchiveRestore { id },
        ("archive", "purge") => Command::ArchivePurge { id, confirmed },
        ("archive", "prune") => Command::ArchivePrune { confirmed },
        ("session", "close" | "archive") => Command::SessionClose { id, confirmed },
        ("tab", "types") => Command::TabTypes,
        ("tab", "actions") => Command::TabActions {
            session_id: positive(&take("--session")?)?,
        },
        ("tab", "list") => Command::TabList {
            session_id: positive(&take("--session")?)?,
        },
        ("tab", "create") => Command::TabCreate {
            session_id: positive(&take("--session")?)?,
            action_id: take("--action")?,
            name: options.remove("--name"),
            config: options
                .remove("--config")
                .map(|text| {
                    serde_json::from_str(&text)
                        .map_err(|e| invalid(format!("Invalid JSON configuration: {e}")))
                })
                .transpose()?,
        },
        ("tab", "select") => Command::TabSelect { id },
        ("tab", "rename") => Command::TabRename {
            id,
            name: take("--name")?,
        },
        ("tab", "reorder") => Command::TabReorder {
            id,
            position: take("--position")?
                .parse()
                .map_err(|_| invalid("Position must be a nonnegative integer"))?,
        },
        ("tab", "open") => Command::TabOpen { id },
        ("tab", "close") => Command::TabClose { id, confirmed },
        ("tab", "delete" | "archive") => Command::TabDelete { id, confirmed },
        ("tab", "configure") => Command::TabConfigure {
            id,
            config: serde_json::from_str(&take("--config")?)
                .map_err(|e| invalid(format!("Invalid JSON configuration: {e}")))?,
        },
        ("project", "config") => Command::ProjectConfig { id },
        ("project", "configure") => {
            use std::io::Read;
            let path = take("--file")?;
            let mut yaml = String::new();
            std::fs::File::open(&path)
                .map_err(|e| invalid(format!("Cannot read {path}: {e}")))?
                .take(64 * 1024 + 1)
                .read_to_string(&mut yaml)
                .map_err(|e| invalid(format!("Cannot read {path}: {e}")))?;
            if yaml.len() > 64 * 1024 {
                return Err(invalid("Configuration exceeds 64 KiB"));
            }
            Command::ProjectConfigure { id, yaml }
        }
        ("project", "list") => Command::ProjectList,
        ("project", "create") => Command::ProjectCreate {
            name: take("--name")?,
            root: absolute(take("--root")?)?,
        },
        ("project", "rename") => Command::ProjectRename {
            id,
            name: take("--name")?,
        },
        ("project", "directory") => Command::ProjectDirectory {
            id,
            directory: absolute(take("--directory")?)?,
        },
        ("project", "fold") => Command::ProjectFold {
            id,
            collapsed: true,
        },
        ("project", "unfold") => Command::ProjectFold {
            id,
            collapsed: false,
        },
        ("project", "close") => Command::ProjectClose { id },
        ("app", "show") => Command::Show,
        ("app", "status") => Command::Status,
        ("app", "quit") => Command::Quit { confirmed },
        ("app", "start") => {
            let executable = options.remove("--executable").map(PathBuf::from);
            if !options.is_empty() {
                return Err(invalid("Unknown start option"));
            }
            return Ok(Invocation::Start(executable));
        }
        _ => return Err(invalid("Unknown command; use --help")),
    };
    if let Some(key) = options.keys().next() {
        return Err(invalid(format!("Unknown option: {key}")));
    }
    Ok(Invocation::Request(command))
}

fn execute(invocation: Invocation) -> Result<Response, RpcError> {
    let data = paths::data_directory(paths::APPLICATION_ID)
        .map_err(|e| RpcError::new("unavailable", e))?;
    let socket = paths::socket_path(&data).map_err(|e| RpcError::new("permission_denied", e))?;
    match invocation {
        Invocation::Help(_) => unreachable!(),
        Invocation::Request(command) => client::call(&socket, command),
        Invocation::Start(executable) => {
            match client::call(&socket, Command::Status) {
                Ok(response) => return Ok(response),
                Err(error) if error.code == "unavailable" => {}
                Err(error) => return Err(error),
            }
            let executable = if let Some(path) = executable {
                path
            } else {
                std::env::current_exe()
                    .map_err(|e| RpcError::new("operation_failed", e.to_string()))?
                    .parent()
                    .unwrap()
                    .join("devcenter-gui")
            };
            let mut command = ProcessCommand::new(executable);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // Detach only the explicitly requested GUI application, not a daemon.
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().map_err(|e| {
                RpcError::new("operation_failed", format!("Cannot start application: {e}"))
            })?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match client::call(&socket, Command::Status) {
                    Ok(response) => return Ok(response),
                    Err(error) if error.code == "unavailable" => {}
                    Err(error) => return Err(error),
                }
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| RpcError::new("operation_failed", e.to_string()))?
                {
                    return Err(RpcError::new("operation_failed",format!("Application exited before becoming available: {status}. Run the GUI executable directly for startup diagnostics.")));
                }
                if Instant::now() >= deadline {
                    return Err(RpcError::new("timeout","Application startup did not become available within 15 seconds; it may still start. Check status before retrying."));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");
    let invocation = match parse(args) {
        Ok(value) => value,
        Err(error) => return print_error(error, json),
    };
    if let Invocation::Help(text) = invocation {
        print!("{text}");
        return 0;
    }
    match execute(invocation) {
        Ok(response) if response.ok => {
            println!("{}", output::success(&response, json));
            0
        }
        Ok(response) => print_error(response.error.unwrap(), json),
        Err(error) => print_error(error, json),
    }
}
fn print_error(error: RpcError, json: bool) -> i32 {
    let code = error.exit_code();
    eprintln!("{}", output::error(error, json));
    code
}

#[cfg(test)]
mod tests;
