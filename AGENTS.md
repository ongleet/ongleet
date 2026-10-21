# Repository instructions

## Communication and tools

- Use English for code, comments, documentation, commit messages, and other repository text. Match the user's language in conversation.
- Use `vim` rather than `nano` for interactive editing.
- Use mise for Node and Rust dependencies and commands. Ubuntu system libraries are installed through the system package manager.
- Read `README.md`, `docs/STACK.md`, and `docs/VALIDATION.md` before changing terminal architecture. Keep findings and limitations accurate; distinguish observed behavior from assumptions.
- Keep version one small. Report architectural blockers before building features on an unvalidated foundation.

## Scope and implementation order

This is a native Linux terminal workspace application built with Tauri 2, React, TypeScript, Rust, Ghostty WASM through `ghostty-web`, portable-pty, and SQLite. The current preview persists workspaces, tab definitions/openings, launch settings, ordering, and selection. Window hide/reopen, single-instance activation and explicit Quit are implemented; full CLI and packaging remain pending.

1. Validate the terminal stack inside Tauri's WebKitGTK WebView with interactive Codex, including Wayland where available.
2. Complete reliable single-terminal lifecycle, process containment, and bounded IPC.
3. Add the workspace sidebar and terminal tabs.
4. Add SQLite metadata persistence and window hide/reopen behavior.
5. Add a CLI using a user-private Unix socket.
6. Package for Ubuntu and document installation and limitations.

Do not silently replace Ghostty with another engine. No Electron, xterm.js runtime, tmux, Git integration, worktree management, workspace setup scripts, or agent orchestration. These product exclusions do not prohibit ordinary repository maintenance requested by the user. Do not modify shell or Codex configuration automatically.

## Terminal invariants

- Ghostty's VT engine and the browser renderer are separate layers. Native validation found mouse-report forwarding missing in ghostty-web 0.4.0; keep this blocker explicit until a browser adapter is implemented and tested. WASM alone does not provide rendering, input, or selection. Verify behavior under WebKitGTK, not only Chromium or the standalone engine.
- Initialize new Ghostty emulators with RIS after the initial container fit and redraw before connecting output; shared WASM allocation reuse otherwise exposed closed-tab cells. Never reset live emulators during navigation or routine resizing.
- Keep PTY ownership in Rust behind a terminal-session boundary independent of React. Do not recreate emulators during ordinary renders or navigation. Do not implement a daemon in version one.
- Launch executables with argument arrays. Shell syntax requires an explicit shell-command mode. Inherit the normal process environment and apply only explicit overrides.
- Preserve raw bytes across IPC, including split UTF-8 and escape sequences. Use bounded queues and backpressure through frontend consumption; a bounded reader queue alone does not bound Tauri's pending messages.
- Bound scrollback, input, and output buffers. WebView reload attaches to existing PTYs using a bounded 4 MiB byte/resize replay journal; never relaunch a live session on reload. Suppress input/query replies and automatic resizing during replay, retain ACK backpressure, and report unavailable history after journal overflow. Never block the session lock or WebView thread on PTY writes. Hidden terminals must consume output without unnecessary painting or dependence on animation-frame callbacks.
- Resize from the visible container; never resize hidden terminals to zero dimensions. Dispose instances, subscriptions, and native resources on explicit closure.
- Track running, stopped, and exited states with exit codes. Confirm closure of running processes and reject destructive CLI requests without explicit confirmation.
- Each terminal must join a delegated cgroup v2 leaf before user exec. Require cgroup.kill and pidfds, fail clearly without containment, and never silently fall back to process groups or /proc scans. Confirmed closure uses immediate SIGKILL and waits for cleanup. Keep the session-scoped app/leader-death watchdog outside the leaf. This is not a security sandbox for deliberately migrated processes or externally delegated services.
- Do not use synchronous window.confirm checks: the Tauri dialog plugin replaces it with a Promise. Use the in-app confirmation dialog and await the result before destructive operations.
- Use Tauri's native clipboard plugin for application copy/paste. Browser clipboard APIs failed under WebKitGTK. Pass pasted text through Ghostty's paste API to preserve bracketed-paste behavior.
- Ghostty's existing frontend state is authoritative for plain-text snapshots. Do not regex-strip ANSI or add a second emulator. Bound snapshots and return an explicit unavailable error when no connected frontend can provide them.

## Workspace and persistence contract

- Two navigation levels only: workspaces in the sidebar, terminal tabs within each workspace.
- User-configurable workspace properties are name and an existing base directory. Store stable IDs and display order internally.
- Tab metadata includes stable ID, editable name, working-directory override, executable and arguments, explicit environment overrides, and ordering. Process status comes from the runtime.
- New tabs default to the user's shell in the workspace directory. Switching tabs or workspaces preserves processes and emulator state.
- Creating a workspace requires an existing directory and supports a native picker. Renaming or closing a workspace must never create, move, modify, or delete its directory.
- Updating a workspace's base directory affects future launches, not running processes. Keep missing directories in metadata and report a clear launch error; allow correcting the directory.
- SQLite belongs in the standard application user-data directory. Use migrations and transactional updates. Persist selected workspace and selected tab per workspace. Never store the inherited environment or add credential storage.
- Native window close hides the window while the app and PTYs keep running. Repeated launch or --show activates it through a private socket; --show must never start an unavailable app. Keep explicit Quit confirmation and clean up before exiting. Desktop installation remains packaging work.
- The user changed the original restart policy: automatically launch new processes for tabs that were running before shutdown, or in the last committed running set after a crash. Explicitly stopped/closed tabs and observed exits must not auto-start. Metadata restoration does not restore live processes or screens. Processes do not survive crashes, Quit, or reboot; commands may be launched anew next time. Historical output remains optional.
- Separate available tab definitions from their openings. Closing a tab preserves its definition; reopening explicitly launches its saved command. Store the definition kind for future nonterminal types, but only terminal tabs are implemented now, with one opening per definition.
- UI and CLI must use the same Rust operations. The CLI must never access SQLite directly. Restrict the Unix socket to the current user; use stable targeting IDs, JSON output, structured errors, and meaningful exit codes. Do not silently start the application when unavailable.

## Code map

- `src/terminal.ts`: persistent frontend controller, Ghostty integration, input/output attachment, resizing, clipboard, snapshots.
- `src/byte-transport.ts`: shared raw-byte adapter and transport limits.
- `src/render-visibility.ts`: gates the public Ghostty drawing boundary for hidden workspaces while VT parsing continues.
- `src/main.tsx`: prototype React interface.
- `src/WorkspaceSidebar.tsx`: per-workspace action menus, drag handles and keyboard reordering.
- `src-tauri/src/session.rs`: native PTY ownership, output acknowledgements, input worker, process status and cleanup, integration tests.
- `src-tauri/src/main.rs`: Tauri commands and application/window lifecycle.
- `src-tauri/src/containment.rs`: Linux cgroup containment, exec handshake and session-scoped death watchdog.
- `src-tauri/src/instance.rs`: private single-instance activation socket; full CLI remains separate work.
- `scripts/check-lifecycle.py`: isolated X11/XWayland native hide/reopen, activation, Quit and restart checks.
- `src-tauri/src/workspace.rs`: durable metadata, definition/opening state, resume policy, independent sessions, and operations shared by Tauri commands.
- `config/tab-types.yaml` and `src-tauri/src/tab_types.rs`: default YAML launch types and validated user configuration, reloaded for New tab. Existing openings keep independent SQLite configuration.
- `src-tauri/src/storage.rs` and `src-tauri/migrations/`: SQLite migrations, transactional metadata and exclusive writer lock. Never silently reset a database on failure.
- `src-tauri/capabilities/default.json`: frontend permissions; grant only capabilities required by implemented features.
- `scripts/check-native.py` and `src/native-validation.ts`: isolated WebKitGTK automation and mode-gated bounded diagnostics; never target the user's live app.
- `scripts/check-terminal-reuse.mjs`: shared-WASM create/free/resize regression.
- `scripts/terminal-probe.py`: interactive Unicode, flood, keyboard, mouse, and alternate-screen probes.

## Verification

Run checks appropriate to the change:

```bash
mise exec -- npm run check:host
mise exec -- npm run build
mise exec -- npm run check:wasm
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:rendering
mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Cargo commands require a populated cache. Fetch dependencies online when necessary. Launch the native prototype with:

```bash
mise exec -- npm run tauri -- dev
```

Prioritize real PTY and native integration checks over mocks or tests that mirror implementation. Follow `docs/VALIDATION.md` for interactive Codex, Unicode, selection, clipboard, scrolling, keyboard shortcuts, mouse reporting, alternate screen, resizing, and flood responsiveness. Build success and engine tests do not certify these behaviors. Record unavailable desktop access as a testing limitation, not a terminal-stack failure or a passing native test.

For later stages, verify hidden-window continuity, state preservation while switching, child cleanup, Quit/restart restoration, CLI/socket behavior, missing directories, failed launches, and directory non-mutation. Never mark a whole validation stage complete based on confirmation of one behavior.
