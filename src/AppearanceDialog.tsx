import { useEffect, useRef, useSyncExternalStore } from 'react';
import { appearance, themePresets } from './appearance';
import type { ThemeId, ThemePreset } from './theme-presets';

function Preview({ theme, terminal = false }: { theme: ThemePreset; terminal?: boolean }) {
  const colors = terminal
    ? theme.terminal
    : { ...theme.terminal, background: theme.ui.editor, foreground: theme.ui.text };
  return (
    <div
      className="appearance-preview"
      style={{ background: colors.background, color: colors.foreground }}
    >
      <div className="appearance-swatches" aria-label={`${theme.name} color swatches`}>
        {[
          colors.background,
          colors.foreground,
          colors.red,
          colors.green,
          colors.yellow,
          colors.blue,
          colors.magenta,
          colors.cyan,
        ].map((color, i) => (
          <span key={i} style={{ background: color }} />
        ))}
      </div>
      {terminal ? (
        <pre>
          <span style={{ color: colors.green }}>$</span> echo "Hello, Ongleet"{'\n'}Hello, Ongleet
          <span style={{ background: colors.cursor, color: colors.cursorAccent }}> </span>
        </pre>
      ) : (
        <pre>
          <span style={{ color: theme.syntax.keyword }}>const</span> greeting ={' '}
          <span style={{ color: theme.syntax.string }}>"Hello, Ongleet"</span>;{'\n'}
          <span style={{ color: theme.syntax.function }}>print</span>(greeting);
        </pre>
      )}
    </div>
  );
}
export function AppearanceDialog({ close }: { close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const state = useSyncExternalStore(appearance.subscribe, appearance.getSnapshot);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog ref={dialog} aria-labelledby="appearance-title" onCancel={close}>
      <h2 id="appearance-title">Appearance</h2>
      <p>Changes apply immediately and save automatically.</p>
      <label>
        Application theme
        <select
          autoFocus
          value={state.preferences.application}
          onChange={(event) => appearance.update({ application: event.target.value as ThemeId })}
        >
          {themePresets.map((theme) => (
            <option key={theme.id} value={theme.id}>
              {theme.name}
            </option>
          ))}
        </select>
      </label>
      <Preview theme={state.application} />
      <label>
        Terminal theme
        <select
          value={state.preferences.terminal ?? ''}
          onChange={(event) =>
            appearance.update({ terminal: (event.target.value as ThemeId) || null })
          }
        >
          <option value="">Follow application</option>
          {themePresets.map((theme) => (
            <option key={theme.id} value={theme.id}>
              {theme.name}
            </option>
          ))}
        </select>
      </label>
      <Preview theme={state.terminal} terminal />
      <p>
        Existing terminals update cursor, selection and canvas background. Ghostty 0.4.0 keeps
        existing cell colors; the full palette applies when a terminal opens.
      </p>
      {state.persistenceNotice && (
        <p role="alert" className="error">
          {state.persistenceNotice}
        </p>
      )}
      <div className="dialog-actions">
        <button onClick={() => appearance.update({ application: 'original', terminal: null })}>
          Restore defaults
        </button>
        <button onClick={close}>Done</button>
      </div>
    </dialog>
  );
}
