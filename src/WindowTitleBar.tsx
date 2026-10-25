import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function WindowTitleBar({ report }: { report: (error: unknown) => void }) {
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
      <div
        className="window-drag-area"
        onMouseDown={(event) => {
          if (!isTauri() || event.button !== 0) return;
          if (event.detail === 2) void getCurrentWindow().toggleMaximize().catch(report);
          else void getCurrentWindow().startDragging().catch(report);
        }}
      >
        <span className="window-brand-mark" aria-hidden="true">
          ›_
        </span>
        <span>DevCenter</span>
      </div>
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
