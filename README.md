# Terminal workspace preview

A native Linux terminal workspace preview with a compact workspace sidebar and **multiple terminal tabs per workspace**. Create, edit, reorder, select, and close workspaces using a name and an existing directory, with a native directory picker. Create, rename, reorder, select, and close terminal tabs above the terminal. Switching tabs or workspaces retains processes and emulator state. SQLite persists metadata and selection. Window hide/reopen, forwarding to an existing instance, the Unix-socket CLI, and Ubuntu packages are still pending.

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

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/). A working graphical desktop session is required. The prototype does not change shell or Codex configuration. Codex must already be installed, available on PATH, and configured by the user.

Create a workspace and use its initial stopped tab, or add a tab with **+ New tab**. Choose Start for the default user shell or Run Codex. Each tab has independent launch settings: optional working-directory override, executable, JSON argument array, and explicit environment overrides such as `{"PORT":"3000"}`. Save settings stores configuration without launching a process. Blank directory/executable fields use the workspace directory and user shell at launch time. For shell syntax, explicitly choose **Shell command**; executable mode never implicitly evaluates shell syntax.

Use the arrow keys, Home, and End while focused on the tab bar to select tabs. Drag a tab to reorder it, or use Alt+Left/Right while it is focused. Each tab has a cross to close; renaming controls are temporarily hidden. Closing a tab with a session requires confirmation and keeps its saved definition available in **Open configured tab…**. Reopening launches its saved command. There is currently one opening per definition. Restart explicitly relaunches an exited session after confirmation. Copy the terminal selection with Ctrl+Shift+C and paste with Ctrl+Shift+V. Closing a workspace removes its definitions and confirms termination of all its sessions. Hidden tabs continue consuming output with painting suppressed.

## Persistence and command restart

At the user's request, only tabs that were running when the app last closed automatically launch again. These are **new processes**, not recovered live sessions; previous screens and output history are not restored. Tabs explicitly stopped/closed and processes observed to have exited do not auto-start. Stopped open tabs restore as stopped. A crash uses the last committed running set. Startup waits for connected terminal frontends; failed launches report errors without an automatic retry loop.

The database is `workspaces.sqlite3` in Tauri's application data directory, normally `$XDG_DATA_HOME/io.local.terminal-stack-prototype/` or `~/.local/share/io.local.terminal-stack-prototype/`. Versioned migrations are in `src-tauri/migrations/`. Definitions (including their kind/configuration) and open tabs are stored separately. Only explicit environment overrides are saved, never the inherited environment or terminal output. Draft launch edits require **Save settings** or launching to become durable.

Database failures are reported rather than resetting data. An exclusive owner-only lock prevents a second app from overwriting metadata or relaunching commands twice; a second launch currently errors instead of focusing the first window. Metadata is saved as changes occur, so this version does not require a manual Save action for workspace/tab operations. Earlier in-memory versions did not write a database; they have no saved data to migrate automatically.

Ordinary React renders do not recreate the terminal. Closing a session asks for confirmation. **At this stage closing the window quits the prototype after confirmation; it does not yet hide it.** Do not rely on production process-lifecycle guarantees yet.

## Checks

```bash
mise exec -- npm run build
mise exec -- npm run check:wasm
mise exec -- npm run check:rendering
mise exec -- npm run check:engine-loader
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Rust commands require a populated Cargo cache; omit `--offline` for the initial fetch. `check:wasm` uses the exact installed WASM and production byte adapter. It checks split UTF-8 and escape sequences, alternate-screen restoration, mouse/paste modes, terminal replies, oversized-chunk rejection, 100,000 output lines with bounded scrollback, and resizing. It does **not** certify browser rendering, keyboard input, clipboard behavior, or interactive Codex.

The Rust checks use actual Linux PTYs and a Tauri channel callback to exercise byte transport, explicit environment overrides, exit status, ACK backpressure, resizing, input, launch failures, and termination of a child in a separate job-control group. Workspace/tab checks cover identities, ordering/selection, directory non-mutation, independent sibling PTYs, literal arguments versus explicit shell syntax, directory inheritance/overrides, and confirmation before closure. A login-shell integration test may require running outside a restrictive sandbox if the user's shell startup files need desktop access. The rendering check uses real independent Ghostty engines and an instrumented drawing boundary to verify hidden output updates without drawing and redraw on return; native visual behavior still needs manual checks.

Use [the native validation checklist](docs/VALIDATION.md) before proceeding to the full application. Findings and design boundaries are in [the stack evaluation](docs/STACK.md).

## Development server unavailable

The development WebView loads terminal WASM lazily when you press Start. If Vite has stopped, an already-open window can still display its interface but cannot fetch the terminal engine. Start the application with `mise exec -- npm run tauri -- dev`, which starts both Vite and the Rust app. After a temporary asset-load failure, Start retries instead of retaining the failed engine request. If restarting the whole development app, close the previous window using its confirmation flow first. Installed builds load their bundled assets and do not require Vite.
