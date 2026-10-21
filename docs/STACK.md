# Stage-one stack evaluation

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
- React owns one persistent controller. Container resizing uses ResizeObserver and ignores zero-sized containers. Closing explicitly disposes emulator resources and subscriptions.
- The authoritative terminal screen is in the frontend's existing Ghostty instance. Inspect screen reads that buffer, limited to the visible screen and 65,536 JavaScript code units. There is no second emulator and no regex ANSI stripping. The future CLI must report snapshot-unavailable when this frontend is disconnected. A hidden but connected frontend can supply snapshots if WebKitGTK continues processing JavaScript; this still needs validation.

## Open blockers and limits

1. Interactive Codex and browser-facing behavior must pass the native validation checklist. Builds and engine checks alone are insufficient to adopt the stack for the full UI.
2. The prototype terminates Linux processes sharing the PTY session ID, including separate foreground/background job-control groups, using SIGKILL. This intentionally prioritizes termination over graceful shutdown. A process that escapes with setsid(), delegates to another service, or forks during enumeration can escape. `/proc` enumeration also has PID reuse races. **This does not satisfy version one's no-orphan guarantee.** Stage two needs an enforceable containment strategy, likely a per-terminal cgroup under a delegated user hierarchy, with explicit failure when containment cannot be established. Killing only the shell/process group is insufficient.
3. The leader is reaped on status polling; PTY handles are finally released on explicit close. Eager, race-safe release and attachment-loss recovery belong to stage two.
4. Browser clipboard reads failed with NotAllowedError during user testing. The Copy/Paste buttons and Ctrl+Shift+C/V now use Tauri's native clipboard-manager plugin with text-only read/write permissions. Paste still goes through Ghostty's paste API to preserve bracketed paste. Native clipboard behavior under WebKitGTK/Wayland needs retesting after rebuilding.
5. ghostty-web has a render loop per terminal. Efficient hidden-tab painting suppression has not been validated. Do not implement a multi-terminal interface until output consumption and rendering can be controlled separately.
6. This is not packaged for distribution, has no metadata database or control socket, and does not survive window close, crash, Quit, or reboot. No output history is saved.

No fallback terminal engine is selected. If ghostty-web fails the native gate, report the failing behavior and options before proceeding.
