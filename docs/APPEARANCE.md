# Appearance

Use **Application → Appearance…** to choose an application preset and an optional
terminal override. Follow application is the default. Changes apply immediately
and save automatically; Done closes the dialog. Restore defaults selects Ongleet
Original and Follow application. Preferences are global within the local WebView
profile, stored as version 1 preset IDs in `ongleet.appearance`. Unknown IDs fall
back independently to Original or inheritance. Storage failures leave changes
available in memory and display a notice in Appearance.

Presets cover chrome, controls, Files syntax and editor surfaces, PR surfaces,
and Diff additions/deletions. Session tag colors remain explicit user/plugin
values. No network access, system-theme tracking, custom palettes or CLI settings
are needed. CodeMirror compartments update highlighting and light/dark mode
without replacing the document, selection or undo history.

## Terminal compatibility limit

The pinned `ghostty-web` 0.4.0 does **not** implement live terminal cell recoloring.
Its `options.theme` setter only warns. `CanvasRenderer.setTheme` updates canvas
background, cursor and selection colors, but WASM returns already resolved RGB
foreground/background values for each cell. Thus existing cells (including default
background cells) retain launch-time colors even after subsequent output. The
renderer palette does not change these resolved values. The dialog explains this
limit. Hidden renderers receive decoration changes immediately and request a full
repaint on visibility, using the existing bounded adapter.

New terminals receive the complete selected palette (16 ANSI colors, foreground,
background, cursor and selection). Original preserves the engine's previous ANSI
defaults. WASM interprets color zero as unspecified, so VS palettes use `#010101`
for black. Programs emitting explicit RGB keep their values. No injected terminal
escape sequences, emulator reconstruction, history replay, PTY restart or RGB
substitution is used to approximate unsupported live recoloring.

Full live palette switching remains unfinished and needs engine support beyond
the pinned runtime theme option. Engine upgrades must revalidate the rendering
adapter and terminal contracts before removing this limitation.

## Palette references

These are locally bundled adaptations, not exact replicas of other interfaces:

- [VS Code default themes](https://github.com/microsoft/vscode/tree/main/extensions/theme-defaults), including [Dark+](https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/dark_plus.json) and [Light+](https://github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/light_plus.json).
- [Atom One Dark colors](https://github.com/atom/one-dark-syntax/blob/master/styles/colors.less).
- Darcula and Monokai Inspired use familiar palette conventions adapted to Ongleet.

## Verification

`mise exec -- npm run check:appearance` checks restoration, invalid IDs/versions,
inheritance/override, storage failures/retry, subscription cleanup, complete launch
palettes against actual Ghostty WASM, renderer decoration updates, hidden repaint,
and CodeMirror compartment preservation of drafts, selection and undo/redo.
The renderer checks record canvas calls; they do not certify WebKitGTK behavior.

Native acceptance now also runs with `scripts/check-appearance-native.py --backend
wayland` or `--backend xwayland`. It starts a private headless Weston compositor
and WebDriver, creates isolated app data and fixture PTYs, records screenshots,
and checks runtime/PID retention, hidden output, editor undo/redo and fresh-process
preference/draft restoration. Build the native-validation bundle first as described
in [validation](VALIDATION.md). It does not establish physical desktop input or
live WASM recoloring support.
