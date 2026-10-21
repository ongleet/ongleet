import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Workgroup, Workspace } from './main';

type Props = {
  workgroups: Workgroup[];
  createWorkgroup: () => void; editWorkgroup: (group: Workgroup) => void; removeWorkgroup: (group: Workgroup) => void;
  foldWorkgroup: (id: number, collapsed: boolean) => void;
  workspaces: Workspace[]; selectedId: number | null; disabled: boolean; ready: boolean;
  create: (workgroupId: number) => void; edit: (workspace: Workspace) => void; remove: (workspace: Workspace) => void;
  select: (id: number) => void; reorder: (id: number, position: number) => Promise<boolean>;
};

export function WorkspaceSidebar({ workgroups, createWorkgroup, editWorkgroup, removeWorkgroup, foldWorkgroup, workspaces, selectedId, disabled, ready, create, edit, remove, select, reorder }: Props) {
  const [width, setWidth] = useState(210);
  const [maxWidth, setMaxWidth] = useState(() => Math.max(150, Math.min(560, window.innerWidth - 240)));
  const [resizing, setResizing] = useState(false);
  const resizeOrigin = useRef<{ x: number; width: number } | null>(null);
  const clampWidth = (value: number) => Math.max(150, Math.min(maxWidth, value));
  useEffect(() => {
    const fit = () => {
      const maximum = Math.max(150, Math.min(560, window.innerWidth - 240));
      setMaxWidth(maximum);
      setWidth(current => Math.min(current, maximum));
    };
    fit(); window.addEventListener('resize', fit);
    return () => window.removeEventListener('resize', fit);
  }, []);
  const [groupMenuId, setGroupMenuId] = useState<number | null>(null);
  const [menuId, setMenuId] = useState<number | null>(null);
  const [menuPosition, setMenuPosition] = useState({ top: 0, left: 0 });
  const [dragId, setDragId] = useState<number | null>(null);
  const [drop, setDrop] = useState<{ id: number; after: boolean } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuWorkspace = workspaces.find(workspace => workspace.id === menuId);
  const menuGroup = workgroups.find(group => group.id === groupMenuId);
  const trigger = () => document.getElementById(groupMenuId !== null ? `workgroup-menu-${groupMenuId}` : `workspace-menu-${menuId}`);

  useLayoutEffect(() => {
    if (menuId === null && groupMenuId === null) return;
    const rect = trigger()?.getBoundingClientRect();
    const height = menuRef.current?.offsetHeight ?? 90;
    if (rect) setMenuPosition({ left: Math.max(8, Math.min(rect.right - 180, window.innerWidth - 188)), top: Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - height - 8)) });
    menuRef.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [menuId, groupMenuId]);

  useEffect(() => {
    if (menuId === null && groupMenuId === null) return;
    const outside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node) && !trigger()?.contains(event.target as Node)) { setMenuId(null); setGroupMenuId(null); }
    };
    const dismiss = () => { setMenuId(null); setGroupMenuId(null); };
    document.addEventListener('pointerdown', outside, true);
    document.addEventListener('scroll', dismiss, true);
    window.addEventListener('resize', dismiss);
    return () => {
      document.removeEventListener('pointerdown', outside, true);
      document.removeEventListener('scroll', dismiss, true);
      window.removeEventListener('resize', dismiss);
    };
  }, [menuId, groupMenuId]);
  useEffect(() => { if (disabled) { setMenuId(null); setGroupMenuId(null); setDragId(null); setDrop(null); } }, [disabled]);

  function move(workspace: Workspace, position: number) {
    setMenuId(null);
    void reorder(workspace.id, position).then(moved => {
      if (moved) document.getElementById(`workspace-grip-${workspace.id}`)?.focus();
    });
  }

  return <aside id="workspace-sidebar" className="sidebar" aria-label="Workspaces" style={{ width }}>
    <div className="sidebar-heading"><strong>Workgroups</strong><button disabled={!ready || disabled} onClick={createWorkgroup} aria-label="Create workgroup" title="Create workgroup">+</button></div>
    <nav className="workspace-list" aria-label="Select workspace"><ol onDragLeave={event => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrop(null);
    }}>
      {workgroups.map(group => <li key={group.id} className="workgroup" data-workgroup-id={group.id}>
        <div className="workgroup-heading">
          <button className="workgroup-toggle" disabled={disabled} aria-expanded={!group.collapsed} aria-controls={`workgroup-${group.id}`} onClick={() => { setMenuId(null); setGroupMenuId(null); foldWorkgroup(group.id, !group.collapsed); }}>
            <span aria-hidden="true">{group.collapsed ? '▸' : '▾'}</span><span className="workspace-name">{group.name}</span>
          </button>
          <button className="workgroup-icon" disabled={!ready || disabled} aria-label="Create workspace" title={`Create workspace in ${group.name}`} onClick={() => create(group.id)}>+</button>
          <button id={`workgroup-menu-${group.id}`} className="workspace-menu-trigger" disabled={disabled} aria-label={`Actions for workgroup ${group.name}`} aria-haspopup="menu" aria-expanded={groupMenuId === group.id} onClick={() => { setMenuId(null); setGroupMenuId(current => current === group.id ? null : group.id); }}>⋮</button>
        </div>
        <ol id={`workgroup-${group.id}`} className="workgroup-workspaces" hidden={group.collapsed}>
      {workspaces.filter(workspace => workspace.workgroupId === group.id).map(workspace => {
        return <li key={workspace.id} className={`workspace-row${selectedId === workspace.id ? ' selected' : ''}${dragId === workspace.id ? ' dragging' : ''}${drop?.id === workspace.id ? drop.after ? ' drop-after' : ' drop-before' : ''}`}
          onDragOver={event => {
            if (disabled || dragId === null || workspaces.find(w => w.id === dragId)?.workgroupId !== workspace.workgroupId) return;
            event.preventDefault(); event.dataTransfer.dropEffect = 'move';
            const rect = event.currentTarget.getBoundingClientRect();
            const after = event.clientY >= rect.top + rect.height / 2;
            setDrop(previous => previous?.id === workspace.id && previous.after === after ? previous : { id: workspace.id, after });
          }} onDrop={event => {
            if (disabled || dragId === null || workspaces.find(w => w.id === dragId)?.workgroupId !== workspace.workgroupId) return;
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
              const siblings = workspaces.filter(w => w.workgroupId === workspace.workgroupId);
              const target = siblings[siblings.findIndex(w => w.id === workspace.id) + (event.key === 'ArrowUp' ? -1 : 1)];
              if (target) move(workspace, target.displayOrder);
            }}><svg width="12" height="18" viewBox="0 0 12 18" aria-hidden="true" fill="currentColor">
              {[4, 9, 14].flatMap(y => [3, 9].map(x => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.2" />))}
            </svg></button>
          <button className="workspace-choice" aria-current={selectedId === workspace.id ? 'page' : undefined} disabled={disabled}
            title={workspace.baseDirectory} onClick={() => { setMenuId(null); select(workspace.id); }}>
            <span className="workspace-name">{workspace.name}</span>
            <span className="workspace-directory">{workspace.baseDirectory}</span>
          </button>
          <button id={`workspace-menu-${workspace.id}`} className="workspace-menu-trigger" disabled={disabled}
            aria-label={`Actions for ${workspace.name}`} aria-haspopup="menu" aria-expanded={menuId === workspace.id}
            aria-controls={menuId === workspace.id ? 'workspace-menu' : undefined}
            onClick={() => { setGroupMenuId(null); setMenuId(current => current === workspace.id ? null : workspace.id); }}>⋮</button>
        </li>;
      })}
        </ol>
      </li>)}
    </ol></nav>
    <div className="sidebar-resizer" role="separator" tabIndex={0} aria-label="Resize sidebar"
      aria-orientation="vertical" aria-controls="workspace-sidebar" aria-valuemin={150} aria-valuemax={maxWidth} aria-valuenow={width}
      title="Drag to resize · Arrow keys to adjust · Double-click to reset"
      onPointerDown={event => {
        if (event.button !== 0) return;
        event.preventDefault(); event.currentTarget.focus(); setMenuId(null);
        resizeOrigin.current = { x: event.clientX, width };
        event.currentTarget.setPointerCapture(event.pointerId); setResizing(true);
      }} onPointerMove={event => {
        const origin = resizeOrigin.current;
        if (origin) setWidth(clampWidth(origin.width + event.clientX - origin.x));
      }} onPointerUp={event => {
        resizeOrigin.current = null; setResizing(false);
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
      }} onLostPointerCapture={() => { resizeOrigin.current = null; setResizing(false); }}
      onPointerCancel={() => { resizeOrigin.current = null; setResizing(false); }}
      onDoubleClick={() => setWidth(clampWidth(210))}
      onKeyDown={event => {
        const next = event.key === 'ArrowLeft' ? width - 10 : event.key === 'ArrowRight' ? width + 10 : event.key === 'Home' ? 150 : event.key === 'End' ? maxWidth : null;
        if (next !== null) { event.preventDefault(); setWidth(clampWidth(next)); }
      }} />
    {resizing && createPortal(<div className="sidebar-resize-overlay" />, document.body)}
    {menuGroup && !disabled && createPortal(<div role="menu" aria-label={`${menuGroup.name} workgroup actions`} ref={menuRef} className="workspace-menu" style={{ top: menuPosition.top, left: menuPosition.left }} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); trigger()?.focus(); setGroupMenuId(null); } if (event.key === 'Tab') setGroupMenuId(null);
      const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === 'ArrowDown' ? (index + 1) % buttons.length : event.key === 'ArrowUp' ? (index + buttons.length - 1) % buttons.length : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : null;
      if (next !== null) { event.preventDefault(); buttons[next]?.focus(); }
    }}>
      <button role="menuitem" onClick={() => { setGroupMenuId(null); editWorkgroup(menuGroup); }}>Edit workgroup</button>
      <button role="menuitem" disabled={workspaces.some(w => w.workgroupId === menuGroup.id)} title="Move or close its workspaces before deleting the group" className="danger" onClick={() => { setGroupMenuId(null); removeWorkgroup(menuGroup); }}>Delete workgroup</button>
    </div>, document.body)}
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
