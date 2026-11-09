import { t } from './i18n';
import {
  Button,
  Dialog,
  Disclosure,
  DisclosureTrigger,
  Input,
  Select,
} from './components/Controls';
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
  activeTabName,
  configureTab,
  sharedSettings,
  configureShared,
  archiveTab,
}: {
  sharedSettings: { id: number; label: string }[];
  configureShared: (tabId: number) => void;
  activeTabName?: string;
  configureTab?: () => void;
  archiveTab?: () => void;
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
      <Disclosure
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
        <DisclosureTrigger aria-label={t('Session and tab menu')} title={t('Session and tab menu')}>
          <span aria-hidden="true">⋮</span>
        </DisclosureTrigger>
        <div className="app-menu-items">
          {loading && <p role="status">{t('Loading actions…')}</p>}
          {error && (
            <p className="error" role="alert">
              {error}
              <Button
                disabled={disabled}
                onClick={() => {
                  menu.current!.open = false;
                  configure();
                }}
              >
                {t('Open action settings…')}
              </Button>
            </p>
          )}
          {(sessionId === null ? [] : (['tabs', 'external'] as const)).map((group) => (
            <div key={group}>
              <p className="muted">{group === 'tabs' ? t('New tab') : t('External actions')}</p>
              {actions
                .filter((action) => (action.kind === 'command') === (group === 'external'))
                .map((action) => (
                  <Button
                    key={action.id}
                    disabled={disabled || loading}
                    title={
                      group === 'external'
                        ? t('Launch an independent application')
                        : t('Open a new tab')
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
                  </Button>
                ))}
              {!loading &&
                !error &&
                !actions.some(
                  (action) => (action.kind === 'command') === (group === 'external'),
                ) && (
                  <Button
                    disabled={disabled}
                    onClick={() => {
                      menu.current!.open = false;
                      configure();
                    }}
                  >
                    {group === 'tabs'
                      ? t('No tabs configured — configure…')
                      : t('No actions configured — configure…')}
                  </Button>
                )}
              {group === 'tabs' && (
                <Button
                  disabled={disabled}
                  onClick={() => {
                    menu.current!.open = false;
                    setCustom(true);
                  }}
                >
                  {t('Custom tab…')}
                </Button>
              )}
            </div>
          ))}
          {sharedSettings.length > 0 && (
            <div>
              <p className="muted">{t('Shared session settings')}</p>
              {sharedSettings.map((plugin) => (
                <Button
                  key={plugin.id}
                  disabled={disabled}
                  onClick={() => {
                    menu.current!.open = false;
                    configureShared(plugin.id);
                  }}
                >
                  {t(plugin.label)}
                </Button>
              ))}
            </div>
          )}
          {activeTabName !== undefined && (
            <div>
              <p className="muted">{t('Tab')}</p>
              <Button
                disabled={disabled}
                onClick={() => {
                  menu.current!.open = false;
                  configureTab?.();
                }}
              >
                {t('Configure…')}
              </Button>
              <Button
                disabled={disabled}
                title={t('Archive “{value0}” for 30 days', { value0: activeTabName })}
                onClick={() => {
                  menu.current!.open = false;
                  archiveTab?.();
                }}
              >
                {t('Archive tab')}
              </Button>
            </div>
          )}
          <Button
            disabled={disabled}
            onClick={() => {
              menu.current!.open = false;
              archives();
            }}
          >
            {t('Archives')}
          </Button>
        </div>
      </Disclosure>
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
    <Dialog
      ref={(node) => {
        dialog.current = node;
        if (node && !node.open) node.showModal();
      }}
      aria-label={t('Custom tab')}
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <h2>{t('Custom tab')}</h2>
      <p className="muted">{t('Create an individual tab without changing project actions.')}</p>
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
            {t('Name')}
            <Input
              autoFocus
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            {t('Tab type')}
            <Select
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
                    {t(plugin.label)}
                  </option>
                ))}
            </Select>
          </label>
          <Editor value={draft} onChange={setDraft} />
        </fieldset>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <Button type="button" disabled={busy} onClick={dismiss}>
            {t('Cancel')}
          </Button>
          <Button disabled={disabled || busy || !name.trim()}>{t('Create tab')}</Button>
        </div>
      </form>
    </Dialog>
  );
}
