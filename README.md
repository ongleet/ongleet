# DevCenter

DevCenter is a native Linux terminal session application with a compact sidebar of **projects containing sessions**, and **multiple terminal tabs per session**. Create, edit, reorder, select, and close sessions using a name and an existing directory, with a native directory picker. Create, rename, reorder, select, and close terminal tabs above the terminal. Switching tabs or sessions retains processes and emulator state. SQLite persists metadata and selection. Window close hides the app and its live terminals; launching the same binary again reopens the existing instance. A Unix-socket CLI controls sessions, projects and the application. Terminal-specific CLI commands and Ubuntu packages remain pending.

Session/tab metadata, ordering and selection are saved transactionally. Changing a session's base directory affects future launches for tabs without a directory override; it never changes a running process's directory. Session operations never create, move, or delete the directory. Missing saved directories remain in metadata and produce a clear launch error until corrected.

Each session has a vertical **⋮** menu for editing and deletion. Drag its grip to reorder; the drop indicator shows the insertion point. Keyboard users can use Alt+Up/Down while the grip is focused. Deletion removes saved session/tab metadata and terminates its terminal processes after confirmation; the directory stays untouched.

The stack is Tauri 2, React + TypeScript, Rust, portable-pty, and pinned `ghostty-web` 0.4.0. There is no Electron, xterm.js runtime, tmux, Git integration, or agent orchestration.

## Run

Use mise for Node and Rust:

```bash
mise install
mise exec -- npm install
mise exec -- npm run check:host
mise exec -- npm run tauri -- dev
```

Ubuntu native prerequisites (system libraries are installed with apt, not mise):

```bash
sudo apt update
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
```

Terminal launch currently also requires Linux cgroup v2, a writable systemd user delegation at `/sys/fs/cgroup/user.slice/user-<uid>.slice/user@<uid>.service`, `cgroup.kill`, and pidfds. Unsupported systems report a clear launch error; there is currently no weaker fallback. This has been tested on the Ubuntu host recorded in `docs/VALIDATION.md`, not all Linux distributions. macOS support and an optional basic cleanup mode are future work.

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/). A working graphical desktop session is required. The prototype does not change shell or Codex configuration. Codex must already be installed, available on PATH, and configured by the user.

Click **+ New tab** and choose **Bash** or **Codex** to launch a new terminal in the session directory. Each opening keeps its own launch configuration and runtime state. Launch settings are edited under Edit project → Actions, or in its YAML tab. To replace an exited or legacy stopped tab, close it and open a new tab.

## Projects and actions (YAML)

A project has a name and an existing root directory. Use the sidebar's top **+** to create one, its **⋮** menu to edit it, and its own **+** to create a session inside it. New session paths default to that group's root; you can override the directory. Creating a session immediately opens and starts the first terminal action listed in its project YAML, using the action name for the tab. Put your preferred default first (Bash in the shipped configuration). Invalid YAML prevents creation instead of leaving an empty placeholder terminal. The session editor also lets you move an existing session to another project. Group root changes affect the default for future sessions, while existing session paths and running processes stay unchanged.

Click the group's chevron/name to fold or unfold its session list. Folding is persisted and only hides sidebar entries: it does not close sessions, change the active terminal, or interrupt output. Reorder sessions inside their group with their existing drag grips/keyboard shortcuts. A group can be deleted only after its sessions have been moved or closed. Session/root directories and group configuration files are never deleted by these operations.

Each project has one YAML file at `$XDG_CONFIG_HOME/devcenter/projects/<id>/actions.yaml`, normally under `~/.config/`. Open **Edit project** to see its exact path. The file is initialized on first Edit/New tab access with Bash/Codex defaults. Each file is independent; global configuration files are not used. For the migrated Default group, for example: `vim ~/.config/devcenter/projects/1/actions.yaml`.

The Edit project modal has **General**, **Actions**, and **YAML** tabs. General edits the name/root. Actions provides a form for adding, removing, and reordering actions, with executable/arguments or explicit shell-command mode, optional directory and explicit environment overrides. The **Common environment variables** section provides shared name/value pairs for all actions in the project. Arguments use a JSON array of strings; environment overrides use a JSON object of strings. YAML edits the same list directly. Switching views transfers unsaved action edits between them; **Save actions** validates and atomically replaces that group's file. General settings use **Save project** separately. Existing openings keep their copied launch settings. An invalid YAML file opens in the YAML editor so it can be repaired.

Actions are the tab choices under **New tab**, which uses only the active session's project configuration. The action list uses `actions`, alongside optional common `env` values (older action-list names are rejected):

```yaml
env:
  PROJECT_MODE: development
actions:
  - id: bash
    name: Bash
    kind: terminal
    config:
      executable: bash
      args: []
      env: {}
  - id: codex
    name: Codex
    kind: terminal
    config:
      executable: dcodex
      args: []
      env: {}
```

Environment precedence is the inherited process environment, then the project’s common `env`, then each action’s `config.env`. Values are literal strings. Common variables are reloaded for each new process; editing them never changes an already running process. They live in the same project YAML file and are not merged into saved tab metadata.

IDs must be unique; names are shown in the picker. Only `kind: terminal` is supported. `config` also accepts `cwdOverride` and explicit `shellCommand`; shell-command mode cannot include `executable` or arguments. Executable mode passes arguments literally. The normal environment is inherited with only `env` overrides applied. Blank/omitted `cwdOverride` uses the session base directory. The file supports up to 100 types and 64 KiB. Commands must already be installed. Changes affect new tabs only; existing tabs keep their saved configuration. No session setup scripts or shell/Codex configuration changes are performed.

Use the arrow keys, Home, and End while focused on the tab bar to select tabs. Drag a tab to reorder it, or use Alt+Left/Right while it is focused. Each tab has a cross to close; renaming controls are temporarily hidden. Closing a tab with a running process requires confirmation and retains its saved metadata internally. The configured-tab reopening selector has been removed; New tab creates a fresh opening from YAML. Copy the terminal selection with Ctrl+Shift+C and paste with Ctrl+Shift+V. Closing a session removes its definitions and confirms termination of all its sessions. Hidden tabs continue consuming output with painting suppressed.

## Terminal mouse, clipboard and links

Drag to select terminal text; releasing the mouse copies the selection to the native clipboard. Ctrl+Shift+C copies it explicitly. When a program requests mouse reporting, clicks, drags and wheel events go to that program; hold Shift while dragging to select text locally instead.

Ctrl+Shift+V pastes native clipboard text with bracketed-paste framing, or quoted local paths when files were copied from a file manager (up to 32 files and the terminal input limit). It does not copy file contents into the terminal. Image clipboard contents forward Ctrl+V to the running program. In Codex, Ctrl+V also reaches its own image-paste handler directly; image decoding and attachment remain Codex's responsibility. In shells, Ctrl+V retains its conventional literal-next-character behavior. File drag-and-drop is not implemented.

Ctrl+click opens terminal HTTP(S) links in the default browser and local `file://` links in their associated application. This includes labeled OSC 8 links emitted by Codex. File-line navigation depends on the associated application; missing files report an error. Only these URL schemes are accepted. The frontend retains bounded OSC 8 URL metadata because ghostty-web 0.4.0 exposes hyperlink cell IDs but its URL lookup is a stub. Older link metadata can become unavailable after its 4,096-entry / 4 Mi-code-unit budget is exceeded; there is no second terminal emulator.

## WebView reload

Reloading the interface reconnects to existing PTYs instead of launching duplicate processes. This also reconnects exited sessions so their retained output can be inspected. Rust retains a bounded in-memory byte/resize journal and replays it into the new Ghostty frontend with the same ACK backpressure used for live output. Terminal-generated responses and user input are suppressed during replay so old terminal queries cannot send duplicate replies into the process.

The journal has a 4 MiB accounting budget per session; replay temporarily holds another bounded copy. If the budget is exceeded, the journal is discarded and reload explicitly warns that the previous screen is unavailable. Only subsequent output is shown, and terminal modes may need the running program to redraw/reinitialize. Selection and scroll position are not restored. This journal is never saved to SQLite or disk and disappears when its session is closed or the application exits. It does not restore processes after a crash or an application restart.

## Persistence and command restart

At the user's request, only tabs that were running when the app last closed automatically launch again. These are **new processes**, not recovered live sessions; previous screens and output history are not restored. Tabs explicitly stopped/closed and processes observed to have exited do not auto-start. Stopped open tabs restore as stopped. A crash uses the last committed running set. Startup waits for connected terminal frontends; failed launches report errors without an automatic retry loop.

### Codex conversation restoration

The shipped Codex action uses `executable: dcodex`. This opt-in wrapper starts Codex normally on first launch and uses `codex resume <conversation-id>` on subsequent launches of the same stable session/tab definition. New tabs have independent conversations. Existing project YAML and saved tab configurations are not rewritten: change the Codex action's executable from `codex` to `dcodex` in Edit project → Actions, then open a new Codex tab. Existing plain `codex` tabs retain their original behavior.

The app supplies session/tab IDs and its actual data directory, then runs the wrapper through `/proc/<app-pid>/exe`. It does not require a separately built helper. Command-line `SessionStart` and `UserPromptSubmit` hooks record Codex's reported ID, including changes after switching/forking sessions, into `codex-sessions/<session-id>/<tab-id>.json` under application data. Writes are atomic and mode 0600, with a per-launch generation rejecting older hooks. Files contain identifiers only, not transcripts, credentials, or inherited environment. They remain after tab/session closure; IDs are not reused. Codex stores its own conversation history separately under its configured `CODEX_HOME`.

Use a Codex version supporting `SessionStart` command hooks, inline hook configuration, `--dangerously-bypass-hook-trust`, and `--no-daemon` (0.163.0 was exercised locally). The wrapper enables hooks for its invocation and adds recording hooks alongside existing hook sources. Like Agent Orchestrator, it passes **`--dangerously-bypass-hook-trust` for that invocation** because session-flag hooks lack a durable source trust hash. This permits **all enabled hooks**, including other user/project/plugin hooks, to run without the usual hook review. Choose this action only when those hook sources are acceptable; use plain `codex` to retain the normal trust flow. It does not edit global/project Codex configuration. The wrapper uses `--no-daemon` to keep the process within terminal containment. Live work stops on app Quit/crash; the next launch resumes the saved conversation in a new process.

Arguments are optional: use `args: ["--", "--model", "MODEL"]` to forward Codex options. To deliberately replace a conversation, run the standalone helper with the same session/tab IDs and `--fresh` while that tab is stopped. A configured `args: ["--fresh"]` deliberately starts a fresh conversation on every launch. A missing/unavailable Codex conversation reports Codex's error instead of silently starting a new one. Malformed mapping files also fail without resetting the file; move the damaged file aside explicitly to recover.

For use outside a configured tab, build the standalone helper with `mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --offline --bin dcodex`, then run `src-tauri/target/debug/dcodex --session-id 12 --tab-id 34`. It uses the standard application data directory and current working directory. Use the same XDG data and Codex home settings to find the same mapping/history. This standalone command does not start the GUI or automatically restore app process containment.

Schema version 2 originally migrated workspaces into a **Default** workgroup without changing session/tab IDs, paths, settings, selection or resume flags. Its initial root is the first saved session directory (your home directory on a fresh installation). Project IDs, names, roots, ordering, fold state and session membership are saved with the rest of the metadata. A failed migration rolls back rather than resetting data.

Schema version 3 renames workgroups to **projects** and workspaces to **sessions**, preserving IDs, directories, ordering, selection, tab definitions/openings and restart flags. Startup migrates the schema automatically while holding the writer lock and moves the configuration tree from `workgroups/` to `projects/`. Conflicting configuration trees are reported without merging or overwriting. Existing YAML is preserved verbatim: legacy `{{workspace.*}}` / `{{workgroup.*}}` placeholders and `WORKSPACE_*` / `WORKGROUP_*` context variables remain supported alongside the new names. `dcodex --workspace-id` remains an alias for `--session-id`. The database filename and `codex-sessions/` mapping paths stay unchanged to preserve existing data and conversation associations. The renamed control protocol uses version 2; rebuild the GUI and CLI together.

To migrate without launching terminals, quit DevCenter and run `devcenter-gui --migrate`. Migration refuses an active metadata writer. Database migrations are transactional and failures never reset metadata.

The database is `workspaces.sqlite3` in DevCenter's application data directory, normally `$XDG_DATA_HOME/devcenter/` or `~/.local/share/devcenter/`. Versioned migrations are in `src-tauri/migrations/`. Definitions (including their kind/configuration) and open tabs are stored separately. Only explicit environment overrides are saved, never the inherited environment or terminal output. New tabs persist a copy of their YAML launch configuration.

DevCenter stores configuration in `$XDG_CONFIG_HOME/devcenter/` (normally `~/.config/devcenter/`) and data in `$XDG_DATA_HOME/devcenter/` (normally `~/.local/share/devcenter/`). On first startup it moves the previous `io.local.terminal-stack-prototype` directories to these paths, preserving project YAML, database sidecars, IDs and Codex conversation mappings. Quit the previous application first. Migration refuses a live legacy writer or an existing destination rather than merging or overwriting files. You can run `devcenter-gui --migrate` to migrate without opening a window. It is safe to repeat after a successful move.

The short commands are `devcenter` for session/application control and `dcodex` for Codex conversation restoration. `terminal-workspace` and `devcenter-codex` remain compatibility aliases, and saved tab definitions using the old wrapper name still work. These commands are available in application terminals after rebuilding/restarting; installation on your personal PATH remains packaging work.

Database failures are reported rather than resetting data. An exclusive owner-only lock prevents a second app from overwriting metadata or relaunching commands twice; a second launch focuses the first window through a private activation socket. Metadata is saved as changes occur, so this version does not require a manual Save action for session/tab operations. Earlier in-memory versions did not write a database; they have no saved data to migrate automatically.

Ordinary React renders do not recreate the terminal. Closing the window hides it; terminals keep consuming output. Launch the same binary again to reopen it, or use `src-tauri/target/debug/devcenter-gui --show`. `--show` fails if the application is unavailable and never starts it. The footer **Quit** action warns before terminating running processes. Reopening a hidden window preserves live processes and screens; restarting after Quit launches eligible commands as fresh processes under the restart policy above.

Each terminal joins a private cgroup before the requested executable runs. Confirmed close and Quit use immediate SIGKILL for the whole cgroup, including ordinary detached/setsid descendants, and wait for cleanup. There is no graceful shutdown interval: save application work before confirming. A per-session watchdog also cleans up after application death or command-leader exit. It ends with its session; there is no persistent daemon. This is lifecycle containment, not a security sandbox against same-user programs deliberately moving into another cgroup or delegating work to external services.

The control socket is mode 0600 under `/run/user/<uid>` (required owner-only), validates peer UID on both ends, and accepts versioned bounded JSON requests. Ubuntu packaging will provide the desktop launcher; launching the same executable already performs activation.

## External actions

In **Edit project → Actions**, choose **External command** as the action type. Use the **Actions** button beside **New tab** to launch it. New tab lists terminal actions only, and new sessions automatically start the first terminal action even when external actions precede it. A project needs a terminal action to create a session.

External commands launch without a PTY or application tab. They inherit the desktop application's environment, apply common project variables and then action-specific overrides, and default to the session directory. Launch errors and nonzero exits are reported while the frontend is connected; stdout/stderr are discarded. Launch acceptance does not guarantee a visible window, particularly for applications that delegate to an already-running instance. Install/configure the external application yourself; actions never modify shell configuration.

Example entries in a project's `actions` list:

```yaml
  - id: vscode
    name: Open in VS Code
    kind: command
    config:
      executable: code
      args: ["--new-window", "{{session.path}}"]
  - id: desktop-terminal
    name: Open external terminal
    kind: command
    config:
      executable: gnome-terminal
      args: ["--working-directory={{session.path}}"]
```

Choose an installed executable and its supported arguments. These examples are opt-in; Bash and Codex remain the shipped defaults. Direct executable mode passes argument arrays without shell evaluation. External action executable, arguments, directory override and environment values support `{{session.path}}`, `{{session.name}}`, `{{session.id}}`, `{{project.path}}`, `{{project.name}}`, and `{{project.id}}`. Values come from current session metadata at launch, not a shell's current directory. Substitution is a single literal pass, preserving spaces, Unicode, braces and shell metacharacters inside argument values. Unknown or unclosed placeholders fail the launch. Tilde and arbitrary environment-variable expansion are not supported. Placeholder expansion is currently for external actions only.

Shell-command mode instead exposes `SESSION_PATH`, `SESSION_NAME`, `PROJECT_PATH`, and `PROJECT_NAME` as reserved environment variables; for example `code --new-window "$SESSION_PATH"`. Placeholder syntax is rejected inside `shellCommand` to avoid interpolating a path into executable shell syntax. Arguments containing shell programs are still your responsibility; prefer direct executable mode. The reserved context variables take precedence over configured values of the same names. Common environment values are also interpolated when used by an external action; terminal actions retain their existing literal environment behavior.

External applications are deliberately independent of managed terminal sessions. The app does not terminate them on session close or Quit, and never restores them. Their ultimate lifetime also depends on the desktop/session manager or the launched application. The app reaps directly launched children while running and permits at most 64 outstanding direct children; launcher applications may delegate to desktop services. External processes are excluded from terminal running counts and close/Quit confirmations. These actions are explicit launches and are never run automatically on session creation or restoration.

## CLI

Build the frontend with `mise exec -- npm run build`, then the GUI and command-line executables with `mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline`. Restart the GUI after updating it to load the control protocol. App-launched terminals automatically prepend the application executable directory to PATH, so `devcenter` is available there when built alongside the GUI. This does not change your personal PATH or shell configuration. Explicit project/action PATH overrides take precedence, and shell startup files may replace PATH. Existing terminals retain their environment; open a new tab after restarting the updated app.

The CLI is `src-tauri/target/debug/devcenter`. Use `--help` for the full command reference and automation contract, `session --help` for a resource, or `session create --help` for a specific command (`-h` also works). Help runs without connecting to the application. It documents command effects, JSON responses, ID discovery, exit codes, confirmation and timeout/retry behavior for scripts and AI agents. Outside app terminals, until packaging installs it on PATH, substitute that path for `devcenter` in these examples:

```bash
devcenter project list
devcenter session list
devcenter session create --project 1 --name Example --directory /existing/project
devcenter session rename 2 --name Backend
devcenter session directory 2 --directory /existing/other-project
devcenter session reorder 2 --position 0
devcenter session select 2
devcenter session close 2 --confirm
devcenter app show
devcenter app status
devcenter app quit --confirm
```

IDs are stable positive integers returned by list/create, not names or list indexes. Reorder positions are zero-based within the session's project. Omitting `--directory` on creation uses the group's root; relative paths resolve from the CLI's current directory. Creating a session selects it and opens its first configured terminal action through the connected frontend, including when the window is hidden. The response may report `starting` before that frontend launches the process; a disconnected/reloading frontend delays launch. Poll `session list` for observed process status. Directory changes affect future launches and never change an existing process's cwd or create/move/delete directories.

Project commands support `create --name NAME --root PATH`, `list`, `rename ID --name NAME`, `directory ID --directory PATH`, `fold ID`, `unfold ID`, and `close ID` (empty groups only). `session move ID --project ID` changes membership while preserving running processes. UI and CLI call the same Rust registry methods; successful CLI changes refresh the sidebar automatically.

Successes are JSON on stdout (`{"ok":true,"data":...}`); failures are JSON on stderr (`{"ok":false,"error":{"code":"...","message":"..."}}`). JSON is always enabled; `--json` is accepted explicitly. Help is plain text. Exit codes: 0 success, 2 invalid arguments/request, 3 application unavailable, 4 missing ID, 5 confirmation required, 6 operation failed, 7 permission denied, 8 protocol/size/timeout. Running-session closure and Quit require `--confirm`; without it no process is terminated. A timeout can occur after a mutation was committed: check state before retrying a creation.

Ordinary commands never start the application. Only `devcenter app start` starts the sibling GUI executable, or use `app start --executable /path/to/devcenter-gui`. Existing instances are reused. It needs the same desktop session, user and XDG data directory as the GUI. The CLI itself runs without a display and never opens SQLite. Session list includes open tab IDs/names/statuses; direct terminal manipulation, text input and output snapshots remain pending.


## Checks

```bash
mise exec -- npm run build
mise exec -- npm run check:wasm
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:rendering
mise exec -- npm run check:engine-loader
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Rust commands require a populated Cargo cache; omit `--offline` for the initial fetch. `check:wasm` uses the exact installed WASM and production byte adapter. It checks split UTF-8 and escape sequences, alternate-screen restoration, mouse/paste modes, terminal replies, oversized-chunk rejection, 100,000 output lines with bounded scrollback, and resizing. It does **not** certify browser rendering, keyboard input, clipboard behavior, or interactive Codex.

The Rust checks use actual Linux PTYs and a Tauri channel callback to exercise byte transport, explicit environment overrides, exit status, ACK backpressure, resizing, input, launch failures, and termination of a child in a separate job-control group. Session/tab checks cover identities, ordering/selection, directory non-mutation, independent sibling PTYs, literal arguments versus explicit shell syntax, directory inheritance/overrides, and confirmation before closure. A login-shell integration test may require running outside a restrictive sandbox if the user's shell startup files need desktop access. The rendering check uses real independent Ghostty engines and an instrumented drawing boundary to verify hidden output updates without drawing and redraw on return; native visual behavior still needs manual checks.

Use [the native validation checklist](docs/VALIDATION.md) before proceeding to the full application. Findings and design boundaries are in [the stack evaluation](docs/STACK.md).

## Development server unavailable

The development WebView loads terminal WASM lazily when a terminal frontend connects. If Vite has stopped, an already-open window can still display its interface but cannot fetch the terminal engine. Start the application with `mise exec -- npm run tauri -- dev`, which starts both Vite and the Rust app. After a temporary asset-load failure, a new tab retries instead of retaining the failed engine request. If restarting the whole development app, close the previous window using its confirmation flow first. Installed builds load their bundled assets and do not require Vite.

## Native stability checks

The repeatable [native validation procedure](docs/VALIDATION.md#reproduce-the-native-checks) runs an isolated bundled Tauri application through WebKitGTK WebDriver, using temporary metadata instead of your saved sessions. It covers reload/reconnection, hidden output, native clipboard shortcuts, actual PTY resizing, repeated tab creation and unsent Codex prompt editing. A special frontend build exposes bounded diagnostics only for that test mode; normal builds contain no validation bridge.

The browser mouse adapter now forwards reports to terminal applications; the isolated Wayland curses check passes. Native pointer selection/copy, image/file clipboard detection and OSC 8 link activation also pass on this host. Actual Codex image attachment and remaining manual/X11 checks are still open; the full terminal validation gate is not complete. Closed-tab cell reuse was reproduced and the app now resets new emulators after their initial fit before attaching output. No terminal engine was replaced.
