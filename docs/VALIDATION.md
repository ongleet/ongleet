# Native validation gate

Do not mark a check passed without observing it. Record the OS, display backend, WebKitGTK version, ghostty-web version, and Codex version. Run on Wayland and on X11 where available. `GDK_BACKEND=wayland` or `GDK_BACKEND=x11` can select the GTK backend when the corresponding display is accessible.

## Current evidence

Host: Ubuntu 26.04.1; GTK 3.24.52; WebKitGTK 2.52.6; Wayland session variables; ghostty-web 0.4.0.

| Check | Result |
| --- | --- |
| TypeScript check + Vite production build including WASM | Passed |
| Ghostty WASM through production byte adapter | Passed |
| 100,000 output lines and configured scrollback bound, engine only | Passed |
| Native Rust compilation | Passed (`cargo check` and `cargo build`) |
| Native PTY, workspace, tab and SQLite tests | Passed: sixteen tests, including YAML validation/reload, independent template openings, reopen, resume flags, retained definitions, missing directories, failed writes, transaction rollback, exclusive lock and future-schema refusal. Login-shell test passes outside the sandbox. |
| Hidden output and drawing adapter | Passed against independent real WASM states with an instrumented drawing boundary |
| Desktop connection from agent environment | Blocked: `Gtk.init_check()` returned false |
| Launch compiled Tauri executable from agent environment | Blocked: GTK backend initialization failed before opening a window |
| Outside-sandbox desktop attempt | Rejected by session approval policy |
| Interactive Codex in the Tauri WebKitGTK window | Pending |
| Native keyboard, selection, clipboard, scrolling, mouse, resize | Pending |
| Clipboard user test | Native plugin fix confirmed working by the user |
| Workspace sidebar, terminal tabs and persistence | Implemented: durable definitions/openings and selective command restart; native acceptance pending |
| Hidden-window output and CLI | Not implemented; later stages |

The earlier desktop-access failures describe the initial sandboxed environment. Desktop initialization now succeeds outside the sandbox. This is not a claim that all interactive checks have passed.

## Interactive procedure

1. Run `mise exec -- npm run tauri -- dev` from a desktop terminal. Start a shell in an existing directory. Type text, arrows, Backspace, Delete, Home/End, Ctrl+A/E, Ctrl+C, Ctrl+D, and Ctrl+L. Check key behavior without accidentally intercepting terminal controls in the application.
2. Print `café λ 日本語 🙂 é`. Select across lines, copy using Ctrl+Shift+C, and paste using Ctrl+Shift+V. Test primary selection/middle-click if expected on the desktop. Check IME/composition input where available.
3. Add a YAML tab type for the Python fixture: executable `/usr/bin/python3`, arguments `["/absolute/path/to/scripts/terminal-probe.py", "unicode"]`. Check the deliberately split bytes produce intact text. Repeat with `flood`; interact with the window during output, scroll afterward, and inspect process memory while repeating it. Engine-only tests do not prove bounded total WebView memory or responsiveness.
4. Run the same fixture with `screen`. Check alternate screen, arrow and function keys, scrolling/mouse reports, and resize handling. Press q and verify restoration of the normal screen. Test selection with mouse reporting enabled and document the modifier behavior.
5. Choose Codex from New tab, or type `codex` in the shell. Check prompt editing, Unicode, multiline paste, menus, scrolling, Ctrl+C, approval prompts, full-screen/alternate-screen behavior where used, and resizing while Codex is active. Do not change shell/Codex configuration automatically.
6. Resize repeatedly and minimize/restore the window. A hidden window must never trigger a zero-size PTY resize. Confirm the terminal state remains intact. Window-close/hide behavior is intentionally not implemented in this prototype.
7. Close a running session, first cancelling and then confirming. Verify ordinary shell jobs terminate. The escaped-descendant limitation in STACK.md remains a blocker even if this check passes.
8. When a snapshot consumer is available, inspect screen snapshots during normal and alternate-screen use. The inspection button has been removed; the frontend adapter remains. Compare plain text with the display, including wide characters and combining marks. Close the session and confirm screen inspection becomes unavailable. No CLI snapshot support exists yet.

## Sidebar acceptance

1. Create two workspaces using existing directories, including a path with spaces. Use the native picker and manually entered paths. Reject blank names and missing directories without creating anything on disk.
2. Start a shell or command in both. Print distinct markers, run a continuously emitting command in one, and switch repeatedly. Verify the original process and screen remain, hidden output appears on return, and dimensions remain valid.
3. Edit names from each workspace's vertical menu, drag the grips to reorder, and test Alt+Up/Down on a focused grip. Verify selection stays associated with the same workspace ID and the bottom sidebar text is absent.
4. Update a base directory while its shell is running. Confirm its existing working directory is unchanged; close/restart its terminal and verify the new launch uses the updated base directory unless explicitly overridden.
5. Cancel closure of a workspace containing a session, then confirm it. Verify other workspaces keep running and the closed workspace's directory and contents remain intact. The documented escaped-descendant limitation still applies.
6. Verify stopped workspaces close without a process confirmation, closing the selected workspace selects a neighbor, and closing the last workspace shows the creation screen.
7. Restart and verify saved workspaces, definitions, ordering and selection return. Window close still quits after confirmation; previously running commands start anew on next launch.

Remaining native checks and process-containment blockers must be resolved before claiming version-one guarantees.

## Terminal-tab acceptance

1. Create several tabs in one workspace, then another workspace with different tabs. Start a shell, Codex, and a continuously emitting command independently. Switch tabs/workspaces and verify each process, screen, scrollback, and selected tab are preserved without zero-size resizes.
2. Close tabs with their crosses; renaming controls are temporarily hidden. Drag running tabs before/after one another; verify original processes/screens remain. Test both directions, cancellation, same-position drops, and ordering with closed definitions still saved. Use arrows/Home/End for selection and Alt+Left/Right for reordering; test names long enough to require horizontal scrolling.
3. Launch an executable with literal arguments containing shell operators. Confirm no shell evaluation occurs. Then explicitly choose Shell command and verify shell syntax works. Test independent environment overrides in two tabs without saving the inherited environment.
4. Launch tabs with and without directory overrides. Change the workspace directory and verify existing processes remain unchanged, future launches without overrides follow the new base, and explicit overrides stay fixed. Try a missing launch directory and invalid executable.
5. Cancel and then confirm closing a running tab. Verify siblings stay running. Cancel closing a workspace with multiple sessions and confirm none stop; then confirm closure and verify all its sessions stop. The escaped-descendant limitation remains.
6. Let a process exit with a nonzero code; verify its tab shows the code and its output stays available. Close it and use New tab for an explicit new launch. Close the last tab and create a replacement from the empty state.
7. Verify native clipboard, selection, mouse reporting, and alternate-screen restoration after repeatedly switching between tabs. Compare hidden-output flood responsiveness and memory to the single-terminal baseline.

## Persistence and relaunch acceptance

1. Save several workspaces and definitions, reorder/select them, close one tab, and restart. Verify metadata survives and closed metadata stays closed and New tab offers the YAML types instead of a reopening menu.
2. Leave one command running, explicitly stop another, let a third exit, and close a fourth tab. Restart: only the formerly running command should launch automatically, once, with a new PID and fresh terminal state. Stopped open tabs remain stopped; closed definitions remain available without running.
3. Close the window with confirmation while several tabs run. Verify their processes terminate, then launch anew on the next application start. This is command relaunch, not process survival.
4. With the app closed, remove/rename a workspace directory. Verify restoration keeps the workspace, launch reports an error, and no directory is recreated. Correct the path and start explicitly.
5. Verify a second application cannot write the same database or duplicate startup launches. It currently reports an error; window forwarding is pending.
6. Inspect the standard app-data database and its permissions. Verify no inherited environment or terminal output was saved, and that new tabs persist their own copy of the YAML launch configuration.

## YAML tab types acceptance

1. Open New tab and launch Bash and Codex. Verify independent processes start in the workspace directory and multiple tabs of the same type can run.
2. Add a type with literal arguments, environment overrides and a directory override. Reopen the picker without restarting; verify the new type and launch configuration. Existing tabs must remain unchanged.
3. Try malformed YAML, duplicate IDs, unknown fields/kinds and conflicting shell/executable modes. Verify a clear error without replacing the file or launching a tab. Fix it and retry.
4. Close a running tab with confirmation; verify no configured-tab reopening selector remains. Restart and verify the existing resume policy is preserved.
