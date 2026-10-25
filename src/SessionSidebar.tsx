import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Project, Session } from './main';

function tagForeground(background: string) {
  const channels = [1, 3, 5].map((offset) => {
    const value = parseInt(background.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2] > 0.179
    ? '#000000'
    : '#ffffff';
}

const sidebarWidthKey = 'devcenter.sidebarWidth';

function savedSidebarWidth() {
  try {
    const saved = window.localStorage.getItem(sidebarWidthKey);
    const width = saved === null ? NaN : Number(saved);
    if (Number.isFinite(width) && width >= 150 && width <= 560) return width;
  } catch {
    // Keep resizing available when local storage is unavailable.
  }
  return 210;
}

type Props = {
  projects: Project[];
  createProject: () => void;
  editProject: (group: Project) => void;
  removeProject: (group: Project) => void;
  foldProject: (id: number, collapsed: boolean) => void;
  reorderProject: (id: number, position: number) => Promise<boolean>;
  sessions: Session[];
  selectedId: number | null;
  disabled: boolean;
  ready: boolean;
  create: (projectId: number) => void;
  edit: (session: Session) => void;
  remove: (session: Session) => void;
  select: (id: number) => void;
  reorder: (id: number, position: number) => Promise<boolean>;
};

export function SessionSidebar({
  projects,
  createProject,
  editProject,
  removeProject,
  foldProject,
  reorderProject,
  sessions,
  selectedId,
  disabled,
  ready,
  create,
  edit,
  remove,
  select,
  reorder,
}: Props) {
  const [preferredWidth, setWidth] = useState(savedSidebarWidth);
  const [maxWidth, setMaxWidth] = useState(() =>
    Math.max(150, Math.min(560, window.innerWidth - 240)),
  );
  const width = Math.min(preferredWidth, maxWidth);
  const [resizing, setResizing] = useState(false);
  const resizeOrigin = useRef<{ x: number; width: number } | null>(null);
  const clampWidth = (value: number) => Math.max(150, Math.min(maxWidth, value));
  useEffect(() => {
    try {
      window.localStorage.setItem(sidebarWidthKey, String(preferredWidth));
    } catch {
      // A storage failure must not interrupt sidebar interaction.
    }
  }, [preferredWidth]);
  useEffect(() => {
    const fit = () => {
      const maximum = Math.max(150, Math.min(560, window.innerWidth - 240));
      setMaxWidth(maximum);
    };
    fit();
    window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, []);
  const [groupMenuId, setGroupMenuId] = useState<number | null>(null);
  const [menuId, setMenuId] = useState<number | null>(null);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0 });
  const [dragId, setDragId] = useState<number | null>(null);
  const [dragProjectId, setDragProjectId] = useState<number | null>(null);
  const [projectDrop, setProjectDrop] = useState<{ id: number; after: boolean } | null>(null);
  const [drop, setDrop] = useState<{ id: number; after: boolean } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuSession = sessions.find((session) => session.id === menuId);
  const menuGroup = projects.find((group) => group.id === groupMenuId);
  const trigger = () =>
    document.getElementById(
      groupMenuId !== null ? `project-menu-${groupMenuId}` : `session-menu-${menuId}`,
    );

  useLayoutEffect(() => {
    if (menuId === null && groupMenuId === null) return;
    const rect = trigger()?.getBoundingClientRect();
    const height = menuRef.current?.offsetHeight ?? 90;
    if (rect)
      setMenuPosition({
        left: Math.max(8, Math.min(rect.right - 180, window.innerWidth - 188)),
        top: Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - height - 8)),
      });
    menuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [menuId, groupMenuId]);

  useEffect(() => {
    if (menuId === null && groupMenuId === null) return;
    const outside = (event: PointerEvent) => {
      if (
        !menuRef.current?.contains(event.target as Node) &&
        !trigger()?.contains(event.target as Node)
      ) {
        setMenuId(null);
        setGroupMenuId(null);
      }
    };
    const dismiss = () => {
      setMenuId(null);
      setGroupMenuId(null);
    };
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    return () => {
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
    };
  }, [menuId, groupMenuId]);
  useEffect(() => {
    if (disabled) {
      setMenuId(null);
      setGroupMenuId(null);
      setDragId(null);
      setDrop(null);
    }
  }, [disabled]);

  function move(session: Session, position: number) {
    setMenuId(null);
    void reorder(session.id, position).then((moved) => {
      if (moved) document.getElementById(`session-grip-${session.id}`)?.focus();
    });
  }

  function moveProject(id: number, position: number) {
    setGroupMenuId(null);
    void reorderProject(id, position).then((moved) => {
      if (moved) document.getElementById(`project-grip-${id}`)?.focus();
    });
  }

  return (
    <aside id="session-sidebar" className="sidebar" aria-label="Sessions" style={{ width }}>
      <div className="sidebar-heading">
        <strong>Projects</strong>
        <button
          disabled={!ready || disabled}
          onClick={createProject}
          aria-label="Create project"
          title="Create project"
        >
          +
        </button>
      </div>
      <nav className="session-list" aria-label="Select session">
        <ol
          onDragLeave={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
          }}
        >
          {projects.map((group) => (
            <li key={group.id} className="project" data-project-id={group.id}>
              <div
                className={`project-heading${projectDrop?.id === group.id ? (projectDrop.after ? ' drop-after' : ' drop-before') : ''}`}
                onDragOver={(event) => {
                  if (disabled || dragProjectId === null) return;
                  event.preventDefault();
                  event.dataTransfer.dropEffect = 'move';
                  const rect = event.currentTarget.getBoundingClientRect();
                  setProjectDrop({
                    id: group.id,
                    after: event.clientY >= rect.top + rect.height / 2,
                  });
                }}
                onDragLeave={() => setProjectDrop(null)}
                onDrop={(event) => {
                  if (disabled || dragProjectId === null) return;
                  event.preventDefault();
                  const rect = event.currentTarget.getBoundingClientRect();
                  const from = projects.findIndex((project) => project.id === dragProjectId);
                  const gap =
                    projects.indexOf(group) + (event.clientY >= rect.top + rect.height / 2 ? 1 : 0);
                  const position = gap - (from < gap ? 1 : 0);
                  if (from >= 0 && from !== position) moveProject(dragProjectId, position);
                  setDragProjectId(null);
                  setProjectDrop(null);
                }}
              >
                <button
                  id={`project-grip-${group.id}`}
                  className="session-grip"
                  draggable={!disabled}
                  disabled={disabled}
                  aria-label={`Reorder project ${group.name}`}
                  title="Drag to reorder · Alt+↑ / Alt+↓"
                  onDragStart={(event) => {
                    setGroupMenuId(null);
                    setMenuId(null);
                    setDragProjectId(group.id);
                    event.dataTransfer.effectAllowed = 'move';
                    event.dataTransfer.setData('text/plain', String(group.id));
                    event.dataTransfer.setDragImage(event.currentTarget.parentElement!, 20, 18);
                  }}
                  onDragEnd={() => {
                    setDragProjectId(null);
                    setProjectDrop(null);
                  }}
                  onKeyDown={(event) => {
                    if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return;
                    event.preventDefault();
                    const position = projects.indexOf(group) + (event.key === 'ArrowUp' ? -1 : 1);
                    if (position >= 0 && position < projects.length)
                      moveProject(group.id, position);
                  }}
                >
                  <svg
                    width="12"
                    height="18"
                    viewBox="0 0 12 18"
                    aria-hidden="true"
                    fill="currentColor"
                  >
                    {[4, 9, 14].flatMap((y) =>
                      [3, 9].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.2" />),
                    )}
                  </svg>
                </button>
                <button
                  className="project-toggle"
                  disabled={disabled}
                  aria-expanded={!group.collapsed}
                  aria-controls={`project-${group.id}`}
                  onClick={() => {
                    setMenuId(null);
                    setGroupMenuId(null);
                    foldProject(group.id, !group.collapsed);
                  }}
                >
                  <span aria-hidden="true">{group.collapsed ? '▸' : '▾'}</span>
                  <span className="session-name">{group.name}</span>
                </button>
                <button
                  className="project-icon"
                  disabled={!ready || disabled}
                  aria-label="Create session"
                  title={`Create session in ${group.name}`}
                  onClick={() => create(group.id)}
                >
                  +
                </button>
                <button
                  id={`project-menu-${group.id}`}
                  className="session-menu-trigger"
                  disabled={disabled}
                  aria-label={`Actions for project ${group.name}`}
                  aria-haspopup="menu"
                  aria-expanded={groupMenuId === group.id}
                  onClick={() => {
                    setMenuId(null);
                    setGroupMenuId((current) => (current === group.id ? null : group.id));
                  }}
                >
                  ⋮
                </button>
              </div>
              <ol id={`project-${group.id}`} className="project-sessions" hidden={group.collapsed}>
                {sessions
                  .filter((session) => session.projectId === group.id)
                  .map((session) => {
                    return (
                      <li
                        key={session.id}
                        className={`session-row${selectedId === session.id ? ' selected' : ''}${dragId === session.id ? ' dragging' : ''}${drop?.id === session.id ? (drop.after ? ' drop-after' : ' drop-before') : ''}`}
                        onDragOver={(event) => {
                          if (
                            disabled ||
                            dragId === null ||
                            sessions.find((w) => w.id === dragId)?.projectId !== session.projectId
                          )
                            return;
                          event.preventDefault();
                          event.dataTransfer.dropEffect = 'move';
                          const rect = event.currentTarget.getBoundingClientRect();
                          const after = event.clientY >= rect.top + rect.height / 2;
                          setDrop((previous) =>
                            previous?.id === session.id && previous.after === after
                              ? previous
                              : { id: session.id, after },
                          );
                        }}
                        onDrop={(event) => {
                          if (
                            disabled ||
                            dragId === null ||
                            sessions.find((w) => w.id === dragId)?.projectId !== session.projectId
                          )
                            return;
                          event.preventDefault();
                          const rect = event.currentTarget.getBoundingClientRect();
                          const gap =
                            session.displayOrder +
                            (event.clientY >= rect.top + rect.height / 2 ? 1 : 0);
                          const from = sessions.findIndex((item) => item.id === dragId);
                          const position = gap - (from < gap ? 1 : 0);
                          if (from >= 0 && from !== position) void reorder(dragId, position);
                          setDragId(null);
                          setDrop(null);
                        }}
                      >
                        <button
                          id={`session-grip-${session.id}`}
                          className="session-grip"
                          draggable={!disabled}
                          disabled={disabled}
                          aria-label={`Reorder ${session.name}`}
                          title="Drag to reorder · Alt+↑ / Alt+↓"
                          onDragStart={(event) => {
                            setMenuId(null);
                            setDragId(session.id);
                            event.dataTransfer.effectAllowed = 'move';
                            event.dataTransfer.setData('text/plain', String(session.id));
                            event.dataTransfer.setDragImage(
                              event.currentTarget.parentElement!,
                              20,
                              18,
                            );
                          }}
                          onDragEnd={() => {
                            setDragId(null);
                            setDrop(null);
                          }}
                          onKeyDown={(event) => {
                            if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key))
                              return;
                            event.preventDefault();
                            const siblings = sessions.filter(
                              (w) => w.projectId === session.projectId,
                            );
                            const target =
                              siblings[
                                siblings.findIndex((w) => w.id === session.id) +
                                  (event.key === 'ArrowUp' ? -1 : 1)
                              ];
                            if (target) move(session, target.displayOrder);
                          }}
                        >
                          <svg
                            width="12"
                            height="18"
                            viewBox="0 0 12 18"
                            aria-hidden="true"
                            fill="currentColor"
                          >
                            {[4, 9, 14].flatMap((y) =>
                              [3, 9].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.2" />),
                            )}
                          </svg>
                        </button>
                        <button
                          className="session-choice"
                          aria-current={selectedId === session.id ? 'page' : undefined}
                          disabled={disabled}
                          title={session.baseDirectory}
                          onClick={() => {
                            setMenuId(null);
                            select(session.id);
                          }}
                        >
                          <span className="session-name">{session.name}</span>
                          <span className="session-directory">{session.baseDirectory}</span>
                          {session.tags.length > 0 && (
                            <span className="session-tags">
                              {session.tags.map((tag) => (
                                <span
                                  key={tag.name}
                                  className="session-tag"
                                  title={tag.name}
                                  style={{
                                    backgroundColor: tag.background,
                                    color: tag.background
                                      ? tagForeground(tag.background)
                                      : undefined,
                                  }}
                                >
                                  {tag.dot && (
                                    <span
                                      className="session-tag-dot"
                                      style={{ backgroundColor: tag.dot }}
                                    />
                                  )}
                                  {tag.name}
                                </span>
                              ))}
                            </span>
                          )}
                        </button>
                        <button
                          id={`session-menu-${session.id}`}
                          className="session-menu-trigger"
                          disabled={disabled}
                          aria-label={`Actions for ${session.name}`}
                          aria-haspopup="menu"
                          aria-expanded={menuId === session.id}
                          aria-controls={menuId === session.id ? 'session-menu' : undefined}
                          onClick={() => {
                            setGroupMenuId(null);
                            setMenuId((current) => (current === session.id ? null : session.id));
                          }}
                        >
                          ⋮
                        </button>
                      </li>
                    );
                  })}
              </ol>
            </li>
          ))}
        </ol>
      </nav>
      <div
        className="sidebar-resizer"
        role="separator"
        tabIndex={0}
        aria-label="Resize sidebar"
        aria-orientation="vertical"
        aria-controls="session-sidebar"
        aria-valuemin={150}
        aria-valuemax={maxWidth}
        aria-valuenow={width}
        title="Drag to resize · Arrow keys to adjust · Double-click to reset"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          setMenuId(null);
          resizeOrigin.current = { x: event.clientX, width };
          event.currentTarget.setPointerCapture(event.pointerId);
          setResizing(true);
        }}
        onPointerMove={(event) => {
          const origin = resizeOrigin.current;
          if (origin) setWidth(clampWidth(origin.width + event.clientX - origin.x));
        }}
        onPointerUp={(event) => {
          resizeOrigin.current = null;
          setResizing(false);
          if (event.currentTarget.hasPointerCapture(event.pointerId))
            event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onLostPointerCapture={() => {
          resizeOrigin.current = null;
          setResizing(false);
        }}
        onPointerCancel={() => {
          resizeOrigin.current = null;
          setResizing(false);
        }}
        onDoubleClick={() => setWidth(clampWidth(210))}
        onKeyDown={(event) => {
          const next =
            event.key === 'ArrowLeft'
              ? width - 10
              : event.key === 'ArrowRight'
                ? width + 10
                : event.key === 'Home'
                  ? 150
                  : event.key === 'End'
                    ? maxWidth
                    : null;
          if (next !== null) {
            event.preventDefault();
            setWidth(clampWidth(next));
          }
        }}
      />
      {resizing && createPortal(<div className="sidebar-resize-overlay" />, document.body)}
      {menuGroup &&
        !disabled &&
        createPortal(
          <div
            role="menu"
            aria-label={`${menuGroup.name} project actions`}
            ref={menuRef}
            className="session-menu"
            style={{ top: menuPosition.top, left: menuPosition.left }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                trigger()?.focus();
                setGroupMenuId(null);
              }
              if (event.key === 'Tab') setGroupMenuId(null);
              const buttons = Array.from(
                event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'),
              );
              const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
              const next =
                event.key === 'ArrowDown'
                  ? (index + 1) % buttons.length
                  : event.key === 'ArrowUp'
                    ? (index + buttons.length - 1) % buttons.length
                    : event.key === 'Home'
                      ? 0
                      : event.key === 'End'
                        ? buttons.length - 1
                        : null;
              if (next !== null) {
                event.preventDefault();
                buttons[next]?.focus();
              }
            }}
          >
            <button
              role="menuitem"
              onClick={() => {
                setGroupMenuId(null);
                editProject(menuGroup);
              }}
            >
              Edit project
            </button>
            <button
              role="menuitem"
              disabled={sessions.some((w) => w.projectId === menuGroup.id)}
              title="Move or close its sessions before deleting the group"
              className="danger"
              onClick={() => {
                setGroupMenuId(null);
                removeProject(menuGroup);
              }}
            >
              Delete project
            </button>
          </div>,
          document.body,
        )}
      {menuSession &&
        !disabled &&
        createPortal(
          <div
            id="session-menu"
            role="menu"
            aria-label={`${menuSession.name} actions`}
            ref={menuRef}
            className="session-menu"
            style={{ top: menuPosition.top, left: menuPosition.left }}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                trigger()?.focus();
                setMenuId(null);
                return;
              }
              const buttons = Array.from(
                event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'),
              );
              const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
              const next =
                event.key === 'ArrowDown'
                  ? (index + 1) % buttons.length
                  : event.key === 'ArrowUp'
                    ? (index + buttons.length - 1) % buttons.length
                    : event.key === 'Home'
                      ? 0
                      : event.key === 'End'
                        ? buttons.length - 1
                        : null;
              if (next !== null) {
                event.preventDefault();
                buttons[next]?.focus();
              }
              if (event.key === 'Tab') setMenuId(null);
            }}
          >
            <button
              role="menuitem"
              onClick={() => {
                setMenuId(null);
                edit(menuSession);
              }}
            >
              Edit session
            </button>
            <div role="separator" />
            <button
              role="menuitem"
              className="danger"
              onClick={() => {
                setMenuId(null);
                remove(menuSession);
              }}
            >
              Archive session
            </button>
          </div>,
          document.body,
        )}
    </aside>
  );
}
