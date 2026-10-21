# Terminal workspace preview

A native Linux terminal workspace preview with a compact sidebar of **workgroups containing workspaces**, and **multiple terminal tabs per workspace**. Create, edit, reorder, select, and close workspaces using a name and an existing directory, with a native directory picker. Create, rename, reorder, select, and close terminal tabs above the terminal. Switching tabs or workspaces retains processes and emulator state. SQLite persists metadata and selection. Window close hides the app and its live terminals; launching the same binary again reopens the existing instance. The full Unix-socket CLI and Ubuntu packages remain pending.

Workspace/tab metadata, ordering and selection are saved transactionally. Changing a workspace's base directory affects future launches for tabs without a directory override; it never changes a running process's directory. Workspace operations never create, move, or delete the directory. Missing saved directories remain in metadata and produce a clear launch error until corrected.

Each workspace has a vertical **⋮** menu for editing and deletion. Drag its grip to reorder; the drop indicator shows the insertion point. Keyboard users can use Alt+Up/Down while the grip is focused. Deletion removes saved workspace/tab metadata and terminates its sessions after confirmation; the directory stays untouched.

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

Click **+ New tab** and choose **Bash** or **Codex** to launch a new terminal in the workspace directory. Each opening keeps its own launch configuration and runtime state. Launch settings are edited under Edit workgroup → Actions, or in its YAML tab. To replace an exited or legacy stopped tab, close it and open a new tab.

## Workgroups and actions (YAML)

A workgroup has a name and an existing root directory. Use the sidebar's top **+** to create one, its **⋮** menu to edit it, and its own **+** to create a workspace inside it. New workspace paths default to that group's root; you can override the directory. Creating a workspace immediately opens and starts the first action listed in its workgroup YAML, using the action name for the tab. Put your preferred default first (Bash in the shipped configuration). Invalid YAML prevents creation instead of leaving an empty placeholder terminal. The workspace editor also lets you move an existing workspace to another workgroup. Group root changes affect the default for future workspaces, while existing workspace paths and running processes stay unchanged.

Click the group's chevron/name to fold or unfold its workspace list. Folding is persisted and only hides sidebar entries: it does not close workspaces, change the active terminal, or interrupt output. Reorder workspaces inside their group with their existing drag grips/keyboard shortcuts. A group can be deleted only after its workspaces have been moved or closed. Workspace/root directories and group configuration files are never deleted by these operations.

Each workgroup has one YAML file at `$XDG_CONFIG_HOME/io.local.terminal-stack-prototype/workgroups/<id>/actions.yaml`, normally under `~/.config/`. Open **Edit workgroup** to see its exact path. The file is initialized on first Edit/New tab access with Bash/Codex defaults. Each file is independent; global configuration files are not used. For the migrated Default group, for example: `vim ~/.config/io.local.terminal-stack-prototype/workgroups/1/actions.yaml`.

The Edit workgroup modal has **General**, **Actions**, and **YAML** tabs. General edits the name/root. Actions provides a form for adding, removing, and reordering actions, with executable/arguments or explicit shell-command mode, optional directory and explicit environment overrides. The **Common environment variables** section provides shared name/value pairs for all actions in the workgroup. Arguments use a JSON array of strings; environment overrides use a JSON object of strings. YAML edits the same list directly. Switching views transfers unsaved action edits between them; **Save actions** validates and atomically replaces that group's file. General settings use **Save workgroup** separately. Existing openings keep their copied launch settings. An invalid YAML file opens in the YAML editor so it can be repaired.

Actions are the tab choices under **New tab**, which uses only the active workspace's workgroup configuration. The action list uses `actions`, alongside optional common `env` values (older action-list names are rejected):

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
      executable: codex
      args: []
      env: {}
```

Environment precedence is the inherited process environment, then the workgroup’s common `env`, then each action’s `config.env`. Values are literal strings. Common variables are reloaded for each new process; editing them never changes an already running process. They live in the same workgroup YAML file and are not merged into saved tab metadata.

IDs must be unique; names are shown in the picker. Only `kind: terminal` is supported. `config` also accepts `cwdOverride` and explicit `shellCommand`; shell-command mode cannot include `executable` or arguments. Executable mode passes arguments literally. The normal environment is inherited with only `env` overrides applied. Blank/omitted `cwdOverride` uses the workspace base directory. The file supports up to 100 types and 64 KiB. Commands must already be installed. Changes affect new tabs only; existing tabs keep their saved configuration. No workspace setup scripts or shell/Codex configuration changes are performed.

Use the arrow keys, Home, and End while focused on the tab bar to select tabs. Drag a tab to reorder it, or use Alt+Left/Right while it is focused. Each tab has a cross to close; renaming controls are temporarily hidden. Closing a tab with a running process requires confirmation and retains its saved metadata internally. The configured-tab reopening selector has been removed; New tab creates a fresh opening from YAML. Copy the terminal selection with Ctrl+Shift+C and paste with Ctrl+Shift+V. Closing a workspace removes its definitions and confirms termination of all its sessions. Hidden tabs continue consuming output with painting suppressed.

## WebView reload

Reloading the interface reconnects to existing PTYs instead of launching duplicate processes. This also reconnects exited sessions so their retained output can be inspected. Rust retains a bounded in-memory byte/resize journal and replays it into the new Ghostty frontend with the same ACK backpressure used for live output. Terminal-generated responses and user input are suppressed during replay so old terminal queries cannot send duplicate replies into the process.

The journal has a 4 MiB accounting budget per session; replay temporarily holds another bounded copy. If the budget is exceeded, the journal is discarded and reload explicitly warns that the previous screen is unavailable. Only subsequent output is shown, and terminal modes may need the running program to redraw/reinitialize. Selection and scroll position are not restored. This journal is never saved to SQLite or disk and disappears when its session is closed or the application exits. It does not restore processes after a crash or an application restart.

## Persistence and command restart

At the user's request, only tabs that were running when the app last closed automatically launch again. These are **new processes**, not recovered live sessions; previous screens and output history are not restored. Tabs explicitly stopped/closed and processes observed to have exited do not auto-start. Stopped open tabs restore as stopped. A crash uses the last committed running set. Startup waits for connected terminal frontends; failed launches report errors without an automatic retry loop.

Schema version 2 migrates existing workspaces into a **Default** workgroup without changing workspace/tab IDs, paths, settings, selection or resume flags. Its initial root is the first saved workspace directory (your home directory on a fresh installation). Workgroup IDs, names, roots, ordering, fold state and workspace membership are saved with the rest of the metadata. A failed migration rolls back rather than resetting data.

The database is `workspaces.sqlite3` in Tauri's application data directory, normally `$XDG_DATA_HOME/io.local.terminal-stack-prototype/` or `~/.local/share/io.local.terminal-stack-prototype/`. Versioned migrations are in `src-tauri/migrations/`. Definitions (including their kind/configuration) and open tabs are stored separately. Only explicit environment overrides are saved, never the inherited environment or terminal output. New tabs persist a copy of their YAML launch configuration.

Database failures are reported rather than resetting data. An exclusive owner-only lock prevents a second app from overwriting metadata or relaunching commands twice; a second launch focuses the first window through a private activation socket. Metadata is saved as changes occur, so this version does not require a manual Save action for workspace/tab operations. Earlier in-memory versions did not write a database; they have no saved data to migrate automatically.

Ordinary React renders do not recreate the terminal. Closing the window hides it; terminals keep consuming output. Launch the same binary again to reopen it, or use `src-tauri/target/debug/terminal-workspace-prototype --show`. `--show` fails if the application is unavailable and never starts it. The footer **Quit** action warns before terminating running processes. Reopening a hidden window preserves live processes and screens; restarting after Quit launches eligible commands as fresh processes under the restart policy above.

Each terminal joins a private cgroup before the requested executable runs. Confirmed close and Quit use immediate SIGKILL for the whole cgroup, including ordinary detached/setsid descendants, and wait for cleanup. There is no graceful shutdown interval: save application work before confirming. A per-session watchdog also cleans up after application death or command-leader exit. It ends with its session; there is no persistent daemon. This is lifecycle containment, not a security sandbox against same-user programs deliberately moving into another cgroup or delegating work to external services.

The activation socket is mode 0600 under `/run/user/<uid>` (required owner-only), validates peer UID, and accepts bounded show requests only. Full workspace/terminal CLI commands and JSON CLI output are not implemented yet. Ubuntu packaging will provide the desktop launcher; launching the same executable already performs activation.

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

The Rust checks use actual Linux PTYs and a Tauri channel callback to exercise byte transport, explicit environment overrides, exit status, ACK backpressure, resizing, input, launch failures, and termination of a child in a separate job-control group. Workspace/tab checks cover identities, ordering/selection, directory non-mutation, independent sibling PTYs, literal arguments versus explicit shell syntax, directory inheritance/overrides, and confirmation before closure. A login-shell integration test may require running outside a restrictive sandbox if the user's shell startup files need desktop access. The rendering check uses real independent Ghostty engines and an instrumented drawing boundary to verify hidden output updates without drawing and redraw on return; native visual behavior still needs manual checks.

Use [the native validation checklist](docs/VALIDATION.md) before proceeding to the full application. Findings and design boundaries are in [the stack evaluation](docs/STACK.md).

## Development server unavailable

The development WebView loads terminal WASM lazily when a terminal frontend connects. If Vite has stopped, an already-open window can still display its interface but cannot fetch the terminal engine. Start the application with `mise exec -- npm run tauri -- dev`, which starts both Vite and the Rust app. After a temporary asset-load failure, a new tab retries instead of retaining the failed engine request. If restarting the whole development app, close the previous window using its confirmation flow first. Installed builds load their bundled assets and do not require Vite.

## Native stability checks

The repeatable [native validation procedure](docs/VALIDATION.md#reproduce-the-native-checks) runs an isolated bundled Tauri application through WebKitGTK WebDriver, using temporary metadata instead of your saved workspaces. It covers reload/reconnection, hidden output, native clipboard shortcuts, actual PTY resizing, repeated tab creation and unsent Codex prompt editing. A special frontend build exposes bounded diagnostics only for that test mode; normal builds contain no validation bridge.

Current blocker: ghostty-web 0.4.0's browser layer does not forward mouse reports to terminal applications. WASM mouse-mode support alone is insufficient. Native runs record this explicitly and return exit code 2; the full terminal validation gate is not complete. Closed-tab cell reuse was reproduced and the app now resets new emulators after their initial fit before attaching output. No terminal engine was replaced.
