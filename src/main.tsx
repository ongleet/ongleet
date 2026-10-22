import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { TerminalFrontend, type ProcessStatus } from './terminal';
import { WorkspaceSidebar } from './WorkspaceSidebar';
import { WorkgroupEditor } from './WorkgroupEditor';
import './style.css';

type TabConfig = { cwdOverride: string | null; executable: string | null; args: string[]; env: Record<string, string>; shellCommand: string | null };
type TerminalTab = { id: number; kind: string; isOpen: boolean; resumeOnStart: boolean; name: string; displayOrder: number; config: TabConfig };
export type Workgroup = { id: number; name: string; rootDirectory: string; displayOrder: number; collapsed: boolean };
export type Workspace = { workgroupId: number; id: number; name: string; baseDirectory: string; displayOrder: number; tabs: TerminalTab[]; selectedTabId: number | null };
type WorkspaceSnapshot = { workgroups: Workgroup[]; workspaces: Workspace[]; selectedId: number | null };
type Report = (error: unknown) => void;

function ConfirmationDialog({ message, choose }: { message: string; choose: (confirmed: boolean) => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { dialog.current?.showModal(); }, []);
  return <dialog ref={dialog} aria-label="Confirm action" onCancel={event => { event.preventDefault(); choose(false); }}>
    <div className="confirmation-content">
      <h2>Confirm action</h2><p>{message}</p>
      <div className="dialog-actions"><button autoFocus onClick={() => choose(false)}>Cancel</button><button onClick={() => choose(true)}>Confirm</button></div>
    </div>
  </dialog>;
}

function WorkspaceEditor({ workspace, workgroups, initialWorkgroupId, initialDirectory, save, dismiss, externalError }: {
  workgroups: Workgroup[]; initialWorkgroupId: number;
  workspace?: Workspace; initialDirectory: string;
  save: (name: string, directory: string, workgroupId: number) => Promise<boolean>; dismiss: () => void; externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(workspace?.name ?? '');
  const [workgroupId, setWorkgroupId] = useState(workspace?.workgroupId ?? initialWorkgroupId);
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
      void save(name, directory, workgroupId).then(saved => { if (saved) dismiss(); }).finally(() => setBusy(false));
    }}>
      <h2 id="workspace-editor-title">{workspace ? 'Edit workspace' : 'New workspace'}</h2>
      <label>Workgroup<select value={workgroupId} disabled={busy} onChange={event => {
        const id = Number(event.target.value); setWorkgroupId(id);
        if (!workspace) setDirectory(workgroups.find(group => group.id === id)!.rootDirectory);
      }}>{workgroups.map(group => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
      <label>Name<input id="workspace-name" autoFocus required value={name} onChange={event => setName(event.target.value)} disabled={busy} /></label>
      <label>Base directory<input id="workspace-directory" required value={directory} onChange={event => setDirectory(event.target.value)} disabled={busy} /></label>
      <button type="button" onClick={() => void browse()} disabled={busy}>Choose directory…</button>
      {workspace && <p className="muted">Directory changes apply to future launches. Running processes keep their working directory.</p>}
      {(error || externalError) && <p role="alert" className="error">{error || externalError}</p>}
      <div className="dialog-actions"><button type="button" onClick={dismiss} disabled={busy}>Cancel</button><button disabled={busy || !name.trim() || !directory}>{busy ? 'Please wait…' : 'Save workspace'}</button></div>
    </form>
  </dialog>;
}

function TabTerminal({ tab, visible, reportStatus, markRestoring }: {
  tab: TerminalTab; visible: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  markRestoring: (delta: number) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const frontend = useRef<TerminalFrontend | null>(null);
  const [error, setError] = useState('');
  const report: Report = error => setError(String(error));

  useEffect(() => {
    const terminal = new TerminalFrontend(host.current!, tab.id, status => {
      reportStatus(tab.id, status);
    }, report);
    frontend.current = terminal;
    // Reconnect live/exited sessions after a WebView reload. Only eligible
    // stopped configurations may launch a new process.
    markRestoring(1);
    void terminal.start(tab.resumeOnStart).catch(report).finally(() => markRestoring(-1));
    return () => { terminal.dispose(); frontend.current = null; };
  }, [tab.id, reportStatus, markRestoring]);
  useEffect(() => { frontend.current?.setVisible(visible); }, [visible]);

  return <section id={`tab-panel-${tab.id}`} role="tabpanel" aria-labelledby={`tab-${tab.id}`} className="tab-terminal" hidden={!visible}>
    {error && <div role="alert" className="error">{error}</div>}
    <div ref={host} className="terminal" />
  </section>;
}

function NewTabPicker({ workspaceId, disabled, create, external = false }: { workspaceId: number; disabled: boolean; create: (actionId: string) => Promise<boolean>; external?: boolean }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [types, setTypes] = useState<{ id: string; name: string }[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  async function open() {
    setError(''); setTypes([]); setLoading(true); dialog.current!.showModal();
    try { setTypes((await invoke<{ id: string; name: string; kind: string }[]>('actions', { workspaceId })).filter(action => action.kind === (external ? 'command' : 'terminal'))); }
    catch (error) { setError(String(error)); }
    finally { setLoading(false); }
  }
  return <>
    <button disabled={disabled} onClick={() => void open()}>{external ? 'Actions' : '+ New tab'}</button>
    <dialog ref={dialog} aria-label={external ? "External actions" : "New tab"}>
      <h2>{external ? "External actions" : "New tab"}</h2>
      {external && <p className="muted">Launch independently without opening a tab. Closing a workspace or quitting this app does not stop external applications.</p>}
      {!loading && !error && types.length === 0 && <p>No actions configured. Add one in Edit workgroup → Actions.</p>}
      {loading && <p role="status">Loading actions…</p>}
      {error && <p role="alert" className="error">{error}</p>}
      <div className="tab-type-options">{types.map(type => <button key={type.id} disabled={disabled} onClick={() => {
        dialog.current!.close(); void create(type.id);
      }}>{type.name}</button>)}</div>
      <div className="dialog-actions"><button onClick={() => dialog.current!.close()}>Cancel</button></div>
    </dialog>
  </>;
}

function WorkspacePane({ workspace, visible, statuses, disabled, reportStatus, mutate, closeTab, markRestoring }: {
  workspace: Workspace; visible: boolean; statuses: Record<number, ProcessStatus | null>; disabled: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>;
  closeTab: (tab: TerminalTab) => void; markRestoring: (delta: number) => void;
}) {
  const openTabs = workspace.tabs.filter(tab => tab.isOpen);
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
    void mutate('select_tab', { id });
  }
  return <section className="workspace-pane" hidden={!visible} aria-label={workspace.name}>
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
            <span className="tab-name">{tab.name}</span>
            {statuses[tab.id]?.state === 'exited' && <span>({(statuses[tab.id] as { code: number }).code})</span>}
          </button>
          <button className="tab-icon tab-close" disabled={disabled} tabIndex={tab.id === active?.id ? 0 : -1} aria-label={`Close ${tab.name}`} title="Close tab" onClick={() => closeTab(tab)}>×</button>
        </div>)}
      </div>
      <NewTabPicker workspaceId={workspace.id} disabled={disabled} create={actionId => mutate('create_tab', { workspaceId: workspace.id, actionId })} />
      <NewTabPicker external workspaceId={workspace.id} disabled={disabled} create={actionId => mutate('run_external_action', { workspaceId: workspace.id, actionId })} />
    </div>
    {!active && <div className="empty-state"><p>No open tabs. Choose New tab to launch a configured type.</p></div>}
    {openTabs.map(tab => <TabTerminal key={tab.id} tab={tab} visible={visible && tab.id === active?.id} reportStatus={reportStatus} markRestoring={markRestoring} />)}
  </section>;
}

function App() {
  const [confirmation, setConfirmation] = useState<{ message: string; resolve: (value: boolean) => void } | null>(null);
  function confirm(message: string): Promise<boolean> {
    return new Promise(resolve => setConfirmation({ message, resolve }));
  }

  const [state, setState] = useState<WorkspaceSnapshot>({ workgroups: [], workspaces: [], selectedId: null });
  const [defaults, setDefaults] = useState<[string, string]>(['', '']);
  const [statuses, setStatuses] = useState<Record<number, ProcessStatus | null>>({});
  const [editor, setEditor] = useState<Workspace | 'new' | null>(null);
  const [newWorkgroupId, setNewWorkgroupId] = useState<number | null>(null);
  const [groupEditor, setGroupEditor] = useState<Workgroup | 'new' | null>(null);
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

  const refreshSequence = useRef(0);
  const refresh = useCallback(async () => {
    const sequence = ++refreshSequence.current;
    const snapshot = await invoke<WorkspaceSnapshot>('workspaces');
    if (sequence === refreshSequence.current) {
      setState(snapshot);
      const openIds = new Set(snapshot.workspaces.flatMap(workspace => workspace.tabs.filter(tab => tab.isOpen).map(tab => tab.id)));
      setStatuses(previous => {
        if (Object.keys(previous).every(id => openIds.has(Number(id)))) return previous;
        return Object.fromEntries(Object.entries(previous).filter(([id]) => openIds.has(Number(id))));
      });
    }
  }, []);

  useEffect(() => {
    if (!isTauri()) { report('Native backend unavailable. Start with mise exec -- npm run tauri -- dev.'); return; }
    let disposed = false;
    let pending: ReturnType<typeof setTimeout> | undefined;
    const unlistenExternal = listen<{ name: string; code: number | null; error: string | null }>('external-action-exited', event => {
      if (event.payload.code !== 0) report(`${event.payload.name}: ${event.payload.error ?? `exited with ${event.payload.code ?? 'a signal'}`}. External command output is not captured.`);
    });
    const unlisten = listen('workspace-changed', () => {
      clearTimeout(pending);
      pending = setTimeout(() => { if (!disposed) void refresh().catch(report); }, 25);
    });
    void unlisten.then(async () => {
      if (disposed) return;
      const [, defaults] = await Promise.all([refresh(), invoke<[string, string]>('defaults')]);
      if (!disposed) { setDefaults(defaults); setReady(true); }
    }).catch(report);
    return () => { disposed = true; clearTimeout(pending); ++refreshSequence.current; void unlisten.then(off => off()).catch(report); void unlistenExternal.then(off => off()).catch(report); };
  }, [refresh]);

  async function quit() {
    if (busyRef.current || restoringRef.current > 0) { report('Please wait for the current operation to finish before quitting.'); return; }
    markBusy(true); setError('');
    try {
      const status = await invoke<{ runningSessions: number }>('application_status');
      const confirmed = status.runningSessions > 0;
      if (confirmed && !await confirm(`Quit and force-stop ${status.runningSessions} running terminal session(s) and their child processes?`)) return;
      await invoke('quit', { confirmed });
    } catch (error) { report(error); }
    finally { markBusy(false); }
  }

  async function mutate(command: string, args: Record<string, unknown>): Promise<boolean> {
    markBusy(true); setError('');
    try { await invoke<unknown>(command, args); await refresh(); return true; }
    catch (error) { report(error); return false; }
    finally { markBusy(false); }
  }

  const disabled = busy || restoring > 0;
  return <main className="app">
    <WorkspaceSidebar workgroups={state.workgroups} workspaces={state.workspaces} selectedId={state.selectedId} disabled={disabled} ready={ready}
      create={groupId => { setError(''); setNewWorkgroupId(groupId); setEditor('new'); }}
      createWorkgroup={() => { setError(''); setGroupEditor('new'); }} editWorkgroup={group => { setError(''); setGroupEditor(group); }}
      foldWorkgroup={(id, collapsed) => void mutate('fold_workgroup', { id, collapsed })}
      removeWorkgroup={async group => { if (await confirm(`Delete empty workgroup “${group.name}”? Its directory and configuration file will remain untouched.`)) void mutate('close_workgroup', { id: group.id }); }} edit={workspace => { setError(''); setEditor(workspace); }}
      select={id => void mutate('select_workspace', { id })} reorder={(id, position) => mutate('reorder_workspace', { id, position })}
      remove={async workspace => {
        const hasSessions = workspace.tabs.some(tab => statuses[tab.id]?.state === 'running');
        if (!await confirm(`Delete workspace “${workspace.name}” and its saved tab definitions?${hasSessions ? ' All its terminal sessions and jobs will be terminated.' : ''} The directory and its contents will remain untouched.`)) return;
        void mutate('close_workspace', { id: workspace.id, confirmed: true }).then(closed => {
          if (closed) setStatuses(previous => { const next = { ...previous }; for (const tab of workspace.tabs) delete next[tab.id]; return next; });
        });
      }} />
    <div className="main-pane">
      {error && <div role="alert" className="error">{error}</div>}
      {!state.workspaces.length && <div className="empty-state"><h1>Your terminal workspace</h1><p>Create a workspace pointing to an existing directory.</p><button disabled={!ready || disabled} onClick={() => { if (state.workgroups.length) { setNewWorkgroupId(state.workgroups[0].id); setEditor('new'); } else setGroupEditor('new'); }}>{state.workgroups.length ? 'Create workspace' : 'Create workgroup'}</button></div>}
      {state.workspaces.map(workspace => <WorkspacePane key={workspace.id} workspace={workspace} visible={workspace.id === state.selectedId} statuses={statuses} disabled={disabled} reportStatus={reportStatus} mutate={mutate} markRestoring={markRestoring}
        closeTab={async tab => {
          const confirmed = statuses[tab.id]?.state === 'running';
          if (confirmed && !await confirm(`Close “${tab.name}” and terminate its session and jobs?`)) return;
          void mutate('close_tab', { id: tab.id, confirmed }).then(closed => {
            if (closed) setStatuses(previous => { const next = { ...previous }; delete next[tab.id]; return next; });
          });
        }} />)}
      <footer><span>Ctrl+Shift+C / V: copy / paste · Arrow keys: navigate terminal tabs</span><button disabled={!ready || disabled} onClick={() => void quit()}>Quit</button></footer>
    </div>
    {confirmation && <ConfirmationDialog message={confirmation.message} choose={value => { confirmation.resolve(value); setConfirmation(null); }} />}
    {groupEditor && <WorkgroupEditor key={groupEditor === 'new' ? 'new-group' : groupEditor.id} workgroup={groupEditor === 'new' ? undefined : groupEditor} initialDirectory={defaults[0]} externalError={error} dismiss={() => setGroupEditor(null)}
      save={(name, rootDirectory) => mutate(groupEditor === 'new' ? 'create_workgroup' : 'update_workgroup', { ...(groupEditor === 'new' ? {} : { id: groupEditor.id }), name, rootDirectory })} />}
    {editor && <WorkspaceEditor workgroups={state.workgroups} initialWorkgroupId={newWorkgroupId ?? state.workgroups[0]?.id} key={editor === 'new' ? 'new' : editor.id} workspace={editor === 'new' ? undefined : editor} initialDirectory={state.workgroups.find(group => group.id === newWorkgroupId)?.rootDirectory ?? defaults[0]} dismiss={() => setEditor(null)} externalError={error}
      save={(name, directory, workgroupId) => mutate(editor === 'new' ? 'create_workspace' : 'update_workspace', { ...(editor === 'new' ? {} : { id: editor.id }), name, directory, workgroupId })} />}
  </main>;
}

createRoot(document.getElementById('root')!).render(<App />);
