# Engineering contracts and code map

Historical detailed reference, retained during documentation compression. Read the
[current guide](../ENGINEERING.md) first; older status statements may be superseded.

Read the sections relevant to your change. These contracts supplement
[AGENTS.md](../../AGENTS.md); validation evidence lives in
[VALIDATION.md](VALIDATION.md), and stack decisions in [STACK.md](STACK.md).

## Scope and implementation order

DevCenter uses Tauri 2, React/TypeScript, Rust, Ghostty (`ghostty-web`), portable-pty and SQLite. Priorities: native terminal validation → bounded lifecycle/containment → navigation → persistence/window lifecycle → private-socket CLI → Ubuntu packaging. Consult `TODO.ignore.md` for remaining work.

Do not silently replace Ghostty with another engine. No Electron, xterm.js runtime, tmux, automatic Git discovery, worktree management, session setup scripts, or agent orchestration. These product exclusions do not prohibit ordinary repository maintenance requested by the user. Do not modify shell or Codex configuration automatically. Explicit session-linked, read-only GitHub PR status through the authenticated gh CLI is an intentional scope expansion.

## Terminal invariants

- Ghostty's VT engine and browser renderer are separate. The mouse adapter passed native Wayland checks; X11 acceptance remains pending. Validate rendering/input/selection in WebKitGTK, not merely WASM or Chromium.
- Initialize new Ghostty emulators with RIS after the initial container fit and redraw before connecting output; shared WASM allocation reuse otherwise exposed closed-tab cells. Never reset live emulators during navigation or routine resizing.
- Keep PTY ownership in Rust behind a terminal-session boundary independent of React. Do not recreate emulators during ordinary renders or navigation. Do not implement a daemon in version one.
- Launch executables with argument arrays. Shell syntax requires an explicit shell-command mode. Inherit the normal process environment and apply only explicit overrides.
- Preserve raw bytes across IPC, including split UTF-8 and escape sequences. Use bounded queues and backpressure through frontend consumption; a bounded reader queue alone does not bound Tauri's pending messages.
- Bound scrollback, input, and output buffers. WebView reload attaches to existing PTYs using a bounded 4 MiB byte/resize replay journal; never relaunch a live session on reload. Suppress input/query replies and automatic resizing during replay, retain ACK backpressure, and report unavailable history after journal overflow. Never block the session lock or WebView thread on PTY writes. Hidden terminals must consume output without unnecessary painting or dependence on animation-frame callbacks.
- Resize from the visible container; never resize hidden terminals to zero dimensions. Dispose instances, subscriptions, and native resources on explicit closure.
- Track running, stopped, and exited states with exit codes. Confirm closure of running processes and reject destructive CLI requests without explicit confirmation.
- Re-exec the running application through `/proc/<app-pid>/exe` for terminal bootstrap; its filesystem path can be replaced/deleted by rebuilds or upgrades. Never strip a deleted suffix or run an unrelated replacement image.
- Each terminal must join a delegated cgroup v2 leaf before user exec. Require cgroup.kill and pidfds, fail clearly without containment, and never silently fall back to process groups or /proc scans. Confirmed closure uses immediate SIGKILL and waits for cleanup. Keep the session-scoped app/leader-death watchdog outside the leaf. This is not a security sandbox for deliberately migrated processes or externally delegated services.
- Do not use synchronous window.confirm checks: the Tauri dialog plugin replaces it with a Promise. Use the in-app confirmation dialog and await the result before destructive operations.
- Use Tauri's native clipboard plugin for application copy/paste. Browser clipboard APIs failed under WebKitGTK. Pass pasted text through Ghostty's paste API to preserve bracketed-paste behavior.
- Ghostty's existing frontend state is authoritative for plain-text snapshots. Do not regex-strip ANSI or add a second emulator. Bound snapshots and return an explicit unavailable error when no connected frontend can provide them.

## Session and persistence contract

- Navigation is projects → sessions → terminal tabs. Projects have a name and existing root, supply the default path for new sessions, and fold their sidebar list without changing processes or the active terminal. Persist membership/fold state. Existing session paths never follow root changes automatically.
- User-configurable session properties are name and an existing base directory. Store stable IDs and display order internally.
- Tab metadata includes stable ID, editable name, working-directory override, executable and arguments, explicit environment overrides, and ordering. Process status comes from the runtime.
- External `kind: command` actions launch explicitly without tabs or PTYs and intentionally outlive session/app closure. Bound direct-child waiters, discard output, reap children and report launch/nonzero-exit failures. Resolve documented session/project placeholders literally in argument arrays; shell commands use quoted reserved context environment variables. Never automatically execute external actions or store them as tab definitions.
- Common project environment variables live in the actions YAML root `env`. Merge inherited process environment → common group env → explicit action overrides at launch. Never mutate running process environments or persist the inherited/merged environment in tab metadata.
- New tabs default to the user's shell in the session directory. Switching tabs or sessions preserves processes and emulator state.
- Creating a session opens and starts the first configured terminal project action with its name and copied launch settings; validate YAML before changing metadata. Never create a generic stopped Terminal N placeholder. Creating a session requires an existing directory and supports a native picker. Renaming or closing a session must never create, move, modify, or delete its directory.
- Updating a session's base directory affects future launches, not running processes. Keep missing directories in metadata and report a clear launch error; allow correcting the directory.
- Files tabs remember their current folder and optional open editor file in SQLite tab configuration. Folder updates must preserve the editor file; returning to Files clears only the editor file. Restore saved disk contents, never silently discard a live editor on metadata refresh, and report missing/unsupported files without recreating or externally launching them. Unsaved editor contents remain in memory.
- SQLite belongs in the standard application user-data directory. Use migrations and transactional updates. Persist selected session and selected tab per session. Never store the inherited environment or add credential storage.
- Native window close confirms and flushes plugins, then hides for two minutes before cleanup/exit. Reopening cancels the deadline. Explicit Quit remains immediate after confirmation. Repeated launch or --show activates it through a private socket; --show must never start an unavailable app. Keep explicit Quit confirmation and clean up before exiting. Desktop installation remains packaging work.
- The user changed the original restart policy: automatically launch new processes for tabs that were running before shutdown, or in the last committed running set after a crash. Explicitly stopped/closed tabs and observed exits must not auto-start. Metadata restoration does not restore live processes or screens. Processes do not survive crashes, Quit, or reboot; commands may be launched anew next time. Historical output remains optional.
- Separate available tab definitions from their openings. Closing a tab preserves its definition; reopening explicitly launches its saved command. Store the plugin identifier and configuration version/payload with each definition. Built-in terminal, files with text editing and session-linked pull-request plugins have one opening per definition.
- UI and CLI must use the same Rust operations. The CLI must never access SQLite directly. Restrict the Unix socket to the current user; use stable targeting IDs, JSON output, structured errors, and meaningful exit codes. Do not silently start the application when unavailable.

## Code map

- Frontend: `src/main.tsx`, `src/plugins/`, `src/ProjectEditor.tsx`, `src/SessionSidebar.tsx`.
- Terminal: `src/plugins/terminal/frontend.ts`, `src/byte-transport.ts`, `src/render-visibility.ts`; Rust `terminal_session.rs`, `containment.rs`.
- Backend: `src-tauri/src/session/`, `plugins/`, `storage.rs`, `migrations/`, `actions.rs`.
- Lifecycle/CLI: Rust `main.rs`, `instance.rs`, `control/`, `cli/`, `bin/`; Codex wrapper: `codex.rs`, `bin/dcodex.rs`.
- Validation: `docs/VALIDATION.md`, `scripts/check-*.py`, `scripts/check-*.mjs`, `scripts/terminal-probe.py`.
- Actions: `config/actions.yaml` and per-project `projects/<id>/actions.yaml`; only the `actions` schema, no legacy/global fallback. Reload on creation; existing openings retain independent SQLite configuration. Keep configuration outside project directories.
- Branding migrations must preserve database sidecars/Codex mappings and reject live writers/conflicts. Grant only required frontend capabilities.

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
