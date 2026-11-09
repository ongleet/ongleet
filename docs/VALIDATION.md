# Validation guide

The terminal compatibility gate remains open. Builds and engine tests do not
certify WebKitGTK behavior. Record skipped checks and unavailable desktop access
as limitations, never passes. One passing check does not complete a stage.

## Recorded evidence

Observed host: Ubuntu 26.04.1, GTK 3.24.52, WebKitGTK 2.52.6 and ghostty-web 0.4.0.
Interactive Wayland Codex version: 0.163.0. X11 evidence uses XWayland, not a
separate Xorg session. Results below describe individual runs, not current coverage
of every subsequent change.

| Area             | Evidence and limits                                                                                                                                                                                                                                                                                |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Terminal         | Wayland/XWayland typing, UTF-8, mouse/curses, clipboard, alternate-screen resize/replay, reload, hidden output and reuse passed (`nbj7b88j`, `odspnvg9`). Hidden floods reached 350,000 lines with bounded scrollback and replay-overflow warnings. Finite runs do not prove global memory bounds. |
| PTY ownership    | Real PTYs passed ACK backpressure, blocked-input cancellation, exact bytes, exit notifications, replay, detached descendants and owner-death cleanup. Event-driven waits passed 146 Rust tests on 2026-11-05; desktop CPU impact remains unmeasured.                                               |
| Idle and history | Five-terminal sampling found zero frames/draws/status polls over three seconds (`8fohim6s`). Later five-second snapshot throttling passed model/build checks; native lifecycle/restart and CPU comparisons remain untested. Abrupt death can lose the latest history.                              |
| Lifecycle        | Hide/reopen, selective fresh-process restart, activation and delayed close passed. A 15-check run passed with zero survivors (`4xnratrw`); a mixed-plugin run timed out during close confirmation (`96xd5pgl`). Cleanup remains unresolved.                                                        |
| Files/editor     | Refresh, drafts, conflict/merge, restoration and archives passed 23 checks (`6p264nv5`). Hidden-tab CLI/SDK `open-path` safeguards passed (`ld39o4eu`). Physical shortcuts and external-writer races remain limits.                                                                                |
| Diff             | General checks passed (`f8p_k8zr`); syntax/theme/reload passed on both backends (`3_590aof`, `9aq64pbc`). Navigation and refresh retained file/viewport/unchanged DOM (`o6k1uyxg`, `57rr9kk8`). New line wrapping has build evidence only.                                                         |
| Interactions     | CLI/SDK discovery, hidden targets, ownership, bounds, cancellation and ambiguous errors passed (`nne3so8x`). Discussions/UI and exact PTY delivery passed (`vt2w5hyf`); selected code and unsaved-buffer delivery passed (`thq9n1ru`). Delivery does not prove agent processing.                   |
| Notes            | Multiple notes, revision checks, autosave, reload and archive/restore passed (`vcm59k8w`). Immediate agent access after restore without reload remains unverified.                                                                                                                                 |
| Appearance       | Visual/state checks passed on private Wayland/XWayland displays. Shared checkboxes passed six presets (`ydsxeylq`). Headless native keyboard focus was unavailable. Ghostty 0.4.0 does not support live cell recoloring.                                                                           |
| Agent submission | On 2026-11-05, negotiated bracketed paste plus separate Enter submitted a 7,275-byte fixture to live Codex. A Diff comment also triggered a reply without manual Enter. Other agent TUIs/backends were not exercised.                                                                              |

Report IDs refer to temporary harness artifacts and may no longer exist.
[Historical acceptance records](reference/VALIDATION.md) describe earlier states;
current evidence above supersedes older pending or failing claims.

On 2026-11-07, 153 Rust tests passed across all targets (two credential tests
remained ignored). Recovery coverage includes missing-Codex recovery across
registry restart with the original conversation ID, explicit-stop suppression,
failed-spawn eligibility and normal-exit suppression. Shared-watchdog coverage
includes concurrent terminal registration with exactly one watchdog, independent
terminal closure, application death with three terminals and detached descendants,
and watchdog death with cleanup and rejection of further launches.

The rebuilt production binary also passed a private Weston Wayland check with
three live terminals and one watchdog: no watchdog PTY descriptors, no CPU ticks
during a 500 ms idle sample, independent terminal closure, and SIGKILL cleanup of
remaining leaders, detached descendants, cgroups and the watchdog. The same checks
passed with the executable extracted from the rebuilt Debian package. Artifacts:
`/tmp/ongleet-shared-watchdog-native-syag0h_x/report.json` (release binary) and
`/tmp/ongleet-shared-watchdog-native-ssp47n54/report.json` (Debian executable).
This does not certify physical desktop interaction, long-running resource
stability, or installation/upgrade behavior on the user's desktop.

## System service extraction (2026-11-07)

After extracting the seven injected services, the Rust suite passed 164 active
tests across all targets; two native credential tests were skipped by default.
Those two tests also passed separately on private D-Bus sessions with temporary,
unlocked Secret Service keyrings. Credential policy tests cover migration
preflight before any mutation, locked-store retention and deletion without
reading secret bytes. Real file tests retain revision/permission checks,
replacement/recreation notifications and explicit lock release despite an
inherited descriptor. Native PTY tests retain simultaneous registration,
independent close, crash cleanup, blocked I/O cancellation and replay coverage.

The rebuilt Debian executable passed eight service scenarios on private Weston
Wayland and XWayland displays: three live PTYs with one idle watchdog and no
watchdog PTY descriptors, exact Unicode CLI input, repeated-instance activation,
independent terminal closure, normal Quit with geometry persistence, selective
fresh-process restoration, SIGKILL cleanup of leaders, detached descendants
and cgroups, and typed native diagnostic delivery under `ONGLEET_DEBUG=1`.
Reports: `/tmp/ongleet-system-native-dcx643ig/report.json` (Wayland)
and `/tmp/ongleet-system-native-yqynyo7b/report.json` (XWayland). These checks observe
activation requests and successful existing-instance reuse, not physical focus.

Fifteen clipboard/selection/input checks passed with the rebuilt native-validation
binary on private Xvfb, including file URI quoting, image forwarding, size limits,
primary selection and exact CLI input. Both selections were restored and no
cleanup survivors remained. Report:
`/tmp/ongleet-clipboard-validation-dj6mk_kv/report.json`.

These finite runs do not certify physical desktop interaction, other compositors,
long-running stability or installation/upgrade behavior. Production frontend
assets are restored after native-validation checks.

Reproduce the packaged service checks without the diagnostic frontend bridge:

```bash
mise exec -- /usr/bin/python3 scripts/check-system-native.py --backend wayland \
  --application /absolute/extracted/usr/bin/ongleet-gui \
  --cli /absolute/extracted/usr/bin/ongleet
mise exec -- /usr/bin/python3 scripts/check-system-native.py --backend xwayland \
  --application /absolute/extracted/usr/bin/ongleet-gui \
  --cli /absolute/extracted/usr/bin/ongleet
```

## Remaining acceptance

- Physical keyboard focus, modal navigation, clipboard/selection and Japanese/Chinese
  candidate-selection IMEs. Separate Xorg and other compositors remain untested.
- Codex image attachment, independent resume/fork/hooks, missing history, crash
  recovery and closure during retry delay. Live GitHub authentication/polling is
  not certified by fixture responses.
- Unattended packaged heap corruption and the earlier live WebKit busy loop.
  The crash root cause remains unresolved; saved evidence is under
  `.validation/crash-2026-10-26/`.
- Mixed-plugin Quit timeout/cleanup, sustained resource bounds and sudden power
  loss. Builds do not prove memory stability or clean shutdown.
- Clean-system Ubuntu installation, upgrade behavior, WASM provenance and notices.
- Native acceptance for recursive Files filename/path search, including rapid
  query changes, hidden files, draft-safe result opening and partial-result messages.
- Native acceptance after event-driven PTY waits, history throttling and Diff
  wrapping. Current builds do not validate already-running executables.

Files saves cannot eliminate unrelated external-writer races or preserve
ACLs/xattrs. Draft persistence does not establish power-loss durability. Terminal
text snapshots do not restore processes, modes or a complete transcript.

## Reproduce native terminal checks

Use isolated metadata, private displays and test-owned processes. Never target the
user's live instance. Record OS, display backend, WebKitGTK, Ghostty and Codex
versions. A running Vite server can hot-reload other development instances.

```bash
mise exec -- npm run build -- --mode native-validation
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline
mise exec -- /usr/bin/python3 scripts/check-native.py --backend wayland
mise exec -- /usr/bin/python3 scripts/check-native.py --backend x11 --skip-mouse --skip-codex
```

Use Ubuntu `webkitgtk-webdriver` or `--driver /path/to/WebKitWebDriver`.
Desktop and writable delegated user cgroup access are required. Offline Cargo
needs a populated cache. Preserve `MISE_DATA_DIR` and `MISE_CACHE_DIR` when
isolating XDG directories to avoid fresh toolchain installation.

Copy bundled GUI/CLI executables before later Cargo builds can replace them.
Keep the CLI named `ongleet` beside the copied GUI. The diagnostic bridge exists
only in `native-validation` bundles. Clipboard mutation requires a private
display; skip unsafe desktop clipboard checks. Never change personal shell or
Codex configuration.

Harness reports live under their printed `/tmp/` paths. Exit codes: 0 selected
checks passed, 1 failure, 2 unresolved blockers. Harness cleanup alone does not
prove general containment. Restore production frontend assets afterward:

```bash
mise exec -- npm run build
```

## Lifecycle, plugins, startup and editor

After building the native-validation frontend and bundled GUI:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-terminal-startup.py --application /path/to/native-validation-app --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-file-editor.py \
  --driver /path/to/WebKitWebDriver \
  --application /path/to/native-validation-app
```

Lifecycle requires X11/XWayland and libX11. Add
`--plugins --application /absolute/copied/ongleet-gui` for mixed plugins; optionally
add `--projects --actions-editor --cli /absolute/copied/ongleet`.
Editor archive/socket checks accept `--cli /absolute/copied/ongleet`.

PR fixtures use dummy responses and require a private D-Bus session with an
unlocked temporary Secret Service. They refuse the desktop bus. Fixture success
does not certify live GitHub access.

## Diff, notes and appearance

Use the native-validation frontend and bundled GUI for interaction and Note
checks:

```bash
mise exec -- /usr/bin/python3 scripts/check-diff-native.py --interactions-only \
  --application /path/to/native-validation-app \
  --cli /path/to/ongleet --driver /usr/bin/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-diff-native.py --notes-only \
  --application /path/to/native-validation-app \
  --cli /path/to/ongleet --driver /usr/bin/WebKitWebDriver
```

For general Diff checks, build the normal production frontend and bundled GUI:

```bash
mise exec -- npm run build
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline
mise exec -- /usr/bin/python3 scripts/check-diff-native.py \
  --application src-tauri/target/debug/ongleet-gui --driver /usr/bin/WebKitWebDriver
```

Use `--syntax-only`, `--comments-only`, `--selection-only` or `--checkboxes-only`
for focused Diff scenarios. Appearance uses the native-validation build:

```bash
mise exec -- python scripts/check-appearance-native.py --backend wayland --surfaces
mise exec -- python scripts/check-appearance-native.py --backend xwayland --surfaces
```

Appearance fixtures use dummy authentication. Headless focus can return exit 2;
do not count unavailable focus checks as passes. Restore production frontend
assets after native-validation builds.

On 2026-11-08, global configuration appearance/language acceptance passed on a
private Wayland display (`wxhaphuj`): all six presets, retained terminal runtime/PID,
hidden output, editor draft/undo/redo, computed Files/Diff surfaces, YAML/form
roundtrips, live saved language changes and fresh-process YAML precedence over a
stale WebView cache. Cleanup reported zero survivors. Physical focus, long French
labels and localized CodeMirror search remain unverified.

## Manual checklist

1. Test shell controls, Unicode, multiline selection/paste and
   `scripts/terminal-probe.py` modes `unicode`, `flood`, `screen`. Exercise Codex
   editing, approvals, interrupts and resize without submitting unintended tasks.
2. Navigate, fold and reorder during output. Verify stable PIDs/screens/selection,
   immediate input focus and nonzero dimensions. Check literal arguments, explicit
   shell mode, environment and directory overrides separately.
3. Cancel, then confirm destructive closure. Verify sibling survival, cgroup
   cleanup and untouched working directories. Test missing executables/directories
   without recreating paths.
4. Reload during live, hidden and exited output. Check reply suppression, no live
   process relaunch, continuing output and overflow warnings. Reload does not
   restore terminal selection or scroll position.
5. Hide/reopen must retain PIDs. Quit/restart must create fresh PIDs only for
   previously running tabs. Check writer/socket ownership and saved configuration.
6. Test YAML errors atomically, editor draft/conflict guards, archive restore/purge,
   Note revisions and Diff selection/discussions. Metadata stores explicit
   environment overrides; terminal history remains separate from SQLite.

## Diagnostics

```bash
ONGLEET_DEBUG=1 mise exec -- npm run tauri -- dev
```

Enable diagnostics before launching; reopening an existing instance cannot change
its environment. GTK key/focus traces go to stderr and may reveal typed input.
Retain surrounding trace lines when reproducing warnings. `send_event` alone does
not identify physical input. See [crash investigation](USAGE.md#investigating-native-crashes)
for bounded log files and crash evidence.

## macOS port preparation (2026-11-07)

Linux frontend production build, system-boundary tests and 165 active Rust tests
passed (two Linux credential tests remain ignored by default). The Debian package
was rebuilt; its metadata and bundled executables were inspected, and its
extracted CLI/GUI help commands passed. These results
protect the common/Linux adaptations, not macOS desktop behavior.

A complete `cargo check --target aarch64-apple-darwin` was attempted on this Linux
host and stopped in `objc2-exception-helper`: the Apple compiler/SDK is missing.
An isolated ARM64 metadata-only harness at `/tmp/ongleet-darwin-typecheck` checked
the production Darwin paths, files, process, transport, kqueue, containment, PTY,
Security.framework, clipboard and input-monitor modules plus their native tests.
Its Tauri contract types/runtime signatures are placeholders and its URL type is
re-exported from `url`; it excludes `desktop.rs`, the actual application composition
and the common worker tests. It is not a complete application compilation, link,
execution or desktop validation.

All macOS execution, isolated keychain tests, Finder/Terminal acceptance,
WKWebView/Ghostty checks, full ARM64 build, signing verification and DMG production
are **not executed**. Follow [the macOS guide](MACOS.md) before qualification.

Rust formatting and formatting checks on changed frontend/documentation files
passed. The repository-wide Prettier check still reports four untouched files:
`.playwright-mcp/page-2026-10-27T20-35-19-474Z.yml`, `docs/PLUGINS.md`,
`src-tauri/linux/Ongleet.desktop.hbs` and `src/native-validation.ts`.

## Windows backend validation (2026-11-08)

The shared contracts now delegate explicit shell commands, Codex hook quoting,
atomic replacement and directory synchronization to native services. Containment
strength and post-exit output-drain deadlines are independent capabilities.
The boundary checker recognizes Windows native imports and the reserved private
Windows backend directory.

On Linux, the production frontend build, boundary checker tests and 174 active
Rust tests across all targets passed. Two native credential tests remained
ignored. New real-file checks cover first installation, replacement and failed
commit retention; real shell checks cover literal Unicode and metacharacters.
A real-PTY worker test confirms strict containment can coexist with bounded
post-exit drainage. An initial sandbox run rejected socket credentials; a native
rerun passed. One native run observed an existing watch-test startup race; the
subsequent complete run passed without changing the watcher.

The complete Tauri backend compiles on the hosted Windows x64 runner. At
`b1746c3`, all 187 Windows Rust tests passed. Native scenarios cover ConPTY
Unicode/resize, blocked I/O cancellation and saturated-output teardown, assignment
refusal before execution, owner-death descendant cleanup, detached applications,
executable/shim argument quoting and image-replacement guards, authenticated pipe
half-close/deadlines/bounds, file replacement/watches and DPAPI persistence,
attribute isolation, replacement/deletion, corruption and the 64 KiB limit.
Editor revision/format checks and legacy directory migrations also passed. The
metadata writer lease follows directory identity without holding a child file open.

The native Windows Codex subprocess fixture passed with Unicode/quoted deployment
paths, literal shell metacharacters, a 3 MiB hook event, conversation recording,
resume, fresh launch and bounded restart. Linux and macOS jobs passed in that run.
The workflow additionally requires a bundled release build, staged executable
smoke checks and ZIP upload; inspect the completed [Tests workflow](https://github.com/ongleet/ongleet/actions/workflows/tests.yml)
for the result at the deployed commit. Windows 11 physical desktop behavior is
untested; see [the acceptance checklist](WINDOWS.md).

## Inherited plugin settings (2026-11-09)

The production frontend/backend builds, 178 active Rust tests (two native
credential tests ignored), 22 PR behavior tests, plugin boundaries and i18n
checks passed. New Rust checks cover real-file global settings persistence,
sibling retention, stale/oversized-write rejection, corrupt-file preservation,
and nullable local configuration reset. The full Rust suite needed execution
outside the sandbox because its Unix socket peer-identity fixture is restricted
inside it.

Private Weston Wayland/WebKitGTK acceptance passed in
`/tmp/ongleet-settings-native-nnslsn3o/report.json`, using two projects, multiple
Files/PR tabs, fixture GitHub HTTP and a real temporary Secret Service on a
private D-Bus session. It verified global/session/tab inheritance, sorting,
hidden files, stale revisions, reset to inheritance, scoped account isolation,
local account selection, return from a local PR without altering the session
association, and Quit/restart persistence. The final run had no cleanup survivors.
Screenshots confirm shared settings and local PR controls, including 16-pixel
checkboxes. Production frontend assets and the bundled debug GUI were restored.

Reproduce after building native-validation assets and copying the GUI/CLI as
above:

```bash
dbus-run-session -- python3 scripts/check-plugin-settings-native.py \
  --application /path/to/copied/ongleet-gui
```

This does not validate live GitHub permissions/rate limits, physical keyboard
focus, XWayland, macOS or Windows behavior for these new settings.
