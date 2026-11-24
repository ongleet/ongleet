import { useOpeningHighlight } from './components/useOpeningHighlight';
import { loadTabTitles, openTabTitles, saveTabTitles } from './tab-titles';
import { useRenderDiagnostic } from './render-diagnostics';
import { t, useLocale } from './i18n';
import { Button, Dialog, Input, Select, Textarea, Tab, TabList } from './components/Controls';
import { reconcileCommentSessions } from './diff-comment-cleanup';
import { appearance, applyAppearance } from './appearance';
import { applyApplicationAppearance, type ApplicationAppearance } from './application-settings';
import './branding-storage';
import { useCallback, useEffect, useEffectEvent, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import type { ProcessStatus } from './terminal';
import { allowEditorClose, fileEditors } from './plugins/files/src/editor-guards';
import { TabHost, flushPlugins, plugins, PluginIcon } from './plugins';
import type { PluginTab } from './plugins/types';
import { SessionSidebar } from './SessionSidebar';
import { ArchivesDialog } from './ArchivesDialog';
import { ProjectEditor } from './ProjectEditor';
import { TabConfiguration } from './TabConfiguration';
import { SessionMenu } from './SessionMenu';
import { WindowTitleBar } from './WindowTitleBar';
import './style.css';
import './components/checkbox.css';

export type Project = {
  id: number;
  name: string;
  rootDirectory: string;
  displayOrder: number;
  collapsed: boolean;
};
export type Session = {
  pluginData?: Record<
    string,
    {
      value: {
        direction: 'right' | 'below' | null;
        secondary: number[];
        selected: number | null;
        ratio?: number;
      };
    }
  >;
  description: string;
  tags: import('./sdk').SessionTag[];
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
    <Dialog
      ref={dialog}
      aria-label={t('Confirm action')}
      onCancel={(event) => {
        event.preventDefault();
        choose(false);
      }}
    >
      <div className="confirmation-content">
        <h2>{t('Confirm action')}</h2>
        <p>{message}</p>
        <div className="dialog-actions">
          <Button autoFocus onClick={() => choose(false)}>
            {t('Cancel')}
          </Button>
          <Button onClick={() => choose(true)}>{t('Confirm')}</Button>
        </div>
      </div>
    </Dialog>
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
  save: (
    name: string,
    directory: string,
    projectId: number,
    description: string,
  ) => Promise<boolean>;
  dismiss: () => void;
  externalError: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [name, setName] = useState(session?.name ?? '');
  const [description, setDescription] = useState(session?.description ?? '');
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
    <Dialog
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
          void save(name, directory, projectId, description)
            .then((saved) => {
              if (saved) dismiss();
            })
            .finally(() => setBusy(false));
        }}
      >
        <h2 id="session-editor-title">{session ? t('Edit session') : t('New session')}</h2>
        <label>
          {t('Project')}
          <Select
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
          </Select>
        </label>
        <label>
          {t('Name')}
          <Input
            id="session-name"
            autoFocus
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            disabled={busy}
          />
        </label>
        <label>
          {t('Description')}
          <Textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            disabled={busy}
            rows={4}
          />
        </label>
        <label>
          {t('Base directory')}
          <Input
            id="session-directory"
            required
            value={directory}
            onChange={(event) => setDirectory(event.target.value)}
            disabled={busy}
          />
        </label>
        <Button type="button" onClick={() => void browse()} disabled={busy}>
          {t('Choose directory…')}
        </Button>
        {session && (
          <p className="muted">
            {t(
              'Directory changes apply to future launches. Running processes keep their working directory.',
            )}
          </p>
        )}
        {(error || externalError) && (
          <p role="alert" className="error">
            {error || externalError}
          </p>
        )}
        <div className="dialog-actions">
          <Button type="button" onClick={dismiss} disabled={busy}>
            {t('Cancel')}
          </Button>
          <Button disabled={busy || !name.trim() || !directory}>
            {busy ? t('Please wait…') : t('Save session')}
          </Button>
        </div>
      </form>
    </Dialog>
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
  configurationTab,
  dismissConfiguration,
  configureSession,
  configureTab,
  archives,
}: {
  configureSession: () => void;
  configureTab: (tab: PluginTab) => void;
  archives: () => void;
  configurationTab: PluginTab | null;
  dismissConfiguration: () => void;
  session: Session;
  visible: boolean;
  statuses: Record<number, ProcessStatus | null>;
  disabled: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  mutate: (command: string, args: Record<string, unknown>) => Promise<boolean>;
  closeTab: (tab: PluginTab) => void;
  markRestoring: (delta: number) => void;
}) {
  const [sharedSettingsTab, setSharedSettingsTab] = useState<number | null>(null);
  const sessionMenu = useRef<HTMLDetailsElement>(null);
  const openTabs = session.tabs.filter((tab) => tab.isOpen);
  const openedTabs = useOpeningHighlight(openTabs.map((tab) => tab.id));
  const layout = session.pluginData?.['ongleet.layout']?.value;
  const split = !!layout?.direction;
  const secondary = layout?.secondary ?? [];
  const activeGroup = secondary.includes(session.selectedTabId ?? -1) ? 1 : 0;
  const [context, setContext] = useState<{ id: number; x: number; y: number } | null>(null);
  useEffect(() => {
    if (context)
      document.querySelector<HTMLButtonElement>('.tab-context-menu button:not(:disabled)')?.focus();
  }, [context]);
  const [preview, setPreview] = useState<'right' | 'below' | null>(null);
  const [resizeRatio, setResizeRatio] = useState<number | null>(null);
  const ratio = resizeRatio ?? layout?.ratio ?? 0.5;
  const active = openTabs.find((tab) => tab.id === session.selectedTabId);
  function saveLayout(
    direction: 'right' | 'below' | null,
    ids = secondary,
    selected = layout?.selected ?? null,
    value = ratio,
  ) {
    return mutate('set_tab_layout', {
      id: session.id,
      direction,
      secondary: ids,
      selected,
      ratio: value,
    });
  }
  async function divide(id: number, direction: 'right' | 'below') {
    setContext(null);
    if (split || openTabs.length < 2) return;
    if (!(await saveLayout(direction, [id], openTabs.find((t) => t.id !== id)?.id ?? null))) return;
    await mutate('select_tab', { id });
  }
  async function moveTo(id: number, group: number, position?: number) {
    if (!split) return;
    if (
      !(await saveLayout(
        layout!.direction,
        group ? [...secondary.filter((n) => n !== id), id] : secondary.filter((n) => n !== id),
      ))
    )
      return;
    if (position !== undefined) await mutate('reorder_tab', { id, position });
    await mutate('select_tab', { id });
  }
  const [tabTitles, setTabTitles] = useState<Record<number, string>>(() =>
    loadTabTitles(session.id, session.tabs),
  );
  useEffect(() => {
    saveTabTitles(session.id, session.tabs, tabTitles);
    const current = openTabTitles(tabTitles, session.tabs);
    if (Object.keys(current).length !== Object.keys(tabTitles).length) setTabTitles(current);
  }, [session.id, session.tabs, tabTitles]);
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
  useRenderDiagnostic('SessionPane', {
    session,
    visible,
    tabTitles,
    dragTabId,
    tabDrop,
    sharedSettingsTab,
  });
  function navigate(event: React.KeyboardEvent, index: number, groupTabs: PluginTab[]) {
    const length = groupTabs.length;
    if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault();
      const position = index + (event.key === 'ArrowLeft' ? -1 : 1);
      const id = groupTabs[index].id;
      if (position >= 0 && position < length)
        void mutate('reorder_tab', {
          id,
          position: openTabs.findIndex((t) => t.id === groupTabs[position].id),
        }).then((moved) => {
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
    const id = groupTabs[target].id;
    void mutate('select_tab', { id });
  }
  return (
    <section
      className={`session-pane split-session ${split ? 'split-' + layout!.direction : ''}`}
      hidden={!visible}
      aria-label={session.name}
      style={{ '--split-ratio': `${ratio * 100}%` } as React.CSSProperties}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          setContext(null);
          setPreview(null);
        }
      }}
    >
      {(split ? [0, 1] : [0]).map((group) => {
        const groupTabs = openTabs.filter((tab) => Number(secondary.includes(tab.id)) === group);
        const groupActive =
          group === activeGroup ? active : openTabs.find((t) => t.id === layout?.selected);
        return (
          <div
            key={group}
            className={`group-header group-${group}`}
            onClickCapture={(event) => {
              if (
                !(event.target as HTMLElement).closest('[role="tab"], .tab-close') &&
                group !== activeGroup &&
                groupActive
              )
                void mutate('select_tab', { id: groupActive.id });
            }}
          >
            <div
              className="tab-bar"
              onClick={(event) => {
                if (
                  !disabled &&
                  (event.target === event.currentTarget ||
                    event.target === event.currentTarget.querySelector('.tab-list'))
                ) {
                  const menu = event.currentTarget.querySelector<HTMLDetailsElement>('details');
                  if (menu) {
                    menu.open = true;
                    menu.querySelector('summary')?.focus();
                  }
                }
              }}
            >
              <TabList
                onDragOver={(e) => {
                  if (dragTabId !== null) e.preventDefault();
                }}
                onDrop={(e) => {
                  if (dragTabId !== null) {
                    e.preventDefault();
                    void moveTo(dragTabId, group);
                    setDragTabId(null);
                  }
                }}

                aria-label={t('{value0} tabs', { value0: session.name })}
                className="tab-list"
                onDragLeave={(event) => {
                  if (
                    !(event.relatedTarget instanceof Node) ||
                    !event.currentTarget.contains(event.relatedTarget)
                  )
                    setTabDrop(null);
                }}
              >
                {groupTabs.map((tab, index) => (
                  <div
                    key={tab.id}
                    role="presentation"
                    className={`tab-item${openedTabs.has(tab.id) ? ' opening-highlight' : ''}${tab.id === groupActive?.id ? ' selected' : ''}${dragTabId === tab.id ? ' dragging' : ''}${tabDrop?.id === tab.id ? (tabDrop.after ? ' drop-after' : ' drop-before') : ''}`}
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
                      event.stopPropagation();
                      if (Number(secondary.includes(dragTabId)) !== group) {
                        const rect = event.currentTarget.getBoundingClientRect();
                        const gap =
                          openTabs.findIndex((t) => t.id === tab.id) +
                          (event.clientX >= rect.left + rect.width / 2 ? 1 : 0);
                        const from = openTabs.findIndex((t) => t.id === dragTabId);
                        void moveTo(dragTabId, group, gap - Number(from < gap));
                        setDragTabId(null);
                        return;
                      }
                      const rect = event.currentTarget.getBoundingClientRect();
                      const gap = index + (event.clientX >= rect.left + rect.width / 2 ? 1 : 0);
                      const from = groupTabs.findIndex((item) => item.id === dragTabId);
                      const position = gap - (from < gap ? 1 : 0);
                      if (from >= 0 && from !== position)
                        void mutate('reorder_tab', {
                          id: dragTabId,
                          position: openTabs.findIndex((t) => t.id === groupTabs[position]?.id),
                        });
                      setDragTabId(null);
                      setTabDrop(null);
                    }}
                  >
                    <Tab
                      id={`tab-${tab.id}`}
                      aria-selected={tab.id === groupActive?.id}
                      aria-controls={`tab-panel-${tab.id}`}
                      tabIndex={tab.id === groupActive?.id ? 0 : -1}
                      disabled={disabled}
                      draggable={!disabled}
                      onKeyDown={(event) => navigate(event, index, groupTabs)}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setContext({ id: tab.id, x: e.clientX, y: e.clientY });
                      }}
                      onClick={() => void mutate('select_tab', { id: tab.id })}
                      title={t('{value0} — {value1} · Drag to reorder · Alt+← / Alt+→', {
                        value0: tabTitles[tab.id] ?? tab.name,
                        value1:
                          tab.kind === 'terminal'
                            ? (statuses[tab.id]?.state ?? 'stopped')
                            : tab.kind,
                      })}
                      onDragStart={(event) => {
                        setDragTabId(tab.id);
                        event.dataTransfer.effectAllowed = 'move';
                        event.dataTransfer.setData('text/plain', String(tab.id));
                        event.dataTransfer.setDragImage(event.currentTarget.parentElement!, 25, 16);
                      }}
                      onDragEnd={() => {
                        setDragTabId(null);
                        setTabDrop(null);
                        setPreview(null);
                      }}
                    >
                      <PluginIcon kind={tab.kind} />
                      <span className="tab-name">{tabTitles[tab.id] ?? tab.name}</span>
                      {statuses[tab.id]?.state === 'exited' && (
                        <span>({(statuses[tab.id] as { code: number }).code})</span>
                      )}
                    </Tab>
                    <Button
                      className="tab-icon tab-close"
                      disabled={disabled}
                      tabIndex={tab.id === groupActive?.id ? 0 : -1}
                      aria-label={t('Close {value0}', { value0: tabTitles[tab.id] ?? tab.name })}
                      title={t('Close and archive tab for 30 days')}
                      onClick={() => closeTab(tab)}
                    >
                      ×
                    </Button>
                  </div>
                ))}
              </TabList>
              <SessionMenu
                menuRef={sessionMenu}
                sessionId={session.id}
                disabled={disabled}
                mutate={mutate}
                configure={configureSession}
                sharedSettings={openTabs
                  .filter(
                    (tab, index) =>
                      plugins.find((plugin) => plugin.id === tab.kind)?.SessionConfiguration &&
                      openTabs.findIndex((other) => other.kind === tab.kind) === index,
                  )
                  .map((tab) => ({
                    id: tab.id,
                    label: plugins.find((plugin) => plugin.id === tab.kind)!.label,
                  }))}
                configureShared={setSharedSettingsTab}
                archives={archives}
                activeTabName={groupActive?.name}
                configureTab={() => {
                  if (groupActive) configureTab(groupActive);
                }}
                archiveTab={() => {
                  if (groupActive) closeTab(groupActive);
                }}
              />
            </div>
          </div>
        );
      })}
      {split && (
        <div
          className="split-separator"
          role="separator"
          tabIndex={0}
          aria-label={t('Resize groups')}
          aria-orientation={layout!.direction === 'right' ? 'vertical' : 'horizontal'}
          aria-valuemin={20}
          aria-valuemax={80}
          aria-valuenow={Math.round(ratio * 100)}
          onKeyDown={(e) => {
            if (
              ['ArrowLeft', 'ArrowUp', 'ArrowRight', 'ArrowDown', 'Home', 'End'].includes(e.key)
            ) {
              e.preventDefault();
              void saveLayout(
                layout!.direction,
                secondary,
                layout?.selected,
                e.key === 'Home'
                  ? 0.2
                  : e.key === 'End'
                    ? 0.8
                    : Math.max(
                        0.2,
                        Math.min(
                          0.8,
                          ratio + (['ArrowLeft', 'ArrowUp'].includes(e.key) ? -0.05 : 0.05),
                        ),
                      ),
              );
            }
          }}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e) => {
            if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
            const r = e.currentTarget.parentElement!.getBoundingClientRect();
            setResizeRatio(
              Math.max(
                0.2,
                Math.min(
                  0.8,
                  layout!.direction === 'right'
                    ? (e.clientX - r.left) / r.width
                    : (e.clientY - r.top) / r.height,
                ),
              ),
            );
          }}
          onPointerUp={(e) => {
            e.currentTarget.releasePointerCapture(e.pointerId);
            void saveLayout(layout!.direction).finally(() => setResizeRatio(null));
          }}
          onPointerCancel={() => setResizeRatio(null)}
        />
      )}
      {context && (
        <div className="tab-context-backdrop" onPointerDown={() => setContext(null)}>
          <div
            className="tab-context-menu"
            role="menu"
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                setContext(null);
                document.getElementById(`tab-${context.id}`)?.focus();
              }
              if (['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(e.key)) {
                e.preventDefault();
                const items = [
                  ...e.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'),
                ];
                const i = items.indexOf(document.activeElement as HTMLButtonElement);
                items[
                  e.key === 'Home'
                    ? 0
                    : e.key === 'End'
                      ? items.length - 1
                      : (i + (e.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length
                ]?.focus();
              }
            }}
            style={{
              left: Math.min(context.x, window.innerWidth - 240),
              top: Math.min(context.y, window.innerHeight - 180),
            }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            <Button
              role="menuitem"
              disabled={split || openTabs.length < 2}
              onClick={() => void divide(context.id, 'right')}
            >
              {t('Split Right')}
            </Button>
            <Button
              role="menuitem"
              disabled={split || openTabs.length < 2}
              onClick={() => void divide(context.id, 'below')}
            >
              {t('Split Down')}
            </Button>
            {split && (
              <>
                <Button
                  role="menuitem"
                  onClick={() => {
                    void moveTo(context.id, 1 - Number(secondary.includes(context.id)));
                    setContext(null);
                  }}
                >
                  {t('Move to Other Group')}
                </Button>
                <Button
                  role="menuitem"
                  onClick={() => {
                    void saveLayout(layout!.direction === 'right' ? 'below' : 'right');
                    setContext(null);
                  }}
                >
                  {t('Change Split Orientation')}
                </Button>
                <Button
                  role="menuitem"
                  onClick={() => {
                    void saveLayout(null, []);
                    setContext(null);
                  }}
                >
                  {t('Join Groups')}
                </Button>
              </>
            )}
          </div>
        </div>
      )}
      {!active && (
        <div className="empty-state">
          <p>{t('No open tabs. Open the ⋮ menu to create a tab.')}</p>
        </div>
      )}
      {openTabs.map((tab) => (
        <div
          key={tab.id}
          className={`group-content group-${Number(secondary.includes(tab.id))}`}
          hidden={
            !visible ||
            (tab.id !== session.selectedTabId && (!split || tab.id !== layout?.selected))
          }
          onPointerDownCapture={() => {
            if (tab.id !== session.selectedTabId) void mutate('select_tab', { id: tab.id });
          }}
          onDragOver={(e) => {
            if (dragTabId === null) return;
            e.preventDefault();
            const r = e.currentTarget.getBoundingClientRect();
            const x = (r.right - e.clientX) / r.width,
              y = (r.bottom - e.clientY) / r.height;
            setPreview(
              !split && openTabs.length > 1
                ? x <= 0.25 && (y > 0.25 || x <= y)
                  ? 'right'
                  : y <= 0.25
                    ? 'below'
                    : null
                : null,
            );
          }}
          onDragLeave={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node)) setPreview(null);
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragTabId !== null) {
              if (preview) void divide(dragTabId, preview);
              else void moveTo(dragTabId, Number(secondary.includes(tab.id)));
            }
            setPreview(null);
            setDragTabId(null);
          }}
        >
          <TabHost
            openConfiguration={() => configureTab(tab)}
            sessionConfigurationOpen={visible && sharedSettingsTab === tab.id}
            openSessionConfiguration={() => setSharedSettingsTab(tab.id)}
            dismissSessionConfiguration={() => setSharedSettingsTab(null)}
            configurationOpen={configurationTab?.id === tab.id}
            dismissConfiguration={dismissConfiguration}
            reportTitle={reportTitle}
            key={tab.id}
            session={session}
            tab={tab}
            visible={visible && (tab.id === active?.id || (split && tab.id === layout?.selected))}
            reportStatus={reportStatus}
            markRestoring={markRestoring}
          />
        </div>
      ))}
      {preview && <div className={`split-preview preview-${preview}`} />}
    </section>
  );
}

function App() {
  useLocale();
  const [appearanceReady, setAppearanceReady] = useState(!isTauri());
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    void invoke<string>('default_actions')
      .then((yaml) =>
        invoke<{ appearance?: ApplicationAppearance | null }>('parse_actions', { yaml }),
      )
      .then((settings) => {
        if (active && settings.appearance) applyApplicationAppearance(settings.appearance);
      })
      .catch((error) => {
        if (active) setError(String(error));
      })
      .finally(() => {
        if (active) setAppearanceReady(true);
      });
    return () => {
      active = false;
    };
  }, []);
  const [configurationTab, setConfigurationTab] = useState<PluginTab | null>(null);
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
  const [groupEditorView, setGroupEditorView] = useState<'general' | 'actions'>('general');
  const [defaultEditor, setDefaultEditor] = useState(false);
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

  useRenderDiagnostic('App', {
    state,
    statuses,
    busy,
    ready,
    restoring,
    defaults,
    error,
    appearanceReady,
  });
  const refreshSequence = useRef(0);
  const refresh = useCallback(async () => {
    const sequence = ++refreshSequence.current;
    const snapshot = await invoke<SessionSnapshot>('sessions');
    if (sequence === refreshSequence.current) {
      try {
        reconcileCommentSessions(snapshot.sessions.map((session) => session.id));
      } catch (error) {
        report(`Could not clean up Diff comments: ${String(error)}`);
      }
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
          t('Quit and force-stop {value0} running terminal session(s) and their child processes?', {
            value0: status.runningTerminals,
          }),
        ))
      )
        return;
      await flushPlugins(true);
      await invoke('quit', { confirmed });
    } catch (error) {
      report(error);
    } finally {
      markBusy(false);
    }
  }

  const requestQuit = useEffectEvent(() => void quit());

  useEffect(() => {
    if (!isTauri()) return;
    const immediate = listen('application-quit-requested', () => requestQuit());
    const unlisten = listen('application-close-requested', () => requestQuit());
    void immediate.catch(report);
    void unlisten.catch(report);
    return () => {
      void immediate.then((off) => off()).catch(report);
      void unlisten.then((off) => off()).catch(report);
    };
  }, []);

  async function mutate(command: string, args: Record<string, unknown>): Promise<boolean> {
    markBusy(true);
    setError('');
    try {
      if (['archive_tab', 'close_session', 'close_project'].includes(command)) await flushPlugins();
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

  async function archiveTab(tab: PluginTab) {
    if (!(await allowEditorClose([tab.id], confirm))) return;
    const confirmed = statuses[tab.id]?.state === 'running';
    if (
      confirmed &&
      !(await confirm(
        t('Archive “{value0}” for 30 days and terminate its session and jobs?', {
          value0: tab.name,
        }),
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
  }

  // A pending terminal attachment must not lock navigation or sibling tabs.
  // Quit still waits for attachments so it cannot race a new process launch.
  const disabled = busy;
  return (
    <main className="app">
      <WindowTitleBar
        archives={() => setShowArchives(true)}
        quit={() => void quit()}
        quitDisabled={!ready || disabled || restoring > 0}
        report={report}
        disabled={disabled || !ready}
        defaults={() => {
          setError('');
          setDefaultEditor(true);
        }}
        createProject={() => {
          setError('');
          setGroupEditor('new');
        }}
      />
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
              t(
                'Delete empty project “{value0}”? Its directory and configuration file will remain untouched.',
                { value0: group.name },
              ),
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
              t(
                'Archive session “{value0}” and its saved tab definitions for 30 days?{value1} The directory and its contents will remain untouched.',
                {
                  value0: session.name,
                  value1: hasSessions
                    ? t(' All its terminal sessions and jobs will be terminated.')
                    : '',
                },
              ),
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
            <h1>{t('Welcome to Ongleet')}</h1>
            <p>{t('Create a session pointing to an existing directory.')}</p>
            <Button
              disabled={!ready || disabled}
              onClick={() => {
                if (state.projects.length) {
                  setNewProjectId(state.projects[0].id);
                  setEditor('new');
                } else setGroupEditor('new');
              }}
            >
              {state.projects.length ? t('Create session') : t('Create project')}
            </Button>
          </div>
        )}
        {appearanceReady &&
          state.sessions.map((session) => (
            <SessionPane
              key={session.id}
              configureTab={setConfigurationTab}
              archives={() => setShowArchives(true)}
              configureSession={() => {
                const project = state.projects.find((project) => project.id === session.projectId);
                if (project) {
                  setError('');
                  setGroupEditorView('actions');
                  setGroupEditor(project);
                }
              }}
              configurationTab={configurationTab}
              dismissConfiguration={() => setConfigurationTab(null)}
              session={session}
              visible={session.id === state.selectedId}
              statuses={statuses}
              disabled={disabled}
              reportStatus={reportStatus}
              mutate={mutate}
              markRestoring={markRestoring}
              closeTab={archiveTab}
            />
          ))}
      </div>
      {configurationTab &&
        !plugins.find((plugin) => plugin.id === configurationTab.kind)?.contentConfiguration && (
          <TabConfiguration
            key={configurationTab.id}
            tab={configurationTab}
            dismiss={() => setConfigurationTab(null)}
            save={(config) => mutate('configure_plugin_tab', { id: configurationTab.id, config })}
          />
        )}
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
      {defaultEditor && (
        <ProjectEditor
          defaults
          initialDirectory={defaults[0]}
          externalError=""
          dismiss={() => setDefaultEditor(false)}
          save={async () => false}
        />
      )}
      {groupEditor && (
        <ProjectEditor
          key={groupEditor === 'new' ? 'new-group' : groupEditor.id}
          project={groupEditor === 'new' ? undefined : groupEditor}
          initialDirectory={defaults[0]}
          externalError={error}
          initialView={groupEditorView}
          dismiss={() => {
            setGroupEditor(null);
            setGroupEditorView('general');
          }}
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
          save={(name, directory, projectId, description) =>
            mutate(editor === 'new' ? 'create_session' : 'update_session', {
              ...(editor === 'new' ? {} : { id: editor.id }),
              description,
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

applyAppearance();
appearance.subscribe(applyAppearance);

createRoot(document.getElementById('root')!).render(<App />);
