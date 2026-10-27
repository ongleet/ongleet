import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function WindowTitleBar({
  report,
  quit,
  quitDisabled,
  archives,
  defaults,
  createProject,
  disabled,
}: {
  report: (error: unknown) => void;
  quit: () => void;
  quitDisabled: boolean;
  archives: () => void;
  defaults: () => void;
  createProject: () => void;
  disabled: boolean;
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    const window = getCurrentWindow();
    let disposed = false;
    const update = async () => {
      const value = await window.isMaximized();
      if (!disposed) setMaximized(value);
    };
    const unlisten = window.onResized(() => void update().catch(report));
    void update().catch(report);
    return () => {
      disposed = true;
      void unlisten.then((off) => off()).catch(report);
    };
  }, []);

  return (
    <header className="window-titlebar">
      <span className="window-brand-mark" aria-label="DevCenter">
        ›_
      </span>
      <details
        className="titlebar-menu"
        name="titlebar-menu"
        ref={menu}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            event.currentTarget.open = false;
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.currentTarget.open = false;
            event.currentTarget.querySelector('summary')?.focus();
          }
        }}
      >
        <summary aria-label="Application menu" title="Application menu">
          Application
        </summary>
        <div className="app-menu-items">
          <button
            disabled={disabled}
            onClick={() => {
              menu.current!.open = false;
              createProject();
            }}
          >
            New project…
          </button>
          <button
            disabled={disabled}
            onClick={() => {
              menu.current!.open = false;
              defaults();
            }}
          >
            Configure…
          </button>
          <button
            disabled={disabled}
            onClick={() => {
              menu.current!.open = false;
              archives();
            }}
          >
            Archives
          </button>
          <button
            disabled={quitDisabled}
            onClick={() => {
              menu.current!.open = false;
              quit();
            }}
          >
            Quit
          </button>
        </div>
      </details>
      <div
        className="window-drag-area"
        onMouseDown={(event) => {
          if (!isTauri() || event.button !== 0) return;
          if (event.detail === 2) void getCurrentWindow().toggleMaximize().catch(report);
          else void getCurrentWindow().startDragging().catch(report);
        }}
      />
      <div className="window-controls">
        <button
          aria-label="Minimize window"
          title="Minimize"
          disabled={!isTauri()}
          onClick={() => void getCurrentWindow().minimize().catch(report)}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M3 8h10" />
          </svg>
        </button>
        <button
          aria-label={maximized ? 'Restore window' : 'Maximize window'}
          title={maximized ? 'Restore' : 'Maximize'}
          disabled={!isTauri()}
          onClick={() => void getCurrentWindow().toggleMaximize().catch(report)}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            {maximized ? <path d="M5 5V3h8v8h-2M3 5h8v8H3z" /> : <path d="M3 3h10v10H3z" />}
          </svg>
        </button>
        <button
          className="window-close"
          aria-label="Close window"
          title="Close — keep sessions running"
          disabled={!isTauri()}
          onClick={() => void getCurrentWindow().close().catch(report)}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="m4 4 8 8m0-8-8 8" />
          </svg>
        </button>
      </div>
    </header>
  );
}
