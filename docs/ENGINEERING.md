# Engineering guide

Read the sections relevant to your change. See [stack](STACK.md), [validation](VALIDATION.md) and [full contracts](reference/ENGINEERING.md) for detail.

## Native service boundary

Each GUI, CLI and helper entry point constructs `system::SystemServices` once.
The registry receives it through `SessionRegistry::open_with`; storage and
standalone operations receive the service they use. Tauri commands obtain the
same services from application state. Production features must never call the
factory. Test-only fixture constructors may compose real Linux services.

`src-tauri/src/system/mod.rs` defines the object-safe contracts: `SystemPaths`,
`TerminalService`, `ProcessService`, `ControlTransport`, `SystemFiles`,
`SecretStore` and `DesktopService`. Async methods return `SystemFuture`; errors
remain strings. Connections, listeners, locks and file watches are opaque owned
resources. Control endpoint representation is private to the backend. Desktop
contracts accept Tauri window/application handles, never GTK or WebKit objects.

The private `system/linux/` backend owns descriptors, PTYs, process groups,
cgroups, pidfds, watchdog registration, Unix sockets and peer credentials,
private permissions, inotify, Secret Service and GTK/WebKit integration. Native
resources remain lazy: composing services starts neither a watchdog nor a
credential connection. Linux dependencies are target-conditional; other targets
fail compilation explicitly unless they are macOS Apple Silicon or Windows x64.
The private `system/macos/` backend supplies Darwin mechanisms; see [macOS preparation](MACOS.md).
The private `system/windows/` backend owns ConPTY, Jobs, named pipes, ACLs, DPAPI
and WebView2-facing desktop integration; see [Windows](WINDOWS.md).
Linux retains its required confinement, with no fallback.

Features retain launch decisions, process deadlines/output bounds, Codex arguments
and restart policy, secret identities/migration policy, editor conflict checks,
window persistence, confirmations, delayed closure and diagnostic formatting.
Shell argument construction and Codex hook commands belong to `SystemPaths`;
feature code passes scripts or literal arguments without choosing native quoting.
`SystemFiles::atomic_replace` commits a flushed temporary file on the same volume;
`sync_directory` owns platform-specific directory durability. Existing-file updates
for YAML, Codex mappings, history, editor saves, geometry and log rotation use
these contracts. Migration renames to absent destinations remain ordinary moves.
Native diagnostics cross the boundary as typed events and counters. Terminal workers retain
bounded queues, ACK/replay/history and separate paste/Enter delivery. The backend
returns independent reader/writer cancellations, shared resize/stop control and
a consumed wait handle. The terminal service advertises strict or best-effort cleanup
and an independent optional post-exit drain timeout. Darwin uses a two-second
cancellation deadline and reports truncation; Linux keeps its existing drainage. Natural exit drains final output; explicit closure wakes
both I/O workers and waits for cleanup.

`npm run check:system-boundaries` rejects native APIs and Linux system paths in
production code outside the three private backends, backend selection outside `system/mod.rs`
and feature calls to the factory. The frontend build runs this check. Test-only
Rust items and fixture modules are excluded; native integration tests keep real
PTYs, sockets, files and cgroups.

## Terminal contracts

Rust owns PTYs. Keep controllers across renders and navigation. Preserve raw
bytes, bounded queues and ACK backpressure until frontend consumption. Hidden
terminals must consume output without animation frames. Idle terminals must not
run recurring render frames or frontend status polls.

Pinned Ghostty adapters replace private `startRenderLoop` and detach
`SelectionManager.boundClickHandler` to preserve selections after outside clicks.
On upgrades, revalidate writes, cursor notifications, hover, selection, scrolling,
visibility and listener cleanup. Resize only visible, nonzero containers.
Dispose resources on explicit closure.

On Linux, wait on PTY readiness and an eventfd cancellation signal without polling.
Windows uses overlapped pipes and cancellation events; Darwin uses kqueue.
Use a blocking reaper for child exit. Separate input and reader cancellation: natural
exit drains final output; explicit closure wakes both workers. ACK waits use
condition-variable notifications without periodic timeouts.

Write through a plain duplicated PTY descriptor. The portable-pty writer
destructor injects newline/EOF bytes during cleanup.

Terminal history snapshots run only after output or resize, with a five-second interval during continuous activity. Clean flushes skip scanning, and unchanged text skips persistence. Explicit lifecycle flushes bypass the interval; output arriving during an in-flight save stays dirty. Keep the existing 2,000-line/60,000-code-unit bounds and replay guards.

Use literal executable/argument arrays; shell syntax requires explicit shell mode. Re-exec `/proc/<app-pid>/exe`, never a replaced pathname. Join delegated cgroup v2 before user exec; require pidfds and `cgroup.kill`. Confirmed close immediately kills the leaf and waits; keep the death watchdog outside it. Deliberate cgroup migration/external services are outside containment.

One watchdog process is started lazily per application instance. The terminal
bootstrap passes its own pidfd over a private socket and waits for watchdog
registration before joining its cgroup and executing user code. The watchdog
blocks on the application and terminal pidfds; it owns no PTY descriptors.
Application death kills all registered groups. If the watchdog dies first, the
application kills its tracked groups and rejects further launches until restart.

Apply RIS only to new emulators after initial fit/redraw, before output attachment. Reload reattaches rather than relaunching. Suppress input/query replies/resizing during replay; report journal overflow. Ghostty frontend state is authoritative for snapshots: no second emulator or ANSI stripping.

Use native clipboard and Ghostty paste APIs. Validate selection mutations only on a private display; clipboard formats alone do not prove safe restoration on Wayland. A running Vite server can hot-reload a live development instance when source files change, even when test metadata/display are isolated. Await the in-app destructive confirmation; synchronous `window.confirm` is unsafe under Tauri.

## Metadata and actions

- Projects contain sessions and plugin tabs. Root changes affect future sessions; session directory changes affect future launches. Never mutate session directories during metadata operations.
- Validate YAML before creating metadata. A new session launches the first configured terminal action, never a stopped placeholder.
- Keep definitions separate from openings. CLI closure retains definitions; GUI removal archives them. Persist plugin ID/version/payload and selection. Runtime supplies process status.
- Use Diesel models and queries in `storage/models.rs` and `storage/schema.rs` for metadata reads/writes. Keep raw SQL confined to historical migrations and SQLite pragmas.
- Use transactional migrations in standard user-data SQLite; never reset a failed database. Store explicit overrides only, never inherited environment or credentials.
- Merge inherited environment, project root `env`, then action overrides. Existing processes/configurations stay independent. Action definitions use the `actions` YAML schema; inherited plugin defaults use `plugin-settings.yaml`. Configuration lives outside project roots.
- External commands launch explicitly, independently of tabs/PTYs. Reap bounded children, discard output and report failures. Preserve literal placeholders; shell context uses quoted reserved environment variables.
- Files folder updates preserve `editorFile`; Back clears only that field. Restore disk contents without silently discarding live edits. Drafts persist in WebView localStorage with their original revision, scoped by tab/path; missing files recover drafts without recreation. Explicit discard clears drafts; close/Quit retain them. Conflicting saves offer cancel, revision-checked force save, or a bounded worker-based three-way merge into the editor, rebasing onto current disk contents without writing.
- Window close confirms, flushes plugins and hides for two minutes before cleanup/exit; reopening cancels the deadline. Explicit Quit confirms and exits immediately; repeated launch/`--show` activates an existing instance. `--show` never starts an unavailable app. Bundled terminal launches detach after startup; `--foreground` retains diagnostic output. Restart the last recorded running set and eligible failed launches/nonzero `ong-codex` exits. Explicitly stopped/closed tabs and other exited commands never auto-start.
- UI/CLI share Rust operations. Keep sockets current-user-only, stable IDs, structured errors and meaningful exit codes. Never silently start the app.

## UI localization

Use `t` from `src/i18n.ts` for UI copy. English source messages are catalog keys;
French translations live in `src/locales/fr.ts`. Use named placeholders for dynamic
values and keep user content and protocol identifiers outside translation calls.
English is the default and fallback. The global configuration Appearance tab stores themes and language in
`actions.yaml`; localStorage provides a legacy fallback and startup cache.
Project configurations never inherit global appearance settings. `useLocale` subscribes React surfaces;
CodeMirror uses locale subscriptions to reconfigure existing editors without
recreating documents or undo history. Run `mise exec -- npm run check:i18n` for
preference, fallback, interpolation and static catalog checks.

## Shared form controls

Use `src/components/Controls.tsx` for app and plugin buttons, inputs, selects, textareas, dialogs,
disclosures and tabs. Primitives forward native props and React 19 refs without
DOM wrappers. Shared styles live in `src/style.css`; contextual classes handle
compact toolbars, icon buttons and layout variants.

`Tab`, `TabList` and `TabPanel` own accessibility roles. Callers supply labels,
selection, relationships and navigation. `Tab` defaults to a non-submitting
button; `aria-selected` sets focus order. Regular `Button` keeps native submit
behavior. Keep native `option`, `label` and `form` elements. Menus keep their roles,
popover/disclosure behavior and shared buttons. Libraries own editor/terminal DOM.

Use `src/components/Checkbox.tsx` for app checkbox inputs. Shared styles cover
unchecked, checked, disabled and keyboard-focus states, including CodeMirror's
generated search checkboxes. Exclude checkboxes from text-input sizing rules.
Checkbox-style menu items keep menu roles and button behavior.

## Code map

- UI/plugins: `src/main.tsx`, `src/plugins/`, `src/ProjectEditor.tsx`, `src/SessionSidebar.tsx`.
- Terminal frontend: `src/plugins/terminal/frontend.ts`, `src/byte-transport.ts`, `src/terminal-rendering.ts`, `src/render-visibility.ts`.
- Rust PTY/containment: `src-tauri/src/terminal_session.rs`, `src-tauri/src/system/linux/terminal.rs`, `src-tauri/src/system/linux/containment.rs`.
- Plugin SDK/core services: `src/sdk/`, `src/plugins/*/manifest.json`, `src-tauri/src/core/`; see [plugin development](PLUGINS.md).
- Registry/storage/actions: `src-tauri/src/session/`, `plugins/`, `storage.rs`, `migrations/`, `actions.rs`.
- Lifecycle/control: Rust `main.rs`, `instance.rs`, `control/`, `cli/`, `bin/`. Codex: `codex.rs`, `bin/ong-codex.rs`.
- Shipped defaults: `config/actions.yaml`; user template: `$XDG_CONFIG_HOME/ongleet/actions.yaml` (copied on project initialization); project overrides: `projects/<id>/actions.yaml`.
- Validation: `scripts/check-*.py`, `scripts/check-*.mjs`, `scripts/terminal-probe.py`.

Preserve database sidecars/Codex mappings during branding migration; reject live writers/conflicts. Grant only required frontend capabilities.

## Checks

Run checks appropriate to the change through mise:

```bash
mise exec -- npm run check:host
mise exec -- npm run build
mise exec -- npm run check:plugins
mise exec -- npm run check:system-boundaries
mise exec -- npm run check:editor-merge
mise exec -- npm run check:wasm
mise exec -- npm run check:terminal-history
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:terminal-selection
mise exec -- npm run check:rendering
mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Cargo needs a populated cache. Launch development with `mise exec -- npm run tauri -- dev`. Native evidence, not build success, establishes WebKitGTK behavior.
