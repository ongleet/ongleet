import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { actionEditor, plugins } from './plugins';

type Action = { id: string; name: string; kind: string; configVersion: number };

export function SessionMenu({
  sessionId,
  disabled,
  mutate,
  configure,
  archives,
}: {
  configure: () => void;
  archives: () => void;
  sessionId: number | null;
  disabled: boolean;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>;
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  const [actions, setActions] = useState<Action[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [custom, setCustom] = useState(false);
  async function load() {
    if (sessionId === null) return;
    setLoading(true);
    setError('');
    setActions([]);
    try {
      setActions(
        (await invoke<Action[]>('actions', { sessionId })).filter(
          (action) =>
            action.kind === 'command' ||
            plugins.some(
              (plugin) =>
                plugin.id === action.kind && plugin.configVersion === action.configVersion,
            ),
        ),
      );
    } catch (error) {
      setError(String(error));
    } finally {
      setLoading(false);
    }
  }
  return (
    <>
      <details
        className="titlebar-menu session-actions-menu"
        name="titlebar-menu"
        ref={menu}
        onToggle={(event) => {
          if (event.currentTarget.open) void load();
        }}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            event.currentTarget.open = false;
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.currentTarget.open = false;
            event.currentTarget.querySelector('summary')?.focus();
          }
        }}
      >
        <summary aria-label="Session menu">Session</summary>
        <div className="app-menu-items">
          {loading && <p role="status">Loading actions…</p>}
          {error && (
            <p className="error" role="alert">
              {error}
              <button
                disabled={disabled}
                onClick={() => {
                  menu.current!.open = false;
                  configure();
                }}
              >
                Open action settings…
              </button>
            </p>
          )}
          {(sessionId === null ? [] : (['tabs', 'external'] as const)).map((group) => (
            <div key={group}>
              <p className="muted">{group === 'tabs' ? 'New tab' : 'External actions'}</p>
              {actions
                .filter((action) => (action.kind === 'command') === (group === 'external'))
                .map((action) => (
                  <button
                    key={action.id}
                    disabled={disabled || loading}
                    title={
                      group === 'external' ? 'Launch an independent application' : 'Open a new tab'
                    }
                    onClick={() => {
                      menu.current!.open = false;
                      void mutate(group === 'external' ? 'run_external_action' : 'create_tab', {
                        sessionId,
                        actionId: action.id,
                      });
                    }}
                  >
                    {action.name}
                  </button>
                ))}
              {!loading &&
                !error &&
                !actions.some(
                  (action) => (action.kind === 'command') === (group === 'external'),
                ) && (
                  <button
                    disabled={disabled}
                    onClick={() => {
                      menu.current!.open = false;
                      configure();
                    }}
                  >
                    {group === 'tabs'
                      ? 'No tabs configured — configure…'
                      : 'No actions configured — configure…'}
                  </button>
                )}
              {group === 'tabs' && (
                <button
                  disabled={disabled}
                  onClick={() => {
                    menu.current!.open = false;
                    setCustom(true);
                  }}
                >
                  Custom tab…
                </button>
              )}
            </div>
          ))}
          <button
            disabled={disabled}
            onClick={() => {
              menu.current!.open = false;
              archives();
            }}
          >
            Archives
          </button>
        </div>
      </details>
      {custom && (
        <CustomTab
          disabled={disabled}
          dismiss={() => setCustom(false)}
          create={(name, kind, configVersion, config) =>
            mutate('create_custom_tab', { sessionId, name, kind, configVersion, config })
          }
        />
      )}
    </>
  );
}

function CustomTab({
  disabled,
  dismiss,
  create,
}: {
  disabled: boolean;
  dismiss: () => void;
  create: (name: string, kind: string, configVersion: number, config: unknown) => Promise<boolean>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState('Custom tab');
  const [kind, setKind] = useState('terminal');
  const [draft, setDraft] = useState(() =>
    actionEditor('terminal')!.toDraft({ executable: 'bash' }),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const editor = actionEditor(kind)!;
  const Editor = editor.ConfigurationEditor;
  return (
    <dialog
      ref={(node) => {
        dialog.current = node;
        if (node && !node.open) node.showModal();
      }}
      aria-label="Custom tab"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <h2>Custom tab</h2>
      <p className="muted">Create an individual tab without changing project actions.</p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setError('');
          setBusy(true);
          void (async () => {
            try {
              const config = editor.fromDraft(draft);
              if (
                await create(
                  name,
                  kind,
                  plugins.find((plugin) => plugin.id === kind)!.configVersion,
                  config,
                )
              )
                dismiss();
            } catch (error) {
              setError(String(error));
            } finally {
              setBusy(false);
            }
          })();
        }}
      >
        <fieldset disabled={disabled || busy}>
          <label>
            Name
            <input
              autoFocus
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            Tab type
            <select
              value={kind}
              onChange={(event) => {
                const kind = event.target.value;
                setKind(kind);
                setDraft(
                  actionEditor(kind)!.toDraft(kind === 'terminal' ? { executable: 'bash' } : {}),
                );
              }}
            >
              {plugins
                .filter((plugin) => actionEditor(plugin.id))
                .map((plugin) => (
                  <option key={plugin.id} value={plugin.id}>
                    {plugin.label}
                  </option>
                ))}
            </select>
          </label>
          <Editor value={draft} onChange={setDraft} />
        </fieldset>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button type="button" disabled={busy} onClick={dismiss}>
            Cancel
          </button>
          <button disabled={disabled || busy || !name.trim()}>Create tab</button>
        </div>
      </form>
    </dialog>
  );
}
