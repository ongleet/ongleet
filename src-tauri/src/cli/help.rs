use super::{invalid, RpcError};

// Keep usage and effects together so every command has useful scoped help.
const COMMANDS: &[(&str, &str, &str)] = &[
    ("tab", "types", "Describe built-in plugin types, configuration schemas, defaults, constraints and valid JSON examples."),
    ("tab", "actions [--session ID]", "List available tab actions for this session with action IDs, plugin kinds, versions and copied configuration. Excludes external commands and unavailable plugins."),
    ("tab", "list [--session ID]", "List all plugin tab definitions, copied configuration, openings, observed status and restart eligibility."),
    ("tab", "create [--session ID] --action ACTION_ID [--name NAME] [--config JSON]", "Open a fresh plugin tab from a configured project action. Discover action IDs with tab actions --session ID, and configuration fields with tab types. --config replaces the complete action configuration before launch. External command actions cannot open tabs."),
    ("tab", "select ID", "Select an open tab without recreating plugin resources."),
    ("tab", "rename ID --name NAME", "Rename a tab definition."),
    ("tab", "reorder ID --position INDEX", "Move an open tab to a zero-based position."),
    ("tab", "open ID", "Reopen a saved definition. Terminal tabs launch their saved command anew."),
    ("tab", "close ID [--confirm]", "Close a plugin tab, retaining its definition. Running terminals require --confirm."),
    ("tab", "delete ID [--confirm]", "Permanently remove a tab definition and close its opening. Running terminals require --confirm. Session directories and retained history files are preserved."),
    ("tab", "configure ID --config JSON", "Validate and replace plugin-specific configuration. Running terminals retain their original launch settings."),
    ("session", "list", "List sessions and open tabs, including IDs, selection and observed process status."),
    ("session", "create [--project ID] --name NAME [--directory PATH]", "Create and select a session; launch its project's first terminal action. Directory defaults to the group's root. Requires valid actions YAML and an existing directory. A starting status is not proof of a successful launch: poll session list."),
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
  Default output is readable text. Pass --json before the resource or after
  command arguments for structured output (including errors).
  With --json, normal results: one JSON object on stdout, exit 0:
    {"ok":true,"data":...}
  With --json, failures: one JSON object on stderr, nonzero exit:
    {"ok":false,"error":{"code":"not_found","message":"..."}}
  Parse error.code for decisions; error.message is diagnostic text.
  Text output is for humans; scripts should use --json. Help is plain text,
  exits 0, and does not contact the app. No interactive prompts are used.

  Inside DevCenter terminals, tab list/actions/create default --session to
  DEVCENTER_SESSION_ID; session create defaults --project to DEVCENTER_PROJECT_ID.
  Explicit options take precedence. Outside, supply the option or variable.
  Invalid context fails; there is no fallback to the selected GUI session.
  Context is fixed at terminal launch; existing terminals need relaunch.

  Discover tab action IDs with tab actions --session ID; tab types returns
  plugin configuration schemas, defaults and examples. tab create --config JSON
  applies a replacement configuration before launch; tab configure replaces
  saved configuration for future terminal launches. Neither is a JSON patch.

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
  Sending terminal input and reading screens are not implemented.
  Generic tab operations: devcenter tab --help.

EXIT CODES
  0 success                  2 invalid arguments/request
  3 app unavailable          4 ID not found
  5 confirmation required    6 operation failed
  7 permission denied        8 protocol/size/timeout
"#;

pub(super) fn render(scope: &[String]) -> Result<String, RpcError> {
    if scope.is_empty() {
        return Ok(String::from(
            "devcenter [--json] <resource> <command> [options]\n\nRESOURCES\n  tab         Manage plugin tabs\n  session     Manage sessions\n  project     Manage projects\n  app         Control the application\n\nHELP\n  devcenter tab --help                 List tab commands\n  devcenter tab create --help          Command usage and effects\n  devcenter automation --help          JSON, errors and exit codes\n  -h is an alias for --help.\n",
        ));
    }
    if scope == ["automation"] {
        return Ok(AUTOMATION.into());
    }
    if scope.len() > 2 {
        return Err(invalid(
            "Expected --help, RESOURCE --help or RESOURCE COMMAND --help",
        ));
    }
    let selected: Vec<_> = COMMANDS
        .iter()
        .filter(|(resource, usage, _)| {
            &scope[0] == resource
                && scope
                    .get(1)
                    .is_none_or(|s| usage.split_whitespace().next() == Some(s.as_str()))
        })
        .collect();
    if selected.is_empty() {
        return Err(invalid("Unknown help topic; use devcenter --help"));
    }
    if scope.len() == 1 {
        let mut text = format!("devcenter {} <command> [options]\n\nCOMMANDS\n", scope[0]);
        for (resource, usage, _) in selected {
            text.push_str(&format!("  devcenter {resource} {usage}\n"));
        }
        text.push_str(&format!(
            "\nDetails: devcenter {} <command> --help\nAutomation: devcenter automation --help\n",
            scope[0]
        ));
        return Ok(text);
    }
    let (resource, usage, description) = selected[0];
    let mut text = format!("devcenter {resource} {usage}\n\n{description}\n");
    match (*resource, scope[1].as_str()) {
        ("tab", "list" | "actions" | "create") => text.push_str(
            "\n--session defaults to DEVCENTER_SESSION_ID; an explicit option wins.\nOutside a DevCenter terminal, supply --session ID or set that variable.\n",
        ),
        ("session", "create") => text.push_str(
            "\n--project defaults to DEVCENTER_PROJECT_ID; an explicit option wins.\nDiscover project IDs: devcenter project list\n",
        ),
        ("tab", "configure") => text.push_str(
            "\n--config replaces the complete configuration, not a patch.\nFields and examples: devcenter tab types\nDiscover tab IDs: devcenter tab list\n",
        ),
        ("tab", "types") => text.push_str(
            "\nReturns data.types with configSchema, examples and notes per plugin,\nplus configuration limits and a suggested workflow.\nAvailable project actions: devcenter tab actions\n",
        ),
        _ => {}
    }
    if matches!(scope[1].as_str(), "create" | "rename" | "directory") {
        text.push_str(
            "\nQuote names/paths with spaces. Relative directory paths use the CLI cwd.\n",
        );
    }
    if matches!(scope[1].as_str(), "create" | "close" | "delete" | "quit") {
        text.push_str(
            "\nAfter a timeout, inspect list/status before retrying; changes may have committed.\n",
        );
    }
    text.push_str("\nResults are text; add --json for structured output. Errors use stderr and a nonzero exit code.\nAutomation details: devcenter automation --help\n");
    Ok(text)
}
