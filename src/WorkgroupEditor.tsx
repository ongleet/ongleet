import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Workgroup } from './main';

type Action = { id: string; name: string; kind: string; config: {
  executable: string | null; args: string[]; env: Record<string, string>;
  cwdOverride: string | null; shellCommand: string | null;
} };
type Draft = { id: string; name: string; kind: string; mode: 'executable' | 'shell'; executable: string;
  args: string; env: string; cwd: string; shell: string };
type Settings = { env: Record<string, string>; actions: Action[] };
type EnvRow = { id: string; name: string; value: string };
const envRows = (env: Record<string, string>): EnvRow[] => Object.entries(env).map(([name, value], index) => ({ id: `env-${index}`, name, value }));
type View = 'general' | 'actions' | 'yaml';
const drafts = (actions: Action[]): Draft[] => actions.map(action => ({ id: action.id, name: action.name, kind: action.kind,
  mode: action.config.shellCommand === null ? 'executable' : 'shell', executable: action.config.executable ?? '',
  args: JSON.stringify(action.config.args), env: JSON.stringify(action.config.env),
  cwd: action.config.cwdOverride ?? '', shell: action.config.shellCommand ?? '' }));
function actionsFrom(draft: Draft[]): Action[] {
  return draft.map(action => {
    const args: unknown = action.mode === 'executable' ? JSON.parse(action.args) : [];
    const env: unknown = JSON.parse(action.env);
    if (!Array.isArray(args) || !args.every(value => typeof value === 'string')) throw new Error(`${action.name}: arguments must be a JSON array of strings.`);
    if (!env || typeof env !== 'object' || Array.isArray(env) || !Object.values(env).every(value => typeof value === 'string')) throw new Error(`${action.name}: environment must be a JSON object of strings.`);
    return { id: action.id, name: action.name, kind: action.kind, config: {
      executable: action.mode === 'executable' ? action.executable : null, args,
      env: env as Record<string, string>, cwdOverride: action.cwd.trim() || null,
      shellCommand: action.mode === 'shell' ? action.shell : null,
    } };
  });
}

export function WorkgroupEditor({ workgroup, initialDirectory, save, dismiss, externalError }: {
  workgroup?: Workgroup; initialDirectory: string; save: (name: string, rootDirectory: string) => Promise<boolean>;
  dismiss: () => void; externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(workgroup?.name ?? '');
  const [directory, setDirectory] = useState(workgroup?.rootDirectory ?? initialDirectory);
  const [configPath, setConfigPath] = useState('');
  const [view, setView] = useState<View>('general');
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
    if (workgroup) {
      void invoke<string>('workgroup_config', { id: workgroup.id }).then(value => { if (!disposed) setConfigPath(value); }).catch(() => {});
      void invoke<string>('workgroup_actions', { id: workgroup.id }).then(async text => {
        if (disposed) return;
        setYaml(text); setLoaded(true);
        try {
          const settings = await invoke<Settings>('parse_actions', { yaml: text });
          if (!disposed) { setDraft(drafts(settings.actions)); setCommonEnv(envRows(settings.env)); }
        } catch (error) { if (!disposed) { setView('yaml'); setError(String(error)); } }
      }).catch(error => { if (!disposed) setError(String(error)); });
    }
    return () => { disposed = true; };
  }, [workgroup?.id]);

  function settingsFromForm(): Settings {
    const names = new Set<string>();
    for (const row of commonEnv) {
      if (!row.name || names.has(row.name)) throw new Error('Common environment variables require unique, nonempty names.');
      names.add(row.name);
    }
    return { env: Object.fromEntries(commonEnv.map(row => [row.name, row.value])), actions: actionsFrom(draft) };
  }
  function editEnv(id: string, changes: Partial<EnvRow>) {
    setCommonEnv(current => current.map(row => row.id === id ? { ...row, ...changes } : row));
    setDirty(true); setNotice('');
  }
  async function switchView(next: View) {
    if (next === view || busy) return;
    setBusy(true); setError(''); setNotice('');
    try {
      if (view === 'actions') setYaml(await invoke<string>('format_actions', { config: settingsFromForm() }));
      if (next === 'actions') { const settings = await invoke<Settings>('parse_actions', { yaml }); setDraft(drafts(settings.actions)); setCommonEnv(envRows(settings.env)); }
      setView(next);
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  async function saveActions() {
    if (!workgroup) return;
    setBusy(true); setError(''); setNotice('');
    try {
      const text = view === 'actions' ? await invoke<string>('format_actions', { config: settingsFromForm() }) : yaml;
      await invoke('save_workgroup_actions', { id: workgroup.id, yaml: text });
      setYaml(text); setDirty(false); setNotice('Actions saved. New launches will use these settings.');
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  function patch(id: string, changes: Partial<Draft>) {
    setDraft(current => current.map(action => action.id === id ? { ...action, ...changes } : action));
    setDirty(true); setNotice('');
  }
  function move(index: number, offset: number) {
    setDraft(current => { const next = [...current]; const [action] = next.splice(index, 1); next.splice(index + offset, 0, action); return next; });
    setDirty(true); setNotice('');
  }
  function close() {
    if (!busy) dismiss();
  }
  return <dialog ref={dialog} aria-label="Workgroup editor" onCancel={event => { event.preventDefault(); close(); }}>
    <h2>{workgroup ? 'Edit workgroup' : 'New workgroup'}</h2>
    {workgroup && <div role="tablist" aria-label="Workgroup settings" className="settings-tabs" onKeyDown={event => {
      const views: View[] = ['general', 'actions', 'yaml'];
      const index = views.indexOf(view);
      const next = event.key === 'ArrowRight' ? (index + 1) % 3 : event.key === 'ArrowLeft' ? (index + 2) % 3 : event.key === 'Home' ? 0 : event.key === 'End' ? 2 : null;
      if (next !== null && !busy) { event.preventDefault(); void switchView(views[next]); document.getElementById(`settings-${views[next]}`)?.focus(); }
    }}>
      {(['general', 'actions', 'yaml'] as View[]).map(item => <button key={item} id={`settings-${item}`} type="button" role="tab" aria-selected={view === item} aria-controls={`settings-panel-${item}`} tabIndex={view === item ? 0 : -1} disabled={busy || (item !== 'general' && !loaded)} onClick={() => void switchView(item)}>{item === 'general' ? 'General' : item === 'actions' ? 'Actions' : 'YAML'}</button>)}
    </div>}
    <section id="settings-panel-general" role="tabpanel" aria-labelledby="settings-general" hidden={view !== 'general'}>
      <form onSubmit={event => { event.preventDefault(); setBusy(true); void save(name, directory).then(saved => { if (saved) { if (dirty) setNotice('Workgroup saved. Save your action changes in Actions or YAML.'); else dismiss(); } }).finally(() => setBusy(false)); }}>
        <label>Name<input required autoFocus value={name} disabled={busy} onChange={event => setName(event.target.value)} /></label>
        <label>Root directory<input required value={directory} disabled={busy} onChange={event => setDirectory(event.target.value)} /></label>
        <button type="button" disabled={busy} onClick={() => { setBusy(true); void open({ directory: true, multiple: false, title: 'Choose workgroup root' }).then(path => { if (typeof path === 'string') setDirectory(path); }).catch(error => setError(String(error))).finally(() => setBusy(false)); }}>Choose directory…</button>
        <p className="muted">The root is the default directory for new workspaces. Existing paths stay unchanged.</p>
        {configPath && <label>Actions file<input readOnly value={configPath} onFocus={event => event.currentTarget.select()} /></label>}
        {!workgroup && <p className="muted">Save this workgroup first to customize its actions.</p>}
        <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>Cancel</button><button disabled={busy || !name.trim() || !directory}>Save workgroup</button></div>
      </form>
    </section>
    <section id="settings-panel-actions" role="tabpanel" aria-labelledby="settings-actions" hidden={view !== 'actions'} className="action-form">
      <p className="muted">The first terminal action starts automatically in new workspaces. External commands open applications without a tab.</p>
      <section className="common-env" aria-label="Common environment variables">
        <h3>Common environment variables</h3>
        <p className="muted">Shared by all actions in this workgroup. Action-specific overrides take precedence. Changes apply to future launches.</p>
        {commonEnv.map(row => <div className="env-row" key={row.id}>
          <label>Name<input aria-label="Common variable name" disabled={busy} value={row.name} onChange={event => editEnv(row.id, { name: event.target.value })} /></label>
          <label>Value<input aria-label="Common variable value" disabled={busy} value={row.value} onChange={event => editEnv(row.id, { value: event.target.value })} /></label>
          <button type="button" disabled={busy} onClick={() => { setCommonEnv(current => current.filter(item => item.id !== row.id)); setDirty(true); setNotice(''); }}>Remove variable</button>
        </div>)}
        <button type="button" disabled={busy} onClick={() => { setCommonEnv(current => [...current, { id: `env-${crypto.getRandomValues(new Uint32Array(2)).join('-')}`, name: '', value: '' }]); setDirty(true); setNotice(''); }}>Add variable</button>
      </section>
      {draft.map((action, index) => <fieldset key={action.id} disabled={busy}>
        <legend>{index + 1}. {action.name || 'New action'}</legend>
        <label>Name<input aria-label="Action name" value={action.name} onChange={event => patch(action.id, { name: event.target.value })} /></label>
        <label>Action type<select aria-label="Action type" value={action.kind} onChange={event => patch(action.id, { kind: event.target.value })}><option value="terminal">Terminal tab</option><option value="command">External command</option></select></label>
        {action.kind === 'command' && <p className="muted">Runs independently of workspace tabs. Arguments and directory support {'{{workspace.path}}'}, {'{{workspace.name}}'} and {'{{workgroup.path}}'}. Output is discarded. Use WORKSPACE_PATH and WORKGROUP_PATH environment variables in shell-command mode.</p>}
        <label>Launch mode<select value={action.mode} onChange={event => patch(action.id, { mode: event.target.value as Draft['mode'] })}><option value="executable">Executable + arguments</option><option value="shell">Shell command</option></select></label>
        {action.mode === 'executable' ? <>
          <label>Executable<input aria-label="Executable" value={action.executable} onChange={event => patch(action.id, { executable: event.target.value })} /></label>
          <label>Arguments (JSON array)<input aria-label="Arguments" value={action.args} onChange={event => patch(action.id, { args: event.target.value })} /></label>
        </> : <label>Shell command<input aria-label="Shell command" value={action.shell} onChange={event => patch(action.id, { shell: event.target.value })} /></label>}
        <label>Directory override<input value={action.cwd} placeholder="Workspace directory" onChange={event => patch(action.id, { cwd: event.target.value })} /></label>
        <label>Environment overrides (JSON)<input aria-label="Environment overrides" value={action.env} onChange={event => patch(action.id, { env: event.target.value })} /></label>
        <div className="action-controls">
          <button type="button" disabled={index === 0} onClick={() => move(index, -1)}>Move earlier</button>
          <button type="button" disabled={index === draft.length - 1} onClick={() => move(index, 1)}>Move later</button>
          <button type="button" disabled={draft.length <= 1} onClick={() => { setDraft(current => current.filter(item => item.id !== action.id)); setDirty(true); setNotice(''); }}>Remove action</button>
        </div>
      </fieldset>)}
      <button type="button" disabled={busy || draft.length >= 100} onClick={() => { setDraft(current => [...current, { id: `action-${crypto.getRandomValues(new Uint32Array(2)).join('-')}`, name: 'New action', kind: 'terminal', mode: 'executable', executable: 'bash', args: '[]', env: '{}', cwd: '', shell: '' }]); setDirty(true); setNotice(''); }}>Add action</button>
    </section>
    <section id="settings-panel-yaml" role="tabpanel" aria-labelledby="settings-yaml" hidden={view !== 'yaml'} className="action-yaml">
      <label htmlFor="workgroup-actions-yaml">Actions (YAML)</label>
      <textarea id="workgroup-actions-yaml" spellCheck={false} maxLength={65536} value={yaml} disabled={busy || !loaded} onChange={event => { setYaml(event.target.value); setDirty(true); setNotice(''); }} />
      <p className="muted">Use the actions list. The first terminal entry is the default for new workspaces.</p>
    </section>
    {(error || externalError) && <p role="alert" className="error">{error || externalError}</p>}
    {notice && <p role="status">{notice}</p>}
    {dirty && <p className="muted">Unsaved action changes</p>}
    {view !== 'general' && <div className="dialog-actions settings-footer"><button type="button" disabled={busy} onClick={close}>Close</button><button type="button" disabled={busy || !loaded || !dirty} onClick={() => void saveActions()}>Save actions</button></div>}
  </dialog>;
}
