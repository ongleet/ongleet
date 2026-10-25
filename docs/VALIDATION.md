# Validation guide

For unexplained native keyboard warnings, launch with
`DEVCENTER_DEBUG=1 mise exec -- npm run tauri -- dev`.
The opt-in stderr trace records GTK key/focus events, elapsed and GDK timestamps,
hardware codes, key values, modifiers, layout group, window focus/visibility and
the GTK `send_event` flag. Key values can reveal typed input. The flag alone
does not establish whether an event came from a physical device. Logging leaves
event propagation unchanged; reproduce the warning and retain adjacent trace lines.

The full terminal compatibility gate remains open. Pass only observed checks; skipped checks and unavailable desktop access are limitations.

## Recorded evidence

Observed host: Ubuntu 26.04.1, GTK 3.24.52, WebKitGTK 2.52.6, ghostty-web 0.4.0. Interactive Wayland Codex run: 0.163.0. X11 runs used XWayland, not a separate Xorg session. Historical test counts describe individual runs, not current coverage totals.

- Builds, WASM parsing, shared-engine reuse, hidden-rendering and input/link adapter checks passed.
- Native Wayland/XWayland checks passed shell typing, arrows, wide characters/emoji, split UTF-8, native clipboard shortcuts, alternate-screen resizing/replay, hidden output, reload with unchanged PID and query-reply suppression.
- Wayland mouse forwarding, pointer selection/copy and private-handler OSC 8 activation passed. XWayland mouse reporting also passed in `096ye_uk`; separate Xorg remains unverified.
- Actual Codex startup, prompt/cursor editing, slash menu, resize/reload and unsent draft preservation passed. No task was submitted.
- Floods produced 350,000 lines across three hidden streams, at most 2,000 scrollback lines and explicit 4 MiB replay-overflow warnings. Tab interactions took about 22–75 ms. Debug WebProcess RSS samples were 371 MiB before, then 383/370/364 MiB; app RSS was about 197/201 MiB. These finite observations do not prove global memory bounds.
- Real PTY tests passed detached/double-fork and app-death cleanup, eager resource release, blocked-I/O closure, replay/ACK validation and containment failure reporting.
- Native lifecycle checks passed hide/reopen with the same process, activation, cancelled/confirmed Quit, socket cleanup and selective fresh-process restart.
- Project/action editing and session/project/application CLI checks passed with isolated metadata, stable sibling PTYs, confirmations, bounds/permissions and unavailable-app handling. Later CLI additions still need native end-to-end acceptance.
- Mixed-plugin checks passed Files navigation and PR fixtures, retained errors/results, visible-only polling and cancellation cleanup. Live GitHub was not contacted.
- Editor/restoration/archive XWayland run `h2rfiwkv` passed fifteen checks without cleanup survivors. The Rust archive suite passed 71 application tests plus CLI/helper suites. Restored-running launch behavior has registry/containment coverage, not this GUI scenario.
- History run `nnyy2t2r` verified saved text beside a fresh PID and no duplicate prefix after reload. This does not test actual power loss.
- Startup run `whys2t3n` verified local missing-directory/executable errors and exit 23 while eleven sibling PTYs survived. The earlier live WebKit busy loop remains unprofiled/unresolved.

Full dated reports, paths, migration evidence and earlier failures: [validation record](reference/VALIDATION.md). Later entries supersede historical claims that Wayland mouse forwarding or tab management were unimplemented.

## Remaining acceptance

- Codex approvals, scrolling, active-task Ctrl+C, actual image attachment, independent conversation resume/fork/hooks/missing history, crash recovery and close/Quit during retry delay.
- Function keys beyond F1/F12, combining graphemes, IME, primary selection, broader multiline bracketed paste/selection and separate Xorg mouse reporting.
- Minimize/restore, separate Xorg, Wayland window-manager close, full session/selection retention and horizontally scrolled-tab pointer access.
- Snapshot graphemes/selection and unavailable frontend behavior; CLI input/snapshots when implemented. Input queue saturation, cancellation and stale runtime IDs need complete boundary coverage.
- Physical editor shortcuts, native editor clipboard and Wayland editor/archive interaction. WebDriver Ctrl-letter remapping required DOM shortcut events in editor checks.
- Sudden power loss, fullscreen/cleared history, live GitHub, clean-system Ubuntu packaging and final resource/directory/state guarantees.

Unsaved editor contents can be lost on crash, WebView reload or destructive CLI operations. Save checks cannot eliminate unrelated external-writer races; ACLs/xattrs are not retained.

## Reproduce native terminal checks

Use an isolated build, never the user's live instance. Record OS, display backend, WebKitGTK, Ghostty and Codex versions.

```bash
mise exec -- npm run build -- --mode native-validation
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline
mise exec -- /usr/bin/python3 scripts/check-native.py --backend wayland
mise exec -- /usr/bin/python3 scripts/check-native.py --backend x11 --skip-mouse --skip-codex
```

Use Ubuntu `webkitgtk-webdriver` or `--driver /path/to/WebKitWebDriver`; matching 2.52.6 was observed. Desktop and writable user cgroup access are required. Preserve `MISE_DATA_DIR` and `MISE_CACHE_DIR` when isolating XDG directories: nested mise otherwise attempts fresh tool installation.

Harnesses use private app data/config/cache and test-owned processes. The bounded diagnostic bridge exists only in `native-validation` bundles. Clipboard checks back up/restore desktop contents or explicitly skip when backup is unsafe. No personal shell/Codex configuration changes occur.

Reports remain under `/tmp/terminal-native-validation-*`. Exit 0 means selected checks passed, 1 means failure, 2 means unresolved blockers. Harness cleanup does not prove general application containment. Restore the production frontend afterward:

```bash
mise exec -- npm run build
```

## Lifecycle, plugins, startup and editor

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-terminal-startup.py --application /path/to/native-validation-app --driver /path/to/WebKitWebDriver
mise exec -- npm run tauri -- build --no-bundle --debug
mise exec -- /usr/bin/python3 scripts/check-file-editor.py \
  --driver /path/to/WebKitWebDriver \
  --application src-tauri/target/debug/devcenter-gui
```

Lifecycle requires X11/XWayland and libX11. For mixed plugins, add `--plugins --application /absolute/copied/devcenter-gui`; optionally add `--projects --actions-editor --cli /absolute/copied/devcenter`. Keep the CLI named `devcenter` beside the copied GUI. Copy the bundled GUI immediately after building; later Cargo builds can replace it with a development-server variant. PR checks use a private `gh` fixture and take about two minutes for real polling intervals. Editor checks accept optional `--cli /absolute/path/to/devcenter` for archive socket checks.

## Manual checklist

1. Exercise shell controls, `café λ 日本語 🙂 é`, multiline selection/paste and `scripts/terminal-probe.py` modes `unicode`, `flood`, `screen`. Check Codex input, approvals, interrupts and resizing without submitting unintended tasks.
2. Switch/fold/reorder projects, sessions and tabs during output. Verify stable PIDs/screens/selection, immediate input focus, keyboard access and nonzero dimensions. Test literal arguments separately from explicit shell mode and environment/directory overrides.
3. Cancel then confirm destructive closures. Verify sibling survival, cgroup cleanup and directory non-mutation. Test invalid executables and renamed/missing directories without recreation.
4. Reload during live/hidden/exited output. Verify no duplicate replies/relaunch, overflow warnings and continuing output. Selection/scroll position are not restored.
5. Hide/reopen: same PIDs. Quit/restart: fresh PIDs only for formerly running tabs. Stopped/closed/exited tabs stay stopped. Verify exclusive writer/socket ownership and persisted selections/configuration.
6. Test YAML reload/errors atomically, editor conflict/dirty guards and archive restore/purge confirmation. Metadata stores explicit environment overrides; terminal history lives separately from SQLite.

Detailed acceptance steps and individual reports remain in the [full record](reference/VALIDATION.md).

## Shared core API and SDK migration (2026-10-23)

The production build, formatting, Rust check and complete Rust suite pass (73 application tests plus CLI, wrapper and helper suites). Ghostty WASM, terminal reuse, hidden rendering, input/link adapters and engine loading pass. Every frontend build checks plugin import boundaries.

Observed native evidence with isolated metadata:

- `terminal-lifecycle-validation-rkvu1rjo`: sixteen mixed-plugin/lifecycle checks pass with no cleanup survivors. The frontend-only SDK example crosses filesystem/process/storage boundaries; structured conflicts, timeouts, output limits, duplicate rejection, explicit cancellation, subscription disposal, reload cleanup and archive restoration pass. PR association replacement joins active work; hidden polling pauses and a full visible 60-second interval is observed. Pending PR work is joined on close and Quit. CLI operations, selective restart and terminal history/reload pass.
- `devcenter-editor-validation-_7rwtfk_`: fifteen editor/archive checks pass with no cleanup survivors, covering Unicode editing, save conflicts, dirty guards, retained drafts, missing-file recovery and restoration. An earlier concurrent run timed out while taking a WebDriver screenshot; the isolated retry completed.
- `terminal-native-validation-68ay2kvg`: twelve Wayland terminal checks pass through the SDK, including native clipboard, mouse/curses, resize, raw-byte replay, hidden floods and terminal reuse. Interactive Codex was explicitly skipped. Two WebKit worker processes were observed at harness cleanup and killed by the harness; no application or PTY process was listed as a survivor.

These runs do not close the full terminal-stack gate. Live GitHub, the native 32-job saturation boundary, interactive Codex, remaining X11/Xorg input cases, IME and power-loss behavior remain unverified. The existing manual checklist still applies. Production frontend assets were restored after native validation.

## Files refresh and overwrite confirmation (2026-10-25)

Production frontend and native debug builds pass. Native WebKitGTK/X11 editor run `devcenter-editor-validation-ufbwwfue` passes fifteen checks with no cleanup survivors: clean-file automatic refresh, retained drafts, cancelled overwrite, renewed confirmation after another external write, confirmed overwrite, and existing editor/archive regression coverage. Refresh pauses for hidden tabs and resumes when visible; focus-triggered refresh and edits arriving during an in-flight read remain unverified by dedicated native scenarios. Physical keyboard shortcut limitations documented above still apply.

## Files inotify notifications (2026-10-25)

Replaced one-second file polling with an attachment-owned Linux inotify parent-directory watch, filtered by basename. Notifications coalesce until read acknowledgement; closing/detaching/removing the owner or quitting stops and joins the worker. Production frontend/native builds, formatting, plugin boundaries and the complete Rust suite pass outside the socket-restricted sandbox. The real-inotify Rust test covers sibling filtering, event coalescing, atomic replacement, deletion/recreation and worker disposal.

Native WebKitGTK/X11 run `devcenter-editor-validation-ca7m4bqt` passes sixteen editor/archive checks with no cleanup survivors. It verifies repeated atomic replacements, deletion/recreation, no filesystem reads during 1.5 seconds idle (using IN_ACCESS with a positive control), retained drafts and revision-bound overwrite confirmation. An earlier run exposed a missing asynchronous Back-navigation wait in the harness; another rejected ineffective instrumentation of Tauri's immutable invoke property. The final harness observes actual filesystem access. Focus-triggered refresh, in-flight editing races, notification queue overflow and the global watch limit have no dedicated native acceptance scenario. Parent-directory relocation requires reopening; remote network-filesystem and mmap writes remain outside inotify coverage.

## Persistent drafts and three-way save conflicts (2026-10-25)

Edits persist synchronously in WebView localStorage with the original text/revision, scoped by tab ID and canonical path. The editor detail bar highlights unsaved changes and local draft availability. Reload/restart and GUI closure retain drafts; save, reverting to the baseline, and explicit Reload/Back discard remove them. Missing source files can recover their draft without recreation. Quota/storage failures retain live edits and show a visible error. Storage clearing and power-loss durability are not guaranteed.

Conflicting saves offer Cancel, Force save and Merge into editor. The line-based three-way merge uses node-diff3 in a disposable Web Worker, with bounded input/matching work and a five-second timeout. It rereads the current disk version, updates the editor and draft baseline without writing, and preserves competing edits plus original context in conflict markers. Concurrent editor changes reject the result; a subsequent disk change is revision-checked again at save. Large or highly repetitive changes can require manual merging or explicit force save.

Production frontend/native builds, plugin boundaries and formatting pass. `npm run check:editor-merge` passes seven behavior tests covering independent/overlapping/identical edits, whitespace/Unicode/EOF preservation, deletion/insertion, empty originals and bounded repetitive work. Native WebKitGTK/X11 report `devcenter-editor-validation-3e9zcmm4` passes 22 checks with no cleanup survivors, including persisted drafts across reload/restart, conflict cancellation/forcing, explicit discard, simulated quota failure and recovery, missing-file draft recovery, native worker merging, marker restoration and resolved save. The prior merge run reached the worker result but failed on a multiline expected-value escaping error in the harness; the final harness passes that value as a WebDriver argument. In-flight typing during merge, worker timeout, simultaneous editors for one file, and power-loss durability have no dedicated native acceptance scenario. Physical keyboard shortcut limitations remain as previously documented.

## Codex activity tags

`scripts/check-codex-activity.py --application /absolute/native-validation/devcenter-gui --cli /absolute/devcenter --driver /path/to/WebKitWebDriver` exercises real contained wrapper processes, generated hook commands, bounded mapping files, the private control socket, scoped SDK tag operations and the WebKitGTK sidebar with temporary XDG metadata. Its local fake Codex parses the injected TOML and emits fixture events; it never contacts a model or reads personal Codex configuration. It checks concurrent tabs and pending waits, Stop/Interrupt, wrapper restart and stale generations, reload retention, manual-tag preservation, closure and SessionEnd. Production frontend assets must be restored after building the native-validation bundle.

The fixture establishes application integration, not hook delivery by actual Codex. Live `request_user_input`, permission approvals, interrupts and continuation behavior remain unverified. A plain-text final question is only distinguishable as Idle. User-configured hooks may continue a stopped turn or resolve a permission request automatically.

Native X11/WebKitGTK report `/tmp/devcenter-codex-activity-6h9_oa3b/report.json` passed the ordinary Codex tag, independent background/dot colors, automatic text contrast, descending priority, unique-name updates and shared CLI/SDK listing/add/removal. It also passed concurrent hook waits, restart generation rejection, reload and closure checks with zero fixture process survivors. The full Rust suite, production frontend build, plugin boundaries and changed-file formatting passed. This uses fixture hook delivery; live Codex remains pending.

## Terminal input and custom window controls (2026-10-25)

Native Wayland run `terminal-native-validation-hkg5__hu` passed sixteen checks;
XWayland run `terminal-native-validation-096ye_uk` passed seventeen. Both observed
Codex 0.164.0 multiline Unicode clipboard paste, Ctrl+C clearing an unsent draft,
prompt editing, slash menus and draft preservation across resize/reload. No task
was submitted: approvals, active-task interruption and Codex scrolling remain
unverified. Both backends passed F1/F12 as ncurses function keys, mouse reporting,
selection/copy, clipboard types, OSC 8 activation, split UTF-8, PTY resize/replay,
hidden floods and tab reuse. Custom maximize/restore followed native window state
on both; XWayland custom close-to-hide/reopen preserved the shell PID. This is
XWayland evidence, not a separate Xorg desktop test.

XWayland run `terminal-native-validation-m7j2uzo6` observed native minimization,
but `--show` did not clear the reported minimized state before timeout. Reproduce
with `scripts/check-native.py --backend x11 --check-minimize` and the driver/build
arguments above. The cause remains unresolved; do not count restoration as passed.
Geometry/maximization persistence across Quit/restart, dragging, rounded transparent
corners, IME, primary selection and broader grapheme/selection coverage remain open.

The harness now uses the Session menu, waits for actual output rather than command
echo, waits for clipboard ownership changes and waits for the actual GUI process
at shutdown. Each successful run still listed one WebKit WebProcess at final
cleanup, which the harness killed; neither listed a surviving GUI or PTY process.
Do not infer general cleanup completion from these runs.

Codex screenshots show blank spacing omitted by the current text snapshot
(`codex-multiline.txt` versus `codex-multiline.png`). Token assertions allow this
known limitation; exact whitespace fidelity remains open before CLI snapshots.
Production frontend assets were restored after validation.

## Minimized-window activation (2026-10-25)

`--show` now presents the native GTK window on the main thread. In the pinned
Tao implementation, unminimize queues a GTK request while set_focus rejects a
window whose cached minimized flag remains true, so consecutive asynchronous
requests can omit the presentation needed for activation.

XWayland run `terminal-native-validation-_wk_hi6i` passed sixteen checks,
including observed native minimization, restoration through `--show`,
close-to-hide/reopen with unchanged shell PID and the terminal regression suite.
No cleanup survivors were observed. Codex was explicitly skipped. This supersedes
the earlier XWayland restore failure for this build.

Wayland runs `tzuis95c` and `m_uuc3ox` timed out waiting for a native minimized
state, before testing restoration; the latter was rerun after asking the user to
leave the desktop untouched. An earlier run `jsulfbdm` failed to observe maximize;
user interaction may have affected that run. Wayland minimize/restore remains
unverified; these results do not establish a restoration regression. Xorg,
window dragging and geometry/maximization persistence across restart remain open.

WebDriver's close-button click endpoint also timed out after hiding the window
in `qvn9_l7t`. The harness now asynchronously activates that DOM button to avoid
waiting for a hidden WebView repaint; native pointer delivery for that close
scenario is therefore not established by the successful run. Production frontend
and native assets were restored after these checks.
