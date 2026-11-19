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
run recurring render frames or frontend status polls. Visible output paints at most
30 times per second; selection, hover, scroll and visibility redraws bypass the
output delay. Parsing and ACK backpressure remain synchronous.

Pinned Ghostty adapters replace private `startRenderLoop` and detach
`SelectionManager.boundClickHandler` to preserve selections after outside clicks.
On upgrades, revalidate writes, cursor notifications, hover, selection, scrolling,
visibility and listener cleanup. Resize only visible, nonzero containers.
Dispose resources on explicit closure.

`src/terminal-osc.ts` supplies the pinned engine's missing OSC 10/11 default-color
query replies from the current terminal theme, preserving BEL/ST terminators.
Replies use the bounded input queue and its replay/disposal guards. The adapter
consumes unsupported VTE shell (OSC 666) and program-status (OSC 7501)
notifications before Ghostty, leaving other bytes and warnings intact. Pending
OSC capture is capped at 8 KiB; oversized sequences pass through unchanged.
This is a narrow compatibility boundary, not a second terminal emulator.

Restored plugin content attaches lazily on first visibility or configuration
opening, then remains mounted while hidden. Terminal and PR entrypoints opt into
immediate background attachment to preserve PTY consumption and status polling.
Terminal `loadOnDemand` configuration overrides eager attachment until first
visibility or a validated state/action interaction. Wake requests use bounded
broker reservations before emitting `tab-wake`, bind the registered attachment,
and revalidate caller/target identities before dispatch. Startup shares the
five-second request deadline. Terminal handlers register after startup/replay;
input actions serialize and check cancellation before dispatch. Once attached,
hidden terminals keep consuming output. Snapshot-only `runtimePresent` forces
reattachment of live PTYs on WebView reload, without persisting runtime state.
Closed/stopped definitions keep their existing start eligibility.

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
- Window close and Quit share confirmation, plugin flush and immediate cleanup/exit; repeated launch/`--show` activates an existing instance. `--show` never starts an unavailable app. Bundled terminal launches detach after startup; `--foreground` retains diagnostic output. Restart the last recorded running set and eligible failed launches/nonzero `ong-codex` exits. Explicitly stopped/closed tabs and other exited commands never auto-start.
- UI/CLI share Rust operations. Keep sockets current-user-only, stable IDs, structured errors and meaningful exit codes. Never silently start the app.

## Diff rendering

Small Diff patches remain expanded regardless of file count. Patches above
500 lines or 64 KiB show a size-limit explanation and a Load diff button instead
of mounting the patch renderer immediately; manual review choices are retained.
Viewed marks are tied to the exact patch text in memory. When that text changes,
the mark clears and the file reopens regardless of size, staying open on refresh
until the reader changes its expansion state. Unchanged patches retain their marks.
Expanded file contents use `content-visibility: auto` with a remembered intrinsic
block size, allowing the browser to skip offscreen layout and paint while keeping
DOM selections and discussions mounted. Headers remain outside size containment
for sidebar navigation and sticky positioning. Unsupported engines render normally.
React mounting and patch parsing still run on the UI thread. Syntax highlighting
runs through one lazy shared module worker (`syntax-client.ts`, `syntax-worker.ts`),
with one request in flight, queued requests shared by patch/path, and a ten-second
request deadline. Plain patches render before tokens arrive. Unmounts and patch
changes cancel their subscriptions; the last cancellation terminates active work.
Results are applied only to matching path/patch/hunks. Errors retain plain text.
The worker stops after ten idle seconds. A sixteen-entry LRU caches only patch keys
up to 32K UTF-16 code units (64 KiB); theme changes reuse tokens and update CSS.
Vite's ES worker format preserves lazy language-parser imports.

Discussion snapshots remain immutable after publication. `CommentStore.reconcile`
retains unchanged thread references and skips no-op persistence. `DiscussionIndex`
indexes each new snapshot once by path, separating file, line and orphan threads;
unchanged buckets retain their identities as scoped render revisions. Patch widgets
and handlers remain stable across unrelated updates without serializing comments
inside React renders. `LineDiscussionIndex` looks up widgets by side/line and merges
both sides in source order on context rows. Thread IDs key widgets so removing one
thread does not discard another thread's reply draft on the same line.

## Agent message queues

`src/components/agent-queue.ts` owns the bounded per-session/recipient FIFO and
work/idle gate. The shared service binds queue observers and writes to host-owned
SDK attachments declaring `tabs`; composer unmounting does not own pending work.
It forwards captured user messages only, with no agent creation or task planning.

Persist a `sending` marker before the bounded `send-text` interaction. Interrupted
writes restore as uncertain; no automatic retries or restart resumption. Each
successful write requires a subsequent observed work/idle cycle before another
automatic write. Recheck idle after capability discovery and immediately before
invoking the target. Unknown/stopped/unavailable activity pauses delivery. Pending
text is a capture, not a live source reference. A same-session sender can take over
observation after source closure; an in-flight write is never replayed on handoff.
Quit pauses queues and settles current writes before plugin history flush; ordinary
archive/close flushes do not pause unrelated queues. UI subscriptions are scoped to
the recipient; selector updates depend on pending recipient membership and names.

Run `npm run check:selection-send` for payload, activity and FIFO behavior.
These checks do not establish native WebKitGTK interactions or live-agent timing.

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
- Terminal frontend: `src/plugins/terminal/src/frontend.ts`, `src/byte-transport.ts`, `src/terminal-rendering.ts`, `src/render-visibility.ts`.
- Rust PTY/containment: `src-tauri/src/terminal_session.rs`, `src-tauri/src/system/linux/terminal.rs`, `src-tauri/src/system/linux/containment.rs`.
- Diff plugin: `src/plugins/diff/src/Content.tsx` composes the tab; `components/` owns UI,
  `hooks/` owns comparison/discussion/scroll lifecycle and SDK interactions, and root modules
  contain Git, review, comment and syntax models.
- Plugin SDK/core services: `src/sdk/`, `src/plugins/*/manifest.json`, `src-tauri/src/core/`; see [plugin development](PLUGINS.md).
- Registry/storage/actions: `src-tauri/src/session/`, `plugins/`, `storage.rs`, `migrations/`, `actions.rs`.
- Lifecycle/control: Rust `main.rs`, `instance.rs`, `control/`, `cli/`, `bin/`. Codex: `codex.rs`, `bin/ong-codex.rs`.
- Shipped defaults: `config/actions.yaml`; user template: `$XDG_CONFIG_HOME/ongleet/actions.yaml` (copied on project initialization); project overrides: `projects/<id>/actions.yaml`.
- Validation: `scripts/check-*.py`, `scripts/check-*.mjs`, `scripts/terminal-probe.py`.

Preserve database sidecars/Codex mappings during branding migration; reject live writers/conflicts. Grant only required frontend capabilities.

## Checks

Format JavaScript, TypeScript, CSS, JSON, YAML and Markdown with
`mise exec -- npm run format`. Prettier uses `.prettierrc.json`; generated files,
local notes, browser captures and profiling recordings are excluded through
`.prettierignore`. CI runs `mise exec -- npm run format:check`. Rust formatting
uses `cargo fmt` separately.

Run checks appropriate to the change through mise:

```bash
mise exec -- npm run check:host
mise exec -- npm run build
mise exec -- npm run check:plugins
mise exec -- npm run check:system-boundaries
mise exec -- npm run check:editor-merge
mise exec -- npm run check:wasm
mise exec -- npm run check:tab-titles
mise exec -- npm run check:terminal-history
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:terminal-selection
mise exec -- npm run check:rendering
mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Cargo needs a populated cache. Launch development with `mise exec -- npm run tauri -- dev`. Native evidence, not build success, establishes WebKitGTK behavior.

## Linux CPU sampling

Find the GUI PID with `pgrep -a ongleet-gui`, then run:

```bash
python3 scripts/profile-cpu.py <PID> --duration 30
```

The tool samples `/proc` without privileges or dependencies and prints the busiest
20 threads each interval, followed by accumulated CPU seconds. One full core is
100%. The explicit process tree includes WebKit and terminal children while they
remain descendants. Command arguments and terminal contents are never collected.
Sampling is read-only and stops if the root exits or its PID is reused.
Short-lived threads between samples, CPU before a thread's first observation,
and reparented processes are not measured. This identifies busy threads, not
function call stacks; it does not establish the cause of a WebKit busy loop.

## Profiling the installed Linux package

Build an optimized diagnostic Debian package with WebKit Inspector and JavaScript
source maps:

```bash
mise exec -- npm run package:deb:profile
sudo apt install --reinstall ./src-tauri/target/release/bundle/deb/Ongleet_0.1.0_amd64.deb
```

Quit the previous instance using Application > Quit or the window close button. Then launch the installed binary from a separate desktop terminal:

```bash
ONGLEET_DEBUG=1 /usr/bin/ongleet-gui --foreground
```

The profiling feature opens WebKit Inspector when debug diagnostics are enabled.
In Inspector, select Timelines, enable JavaScript & Events recording, start a
recording, reproduce the slow Diff/comment interaction, and stop recording.
Inspect the JavaScript call tree and layout/rendering records. Export the timeline
recording using Inspector's export control for analysis. Available labels depend
on the installed WebKit version. Source maps accompany the optimized assets.
This profiles the installed WebView; the proc sampler above only measures thread
CPU. The normal `package:deb` build does not enable the profiling feature.

## Diff render diagnostics

With `ONGLEET_DEBUG=1`, Diff logs bounded console batches prefixed
`[diff-diagnostics]`. In the profiling package's WebKit Inspector Console, filter
by that prefix, reproduce the slowdown, then copy the output. These are frontend
console messages, not entries in the native `debug.log`.

The first batch includes an `enabled` event. Select Info or All message levels
in Inspector Console. These messages are not embedded in exported timeline JSON;
copy Console output separately. Diff declares the `env` capability required to
read `ONGLEET_DEBUG`; initialization errors produce a warning rather than silently
disabling diagnostics.

`render-committed` records the component, instance number, render count and names
of changed tracked inputs. `App` and `SessionPane` are tracked alongside Diff
components to distinguish parent updates from local state changes. `parent-or-untracked-input` means none of those tracked
inputs changed; it does not establish the exact scheduling cause. The hook logs
commits, not aborted render attempts or DOM paint duration. Initialization reads
the shared debug environment once; initial commits before that read completes
are omitted. Instance numbers are local to the current WebView.

`textarea-input`, `refresh-scheduled`, `refresh-start`, `highlight-request` and `highlight-ready`
provide timestamped correlation. Comment text, paths and prop/state values are
never printed. Batches flush after 500 ms when the event loop permits; at most
128 events are retained per batch and overflow is dropped. Console diagnostics
add overhead and cannot alone establish that input caused a concurrent render.

Codex sidebar summaries are derived from open tabs' runtime activity, never written
by individual hooks as a shared tag. The reserved `Codex` display replaces legacy
tags of that name. Counted indicators show questions/approvals in orange, work
(including tool calls) in blue, idle in green and stopped in gray; hover details
list member tabs. Empty groups and summaries without participating tabs are hidden.
