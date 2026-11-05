# Appearance

Choose an app preset and optional terminal override in **Application → Appearance…**.
**Follow application** is the default. Changes apply and save immediately;
**Done** closes the dialog. **Restore defaults** selects Ongleet Original and
Follow application.

Preferences are global to the local WebView profile. Version 1 preset IDs live in
`ongleet.appearance`. Unknown IDs independently fall back to Original or
inheritance. Storage failures keep changes in memory and show a notice.

Presets cover chrome, controls, Files syntax/editor surfaces, PR surfaces and Diff
additions/deletions. Diff blends the app preset's terminal ANSI green/red with its
background, using stronger gutter tints. File-status colors use the same palette:

- Green: added/copied.
- Red: deleted/unmerged.
- Yellow: modified/type changes.
- Blue: renamed.

Terminal overrides do not affect Diff. Session tags keep explicit user/plugin
colors. No network access, system-theme tracking, custom palettes or CLI settings
are needed. CodeMirror compartments update highlighting and light/dark mode while
preserving documents, selections and undo history.

## Terminal compatibility limit

The pinned `ghostty-web` 0.4.0 does **not** implement live terminal cell recoloring.
Its `options.theme` setter only warns. `CanvasRenderer.setTheme` updates canvas
background, cursor and selection colors, but WASM returns already resolved RGB
foreground/background values for each cell. Existing cells, including default background cells, keep launch-time colors after later output. The
renderer palette does not change these resolved values. The dialog explains this
limit. Hidden renderers receive decoration changes immediately and request a full
repaint on visibility, using the existing bounded adapter.

New terminals receive the complete selected palette (16 ANSI colors, foreground,
background, cursor and selection). Original preserves the engine's previous ANSI
defaults. WASM interprets color zero as unspecified, so VS palettes use `#010101`
for black. Programs emitting explicit RGB keep their values. No injected terminal
escape sequences, emulator reconstruction, history replay, PTY restart or RGB
substitution is used to approximate unsupported live recoloring.

Full live palette switching needs engine support beyond the pinned runtime theme option. Engine upgrades must revalidate the rendering
adapter and terminal contracts before removing this limitation.

## Palette references

These bundled palettes adapt other interfaces; they are not exact copies:

- [VS Code default themes](https://github.com/microsoft/vscode/tree/main/extensions/theme-defaults), including [Dark+](https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/dark_plus.json) and [Light+](https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/light_plus.json).
- [Atom One Dark colors](https://github.com/atom/one-dark-syntax/blob/master/styles/colors.less).
- Darcula and Monokai Inspired use familiar palette conventions adapted to Ongleet.

## Verification

`mise exec -- npm run check:appearance` checks restoration, invalid IDs/versions,
inheritance/override, storage failures/retry, subscription cleanup, complete launch
palettes against actual Ghostty WASM, renderer decoration updates, hidden repaint,
and CodeMirror compartment preservation of drafts, selection and undo/redo.
The renderer checks record canvas calls; they do not certify WebKitGTK behavior.

Native acceptance also runs with `scripts/check-appearance-native.py --backend
wayland` or `--backend xwayland`. It starts a private headless Weston compositor
and WebDriver, creates isolated app data and fixture PTYs, records screenshots,
and checks runtime/PID retention, hidden output, editor undo/redo and fresh-process
preference/draft restoration. Build the native-validation bundle first as described
in [validation](VALIDATION.md). It does not prove physical desktop input or
live WASM recoloring support.

Add `--surfaces` to inspect configuration, project creation, populated archives,
PR reviews/checks, validation errors and fixture network failures under Light+
and One Dark. The native-validation HTTP fixture uses a dummy environment token.
Headless native-focus checks are reported separately as unavailable (exit 2),
leaving real-desktop focus acceptance open.
