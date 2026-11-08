# Windows implementation status

The Windows x64 backend implements all seven injected system services. It uses
WebView2, Ghostty WASM and Rust-owned ConPTY terminals. CI validates the backend
on Windows Server; a Windows 11 desktop has not been qualified.

## Executable deployment

Keep `ongleet-gui.exe`, `ongleet.exe` and `ong-codex.exe` together in one directory.
The CI ZIP contains these three executables only. Installers, signing, ARM64 and
WSL are outside this version. Windows requires the Microsoft Edge WebView2
Evergreen Runtime. The application does not install it or change PATH, system
links, shell profiles or personal Codex configuration. The Windows CI build uses
the static MSVC CRT so deployment does not add a Visual C++ runtime DLL to the ZIP.

Run `ongleet-gui.exe` to open the application. The CLI addresses an existing
instance; `ongleet.exe --help` lists commands. Ongleet prepends its executable
directory only to its own terminal environment, so its CLI and wrapper are
available inside managed terminals without editing the user's environment.

Data defaults to `%LOCALAPPDATA%\Ongleet`; configuration defaults to
`%APPDATA%\Ongleet`. Explicit absolute `XDG_DATA_HOME` and `XDG_CONFIG_HOME` values
instead use their `ongleet` subdirectory. Relative overrides are rejected. There
is no automatic Unix-data import. Newly seeded shell actions use built-in Windows
PowerShell; existing action files retain their commands. `shellCommand` uses
PowerShell with `-NoProfile`; explicit executables and argument arrays stay literal.

## Native resource ownership

Each terminal starts suspended, is assigned to a non-inheritable Job with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, and resumes only after assignment succeeds.
Ongleet alone retains the Job handles. Explicit closure terminates the Job and
waits for its active-process completion; application death closes the last handle
and terminates descendants. Captured commands have separate Jobs. Detached
external actions request breakaway and outlive Ongleet; an enclosing host Job can
forbid breakaway, in which case launch fails rather than silently attaching the
application to terminal lifetime. No installed service or watchdog is used.

ConPTY uses synchronous pseudoconsole handles and application-side overlapped
pipes. Independent cancellation events interrupt blocked reader/writer operations
through `CancelIoEx`. Pseudoconsole teardown runs independently of output reading.
Strict containment is separate from the two-second post-exit output-drain deadline;
truncation is reported through the shared worker contract.

Native executables use Windows argument quoting. Windows command shims run through
a one-shot Ongleet bootstrap after Job assignment, using Rust's Windows batch-file
escaping. Codex hooks use PowerShell literal strings, native ProcessStartInfo
arguments and an explicitly redirected raw stdin stream into the recorder,
with BOM-free input encoding restored immediately after child launch, without
changing user shell configuration. Helper launches verify captured running
and sibling executable identities and require restart after replacement.

Local-only named pipes use a data-directory/user/logon endpoint, a protected DACL,
reciprocal process-token checks, bounded frames and overlapped deadlines. An EOF
frame and acknowledgement preserve half-close without discarding unread replies.
Private files use protected DACLs, native locks, file identities and same-volume
atomic moves. The metadata writer lease is a private kernel mutex keyed by the
data directory's volume/file identity, owned by a dedicated blocking thread. It
survives directory rename without keeping a child file open. Other file locks use
`LockFileEx`. Directory watches are armed before returning and survive target
replacement/recreation. Windows has no POSIX directory fsync equivalent; temporary
files are flushed before replacement and moves request write-through. As elsewhere,
editor saves cannot eliminate unrelated external-writer races or preserve every
original ACL/attribute.

Credentials live in a separate user-scoped DPAPI-encrypted store with private
ACLs, exclusive update locking and atomic replacement. The shared 64 KiB payload
limit is retained. Decryption/access failures are errors; there is no plaintext
fallback.

## CI qualification

The existing workflow runs `windows-2025` x64 MSVC checks with mise and Python 3.13,
while retaining Linux and macOS jobs. Windows runs formatting, boundary checks,
production frontend build, all applicable Rust tests and a native Codex subprocess
fixture. Only successful tests permit the bundled release build, staged CLI smoke
checks and executable ZIP upload. Hosted Windows Server results establish backend
coverage, not Windows 11 desktop qualification. All three jobs must pass before
calling the backend CI-tested.

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
