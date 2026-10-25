import { useEffect, useRef, useState } from 'react';
import { actionEditor } from './plugins';
import type { PluginTab } from './plugins/types';

export function TabConfiguration({
  tab,
  save,
  dismiss,
}: {
  tab: PluginTab;
  save: (config: unknown) => Promise<boolean>;
  dismiss: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const editor = actionEditor(tab.kind);
  const [draft, setDraft] = useState(
    () => editor?.toDraft(tab.config) ?? JSON.stringify(tab.config),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  const Editor = editor?.ConfigurationEditor;
  return (
    <dialog
      ref={dialog}
      aria-label="Tab configuration"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <h2>Configure {tab.name}</h2>
      {tab.kind === 'terminal' && (
        <p className="muted">
          Launch settings apply to future launches. The running process keeps its current settings.
        </p>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          setError('');
          void (async () => {
            try {
              const config = editor ? editor.fromDraft(draft) : JSON.parse(draft);
              if (await save(config)) dismiss();
              else
                setError(
                  'Unable to save tab configuration. See the application error for details.',
                );
            } catch (error) {
              setError(String(error));
            } finally {
              setBusy(false);
            }
          })();
        }}
      >
        <fieldset disabled={busy}>
          {Editor ? (
            <Editor value={draft} onChange={setDraft} />
          ) : (
            <label>
              Configuration (JSON)
              <textarea value={draft} onChange={(event) => setDraft(event.target.value)} />
            </label>
          )}
        </fieldset>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button type="button" disabled={busy} onClick={dismiss}>
            Cancel
          </button>
          <button disabled={busy}>Save configuration</button>
        </div>
      </form>
    </dialog>
  );
}
