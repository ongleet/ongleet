# User guide screenshots

These PNGs are actual WebKitGTK captures of Ongleet on Linux at 1280 × 900.
They use a temporary Git repository, application configuration, data and cache.
No personal sessions, credentials or live GitHub responses are included. The
Pull request image illustrates the unconfigured state.

To regenerate on a supported Linux development host with `WebKitWebDriver` and
`Xvfb` installed, build the current bundled application and matching CLI:

```bash
mise exec -- npm run build
mise exec -- cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --offline
python3 scripts/capture-user-guide.py --backend xvfb
```

The capture script needs permission to create a local WebDriver listener and run
native GUI processes. It writes images here and prints the temporary artifact
directory, which contains its native log and cleanup report. It uses the sibling
`ongleet` CLI beside the GUI executable; pass `--application /path/to/ongleet-gui`
to select another matching bundle. The script stops its isolated application and
display after capture. Review every image before committing regenerated files.

Captures document the illustrated screens. They do not certify all keyboard,
clipboard, display-backend or platform behavior; consult the
[native validation guide](../VALIDATION.md) for that evidence.
