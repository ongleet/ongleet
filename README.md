# DevCenter

DevCenter is a native Linux terminal session application with a compact sidebar of **projects containing sessions**, and **multiple plugin tabs per session**. Create, edit, reorder, select, and close sessions using a name and an existing directory, with a native directory picker. Create, rename, reorder, select, and close terminal tabs above the terminal. Switching tabs or sessions retains processes and emulator state. SQLite persists metadata and selection. Window close hides the app and its live terminals; launching the same binary again reopens the existing instance. A Unix-socket CLI controls tabs, sessions, projects and the application. Terminal text input is available through the CLI; local Ubuntu packages can be built as described below.

Session/tab metadata, ordering and selection are saved transactionally. Changing a session's base directory affects future launches for tabs without a directory override; it never changes a running process's directory. Session operations never create, move, or delete the directory. Missing saved directories remain in metadata and produce a clear launch error until corrected.

Each session has a vertical **⋮** menu for editing and archiving. Drag its grip to reorder; the drop indicator shows the insertion point. Keyboard users can use Alt+Up/Down while the grip is focused. Archiving removes the session from the active sidebar, stops its terminal processes after confirmation and retains its metadata for 30 days; the directory stays untouched.

The stack is Tauri 2, React + TypeScript, Rust, portable-pty, and pinned `ghostty-web` 0.4.0. There is no Electron, xterm.js runtime, tmux, Git worktree management, automatic Git discovery, or agent orchestration. Explicit session-linked GitHub PR status is supported.

The compact themed window title bar supports dragging and double-clicking to maximize or restore, with minimize, maximize/restore, and close controls. Close hides the window and keeps sessions running; use **Application → Quit** to shut down. Window size and maximized state are saved on close and Quit, then restored on startup. Position is restored when supported by the desktop (Wayland controls window placement). Minimized windows reopen visible. State is stored in `window-state.json` in the application data directory; missing or invalid state uses the default size.

## Archives

DevCenter manages archives centrally in SQLite for every plugin. Use **Session → Archives** in the top bar to restore sessions/tabs or permanently delete an archive after confirmation. Use **Tab → Configure…** to edit the active tab’s saved plugin configuration. Terminal changes apply to future launches and preserve running processes. Use **Tab → Archive tab** for the active tab, or its close cross, to archive its definition; the session menu offers **Archive session**. The CLI’s `tab close` remains a non-archiving operation that only closes an opening.

Archives expire **30 days after archival**, irrespective of when a session was last used. Cleanup runs at application startup and at each session/tab archival; if the application is closed, expired data is cleaned next time it starts. **Purge expired** or `archive prune --confirm` runs the same maintenance explicitly. An active session is never purged for being old or idle.

Restoration preserves stable IDs, names, working directories, plugin versions/configuration, openings, selections, Files editor paths and the session PR association. Restore an archived parent session before its separately archived tabs. Removed projects are recreated from their saved metadata when needed. Previously running terminal commands can launch in **new processes** when restored; confirmation in the GUI identifies those commands. Stopped/exited terminals do not become restart-eligible. Unsaved editor buffers and live terminal/process state are not archived; GUI guards still require confirmation before discarding edits. Missing working files/directories are never recreated.

Permanent deletion removes the archive, its DevCenter terminal text history and its DevCenter Codex mappings/lock files. It never deletes project/session directories, edited files, Codex’s own conversations or manual database backups. A durable deletion marker is committed before artifact cleanup; interrupted/failed deletion remains non-restorable and is retried at startup or the next cleanup. Failures appear in Archives, rather than preventing the application from starting. Archives created before this feature existed cannot recover already deleted metadata.

```bash
devcenter session archive 12 --confirm
devcenter tab archive 7 --confirm
devcenter archive list
devcenter archive restore 3
devcenter archive purge 3 --confirm
devcenter archive prune --confirm
```

## Run

Use mise for Node and Rust:

```bash
mise install
mise exec -- npm install
mise exec -- npm run check:host
mise exec -- npm run tauri -- dev
```

Ubuntu native prerequisites (system libraries are installed with apt, not mise):

```bash
sudo apt update
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
```

Terminal launch currently also requires Linux cgroup v2, a writable systemd user delegation at `/sys/fs/cgroup/user.slice/user-<uid>.slice/user@<uid>.service`, `cgroup.kill`, and pidfds. Unsupported systems report a clear launch error; there is currently no weaker fallback. This has been tested on the Ubuntu host recorded in `docs/VALIDATION.md`, not all Linux distributions. macOS support and an optional basic cleanup mode are future work.

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/). A working graphical desktop session is required. The prototype does not change shell or Codex configuration. Codex must already be installed, available on PATH, and configured by the user.

Open the **Session** menu in the top bar to launch **Bash**, **Codex**, **Files**, or **Pull request** directly. The same menu lists configured external actions. **Custom tab…** opens a name, plugin type and configuration form for a single new tab without changing project actions. Each opening keeps its own launch configuration and runtime state. Launch settings are edited under Edit project → Actions, or in its YAML tab. To replace an exited or legacy stopped tab, close it and open a new tab.

## Install a local Ubuntu package

Build a release package on the computer you want to test, after installing the
build prerequisites above:

```bash
mise exec -- npm run package:deb
sudo apt install ./src-tauri/target/release/bundle/deb/DevCenter_0.1.0_amd64.deb
```

Use the actual filename emitted by the build if its capitalization or architecture
is different. Launch **DevCenter** from the desktop application menu or run
`devcenter-gui`. The package installs `devcenter`, `dcodex` and the compatibility
commands on PATH. Bundled frontend assets require neither Node nor Vite at runtime.
Use **Application → Quit** to stop an existing development instance before testing
the installed build. Both builds use the same saved application data.

This is a local preview build, not a cross-distribution release. Packages built on
newer Ubuntu versions may require newer system libraries and will not necessarily
run on older Ubuntu/Debian releases. Terminal launches still require the systemd
cgroup support described above. Codex and `gh`, when used, must be installed and
available in the desktop application's environment.

Tauri also supports AppImage for running without package installation, but that
format has not been validated for DevCenter's process containment requirements.
Distribution to other machines also requires the applicable third-party notices
and Ghostty WASM source/build provenance described in [the stack guide](docs/STACK.md).

## Projects and actions (YAML)

A project has a name and an existing root directory. Use the sidebar's top **+** to create one, its **⋮** menu to edit it, and its own **+** to create a session inside it. New session paths default to that group's root; you can override the directory. Creating a session immediately opens and starts the first terminal action listed in its project YAML, using the action name for the tab. Put your preferred default first (Bash in the shipped configuration). Invalid YAML prevents creation instead of leaving an empty placeholder terminal. The session editor also lets you move an existing session to another project. Group root changes affect the default for future sessions, while existing session paths and running processes stay unchanged.

Click the group's chevron/name to fold or unfold its session list. Folding is persisted and only hides sidebar entries: it does not close sessions, change the active terminal, or interrupt output. Reorder sessions inside their group with their existing drag grips/keyboard shortcuts. A group can be deleted only after its sessions have been moved or closed. Session/root directories and group configuration files are never deleted by these operations.

Each project has one YAML file at `$XDG_CONFIG_HOME/devcenter/projects/<id>/actions.yaml`, normally under `~/.config/`. Open **Edit project** to see its exact path. The file is initialized on first Edit/New tab access with Bash, Codex, Files and Pull request defaults. Each file is independent. The top bar **Application** menu provides **New project…**, **Configure…** for default actions, and **Quit**. Default actions are edited as a form or YAML and stored at `$XDG_CONFIG_HOME/devcenter/actions.yaml`. They are copied when a project actions file is first initialized; existing project files keep their settings. The first terminal action starts in each new session. Invalid defaults prevent initialization and can be repaired in the default YAML editor. For the migrated Default group, for example: `vim ~/.config/devcenter/projects/1/actions.yaml`.

The Edit project modal has **General**, **Actions**, and **YAML** tabs. General edits the name/root. Actions provides a form for adding, removing, and reordering actions, with executable/arguments or explicit shell-command mode, optional directory and explicit environment overrides. The **Common environment variables** section provides shared name/value pairs for all actions in the project. Arguments use a JSON array of strings; environment overrides use a JSON object of strings. YAML edits the same list directly. Switching views transfers unsaved action edits between them; **Save actions** validates and atomically replaces that group's file. General settings use **Save project** separately. Existing openings keep their copied launch settings. An invalid YAML file opens in the YAML editor so it can be repaired.

Actions are the tab choices under **New tab**, which uses only the active session's project configuration. The action list uses `actions`, alongside optional common `env` values (older action-list names are rejected):

```yaml
env:
  PROJECT_MODE: development
actions:
  - id: bash
    name: Bash
    kind: terminal
    config:
      executable: bash
      args: []
      env: {}
  - id: codex
    name: Codex
    kind: terminal
    config:
      executable: dcodex
      args: []
      env: {}
```

Environment precedence is the inherited process environment, then the project’s common `env`, then each action’s `config.env`. Values are literal strings. Common variables are reloaded for each new process; editing them never changes an already running process. They live in the same project YAML file and are not merged into saved tab metadata.

IDs must be unique; names are shown in the picker. Tab actions support `kind: terminal`, `kind: files` and `kind: pull-request`; `kind: command` launches an external application without a tab. Terminal `config` also accepts `cwdOverride` and explicit `shellCommand`; shell-command mode cannot include `executable` or arguments. Executable mode passes arguments literally. The normal environment is inherited with only `env` overrides applied. Blank/omitted `cwdOverride` uses the session base directory. The file supports up to 100 types and 64 KiB. Commands must already be installed. Changes affect new tabs only; existing tabs keep their saved configuration. No session setup scripts or shell/Codex configuration changes are performed.

Use the arrow keys, Home, and End while focused on the tab bar to select tabs. Drag a tab to reorder it, or use Alt+Left/Right while it is focused. Each tab has a cross to close and archive its definition for 30 days; renaming controls are temporarily hidden. Archiving a tab with a running process requires confirmation. The configured-tab reopening selector has been removed; New tab creates a fresh opening from YAML. Copy the terminal selection with Ctrl+Shift+C and paste with Ctrl+Shift+V. Archiving a session retains its definitions in Archives and confirms termination of all its terminals. Hidden tabs continue consuming output with painting suppressed.

## Built-in tab plugins

Tabs use trusted plugins bundled into the application. Adding a plugin requires rebuilding DevCenter; there is no runtime installer or public SDK. The application owns navigation, stable IDs, ordering, selection and openings. Plugins own their configuration, content and resources. Terminal tabs retain the existing PTY, containment, replay, history and Codex behavior. Process status and restart eligibility apply only to terminals.

New project configurations include **Files** and **Pull request** by default. Existing project files keep their custom actions; add these entries through Edit project → Actions when absent, choose the action type and save. The same configuration can be added to the project's YAML `actions` list:

```yaml
- id: files
  name: Files
  kind: files
  config: {}
- id: pr
  name: Pull request
  kind: pull-request
  config: {}
```

`configVersion` defaults to 1. Action changes affect newly created definitions; existing definitions keep their copied configuration. New sessions still require and start their first configured terminal action. Closing a tab retains its definition; the CLI can reopen it with `devcenter tab open ID`. See `devcenter tab --help` for generic list/create/configure/rename/select/order/close operations. Raw terminal input and screen CLI operations remain deferred.

**Files** starts in the session directory unless `config.folder` supplies an absolute path. Search filters filenames in the current folder as you type (case-insensitive, including entries not yet loaded, without recursing into subfolders). Dotfiles and dotfolders are hidden by default; Ctrl+H toggles them in the active Files tab and reloads the list. It supports parent navigation, an editable folder path, refresh and an integrated plain-text editor (CodeMirror). Directory symlinks are followed, and browsing may leave the session directory. Successful navigation persists the current folder for restart. Each Files tab also remembers its open editor file in SQLite and reopens it from disk after application restart or WebView reload. Returning to Files clears the remembered editor file while keeping the folder. Missing or unsupported saved files recover their local draft when available; otherwise they show an error and offer Keep browsing, without recreating or externally launching them. Listings default to case-insensitive name A–Z with folders first; the sorting dropdown offers name, modification date and file size (bytes) in both directions, always with folders first. Unavailable modification dates or sizes sort before known values in ascending order. Sorting scans at most 100,000 entries and 16 MiB of names/paths, reporting an explicit error above either limit. Infinite scrolling appends sorted results in bounded batches of up to 200 entries and 128 KiB, with an offset limit of one million; concurrent directory changes can shift batch boundaries. Refresh reloads the list. Missing/inaccessible paths report errors without creating them. Non-UTF-8 filenames report an explicit error. Click a UTF-8 text file (up to 1 MiB) to replace the browser with its editor; Files returns to the same folder/list. The editor’s Actions menu contains Back to Files, Save (Ctrl+S), Reload, undo/redo and find/replace (Ctrl+F). It also provides automatic syntax highlighting based on the filename (including JavaScript/TypeScript, Python, Rust, HTML/CSS, JSON, YAML, Markdown and shell scripts), line numbers, Tab/Shift+Tab indentation and native clipboard shortcuts. Language parsers load on demand; unrecognized filenames remain plain text. Settings → Wrap long lines in the same menu applies to all editors and is remembered across restarts. Unmodified visible editors use Linux inotify notifications and reread on window focus; external changes refresh automatically without replacing local edits. Saving over external changes offers Cancel, Force save, or Merge into editor. The three-way merge applies local edits to the latest disk contents without writing the file, inserts conflict markers for overlapping edits, and updates the draft baseline so you can review, resolve and save. Further disk changes require confirmation again. Reload rereads the file; returning to Files asks before discarding unsaved edits; GUI tab/session closure and GUI Quit ask before closing modified editors, retaining their local drafts. Navigation to other tabs retains the editor. Binary files open in their associated applications. Uniform LF/CRLF and UTF-8 BOM are preserved; mixed/legacy line endings are rejected. Save checks the loaded revision and replaces the file atomically in its directory, preserving ordinary access permissions and group. It requires a writable, single-link file owned by you; symlinks resolve to their targets at opening. Extended attributes/ACLs are not copied to the replacement. External writers are checked again immediately before replacement, but there is no filesystem-wide compare-and-swap guarantee. Edits are backed up synchronously in WebView localStorage, keyed by tab ID and canonical file path, with the original file contents/revision for conflict detection after reload or restart. The detail bar shows a highlighted unsaved-changes label and whether the draft is stored locally. Saving, undoing all changes, or explicitly discarding via Reload/Back removes the draft. Local storage failures (including quota exhaustion) show an error while keeping edits in memory. Clearing application WebView storage removes drafts. There are no file creation, copy, move or delete operations.

**Pull request** stores one explicit GitHub `owner/repository` and PR number (or GitHub PR URL) on the session, shared by its PR tabs. Enter the association inside the tab and choose Save. The plugin checks the app’s launch environment for a nonblank `GH_TOKEN`, then `GITHUB_TOKEN`, before using a saved connection. Environment tokens validate automatically through GitHub and remain in memory; an invalid token reports an error without falling back. The connection panel identifies the variable in use. Set or change variables before launching DevCenter and restart the app to apply changes; variables exported in an already running terminal do not update the app. With neither variable set, connect manually with a personal access token; the plugin validates the account before saving it in Linux Secret Service. All PR tabs share one account across sessions, projects and restarts. Manually saved tokens require an unlocked Secret Service (such as GNOME Keyring or KWallet); unavailable or locked storage reports an error with no plaintext fallback. Existing users enter a token once; association/settings migration is unnecessary. There is no `gh` dependency or fallback.

The masked token field supports **Connect**, **Replace token** and **Disconnect**. Failed replacement preserves the old connection. For manually saved connections, Disconnect deletes the stored credential. For environment connections, Disconnect pauses the connection until Reconnect, tab reopening or app restart, preserving any saved token and the process environment. Both modes abort requests and clear displayed PR data and unread indicators across PR tabs; associations, account-specific review cursors and last-known PR state tags remain. It does not revoke the token at GitHub. Closing the last PR tab releases the in-memory account. Expired/revoked tokens pause polling until replaced; permission errors retain the account and explain the access problem.

Refresh runs on connection, opening, manually and every 120 seconds by default while a PR tab is open, including hidden tabs and sessions. Configure `refreshIntervalSeconds` (30–86400) inside the tab or in its action configuration. Successful refreshes show state, reviews, review decision, latest-commit check runs/commit statuses and a browser link; colored `PR: Open`, `PR: Closed` or `PR: Merged` session tags preserve sibling tags. Reviews and checks paginate independently, retaining GraphQL review IDs; either collection exceeding 10,000 entries rejects the whole refresh. Network/permission failures retain the last successful snapshot and refresh time. Rate limits defer requests according to GitHub retry/reset headers.

New submitted reviews by other accounts show **PR: New review** and reviewer details. The first successful fetch establishes a baseline. **Mark reviews read** persists progress locally per session, PR and GitHub account, shared by PR tabs. Pending reviews, deleted authors, standalone comments and edits to existing reviews do not create notifications. Clearing the association removes PR state/unread tags; closing the last PR tab stops polling while retaining the last-known state tag. Authentication is GitHub.com only, token-only and read-only; automatic branch discovery, OAuth and PR mutations remain deferred.

For a fine-grained token, select the repository owner and repositories, with **Pull requests: read**, **Contents: read**, **Commit statuses: read**, and **Checks: read** where available (metadata read is included). These permissions correspond to PR/reviews, commit and check/status fields queried by the plugin. Fine-grained PAT access to GraphQL check runs can still be restricted; if the token settings do not offer Checks or GitHub denies these fields, use a permitted classic-token alternative. A classic token uses `public_repo` for public repositories or `repo` for private repositories; these scopes are broader than this plugin's read-only operations. Repository visibility follows token grants, organization approval/SSO and organization policies. See [GitHub GraphQL authentication](https://docs.github.com/en/graphql/guides/forming-calls-with-graphql), [token permissions](https://docs.github.com/en/rest/authentication/permissions-required-for-fine-grained-personal-access-tokens), and the [GitHub CLI report on fine-grained check access](https://github.com/cli/cli/issues/12597). Live public/private repository acceptance remains separate from fixture validation.

Schema version 4 wraps existing `config_json` values with a configuration version. Schema version 5 adds the session PR association transactionally and also handles intermediate schema-4 databases that lack that column. Existing IDs, tab openings, ordering, selections, terminal settings and restart flags are preserved. Missing plugins or unsupported plugin versions display an unavailable panel while retaining their opaque configuration; they do not block metadata restoration. All open plugin tabs restore independently of terminal process restart policy.

## Terminal mouse, clipboard and links

Drag to select terminal text; releasing the mouse copies the selection to the native clipboard and the desktop primary selection. Ctrl+Shift+C copies it explicitly. Middle-click pastes the primary selection. A click without a drag preserves existing clipboard contents. When a program requests mouse reporting, clicks, drags and wheel events go to that program; hold Shift while dragging to select text locally instead.

Ctrl+Shift+V pastes native clipboard text with bracketed-paste framing, or quoted local paths when files were copied from a file manager (up to 32 files and the terminal input limit). It does not copy file contents into the terminal. Image clipboard contents forward Ctrl+V to the running program. In Codex, Ctrl+V also reaches its own image-paste handler directly; image decoding and attachment remain Codex's responsibility. In shells, Ctrl+V retains its conventional literal-next-character behavior. File drag-and-drop is not implemented.

Ctrl+click opens terminal HTTP(S) links in the default browser and local `file://` links in their associated application. This includes labeled OSC 8 links emitted by Codex. File-line navigation depends on the associated application; missing files report an error. Only these URL schemes are accepted. The frontend retains bounded OSC 8 URL metadata because ghostty-web 0.4.0 exposes hyperlink cell IDs but its URL lookup is a stub. Older link metadata can become unavailable after its 4,096-entry / 4 Mi-code-unit budget is exceeded; there is no second terminal emulator.

## WebView reload

Reloading the interface reconnects to existing PTYs instead of launching duplicate processes. This also reconnects exited sessions so their retained output can be inspected. Rust retains a bounded in-memory byte/resize journal and replays it into the new Ghostty frontend with the same ACK backpressure used for live output. Terminal-generated responses and user input are suppressed during replay so old terminal queries cannot send duplicate replies into the process.

The journal has a 4 MiB accounting budget per session; replay temporarily holds another bounded copy. If the budget is exceeded, the journal is discarded and reload explicitly warns that the previous screen is unavailable. Only subsequent output is shown, and terminal modes may need the running program to redraw/reinitialize. Selection and scroll position are not restored. This journal is never saved to SQLite or disk and disappears when its session is closed or the application exits. It does not restore processes after a crash or an application restart.

## Persistence and command restart

At the user's request, only tabs that were running when the app last closed automatically launch again. These are **new processes**, not recovered live sessions; previous terminal text is restored as history, without restoring process or emulator state. Tabs explicitly stopped/closed and processes observed to have exited do not auto-start. Stopped open tabs restore as stopped. A crash uses the last committed running set. Startup waits for connected terminal frontends; failed launches report errors without an automatic retry loop.

### Persistent terminal history

Every terminal tab automatically saves a plain-text snapshot of its current Ghostty buffer, including available scrollback. No wrapper or shell configuration is required. After Quit, reboot or a crash, the saved text appears above a `Previous terminal history; new process below` separator. Eligible tabs then launch a fresh process; stopped tabs display their saved history without launching. Live WebView reloads retain the original history prefix and replay the current process without duplicating that prefix.

Snapshots retain up to 2,000 lines and approximately 60,000 UTF-16 code units (at most 240 KiB UTF-8). They are updated at most once per second while output arrives, after observed EOF, and just before the GUI Quit action. A crash, abrupt shutdown or CLI Quit can lose recent output since the last snapshot. This is a snapshot of retained terminal text, not a complete command log: cleared/discarded output is unavailable, and full-screen programs save their active screen. Colors, terminal modes, selection and process state are not saved. New programs may clear the restored display using their normal terminal controls.

Files live under `terminal-history/<session-id>/<tab-id>.txt` in application data, keyed by stable IDs, with mode 0600 and atomic replacement. They contain displayed terminal text, which can include sensitive output. Files are bounded per tab and remain after closure, like Codex conversation mappings. No terminal output is added to SQLite. Previously existing tabs begin recording after this version runs; earlier output cannot be recovered from disk.

### Codex conversation restoration

The shipped Codex action uses `executable: dcodex`. This opt-in wrapper starts Codex normally on first launch and uses `codex resume <conversation-id>` on subsequent launches of the same stable session/tab definition. New tabs have independent conversations. Existing project YAML and saved tab configurations are not rewritten: change the Codex action's executable from `codex` to `dcodex` in Edit project → Actions, then open a new Codex tab. Existing plain `codex` tabs retain their original behavior.

The app supplies session/tab IDs and its actual data directory, then runs the wrapper through `/proc/<app-pid>/exe`. It does not require a separately built helper. Command-line `SessionStart` and `UserPromptSubmit` hooks record Codex's reported ID, including changes after switching/forking sessions, into `codex-sessions/<session-id>/<tab-id>.json` under application data. Writes are atomic and mode 0600, with a per-launch generation rejecting older hooks. Files contain conversation/launch/turn/tool-call identifiers and bounded hook activity metadata, never prompt text, transcripts, credentials, or inherited environment. They remain after tab/session closure; IDs are not reused. Codex stores its own conversation history separately under its configured `CODEX_HOME`.

Use a Codex version supporting `SessionStart` command hooks, inline hook configuration, `--dangerously-bypass-hook-trust`, and `--no-daemon` (0.163.0 was exercised locally). The wrapper enables hooks for its invocation and adds recording hooks alongside existing hook sources. Like Agent Orchestrator, it passes **`--dangerously-bypass-hook-trust` for that invocation** because session-flag hooks lack a durable source trust hash. This permits **all enabled hooks**, including other user/project/plugin hooks, to run without the usual hook review. Choose this action only when those hook sources are acceptable; use plain `codex` to retain the normal trust flow. It does not edit global/project Codex configuration. The wrapper uses `--no-daemon` to keep the process within terminal containment. Live work stops on app Quit/crash; the next launch resumes the saved conversation in a new process.

`dcodex` uses an ordinary session tag named **Codex**. Only its dot changes: blue (`#60a5fa`) for Working, amber (`#f59e0b`) for a pending hooked question, orange (`#f97316`) for Approval, green (`#22c55e`) for Idle, and grey (`#94a3b8`) for Stopped. It tracks each open tab independently and prioritizes Question, Approval, Working, Idle, then Stopped across the session. The tag is added/updated through the same tag operations as CLI/SDK tags and is included in their lists; there is no separate sidebar badge. Its default priority is 100; later updates preserve its configured priority and background. Closing the last tracked opening removes it. Hooks overwrite the dot of a tag named Codex, so use another name for unrelated tags.

Updates use the private application socket and validate the current launch generation. Failures do not block Codex; `DEVCENTER_DEBUG=1` reports them. Hook updates ignore subagents and stale turn completions, and retain pending questions/approvals across unrelated tool completions. Hooks are injected only for this invocation. Disabling/replacing these hooks prevents tracking. Actual `request_user_input` hook delivery remains to be verified in live Codex; a question written as ordinary final text appears as Idle, and permission hooks can run even when another hook resolves approval automatically. Stop hooks that continue a turn may briefly show Idle before the continuation prompt. Like other tags, the last dot is saved; a fresh wrapper launch updates it to Idle.

`dcodex` supplies brief developer instructions describing DevCenter, its CLI and the launch-session environment variables, including the distinction between a DevCenter session and a Codex conversation. These instructions leave the user's first prompt untouched. They override the configured `developer_instructions` value for this invocation; an explicit forwarded `-c developer_instructions=...` can replace them. No global Codex configuration is changed.

While the tab stays open, `dcodex` supervises Codex and automatically resumes its latest recorded conversation two seconds after any exit, including `/quit`, Ctrl+D, errors and signals. Close the tab or Quit DevCenter to stop it. `--no-restart` runs Codex once and preserves its exit code. `--fresh` applies only to the initial launch within this wrapper invocation; automatic retries resume when an ID is saved, otherwise they start a new conversation. Five consecutive runs lasting less than ten seconds stop the restart loop with an error. An executable launch failure also stops it. Exiting before a conversation ID is recorded (for example during startup or an update) no longer prevents restarting. This does not recover unsent drafts or in-flight work, and cannot restart a killed wrapper or a stopped application.

Arguments are optional: use `args: ["--", "--model", "MODEL"]` to forward Codex options. To deliberately replace a conversation, run the standalone helper with the same session/tab IDs and `--fresh` while that tab is stopped. A configured `args: ["--fresh"]` deliberately starts a fresh conversation on every launch. A missing/unavailable Codex conversation reports Codex's error instead of silently starting a new one. Malformed mapping files also fail without resetting the file; move the damaged file aside explicitly to recover.

For use outside a configured tab, build the standalone helper with `mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --offline --bin dcodex`, then run `src-tauri/target/debug/dcodex --session-id 12 --tab-id 34`. It uses the standard application data directory and current working directory. Use the same XDG data and Codex home settings to find the same mapping/history. This standalone command does not start the GUI or automatically restore app process containment.

Schema version 2 originally migrated workspaces into a **Default** workgroup without changing session/tab IDs, paths, settings, selection or resume flags. Its initial root is the first saved session directory (your home directory on a fresh installation). Project IDs, names, roots, ordering, fold state and session membership are saved with the rest of the metadata. A failed migration rolls back rather than resetting data.

Schema version 3 renames workgroups to **projects** and workspaces to **sessions**, preserving IDs, directories, ordering, selection, tab definitions/openings and restart flags. Startup migrates the schema automatically while holding the writer lock and moves the configuration tree from `workgroups/` to `projects/`. Conflicting configuration trees are reported without merging or overwriting. Existing YAML is preserved verbatim: legacy `{{workspace.*}}` / `{{workgroup.*}}` placeholders and `WORKSPACE_*` / `WORKGROUP_*` context variables remain supported alongside the new names. `dcodex --workspace-id` remains an alias for `--session-id`. The database filename and `codex-sessions/` mapping paths stay unchanged to preserve existing data and conversation associations. The renamed control protocol uses version 2; rebuild the GUI and CLI together.

To migrate without launching terminals, quit DevCenter and run `devcenter-gui --migrate`. Migration refuses an active metadata writer. Database migrations are transactional and failures never reset metadata.

The database is `workspaces.sqlite3` in DevCenter's application data directory, normally `$XDG_DATA_HOME/devcenter/` or `~/.local/share/devcenter/`. Versioned migrations are in `src-tauri/migrations/`. Definitions (including their kind/configuration) and open tabs are stored separately. Only explicit environment overrides are saved, never the inherited environment or terminal output in SQLite. Terminal text snapshots are stored separately as described above. New tabs persist a copy of their YAML launch configuration.

DevCenter stores configuration in `$XDG_CONFIG_HOME/devcenter/` (normally `~/.config/devcenter/`) and data in `$XDG_DATA_HOME/devcenter/` (normally `~/.local/share/devcenter/`). On first startup it moves the previous `io.local.terminal-stack-prototype` directories to these paths, preserving project YAML, database sidecars, IDs and Codex conversation mappings. Quit the previous application first. Migration refuses a live legacy writer or an existing destination rather than merging or overwriting files. You can run `devcenter-gui --migrate` to migrate without opening a window. It is safe to repeat after a successful move.

The short commands are `devcenter` for session/application control and `dcodex` for Codex conversation restoration. `terminal-workspace` and `devcenter-codex` remain compatibility aliases, and saved tab definitions using the old wrapper name still work. These commands are available in application terminals after rebuilding/restarting; the Ubuntu package also installs them in `/usr/bin`.

Database failures are reported rather than resetting data. An exclusive owner-only lock prevents a second app from overwriting metadata or relaunching commands twice; a second launch focuses the first window through a private activation socket. Metadata is saved as changes occur, so this version does not require a manual Save action for session/tab operations. Earlier in-memory versions did not write a database; they have no saved data to migrate automatically.

Ordinary React renders do not recreate the terminal. Closing the window hides it; terminals keep consuming output. Launch the same binary again to reopen it, or use `src-tauri/target/debug/devcenter-gui --show`. `--show` fails if the application is unavailable and never starts it. The footer **Quit** action warns before terminating running processes. Reopening a hidden window preserves live processes and screens; restarting after Quit launches eligible commands as fresh processes under the restart policy above.

Each terminal joins a private cgroup before the requested executable runs. Confirmed close and Quit use immediate SIGKILL for the whole cgroup, including ordinary detached/setsid descendants, and wait for cleanup. There is no graceful shutdown interval: save application work before confirming. A per-session watchdog also cleans up after application death or command-leader exit. It ends with its session; there is no persistent daemon. This is lifecycle containment, not a security sandbox against same-user programs deliberately moving into another cgroup or delegating work to external services.

The control socket is mode 0600 under `/run/user/<uid>` (required owner-only), validates peer UID on both ends, and accepts versioned bounded JSON requests. Ubuntu packaging provides the desktop launcher; launching the same executable already performs activation.

## External actions

In **Edit project → Actions**, choose **External command** as the action type. Use the **Actions** button beside **New tab** to launch it. New tab lists terminal actions only, and new sessions automatically start the first terminal action even when external actions precede it. A project needs a terminal action to create a session.

External commands launch without a PTY or application tab. They inherit the desktop application's environment, apply common project variables and then action-specific overrides, and default to the session directory. Launch errors and nonzero exits are reported while the frontend is connected; stdout/stderr are discarded. Launch acceptance does not guarantee a visible window, particularly for applications that delegate to an already-running instance. Install/configure the external application yourself; actions never modify shell configuration.

Example entries in a project's `actions` list:

```yaml
- id: vscode
  name: Open in VS Code
  kind: command
  config:
    executable: code
    args: ['--new-window', '{{session.path}}']
- id: desktop-terminal
  name: Open external terminal
  kind: command
  config:
    executable: gnome-terminal
    args: ['--working-directory={{session.path}}']
```

Choose an installed executable and its supported arguments. These examples are opt-in; Bash and Codex remain the shipped defaults. Direct executable mode passes argument arrays without shell evaluation. External action executable, arguments, directory override and environment values support `{{session.path}}`, `{{session.name}}`, `{{session.id}}`, `{{project.path}}`, `{{project.name}}`, and `{{project.id}}`. Values come from current session metadata at launch, not a shell's current directory. Substitution is a single literal pass, preserving spaces, Unicode, braces and shell metacharacters inside argument values. Unknown or unclosed placeholders fail the launch. Tilde and arbitrary environment-variable expansion are not supported. Placeholder expansion is currently for external actions only.

Shell-command mode instead exposes `SESSION_PATH`, `SESSION_NAME`, `PROJECT_PATH`, and `PROJECT_NAME` as reserved environment variables; for example `code --new-window "$SESSION_PATH"`. Placeholder syntax is rejected inside `shellCommand` to avoid interpolating a path into executable shell syntax. Arguments containing shell programs are still your responsibility; prefer direct executable mode. The reserved context variables take precedence over configured values of the same names. Common environment values are also interpolated when used by an external action; terminal actions retain their existing literal environment behavior.

External applications are deliberately independent of managed terminal sessions. The app does not terminate them on session close or Quit, and never restores them. Their ultimate lifetime also depends on the desktop/session manager or the launched application. The app reaps directly launched children while running and permits at most 64 outstanding direct children; launcher applications may delegate to desktop services. External processes are excluded from terminal running counts and close/Quit confirmations. These actions are explicit launches and are never run automatically on session creation or restoration.

## CLI

Build the frontend with `mise exec -- npm run build`, then the GUI and command-line executables with `mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline`. Restart the GUI after updating it to load the control protocol. App-launched terminals automatically prepend the application executable directory to PATH, so `devcenter` is available there when built alongside the GUI. This does not change your personal PATH or shell configuration. Explicit project/action PATH overrides take precedence, and shell startup files may replace PATH. Existing terminals retain their environment; open a new tab after restarting the updated app.

The CLI is `src-tauri/target/debug/devcenter`. Use `--help` for a short resource index, `session --help` for its command list, or `session create --help` for that command’s usage, effects and relevant defaults (`-h` also works). The separate `automation --help` topic documents JSON responses, ID discovery, exit codes and timeout/retry behavior. Help runs without connecting to the application. Outside app terminals, substitute that path for `devcenter` unless the Ubuntu package is installed:

```bash
devcenter project list
devcenter session list
devcenter session create --project 1 --name Example --directory /existing/project
devcenter session tags 2
devcenter session tag-add 2 --name Ready --dot '#22c55e' --priority 10
devcenter session tag-add 2 --name Backend --background '#1e3a5f' --dot '#60a5fa' --priority 5
devcenter session tag-remove 2 --name Ready
devcenter session rename 2 --name Backend
devcenter session directory 2 --directory /existing/other-project
devcenter session reorder 2 --position 0
devcenter session select 2
devcenter session close 2 --confirm
devcenter tab types
devcenter tab actions --session 3
devcenter tab list --session 3
devcenter tab create --session 3 --action bash --name 'Agent shell' --config '{"executable":"/bin/bash","args":[],"env":{}}'
devcenter tab rename 7 --name Backend
devcenter tab configure 7 --config '{"executable":"/bin/bash","args":[],"env":{}}'
devcenter tab close 7 --confirm
devcenter tab open 7
devcenter tab delete 7 --confirm
devcenter app show
devcenter app status
devcenter app quit --confirm
```

Every newly launched terminal receives reserved `DEVCENTER_SESSION_ID`, `DEVCENTER_PROJECT_ID` and `DEVCENTER_TAB_ID` variables. Inside it, `devcenter tab actions`, `tab list` and `tab create --action ACTION_ID` default to that session; `session create --name NAME` defaults to that project. Explicit `--session`/`--project` takes precedence. Outside a terminal, supply the option or set the variable explicitly. Missing/invalid context fails; the CLI never guesses from the GUI selection. Destructive commands still require explicit IDs. Context reflects launch time: project moves do not update running environments, and terminals already running before this change need relaunching.

Session tags appear in the sidebar and in `session list` / `session config`. The tag API only lists, adds/updates one name, or removes one name: `session tags ID`, `session tag-add ID --name NAME [--background COLOR] [--dot COLOR] [--priority NUMBER]`, and `session tag-remove ID --name NAME`. Names are unique case-insensitively, trimmed and limited to 1–40 characters without controls. Adding an existing name replaces its style and priority without creating a duplicate or changing other tags. Removing a missing name succeeds without changes. Background and dot are independent optional `#RRGGBB` colors; omitted colors are absent. Text contrast adapts to the background. Priority defaults to 0 (signed 32-bit integer); larger values come first, preserving existing order among ties. Up to 16 tags persist across restarts and archival/restoration. Bulk replacement is not exposed by the CLI or SDK.

IDs are stable positive integers returned by list/create, not names or list indexes. Reorder positions are zero-based within the session's project. Omitting `--directory` on creation uses the group's root; relative paths resolve from the CLI's current directory. Creating a session selects it and opens its first configured terminal action through the connected frontend, including when the window is hidden. The response may report `starting` before that frontend launches the process; a disconnected/reloading frontend delays launch. Poll `session list` for observed process status. Directory changes affect future launches and never change an existing process's cwd or create/move/delete directories.

Project commands support `create --name NAME --root PATH`, `list`, `rename ID --name NAME`, `directory ID --directory PATH`, `fold ID`, `unfold ID`, and `close ID` (empty groups only). `session move ID --project ID` changes membership while preserving running processes. UI and CLI call the same Rust registry methods; successful CLI changes refresh the sidebar automatically.

Commands print readable text by default; errors go to stderr. For scripts and agents, pass `--json` before the resource or after command arguments: successes are JSON on stdout (`{"ok":true,"data":...}`) and failures are JSON on stderr (`{"ok":false,"error":{"code":"...","message":"..."}}`). For example, `devcenter --json tab list`. Help remains plain text. Exit codes: 0 success, 2 invalid arguments/request, 3 application unavailable, 4 missing ID, 5 confirmation required, 6 operation failed, 7 permission denied, 8 protocol/size/timeout. Running-session closure and Quit require `--confirm`; without it no process is terminated. A timeout can occur after a mutation was committed: check state before retrying a creation.

Project and session configuration is available through the same private socket:

```bash
actions_file=$(mktemp /tmp/devcenter-actions.XXXXXX.yaml)
devcenter --json project config 2 | jq -r '.data.yaml' > "$actions_file"
vim "$actions_file"
devcenter project configure 2 --file "$actions_file"
devcenter session config 12
devcenter session configure 12 --name "Feature work" --directory /path/to/existing/directory --project 2
devcenter session pull-request 12 --target https://github.com/owner/repository/pull/123
devcenter session pull-request 12 --target owner/repository --number 123
devcenter session pull-request 12 --target none
```

Use IDs from `project list` and `session list`. Configuration commands require explicit IDs and do not follow GUI selection. `project config` returns metadata and verbatim actions YAML (including common `env`); `project configure` replaces the complete document after validation, using the same atomic save as the GUI. The file limit is 64 KiB; the encoded socket request limit is 128 KiB. Use `project rename` and `project directory` to change the name or root. `session configure` updates the supplied name, directory and/or project together, preserving omitted properties, tab definitions and running processes. Directory changes apply to future launches. `session config` includes the linked pull request; association changes neither contact GitHub nor open a PR tab. Existing `tab actions` / `tab create` calls reload saved project actions.

Ordinary commands never start the application. Only `devcenter app start` starts the sibling GUI executable, or use `app start --executable /path/to/devcenter-gui`. Existing instances are reused. It needs the same desktop session, user and XDG data directory as the GUI. The CLI itself runs without a display and never opens SQLite. Tab IDs and action IDs above are examples: use the IDs returned by your app and `tab actions --session ID`. This command returns the project's available tab action IDs, names, plugin kinds, versions and copied configurations, excluding external commands and unavailable plugins. `tab types` returns each built-in plugin's configuration schema, defaults, constraints and valid JSON examples. `tab create` copies a configured plugin action and opens a fresh tab. Optional `--name` and `--config JSON` override its name and complete configuration before the first launch. Configuration is a replacement object, not a patch; omitted fields use plugin defaults. Both creation and later configuration use the same plugin validators. The complete encoded CLI request must fit below 128 KiB. `tab rename` changes its name; `tab configure --config JSON` replaces its plugin configuration (terminal changes apply to future launches). `tab list --session ID` includes open and closed definitions, configuration, selection, restart eligibility and observed status. `tab close` keeps the active definition for `tab open`; `tab delete` / `tab archive` moves it into Archives for 30 days. Both require `--confirm` when a terminal is running and preserve working files. `session close` / `session archive` archives the entire session. Tab reorder/select are also available. Session list includes open tab IDs/names/statuses; terminal text input is available through `tab input`; output snapshots remain pending.

## Checks

Use `mise exec -- npm run format` to format supported source and documentation
files with Prettier, and `mise exec -- npm run format:check` to check formatting
without changing files. Rust formatting remains managed by `cargo fmt`.

```bash
mise exec -- npm run format:check
mise exec -- npm run build
mise exec -- npm run check:wasm
mise exec -- npm run check:terminal-reuse
mise exec -- npm run check:rendering
mise exec -- npm run check:engine-loader
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Rust commands require a populated Cargo cache; omit `--offline` for the initial fetch. `check:wasm` uses the exact installed WASM and production byte adapter. It checks split UTF-8 and escape sequences, alternate-screen restoration, mouse/paste modes, terminal replies, oversized-chunk rejection, 100,000 output lines with bounded scrollback, and resizing. It does **not** certify browser rendering, keyboard input, clipboard behavior, or interactive Codex.

The Rust checks use actual Linux PTYs and a Tauri channel callback to exercise byte transport, explicit environment overrides, exit status, ACK backpressure, resizing, input, launch failures, and termination of a child in a separate job-control group. Session/tab checks cover identities, ordering/selection, directory non-mutation, independent sibling PTYs, literal arguments versus explicit shell syntax, directory inheritance/overrides, and confirmation before closure. A login-shell integration test may require running outside a restrictive sandbox if the user's shell startup files need desktop access. The rendering check uses real independent Ghostty engines and an instrumented drawing boundary to verify hidden output updates without drawing and redraw on return; native visual behavior still needs manual checks.

Use [the native validation checklist](docs/VALIDATION.md) before proceeding to the full application. Findings and design boundaries are in [the stack evaluation](docs/STACK.md).

## Development server unavailable

The development WebView loads terminal WASM lazily when a terminal frontend connects. If Vite has stopped, an already-open window can still display its interface but cannot fetch the terminal engine. Start the application with `mise exec -- npm run tauri -- dev`, which starts both Vite and the Rust app. After a temporary asset-load failure, a new tab retries instead of retaining the failed engine request. If restarting the whole development app, close the previous window using its confirmation flow first. Installed builds load their bundled assets and do not require Vite.

## Native stability checks

The repeatable [native validation procedure](docs/VALIDATION.md#reproduce-the-native-checks) runs an isolated bundled Tauri application through WebKitGTK WebDriver, using temporary metadata instead of your saved sessions. It covers reload/reconnection, hidden output, native clipboard shortcuts, actual PTY resizing, repeated tab creation and unsent Codex prompt editing. A special frontend build exposes bounded diagnostics only for that test mode; normal builds contain no validation bridge.

The browser mouse adapter now forwards reports to terminal applications; the isolated Wayland curses check passes. Native pointer selection/copy, image/file clipboard detection and OSC 8 link activation also pass on this host. Actual Codex image attachment and remaining manual/X11 checks are still open; the full terminal validation gate is not complete. Closed-tab cell reuse was reproduced and the app now resets new emulators after their initial fit before attaching output. No terminal engine was replaced.

## Frontend plugin SDK

Terminal, Files and Pull request use the scoped JavaScript SDK backed by reusable Rust core services. Bundled JSON manifests drive frontend registration, configuration validation and CLI discovery. The SDK example demonstrates filesystem, literal subprocess arguments and versioned storage without a Rust provider. See [plugin development](docs/PLUGINS.md) for the API, persistence/migration contract and adding capabilities.

### Sending terminal text

```bash
devcenter tab input 42 --text 'echo hello'
devcenter tab input 42 --text 'echo hello' --enter
```

Use a running terminal tab ID from `tab list --session ID`. Input works while hidden and does not require a connected frontend. Text is sent verbatim as UTF-8; `--enter` appends a carriage return. Embedded newlines/control characters retain their terminal meaning, so multiline text may execute commands even without `--enter`. Each request accepts at most 8 KiB including Enter and uses the existing bounded PTY input queue. Errors report oversized input, unavailable terminals or a busy queue. Success acknowledges the PTY write, not command completion. After a timeout or write failure, input may have partially or fully arrived; inspect the terminal before retrying.

Bundled JavaScript plugins can use `sdk.pty.sendText(runtimeId, 'echo hello', { enter: true })` for their own attached terminal. `sdk.pty.input(runtimeId, bytes)` remains available for raw bytes. This SDK is scoped to the calling tab; it is not a standalone Node.js client.

### Investigating native crashes

Launch a rebuilt GUI with `DEVCENTER_DEBUG=1 devcenter-gui` (development: `DEVCENTER_DEBUG=1 mise exec -- npm run tauri -- dev`). Quit any existing instance first: launching again only shows the running process and cannot enable its diagnostics.

Debug mode writes timestamps, WebKit resource/load and process-termination events, window/application lifecycle events, SDK operation names and durations, and a 30-second GUI heartbeat with resident memory/thread counts. Logs live in `$XDG_DATA_HOME/devcenter/` (default `~/.local/share/devcenter/`): `debug.log` and `debug.previous.log`, each capped at approximately 4 MiB. Startup preserves the preceding log; rotation replaces the older backup. Writes are unbuffered so completed breadcrumbs survive a process abort. Logging is best effort and may skip concurrent events. SDK completion records indicate that an operation returned or unwound, not whether it succeeded.

These files exclude SDK payloads, URLs, credentials and terminal contents. Existing native keyboard/focus diagnostics still go to stderr and may reveal typed input. To retain those and native allocator/GTK messages as well, launch with `DEVCENTER_DEBUG=1 devcenter-gui 2>devcenter-native.log`; this separate stderr capture is not rotated. After a crash, preserve both debug files, stderr capture if used, and Ubuntu's `/var/crash/_usr_bin_devcenter-gui.1000.crash` before restarting repeatedly. Breadcrumbs help correlate activity; heap corruption still requires a core dump or instrumented reproduction to identify the original invalid access.
