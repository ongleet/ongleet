import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

const minimum = 150;
const maximum = 560;
const defaultWidth = 260;

function savedWidth(storageKey: string) {
  try {
    const value = localStorage.getItem(storageKey);
    const width = value === null ? NaN : Number(value);
    if (Number.isFinite(width) && width >= minimum && width <= maximum) return width;
  } catch {
    // Resizing remains available when local storage is unavailable.
  }
  return defaultWidth;
}

export function FileSidebar({
  children,
  storageKey,
  label,
  className = '',
}: {
  children: ReactNode;
  storageKey: string;
  label: string;
  className?: string;
}) {
  const [preferredWidth, setWidth] = useState(() => savedWidth(storageKey));
  const [limit, setLimit] = useState(maximum);
  const [pixelRatio, setPixelRatio] = useState(() => window.devicePixelRatio || 1);
  const [resizing, setResizing] = useState(false);
  const sidebar = useRef<HTMLDivElement>(null);
  const origin = useRef<{ x: number; width: number } | null>(null);
  // Snap restored and dragged widths so the viewer starts on a physical pixel.
  const width = Math.round(Math.min(preferredWidth, limit) * pixelRatio) / pixelRatio;
  const clamp = (value: number) => Math.max(minimum, Math.min(limit, value));

  useEffect(() => {
    const updatePixelRatio = () => setPixelRatio(window.devicePixelRatio || 1);
    window.addEventListener('resize', updatePixelRatio);
    return () => window.removeEventListener('resize', updatePixelRatio);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(preferredWidth));
    } catch {
      // A storage failure must not interrupt sidebar interaction.
    }
  }, [preferredWidth, storageKey]);

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
    <div ref={sidebar} className={`file-sidebar ${className}`} style={{ width }}>
      <aside aria-label={label}>{children}</aside>
      <div
        className="file-sidebar-resizer"
        role="separator"
        tabIndex={0}
        aria-label={`Resize ${label.toLowerCase()} sidebar`}
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
      {resizing && createPortal(<div className="file-sidebar-resize-overlay" />, document.body)}
    </div>
  );
}
