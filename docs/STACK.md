# Stage-one stack evaluation

Current scope: workspace sidebar and terminal tabs, requested after user feedback on the native prototype. Rust keeps workspace/tab metadata, launch configuration, ordering, and selected tabs in memory. Each tab has an independently retained PTY and emulator. This does not mark all interactive validation checks complete or resolve the process-containment limitation below.

## Candidate and licensing

The inspected and installed candidate is [`coder/ghostty-web`](https://github.com/coder/ghostty-web), npm version **0.4.0**, pinned in package.json and package-lock.json. Its package metadata and [upstream license](https://github.com/coder/ghostty-web/blob/main/LICENSE) declare MIT. Published runtime dependencies are empty. Upstream's development tools include xterm packages for comparisons; this application neither installs nor uses those packages.

This is a third-party integration around a patched Ghostty WASM build, not a supported native Ghostty surface or a promise of official browser support. Before distribution, include the applicable Ghostty and ghostty-web license notices and record the source revision/build provenance for the shipped WASM.

The Ghostty VT engine handles byte parsing and screen state. **WASM alone is not a browser terminal.** ghostty-web provides Canvas rendering, font measurement, input encoding, selection, clipboard integration, mouse handling, and scrolling in TypeScript. Every one of these browser-facing behaviors must be tested under WebKitGTK rather than inferred from native Ghostty or Chromium.

## API actually used

The installed declarations and distributed JavaScript were inspected, alongside [upstream sources](https://github.com/coder/ghostty-web/tree/main/lib). The prototype uses `Ghostty.load(assetUrl)` and a `Terminal({ ghostty })` rather than relying on `init()` to guess an asset path. Vite imports the package's exported WASM with `?url`, placing a fingerprinted local asset into the build. No CDN is used at runtime.

Other used APIs: `open`, `write(Uint8Array)`, `onData`, `onResize`, `FitAddon`, `paste`, `getSelection`, `buffer.active.getLine().translateToString()`, `dispose`, and `attachCustomKeyEventHandler`. In this version the custom handler returns true when it handles an event. Its semantics differ from some other terminal APIs despite the compatibility-oriented names.

In 0.4.0, `write` parses into WASM synchronously; its optional completion callback is scheduled through requestAnimationFrame. The output ACK is sent after the synchronous parse, without waiting for a paint, because hidden-window animation frames may pause. Reevaluate this assumption when upgrading the library.

## Ownership and limits

- `TerminalSession` owns the PTY and child independently of React. Commands address a session ID. This provides a boundary that can later move behind a daemon; no daemon exists here.
- Rust reads raw bytes in chunks up to 16 KiB. Exactly one data envelope can remain unacknowledged in the Tauri channel. Reading stops until the frontend ACKs, so pressure propagates back to the PTY producer. There is no accumulating raw-output log.
- JavaScript converts byte arrays directly to Uint8Array. It does not decode each chunk independently. Ghostty retains partial UTF-8 and VT parser state.
- Scrollback is configured to 2,000 lines. Input has a 64 KiB frontend budget, 8 KiB native writes, and a bounded one-request native input queue. A dedicated writer avoids holding the session lock while waiting for PTY input space.
- React keeps one persistent controller per tab, keyed by its stable ID inside its workspace. Container resizing uses ResizeObserver and ignores zero-sized containers. Closing explicitly disposes emulator resources and subscriptions. Tab/workspace selection and reordering change visibility/order rather than mounting new terminals.
- Workspaces retain their own selected tab. Session IDs identify a launch; stable tab IDs identify configuration across explicit stop/restart. Sessions are indexed by tab ID, not workspace ID. Workspace closure checks confirmation before terminating any of its tab sessions. The UI uses the same registry methods intended for future CLI control.
- Tab configuration contains an optional directory override, optional executable, argument array, explicit environment overrides, and optional explicit shell-command mode. Rust resolves the current workspace directory at launch when no override exists. Shell-command mode invokes the user's shell with `-lc`; direct mode passes literal arguments without shell evaluation. The inherited environment is not stored in metadata.
- All controllers share the loaded Ghostty WASM instance but use separate terminal handles. Hidden workspaces still parse and ACK raw bytes; their public `CanvasRenderer.render` method is gated off. Restoring visibility fits the container and forces a redraw. Upstream's lightweight per-terminal animation/cursor polling remains; private animation-loop fields are not patched. Recheck this adapter when upgrading ghostty-web.
- The authoritative terminal screen is in the frontend's existing Ghostty instance. Inspect screen reads that buffer, limited to the visible screen and 65,536 JavaScript code units. There is no second emulator and no regex ANSI stripping. The future CLI must report snapshot-unavailable when this frontend is disconnected. A hidden but connected frontend can supply snapshots if WebKitGTK continues processing JavaScript; this still needs validation.

## Open blockers and limits

1. Remaining interactive Codex and browser-facing checks are tracked in the native validation checklist. User feedback authorized the sidebar work; builds and engine checks alone still do not certify those behaviors.
2. The prototype terminates Linux processes sharing the PTY session ID, including separate foreground/background job-control groups, using SIGKILL. This intentionally prioritizes termination over graceful shutdown. A process that escapes with setsid(), delegates to another service, or forks during enumeration can escape. `/proc` enumeration also has PID reuse races. **This does not satisfy version one's no-orphan guarantee.** Stage two needs an enforceable containment strategy, likely a per-terminal cgroup under a delegated user hierarchy, with explicit failure when containment cannot be established. Killing only the shell/process group is insufficient.
3. The leader is reaped on status polling; PTY handles are finally released on explicit close. Eager, race-safe release and attachment-loss recovery belong to stage two.
4. Browser clipboard reads failed with NotAllowedError during user testing. The Copy/Paste buttons and Ctrl+Shift+C/V now use Tauri's native clipboard-manager plugin with text-only read/write permissions. Paste still goes through Ghostty's paste API to preserve bracketed paste. The user confirmed the fix works; the broader selection/clipboard checklist remains open.
5. Hidden painting suppression and independent screen updates pass the automated adapter check. Native workspace switching, selection retention, and flood performance across multiple terminals still require interactive observation.
6. This is not packaged for distribution, has no metadata database or control socket, and does not survive window close, crash, Quit, or reboot. No output history is saved.

No fallback terminal engine is selected. If ghostty-web fails the native gate, report the failing behavior and options before proceeding.
