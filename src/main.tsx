import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { TerminalFrontend, type ProcessStatus } from './terminal';
import { WorkspaceSidebar } from './WorkspaceSidebar';
import './style.css';

type TabConfig = { cwdOverride: string | null; executable: string | null; args: string[]; env: Record<string, string>; shellCommand: string | null };
type TerminalTab = { id: number; kind: string; isOpen: boolean; resumeOnStart: boolean; name: string; displayOrder: number; config: TabConfig };
export type Workspace = { id: number; name: string; baseDirectory: string; displayOrder: number; tabs: TerminalTab[]; selectedTabId: number | null };
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

function TabTerminal({ workspace, tab, visible, shell, disabled, markBusy, reportStatus, onState, markRestoring }: {
  workspace: Workspace; tab: TerminalTab; visible: boolean; shell: string; disabled: boolean;
  markBusy: (busy: boolean) => void; reportStatus: (id: number, status: ProcessStatus | null) => void;
  onState: (state: WorkspaceSnapshot) => void; markRestoring: (delta: number) => void;
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
    // Only the initial attachment may resume a restored/reopened tab. Later
    // metadata snapshots must never launch a second process for a live tab.
    if (tab.resumeOnStart) {
      markRestoring(1);
      void terminal.start().catch(report).finally(() => markRestoring(-1));
    }
    return () => { terminal.dispose(); frontend.current = null; };
  }, [tab.id, reportStatus, markRestoring]);
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

function WorkspacePane({ workspace, visible, statuses, disabled, shell, markBusy, reportStatus, mutate, onState, closeTab, markRestoring }: {
  workspace: Workspace; visible: boolean; statuses: Record<number, ProcessStatus | null>; disabled: boolean; shell: string;
  markBusy: (busy: boolean) => void; reportStatus: (id: number, status: ProcessStatus | null) => void;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>; onState: (state: WorkspaceSnapshot) => void;
  closeTab: (tab: TerminalTab) => void; markRestoring: (delta: number) => void;
}) {
  const openTabs = workspace.tabs.filter(tab => tab.isOpen);
  const availableTabs = workspace.tabs.filter(tab => !tab.isOpen);
  const active = openTabs.find(tab => tab.id === workspace.selectedTabId);
  const [dragTabId, setDragTabId] = useState<number | null>(null);
  const [tabDrop, setTabDrop] = useState<{ id: number; after: boolean } | null>(null);
  function navigate(event: React.KeyboardEvent, index: number) {
    const length = openTabs.length;
    if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault();
      const position = index + (event.key === 'ArrowLeft' ? -1 : 1);
      const id = openTabs[index].id;
      if (position >= 0 && position < length) void mutate('reorder_tab', { id, position }).then(moved => {
        if (moved) document.getElementById(`tab-${id}`)?.focus();
      });
      return;
    }
    const target = event.key === 'ArrowRight' ? (index + 1) % length : event.key === 'ArrowLeft' ? (index + length - 1) % length : event.key === 'Home' ? 0 : event.key === 'End' ? length - 1 : null;
    if (target === null) return;
    event.preventDefault();
    const id = openTabs[target].id;
    void mutate('select_tab', { id }).then(selected => { if (selected) document.getElementById(`tab-${id}`)?.focus(); });
  }
  return <section className="workspace-pane" hidden={!visible} aria-label={workspace.name}>
    <header className="workspace-heading"><div><h1>{workspace.name}</h1><p title={workspace.baseDirectory}>{workspace.baseDirectory}</p></div></header>
    <div className="tab-bar">
      <div role="tablist" aria-label={`${workspace.name} terminals`} className="tab-list" onDragLeave={event => {
        if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) setTabDrop(null);
      }}>
        {openTabs.map((tab, index) => <div key={tab.id} role="presentation"
          className={`tab-item${tab.id === active?.id ? ' selected' : ''}${dragTabId === tab.id ? ' dragging' : ''}${tabDrop?.id === tab.id ? tabDrop.after ? ' drop-after' : ' drop-before' : ''}`}
          onDragOver={event => {
            if (disabled || dragTabId === null) return;
            event.preventDefault(); event.dataTransfer.dropEffect = 'move';
            const rect = event.currentTarget.getBoundingClientRect();
            const after = event.clientX >= rect.left + rect.width / 2;
            setTabDrop(previous => previous?.id === tab.id && previous.after === after ? previous : { id: tab.id, after });
          }} onDrop={event => {
            if (disabled || dragTabId === null) return;
            event.preventDefault();
            const rect = event.currentTarget.getBoundingClientRect();
            const gap = index + (event.clientX >= rect.left + rect.width / 2 ? 1 : 0);
            const from = openTabs.findIndex(item => item.id === dragTabId);
            const position = gap - (from < gap ? 1 : 0);
            if (from >= 0 && from !== position) void mutate('reorder_tab', { id: dragTabId, position });
            setDragTabId(null); setTabDrop(null);
          }}>
          <button id={`tab-${tab.id}`} role="tab" aria-selected={tab.id === active?.id} aria-controls={`tab-panel-${tab.id}`} tabIndex={tab.id === active?.id ? 0 : -1}
            disabled={disabled} draggable={!disabled} onKeyDown={event => navigate(event, index)} onClick={() => void mutate('select_tab', { id: tab.id })}
            title={`${tab.name} — ${statuses[tab.id]?.state ?? 'stopped'} · Drag to reorder · Alt+← / Alt+→`}
            onDragStart={event => {
              setDragTabId(tab.id); event.dataTransfer.effectAllowed = 'move';
              event.dataTransfer.setData('text/plain', String(tab.id));
              event.dataTransfer.setDragImage(event.currentTarget.parentElement!, 25, 16);
            }} onDragEnd={() => { setDragTabId(null); setTabDrop(null); }}>
            <span className={`status-dot ${statuses[tab.id]?.state ?? 'stopped'}`} /><span className="tab-name">{tab.name}</span>
            {statuses[tab.id]?.state === 'exited' && <span>({(statuses[tab.id] as { code: number }).code})</span>}
          </button>
          <button className="tab-icon tab-close" disabled={disabled} tabIndex={tab.id === active?.id ? 0 : -1} aria-label={`Close ${tab.name}`} title="Close tab" onClick={() => closeTab(tab)}>×</button>
        </div>)}
      </div>
      {availableTabs.length > 0 && <select aria-label="Open a configured tab" value="" disabled={disabled} onChange={event => {
        const id = Number(event.target.value); if (id) void mutate('open_tab', { id });
      }}><option value="" disabled>Open configured tab…</option>{availableTabs.map(tab => <option key={tab.id} value={tab.id}>{tab.name}</option>)}</select>}
      <button disabled={disabled} aria-label="Create terminal tab" onClick={() => void mutate('create_tab', { workspaceId: workspace.id })}>+ New tab</button>
    </div>
    {!active && <div className="empty-state"><p>No open tabs. Choose a configured tab above, or create a new definition.</p><button disabled={disabled} onClick={() => void mutate('create_tab', { workspaceId: workspace.id })}>Create terminal tab</button></div>}
    {openTabs.map(tab => <TabTerminal key={tab.id} workspace={workspace} tab={tab} visible={visible && tab.id === active?.id} shell={shell} disabled={disabled} markBusy={markBusy} reportStatus={reportStatus} onState={onState} markRestoring={markRestoring} />)}
  </section>;
}

function App() {
  const [state, setState] = useState<WorkspaceSnapshot>({ workspaces: [], selectedId: null });
  const [defaults, setDefaults] = useState<[string, string]>(['', '']);
  const [statuses, setStatuses] = useState<Record<number, ProcessStatus | null>>({});
  const [editor, setEditor] = useState<Workspace | 'new' | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [ready, setReady] = useState(false);
  const busyRef = useRef(false);
  const [restoring, setRestoring] = useState(0);
  const restoringRef = useRef(0);
  const markRestoring = useCallback((delta: number) => {
    restoringRef.current = Math.max(0, restoringRef.current + delta);
    setRestoring(restoringRef.current);
  }, []);
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
      if (busyRef.current || restoringRef.current > 0) { report('Please wait for the current operation to finish before quitting.'); return; }
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

  const disabled = busy || restoring > 0;
  return <main className="app">
    <WorkspaceSidebar workspaces={state.workspaces} selectedId={state.selectedId} statuses={statuses} disabled={disabled} ready={ready}
      create={() => { setError(''); setEditor('new'); }} edit={workspace => { setError(''); setEditor(workspace); }}
      select={id => void mutate('select_workspace', { id })} reorder={(id, position) => mutate('reorder_workspace', { id, position })}
      remove={workspace => {
        const hasSessions = workspace.tabs.some(tab => !!statuses[tab.id]);
        if (!window.confirm(`Delete workspace “${workspace.name}” and its saved tab definitions?${hasSessions ? ' All its terminal sessions and jobs will be terminated.' : ''} The directory and its contents will remain untouched.`)) return;
        void mutate('close_workspace', { id: workspace.id, confirmed: true }).then(closed => {
          if (closed) setStatuses(previous => { const next = { ...previous }; for (const tab of workspace.tabs) delete next[tab.id]; return next; });
        });
      }} />
    <div className="main-pane">
      {error && <div role="alert" className="error">{error}</div>}
      {!state.workspaces.length && <div className="empty-state"><h1>Your terminal workspace</h1><p>Create a workspace pointing to an existing directory.</p><button disabled={!ready || disabled} onClick={() => setEditor('new')}>Create workspace</button></div>}
      {state.workspaces.map(workspace => <WorkspacePane key={workspace.id} workspace={workspace} visible={workspace.id === state.selectedId} statuses={statuses} shell={defaults[1]} disabled={disabled} markBusy={markBusy} reportStatus={reportStatus} mutate={mutate} onState={setState} markRestoring={markRestoring}
        closeTab={tab => {
          const confirmed = !!statuses[tab.id];
          if (confirmed && !window.confirm(`Close “${tab.name}” and terminate its session and jobs? Its configuration will stay available.`)) return;
          void mutate('close_tab', { id: tab.id, confirmed }).then(closed => {
            if (closed) setStatuses(previous => { const next = { ...previous }; delete next[tab.id]; return next; });
          });
        }} />)}
      <footer>Ctrl+Shift+C / V: copy / paste · Arrow keys: navigate terminal tabs</footer>
    </div>
    {editor && <WorkspaceEditor key={editor === 'new' ? 'new' : editor.id} workspace={editor === 'new' ? undefined : editor} initialDirectory={defaults[0]} dismiss={() => setEditor(null)} externalError={error}
      save={(name, directory) => mutate(editor === 'new' ? 'create_workspace' : 'update_workspace', { ...(editor === 'new' ? {} : { id: editor.id }), name, directory })} />}
  </main>;
}

createRoot(document.getElementById('root')!).render(<App />);
