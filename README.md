# Ongleet

**An agent-first IDE. A workspace you and your agent can both drive.**

Ongleet combines terminals, files, diffs, notes and pull request status in a native
Linux workspace. Use the GUI, let an agent drive through the CLI, or use both.

Agents use the CLI to organize projects and sessions, configure tabs, inspect state
and run plugin actions. GUI and CLI control the same workspace.

[Get started](#get-started) · [CLI workflow](#let-your-agent-drive) ·
[Architecture](#built-for-continuity-and-control) · [Usage guide](docs/USAGE.md)

## One workspace for the whole task

Organize work as **projects → sessions → tabs**. Keep agents, shells, files and
reviews in one session. Switching tasks preserves running terminals and screens.

| Tab              | What it brings to your workflow                                                                                                              |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| **Terminal**     | Run your shell, Codex or another terminal tool with Ghostty terminal emulation. Hidden terminals keep consuming output.                      |
| **Files**        | Browse a folder tree and edit with syntax highlighting, recoverable local drafts and conflict-aware saves.                                   |
| **Diff**         | Review branch, commit and local changes in a continuous view, with file navigation, viewed markers and discussions you can send to an agent. |
| **Note**         | Keep autosaved Markdown notes, TODOs and agent memory in a session notebook. Agents can read and update notes with revision checks.          |
| **Pull request** | Follow an explicitly linked GitHub PR, its checks and reviews alongside your work.                                                           |

Select code in Files or Diff, add a question, and **send it to an agent tab** with
its path, line range and surrounding context. Files can send unsaved buffer text,
so you can discuss a change before saving it.

Session tags surface activity and review status in the sidebar. Reorder sessions
and tabs, fold projects, and choose from six appearance presets shared by the app,
editor and diff viewer. New terminals use the selected palette; existing terminal
cells retain their launch colors.

## Let your agent drive

Agents discover available tools through the CLI.

```bash
# Discover the workspace and available tab configurations.
ongleet project list --json
ongleet session list --json
ongleet tab types --json

# Inside an Ongleet terminal, these use the current session automatically.
ongleet tab actions --json
ongleet tab create --action bash --name 'Test runner' --json
ongleet tab list --json

# Replace 42 with a tab ID returned by the application.
ongleet tab capabilities 42 --json
ongleet tab state 42 --json

# Send a command to a running terminal, even while it is hidden.
ongleet tab input 42 --text 'git status --short' --enter
```

The CLI also manages project configuration, session directories, tab selection,
ordering, tags, archives and app lifecycle. Plugins declare structured
state and actions: an agent can open a file in Files, read a terminal buffer,
update a notebook, or mark reviewed files in Diff. Discover the target first:

```bash
# Use the ID of an open Files tab.
ongleet tab capabilities 43 --json
ongleet tab invoke 43 open-path --args '{"path":"/absolute/project/src/main.tsx"}' --json
```

Commands use JSON, stable IDs, validated arguments and explicit errors. GUI and
CLI share core operations, so the interface follows agent changes. Terminals
receive project, session and tab IDs through environment variables. The CLI never
guesses context from the selected tab.

Choose which agents to launch and what to delegate. Shells and manual editing
use the same workspace.

See [CLI usage](docs/USAGE.md#cli), `ongleet automation --help` and
[discoverable plugin interactions](docs/PLUGINS.md#discoverable-tab-interactions).
State and actions depend on each plugin's declared capabilities and an attached
open tab. Terminal input acknowledges a write, not command completion; inspect
state before retrying an action after a timeout.

## Built for continuity and control

Ongleet uses **Tauri 2, React, TypeScript, Rust, SQLite and Ghostty WASM** through
pinned `ghostty-web` 0.4.0. Each layer has one role:

| Design                                                            | Benefit                                                                                                                           |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| **Rust owns PTYs and process lifetimes.**                         | Changing tabs does not restart commands. A WebView reload reattaches to existing terminals.                                       |
| **Ghostty owns terminal parsing and screen state.**               | Shells and interactive terminal agents share a real terminal engine. Hidden tabs process output without painting.                 |
| **Bounded transport with backpressure.**                          | Terminal output moves through explicit queue and replay limits, keeping buffering bounded during heavy output.                    |
| **GUI and CLI share Rust operations over a private Unix socket.** | People and agents manipulate the same application state, with current-user access checks and no direct CLI database writes.       |
| **SQLite persists workspace metadata and plugin data.**           | Stable identities, layout and notes survive restart. Archives retain session and tab definitions for 30 days.                     |
| **Linux cgroups contain managed terminal processes.**             | Confirmed closure cleans up terminal descendants, including ordinary detached children.                                           |
| **Bundled plugins use a scoped SDK and shared manifests.**        | Configuration schemas and declared interactions are discoverable by agents; plugins share native services and lifecycle handling. |

The desktop uses WebKitGTK rather than shipping Electron. There is no tmux layer
or persistent daemon. Installed builds bundle their frontend and terminal assets,
with no Node or Vite runtime requirement.

Restart can relaunch eligible commands as **new processes**. Archives keep session
and tab definitions for 30 days, without live processes or terminal checkpoints.
Window close hides the app for two minutes, then shuts down; reopening cancels
shutdown. **Application → Quit** exits immediately after confirmation. Session
metadata operations never create, move or delete working directories.

Read the [architecture guide](docs/STACK.md) and
[engineering contracts](docs/ENGINEERING.md) for the boundaries and recovery model.

## Get started

Windows x64 executable deployment, WebView2 requirements and the separate
Windows 11 desktop checklist are documented in [the Windows guide](docs/WINDOWS.md).

Ongleet is a **Linux preview**, validated on the Ubuntu host documented
in the [native validation guide](docs/VALIDATION.md). It requires a graphical
desktop, systemd cgroup v2 delegation, `cgroup.kill` and pidfds. Other Linux setups
are not all validated. A [macOS 14+ Apple Silicon port](docs/MACOS.md) is
prepared for review; the complete Mac build and native acceptance are unverified.

Install the Ubuntu build prerequisites:

```bash
sudo apt update
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
```

Install the toolchains with [mise](https://mise.jdx.dev/) and run from source:

```bash
mise install
mise exec -- npm install
mise exec -- npm run check:host
mise run dev
```

`mise run dev` keeps persistent preview data, configuration and cache under
`~/.local/share/ongleet-dev`, `~/.config/ongleet-dev` and `~/.cache/ongleet-dev`.
It can run alongside the installed app with separate sessions and settings.
Project files, Codex configuration and desktop credentials remain shared.
Development instances still share Vite port 1420.

Create a project pointing to an existing directory, then add a session. Each new
session launches its first configured terminal action. Use the tab menu to add
more tools; **Custom tab…** lets you choose a plugin and its configuration.
Project actions are editable through a form or YAML.

Install Codex, Git and GitHub tooling separately when you need them. Ongleet does
not change your shell or Codex configuration. The bundled `ong-codex` helper adds
conversation tracking and restart/resume behavior for Codex sessions.

To build and install a local Ubuntu package:

```bash
mise exec -- npm run package:deb
sudo apt install ./src-tauri/target/release/bundle/deb/Ongleet_0.1.0_amd64.deb
```

Use the filename emitted by the build. Launch **Ongleet** from the desktop menu or
run `ongleet-gui`; the package also installs `ongleet` and `ong-codex` on PATH.
The package uses regular application data, separate from `mise run dev`.
Packages depend on the build host's system libraries; see
[packaging details](docs/USAGE.md#install-a-local-ubuntu-package).

## Go further

- [Usage guide](docs/USAGE.md): project actions, YAML, archives, CLI, shortcuts and troubleshooting.
- [Plugin development](docs/PLUGINS.md): bundled plugins, SDK, schemas and agent interactions.
- [Architecture](docs/STACK.md): runtime boundaries, persistence and bounded transport.
- [Engineering](docs/ENGINEERING.md): code map, invariants and check commands.
- [Native validation](docs/VALIDATION.md): tested behavior, reproduction steps and remaining limitations. Engine tests and builds alone do not certify desktop behavior.
- [Appearance](docs/APPEARANCE.md): presets and terminal palette behavior.

## License

[MIT](LICENSE). Third-party dependencies retain their respective licenses.
