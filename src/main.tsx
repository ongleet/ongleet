import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import type { ProcessStatus } from './terminal';
import { allowEditorClose, fileEditors } from './plugins/files/editor-guards';
import { TabHost, flushPlugins, plugins } from './plugins';
import type { PluginTab } from './plugins/types';
import { SessionSidebar } from './SessionSidebar';
import { ArchivesDialog } from './ArchivesDialog';
import { ProjectEditor } from './ProjectEditor';
import './style.css';

export type Project = {
  id: number;
  name: string;
  rootDirectory: string;
  displayOrder: number;
  collapsed: boolean;
};
export type Session = {
  pullRequest: { repository: string; number: number } | null;
  projectId: number;
  id: number;
  name: string;
  baseDirectory: string;
  displayOrder: number;
  tabs: PluginTab[];
  selectedTabId: number | null;
};
type SessionSnapshot = { projects: Project[]; sessions: Session[]; selectedId: number | null };
type Report = (error: unknown) => void;

function ConfirmationDialog({
  message,
  choose,
}: {
  message: string;
  choose: (confirmed: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      aria-label="Confirm action"
      onCancel={(event) => {
        event.preventDefault();
        choose(false);
      }}
    >
      <div className="confirmation-content">
        <h2>Confirm action</h2>
        <p>{message}</p>
        <div className="dialog-actions">
          <button autoFocus onClick={() => choose(false)}>
            Cancel
          </button>
          <button onClick={() => choose(true)}>Confirm</button>
        </div>
      </div>
    </dialog>
  );
}

function SessionEditor({
  session,
  projects,
  initialProjectId,
  initialDirectory,
  save,
  dismiss,
  externalError,
}: {
  projects: Project[];
  initialProjectId: number;
  session?: Session;
  initialDirectory: string;
  save: (name: string, directory: string, projectId: number) => Promise<boolean>;
  dismiss: () => void;
  externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(session?.name ?? '');
  const [projectId, setProjectId] = useState(session?.projectId ?? initialProjectId);
  const [directory, setDirectory] = useState(session?.baseDirectory ?? initialDirectory);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  async function browse() {
    setBusy(true);
    setError('');
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: 'Choose session directory',
      });
      if (typeof path === 'string') setDirectory(path);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby="session-editor-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          void save(name, directory, projectId)
            .then((saved) => {
              if (saved) dismiss();
            })
            .finally(() => setBusy(false));
        }}
      >
        <h2 id="session-editor-title">{session ? 'Edit session' : 'New session'}</h2>
        <label>
          Project
          <select
            value={projectId}
            disabled={busy}
            onChange={(event) => {
              const id = Number(event.target.value);
              setProjectId(id);
              if (!session) setDirectory(projects.find((group) => group.id === id)!.rootDirectory);
            }}
          >
            {projects.map((group) => (
              <option key={group.id} value={group.id}>
                {group.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Name
          <input
            id="session-name"
            autoFocus
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            disabled={busy}
          />
        </label>
        <label>
          Base directory
          <input
            id="session-directory"
            required
            value={directory}
            onChange={(event) => setDirectory(event.target.value)}
            disabled={busy}
          />
        </label>
        <button type="button" onClick={() => void browse()} disabled={busy}>
          Choose directory…
        </button>
        {session && (
          <p className="muted">
            Directory changes apply to future launches. Running processes keep their working
            directory.
          </p>
        )}
        {(error || externalError) && (
          <p role="alert" className="error">
            {error || externalError}
          </p>
        )}
        <div className="dialog-actions">
          <button type="button" onClick={dismiss} disabled={busy}>
            Cancel
          </button>
          <button disabled={busy || !name.trim() || !directory}>
            {busy ? 'Please wait…' : 'Save session'}
          </button>
        </div>
      </form>
    </dialog>
  );
}

function NewTabPicker({
  sessionId,
  disabled,
  create,
  external = false,
}: {
  sessionId: number;
  disabled: boolean;
  create: (actionId: string) => Promise<boolean>;
  external?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [types, setTypes] = useState<{ id: string; name: string }[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  async function open() {
    setError('');
    setTypes([]);
    setLoading(true);
    dialog.current!.showModal();
    try {
      setTypes(
        (
          await invoke<{ id: string; name: string; kind: string; configVersion: number }[]>(
            'actions',
            { sessionId },
          )
        ).filter((action) =>
          external
            ? action.kind === 'command'
            : plugins.some(
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
      <button disabled={disabled} onClick={() => void open()}>
        {external ? 'Actions' : '+ New tab'}
      </button>
      <dialog ref={dialog} aria-label={external ? 'External actions' : 'New tab'}>
        <h2>{external ? 'External actions' : 'New tab'}</h2>
        {external && (
          <p className="muted">
            Launch independently without opening a tab. Closing a session or quitting this app does
            not stop external applications.
          </p>
        )}
        {!loading && !error && types.length === 0 && (
          <p>No actions configured. Add one in Edit project → Actions.</p>
        )}
        {loading && <p role="status">Loading actions…</p>}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="tab-type-options">
          {types.map((type) => (
            <button
              key={type.id}
              disabled={disabled}
              onClick={() => {
                dialog.current!.close();
                void create(type.id);
              }}
            >
              {type.name}
            </button>
          ))}
        </div>
        <div className="dialog-actions">
          <button onClick={() => dialog.current!.close()}>Cancel</button>
        </div>
      </dialog>
    </>
  );
}

function SessionPane({
  session,
  visible,
  statuses,
  disabled,
  reportStatus,
  mutate,
  closeTab,
  markRestoring,
}: {
  session: Session;
  visible: boolean;
  statuses: Record<number, ProcessStatus | null>;
  disabled: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>;
  closeTab: (tab: PluginTab) => void;
  markRestoring: (delta: number) => void;
}) {
  const openTabs = session.tabs.filter((tab) => tab.isOpen);
  const active = openTabs.find((tab) => tab.id === session.selectedTabId);
  const [tabTitles, setTabTitles] = useState<Record<number, string>>({});
  const reportTitle = useCallback((id: number, title: string | null) => {
    setTabTitles((previous) => {
      if ((previous[id] ?? null) === title) return previous;
      const next = { ...previous };
      if (title === null) delete next[id];
      else next[id] = title;
      return next;
    });
  }, []);
  const [dragTabId, setDragTabId] = useState<number | null>(null);
  const [tabDrop, setTabDrop] = useState<{ id: number; after: boolean } | null>(null);
  function navigate(event: React.KeyboardEvent, index: number) {
    const length = openTabs.length;
    if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault();
      const position = index + (event.key === 'ArrowLeft' ? -1 : 1);
      const id = openTabs[index].id;
      if (position >= 0 && position < length)
        void mutate('reorder_tab', { id, position }).then((moved) => {
          if (moved) document.getElementById(`tab-${id}`)?.focus();
        });
      return;
    }
    const target =
      event.key === 'ArrowRight'
        ? (index + 1) % length
        : event.key === 'ArrowLeft'
          ? (index + length - 1) % length
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? length - 1
              : null;
    if (target === null) return;
    event.preventDefault();
    const id = openTabs[target].id;
    void mutate('select_tab', { id });
  }
  return (
    <section className="session-pane" hidden={!visible} aria-label={session.name}>
      <div className="tab-bar">
        <div
          role="tablist"
          aria-label={`${session.name} tabs`}
          className="tab-list"
          onDragLeave={(event) => {
            if (
              !(event.relatedTarget instanceof Node) ||
              !event.currentTarget.contains(event.relatedTarget)
            )
              setTabDrop(null);
          }}
        >
          {openTabs.map((tab, index) => (
            <div
              key={tab.id}
              role="presentation"
              className={`tab-item${tab.id === active?.id ? ' selected' : ''}${dragTabId === tab.id ? ' dragging' : ''}${tabDrop?.id === tab.id ? (tabDrop.after ? ' drop-after' : ' drop-before') : ''}`}
              onDragOver={(event) => {
                if (disabled || dragTabId === null) return;
                event.preventDefault();
                event.dataTransfer.dropEffect = 'move';
                const rect = event.currentTarget.getBoundingClientRect();
                const after = event.clientX >= rect.left + rect.width / 2;
                setTabDrop((previous) =>
                  previous?.id === tab.id && previous.after === after
                    ? previous
                    : { id: tab.id, after },
                );
              }}
              onDrop={(event) => {
                if (disabled || dragTabId === null) return;
                event.preventDefault();
                const rect = event.currentTarget.getBoundingClientRect();
                const gap = index + (event.clientX >= rect.left + rect.width / 2 ? 1 : 0);
                const from = openTabs.findIndex((item) => item.id === dragTabId);
                const position = gap - (from < gap ? 1 : 0);
                if (from >= 0 && from !== position)
                  void mutate('reorder_tab', { id: dragTabId, position });
                setDragTabId(null);
                setTabDrop(null);
              }}
            >
              <button
                id={`tab-${tab.id}`}
                role="tab"
                aria-selected={tab.id === active?.id}
                aria-controls={`tab-panel-${tab.id}`}
                tabIndex={tab.id === active?.id ? 0 : -1}
                disabled={disabled}
                draggable={!disabled}
                onKeyDown={(event) => navigate(event, index)}
                onClick={() => void mutate('select_tab', { id: tab.id })}
                title={`${tabTitles[tab.id] ?? tab.name} — ${tab.kind === 'terminal' ? (statuses[tab.id]?.state ?? 'stopped') : tab.kind} · Drag to reorder · Alt+← / Alt+→`}
                onDragStart={(event) => {
                  setDragTabId(tab.id);
                  event.dataTransfer.effectAllowed = 'move';
                  event.dataTransfer.setData('text/plain', String(tab.id));
                  event.dataTransfer.setDragImage(event.currentTarget.parentElement!, 25, 16);
                }}
                onDragEnd={() => {
                  setDragTabId(null);
                  setTabDrop(null);
                }}
              >
                <span className="tab-name">{tabTitles[tab.id] ?? tab.name}</span>
                {statuses[tab.id]?.state === 'exited' && (
                  <span>({(statuses[tab.id] as { code: number }).code})</span>
                )}
              </button>
              <button
                className="tab-icon tab-close"
                disabled={disabled}
                tabIndex={tab.id === active?.id ? 0 : -1}
                aria-label={`Close ${tabTitles[tab.id] ?? tab.name}`}
                title="Close and archive tab for 30 days"
                onClick={() => closeTab(tab)}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        <NewTabPicker
          sessionId={session.id}
          disabled={disabled}
          create={(actionId) => mutate('create_tab', { sessionId: session.id, actionId })}
        />
        <NewTabPicker
          external
          sessionId={session.id}
          disabled={disabled}
          create={(actionId) => mutate('run_external_action', { sessionId: session.id, actionId })}
        />
      </div>
      {!active && (
        <div className="empty-state">
          <p>No open tabs. Choose New tab to launch a configured type.</p>
        </div>
      )}
      {openTabs.map((tab) => (
        <TabHost
          reportTitle={reportTitle}
          key={tab.id}
          session={session}
          tab={tab}
          visible={visible && tab.id === active?.id}
          reportStatus={reportStatus}
          markRestoring={markRestoring}
        />
      ))}
    </section>
  );
}

function App() {
  const [showArchives, setShowArchives] = useState(false);
  const [confirmation, setConfirmation] = useState<{
    message: string;
    resolve: (value: boolean) => void;
  } | null>(null);
  function confirm(message: string): Promise<boolean> {
    return new Promise((resolve) => setConfirmation({ message, resolve }));
  }

  const [state, setState] = useState<SessionSnapshot>({
    projects: [],
    sessions: [],
    selectedId: null,
  });
  const [defaults, setDefaults] = useState<[string, string]>(['', '']);
  const [statuses, setStatuses] = useState<Record<number, ProcessStatus | null>>({});
  const [editor, setEditor] = useState<Session | 'new' | null>(null);
  const [newProjectId, setNewProjectId] = useState<number | null>(null);
  const [groupEditor, setGroupEditor] = useState<Project | 'new' | null>(null);
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
  const report: Report = (error) => setError(String(error));
  const markBusy = useCallback((busy: boolean) => {
    busyRef.current = busy;
    setBusy(busy);
  }, []);
  const reportStatus = useCallback((id: number, status: ProcessStatus | null) => {
    setStatuses((previous) => {
      const old = previous[id];
      if (
        old?.state === status?.state &&
        (old?.state !== 'exited' || status?.state !== 'exited' || old.code === status.code)
      )
        return previous;
      return { ...previous, [id]: status };
    });
  }, []);

  const refreshSequence = useRef(0);
  const refresh = useCallback(async () => {
    const sequence = ++refreshSequence.current;
    const snapshot = await invoke<SessionSnapshot>('sessions');
    if (sequence === refreshSequence.current) {
      setState(snapshot);
      const openIds = new Set(
        snapshot.sessions.flatMap((session) =>
          session.tabs.filter((tab) => tab.isOpen).map((tab) => tab.id),
        ),
      );
      setStatuses((previous) => {
        if (Object.keys(previous).every((id) => openIds.has(Number(id)))) return previous;
        return Object.fromEntries(
          Object.entries(previous).filter(([id]) => openIds.has(Number(id))),
        );
      });
    }
  }, []);

  useEffect(() => {
    if (!isTauri()) {
      report('Native backend unavailable. Start with mise exec -- npm run tauri -- dev.');
      return;
    }
    let disposed = false;
    let pending: ReturnType<typeof setTimeout> | undefined;
    const unlistenExternal = listen<{ name: string; code: number | null; error: string | null }>(
      'external-action-exited',
      (event) => {
        if (event.payload.code !== 0)
          report(
            `${event.payload.name}: ${event.payload.error ?? `exited with ${event.payload.code ?? 'a signal'}`}. External command output is not captured.`,
          );
      },
    );
    const unlisten = listen('session-changed', () => {
      clearTimeout(pending);
      pending = setTimeout(() => {
        if (!disposed) void refresh().catch(report);
      }, 25);
    });
    void unlisten
      .then(async () => {
        if (disposed) return;
        const [, defaults] = await Promise.all([refresh(), invoke<[string, string]>('defaults')]);
        if (!disposed) {
          setDefaults(defaults);
          setReady(true);
        }
      })
      .catch(report);
    return () => {
      disposed = true;
      clearTimeout(pending);
      ++refreshSequence.current;
      void unlisten.then((off) => off()).catch(report);
      void unlistenExternal.then((off) => off()).catch(report);
    };
  }, [refresh]);

  async function quit() {
    if (busyRef.current || restoringRef.current > 0) {
      report('Please wait for the current operation to finish before quitting.');
      return;
    }
    markBusy(true);
    setError('');
    try {
      if (!(await allowEditorClose([...fileEditors.keys()], confirm))) return;
      const status = await invoke<{ runningTerminals: number }>('application_status');
      const confirmed = status.runningTerminals > 0;
      if (
        confirmed &&
        !(await confirm(
          `Quit and force-stop ${status.runningTerminals} running terminal session(s) and their child processes?`,
        ))
      )
        return;
      await flushPlugins();
      await invoke('quit', { confirmed });
    } catch (error) {
      report(error);
    } finally {
      markBusy(false);
    }
  }

  async function mutate(command: string, args: Record<string, unknown>): Promise<boolean> {
    markBusy(true);
    setError('');
    try {
      await invoke<unknown>(command, args);
      await refresh();
      return true;
    } catch (error) {
      report(error);
      return false;
    } finally {
      markBusy(false);
    }
  }

  // A pending terminal attachment must not lock navigation or sibling tabs.
  // Quit still waits for attachments so it cannot race a new process launch.
  const disabled = busy;
  return (
    <main className="app">
      <SessionSidebar
        projects={state.projects}
        sessions={state.sessions}
        selectedId={state.selectedId}
        disabled={disabled}
        ready={ready}
        create={(groupId) => {
          setError('');
          setNewProjectId(groupId);
          setEditor('new');
        }}
        createProject={() => {
          setError('');
          setGroupEditor('new');
        }}
        editProject={(group) => {
          setError('');
          setGroupEditor(group);
        }}
        foldProject={(id, collapsed) => void mutate('fold_project', { id, collapsed })}
        reorderProject={(id, position) => mutate('reorder_project', { id, position })}
        removeProject={async (group) => {
          if (
            await confirm(
              `Delete empty project “${group.name}”? Its directory and configuration file will remain untouched.`,
            )
          )
            void mutate('close_project', { id: group.id });
        }}
        edit={(session) => {
          setError('');
          setEditor(session);
        }}
        select={(id) => void mutate('select_session', { id })}
        reorder={(id, position) => mutate('reorder_session', { id, position })}
        remove={async (session) => {
          if (
            !(await allowEditorClose(
              session.tabs.map((tab) => tab.id),
              confirm,
            ))
          )
            return;
          const hasSessions = session.tabs.some((tab) => statuses[tab.id]?.state === 'running');
          if (
            !(await confirm(
              `Archive session “${session.name}” and its saved tab definitions for 30 days?${hasSessions ? ' All its terminal sessions and jobs will be terminated.' : ''} The directory and its contents will remain untouched.`,
            ))
          )
            return;
          void mutate('close_session', { id: session.id, confirmed: true }).then((closed) => {
            if (closed)
              setStatuses((previous) => {
                const next = { ...previous };
                for (const tab of session.tabs) delete next[tab.id];
                return next;
              });
          });
        }}
      />
      <div className="main-pane">
        {error && (
          <div role="alert" className="error">
            {error}
          </div>
        )}
        {!state.sessions.length && (
          <div className="empty-state">
            <h1>Welcome to DevCenter</h1>
            <p>Create a session pointing to an existing directory.</p>
            <button
              disabled={!ready || disabled}
              onClick={() => {
                if (state.projects.length) {
                  setNewProjectId(state.projects[0].id);
                  setEditor('new');
                } else setGroupEditor('new');
              }}
            >
              {state.projects.length ? 'Create session' : 'Create project'}
            </button>
          </div>
        )}
        {state.sessions.map((session) => (
          <SessionPane
            key={session.id}
            session={session}
            visible={session.id === state.selectedId}
            statuses={statuses}
            disabled={disabled}
            reportStatus={reportStatus}
            mutate={mutate}
            markRestoring={markRestoring}
            closeTab={async (tab) => {
              if (!(await allowEditorClose([tab.id], confirm))) return;
              const confirmed = statuses[tab.id]?.state === 'running';
              if (
                confirmed &&
                !(await confirm(
                  `Archive “${tab.name}” for 30 days and terminate its session and jobs?`,
                ))
              )
                return;
              void mutate('archive_tab', { id: tab.id, confirmed }).then((closed) => {
                if (closed)
                  setStatuses((previous) => {
                    const next = { ...previous };
                    delete next[tab.id];
                    return next;
                  });
              });
            }}
          />
        ))}
        <footer>
          <span>Ctrl+Shift+C / V: copy / paste · Arrow keys: navigate terminal tabs</span>
          <button disabled={!ready || disabled} onClick={() => setShowArchives(true)}>
            Archives
          </button>
          <button disabled={!ready || disabled || restoring > 0} onClick={() => void quit()}>
            Quit
          </button>
        </footer>
      </div>
      {showArchives && (
        <ArchivesDialog
          dismiss={() => setShowArchives(false)}
          changed={refresh}
          confirm={confirm}
        />
      )}
      {confirmation && (
        <ConfirmationDialog
          message={confirmation.message}
          choose={(value) => {
            confirmation.resolve(value);
            setConfirmation(null);
          }}
        />
      )}
      {groupEditor && (
        <ProjectEditor
          key={groupEditor === 'new' ? 'new-group' : groupEditor.id}
          project={groupEditor === 'new' ? undefined : groupEditor}
          initialDirectory={defaults[0]}
          externalError={error}
          dismiss={() => setGroupEditor(null)}
          save={(name, rootDirectory) =>
            mutate(groupEditor === 'new' ? 'create_project' : 'update_project', {
              ...(groupEditor === 'new' ? {} : { id: groupEditor.id }),
              name,
              rootDirectory,
            })
          }
        />
      )}
      {editor && (
        <SessionEditor
          projects={state.projects}
          initialProjectId={newProjectId ?? state.projects[0]?.id}
          key={editor === 'new' ? 'new' : editor.id}
          session={editor === 'new' ? undefined : editor}
          initialDirectory={
            state.projects.find((group) => group.id === newProjectId)?.rootDirectory ?? defaults[0]
          }
          dismiss={() => setEditor(null)}
          externalError={error}
          save={(name, directory, projectId) =>
            mutate(editor === 'new' ? 'create_session' : 'update_session', {
              ...(editor === 'new' ? {} : { id: editor.id }),
              name,
              directory,
              projectId,
            })
          }
        />
      )}
    </main>
  );
}

createRoot(document.getElementById('root')!).render(<App />);
