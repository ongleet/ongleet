# Engineering guide

Read only sections relevant to the change. See [stack](STACK.md), [validation](VALIDATION.md) and [full contracts](reference/ENGINEERING.md) for detail.

## Terminal contracts

Keep PTYs in Rust and controllers persistent across renders/navigation. Preserve raw bytes, bounded queues and ACK backpressure through frontend consumption. Hidden terminals must consume output without animation-frame dependencies. Resize only from visible nonzero containers; dispose resources on explicit closure.

Use literal executable/argument arrays; shell syntax requires explicit shell mode. Re-exec `/proc/<app-pid>/exe`, never a replaced pathname. Join delegated cgroup v2 before user exec; require pidfds and `cgroup.kill`. Confirmed close immediately kills the leaf and waits; keep the death watchdog outside it. Deliberate cgroup migration/external services are outside containment.

Apply RIS only to new emulators after initial fit/redraw, before output attachment. Reload reattaches rather than relaunching. Suppress input/query replies/resizing during replay; report journal overflow. Ghostty frontend state is authoritative for snapshots: no second emulator or ANSI stripping.

Use native clipboard and Ghostty paste APIs. Await the in-app destructive confirmation; synchronous `window.confirm` is unsafe under Tauri.

## Metadata and actions

- Projects contain sessions and plugin tabs. Root changes affect future sessions; session directory changes affect future launches. Never mutate session directories during metadata operations.
- Validate YAML before creating metadata. A new session launches the first configured terminal action, never a stopped placeholder.
- Keep definitions separate from openings. CLI closure retains definitions; GUI removal archives them. Persist plugin ID/version/payload and selection. Runtime supplies process status.
- Use transactional migrations in standard user-data SQLite; never reset a failed database. Store explicit overrides only, never inherited environment or credentials.
- Merge inherited environment, project root `env`, then action overrides. Existing processes/configurations stay independent. Only the `actions` YAML schema is supported; configuration lives outside project roots.
- External commands launch explicitly, independently of tabs/PTYs. Reap bounded children, discard output and report failures. Preserve literal placeholders; shell context uses quoted reserved environment variables.
- Files folder updates preserve `editorFile`; Back clears only that field. Restore disk contents without silently discarding live edits. Drafts persist in WebView localStorage with their original revision, scoped by tab/path; missing files recover drafts without recreation. Explicit discard clears drafts; close/Quit retain them. Conflicting saves offer cancel, revision-checked force save, or a bounded worker-based three-way merge into the editor, rebasing onto current disk contents without writing.
- Window close hides; repeated launch/`--show` activates. `--show` never starts an unavailable app. Quit confirms and cleans up. Restart only the last recorded running set; stopped/closed/exited tabs never auto-start.
- UI/CLI share Rust operations. Keep sockets current-user-only, stable IDs, structured errors and meaningful exit codes. Never silently start the app.

## Code map

- UI/plugins: `src/main.tsx`, `src/plugins/`, `src/ProjectEditor.tsx`, `src/SessionSidebar.tsx`.
- Terminal frontend: `src/plugins/terminal/frontend.ts`, `src/byte-transport.ts`, `src/render-visibility.ts`.
- Rust PTY/containment: `src-tauri/src/terminal_session.rs`, `containment.rs`.
- Plugin SDK/core services: `src/sdk/`, `src/plugins/*/manifest.json`, `src-tauri/src/core/`; see [plugin development](PLUGINS.md).
- Registry/storage/actions: `src-tauri/src/session/`, `plugins/`, `storage.rs`, `migrations/`, `actions.rs`.
- Lifecycle/control: Rust `main.rs`, `instance.rs`, `control/`, `cli/`, `bin/`. Codex: `codex.rs`, `bin/dcodex.rs`.
- Defaults: `config/actions.yaml`; project overrides: `projects/<id>/actions.yaml`.
- Validation: `scripts/check-*.py`, `scripts/check-*.mjs`, `scripts/terminal-probe.py`.

Preserve database sidecars/Codex mappings during branding migration; reject live writers/conflicts. Grant only required frontend capabilities.

## Checks

Run checks appropriate to the change through mise:

```bash
mise exec -- npm run check:host
mise exec -- npm run build
mise exec -- npm run check:plugins
mise exec -- npm run check:editor-merge
mise exec -- npm run check:wasm
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:rendering
mise exec -- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Cargo needs a populated cache. Launch development with `mise exec -- npm run tauri -- dev`. Native evidence, not build success, establishes WebKitGTK behavior.
