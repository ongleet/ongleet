import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { TerminalFrontend, type ProcessStatus } from './terminal';
import './style.css';

type TabConfig = { cwdOverride: string | null; executable: string | null; args: string[]; env: Record<string, string>; shellCommand: string | null };
type TerminalTab = { id: number; name: string; displayOrder: number; config: TabConfig };
type Workspace = { id: number; name: string; baseDirectory: string; displayOrder: number; tabs: TerminalTab[]; selectedTabId: number | null };
type WorkspaceSnapshot = { workspaces: Workspace[]; selectedId: number | null };
type Report = (error: unknown) => void;

function WorkspaceEditor({ workspace, initialDirectory, save, dismiss, externalError }: {
  workspace?: Workspace; initialDirectory: string;
  save: (name: string, directory: string) => Promise<boolean>; dismiss: () => void; externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(workspace?.name ?? '');
  const [directory, setDirectory] = useState(workspace?.baseDirectory ?? initialDirectory);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => { dialog.current?.showModal(); }, []);

  async function browse() {
    setBusy(true); setError('');
    try {
      const path = await open({ directory: true, multiple: false, title: 'Choose workspace directory' });
      if (typeof path === 'string') setDirectory(path);
    } catch (error) { setError(String(error)); } finally { setBusy(false); }
  }

  return <dialog ref={dialog} aria-labelledby="workspace-editor-title" onCancel={event => {
    event.preventDefault(); if (!busy) dismiss();
  }}>
    <form onSubmit={event => {
      event.preventDefault(); setBusy(true);
      void save(name, directory).then(saved => { if (saved) dismiss(); }).finally(() => setBusy(false));
    }}>
      <h2 id="workspace-editor-title">{workspace ? 'Edit workspace' : 'New workspace'}</h2>
      <label>Name<input autoFocus required value={name} onChange={event => setName(event.target.value)} disabled={busy} /></label>
      <label>Base directory<input required value={directory} onChange={event => setDirectory(event.target.value)} disabled={busy} /></label>
      <button type="button" onClick={() => void browse()} disabled={busy}>Choose directory…</button>
      {workspace && <p className="muted">Directory changes apply to future launches. Running processes keep their working directory.</p>}
      {(error || externalError) && <p role="alert" className="error">{error || externalError}</p>}
      <div className="dialog-actions"><button type="button" onClick={dismiss} disabled={busy}>Cancel</button><button disabled={busy || !name.trim() || !directory}>{busy ? 'Please wait…' : 'Save workspace'}</button></div>
    </form>
  </dialog>;
}

function TabTerminal({ workspace, tab, visible, shell, disabled, markBusy, reportStatus, onState }: {
  workspace: Workspace; tab: TerminalTab; visible: boolean; shell: string; disabled: boolean;
  markBusy: (busy: boolean) => void; reportStatus: (id: number, status: ProcessStatus | null) => void;
  onState: (state: WorkspaceSnapshot) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const frontend = useRef<TerminalFrontend | null>(null);
  const [cwdOverride, setCwdOverride] = useState(tab.config.cwdOverride ?? '');
  const [executable, setExecutable] = useState(tab.config.executable ?? '');
  const [args, setArgs] = useState(JSON.stringify(tab.config.args));
  const [environment, setEnvironment] = useState(JSON.stringify(tab.config.env));
  const [shellMode, setShellMode] = useState(tab.config.shellCommand !== null);
  const [shellCommand, setShellCommand] = useState(tab.config.shellCommand ?? '');
  const [status, setStatus] = useState<ProcessStatus | null>(null);
  const [error, setError] = useState('');
  const [snapshot, setSnapshot] = useState<string | null>(null);
  const report: Report = error => setError(String(error));

  useEffect(() => {
    const terminal = new TerminalFrontend(host.current!, tab.id, status => {
      setStatus(status); reportStatus(tab.id, status);
    }, report);
    frontend.current = terminal;
    return () => { terminal.dispose(); frontend.current = null; };
  }, [tab.id, reportStatus]);
  useEffect(() => { frontend.current?.setVisible(visible); }, [visible]);

  async function save(start: boolean, codex = false) {
    if (start && status?.state === 'exited' && !window.confirm('Restart this tab and terminate any jobs left by its previous session?')) return;
    markBusy(true); setError('');
    try {
      const parsedArgs: unknown = codex || shellMode ? [] : JSON.parse(args);
      if (!Array.isArray(parsedArgs) || !parsedArgs.every(arg => typeof arg === 'string')) throw new Error('Arguments must be a JSON array of strings.');
      const env: unknown = JSON.parse(environment);
      if (!env || typeof env !== 'object' || Array.isArray(env) || !Object.values(env).every(value => typeof value === 'string')) throw new Error('Environment overrides must be a JSON object with string values.');
      const config: TabConfig = {
        cwdOverride: cwdOverride || null, executable: codex ? 'codex' : shellMode ? null : executable || null,
        args: parsedArgs, env: env as Record<string, string>, shellCommand: !codex && shellMode ? shellCommand : null,
      };
      onState(await invoke<WorkspaceSnapshot>('configure_tab', { id: tab.id, config }));
      if (codex) { setExecutable('codex'); setArgs('[]'); setShellMode(false); }
      if (start) {
        if (status?.state === 'exited') await frontend.current!.close();
        setSnapshot(null); await frontend.current!.start();
      }
    } catch (error) { report(error); } finally { markBusy(false); }
  }

  return <section id={`tab-panel-${tab.id}`} role="tabpanel" aria-labelledby={`tab-${tab.id}`} className="tab-terminal" hidden={!visible}>
    <details className="launch-settings" open={!status}>
      <summary>Launch settings</summary>
      <form className="launch-form" onSubmit={event => { event.preventDefault(); void save(true); }}>
        <label>Working directory override<input value={cwdOverride} onChange={event => setCwdOverride(event.target.value)} disabled={!!status || disabled} placeholder={workspace.baseDirectory} /></label>
        <label>Launch mode<select value={shellMode ? 'shell' : 'executable'} onChange={event => setShellMode(event.target.value === 'shell')} disabled={!!status || disabled}>
          <option value="executable">Executable + arguments</option><option value="shell">Shell command</option>
        </select></label>
        {shellMode ? <label>Shell command<input value={shellCommand} onChange={event => setShellCommand(event.target.value)} disabled={!!status || disabled} placeholder="Explicit shell syntax" required /></label> : <>
          <label>Executable<input value={executable} onChange={event => setExecutable(event.target.value)} disabled={!!status || disabled} placeholder={shell || 'User shell'} /></label>
          <label>Arguments (JSON array)<input value={args} onChange={event => setArgs(event.target.value)} disabled={!!status || disabled} /></label>
        </>}
        <label>Environment overrides (JSON)<input value={environment} onChange={event => setEnvironment(event.target.value)} disabled={!!status || disabled} placeholder={'{"PORT":"3000"}'} /></label>
        <div className="launch-buttons"><button disabled={status?.state === 'running' || disabled}>{status?.state === 'exited' ? 'Restart' : 'Start'}</button>
          <button type="button" disabled={!!status || disabled} onClick={() => void save(true, true)}>Run Codex</button>
          <button type="button" disabled={!!status || disabled} onClick={() => void save(false)}>Save settings</button></div>
      </form>
    </details>
    <nav className="terminal-actions" aria-label={`${tab.name} terminal actions`}>
      <button disabled={!status || disabled} onClick={() => void frontend.current?.copy().catch(report)}>Copy selection</button>
      <button disabled={!status || disabled} onClick={() => void frontend.current?.paste().catch(report)}>Paste</button>
      <button disabled={!status || disabled} onClick={() => { try { setSnapshot(frontend.current!.snapshot()); } catch (error) { report(error); } }}>Inspect screen</button>
      <button disabled={!status || disabled} onClick={() => {
        if (!window.confirm('Stop this terminal session and its jobs? The tab will stay available.')) return;
        markBusy(true);
        void frontend.current?.close().catch(report).finally(() => markBusy(false));
      }}>Stop session</button>
      <output aria-live="polite">{status?.state === 'exited' ? `Exited (${status.code})` : status ? 'Running' : 'Stopped'}</output>
    </nav>
    {error && <div role="alert" className="error">{error}</div>}
    <div ref={host} className="terminal" />
    {snapshot !== null && <div className="snapshot" aria-label="Plain-text snapshot"><button onClick={() => setSnapshot(null)}>Dismiss snapshot</button><pre>{snapshot}</pre></div>}
  </section>;
}

function RenameTabDialog({ tab, save, dismiss, error }: { tab: TerminalTab; save: (name: string) => Promise<boolean>; dismiss: () => void; error: string }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(tab.name);
  const [busy, setBusy] = useState(false);
  useEffect(() => { dialog.current?.showModal(); }, []);
  return <dialog ref={dialog} aria-labelledby="rename-tab-title" onCancel={event => { event.preventDefault(); if (!busy) dismiss(); }}>
    <form onSubmit={event => { event.preventDefault(); setBusy(true); void save(name).then(saved => { if (saved) dismiss(); }).finally(() => setBusy(false)); }}>
      <h2 id="rename-tab-title">Rename tab</h2><label>Name<input autoFocus required disabled={busy} value={name} onChange={event => setName(event.target.value)} /></label>
      {error && <p role="alert" className="error">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={dismiss}>Cancel</button><button disabled={busy || !name.trim()}>Rename</button></div>
    </form>
  </dialog>;
}

function WorkspacePane({ workspace, visible, statuses, disabled, shell, markBusy, reportStatus, mutate, onState, rename, closeTab }: {
  workspace: Workspace; visible: boolean; statuses: Record<number, ProcessStatus | null>; disabled: boolean; shell: string;
  markBusy: (busy: boolean) => void; reportStatus: (id: number, status: ProcessStatus | null) => void;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>; onState: (state: WorkspaceSnapshot) => void;
  rename: (tab: TerminalTab) => void; closeTab: (tab: TerminalTab) => void;
}) {
  const active = workspace.tabs.find(tab => tab.id === workspace.selectedTabId);
  function navigate(event: React.KeyboardEvent, index: number) {
    const length = workspace.tabs.length;
    const target = event.key === 'ArrowRight' ? (index + 1) % length : event.key === 'ArrowLeft' ? (index + length - 1) % length : event.key === 'Home' ? 0 : event.key === 'End' ? length - 1 : null;
    if (target === null) return;
    event.preventDefault();
    const id = workspace.tabs[target].id;
    void mutate('select_tab', { id }).then(selected => { if (selected) document.getElementById(`tab-${id}`)?.focus(); });
  }
  return <section className="workspace-pane" hidden={!visible} aria-label={workspace.name}>
    <header className="workspace-heading"><div><h1>{workspace.name}</h1><p title={workspace.baseDirectory}>{workspace.baseDirectory}</p></div></header>
    <div className="tab-bar">
      <div role="tablist" aria-label={`${workspace.name} terminals`} className="tab-list">
        {workspace.tabs.map((tab, index) => <button key={tab.id} id={`tab-${tab.id}`} role="tab" aria-selected={tab.id === active?.id} aria-controls={`tab-panel-${tab.id}`} tabIndex={tab.id === active?.id ? 0 : -1}
          disabled={disabled} onKeyDown={event => navigate(event, index)} onClick={() => void mutate('select_tab', { id: tab.id })} title={`${tab.name} — ${statuses[tab.id]?.state ?? 'stopped'}`}>
          <span className={`status-dot ${statuses[tab.id]?.state ?? 'stopped'}`} /><span className="tab-name">{tab.name}</span>
          {statuses[tab.id]?.state === 'exited' && <span>({(statuses[tab.id] as { code: number }).code})</span>}
        </button>)}
      </div>
      <button disabled={disabled} aria-label="Create terminal tab" onClick={() => void mutate('create_tab', { workspaceId: workspace.id })}>+ New tab</button>
    </div>
    {active && <nav className="tab-actions" aria-label="Selected tab actions">
      <button disabled={disabled} onClick={() => rename(active)}>Rename tab</button>
      <button disabled={disabled || active.displayOrder === 0} aria-label="Move tab left" onClick={() => void mutate('reorder_tab', { id: active.id, position: active.displayOrder - 1 })}>←</button>
      <button disabled={disabled || active.displayOrder === workspace.tabs.length - 1} aria-label="Move tab right" onClick={() => void mutate('reorder_tab', { id: active.id, position: active.displayOrder + 1 })}>→</button>
      <button disabled={disabled} onClick={() => closeTab(active)}>Close tab</button>
    </nav>}
    {!active && <div className="empty-state"><p>No terminal tabs in this workspace.</p><button disabled={disabled} onClick={() => void mutate('create_tab', { workspaceId: workspace.id })}>Create terminal tab</button></div>}
    {workspace.tabs.map(tab => <TabTerminal key={tab.id} workspace={workspace} tab={tab} visible={visible && tab.id === active?.id} shell={shell} disabled={disabled} markBusy={markBusy} reportStatus={reportStatus} onState={onState} />)}
  </section>;
}

function App() {
  const [state, setState] = useState<WorkspaceSnapshot>({ workspaces: [], selectedId: null });
  const [defaults, setDefaults] = useState<[string, string]>(['', '']);
  const [statuses, setStatuses] = useState<Record<number, ProcessStatus | null>>({});
  const [editor, setEditor] = useState<Workspace | 'new' | null>(null);
  const [renaming, setRenaming] = useState<TerminalTab | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [ready, setReady] = useState(false);
  const busyRef = useRef(false);
  const report: Report = error => setError(String(error));
  const markBusy = useCallback((busy: boolean) => { busyRef.current = busy; setBusy(busy); }, []);
  const reportStatus = useCallback((id: number, status: ProcessStatus | null) => {
    setStatuses(previous => {
      const old = previous[id];
      if (old?.state === status?.state && (old?.state !== 'exited' || status?.state !== 'exited' || old.code === status.code)) return previous;
      return { ...previous, [id]: status };
    });
  }, []);

  useEffect(() => {
    if (!isTauri()) { report('Native backend unavailable. Start with mise exec -- npm run tauri -- dev.'); return; }
    void Promise.all([invoke<WorkspaceSnapshot>('workspaces'), invoke<[string, string]>('defaults')])
      .then(([state, defaults]) => { setState(state); setDefaults(defaults); setReady(true); }).catch(report);
    let disposed = false;
    const unlisten = getCurrentWindow().onCloseRequested(async event => {
      event.preventDefault();
      if (busyRef.current) { report('Please wait for the current operation to finish before quitting.'); return; }
      if (!window.confirm('Quit and terminate terminal sessions in all workspaces?')) return;
      markBusy(true);
      try { await invoke('shutdown', { confirmed: true }); await getCurrentWindow().destroy(); }
      catch (error) { report(error); markBusy(false); }
    });
    void unlisten.then(off => { if (disposed) off(); });
    return () => { disposed = true; void unlisten.then(off => off()); };
  }, [markBusy]);

  async function mutate(command: string, args: Record<string, unknown>): Promise<boolean> {
    markBusy(true); setError('');
    try { setState(await invoke<WorkspaceSnapshot>(command, args)); return true; }
    catch (error) { report(error); return false; }
    finally { markBusy(false); }
  }

  const selected = state.workspaces.find(workspace => workspace.id === state.selectedId);
  return <main className="app">
    <aside className="sidebar" aria-label="Workspaces">
      <div className="sidebar-heading"><strong>Workspaces</strong><button disabled={!ready || busy} onClick={() => setEditor('new')} aria-label="Create workspace" title="Create workspace">+</button></div>
      <nav className="workspace-list" aria-label="Select workspace"><ol>
        {state.workspaces.map(workspace => <li key={workspace.id}>
          <button className="workspace-choice" aria-current={state.selectedId === workspace.id ? 'page' : undefined}
            disabled={busy} title={workspace.baseDirectory} onClick={() => void mutate('select_workspace', { id: workspace.id })}>
            <span className={`status-dot ${workspace.tabs.some(tab => statuses[tab.id]?.state === 'running') ? 'running' : workspace.tabs.some(tab => statuses[tab.id]?.state === 'exited') ? 'exited' : 'stopped'}`} aria-label={workspace.tabs.some(tab => statuses[tab.id]?.state === 'running') ? 'running' : workspace.tabs.some(tab => statuses[tab.id]?.state === 'exited') ? 'exited' : 'stopped'} />
            <span className="workspace-name">{workspace.name}</span>
          </button>
        </li>)}
      </ol></nav>
      {selected && <div className="workspace-management" aria-label="Selected workspace actions">
        <button disabled={busy} onClick={() => setEditor(selected)}>Edit workspace</button>
        <div className="reorder-actions">
          <button disabled={busy || selected.displayOrder === 0} aria-label="Move workspace up" onClick={() => void mutate('reorder_workspace', { id: selected.id, position: selected.displayOrder - 1 })}>↑ Up</button>
          <button disabled={busy || selected.displayOrder === state.workspaces.length - 1} aria-label="Move workspace down" onClick={() => void mutate('reorder_workspace', { id: selected.id, position: selected.displayOrder + 1 })}>↓ Down</button>
        </div>
        <button disabled={busy} onClick={() => {
          const confirmed = selected.tabs.some(tab => !!statuses[tab.id]);
          if (confirmed && !window.confirm(`Close “${selected.name}” and terminate all its terminal sessions and jobs?`)) return;
          void mutate('close_workspace', { id: selected.id, confirmed }).then(closed => {
            if (closed) setStatuses(previous => { const next = { ...previous }; for (const tab of selected.tabs) delete next[tab.id]; return next; });
          });
        }}>Close workspace</button>
      </div>}
      <p className="sidebar-note">Workspaces are kept for this app session. Saving across restarts comes next.</p>
    </aside>
    <div className="main-pane">
      {error && <div role="alert" className="error">{error}</div>}
      {!state.workspaces.length && <div className="empty-state"><h1>Your terminal workspace</h1><p>Create a workspace pointing to an existing directory.</p><button disabled={!ready || busy} onClick={() => setEditor('new')}>Create workspace</button></div>}
      {state.workspaces.map(workspace => <WorkspacePane key={workspace.id} workspace={workspace} visible={workspace.id === state.selectedId} statuses={statuses} shell={defaults[1]} disabled={busy} markBusy={markBusy} reportStatus={reportStatus} mutate={mutate} onState={setState}
        rename={tab => { setError(''); setRenaming(tab); }} closeTab={tab => {
          const confirmed = !!statuses[tab.id];
          if (confirmed && !window.confirm(`Close “${tab.name}” and terminate its terminal session and jobs?`)) return;
          void mutate('close_tab', { id: tab.id, confirmed }).then(closed => {
            if (closed) setStatuses(previous => { const next = { ...previous }; delete next[tab.id]; return next; });
          });
        }} />)}
      <footer>Ctrl+Shift+C / V: copy / paste · Arrow keys: navigate terminal tabs</footer>
    </div>
    {renaming && <RenameTabDialog tab={renaming} error={error} dismiss={() => setRenaming(null)} save={name => mutate('rename_tab', { id: renaming.id, name })} />}
    {editor && <WorkspaceEditor key={editor === 'new' ? 'new' : editor.id} workspace={editor === 'new' ? undefined : editor} initialDirectory={defaults[0]} dismiss={() => setEditor(null)} externalError={error}
      save={(name, directory) => mutate(editor === 'new' ? 'create_workspace' : 'update_workspace', { ...(editor === 'new' ? {} : { id: editor.id }), name, directory })} />}
  </main>;
}

createRoot(document.getElementById('root')!).render(<App />);
