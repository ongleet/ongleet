import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

const storageKey = 'ongleet.diff.sidebarWidth';
const minimum = 150;
const maximum = 560;
const defaultWidth = 260;

function savedWidth() {
  try {
    const value = localStorage.getItem(storageKey);
    const width = value === null ? NaN : Number(value);
    if (Number.isFinite(width) && width >= minimum && width <= maximum) return width;
  } catch {
    // Resizing remains available when local storage is unavailable.
  }
  return defaultWidth;
}

export function DiffSidebar({ children }: { children: ReactNode }) {
  const [preferredWidth, setWidth] = useState(savedWidth);
  const [limit, setLimit] = useState(maximum);
  const [resizing, setResizing] = useState(false);
  const sidebar = useRef<HTMLDivElement>(null);
  const origin = useRef<{ x: number; width: number } | null>(null);
  const width = Math.min(preferredWidth, limit);
  const clamp = (value: number) => Math.max(minimum, Math.min(limit, value));

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(preferredWidth));
    } catch {
      // A storage failure must not interrupt sidebar interaction.
    }
  }, [preferredWidth]);

  useEffect(() => {
    const layout = sidebar.current?.parentElement;
    if (!layout) return;
    const observer = new ResizeObserver(() => {
      if (layout.clientWidth > 0)
        setLimit(Math.max(minimum, Math.min(maximum, layout.clientWidth - 252)));
    });
    observer.observe(layout);
    return () => observer.disconnect();
  }, []);

  const stop = () => {
    origin.current = null;
    setResizing(false);
  };

  return (
    <div ref={sidebar} className="diff-sidebar" style={{ width }}>
      <aside aria-label="Diff files">{children}</aside>
      <div
        className="diff-sidebar-resizer"
        role="separator"
        tabIndex={0}
        aria-label="Resize diff sidebar"
        aria-orientation="vertical"
        aria-valuemin={minimum}
        aria-valuemax={limit}
        aria-valuenow={width}
        title="Drag to resize · Arrow keys to adjust · Double-click to reset"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          event.currentTarget.focus();
          origin.current = { x: event.clientX, width };
          event.currentTarget.setPointerCapture(event.pointerId);
          setResizing(true);
        }}
        onPointerMove={(event) => {
          if (origin.current)
            setWidth(clamp(origin.current.width + event.clientX - origin.current.x));
        }}
        onPointerUp={(event) => {
          stop();
          if (event.currentTarget.hasPointerCapture(event.pointerId))
            event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onPointerCancel={stop}
        onLostPointerCapture={stop}
        onDoubleClick={() => setWidth(clamp(defaultWidth))}
        onKeyDown={(event) => {
          const next =
            event.key === 'ArrowLeft'
              ? width - 10
              : event.key === 'ArrowRight'
                ? width + 10
                : event.key === 'Home'
                  ? minimum
                  : event.key === 'End'
                    ? limit
                    : null;
          if (next !== null) {
            event.preventDefault();
            setWidth(clamp(next));
          }
        }}
      />
      {resizing && createPortal(<div className="diff-sidebar-resize-overlay" />, document.body)}
    </div>
  );
}
