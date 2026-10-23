# Repository instructions

- Write repository text in English; match the user’s conversation language.
- Use `mise exec --` for Node/Rust commands; use `vim` for interactive editing.
- Read `MEMORY.ignore.md` at task start when present. Keep useful dated entries around ten words each; update after work. Keep `TODO.ignore.md` limited to unfinished work.
- Preserve Ghostty, Rust-owned PTYs, bounded IPC and cgroup containment. Do not modify shell/Codex configuration automatically.
- No Electron, xterm.js runtime, tmux, daemon, automatic Git discovery, worktree management, setup scripts or product agent orchestration. Explicit session-linked read-only PR status is supported.

## Read only what the task needs

- [README.md](README.md): product behavior, usage and CLI.
- [docs/STACK.md](docs/STACK.md): stack decisions.
- [docs/ENGINEERING.md](docs/ENGINEERING.md): terminal invariants, session/persistence contracts, code map and check commands. Read relevant sections before changing those areas.
- [docs/VALIDATION.md](docs/VALIDATION.md): native validation procedures, evidence and limitations. Read alongside README/STACK before terminal architecture changes.
- `TODO.ignore.md`: remaining local work, when present.

Run checks appropriate to the change. Prefer real PTY/native integration evidence; builds and engine tests do not certify WebKitGTK behavior. Report untested behavior explicitly, and never mark a validation stage complete from one passing check.
