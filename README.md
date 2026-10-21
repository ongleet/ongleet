# Terminal workspace: stack prototype

Stage one of a native Linux terminal workspace. This repository currently contains a **single-terminal validation prototype**, not the finished workspace application. The workspace UI, SQLite, hide/reopen lifecycle, single-instance behavior, Unix-socket CLI, and Ubuntu packages are intentionally gated on native interactive validation.

The stack is Tauri 2, React + TypeScript, Rust, portable-pty, and pinned `ghostty-web` 0.4.0. There is no Electron, xterm.js runtime, tmux, Git integration, or agent orchestration.

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

See [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/). A working graphical desktop session is required. The prototype does not change shell or Codex configuration. Codex must already be installed, available on PATH, and configured by the user.

Choose an existing working directory, then Start the default shell or Run Codex. For a direct command, enter an executable and a JSON array of arguments. There is no implicit shell evaluation. To explicitly request shell syntax, select your shell executable and supply arguments such as `["-lc", "printf 'hello\\n'"]`.

Ordinary React renders do not recreate the terminal. Closing a session asks for confirmation. **At this stage closing the window quits the prototype after confirmation; it does not yet hide it.** Do not rely on production process-lifecycle guarantees yet.

## Checks

```bash
mise exec -- npm run build
mise exec -- npm run check:wasm
mise exec -- cargo check --manifest-path src-tauri/Cargo.toml --offline
mise exec -- cargo test --manifest-path src-tauri/Cargo.toml --offline
```

Offline Rust commands require a populated Cargo cache; omit `--offline` for the initial fetch. `check:wasm` uses the exact installed WASM and production byte adapter. It checks split UTF-8 and escape sequences, alternate-screen restoration, mouse/paste modes, terminal replies, oversized-chunk rejection, 100,000 output lines with bounded scrollback, and resizing. It does **not** certify browser rendering, keyboard input, clipboard behavior, or interactive Codex.

The Rust checks use actual Linux PTYs and a Tauri channel callback to exercise byte transport, explicit environment overrides, exit status, ACK backpressure, resizing, input, launch failures, and termination of a child in a separate job-control group.

Use [the native validation checklist](docs/VALIDATION.md) before proceeding to the full application. Findings and design boundaries are in [the stack evaluation](docs/STACK.md).
