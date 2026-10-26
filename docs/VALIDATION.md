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

- Codex actual image attachment, independent conversation resume/fork/hooks/missing history, crash recovery and close/Quit during retry delay.
- Japanese/Chinese candidate-selection IMEs and complex grapheme shaping, primary selection, broader multiline bracketed paste/selection and separate Xorg mouse reporting. Basic function-key, combining-input and native Unicode-composition evidence is recorded below.
- Title-bar dragging, rounded transparent corners, separate Xorg/other compositor window behavior, Wayland window-manager close, full session/selection retention and horizontally scrolled-tab pointer access.
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

Lifecycle requires X11/XWayland and libX11. For mixed plugins, add `--plugins --application /absolute/copied/devcenter-gui`; optionally add `--projects --actions-editor --cli /absolute/copied/devcenter`. Keep the CLI named `devcenter` beside the copied GUI. Copy the bundled GUI immediately after building; later Cargo builds can replace it with a development-server variant. PR checks use dummy GraphQL responses and real native credentials. They require a private D-Bus session with an unlocked temporary Secret Service; they refuse the desktop bus. Hidden polling uses real WebKitGTK timers. Editor checks accept optional `--cli /absolute/path/to/devcenter` for archive socket checks.

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

## Wayland activation and backend-aware validation (2026-10-25)

Window activation now belongs to `window_state::activate`; socket dispatch only
requests activation. It uses GTK presentation on both backends. On Wayland, an
inactive visible GTK window is hidden and presented again to remap its surface,
retaining the GTK window, WebView, terminal controllers and Rust PTYs. This also
applies to explicitly showing an inactive, non-minimized window because the
protocol does not expose minimized state. The compositor can choose its placement.
Already-active windows do not remap. No GNOME-specific production API is used.

The earlier test incorrectly expected `is_minimized()` to become true under
Wayland. The [xdg-shell specification](https://raw.githubusercontent.com/wayland-mirror/wayland-protocols/main/stable/xdg-shell/xdg-shell.xml)
provides a minimize request but no minimized-state notification or unminimize
request. GTK activation can also be declined by the compositor. The trace in
`xe2e2513` showed WebDriver's synthetic GTK input yielded activation serial zero.
Even actual compositor input in `5myyd614` did not restore focus with the previous
presentation-only implementation; the corrected test therefore catches the bug.

The Wayland minimize check now verifies an outgoing `xdg_toplevel.set_minimized`,
loss of native focus, restored native focus after `--show`, stable normal/maximized
window dimensions and the same shell PID. It records protocol evidence instead of
claiming to observe compositor minimized state. A temporary GNOME Mutter input
session supplies Enter to the focused minimize button and is stopped after key
release. This harness path needs Python GI and GNOME Mutter RemoteDesktop access;
other Wayland compositors need equivalent native/manual acceptance. It does not
change desktop settings or install a persistent service. Protocol tracing is
scoped to the test driver/application with `DEVCENTER_DEBUG=1`.

Reproduce the focused window checks after a native-validation build:

```bash
mise exec -- /usr/bin/python3 scripts/check-native.py --backend wayland --check-minimize --window-only --skip-codex
mise exec -- /usr/bin/python3 scripts/check-native.py --backend x11 --check-minimize --window-only --skip-codex
```

Supply `--driver PATH` when WebKitWebDriver is not on PATH. Leave the desktop
untouched during these input checks. `--window-only` skips terminal stress checks;
it does not certify the complete terminal compatibility gate.

Wayland full regression run `terminal-native-validation-phyc4qe4` passed eighteen
checks including normal/maximized minimize/reactivation and terminal regression
coverage; Codex was skipped. One WebKit WebProcess was listed at cleanup and
killed by the harness; no GUI or PTY survivor was listed. After moving activation
behind the window lifecycle abstraction, focused Wayland `piwgnsw5` and XWayland
`ibhpnfxw` each passed five checks without cleanup survivors. Earlier XWayland
attempt `mwrkxjcl` stopped at fixture setup because its typed temporary-directory
path contained an extra final character; it exercised no window controls. These
results supersede earlier unresolved minimize/restore entries on this GNOME host.
Other compositors, a separate Xorg session, dragging and restart geometry remain
outside this evidence. Production frontend/native builds were restored afterward.

## Window geometry across Quit/restart (2026-10-25)

Added `--check-window-restart` (requires `--window-only`) to the existing native
harness. It resizes the actual window to 986 × 674, quits the isolated application,
verifies GUI/shell termination and saved geometry, and launches a fresh GUI through
WebDriver using the same private metadata. It then minimizes/reactivates a
maximized window, quits/restarts again and checks maximization, full window size
and the previous normal dimensions when restored. Both restarts assert distinct
GUI and shell PIDs and a visible reopened window. XWayland additionally compares
normal-window position across restart. Screenshots and structured before/after
geometry are retained in each report directory.

Wayland report `terminal-native-validation-dmft_6f6` and XWayland report
`terminal-native-validation-l_5ye7_4` each pass five selected checks with no cleanup
survivors. The normal window reopens at 986 × 674; maximized restart reopens at
1920 × 1080 on this host, then restores to 986 × 674. No production code change was
needed. These checks validate Quit/restart rather than power-loss durability.
They do not validate position on Wayland, separate Xorg, other compositors,
dragging or rounded transparent corners. The full terminal gate remains open.

After building native-validation assets, reproduce with the driver path as needed:

```bash
mise exec -- /usr/bin/python3 scripts/check-native.py --backend wayland --window-only --check-window-restart --skip-codex
mise exec -- /usr/bin/python3 scripts/check-native.py --backend x11 --window-only --check-window-restart --skip-codex
```

Wayland restart checks reuse the GNOME native-input fixture described above. Leave
the test windows untouched during both runs. Production frontend/native assets
were restored after acceptance; Python syntax and diff whitespace checks pass.

## Live Codex approvals, interruption and scrolling (2026-10-25)

Committed window restart acceptance as `a0add47`, then added
`scripts/check-codex-interactive.py` for live terminal acceptance. It launches
Codex 0.164.0 with the configured model (`gpt-6.1-sol` on this host) in a private
workspace and Codex home, read-only sandbox and on-request approvals, without a
shared daemon. It copies file-based authentication temporarily and removes the
entire private Codex home in cleanup. It does not copy or change personal Codex
configuration, hooks or project files. The fixture session is created through the
normal backend operation, then displayed after frontend reload. These are live
model requests using the signed-in account, restricted by the prompts to the
harness fixtures.

Wayland `devcenter-codex-validation-8vlu8kia` and XWayland
`devcenter-codex-validation-kudr4nc5` each passed five selected checks with no
cleanup survivors; private authentication copies were removed. Observed behavior:

- Authenticated interactive Codex starts with isolated metadata.
- An approval prompt blocks the marker command; Escape rejects it and its marker
  remains absent.
- Accepting a later request executes the marker command and completes the turn.
- Ctrl+C during a bounded active command stops its actual subprocess and displays
  the current turn's interruption message.
- Native WebDriver wheel input scrolls a live eighty-line numbered response and
  returns to its latest line. The validation-only bridge reports viewport offset;
  assertions also compare numbered terminal rows because Codex handles scrolling
  inside its alternate screen. Screenshots and text snapshots are retained.

The first exploratory run `8c16ar_l` was invalidated: its assertions could match
historical interruption text or a completion marker in the prompt. The corrected
harness waits for actual process termination and a standalone completion reply.
Wayland `s8uaq6jx` also passed these corrected checks; `gk3b7hf0` stopped during
session-form setup, before Codex started. The final harness uses backend fixture
creation and waits for the long response to settle before inspecting scrolling.
It checks a multi-row change rather than accepting arbitrary screen differences.

After a native-validation build, run with file-based Codex authentication:

```bash
mise exec -- /usr/bin/python3 scripts/check-codex-interactive.py --backend wayland --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-codex-interactive.py --backend x11 --driver /path/to/WebKitWebDriver
```

The default auth source is `CODEX_HOME`, falling back to `~/.codex`; use
`--auth-home PATH` for another file-based profile. Leave the native test window
untouched while running. Approval settings follow the
[official configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).
Physical keyboard/wheel input, actual image attachments, live dcodex activity hook
delivery, resume/fork/crash scenarios and other Codex versions remain outside this
coverage. This does not close the full terminal compatibility gate. Production
frontend/native assets were restored; Python syntax and targeted formatting checks
pass.

## Function keys, Unicode input and native composition (2026-10-25)

Added `scripts/check-keyboard.py` and a temporary compositor-input helper in
`scripts/native_input.py`. A real raw-mode PTY receiver records exact input bytes
independently of terminal snapshots. Literal escape-sequence expectations cover
F1–F12. Combining sequences `é`, `Å`, `ñ` and `👩‍💻` must reach the PTY as exact
UTF-8, including combining marks and the zero-width joiner. A final aggregate
comparison detects unexpected, missing or duplicated input across the sequence.

Composition checks use [GTK Unicode input](https://docs.gtk.org/gtk3/class.IMContextSimple.html)
with the input method selected only for the isolated test process. Real compositor
key symbols enter preedit for `日`, commit it, then enter and cancel `あ`. Assertions
require no raw preedit leakage, exactly one UTF-8 commit, and normal `x` input after
cancellation. Trusted WebKit composition/key events and screenshots are retained.
XWayland and IBus may expose DOM composition only on commit, so cancellation is
verified by native Escape processing and PTY byte outcomes rather than requiring
a DOM compositionend when no DOM composition began. Layout-independent key symbols
preserve the desktop's existing Bépo configuration; settings and engine selection
are never changed. Early invalid artifact directories from a physical-key-position
assumption were deleted. A temporary GNOME Mutter input session releases its keys
and stops on exit; no input service is installed.

Each of these native runs passed four selected checks with no cleanup survivors:

| Backend  | Input method  | Report suffix |
| -------- | ------------- | ------------- |
| Wayland  | GTK simple    | `qobd_zgg`    |
| XWayland | GTK simple    | `tip29iil`    |
| Wayland  | Existing IBus | `k_bs8yhq`    |
| XWayland | Existing IBus | `nipeyhpb`    |

Reports are under `/tmp/devcenter-keyboard-validation-<suffix>`. The existing IBus
engine is `xkb:us::eng`; only XKB engines were available. The helper does not install
or select Japanese/Chinese conversion engines, so candidate selection, conversion,
reconversion and candidate-popup placement remain untested. Test keyboard events
are supplied by WebDriver or the compositor input API, not physical hardware.

Screenshot inspection shows combining accents on the fixed PTY output fixture.
The ZWJ emoji renders as separate woman/laptop symbols even though the input bytes
are correct; complex grapheme shaping remains a known limitation. Text snapshots
also omit combining/joiner information, so byte acceptance does not establish
snapshot fidelity. These findings remain in TODO and do not close the full
terminal compatibility gate. No production input behavior was changed.

After building native-validation assets, reproduce with the appropriate driver:

```bash
mise exec -- /usr/bin/python3 scripts/check-keyboard.py --backend wayland --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-keyboard.py --backend x11 --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-keyboard.py --backend wayland --input-method ibus --driver /path/to/WebKitWebDriver
mise exec -- /usr/bin/python3 scripts/check-keyboard.py --backend x11 --input-method ibus --driver /path/to/WebKitWebDriver
```

The harness needs Python GI and GNOME Mutter RemoteDesktop access; IBus runs need
the existing user's IBus service. Leave the desktop untouched during these checks.
Native traces use `DEVCENTER_DEBUG=1` and may include typed input. Production
frontend/native assets were restored; Python syntax and diff whitespace checks pass.

## Unicode selection, paste bounds and primary selection (2026-10-26)

The terminal now serializes selected text from the existing Ghostty cells and
absolute selection endpoints. Wide continuation cells contribute no extra spaces;
combining graphemes use Ghostty's grapheme lookup, including in scrollback.
Automatic/browser copy and native copy share this serialization. This compatibility
adapter uses the pinned ghostty-web 0.4.0 selection manager because its public
selection range is clipped to the visible viewport; revalidate it on upgrades.

Fixed a second upstream 0.4.0 issue: selection auto-scroll extended an endpoint at
the screen edge even when the viewport could not move. The adapter prevents edge
scrolling at boundaries and stops an active timer after it reaches one. It retains
Ghostty's selection/mouse machinery and restores patched methods on disposal.
A focus click with no drag no longer copies a single cell over existing selections.

Selections populate both CLIPBOARD and PRIMARY through the SDK/native services.
Middle-click uses PRIMARY text and Ghostty's paste framing. The mouse adapter runs
first: application mouse-reporting mode receives middle-button reports instead of
pasted text. `desktop.clipboard(selection)` and `writeText(text, selection)` default
to CLIPBOARD, preserving existing plugin callers. Primary text writes and terminal
text reads retain the 65,500-byte limit and a three-second GTK request timeout.
Standard clipboard image/file classification remains available.

### Isolation incident and limits

An early live-desktop run (`7qqda88_`) failed rich-text clipboard restoration and
left fixture text on the user's clipboard. Its original rich-text backup could
not be recovered. The user was informed; that run is failed evidence. Subsequent
clipboard validation moved to a private authenticated Xvfb server containing only
known fixtures. The final harness does not support live desktop clipboard mutation.
It removes Wayland connection variables, uses a private DISPLAY/Xauthority and
private application data, and seeds its own HTML/text and primary selections.
The restoration helper is restricted to that private X11 context and serves all
fixture data formats from memory. No system package was installed; Xvfb was
extracted under `/tmp/terminal-native-tools` for these runs.

Separate metadata does not prevent shared-desktop effects. The user's live GUI
and Vite server were present during development, so source edits could hot-reload
the live frontend. No input or shutdown command was intentionally sent to that
GUI. Use a separate source/build copy or stop the live development session before
future native acceptance if hot reload must be excluded. The harness never stops
unrelated instances. Restart a development instance when needed to load matching
frontend and Rust command changes.

### Observed checks

Final WebKitGTK private-X11 report `devcenter-clipboard-validation-ag2ehhy2` passes
twelve checks. Its report records the private display/data path and disabled
Wayland connection. Both original fixture selections were restored, and no marked
cleanup survivors were observed. This is Xvfb integration evidence, not live
Wayland, XWayland or a separate Xorg desktop acceptance.

- GTK's observed CRLF-to-LF conversion precedes multiline Unicode paste. Bracketed
  mode adds exactly one delimiter pair; unbracketed mode adds no explicit Enter.
- Native image paste forwards Ctrl+V; copied local file URIs become quoted paths
  with spaces/apostrophes preserved, without executing a shell.
- A 65,500-byte Unicode clipboard crosses the native write chunks intact;
  oversized text is rejected visibly without partially writing to the PTY.
- A focus click preserves both selections. Forward/reverse multiline selection
  preserves combining marks and wide characters. A narrow CJK/emoji selection near
  the top edge remains stable at mouse release and owns PRIMARY.
- Middle-click reads PRIMARY independently of CLIPBOARD. With mouse reporting
  enabled it produces middle-button reports and no primary text paste.

`check:terminal-selection` uses real Ghostty WASM cells for wide boundaries,
leading spaces, combining text, reversed endpoints and selection spanning
scrollback/live rows. `check:terminal-input` and the Rust clipboard file-quoting
regression test pass. Python syntax and targeted formatting checks pass.
Temporary selection mutation-stack instrumentation was removed.

Build native-validation assets, then reproduce without touching desktop selections:

```bash
mise exec -- npm run check:terminal-selection
mise exec -- /usr/bin/python3 scripts/check-clipboard.py --xvfb /path/to/Xvfb --driver /path/to/WebKitWebDriver
```

The script needs Xvfb, xclip and GTK 3. `scripts/check-native.py` now skips desktop
clipboard mutation by default; it points to the private-display harness instead.
The full terminal compatibility gate remains open. Live compositor primary
selection, physical input, soft wrapping and broader retention still need safe
acceptance. Production frontend/native assets were restored after validation.

## Plugin-owned GitHub authentication (2026-10-26)

The PR plugin now uses native HTTPS and plugin-scoped Secret Service credentials.
`scripts/check-pr-auth.py --application /absolute/native-validation/devcenter-gui
--driver /path/to/WebKitWebDriver` exercises the WebKitGTK UI against dummy
GraphQL responses. Credentials cross the real native boundary. It refuses to
write credentials unless the caller supplies a private `/tmp/` D-Bus address and
temporary XDG data directory. Never use real GitHub tokens in fixture mode.

Build/copy the native-validation app using the commands above. Run the harness
on a private X11 display under `dbus-run-session`, with temporary
`XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_CACHE_HOME`. Preserve `MISE_DATA_DIR`
and `MISE_CACHE_DIR` before overriding XDG paths. Within that bus, start
`gnome-keyring-daemon --unlock --components=secrets --control-directory=/tmp/your-private-control`
with a fixture password on stdin; the control directory must have mode 0700.
The service must stay alive for the complete UI restart test. The ignored Rust
`isolated_secret_service_persists_and_isolates_plugin_records` test runs on the
same private bus **after** UI checks: it intentionally locks the collection.
It verifies reconnect persistence, replacement, deletion, plugin isolation and
locked-storage rejection, without touching the desktop keyring.

The native harness covers capability and attachment ownership checks, insecure
URL rejection, four HTTP jobs per attachment, 32 globally, cancellation during
stalled TLS I/O, shared connection, invalid replacement, retained snapshots after
network failure, hidden polling, disconnect and native Quit/restart. GitHub
responses are fixtures; this does not establish live public/private repository
access, token permissions under organization policies, or live GitHub rate-limit
behavior. The TypeScript behavior suite covers authentication failures, storage
failure, rate-limit/reset headers, account changes, stale responses, review
cursors, independent pagination and overflow rejection. Rust tests cover request
and response bounds and transport cancellation. Live GitHub acceptance remains
pending.

Native X11/Xvfb report `/tmp/devcenter-pr-auth-9tmhgn3h/report.json` passes
six checks with zero surviving test processes. The separate private Secret
Service Rust test passes, including locked collection rejection. Fixture tokens
are absent from application metadata/configuration files. The production
frontend build (restored after native validation), plugin boundaries, 13 PR
behavior tests, Rust formatting/check and complete Rust suite pass. HTTP body
bounds are tested with and without Content-Length. Wayland authentication UI,
live GitHub public/private access and real rate-limit acceptance remain untested.

## Launch-environment authentication (2026-10-26)

`scripts/check-pr-auth.py --environment-token` uses dummy `GH_TOKEN` and
`GITHUB_TOKEN` values inherited by the native process. The same private D-Bus,
Secret Service and display requirements apply. Normal fixture runs explicitly
remove inherited GitHub tokens before launching the app.

Native X11/WebKitGTK report `/tmp/devcenter-pr-auth-w6e3bu9h/report.json` passes
two checks with zero survivors: arbitrary SDK environment reads and capability
enforcement, GH_TOKEN precedence, no automatic credential persistence,
disconnect/reconnect, and app restart while preserving an independently saved
credential. Sixteen PR behavior checks pass, including GITHUB_TOKEN fallback,
empty/unset variables, invalid environment tokens, and stale validation after
tab disposal. Two Rust environment tests cover missing/empty values, name and
size bounds, and non-UTF-8 rejection. Production frontend build and Rust check
pass. Live GitHub token acceptance and Wayland environment UI remain untested.
