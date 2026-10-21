import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Workspace } from './main';

type Props = {
  workspaces: Workspace[]; selectedId: number | null; disabled: boolean; ready: boolean;
  create: () => void; edit: (workspace: Workspace) => void; remove: (workspace: Workspace) => void;
  select: (id: number) => void; reorder: (id: number, position: number) => Promise<boolean>;
};

export function WorkspaceSidebar({ workspaces, selectedId, disabled, ready, create, edit, remove, select, reorder }: Props) {
  const [menuId, setMenuId] = useState<number | null>(null);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0 });
  const [dragId, setDragId] = useState<number | null>(null);
  const [drop, setDrop] = useState<{ id: number; after: boolean } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuWorkspace = workspaces.find(workspace => workspace.id === menuId);
  const trigger = () => document.getElementById(`workspace-menu-${menuId}`);

  useLayoutEffect(() => {
    if (menuId === null) return;
    const rect = trigger()?.getBoundingClientRect();
    const height = menuRef.current?.offsetHeight ?? 90;
    if (rect) setMenuPosition({ left: Math.max(8, Math.min(rect.right - 180, window.innerWidth - 188)), top: Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - height - 8)) });
    menuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [menuId]);

  useEffect(() => {
    if (menuId === null) return;
    const outside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node) && !trigger()?.contains(event.target as Node)) setMenuId(null);
    };
    const dismiss = () => setMenuId(null);
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    return () => {
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
    };
  }, [menuId]);
  useEffect(() => { if (disabled) { setMenuId(null); setDragId(null); setDrop(null); } }, [disabled]);

  function move(workspace: Workspace, position: number) {
    setMenuId(null);
    void reorder(workspace.id, position).then(moved => {
      if (moved) document.getElementById(`workspace-grip-${workspace.id}`)?.focus();
    });
  }

  return <aside className="sidebar" aria-label="Workspaces">
    <div className="sidebar-heading"><strong>Workspaces</strong><button disabled={!ready || disabled} onClick={create} aria-label="Create workspace" title="Create workspace">+</button></div>
    <nav className="workspace-list" aria-label="Select workspace"><ol onDragLeave={event => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
    }}>
      {workspaces.map(workspace => {
        return <li key={workspace.id} className={`workspace-row${selectedId === workspace.id ? ' selected' : ''}${dragId === workspace.id ? ' dragging' : ''}${drop?.id === workspace.id ? drop.after ? ' drop-after' : ' drop-before' : ''}`}
          onDragOver={event => {
            if (disabled || dragId === null) return;
            event.preventDefault(); event.dataTransfer.dropEffect = 'move';
            const rect = event.currentTarget.getBoundingClientRect();
            const after = event.clientY >= rect.top + rect.height / 2;
            setDrop(previous => previous?.id === workspace.id && previous.after === after ? previous : { id: workspace.id, after });
          }} onDrop={event => {
            if (disabled || dragId === null) return;
            event.preventDefault();
            const rect = event.currentTarget.getBoundingClientRect();
            const gap = workspace.displayOrder + (event.clientY >= rect.top + rect.height / 2 ? 1 : 0);
            const from = workspaces.findIndex(item => item.id === dragId);
            const position = gap - (from < gap ? 1 : 0);
            if (from >= 0 && from !== position) void reorder(dragId, position);
            setDragId(null); setDrop(null);
          }}>
          <button id={`workspace-grip-${workspace.id}`} className="workspace-grip" draggable={!disabled} disabled={disabled}
            aria-label={`Reorder ${workspace.name}`} title="Drag to reorder · Alt+↑ / Alt+↓"
            onDragStart={event => {
              setMenuId(null); setDragId(workspace.id);
              event.dataTransfer.effectAllowed = 'move';
              event.dataTransfer.setData('text/plain', String(workspace.id));
              event.dataTransfer.setDragImage(event.currentTarget.parentElement!, 20, 18);
            }} onDragEnd={() => { setDragId(null); setDrop(null); }} onKeyDown={event => {
              if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return;
              event.preventDefault();
              const position = workspace.displayOrder + (event.key === 'ArrowUp' ? -1 : 1);
              if (position >= 0 && position < workspaces.length) move(workspace, position);
            }}><svg width="12" height="18" viewBox="0 0 12 18" aria-hidden="true" fill="currentColor">
              {[4, 9, 14].flatMap(y => [3, 9].map(x => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.2" />))}
            </svg></button>
          <button className="workspace-choice" aria-current={selectedId === workspace.id ? 'page' : undefined} disabled={disabled}
            title={workspace.baseDirectory} onClick={() => { setMenuId(null); select(workspace.id); }}>
            <span className="workspace-name">{workspace.name}</span>
          </button>
          <button id={`workspace-menu-${workspace.id}`} className="workspace-menu-trigger" disabled={disabled}
            aria-label={`Actions for ${workspace.name}`} aria-haspopup="menu" aria-expanded={menuId === workspace.id}
            aria-controls={menuId === workspace.id ? 'workspace-menu' : undefined}
            onClick={() => setMenuId(current => current === workspace.id ? null : workspace.id)}>⋮</button>
        </li>;
      })}
    </ol></nav>
    {menuWorkspace && !disabled && createPortal(<div id="workspace-menu" role="menu" aria-label={`${menuWorkspace.name} actions`} ref={menuRef}
      className="workspace-menu" style={{ top: menuPosition.top, left: menuPosition.left }} onKeyDown={event => {
        if (event.key === 'Escape') { event.preventDefault(); trigger()?.focus(); setMenuId(null); return; }
        const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = event.key === 'ArrowDown' ? (index + 1) % buttons.length : event.key === 'ArrowUp' ? (index + buttons.length - 1) % buttons.length : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : null;
        if (next !== null) { event.preventDefault(); buttons[next]?.focus(); }
        if (event.key === 'Tab') setMenuId(null);
      }}>
      <button role="menuitem" onClick={() => { setMenuId(null); edit(menuWorkspace); }}>Edit workspace</button>
      <div role="separator" />
      <button role="menuitem" className="danger" onClick={() => { setMenuId(null); remove(menuWorkspace); }}>Delete workspace</button>
    </div>, document.body)}
  </aside>;
}
