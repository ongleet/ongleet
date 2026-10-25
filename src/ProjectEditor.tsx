import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Project } from './main';
import { plugins, actionEditor } from './plugins';

type Action = { id: string; name: string; kind: string; configVersion: number; config: unknown };
type Draft = {
  id: string;
  name: string;
  kind: string;
  configVersion: number;
  pluginConfig: string;
};
type Settings = { env: Record<string, string>; actions: Action[] };
type EnvRow = { id: string; name: string; value: string };
const envRows = (env: Record<string, string>): EnvRow[] =>
  Object.entries(env).map(([name, value], index) => ({ id: `env-${index}`, name, value }));
type View = 'general' | 'actions' | 'yaml';
const drafts = (actions: Action[]): Draft[] =>
  actions.map((action) => ({
    id: action.id,
    name: action.name,
    kind: action.kind,
    configVersion: action.configVersion,
    pluginConfig:
      actionEditor(action.kind)?.toDraft(action.config) ?? JSON.stringify(action.config),
  }));
function actionsFrom(draft: Draft[]): Action[] {
  return draft.map((action) => ({
    id: action.id,
    name: action.name,
    kind: action.kind,
    configVersion: action.configVersion,
    config:
      actionEditor(action.kind)?.fromDraft(action.pluginConfig) ?? JSON.parse(action.pluginConfig),
  }));
}

export function ProjectEditor({
  project,
  defaults = false,
  initialView = 'general',
  initialDirectory,
  save,
  dismiss,
  externalError,
}: {
  project?: Project;
  defaults?: boolean;
  initialView?: View;
  initialDirectory: string;
  save: (name: string, rootDirectory: string) => Promise<boolean>;
  dismiss: () => void;
  externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(project?.name ?? '');
  const [directory, setDirectory] = useState(project?.rootDirectory ?? initialDirectory);
  const [configPath, setConfigPath] = useState('');
  const [view, setView] = useState<View>(defaults ? 'actions' : initialView);
  const [draft, setDraft] = useState<Draft[]>([]);
  const [yaml, setYaml] = useState('');
  const [commonEnv, setCommonEnv] = useState<EnvRow[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    dialog.current?.showModal();
    let disposed = false;
    if (project || defaults) {
      void invoke<string>(defaults ? 'default_config' : 'project_config', { id: project?.id })
        .then((value) => {
          if (!disposed) setConfigPath(value);
        })
        .catch(() => {});
      void invoke<string>(defaults ? 'default_actions' : 'project_actions', { id: project?.id })
        .then(async (text) => {
          if (disposed) return;
          setYaml(text);
          setLoaded(true);
          try {
            const settings = await invoke<Settings>('parse_actions', { yaml: text });
            if (!disposed) {
              setDraft(drafts(settings.actions));
              setCommonEnv(envRows(settings.env));
            }
          } catch (error) {
            if (!disposed) {
              setView('yaml');
              setError(String(error));
            }
          }
        })
        .catch((error) => {
          if (!disposed) setError(String(error));
        });
    }
    return () => {
      disposed = true;
    };
  }, [project?.id, defaults]);

  function settingsFromForm(): Settings {
    const names = new Set<string>();
    for (const row of commonEnv) {
      if (!row.name || names.has(row.name))
        throw new Error('Common environment variables require unique, nonempty names.');
      names.add(row.name);
    }
    return {
      env: Object.fromEntries(commonEnv.map((row) => [row.name, row.value])),
      actions: actionsFrom(draft),
    };
  }
  function editEnv(id: string, changes: Partial<EnvRow>) {
    setCommonEnv((current) => current.map((row) => (row.id === id ? { ...row, ...changes } : row)));
    setDirty(true);
    setNotice('');
  }
  async function switchView(next: View) {
    if (next === view || busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      if (view === 'actions')
        setYaml(await invoke<string>('format_actions', { config: settingsFromForm() }));
      if (next === 'actions') {
        const settings = await invoke<Settings>('parse_actions', { yaml });
        setDraft(drafts(settings.actions));
        setCommonEnv(envRows(settings.env));
      }
      setView(next);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function saveActions() {
    if (!project && !defaults) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const text =
        view === 'actions'
          ? await invoke<string>('format_actions', { config: settingsFromForm() })
          : yaml;
      await invoke(defaults ? 'save_default_actions' : 'save_project_actions', {
        id: project?.id,
        yaml: text,
      });
      setYaml(text);
      setDirty(false);
      setNotice('Actions saved. New launches will use these settings.');
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  function patch(id: string, changes: Partial<Draft>) {
    setDraft((current) =>
      current.map((action) => (action.id === id ? { ...action, ...changes } : action)),
    );
    setDirty(true);
    setNotice('');
  }
  function move(index: number, offset: number) {
    setDraft((current) => {
      const next = [...current];
      const [action] = next.splice(index, 1);
      next.splice(index + offset, 0, action);
      return next;
    });
    setDirty(true);
    setNotice('');
  }
  function close() {
    if (!busy) dismiss();
  }
  return (
    <dialog
      ref={dialog}
      aria-label={defaults ? 'Default actions' : 'Project editor'}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
    >
      <h2>{defaults ? 'Default actions' : project ? 'Edit project' : 'New project'}</h2>
      {defaults && (
        <p className="muted">
          Copied to projects when their actions are first initialized. Existing project settings
          stay unchanged.
        </p>
      )}
      {defaults && configPath && <p className="muted">{configPath}</p>}
      {(project || defaults) && (
        <div
          role="tablist"
          aria-label="Project settings"
          className="settings-tabs"
          onKeyDown={(event) => {
            const views: View[] = defaults ? ['actions', 'yaml'] : ['general', 'actions', 'yaml'];
            const index = views.indexOf(view);
            const next =
              event.key === 'ArrowRight'
                ? (index + 1) % views.length
                : event.key === 'ArrowLeft'
                  ? (index + views.length - 1) % views.length
                  : event.key === 'Home'
                    ? 0
                    : event.key === 'End'
                      ? views.length - 1
                      : null;
            if (next !== null && !busy) {
              event.preventDefault();
              void switchView(views[next]);
              document.getElementById(`settings-${views[next]}`)?.focus();
            }
          }}
        >
          {((defaults ? ['actions', 'yaml'] : ['general', 'actions', 'yaml']) as View[]).map(
            (item) => (
              <button
                key={item}
                id={`settings-${item}`}
                type="button"
                role="tab"
                aria-selected={view === item}
                aria-controls={`settings-panel-${item}`}
                tabIndex={view === item ? 0 : -1}
                disabled={busy || (item !== 'general' && !loaded)}
                onClick={() => void switchView(item)}
              >
                {item === 'general' ? 'General' : item === 'actions' ? 'Actions' : 'YAML'}
              </button>
            ),
          )}
        </div>
      )}
      <section
        id="settings-panel-general"
        role="tabpanel"
        aria-labelledby="settings-general"
        hidden={view !== 'general'}
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            setBusy(true);
            void save(name, directory)
              .then((saved) => {
                if (saved) {
                  if (dirty)
                    setNotice('Project saved. Save your action changes in Actions or YAML.');
                  else dismiss();
                }
              })
              .finally(() => setBusy(false));
          }}
        >
          <label>
            Name
            <input
              required
              autoFocus
              value={name}
              disabled={busy}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            Root directory
            <input
              required
              value={directory}
              disabled={busy}
              onChange={(event) => setDirectory(event.target.value)}
            />
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              void open({ directory: true, multiple: false, title: 'Choose project root' })
                .then((path) => {
                  if (typeof path === 'string') setDirectory(path);
                })
                .catch((error) => setError(String(error)))
                .finally(() => setBusy(false));
            }}
          >
            Choose directory…
          </button>
          <p className="muted">
            The root is the default directory for new sessions. Existing paths stay unchanged.
          </p>
          {configPath && (
            <label>
              Actions file
              <input
                readOnly
                value={configPath}
                onFocus={(event) => event.currentTarget.select()}
              />
            </label>
          )}
          {!project && <p className="muted">Save this project first to customize its actions.</p>}
          <div className="dialog-actions">
            <button type="button" disabled={busy} onClick={close}>
              Cancel
            </button>
            <button disabled={busy || !name.trim() || !directory}>Save project</button>
          </div>
        </form>
      </section>
      <section
        id="settings-panel-actions"
        role="tabpanel"
        aria-labelledby="settings-actions"
        hidden={view !== 'actions'}
        className="action-form"
      >
        <p className="muted">
          The first terminal action starts automatically in new sessions. External commands open
          applications without a tab.
        </p>
        <section className="common-env" aria-label="Common environment variables">
          <h3>Common environment variables</h3>
          <p className="muted">
            Shared by all actions. Action-specific overrides take precedence. Changes apply to
            future launches.
          </p>
          {commonEnv.map((row) => (
            <div className="env-row" key={row.id}>
              <label>
                Name
                <input
                  aria-label="Common variable name"
                  disabled={busy}
                  value={row.name}
                  onChange={(event) => editEnv(row.id, { name: event.target.value })}
                />
              </label>
              <label>
                Value
                <input
                  aria-label="Common variable value"
                  disabled={busy}
                  value={row.value}
                  onChange={(event) => editEnv(row.id, { value: event.target.value })}
                />
              </label>
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setCommonEnv((current) => current.filter((item) => item.id !== row.id));
                  setDirty(true);
                  setNotice('');
                }}
              >
                Remove variable
              </button>
            </div>
          ))}
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              setCommonEnv((current) => [
                ...current,
                {
                  id: `env-${crypto.getRandomValues(new Uint32Array(2)).join('-')}`,
                  name: '',
                  value: '',
                },
              ]);
              setDirty(true);
              setNotice('');
            }}
          >
            Add variable
          </button>
        </section>
        {draft.map((action, index) => (
          <fieldset key={action.id} disabled={busy}>
            <legend>
              {index + 1}. {action.name || 'New action'}
            </legend>
            <label>
              Name
              <input
                aria-label="Action name"
                value={action.name}
                onChange={(event) => patch(action.id, { name: event.target.value })}
              />
            </label>
            <label>
              Action type
              <select
                aria-label="Action type"
                value={action.kind}
                onChange={(event) =>
                  patch(action.id, {
                    kind: event.target.value,
                    configVersion: 1,
                    pluginConfig: actionEditor(event.target.value)!.toDraft(
                      event.target.value === 'terminal' || event.target.value === 'command'
                        ? { executable: 'bash' }
                        : {},
                    ),
                  })
                }
              >
                {plugins.map((plugin) => (
                  <option key={plugin.id} value={plugin.id}>
                    {plugin.label} tab
                  </option>
                ))}
                <option value="command">External command</option>
              </select>
            </label>
            {action.kind === 'command' && (
              <p className="muted">
                Runs independently of session tabs. Arguments and directory support{' '}
                {'{{session.path}}'}, {'{{session.name}}'} and {'{{project.path}}'}. Output is
                discarded. Use SESSION_PATH and PROJECT_PATH environment variables in shell-command
                mode.
              </p>
            )}
            {(() => {
              const Editor = actionEditor(action.kind)?.ConfigurationEditor;
              return Editor ? (
                <Editor
                  value={action.pluginConfig}
                  onChange={(pluginConfig) => patch(action.id, { pluginConfig })}
                />
              ) : (
                <p>Plugin unavailable: {action.kind}</p>
              );
            })()}
            <div className="action-controls">
              <button type="button" disabled={index === 0} onClick={() => move(index, -1)}>
                Move earlier
              </button>
              <button
                type="button"
                disabled={index === draft.length - 1}
                onClick={() => move(index, 1)}
              >
                Move later
              </button>
              <button
                type="button"
                disabled={draft.length <= 1}
                onClick={() => {
                  setDraft((current) => current.filter((item) => item.id !== action.id));
                  setDirty(true);
                  setNotice('');
                }}
              >
                Remove action
              </button>
            </div>
          </fieldset>
        ))}
        <button
          type="button"
          disabled={busy || draft.length >= 100}
          onClick={() => {
            setDraft((current) => [
              ...current,
              {
                id: `action-${crypto.getRandomValues(new Uint32Array(2)).join('-')}`,
                name: 'New action',
                kind: 'terminal',
                configVersion: 1,
                pluginConfig: actionEditor('terminal')!.toDraft({ executable: 'bash' }),
              },
            ]);
            setDirty(true);
            setNotice('');
          }}
        >
          Add action
        </button>
      </section>
      <section
        id="settings-panel-yaml"
        role="tabpanel"
        aria-labelledby="settings-yaml"
        hidden={view !== 'yaml'}
        className="action-yaml"
      >
        <label htmlFor="project-actions-yaml">Actions (YAML)</label>
        <textarea
          id="project-actions-yaml"
          spellCheck={false}
          maxLength={65536}
          value={yaml}
          disabled={busy || !loaded}
          onChange={(event) => {
            setYaml(event.target.value);
            setDirty(true);
            setNotice('');
          }}
        />
        <p className="muted">
          Use the actions list. The first terminal entry is the default for new sessions.
        </p>
      </section>
      {(error || externalError) && (
        <p role="alert" className="error">
          {error || externalError}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {dirty && <p className="muted">Unsaved action changes</p>}
      {view !== 'general' && (
        <div className="dialog-actions settings-footer">
          <button type="button" disabled={busy} onClick={close}>
            Close
          </button>
          <button
            type="button"
            disabled={busy || !loaded || !dirty}
            onClick={() => void saveActions()}
          >
            Save actions
          </button>
        </div>
      )}
    </dialog>
  );
}
