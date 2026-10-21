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
| Native PTY integration tests | Passed: four tests using actual Linux PTYs |
| Desktop connection from agent environment | Blocked: `Gtk.init_check()` returned false |
| Launch compiled Tauri executable from agent environment | Blocked: GTK backend initialization failed before opening a window |
| Outside-sandbox desktop attempt | Rejected by session approval policy |
| Interactive Codex in the Tauri WebKitGTK window | Pending |
| Native keyboard, selection, clipboard, scrolling, mouse, resize | Pending |
| Clipboard user test | Browser API failed with NotAllowedError; replaced with native clipboard plugin, awaiting retest |
| Hidden-window output, workspace/tab switching, persistence, CLI | Not implemented; later stages |

## Interactive procedure

1. Run `mise exec -- npm run tauri -- dev` from a desktop terminal. Start a shell in an existing directory. Type text, arrows, Backspace, Delete, Home/End, Ctrl+A/E, Ctrl+C, Ctrl+D, and Ctrl+L. Check key behavior without accidentally intercepting terminal controls in the application.
2. Print `café λ 日本語 🙂 é`. Select across lines, copy using Ctrl+Shift+C and the button, and paste using Ctrl+Shift+V and the button. Test primary selection/middle-click if expected on the desktop. Check IME/composition input where available.
3. Run the Python fixture directly via the form: executable `/usr/bin/python3`, arguments `["/absolute/path/to/scripts/terminal-probe.py", "unicode"]`. Check the deliberately split bytes produce intact text. Repeat with `flood`; interact with the window during output, scroll afterward, and inspect process memory while repeating it. Engine-only tests do not prove bounded total WebView memory or responsiveness.
4. Run the same fixture with `screen`. Check alternate screen, arrow and function keys, scrolling/mouse reports, and resize handling. Press q and verify restoration of the normal screen. Test selection with mouse reporting enabled and document the modifier behavior.
5. Run Codex using the button, or type `codex` in the shell. Check prompt editing, Unicode, multiline paste, menus, scrolling, Ctrl+C, approval prompts, full-screen/alternate-screen behavior where used, and resizing while Codex is active. Do not change shell/Codex configuration automatically.
6. Resize repeatedly and minimize/restore the window. A hidden window must never trigger a zero-size PTY resize. Confirm the terminal state remains intact. Window-close/hide behavior is intentionally not implemented in this prototype.
7. Close a running session, first cancelling and then confirming. Verify ordinary shell jobs terminate. The escaped-descendant limitation in STACK.md remains a blocker even if this check passes.
8. Inspect screen during normal and alternate-screen use. Compare plain text with the display, including wide characters and combining marks. Close the session and confirm screen inspection becomes unavailable. No CLI snapshot support exists yet.

The next stage starts only after the native interactive gate has passed and remaining renderer/input blockers have been explicitly reviewed.
