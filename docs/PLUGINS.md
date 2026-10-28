# Bundled frontend plugins and SDK

Plugins are trusted bundled React/TypeScript, HTML and CSS. Rust owns PTYs, containment, metadata and native services. The application host creates one SDK attachment per open tab; hiding a tab retains that attachment. Closing, archiving, reloading or quitting cancels and waits for its subprocess and HTTP jobs. Reload reattaches PTYs without restarting them.

Add a directory under `src/plugins/` with `manifest.json` and `entrypoint.tsx`. Vite discovers the entrypoint and manifest; the Rust build discovers the same manifest without executing JavaScript. No Rust provider or registration edit is needed. Keep IDs and configuration versions stable. Unsupported plugins retain their opaque configuration and data.

A manifest declares `id`, `label`, `configVersion`, `sdkVersion` (currently `1`), `entrypoint` (currently `entrypoint.tsx`), `defaults`, `configSchema`, `capabilities` and discovery `examples`. Include a short `description` for the plugin and each configuration field so agents can understand their purpose through `ongleet --json tab types`. Configuration schemas support objects, arrays, scalar types, nullable type lists, unknown-field rejection, nested property schemas, `absolutePath` and `maxBytes`. Declare `contextDefaults: {"folder":"session.directory"}` for a folder initialized from the session directory. PTY plugins declare `runtime: "pty"`; core launch validation still applies. Configuration and runtime compatibility are separate from plugin data versions.

The entrypoint exports `Content`, `ConfigurationEditor`, `toDraft`, `fromDraft` and optionally `beforeQuit`. Content receives its tab/session and visibility through host props. Call `useSdk()` from `src/sdk`; pass that SDK to retained non-React controllers. Importing Tauri packages or SDK transport internals from plugins is rejected by `npm run check:plugins`.

The bundled [SDK example](../src/plugins/sdk-example/entrypoint.tsx) lists a directory, runs an executable with literal arguments and increments a persisted tab counter. Add an action with `kind: sdk-example` and `config: {}` to try it. The example needs no plugin-specific Rust code.

The public services are:

- `session.listTags()`, `session.addTag(tag)`, `session.removeTag(name)`: the only tag operations, returning the calling tab’s session tags in descending priority order. `SessionTag` has `{name: string, background?: string, dot?: string, priority?: number}`. Requires `storage`. Up to 16 tags; unique case-insensitive trimmed names of 1–40 characters without controls. Background and dot are independently optional `#RRGGBB` colors. Priority defaults to 0, accepts signed 32-bit integers, and sorts higher values first with stable ties. Adding an existing name replaces its style/priority while preserving siblings; removing an absent name succeeds. All mutations are atomic in Rust and share CLI validation/persistence. Use `storage.subscribe` to observe changes. Tags persist in `ongleet.session-tags` and survive archival/restoration; legacy `color` values load as dots. The `Codex` activity tag is an ordinary member of this list, managed through the same operations. Bulk replacement is not exposed.
- `tab.setTitle(title: string | null)`: updates only the calling tab’s display title synchronously. Titles are limited to 256 UTF-16 code units, trimmed, and stripped of control characters. `null` or blank text restores the configured name. Overrides last for the attachment, survive hiding and metadata refreshes, and clear on detach; they do not rename the saved action/tab. Terminal reads standard OSC 0/2 shell titles from the bounded byte stream, including fragmented UTF-8 and BEL/ST terminators; shells must emit these sequences to advertise their current directory or command. No shell startup files are changed; Files uses the open file’s basename.
- `fs.list`, `fs.readText`, `fs.saveText`, `fs.watchText`: bounded scans/text and revision-checked atomic saves. Unsupported text returns `null`, allowing a plugin to choose an external opener. Plugin configuration updates are explicit. `watchText(path, changed)` returns an unsubscribe function and watches the parent directory with Linux inotify, filtering the basename so atomic replacements remain observable. One watch per attachment/tab, 128 globally; events coalesce until `readText` acknowledges before reading. Queue overflow requests a fresh read. Detach, close, session removal and Quit stop and join watchers; subscribers may stop sooner. Network filesystem remote writes and mmap writes are outside inotify coverage; moving/deleting the parent directory invalidates the watch and requires reopening the editor.
- `env.get(name)`: read-only access to the native app’s launch environment; requires `env`. Returns a UTF-8 string (including empty strings), or `null` for an unset variable. All variable names are available to declaring trusted plugins; no variable whitelist is enforced yet. Names are bounded to 1–1024 bytes without NUL or `=`, and values to 64 KiB. Non-UTF-8 values fail explicitly. Values are returned to JavaScript without logging or persistence. This reads the app process environment, not a tab’s shell environment.
- `credentials.get(key)`, `set(key, value)`, `delete(key)`: Linux Secret Service records scoped to the plugin ID derived from the calling tab; requires `credentials`. Keys are 1–128 bytes without controls; values are UTF-8 strings up to 64 KiB. Missing keys return `null`. Each operation uses a fresh encrypted Secret Service session; locked/unavailable storage fails without unlocking prompts or a plaintext fallback. Records belong to the plugin across tabs/projects/restarts and are excluded from SQLite, namespaces, configuration and archives. Tokens are returned to trusted JavaScript; this is not a plugin sandbox.
- `http.request({url, method, headers?, body?, signal?})`: generic Rust reqwest transport; requires `http`. Returns `{status, headers, body}` with lowercase header names and UTF-8 text. HTTPS with normal certificate validation only; no redirects or URL credentials. Twenty-second total timeout, 64 KiB body, 64 KiB headers, 8 KiB URL, 512 KiB response, four concurrent requests per attachment/tab and 32 globally. Abort, detach, close and Quit cancel active transport. Native I/O runs outside registry locks. Credentials, HTTP headers and bodies are never logged. HTTP status errors remain responses for plugin-specific handling.
- `process.run`, `process.cancel`: literal executable/arguments, explicit environment overrides, optional absolute cwd, exit code/stdout/stderr. One active job per tab, 32 globally; callers may lower the 20-second timeout and 512 KiB per-stream output ceilings. Requests are bounded to 64 KiB. Cancellation completes after child cleanup. Detached external application actions remain separate.
- `pty.connect`, `acknowledge`, `input`, `sendText`, `resize`, `status`, `history`, `saveHistory`, `close`: native ownership and raw bytes with replay and backpressure. `sendText(runtimeId, text, { enter?: boolean })` encodes literal UTF-8 and optionally appends CR; the default sends no Enter. Both input methods are limited to 8 KiB per call, including Enter. Embedded newlines/control characters are sent verbatim and can execute commands. Calls acknowledge writes, not command completion; avoid automatic retries after ambiguous errors. The SDK targets only the calling tab’s runtime. `connect` also delivers an `exit` control event with the exit code, independently of data ACKs and EOF; reattachment repeats the exited state. Query `status` at connection and exit to reconcile restart eligibility and session tags, rather than polling. Always acknowledge after the terminal engine consumes output. Receipt alone never ACKs. Native adapters validate tab, frontend attachment and runtime ownership.
- `storage.read`, `replace`, `configure`, `subscribe`: versioned session/tab JSON namespaces scoped to the calling plugin ID, explicit configuration patches and change notifications. A missing namespace has revision zero. Replacement supplies the expected revision and an application-defined data version; conflicts require reloading and merging. Combined namespace data per session or tab is limited to 64 KiB, including envelopes. The host disposes subscriptions on detachment; callers may unsubscribe sooner. Mutable data stays separate from action configuration.
- `desktop.readText`, `writeText`, `clipboard`, `selectPath`, `openFile`, `openUrl`: native clipboard, file selection and external opening. Terminal clipboard distinguishes images from text for Codex paste behavior. `clipboard(selection)` and `writeText(text, selection)` accept an optional `"clipboard" | "primary"` selection, defaulting to `"clipboard"`. Primary reads are text-only; primary writes and terminal text reads are limited to 65,500 UTF-8 bytes and GTK requests time out after three seconds.

Errors are `SdkError` objects with readable messages and stable `code` values: `revision_conflict`, `busy`, `cancelled`, `timeout`, `limit_exceeded`, `unavailable`, `capability_unavailable`, or `operation_failed`. Feature-specific parsing, arguments, polling and messages belong in JavaScript. A failed refresh can retain its last successful result.

For a new native capability, implement a reusable service under `src-tauri/src/core/`, add a typed native adapter and public SDK method, and validate ownership/capability/limits there. Extend manifest capabilities and test the actual native serialization and disposal path. Do not expose raw invocation, native command names, shell interpolation or another Rust plugin provider.

Schema 7 adds session/tab namespace JSON and transactionally migrates the legacy PR association into the `pull-request` session namespace. CLI PR input validation and `pullRequest` response fields remain compatibility adapters. Archive version 2 includes namespaces; the version-1 decoder migrates legacy associations while retaining IDs, selections and opaque plugin data. Archives retain their existing 30-day policy.

Run `mise exec -- npm run check:plugins`, the frontend build and Rust suite. Native validation mode also exposes an SDK probe bridge used by the mixed-plugin harness; production bundles omit it. See [native validation](VALIDATION.md) for isolated WebKitGTK commands and remaining manual acceptance.

## Diff

The bundled **Diff** tab requires `git` on the application's PATH. Add it through
**Custom tab…**, action configuration, or the shipped Diff action (existing user
and project action files are preserved). Comparisons resolve the enclosing
repository only on request by this tab and include its entire tracked tree,
even when the configured folder is a subdirectory or linked worktree.

- **Automatic** (default) compares a feature branch’s merge base with `HEAD`,
  using `origin/HEAD` when available, then `main` or `master`. On the default
  branch, detached HEAD, or without a default branch, it shows the latest commit.
  Initial commits compare against the empty tree. No fetch is performed.
- **Working directory** compares a base tree (default `HEAD`) with final tracked
  contents, including staged and unstaged changes. Changes that cancel disappear.
- **Two refs** compares snapshots directly (`git diff A B`); switching from the
  initial defaults suggests `HEAD~1` to `HEAD`.
- **Merge base** compares the unique common ancestor with the destination
  (`git diff A...B`); switching from the initial defaults suggests `main` to `HEAD`.

Branches, annotated tags, hashes and revision expressions are supported. Missing
refs remain editable errors. Bare repositories support committed comparisons
only. Refs are fixed to object IDs for each refresh; working contents can change
while being read. Refresh when working contents change during loading.
Untracked/ignored files, editing, staging, reverting and fetching are excluded.

The tab title shows the number of changed files after each successful comparison.
A session tag (`Diff #<tab ID>: N files`) carries the same count, including zero.
Each Diff tab updates only its own tag; hidden/closed tabs retain their last
successful count. Counts follow the selected comparison and do not poll in the
background.
Each file header has a **Viewed** checkbox that folds its patch and a separate
expand/collapse button. Viewed marks survive unchanged refreshes within the open
tab; changed patches reopen and clear their marks. Changing comparison settings
or reloading the application resets review state; marks and patch contents are
never stored on disk.

The filename filter runs locally on a collapsible folder tree. Clicking a file
jumps to its patch in a continuous page. File headers stay visible while their
patches scroll, and the tree highlights and reveals the current file, opening
collapsed folders when necessary. The diff panels follow the tree's folder-first
order. The bottom status bar shows comparison refs, refresh time and file counts.
Drag the divider beside the file tree
to resize it; its preferred width is saved in localStorage across tabs and restarts.
The divider also supports arrow keys, Home/End, and double-click to reset. Patches load serially and use
[react-diff-view](https://github.com/otakustay/react-diff-view) in unified mode,
with three context lines and diff coloring. Binary and metadata-only changes show
notices. Non-UTF-8 filenames fail explicitly. Git runs with literal arguments,
optional locking disabled, no external diff/text conversion or recursive
submodule diffs, and the SDK's 20-second/512-KiB-per-stream bounds. Oversized
files show an explicit error; retained patch text is capped at 8 MiB per comparison.

Inline **Compare** saves comparison settings only after success. Reference inputs
suggest up to 1,000 local branches, remote refs and tags, and also accept revision
expressions. **Tab → Configure…** contains only the repository folder. Activation and application focus refresh visible
tabs without polling. Hidden tabs cancel work and retain their display; failures
mark the previous comparison stale. Restart restores settings and reads fresh
results; patches and repository contents are never persisted.

```yaml
actions:
  - id: diff
    name: Diff
    kind: diff
    config:
      folder: /absolute/repository # omitted/null initializes from session.directory
      mode: mergeBase # auto | workingDirectory | refs | mergeBase
      baseRef: main
      targetRef: HEAD # unused in workingDirectory mode
```

Discover configuration with `ongleet --json tab types`, then use an action ID:

```bash
ongleet tab create --session SESSION_ID --action diff --config '{"mode":"refs","baseRef":"HEAD~3","targetRef":"HEAD"}'
ongleet tab configure TAB_ID --config '{"folder":"/absolute/repository","mode":"workingDirectory","baseRef":"HEAD","targetRef":"HEAD"}'
```

Run `mise exec -- npm run check:diff` for temporary-repository behavior checks.
`scripts/check-diff-native.py` exercises bundled WebKitGTK on a private Xvfb
server; see the validation guide for recorded evidence and remaining acceptance.
