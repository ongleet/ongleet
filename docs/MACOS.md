# macOS Apple Silicon preview preparation

The macOS backend is prepared for **macOS 14+ on Apple Silicon**. It has not
been built or run as a complete application on a Mac. Do not describe this port
as natively validated. Linux remains the validated preview platform.

Tauri, Ghostty WASM, Rust-owned PTYs, SQLite/YAML formats, stable IDs, CLI frames,
bounded queues, ACK/replay/history and the existing confirmation/save paths are
shared. The private `system/macos/` backend supplies the seven native services.
Intel, public distribution, notarization and the Mac App Store are outside this
preview. The macOS window is opaque and uses no private transparency APIs.

## Build on a Mac

Install Apple's command-line developer tools and the project's mise toolchains.
Run from the repository root:

```bash
mise install
mise exec -- npm ci
mise exec -- npm run check:host
mise exec -- npm run build
MACOSX_DEPLOYMENT_TARGET=14.0 mise exec -- cargo test \
  --manifest-path src-tauri/Cargo.toml --all-targets --target aarch64-apple-darwin
MACOSX_DEPLOYMENT_TARGET=14.0 mise exec -- npm run package:macos
```

`package:macos` builds the executables before Tauri bundles `.app` and DMG
artifacts under `src-tauri/target/aarch64-apple-darwin/release/bundle/`.
`tauri.macos.conf.json` is merged with the common configuration. It selects
macOS 14, an opaque window, the `ongleet-gui` main executable and local ad hoc
signing (`-`). See [Tauri configuration](https://v2.tauri.app/reference/config/)
and [code signing](https://tauri.app/distribute/sign/macos/).
Ad hoc signing does not guarantee opening without Gatekeeper intervention.
No notarized or publicly trusted package is produced by this configuration.

The application and CLI executables live in `Contents/MacOS`. For example,
when the app has been copied to `/Applications`:

```bash
/Applications/Ongleet.app/Contents/MacOS/ongleet --help
/Applications/Ongleet.app/Contents/MacOS/ongleet-gui --show
/Applications/Ongleet.app/Contents/MacOS/ong-codex --session-id 1 --tab-id 2
```

Replace the example IDs with existing terminal IDs. You can manually add the
bundle's `Contents/MacOS` directory to your own PATH. Packaging does not edit
shell/Codex configuration, create system links or install a service. App-launched
terminal commands receive a local PATH prefix for the bundled CLI, as on Linux;
an explicit action PATH still overrides it.

## Data, secrets and executable replacement

Explicit `XDG_DATA_HOME` and `XDG_CONFIG_HOME` overrides retain their existing
`ongleet` child directory. Without overrides, data lives in
`~/Library/Application Support/Ongleet` and configuration in
`~/Library/Preferences/Ongleet`. Linux directories are not imported automatically.
The control socket lives in a mode-0700 directory under the macOS temporary
location. Its short stable name hashes the data directory; peers must have the
current effective UID (`getpeereid`). Different data directories remain separate
instances. An excessively long temporary socket path fails explicitly.

Helpers use bundle executable paths, with device/inode, size and timestamp
checks captured at composition. Replacing the running image or a bundled sibling
refuses subsequent helper launches until restart. Darwin does not offer Linux's
re-execution through `/proc/<pid>/exe`. This check does not eliminate the narrow
race between verification and executing a pathname.

Secrets use Security.framework generic passwords, deterministic JSON attributes
in the account field and opaque persistent references. Searches filter attributes
without reading password bytes. Updates preserve the item reference. Locked,
denied or unavailable storage is an explicit error; there is no plaintext fallback.
Migration policy and credential size limits remain in the common credential code.

## Cleanup guarantees

Linux still requires delegated cgroup v2, pidfds and `cgroup.kill`. Its cleanup
profile is strict and its post-exit drain behavior is unchanged.

Darwin advertises a best-effort profile. One lazy watchdog belongs to each app
process. A bootstrap must already own its native session and receive watchdog
registration plus application acknowledgment before executing user code. The
watchdog observes application and terminal death through `EVFILT_PROC` and a
private connection. The application observes watchdog disconnection; protection
loss kills tracked sessions and refuses new terminal launches until restart.
No LaunchAgent, installed daemon or administrative privilege is involved.

Cleanup enumerates at most 65,536 PIDs, checks native session membership and
revalidates PID/start-time identity before signaling individual processes.
It never selects processes by name. Each terminal cleanup attempt is bounded by
three seconds; watchdog-wide death cleanup shares a three-second budget.
Enumeration races, descendants that call `setsid`, permission changes and some
simultaneous failures can leave survivors. This is **not cgroup containment**.
Explicitly detached external applications remain independent.

For best-effort terminals, post-exit output drainage additionally stops after two
seconds, cancels the reader and reports truncation/EOF, even when an escaped
child retains the PTY slave. The existing 1 MiB drain limit also applies. Reader
and writer cancellations use separate persistent pipes and blocking `kqueue`
waits, without periodic readiness polling.

File watches observe the target and its ancestors through `kqueue`, rearm after
replacement/recreation and coalesce until acknowledgment. Revision conflict and
save policy remain shared. Filesystem ACL/xattr and unrelated-writer limitations
remain those documented in the engineering guide.

## Native acceptance — not executed

The Darwin tests include real socket ownership/rejection, read/write cancellation,
PTY Unicode/Enter delivery, independent terminal closure, shared watchdog and
application/watchdog crash fixtures, refusal after protection loss, executable
replacement refusal, file replacement/recreation, locks and escaped-child drain
bounds. New process probes are Rust fixtures. Linux `/proc` and Secret Service
fixtures are excluded on Darwin; portable credential-policy tests remain enabled.
A type check is not an execution result for these tests.

Run the ignored keychain test only in a **dedicated macOS test account** whose
default keychain is disposable. The test verifies the actual default keychain path
before locking it. It does not select/create a keychain or alter the search list:

```bash
ONGLEET_TEST_KEYCHAIN=/absolute/disposable.keychain-db \
ONGLEET_TEST_KEYCHAIN_PASSWORD='disposable-test-password' \
MACOSX_DEPLOYMENT_TARGET=14.0 mise exec -- cargo test \
  --manifest-path src-tauri/Cargo.toml --bin ongleet-gui \
  isolated_keychain_persistent_refs_replacement_and_lock_errors -- --ignored
```

Before calling the preview functional, record actual results on an ARM64 Mac for:

- Complete build, test suite, `.app`/DMG production and `codesign --verify --deep --strict`.
- Finder and Terminal launch, CLI reuse, hide/reopen, Command+Q confirmation,
  geometry persistence, restoration, normal shutdown and crash cleanup.
- WKWebView/Ghostty rendering, reload, hidden output, bounded transport and terminal resize.
- Command copy/paste and editor shortcuts alongside Ctrl+C/Ctrl+D, Unicode and IME candidates.
- Text/image/file paste, internal primary selection across terminals and system clipboard isolation.
- Disposable keychain creation/update/search/delete, authorization denial and locked-store errors.
- Replaced bundles, blocked reader/writer cancellation, surviving escaped children,
  detached external apps and refusal after watchdog failure.

Use only `ONGLEET_DEBUG=1` for native diagnostics. Local AppKit key/focus events
and WKWebView page-load/content-process-termination events enter the common typed
logging path. Key traces may reveal typed input. Load-failure/resource counters
are not currently exposed by the Tauri macOS hooks used here and remain a native
diagnostic coverage limitation.
