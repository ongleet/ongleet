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
| Native PTY, session, tab and SQLite tests | Passed: thirty-three Rust tests plus one helper integration check, including project v1→v2 migration, root defaults/overrides, membership/fold persistence, scoped YAML isolation, directory non-mutation, live PTY preservation, cgroup detached/double-fork cleanup, app-death cleanup, eager PTY release without status polling, blocked input/output closure, and PTY reconnect/replay, stale ACK rejection, replay backpressure/budget/EOF, YAML validation/reload, independent template openings, reopen, resume flags, retained definitions, missing directories, failed writes, transaction rollback, exclusive lock and future-schema refusal. Login-shell test passes outside the sandbox. |
| Hidden output and drawing adapter | Passed against independent real WASM states with an instrumented drawing boundary |
| Native Tauri/WebKitGTK automation | Observed outside the sandbox on Wayland and X11/XWayland, using isolated temporary metadata/configuration |
| Interactive Codex in the Tauri WebKitGTK window | Wayland: startup, prompt/cursor editing, slash menu, resize/reload and unsent draft preservation passed; no task submitted. Multiline paste, approval prompts, scrolling and interrupt behavior remain manual |
| Native keyboard, Unicode and clipboard | Shell typing, arrows, wide characters/emoji, deliberately split UTF-8 and native Ctrl+Shift+C/V passed on Wayland and X11. Pointer selection/native copy also passed on Wayland in the 2026-10-22 follow-up; IME, primary selection and broader key coverage remain pending |
| Native curses and resizing | Actual PTY dimensions, alternate-screen replay and return to normal passed on Wayland and X11 |
| Native live output and reuse | Repeated hidden-stream reloads, unchanged shell PID, suppression of replayed DSR replies, exited output replay, ten tab create/close cycles and clean post-stress screen passed on Wayland and X11 |
| Mouse reporting | Wayland passed with the new browser adapter on 2026-10-22, resolving the earlier no-report blocker. X11 pointer automation remains unverified; earlier X11 checks timed out or explicitly skipped it |
| Shared WASM reuse | Reproduced closed-tab cell leakage. Explicit RIS after initial fitting and immediate canvas redraw fixes the observed case; thirty shared-engine cycles and native post-flood checks pass |
| Clipboard user test | Native plugin fix confirmed working by the user |
| Session sidebar, terminal tabs and persistence | Implemented: durable definitions/openings and selective command restart; native acceptance pending |
| Hidden-window lifecycle | Native X11/XWayland close/hide, continued output with same PID, --show, second-launch activation, explicit Quit, socket cleanup and fresh-process restart passed. Wayland terminal regressions pass separately; native Wayland window-manager close remains manual |
| Unix-socket CLI | Session/project/application commands passed isolated native integration: concurrent creation, stable PTYs, live sidebar updates, confirmations, JSON errors, bounds, permissions, unavailable/explicit start. Terminal commands/input/snapshots remain pending |

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

1. Create two sessions using existing directories, including a path with spaces. Use the native picker and manually entered paths. Reject blank names and missing directories without creating anything on disk.
2. Start a shell or command in both. Print distinct markers, run a continuously emitting command in one, and switch repeatedly. Verify the original process and screen remain, hidden output appears on return, and dimensions remain valid.
3. Edit names from each session's vertical menu, drag the grips to reorder, and test Alt+Up/Down on a focused grip. Verify selection stays associated with the same session ID and the bottom sidebar text is absent.
4. Update a base directory while its shell is running. Confirm its existing working directory is unchanged; close/restart its terminal and verify the new launch uses the updated base directory unless explicitly overridden.
5. Cancel closure of a session containing a session, then confirm it. Verify other sessions keep running and the closed session's directory and contents remain intact. Inherited detached descendants must also terminate.
6. Verify stopped sessions close without a process confirmation, closing the selected session selects a neighbor, and closing the last session shows the creation screen.
7. Restart and verify saved sessions, definitions, ordering and selection return. Window close hides without terminating processes; explicit Quit confirms running processes. Previously running commands start anew after an actual application restart.

Remaining interactive checks and Linux environment coverage must be completed before claiming version-one compatibility.

## Terminal-tab acceptance

1. Create several tabs in one session, then another session with different tabs. Start a shell, Codex, and a continuously emitting command independently. Switch tabs/sessions and verify each process, screen, scrollback, and selected tab are preserved without zero-size resizes. After selecting another tab by click or arrows/Home/End in the tab bar, type immediately and verify input reaches the selected terminal without clicking its contents.
2. Close tabs with their crosses; renaming controls are temporarily hidden. Drag running tabs before/after one another; verify original processes/screens remain. Test both directions, cancellation, same-position drops, and ordering with closed definitions still saved. Use arrows/Home/End for selection and Alt+Left/Right for reordering; test names long enough to require horizontal scrolling.
3. Launch an executable with literal arguments containing shell operators. Confirm no shell evaluation occurs. Then explicitly choose Shell command and verify shell syntax works. Test independent environment overrides in two tabs without saving the inherited environment.
4. Launch tabs with and without directory overrides. Change the session directory and verify existing processes remain unchanged, future launches without overrides follow the new base, and explicit overrides stay fixed. Try a missing launch directory and invalid executable.
5. Cancel and then confirm closing a running tab. Verify siblings stay running. Cancel closing a session with multiple sessions and confirm none stop; then confirm closure and verify all its sessions stop. Verify inherited detached descendants also terminate.
6. Let a process exit with a nonzero code; verify its tab shows the code and its output stays available. Close it and use New tab for an explicit new launch. Close the last tab and create a replacement from the empty state.
7. Verify native clipboard, selection, mouse reporting, and alternate-screen restoration after repeatedly switching between tabs. Compare hidden-output flood responsiveness and memory to the single-terminal baseline.

## Persistence and relaunch acceptance

1. Save several sessions and definitions, reorder/select them, close one tab, and restart. Verify metadata survives and closed metadata stays closed and New tab offers the YAML types instead of a reopening menu.
2. Leave one command running, explicitly stop another, let a third exit, and close a fourth tab. Restart: only the formerly running command should launch automatically, once, with a new PID and fresh terminal state. Stopped open tabs remain stopped; closed definitions remain available without running.
3. Close the window while several tabs run, reopen it and verify the same PIDs and screens. Cancel Quit and verify all sessions remain, then confirm Quit and verify termination. Relaunch the application: eligible commands start as new processes.
4. With the app closed, remove/rename a session directory. Verify restoration keeps the session, launch reports an error, and no directory is recreated. Correct the path and start explicitly.
5. Verify a second application cannot write the same database or duplicate startup launches. It must focus/reopen the existing window; concurrent startup must not duplicate sessions.
6. Inspect the standard app-data database and its permissions. Verify no inherited environment or terminal output was saved, and that new tabs persist their own copy of the YAML launch configuration.

## YAML tab types acceptance

1. Open New tab and launch Bash and Codex. Verify independent processes start in the session directory and multiple tabs of the same type can run.
2. Add a type with literal arguments, environment overrides and a directory override. Reopen the picker without restarting; verify the new type and launch configuration. Existing tabs must remain unchanged.
3. Try malformed YAML, duplicate IDs, unknown fields/kinds and conflicting shell/executable modes. Verify a clear error without replacing the file or launching a tab. Fix it and retry.
4. Close a running tab with confirmation; verify no configured-tab reopening selector remains. Restart and verify the existing resume policy is preserved.

## WebView reload acceptance

1. Run Bash and Codex, note shell PIDs and print distinct Unicode markers. Reload the WebView (not the whole Rust application). Verify the same processes remain, screens return without the existing-session error, and input/copy/paste/resize still work.
2. Reload during continuous output and repeatedly with hidden sessions/tabs. Verify output resumes once, siblings stay independent, and no terminal query replies or commands are sent during replay.
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

Each run prints its report path and keeps diagnostics under `/tmp/terminal-native-validation-*`. Exit 0 means the selected automated checks passed, exit 1 indicates a failure, and exit 2 explicitly reports unresolved blockers. The original completed runs returned 2 due to missing mouse reporting or an explicitly skipped pointer check; the 2026-10-22 Wayland follow-up below returns 0 for its selected checks. Do not treat this as a passing whole-stage gate.

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

Observed on the Ubuntu host above: inherited setsid/double-fork descendants terminate after leader exit; SIGKILL of an isolated application-owner fixture triggers watchdog cleanup; PTY handles/input workers release independently of status polling; confirmed closure completes while stdout has an unacknowledged envelope and stdin is saturated. Missing executables fail at the exec handshake. These tests do not establish support for every Linux distribution, deliberate cgroup migration, or externally delegated services. No directories belonging to sessions are modified.

After building the native-validation bundle as above, run the native lifecycle scenario:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --driver /path/to/WebKitWebDriver
```

This scenario requires X11 or XWayland and libX11. It identifies the isolated app through its socket peer PID and sends WM_DELETE_WINDOW only to its own full-sized top-level window. It checks socket mode 0600, malformed request rejection, backend refusal of unconfirmed Quit, cancellation and acceptance of the real Quit confirmation, hidden-window output consumption, activation without duplicate PTYs, explicit Quit, socket removal, unavailable --show behavior, and metadata restoration with a new process. Confirmation uses an accessible in-app dialog: Tauri's dialog plugin replaces window.confirm with an async function, so synchronous window.confirm checks are unsafe. The harness drives both Cancel and Confirm through the real UI. The completed native lifecycle run passed all seven checks, including both choices in the Quit UI; its report is `/tmp/terminal-lifecycle-validation-1_lxnhsb/report.json`. Reports live under /tmp/terminal-lifecycle-validation-*.

The final Wayland report is `/tmp/terminal-native-validation-c4q6gbix/report.json` (nine automated checks, no cleanup survivors). The final Wayland terminal regression includes interactive Codex prompt/menu/resize/reload without submitting a task. Mouse reporting was explicitly skipped at the user's request and remains deferred; exit 2 records that incomplete gate rather than a new lifecycle failure. Native Wayland window-manager hide/reopen and a separately installed desktop launcher remain manual/packaging checks.

## Project acceptance

1. Create two projects with existing root directories. Create a session inside each using its + button. Verify the initial directory matches that group's root, and override it with another existing directory. Each new session must automatically open exactly one tab using the first YAML action's name and launch configuration, with live output immediately. Invalid YAML must prevent creation without a partial session.
2. Edit each group's actions.yaml using the path shown under Edit project. Verify New tab lists only that group's actions, reloads file edits, and reports malformed/oversized configuration without overwriting it. Existing tabs retain their copied settings. New files use bundled defaults. Only the actions root is supported; older property names must be updated manually.
3. Run a streaming terminal. Fold/unfold its group, switch groups, and reload the WebView. Verify folded state persists, the same session/PID and selected terminal remain, and output keeps updating.
4. Rename a group and change its root. Existing session paths and running process cwd must remain unchanged; the next session form uses the new root. Move a session through its editor without remounting its terminal.
5. Restart and verify group membership/folding and session selection restore. Upgrade a schema-v1 fixture with open tabs: IDs, paths, literal arguments, explicit overrides, selection and resume flags must survive the migration.
6. Delete an empty group after confirmation. A nonempty group must refuse deletion. Root/session directory contents and group YAML must remain untouched. Never reuse a deleted group's ID for another group's actions.

The native lifecycle harness includes project scenarios with `--projects`:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --projects --driver /path/to/WebKitWebDriver --application /tmp/terminal-projects-native-app
```

After a native-validation frontend/Cargo build, copy the bundled executable to the supplied /tmp path before running: concurrent test/dev builds can otherwise replace the target/debug executable. The harness uses isolated metadata/configuration and never controls the user's running app.

Observed project run: `/tmp/terminal-lifecycle-validation-ynwdaok8/report.json` passed all ten selected checks, including the three project scenarios and seven lifecycle regressions, with no cleanup survivors. The corresponding Wayland/Codex regression report is `/tmp/terminal-native-validation-btewz50h/report.json`; all nine selected checks passed, with mouse explicitly deferred (exit 2). Rust tests additionally verify successful legacy migration, rollback on invalid legacy relationships, and group-edit rollback after failed SQLite writes.

First-action launch regression: `/tmp/terminal-lifecycle-validation-irn292nc/report.json` passed all ten selected native checks. New sessions open exactly one tab named after their first YAML action and produce live output without using New tab. The Rust suite also verifies persisted copied settings and atomic rejection of empty/invalid action YAML.

## Executable replacement during a running session

After building a native-validation executable, run `scripts/check-lifecycle.py --replace-executable --driver /path/to/WebKitWebDriver --application /path/to/bundled/application`. The harness copies the application into its own private temporary directory, starts it, replaces that copy and unlinks the original inode, and verifies `/proc/<pid>/exe` reports `(deleted)` before creating a session/PTY. It never replaces the supplied executable. The application must still launch its first action, preserve hide/reopen, and terminate cleanly on Quit. This catches the deleted-current_exe launch failure seen during development rebuilds.

Observed report: `/tmp/terminal-lifecycle-validation-yblge92l/report.json`, all eight selected checks passed. The Rust PTY/persistence regression suite also passed. A running older application must be restarted once to load this fix; rebuilding alone cannot update its Rust backend.

## Project action editor

Run the native lifecycle harness with `--projects --actions-editor` to exercise form/YAML switching, action creation/removal/reordering, invalid-save rejection without overwriting the file, successful save/reopen, and real command launch with explicit environment overrides while a sibling PTY keeps running. The modal has separate General, Actions and YAML tabs; action views share the same edits. Rust filesystem tests verify strict actions-only schema, bounded size, invalid environment/directory rejection, malformed-file repair, atomic replacement and per-group isolation. Known local configuration files were explicitly updated to actions; no compatibility reader was retained.

Observed editor run: `/tmp/terminal-lifecycle-validation-z65jy7vr/report.json` passed all eleven selected native checks, including form/YAML synchronization, action ordering/removal, invalid-save protection, save/reopen and command execution with explicit environment overrides. The existing sibling PID/session was preserved, and cleanup reported no survivors. Multiline YAML input used real Enter key events because this WebKitGTK driver strips line breaks from its element-value endpoint.

Common environment acceptance: use Actions → Common environment variables to add/remove name/value pairs; switch through YAML and verify root env is preserved. Save, reopen the modal and launch two actions with a shared variable and a conflicting per-action override. The action override must win, normal inherited variables must remain, and values must stay literal. Modify a common value and start a new process: only the new process gets the new value. Reloading/reattaching an existing PTY must not reapply variables. Rust PTY tests exercise precedence, literal/empty values, reload-on-launch, unchanged running environment and metadata persistence without merging common/inherited values into tab definitions.

Observed common-environment run: `/tmp/terminal-lifecycle-validation-c2xafswq/report.json` passed all eleven selected checks, including common-variable form/YAML persistence, per-action override precedence, real command output, and unchanged existing process environment. The parallel Rust suite also exposed an already-offline cgroup read race; cleanup now waits for verified empty/removal on ENODEV rather than reporting a false termination failure, and the full suite passed afterward.


## Session CLI integration

Build/copy the native-validation GUI as above and build `devcenter` from the same revision. Add `--cli /absolute/path/to/devcenter` to `scripts/check-lifecycle.py`. For example:

```bash
mise exec -- /usr/bin/python3 scripts/check-lifecycle.py --cli /tmp/terminal-cli-validation-client --application /tmp/terminal-cli-validation-app --driver /tmp/terminal-native-tools/extracted/usr/bin/WebKitWebDriver
```

The harness uses isolated XDG paths and test-owned project directories. It exercises session/project creation, listing, renaming, directory updates, selection, moving, local ordering, folding, concurrent creations/requests, live sidebar refresh, preserved PTY identity/cwd, rejected and confirmed process termination, non-existent IDs/directories, directory non-mutation, socket permissions, request size/version/schema errors, hidden-window changes, unavailable errors without auto-start, and explicit start/show/status/Quit. CLI success/error streams are decoded as JSON and exit codes checked. This does not validate terminal CLI operations or output snapshots, which are not implemented.

The initial CLI scenario passed nine checks with no surviving test processes in `/tmp/terminal-lifecycle-validation-sszzp_jw/report.json`. Earlier attempts timed out waiting for the first terminal snapshot; a fresh native-validation rebuild/copy passed. Native test binaries must be isolated copies because subsequent Cargo builds can replace the normal target executable with a different frontend mode.

The combined project/editor/CLI lifecycle run passed all 13 checks with no survivors in `/tmp/terminal-lifecycle-validation-nh0boq2g/report.json`, including session changes while the window was hidden and retained live PTYs. The CLI binary links libc/libgcc only, with no GTK/WebKit dependencies at runtime.

Final CLI-only lifecycle recheck passed nine checks with no survivors in `/tmp/terminal-lifecycle-validation-06r2v_oq/report.json` after response serialization was capped at the wire budget. Rust checks passed 33 application tests, four CLI/protocol/socket tests and one helper integration test; normal frontend production build and Cargo check passed.

Session PATH integration: the isolated native CLI lifecycle run `/tmp/terminal-lifecycle-validation-ejf36jj9/report.json` passed nine checks with no surviving processes. Its configured session action invokes `terminal-workspace session list` by name before starting the output probe, checking successful CLI discovery and socket control inside a real PTY. The fixture keeps the Python probe in-process via runpy; earlier attempts to re-exec Python after the CLI call exited before probe startup, despite captured CLI success. The real-PTY Rust regression also verifies that the executable directory is first on default PATH and an explicit PATH override remains exact.


## External project actions

The isolated native project/editor/lifecycle run `/tmp/terminal-lifecycle-validation-x4pvvn4b/report.json` passed 12 checks with no surviving test processes. A real non-PTY Python action was added through the Actions form, round-tripped through YAML, then launched through the session Actions picker. It recorded literal session path arguments, shell metacharacters, cwd, reserved session environment and common project environment. Tab counts and the existing terminal session ID/PID remained unchanged. This uses a stand-in executable; actual VS Code/desktop-terminal UI integration was not exercised.

Rust checks cover direct external spawning in a new session, discarded/non-PTY stdio, literal arguments, exit status/reaping, unknown/unclosed placeholders, single-pass substitution, context/environment resolution without tabs, initial terminal selection past command actions and atomic refusal when no terminal default exists. Spawn failures and missing directories release the bounded direct-child slot. External applications intentionally outlive session/app closure and are excluded from PTY process ownership; this is documented behavior, not persistent terminal-session support.

## Codex conversation wrapper

Build the standalone helper and exercise the real wrapper/hook subprocess boundary without calling an AI service:

```bash
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --offline --bin dcodex
mise exec -- python3 scripts/check-codex-wrapper.py
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

The subprocess check uses a stand-in Codex executable. It verifies first launch, exact-ID resume, ID replacement, session/tab isolation, explicit fresh launch, literal forwarded arguments, cwd/environment inheritance, exit-code propagation, quoted hook executable paths, private mapping permissions and failure without resetting malformed files. Rust tests also reject stale generations and malformed/oversized hook events. A real cgroup-contained PTY test launches the wrapper through the shared registry, verifies reserved session/tab context wins over action overrides, and records an ID through the actual recording helper. Run PTY/socket tests outside a sandbox that changes peer credentials or denies cgroup access.

Observed: frontend production build, Cargo format/check, helper build, subprocess checks and the full Rust suite passed. One full-suite attempt hit the existing PTY ACK test's byte-count assertion (1,048,578 instead of 1,048,576); the next full run passed without changes to that test or transport code. The initial sandboxed socket suite failed its peer-UID check and passed outside that sandbox.

Installed Codex 0.163.0 delivered a real hook in an isolated temporary profile: a minimal noninteractive request returned OK and its reported thread ID matched the saved mapping. An interactive PTY check also captured an ID after submitting a minimal prompt. These automated capture checks bypassed hook trust inside their isolated test profiles. A follow-up check of normal trust marked session-flag hooks trusted but did not record an ID after an actual reply; AO explicitly uses the invocation-wide hook-trust bypass for this source type. The production wrapper therefore adds that flag too, with its effect on all enabled hooks documented in README. A separate normal startup displayed Codex's review/trust menu for the two recording hooks. Temporary profiles used copies of existing authentication, were removed afterward, and did not edit the user's Codex configuration. These are CLI/PTY checks, not a complete native WebKitGTK Quit/restart acceptance run.

Manual acceptance: change a project's Codex action executable to `dcodex`, open a new tab, check that the enabled hook sources are acceptable and submit a prompt. Quit while the tab remains running and restart: the same tab must resume the saved conversation. Verify two Codex tabs stay independent, a session switch/fork updates only its originating tab's mapping, disabled hooks leave IDs unrecorded, and missing history reports an error rather than starting fresh. Existing plain `codex` tabs and project files are not migrated automatically.


## DevCenter rename and directory migration

The current GUI target is `devcenter-gui`, control CLI is `devcenter`, and Codex wrapper is `dcodex`. The native harness defaults and fixture configuration/data/socket paths use the renamed targets. Earlier report paths and command names above describe historical runs.

Migration tests use private temporary directories and a real SQLite database. They check database/YAML/conversation-map/sidecar preservation, repeated startup, refusal while the legacy writer lock is held, and refusal without modifying either side when a new directory already exists. Build all binaries before the subprocess check: `mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --offline --bins`, then `mise exec -- python3 scripts/check-codex-wrapper.py`. The old CLI and wrapper names remain buildable compatibility aliases.

Observed rename/migration validation: the full Rust suite, Cargo format/check, renamed wrapper subprocess check and production frontend build passed. The isolated native project/editor/CLI/lifecycle run `/tmp/terminal-lifecycle-validation-oxrtnza_/report.json` passed all 14 checks with no cleanup survivors, covering the new config/data/socket paths, `devcenter` discovery inside a PTY, CLI creation, action editing, activation, Quit and restart. A normal frontend and bundled GUI build replaced the native-validation artifacts afterward.

At the user's request, the real legacy data and config directories were migrated with `devcenter-gui --migrate` after verifying the legacy writer was stopped. SHA-256 comparison verified all 467 data files and four config files were preserved. The database is now `~/.local/share/devcenter/workspaces.sqlite3` and project configuration is under `~/.config/devcenter/` on this host.


Agent 2 resume investigation: the Baserow tab definition (session 15/tab 43) uses `dcodex`, has restart enabled, and points to an existing unarchived Codex conversation under the project's configured Codex home. Isolated copies of that tab metadata, mapping and history restored its two user turns through the standalone wrapper, bundled native Tauri/WebKitGTK, and a Vite-served development build. Native snapshots showed the prior conversation; no new prompts were submitted. This establishes that the current saved mapping can resume, but does not explain the user's earlier empty-conversation starts. No production configuration or conversation mapping was changed. Temporary copies of authentication and history were removed after testing.


Follow-up live Agent 2 investigation: the user's exact `mise exec -- npm run tauri -- dev` launch passed the expected `resume` ID, Codex home, session/tab IDs and mapping directory to the running Codex process. The user initially saw an empty conversation and subsequently confirmed that its previous messages appeared in that same instance. This supports delayed startup/display loading for that occurrence, but does not identify the slow stage or establish a timing bound. Additional isolated checks included Codex's paginated-history database; the earlier minimal profile copies had omitted it.

Test isolation correction: nested mise invocations under temporary XDG roots attempted fresh tool installations. Interrupting that test left the user's stable Rust toolchain incomplete. It was repaired with `mise install rust@stable --force`; `mise exec -- cargo --version` and `rustc --version` then succeeded (1.99.1). Future nested-mise native tests must preserve `MISE_DATA_DIR` and `MISE_CACHE_DIR` while isolating application XDG directories.

## Project / Session terminology migration

Workgroup is now Project; Workspace is now Session. UI components, Rust metadata registry/modules, Tauri commands, CLI resources/help, socket payloads, action context names and current documentation use the new hierarchy. Native PTY ownership lives separately in `terminal_session.rs`; runtime counts use `runningTerminals`. Control protocol version 2 requires the rebuilt CLI and GUI together. Historical reports above predate this terminology change.

Schema 3 transactionally renames tables/columns without copying or resetting records. Startup and `devcenter-gui --migrate` migrate the database and move `workgroups/` action configuration to `projects/` under the metadata writer lock. Action YAML contents, stable IDs, database filename and Codex mapping locations are preserved. Legacy placeholders/context variables and the wrapper's `--workspace-id` option remain compatibility inputs. Configuration conflicts fail explicitly; a completed schema migration is retained and config migration can be retried after resolving the conflict.

Observed validation: production frontend build, Cargo format/check, the full Rust suite (including schema-1/schema-2 upgrades, invalid-relationship rollback, repeated open/save and config conflict preservation), and the stand-in Codex wrapper subprocess checks passed. Rust PTY/socket checks ran outside the restrictive sandbox. The isolated native WebKitGTK project/editor/CLI/lifecycle run `/tmp/terminal-lifecycle-validation-0jysr9tt/report.json` passed all 14 selected checks with no cleanup survivors. It covered renamed UI and CLI operations, action editing, directory preservation, continuing PTYs, hidden-window operations and Quit/restart. This does not revalidate the deferred mouse-reporting gate or every terminal-stack behavior. Normal production frontend and bundled application artifacts were rebuilt afterward.

The real stopped DevCenter database was upgraded from schema 2 to 3 on 2026-10-22 after a SQLite backup and copy of legacy action configuration to `~/.local/share/devcenter/backups/project-session-20261022T093932Z`. Before/after row comparisons preserved all 4 projects, 6 sessions, 15 tab definitions, 11 openings and application state, including sequences, selection and restart flags. SHA-256 comparisons preserved all 4 action files and 4 Codex mapping files. SQLite integrity and foreign-key checks passed. Migration did not launch terminals or the GUI.

## Terminal mouse, clipboard and OSC 8 links (2026-10-22)

`mise exec -- npm run check:terminal-input` uses the actual Ghostty WASM for SGR mouse modes, raw legacy coordinates, Shift-selection override, replay suppression, listener disposal and OSC 8 metadata. Link checks include split UTF-8/ST, consecutive links, final-column output, bottom-margin wrap/scroll and RIS. Rust clipboard tests cover URI decoding, literal path quoting, control-character rejection and file-count limits.

The isolated native Wayland run `/tmp/terminal-native-validation-1dfo66km/report.json` passed all 12 selected checks. Real pointer actions selected and copied terminal text without a copy shortcut, the native clipboard distinguished image/png and copied-file URI lists, and Ctrl-click on a labeled OSC 8 link reached a private test-only GIO URI handler. Curses received native mouse reports, resolving the earlier observed missing-forwarding blocker on this host. Reload, independent tabs, native Unicode clipboard paste, alternate-screen resizing, hidden floods, bounded scrollback, replay overflow and terminal reuse also passed. The immediate cleanup report listed one WebKit helper; a follow-up /proc check confirmed that PID had exited. Interactive Codex was skipped in this run; image-format detection and Ctrl+V forwarding do not by themselves certify an actual Codex attachment.

Manual acceptance after rebuilding/restarting: open Codex, paste a screenshot with Ctrl+V and Ctrl+Shift+V and confirm an image attachment appears without submitting it. Copy an image file in the file manager and paste with Ctrl+Shift+V, including paths with spaces; confirm Codex recognizes it. Select/copy Codex output and, when mouse capture is enabled, repeat with Shift-drag. Ctrl-click a web link and an existing local-file link, then repeat after scrollback and WebView reload. No shell/Codex configuration is changed. File drag-and-drop and editor-specific line navigation remain outside this implementation.


## Persistent terminal history (2026-10-23)

The isolated native WebKitGTK/XWayland lifecycle run `/tmp/terminal-lifecycle-validation-nnyy2t2r/report.json` passed all eight checks. After GUI Quit, the private history file contained the former PTY PID; relaunch displayed that text and its separator alongside a different live PID. A WebView reload kept the same new runtime and exactly one history separator. Hide/reopen, Quit cancellation, single-instance activation and process cleanup also passed. This validates Quit/restart and reload, not an actual computer power cycle or sudden power loss.

The Rust disk test covers Unicode text surviving registry restart, separate tabs, private file permissions, stale-runtime rejection, bounded writes/reads and refusal without resetting oversized files. The full Rust suite passed outside the restrictive socket/PTY sandbox. Frontend build, Cargo format and check passed. The persisted text is a bounded Ghostty buffer snapshot, not a full transcript or process checkpoint; fullscreen/clear behavior and the latest-output loss window are documented in README.
