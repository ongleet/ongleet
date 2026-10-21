# Native validation gate

Do not mark a check passed without observing it. Record the OS, display backend, WebKitGTK version, ghostty-web version, and Codex version. Run on Wayland and on X11 where available. `GDK_BACKEND=wayland` or `GDK_BACKEND=x11` can select the GTK backend when the corresponding display is accessible.

## Current evidence

Host: Ubuntu 26.04.1; GTK 3.24.52; WebKitGTK 2.52.6; Wayland desktop with XWayland available; ghostty-web 0.4.0; interactive Codex UI 0.163.0 in the observed Wayland run.

| Check | Result |
| --- | --- |
| TypeScript check + Vite production build including WASM | Passed |
| Ghostty WASM through production byte adapter | Passed |
| 100,000 output lines and configured scrollback bound, engine only | Passed |
| Native Rust compilation | Passed (`cargo check` and `cargo build`) |
| Native PTY, workspace, tab and SQLite tests | Passed: thirty-two Rust tests plus one helper integration check, including workgroup v1→v2 migration, root defaults/overrides, membership/fold persistence, scoped YAML isolation, directory non-mutation, live PTY preservation, cgroup detached/double-fork cleanup, app-death cleanup, eager PTY release without status polling, blocked input/output closure, and PTY reconnect/replay, stale ACK rejection, replay backpressure/budget/EOF, YAML validation/reload, independent template openings, reopen, resume flags, retained definitions, missing directories, failed writes, transaction rollback, exclusive lock and future-schema refusal. Login-shell test passes outside the sandbox. |
| Hidden output and drawing adapter | Passed against independent real WASM states with an instrumented drawing boundary |
| Native Tauri/WebKitGTK automation | Observed outside the sandbox on Wayland and X11/XWayland, using isolated temporary metadata/configuration |
| Interactive Codex in the Tauri WebKitGTK window | Wayland: startup, prompt/cursor editing, slash menu, resize/reload and unsent draft preservation passed; no task submitted. Multiline paste, approval prompts, scrolling and interrupt behavior remain manual |
| Native keyboard, Unicode and clipboard | Shell typing, arrows, wide characters/emoji, deliberately split UTF-8 and native Ctrl+Shift+C/V passed on Wayland and X11. Pointer selection, IME, primary selection and broader key coverage remain pending |
| Native curses and resizing | Actual PTY dimensions, alternate-screen replay and return to normal passed on Wayland and X11 |
| Native live output and reuse | Repeated hidden-stream reloads, unchanged shell PID, suppression of replayed DSR replies, exited output replay, ten tab create/close cycles and clean post-stress screen passed on Wayland and X11 |
| Mouse reporting | Blocked: Wayland curses received no click reports. ghostty-web 0.4.0 browser handlers implement selection but no PTY mouse-report forwarding was found. X11 pointer automation timed out; subsequent X11 run explicitly skipped it |
| Shared WASM reuse | Reproduced closed-tab cell leakage. Explicit RIS after initial fitting and immediate canvas redraw fixes the observed case; thirty shared-engine cycles and native post-flood checks pass |
| Clipboard user test | Native plugin fix confirmed working by the user |
| Workspace sidebar, terminal tabs and persistence | Implemented: durable definitions/openings and selective command restart; native acceptance pending |
| Hidden-window lifecycle | Native X11/XWayland close/hide, continued output with same PID, --show, second-launch activation, explicit Quit, socket cleanup and fresh-process restart passed. Wayland terminal regressions pass separately; native Wayland window-manager close remains manual |
| Full Unix-socket CLI | Pending; only bounded private single-instance show activation is implemented |

The earlier desktop-access failures were sandbox limitations. Current native observations are from 2026-10-21. The terminal validation gate remains open because mouse reporting is blocked and the remaining interactive checks are incomplete.

## Interactive procedure

1. Run `mise exec -- npm run tauri -- dev` from a desktop terminal. Start a shell in an existing directory. Type text, arrows, Backspace, Delete, Home/End, Ctrl+A/E, Ctrl+C, Ctrl+D, and Ctrl+L. Check key behavior without accidentally intercepting terminal controls in the application.
2. Print `café λ 日本語 🙂 é`. Select across lines, copy using Ctrl+Shift+C, and paste using Ctrl+Shift+V. Test primary selection/middle-click if expected on the desktop. Check IME/composition input where available.
3. Add a YAML tab type for the Python fixture: executable `/usr/bin/python3`, arguments `["/absolute/path/to/scripts/terminal-probe.py", "unicode"]`. Check the deliberately split bytes produce intact text. Repeat with `flood`; interact with the window during output, scroll afterward, and inspect process memory while repeating it. Engine-only tests do not prove bounded total WebView memory or responsiveness.
4. Run the same fixture with `screen`. Check alternate screen, arrow and function keys, scrolling/mouse reports, and resize handling. Press q and verify restoration of the normal screen. Test selection with mouse reporting enabled and document the modifier behavior.
5. Choose Codex from New tab, or type `codex` in the shell. Check prompt editing, Unicode, multiline paste, menus, scrolling, Ctrl+C, approval prompts, full-screen/alternate-screen behavior where used, and resizing while Codex is active. Do not change shell/Codex configuration automatically.
6. Resize repeatedly and minimize/restore the window. A hidden window must never trigger a zero-size PTY resize. Confirm the terminal state remains intact. Close the window and reopen by launching the same binary or using --show. Verify the same shell PID, screen and continuing output. Quit explicitly from the footer when finished.
7. Close a running session, first cancelling and then confirming. Verify ordinary shell jobs terminate. Repeat with setsid/double-fork children and check the terminal cgroup becomes empty. Lifecycle containment does not include deliberate cgroup migration or externally delegated services.
8. When a snapshot consumer is available, inspect screen snapshots during normal and alternate-screen use. The inspection button has been removed; the frontend adapter remains. Compare plain text with the display, including wide characters and combining marks. Close the session and confirm screen inspection becomes unavailable. No CLI snapshot support exists yet.

## Sidebar acceptance

1. Create two workspaces using existing directories, including a path with spaces. Use the native picker and manually entered paths. Reject blank names and missing directories without creating anything on disk.
2. Start a shell or command in both. Print distinct markers, run a continuously emitting command in one, and switch repeatedly. Verify the original process and screen remain, hidden output appears on return, and dimensions remain valid.
3. Edit names from each workspace's vertical menu, drag the grips to reorder, and test Alt+Up/Down on a focused grip. Verify selection stays associated with the same workspace ID and the bottom sidebar text is absent.
4. Update a base directory while its shell is running. Confirm its existing working directory is unchanged; close/restart its terminal and verify the new launch uses the updated base directory unless explicitly overridden.
5. Cancel closure of a workspace containing a session, then confirm it. Verify other workspaces keep running and the closed workspace's directory and contents remain intact. Inherited detached descendants must also terminate.
6. Verify stopped workspaces close without a process confirmation, closing the selected workspace selects a neighbor, and closing the last workspace shows the creation screen.
7. Restart and verify saved workspaces, definitions, ordering and selection return. Window close hides without terminating processes; explicit Quit confirms running processes. Previously running commands start anew after an actual application restart.

Remaining interactive checks and Linux environment coverage must be completed before claiming version-one compatibility.

## Terminal-tab acceptance

1. Create several tabs in one workspace, then another workspace with different tabs. Start a shell, Codex, and a continuously emitting command independently. Switch tabs/workspaces and verify each process, screen, scrollback, and selected tab are preserved without zero-size resizes.
2. Close tabs with their crosses; renaming controls are temporarily hidden. Drag running tabs before/after one another; verify original processes/screens remain. Test both directions, cancellation, same-position drops, and ordering with closed definitions still saved. Use arrows/Home/End for selection and Alt+Left/Right for reordering; test names long enough to require horizontal scrolling.
3. Launch an executable with literal arguments containing shell operators. Confirm no shell evaluation occurs. Then explicitly choose Shell command and verify shell syntax works. Test independent environment overrides in two tabs without saving the inherited environment.
4. Launch tabs with and without directory overrides. Change the workspace directory and verify existing processes remain unchanged, future launches without overrides follow the new base, and explicit overrides stay fixed. Try a missing launch directory and invalid executable.
5. Cancel and then confirm closing a running tab. Verify siblings stay running. Cancel closing a workspace with multiple sessions and confirm none stop; then confirm closure and verify all its sessions stop. Verify inherited detached descendants also terminate.
6. Let a process exit with a nonzero code; verify its tab shows the code and its output stays available. Close it and use New tab for an explicit new launch. Close the last tab and create a replacement from the empty state.
7. Verify native clipboard, selection, mouse reporting, and alternate-screen restoration after repeatedly switching between tabs. Compare hidden-output flood responsiveness and memory to the single-terminal baseline.

## Persistence and relaunch acceptance

1. Save several workspaces and definitions, reorder/select them, close one tab, and restart. Verify metadata survives and closed metadata stays closed and New tab offers the YAML types instead of a reopening menu.
2. Leave one command running, explicitly stop another, let a third exit, and close a fourth tab. Restart: only the formerly running command should launch automatically, once, with a new PID and fresh terminal state. Stopped open tabs remain stopped; closed definitions remain available without running.
3. Close the window while several tabs run, reopen it and verify the same PIDs and screens. Cancel Quit and verify all sessions remain, then confirm Quit and verify termination. Relaunch the application: eligible commands start as new processes.
4. With the app closed, remove/rename a workspace directory. Verify restoration keeps the workspace, launch reports an error, and no directory is recreated. Correct the path and start explicitly.
5. Verify a second application cannot write the same database or duplicate startup launches. It must focus/reopen the existing window; concurrent startup must not duplicate sessions.
6. Inspect the standard app-data database and its permissions. Verify no inherited environment or terminal output was saved, and that new tabs persist their own copy of the YAML launch configuration.

## YAML tab types acceptance

1. Open New tab and launch Bash and Codex. Verify independent processes start in the workspace directory and multiple tabs of the same type can run.
2. Add a type with literal arguments, environment overrides and a directory override. Reopen the picker without restarting; verify the new type and launch configuration. Existing tabs must remain unchanged.
3. Try malformed YAML, duplicate IDs, unknown fields/kinds and conflicting shell/executable modes. Verify a clear error without replacing the file or launching a tab. Fix it and retry.
4. Close a running tab with confirmation; verify no configured-tab reopening selector remains. Restart and verify the existing resume policy is preserved.

## WebView reload acceptance

1. Run Bash and Codex, note shell PIDs and print distinct Unicode markers. Reload the WebView (not the whole Rust application). Verify the same processes remain, screens return without the existing-session error, and input/copy/paste/resize still work.
2. Reload during continuous output and repeatedly with hidden workspaces/tabs. Verify output resumes once, siblings stay independent, and no terminal query replies or commands are sent during replay.
3. Let a command exit with a nonzero code and reload. Verify output and exit code return without relaunching it. Legacy stopped configurations must remain stopped.
4. Exceed the 4 MiB journal budget, then reload. Expect an explicit previous-screen-unavailable warning and continuing process/new output. No silently truncated byte history may be replayed. A full-screen program may need to redraw/reinitialize its modes.
5. Resize while producing output and before reload; inspect normal/alternate screens. Selection and scroll position are not restored. Compare memory during repeated reloads and floods. The engine replay check now covers a reused WASM instance after flood/free as well as a fresh reload instance, with split Unicode, normal/alternate buffers, modes and resize events. Native evidence is recorded separately above.

## Reproduce the native checks

Use an isolated bundled build rather than the user's running dev window:

```bash
mise exec -- npm run build -- --mode native-validation
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline
mise exec -- /usr/bin/python3 scripts/check-native.py --backend wayland
mise exec -- /usr/bin/python3 scripts/check-native.py --backend x11 --skip-mouse --skip-codex
```

Install Ubuntu's `webkitgtk-webdriver` package or supply `--driver /path/to/WebKitWebDriver`. The observed run used matching version 2.52.6, downloaded/extracted under `/tmp` without a system installation. Python uses only its standard library. Run outside a restrictive sandbox with access to the desktop. Wayland uses `/run/user/<uid>`; X11 was tested through XWayland on the same Wayland desktop, not a separate Xorg session.

The harness creates private temporary app data/config/cache, uses the real Tauri application and IPC, drives native WebDriver keyboard/pointer input, and reads bounded snapshots from the existing Ghostty controller. The diagnostic bridge is imported only in Vite's explicit `native-validation` mode and is absent from normal bundles. It never controls the user's existing window. Clipboard checks back up and restore the desktop content; an unavailable/custom clipboard backup skips those checks explicitly. Test-owned processes are identified by a unique inherited marker and cleaned up even after failure; this harness cleanup does not establish the application's general no-orphan guarantee. No shell or Codex configuration is edited and no Codex task is submitted.

Each run prints its report path and keeps diagnostics under `/tmp/terminal-native-validation-*`. Exit 0 means the selected automated checks passed, exit 1 indicates a failure, and exit 2 explicitly reports unresolved blockers. Both completed runs currently return 2 due to mouse reporting or an explicitly skipped pointer check. Do not treat this as a passing whole-stage gate.

Restore a normal frontend bundle afterward:

```bash
mise exec -- npm run build
```

The complete Wayland run observed 150,000 + 100,000 + 100,000 output lines, scrollback at most 2,000 lines, an explicit 4 MiB replay-overflow warning, and tab-switch interactions of about 22–75 ms during output. Debug-build WebKit WebProcess RSS was approximately 371 MiB before floods, then 383, 370 and 364 MiB after each closed flood tab. Native app RSS was approximately 197 MiB before and 201 MiB afterward. These are finite-run measurements including runtime caches, not a proof of a global memory bound or release-build performance. X11 independently completed the same output/reuse checks with pointer automation explicitly skipped.

The original stale-cell check failed on the first newly allocated terminal after freeing a filled terminal (`ALT CLOSED` appeared in a blank row). Resetting before fitting was insufficient in native stress checks: fitting can reuse additional cells. Production now resets at the initial fitted size before connecting output, then redraws immediately. The thirty-cycle shared-engine regression and the native single-line post-stress probe independently check that only the new terminal's output appears. This fixes the observed reuse failure without replacing Ghostty or adding an emulator.

## Process ownership and lifecycle checks

Run the Rust PTY integration suite with access to the user cgroup delegation (outside a read-only cgroup sandbox):

```bash
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Observed on the Ubuntu host above: inherited setsid/double-fork descendants terminate after leader exit; SIGKILL of an isolated application-owner fixture triggers watchdog cleanup; PTY handles/input workers release independently of status polling; confirmed closure completes while stdout has an unacknowledged envelope and stdin is saturated. Missing executables fail at the exec handshake. These tests do not establish support for every Linux distribution, deliberate cgroup migration, or externally delegated services. No directories belonging to workspaces are modified.

After building the native-validation bundle as above, run the native lifecycle scenario:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --driver /path/to/WebKitWebDriver
```

This scenario requires X11 or XWayland and libX11. It identifies the isolated app through its socket peer PID and sends WM_DELETE_WINDOW only to its own full-sized top-level window. It checks socket mode 0600, oversized request rejection, backend refusal of unconfirmed Quit, cancellation and acceptance of the real Quit confirmation, hidden-window output consumption, activation without duplicate PTYs, explicit Quit, socket removal, unavailable --show behavior, and metadata restoration with a new process. Confirmation uses an accessible in-app dialog: Tauri's dialog plugin replaces window.confirm with an async function, so synchronous window.confirm checks are unsafe. The harness drives both Cancel and Confirm through the real UI. The completed native lifecycle run passed all seven checks, including both choices in the Quit UI; its report is `/tmp/terminal-lifecycle-validation-1_lxnhsb/report.json`. Reports live under /tmp/terminal-lifecycle-validation-*.

The final Wayland report is `/tmp/terminal-native-validation-c4q6gbix/report.json` (nine automated checks, no cleanup survivors). The final Wayland terminal regression includes interactive Codex prompt/menu/resize/reload without submitting a task. Mouse reporting was explicitly skipped at the user's request and remains deferred; exit 2 records that incomplete gate rather than a new lifecycle failure. Native Wayland window-manager hide/reopen and a separately installed desktop launcher remain manual/packaging checks.

## Workgroup acceptance

1. Create two workgroups with existing root directories. Create a workspace inside each using its + button. Verify the initial directory matches that group's root, and override it with another existing directory. Each new workspace must automatically open exactly one tab using the first YAML action's name and launch configuration, with live output immediately. Invalid YAML must prevent creation without a partial workspace.
2. Edit each group's actions.yaml using the path shown under Edit workgroup. Verify New tab lists only that group's actions, reloads file edits, and reports malformed/oversized configuration without overwriting it. Existing tabs retain their copied settings. New files use bundled defaults. Only the actions root is supported; older property names must be updated manually.
3. Run a streaming terminal. Fold/unfold its group, switch groups, and reload the WebView. Verify folded state persists, the same session/PID and selected terminal remain, and output keeps updating.
4. Rename a group and change its root. Existing workspace paths and running process cwd must remain unchanged; the next workspace form uses the new root. Move a workspace through its editor without remounting its terminal.
5. Restart and verify group membership/folding and workspace selection restore. Upgrade a schema-v1 fixture with open tabs: IDs, paths, literal arguments, explicit overrides, selection and resume flags must survive the migration.
6. Delete an empty group after confirmation. A nonempty group must refuse deletion. Root/workspace directory contents and group YAML must remain untouched. Never reuse a deleted group's ID for another group's actions.

The native lifecycle harness includes workgroup scenarios with `--workgroups`:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --workgroups --driver /path/to/WebKitWebDriver --application /tmp/terminal-workgroups-native-app
```

After a native-validation frontend/Cargo build, copy the bundled executable to the supplied /tmp path before running: concurrent test/dev builds can otherwise replace the target/debug executable. The harness uses isolated metadata/configuration and never controls the user's running app.

Observed workgroup run: `/tmp/terminal-lifecycle-validation-ynwdaok8/report.json` passed all ten selected checks, including the three workgroup scenarios and seven lifecycle regressions, with no cleanup survivors. The corresponding Wayland/Codex regression report is `/tmp/terminal-native-validation-btewz50h/report.json`; all nine selected checks passed, with mouse explicitly deferred (exit 2). Rust tests additionally verify successful legacy migration, rollback on invalid legacy relationships, and group-edit rollback after failed SQLite writes.

First-action launch regression: `/tmp/terminal-lifecycle-validation-irn292nc/report.json` passed all ten selected native checks. New workspaces open exactly one tab named after their first YAML action and produce live output without using New tab. The Rust suite also verifies persisted copied settings and atomic rejection of empty/invalid action YAML.

## Executable replacement during a running session

After building a native-validation executable, run `scripts/check-lifecycle.py --replace-executable --driver /path/to/WebKitWebDriver --application /path/to/bundled/application`. The harness copies the application into its own private temporary directory, starts it, replaces that copy and unlinks the original inode, and verifies `/proc/<pid>/exe` reports `(deleted)` before creating a workspace/PTY. It never replaces the supplied executable. The application must still launch its first action, preserve hide/reopen, and terminate cleanly on Quit. This catches the deleted-current_exe launch failure seen during development rebuilds.

Observed report: `/tmp/terminal-lifecycle-validation-yblge92l/report.json`, all eight selected checks passed. The Rust PTY/persistence regression suite also passed. A running older application must be restarted once to load this fix; rebuilding alone cannot update its Rust backend.

## Workgroup action editor

Run the native lifecycle harness with `--workgroups --actions-editor` to exercise form/YAML switching, action creation/removal/reordering, invalid-save rejection without overwriting the file, successful save/reopen, and real command launch with explicit environment overrides while a sibling PTY keeps running. The modal has separate General, Actions and YAML tabs; action views share the same edits. Rust filesystem tests verify strict actions-only schema, bounded size, invalid environment/directory rejection, malformed-file repair, atomic replacement and per-group isolation. Known local configuration files were explicitly updated to actions; no compatibility reader was retained.

Observed editor run: `/tmp/terminal-lifecycle-validation-z65jy7vr/report.json` passed all eleven selected native checks, including form/YAML synchronization, action ordering/removal, invalid-save protection, save/reopen and command execution with explicit environment overrides. The existing sibling PID/session was preserved, and cleanup reported no survivors. Multiline YAML input used real Enter key events because this WebKitGTK driver strips line breaks from its element-value endpoint.
