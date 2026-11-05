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
