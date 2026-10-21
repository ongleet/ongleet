# Repository instructions

## Communication and tools

- Use English for code, comments, documentation, commit messages, and other repository text. Match the user's language in conversation.
- Use `vim` rather than `nano` for interactive editing.
- Use mise for Node and Rust dependencies and commands. Ubuntu system libraries are installed through the system package manager.
- Read `README.md`, `docs/STACK.md`, and `docs/VALIDATION.md` before changing terminal architecture. Keep findings and limitations accurate; distinguish observed behavior from assumptions.
- Keep version one small. Report architectural blockers before building features on an unvalidated foundation.

## Scope and implementation order

This is a native Linux terminal workspace application built with Tauri 2, React, TypeScript, Rust, Ghostty WASM through `ghostty-web`, portable-pty, and SQLite. The current preview persists workspaces, tab definitions/openings, launch settings, ordering, and selection. Window hide/reopen and complete lifecycle guarantees remain pending.

1. Validate the terminal stack inside Tauri's WebKitGTK WebView with interactive Codex, including Wayland where available.
2. Complete reliable single-terminal lifecycle, process containment, and bounded IPC.
3. Add the workspace sidebar and terminal tabs.
4. Add SQLite metadata persistence and window hide/reopen behavior.
5. Add a CLI using a user-private Unix socket.
6. Package for Ubuntu and document installation and limitations.

Do not silently replace Ghostty with another engine. No Electron, xterm.js runtime, tmux, Git integration, worktree management, workspace setup scripts, or agent orchestration. These product exclusions do not prohibit ordinary repository maintenance requested by the user. Do not modify shell or Codex configuration automatically.

## Terminal invariants

- Ghostty's VT engine and the browser renderer are separate layers. WASM alone does not provide rendering, input, or selection. Verify behavior under WebKitGTK, not only Chromium or the standalone engine.
- Keep PTY ownership in Rust behind a terminal-session boundary independent of React. Do not recreate emulators during ordinary renders or navigation. Do not implement a daemon in version one.
- Launch executables with argument arrays. Shell syntax requires an explicit shell-command mode. Inherit the normal process environment and apply only explicit overrides.
- Preserve raw bytes across IPC, including split UTF-8 and escape sequences. Use bounded queues and backpressure through frontend consumption; a bounded reader queue alone does not bound Tauri's pending messages.
- Bound scrollback, input, and output buffers. Never block the session lock or WebView thread on PTY writes. Hidden terminals must consume output without unnecessary painting or dependence on animation-frame callbacks.
- Resize from the visible container; never resize hidden terminals to zero dimensions. Dispose instances, subscriptions, and native resources on explicit closure.
- Track running, stopped, and exited states with exit codes. Confirm closure of running processes and reject destructive CLI requests without explicit confirmation.
- Killing the shell or its initial process group is insufficient. The prototype's `/proc` session scan does not guarantee containment of escaped descendants. Resolve this before claiming no orphan processes after confirmed close.
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
- Window close must eventually hide the window while the app and PTYs keep running. Provide single-instance show/reopen via CLI and desktop launcher, plus an explicit Quit action with running-process confirmation.
- The user changed the original restart policy: automatically launch new processes for tabs that were running before shutdown, or in the last committed running set after a crash. Explicitly stopped/closed tabs and observed exits must not auto-start. Metadata restoration does not restore live processes or screens. Processes do not survive crashes, Quit, or reboot; commands may be launched anew next time. Historical output remains optional.
- Separate available tab definitions from their openings. Closing a tab preserves its definition; reopening explicitly launches its saved command. Store the definition kind for future nonterminal types, but only terminal tabs are implemented now, with one opening per definition.
- UI and CLI must use the same Rust operations. The CLI must never access SQLite directly. Restrict the Unix socket to the current user; use stable targeting IDs, JSON output, structured errors, and meaningful exit codes. Do not silently start the application when unavailable.

## Code map

- `src/terminal.ts`: persistent frontend controller, Ghostty integration, input/output attachment, resizing, clipboard, snapshots.
- `src/byte-transport.ts`: shared raw-byte adapter and transport limits.
- `src/render-visibility.ts`: gates the public Ghostty drawing boundary for hidden workspaces while VT parsing continues.
- `src/main.tsx`: prototype React interface.
- `src-tauri/src/session.rs`: native PTY ownership, output acknowledgements, input worker, process status and cleanup, integration tests.
- `src-tauri/src/main.rs`: Tauri commands and application setup.
- `src-tauri/src/workspace.rs`: durable metadata, definition/opening state, resume policy, independent sessions, and operations shared by Tauri commands.
- `src-tauri/src/storage.rs` and `src-tauri/migrations/`: SQLite migrations, transactional metadata and exclusive writer lock. Never silently reset a database on failure.
- `src-tauri/capabilities/default.json`: frontend permissions; grant only capabilities required by implemented features.
- `scripts/terminal-probe.py`: interactive Unicode, flood, keyboard, mouse, and alternate-screen probes.

## Verification

Run checks appropriate to the change:

```bash
mise exec -- npm run check:host
mise exec -- npm run build
mise exec -- npm run check:wasm
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
