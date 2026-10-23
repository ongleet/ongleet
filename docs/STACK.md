# Stack summary

DevCenter is a Linux desktop application using Tauri 2, React/TypeScript, Rust, portable-pty, SQLite and Ghostty WASM through pinned `ghostty-web` 0.4.0. No alternate terminal engine or daemon is used.

## Runtime boundaries

- Rust owns PTYs, process containment, metadata and shared UI/CLI operations. React retains one Ghostty controller per stable tab ID; navigation changes visibility, not process ownership.
- Ghostty parses raw bytes and owns screen state. TypeScript supplies rendering/input/selection. Validate browser behavior in WebKitGTK; engine tests alone are insufficient.
- Vite bundles local WASM through `?url`; no runtime CDN. `write(Uint8Array)` parses synchronously in 0.4.0, so ACK follows parsing rather than animation-frame completion. Recheck on upgrade.
- Require delegated systemd cgroup v2, `cgroup.kill` and pidfds. Bootstrap through `/proc/<app-pid>/exe`. No process-group fallback; non-systemd Linux, restricted containers and macOS are unsupported.

## Bounds and recovery

- Output: 16 KiB reads, one unacknowledged IPC envelope, 4 MiB byte/resize replay journal. Overflow discards replay and reports unavailable history. Exit draining permits another 1 MiB; excess is reported/discarded.
- Input: 64 KiB frontend budget, 8 KiB native writes, one queued native request. A dedicated writer avoids blocking registry/WebView threads.
- Scrollback: 2,000 lines. Hidden terminals parse/ACK without painting. Upstream cursor polling remains.
- Reload reattaches to existing live/exited PTYs. Replay suppresses input/query replies/resizing until ready; selection and scroll position are not restored.
- New Ghostty handles receive RIS after initial fit, then immediate redraw before output attachment. Never reset live handles during navigation.
- Frontend screen snapshots: visible screen, 65,536 JavaScript code units. Persistent history: 2,000 lines/~60,000 UTF-16 units, 256 KiB wire/storage bound. History is text, not a process checkpoint or complete transcript.

## Persistence and plugins

SQLite stores stable IDs, definitions, openings, selections, ordering, explicit launch overrides and restart eligibility. Only previously running commands restart, as new processes. Window close hides; Quit terminates contained processes.

Built-in terminal, Files and PR plugins own versioned configuration. Unknown plugins remain opaque/unavailable. Schema 4 wraps plugin payloads; schema 5 repairs either schema-4 PR-column variant; schema 6 adds archives; schema 7 adds bounded versioned plugin namespaces and migrates PR associations. Archive version 2 preserves namespaces and reads version-1 payloads. Archives retain metadata for 30 days, restore stable IDs and use retryable two-phase artifact purge. They retain neither live processes nor unsaved edits.

Files uses bounded scans/batches and a 1 MiB UTF-8 CodeMirror editor. Saves check revisions and atomically replace eligible files. BOM, uniform LF/CRLF, ordinary permissions and group survive; ACLs/xattrs do not. External-writer races remain. Drafts are memory-only.

PR status uses explicit session associations and authenticated `gh`, with 20-second timeouts, at most 32 jobs and 60-second visible-only polling. Cancellation waits for child cleanup. Results are not persisted; no Git discovery or credential storage.

External command actions have no PTY/tab, discard output and intentionally outlive DevCenter. At most 64 direct-child waiters reap/report exits. Literal argument placeholders and explicit shell mode remain separate.

`dcodex` records conversation IDs through invocation hooks and resumes after a two-second delay, stopping after five rapid exits. Missing history never starts a fresh conversation implicitly. Hook trust bypass also permits other enabled hooks without review. No global Codex configuration changes occur.

## Control and distribution

The CLI uses shared Rust operations over a private same-UID socket, never SQLite. Protocol 2 allows 128 KiB requests/1 MiB responses; requests use EOF framing. Server read/write limits are two seconds; CLI response reads allow 30 seconds. Mutations are not retried automatically.

Names: `devcenter-gui`, `devcenter`, `dcodex`; desktop ID `io.local.devcenter`; shared XDG directories `devcenter`. Legacy migration preserves data/configuration and rejects destination conflicts/live writers.

`ghostty-web` declares MIT and ships patched Ghostty WASM. Distribution still requires applicable notices and WASM source/build provenance.

See [engineering contracts](ENGINEERING.md), [validation](VALIDATION.md) and [full stack reference](reference/STACK.md) for API lists, adapters, migration details and design references. The reference records historical implementation states; current behavior is described here and in README.

Bundled frontend plugins call the scoped SDK in `src/sdk/`; native implementations live under `src-tauri/src/core/`. Shared manifests drive Vite/Rust registration and configuration discovery. The Rust PR parser remains only for CLI compatibility. See [plugin development](PLUGINS.md).
