# Windows implementation status

Windows is not supported yet. The shared service prerequisites have been
implemented, and a Windows CI gate is configured. The native backend remains
unfinished. Existing Linux and macOS support selection remains unchanged.

## Remaining backend work

- Implement all seven services in a private `system/windows/` backend with
  target-specific `windows-sys` dependencies and x64 selection.
- Implement direct ConPTY, suspended launch, Job assignment before resume,
  application-owned non-inheritable kill-on-close Jobs and completion cleanup.
  Use independent overlapped read/write cancellation and a two-second post-exit
  drain deadline with strict containment. Teardown must not depend on draining
  output. Supervise captured commands separately; preserve detached applications.
- Implement Windows executable/shim resolution, command-line and hook-shell
  quoting, native Codex recording/restart/resume and PowerShell defaults for new
  actions. Preserve existing YAML and literal executable/argument behavior.
- Implement absolute XDG overrides, AppData defaults, verified sibling executable
  identities and no automatic Unix-data import.
- Implement local-only named pipes with restrictive ACLs, user/logon identity,
  reciprocal process-token verification, framing, half-close and deadlines.
- Implement private DACLs, locks, file identities, atomic replacement,
  cancellable directory watches armed before return and editor revision safety.
- Implement an atomic, private, user-scoped DPAPI credential store supporting the
  shared 64 KiB contract; never fall back to plaintext.
- Implement opaque WebView2 behavior, activation, geometry, clipboard formats,
  file links, Ctrl shortcuts, instance-local primary selection and typed
  diagnostics gated by `ONGLEET_DEBUG`.

## CI and distribution

Port portable fixtures and isolate Unix-only tests. Add real Windows integration
coverage for ConPTY bytes/resize/cancellation/saturation, containment failures and
crashes, detached processes, authenticated transport rejection/deadlines/bounds,
files/watches/conflicts, DPAPI failures and Codex stand-ins with spaces, Unicode
and metacharacters.

The existing workflow now includes `windows-2025` x64 MSVC CI using mise and
Python 3.13, without changing the Linux and macOS jobs. It runs boundary tests,
frontend build, Python syntax checks and all Rust targets. After those pass, it
builds bundled release executables, stages only `ongleet-gui.exe`, `ongleet.exe`
and `ong-codex.exe`, smoke-tests CLI/Codex help and isolated unavailable-instance
errors, and uploads an executable ZIP. A failed build/test prevents publication.

The current code explicitly rejects Windows in `system/mod.rs`, so this gate is
expected to fail until the native backend and portable fixtures are implemented.
Do not skip or suppress that failure. All three platform jobs must pass before
calling the backend CI-tested. Windows-specific backend integration scenarios
listed above still need implementing; `cargo test --all-targets` does not create
that coverage by itself.

Deployment documentation must describe executable layout and the WebView2
prerequisite. Do not automatically install dependencies, change PATH, create
system links or edit personal shell/Codex configuration. Installers, signing,
ARM64 and WSL remain deferred.

## Windows 11 desktop acceptance

Hosted Windows Server tests do not qualify the Windows 11 desktop. Record the
Windows, WebView2, Ghostty and Codex versions and retain evidence for each check:

- [ ] Opaque window, Ghostty rendering, resize, DPI changes and terminal replay.
- [ ] Physical keyboard, Ctrl shortcuts, paste/Enter separation and IME candidates.
- [ ] Text, images and file-list clipboard formats; internal primary selection.
- [ ] Focus, minimize, hide/reopen and repeated-instance activation.
- [ ] Cancel/confirm closure, sibling survival and descendant cleanup on app death.
- [ ] Geometry, maximization, monitor relocation and fresh-process restoration.
- [ ] Native Codex hooks, conversation recording, restart and resume.
- [ ] Executable-only deployment with WebView2 on a separate Windows 11 system.
