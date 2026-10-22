use super::{invalid, RpcError};

// Keep usage and effects together so every command has useful scoped help.
const COMMANDS: &[(&str, &str, &str)] = &[
    ("session", "list", "List sessions and open tabs, including IDs, selection and observed process status."),
    ("session", "create --project ID --name NAME [--directory PATH]", "Create and select a session; launch its project's first terminal action. Directory defaults to the group's root. Requires valid actions YAML and an existing directory. A starting status is not proof of a successful launch: poll session list."),
    ("session", "rename ID --name NAME", "Rename saved session metadata. Its directory and running processes stay unchanged."),
    ("session", "directory ID --directory PATH", "Set an existing base directory for future launches. Running processes keep their working directories."),
    ("session", "move ID --project ID", "Move a session to another project, preserving its directory and live terminals."),
    ("session", "reorder ID --position INDEX", "Move a session to a zero-based position within its project's session list."),
    ("session", "select ID", "Select a session in the interface, preserving live terminals. Use app show separately to reveal a hidden window."),
    ("session", "close ID [--confirm]", "Delete session and tab metadata. Running processes require --confirm and are immediately killed, including contained descendants. The session directory is never deleted. This is not a window-hide operation."),
    ("project", "list", "List projects with stable IDs, names, rootDirectory, displayOrder and collapsed state."),
    ("project", "create --name NAME --root PATH", "Create a project with an existing root directory. Does not create a session or directory."),
    ("project", "rename ID --name NAME", "Rename a project without changing its root or sessions."),
    ("project", "directory ID --directory PATH", "Set an existing root directory used by future sessions. Existing session directories stay unchanged."),
    ("project", "fold ID", "Collapse the sidebar group. Selection and processes stay unchanged."),
    ("project", "unfold ID", "Expand the sidebar group. Selection and processes stay unchanged."),
    ("project", "close ID", "Delete an empty project's metadata. Move or close its sessions first. Directories and group configuration files are not deleted."),
    ("app", "show", "Reveal and activate the running application. Fails if unavailable; does not start it."),
    ("app", "status", "Return pid, visible, projects, sessions and runningTerminals for the running application."),
    ("app", "quit [--confirm]", "Quit the application. Running sessions require --confirm and are immediately killed. Previously running tabs may launch as NEW processes on the next startup."),
    ("app", "start [--executable PATH]", "Explicitly start the GUI, or return status if already running. Defaults to devcenter-gui beside this CLI. Requires a graphical desktop session. Waits up to 15 seconds; after a timeout the app may still start. Run the GUI directly for startup diagnostics."),
];

const AUTOMATION: &str = r#"AUTOMATION CONTRACT
  Normal results: one JSON object on stdout, exit 0:
    {"ok":true,"data":...}
  Failures: one JSON object on stderr, nonzero exit:
    {"ok":false,"error":{"code":"not_found","message":"..."}}
  Parse error.code for decisions; error.message is diagnostic text.
  --json is optional: normal results are always JSON. Help is plain text,
  exits 0, and does not contact the app. No interactive prompts are used.

  Discover IDs with project list / session list; never infer IDs from
  names or list positions. Session list returns data.sessions and
  data.selectedId; project list returns data.projects. Session entries
  include tabs with id, name and status (state: starting/stopped/running/exited;
  exited status includes code). Create returns the created object in data.
  Use returned IDs for subsequent commands.

  Commands require the running app under the same user and XDG data directory.
  Only app start starts it. The CLI uses a private socket, never SQLite.
  Paths must exist; relative directory paths resolve from the CLI's cwd.
  Quote names and paths containing spaces. Shell expansion is the caller's job.
  --confirm authorizes termination; use it only when that action is intended.
  A timeout can happen AFTER a change commits. Inspect list/status before
  retrying, especially create: retries can create duplicates.
  Terminal-specific commands, sending input and reading screens are not
  implemented. Do not invent tab/terminal commands.

EXIT CODES
  0 success                  2 invalid arguments/request
  3 app unavailable          4 ID not found
  5 confirmation required    6 operation failed
  7 permission denied        8 protocol/size/timeout
"#;

pub(super) fn render(scope: &[String]) -> Result<String, RpcError> {
    if scope.len() > 2 {
        return Err(invalid(
            "Expected --help, RESOURCE --help or RESOURCE COMMAND --help",
        ));
    }
    let selected: Vec<_> = COMMANDS
        .iter()
        .filter(|(resource, usage, _)| {
            scope.first().is_none_or(|s| s == resource)
                && scope
                    .get(1)
                    .is_none_or(|s| usage.split_whitespace().next() == Some(s.as_str()))
        })
        .collect();
    if selected.is_empty() {
        return Err(invalid("Unknown help topic; use devcenter --help"));
    }
    let mut text = String::from("devcenter — control the native terminal session application\n\nUSAGE\n  devcenter [--json] <resource> <command> [options]\n  devcenter [<resource> [<command>]] --help\n  -h is an alias for --help. ID is a stable positive integer.\n\nCOMMANDS\n");
    for (resource, usage, description) in selected {
        text.push_str(&format!(
            "  devcenter {resource} {usage}\n    {description}\n\n"
        ));
    }
    if scope.is_empty() {
        text.push_str("EXAMPLE WORKFLOW\n  devcenter app status\n  devcenter project list\n  devcenter session create --project 1 --name \"API work\" --directory \"/existing/project\"\n  devcenter session list\n  devcenter session select 2\n  devcenter app show\n  Replace 1 and 2 with IDs returned by list/create; paths are examples.\n  For details: devcenter session create --help\n\n");
    }
    text.push_str(AUTOMATION);
    Ok(text)
}
