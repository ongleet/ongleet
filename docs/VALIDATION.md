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
| Lifecycle        | Close and Quit now share immediate confirmed shutdown. Eight private X11 checks passed, including PTY/detached-child cleanup and cancellation (`i363vr68`). Earlier delayed-close evidence is superseded. Broader mixed-plugin lifecycle acceptance remains incomplete.                            |
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

Use `--comment-index-only` to seed 160 file/old/new-line discussions across forty
patches and repeat eight filename-filter updates. The isolated WebKitGTK baseline
(`wvreq39p`, before indexing) serialized 336 per-file discussion snapshots; the
indexed build (`tlpquzxx`) serialized zero. Both recorded approximately 32 ms per
interaction using two animation frames, so this scenario demonstrates removed
serialization work, not a measured input-latency gain. Profiles are saved as
`discussion-profile.json` in the reported artifact directory. The same fixture
checks replies, edits, deletion and retained drafts across files and threads on
one line. The general Diff suite (`7pt_cst1`, 25 checks) and discussion suite
(`5c1atojg`, ten checks) also passed. All three acceptance runs left no cleanup
survivors. Behavioral tests cover stable revisions, renames, orphan transitions,
side-specific lookups and merged context-row order.

Use `--rendering-only` for a 42-file expanded comparison: it checks native
offscreen rendering skips, distant sidebar navigation and selectable patch text.
On 2026-11-16 this scenario and the general Diff suite passed on private X11
WebKitGTK (`9yctl2kl`, `giw1p9hk`), with no cleanup survivors. This establishes
rendering behavior, not a measured latency or CPU improvement. Native discussion
checks also passed (`kf53i86v`) with no cleanup survivors.

On 2026-11-16, `--syntax-only` passed six native WebKitGTK checks (`bibwgzwg`):
plain rendering before worker delivery, old/new syntax, live themes, cancellation
across two pending refreshes, reload and unchanged Git index. No cleanup survivors.

`mise exec -- node scripts/profile-diff-syntax.mjs` compares synchronous parsing
against the production worker using Node threads and a 5 ms timer. For twenty
400-line TypeScript patches, one run measured a maximum timer gap of 176 ms
synchronously versus 7 ms with the worker; completion took 171 ms versus 296 ms.
This isolates parser/transport work, including cold worker startup, and does not
measure WebKit rendering, input latency or total application CPU. The worker
client tests cover real-thread tokens, shared requests, cancellation, stale
replies, cache eviction, startup/crash fallback and timeout cleanup. General Diff (25 checks,
`9zgevz7z`) and 42-file rendering (five checks, `y0poza03`) also passed without
cleanup survivors.

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
its environment. See [crash investigation](USAGE.md#investigating-native-crashes)
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

## Diff sidebar reveal (2026-11-10)

A private Xvfb/WebKitGTK run of `check-diff-native.py` passed sidebar row reveal
in both directions, collapsed-ancestor expansion, unchanged viewer scroll,
sticky headers, tab/session navigation and refresh anchoring. The long-line
fixture now explicitly selects `hello.txt` rather than the first folder-sorted
file. The run later timed out waiting for the invalid-reference alert; the
complete native suite did not pass. Production build and Diff engine checks
passed. The 180 ms sidebar auto-scroll debounce and paint containment still need
a desktop scrolling/profile comparison; navigation checks do not establish
performance improvement.

## On-demand terminals (2026-11-10)

With the production frontend embedded in the debug GUI, a private Xvfb/WebKitGTK
run passed six on-demand checks: no initial emulator/process for a hidden lazy
tab; discovery/invalid arguments do not wake it; concurrent API input starts one
PTY without selecting it or interleaving messages; WebView reload reattaches a
live hidden PTY; closed tabs reject wake-up; first visibility starts a deferred
terminal; immediate API calls after tab creation survive listener installation
races. The fixture reported zero cleanup survivors at
`/tmp/ongleet-diff-native-d4q2ufrr/report.json`.

```bash
mise exec -- /usr/bin/python3 scripts/check-diff-native.py --demand-only \
  --application src-tauri/target/debug/ongleet-gui \
  --cli src-tauri/target/debug/ongleet --driver /usr/bin/WebKitWebDriver
```

Rust checks cover wake reservation limits, attachment binding, close/reload
cancellation, schema validation and persistence without restoring runtime state.
The full Rust suite passed with two existing ignored tests; the added
on-demand persistence check also passed separately. Frontend build, i18n,
terminal reuse/history/input checks passed. This does not certify physical
Wayland input, macOS/Windows startup or performance across many deferred tabs.

## Diff sidebar automatic reveal disabled (2026-11-10)

Profile `localhost-recording-10.json` still sampled the native sidebar `scrollTo`
operation heavily despite the debounce. Automatic reveal is now disabled;
selection highlighting and explicit filename navigation remain. A private
Xvfb/WebKitGTK `check-diff-native.py --navigation-only` run passed 11 checks,
including highlighting in both directions without moving the sidebar or opening
folders, sticky headers, tab/session navigation, refresh anchoring and wrapping.
Artifact: `/tmp/ongleet-diff-native-zrmdt41l/report.json`; zero cleanup survivors.
Frontend build and Diff checks passed. Desktop scrolling performance remains to
be confirmed; this focused run does not certify the entire Diff suite.

## Unified close and Quit (2026-11-10)

The title-bar cross, application Quit and native window-manager close use the
same confirmation, plugin flush and immediate shutdown path. The hidden-window
timer and `close_after_delay` command have been removed. Cancelling preserves the
window and running terminals; confirming cleans up the contained process tree
before exiting. Geometry and restart eligibility retain the existing Quit path.

A private Xvfb/WebKitGTK run of `check-window-close.py` passed eight checks,
including equal confirmation text, cancellation, fresh-process restoration,
immediate exit through all three entry points, detached-child cleanup and shell
launcher detachment. Report: `/tmp/ongleet-window-close-i363vr68/report.json`;
no forced cleanup and no surviving PIDs. The full Rust suite passed with two
existing ignored tests; frontend build and i18n checks passed. Mixed-plugin
lifecycle and branding harnesses were updated but not rerun in full. Physical
Wayland and macOS/Windows close behavior remain untested for this change.

Build the bundled GUI after Rust tests and before native checks: a test build
can replace the target executable with one expecting a development server.

## Render diagnostic activation (2026-11-11)

The installed GUI had `ONGLEET_DEBUG=1`, but Diff lacked the SDK `env` capability,
so its environment read was rejected and a swallowed error disabled logging.
That declaration is now present; initialization errors warn, and committed-render
tracking includes App and SessionPane. Profile 12 contains scrolling, no keyboard
or input events; its remaining React/terminal work does not establish the cause
of textarea latency.

A private WebKitGTK `check-diff-native.py --render-logs-only` run verified the
actual SDK gate, enabled marker, textarea input events, Composer/Diff/parent
render records and absence of typed marker text/prop values in the output.
Report: `/tmp/ongleet-diff-native-8a_brpq1/report.json`. Build and Diff checks
passed. These logs provide attribution data; they do not certify a performance
fix or native typing latency.

## OSC compatibility (2026-11-11)

The pinned Ghostty engine rejected OSC 10/11 color queries without replies and
logged unsupported VTE shell (666) and Codex program-status (7501) notifications.
A bounded frontend adapter now answers the exact `10;?` and `11;?` queries,
consumes those notifications, and preserves other terminal bytes and warnings.
Engine checks cover every two-chunk split, byte-wise UTF-8/control input, BEL/ST,
current colors, cancellation, string isolation, unknown warnings and overflow.

Three private X11 WebKitGTK checks passed via `scripts/check-diff-native.py
--osc-only`: exact replies through a real PTY without known warnings, hidden-tab
replies, and WebView reattachment without duplicate historical replies. Report:
`/tmp/ongleet-diff-native-orj9ool_/report.json`; no cleanup survivors. The frontend
build, terminal-input suite and WASM suite passed. These checks do not measure
Diff typing performance or certify other OSC color operations/physical Wayland.

## Diff component split (2026-11-16)

The Diff tab now composes UI from `components/` and retains its comparison,
discussion, scroll and interaction state in `hooks/`. Production frontend/native
builds, Prettier, Diff model checks and localization checks pass.

Private Xvfb/WebKitGTK navigation passed 11 checks, including sidebar resizing,
tab/session navigation, refresh anchoring and retained unchanged patch DOM.
Discussion acceptance passed 10 checks, including inline UI creation, CLI replies,
stale revision rejection, acknowledged PTY delivery, persistence and cleanup.
Reports: `/tmp/ongleet-diff-native-_ctvaam6/report.json` and
`/tmp/ongleet-diff-native-d1prvsz9/report.json`; both report zero cleanup survivors.
The harness now explicitly opens folders before testing collapse and uses a
short-line fixture for compact discussion-row checks; general checks retain
the long-line wrapping fixture.

Follow-up fixture repairs select refs mode before entering an invalid revision,
verify that restart restores the last successful branch configuration, and open
Appearance through Configure before saving a theme draft. An unsaved theme must
leave the rendered syntax unchanged; saving updates colors and survives reload.
The general Diff run now passes all 25 checks, including stale-result retention,
restart, native Git cancellation/reaping and sibling PTY preservation. Syntax
acceptance passes all four checks, including theme persistence and unchanged Git
index contents. Reports: `/tmp/ongleet-diff-native-8_ilsb9n/report.json` and
`/tmp/ongleet-diff-native-s2dxs85f/report.json`; both report zero cleanup survivors.
These results resolve the earlier invalid-reference and Appearance-selector
failures. They do not establish live desktop performance, physical input
acceptance, or a pass of every optional native scenario.

## Codex counted activity summary (2026-11-19)

An isolated Xvfb/WebKitGTK run of `scripts/check-codex-activity.py` passed
with real wrapper processes, hook commands and the private control socket.
It covers simultaneous question/work states, two working then two idle tabs,
restart rejection of stale hooks, reload, closing one tab, stopped counts and
removing the summary after the last tab closes. The Codex executable is a local
fixture; this does not certify live Codex hook delivery or physical Wayland hover.
Rendered counters and the mixed-state screenshot were also checked.
Artifacts: `/tmp/ong-codex-activity-1bt7qwlg`.

Frontend build, i18n and all 15 session/project/tag Rust tests passed. The broader
GUI Rust suite had 138 passing tests, two ignored and one existing archive
fixture failure (`duplicate column name: description`).

## Agent queue validation (2026-11-19)

`npm run check:selection-send` covers captured payload bounds, the last idle
preflight, FIFO ordering, one work/idle cycle per acknowledged write, independent
agents/sessions, editing/removal, explicit sends, uncertain writes, persisted
restart pauses, storage failures/limits and sender handoff. Diff and Notes model
checks also pass.

An isolated Chromium fixture exercised the shared selector and panel: Files and
Notes displayed the same two-message queue, editing paused delivery and propagated
the changed payload to both views, resuming sent the edited first message, repeated
idle notifications held the second, and a work/idle cycle sent the second. The
other recipient remained independent. This used fake activity and write adapters;
it does not validate native WebKitGTK controls, live Codex hook timing, or native
restart/closure behavior. Those acceptance checks remain open.

## Two tab groups (2026-11-19)

The isolated Xvfb/WebKitGTK lifecycle run with `--split` passed twelve checks:
`/tmp/terminal-lifecycle-validation-8mtyfm_0/report.json`. Split-specific evidence
covers context-menu division, two visible terminal panels, retained panel identity,
orientation changes, moving the last group tab, explicit joining, keyboard divider
resizing, right-edge drag-event creation, bottom-edge preview and cancelled drag.
These are browser-dispatched drag events, not physical pointer dragging. The run
also checks Quit, cancellation, native close, single-instance activation, history
and restart. Cleanup reported no surviving processes.

Rust tests cover selection independence, SQLite layout restoration, invalid
layout rejection and new tabs following the active group without durable storage.
The full host run passed 193 tests with two ignored and one excluded pre-existing
archive fixture failure: `restores_legacy_codex_archive_with_new_wrapper_and_original_association`
reports `duplicate column name: description`. Frontend build, formatting, i18n,
terminal reuse and rendering checks passed. Physical Wayland dragging, pointer
resizing and editor draft preservation during group moves remain unverified.

Reproduce with native-validation assets and a bundled GUI, then run:

```bash
mise exec -- xvfb-run -a python3 scripts/check-lifecycle.py --split --driver /usr/bin/WebKitWebDriver --application "$PWD/src-tauri/target/debug/ongleet-gui"
```

Run the bundled GUI build after Cargo tests: tests can replace the GUI executable
with a development binary that expects a Vite server.
