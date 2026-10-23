# Validation guide

The full terminal compatibility gate remains open. Pass only observed checks; skipped checks and unavailable desktop access are limitations.

## Recorded evidence

Observed host: Ubuntu 26.04.1, GTK 3.24.52, WebKitGTK 2.52.6, ghostty-web 0.4.0. Interactive Wayland Codex run: 0.163.0. X11 runs used XWayland, not a separate Xorg session. Historical test counts describe individual runs, not current coverage totals.

- Builds, WASM parsing, shared-engine reuse, hidden-rendering and input/link adapter checks passed.
- Native Wayland/XWayland checks passed shell typing, arrows, wide characters/emoji, split UTF-8, native clipboard shortcuts, alternate-screen resizing/replay, hidden output, reload with unchanged PID and query-reply suppression.
- Wayland mouse forwarding, pointer selection/copy and private-handler OSC 8 activation passed. X11 mouse reporting remains unverified.
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

- Codex multiline paste, approvals, scrolling, Ctrl+C, actual image attachment, independent conversation resume/fork/hooks/missing history, crash recovery and close/Quit during retry delay.
- Function keys, combining graphemes, IME, primary selection, multiline bracketed paste, broader selection and X11 mouse reporting.
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
