import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PluginContentProps } from '../types';
type Listing = {
  folder: string;
  parent: string | null;
  entries: { name: string; path: string; directory: boolean; symlink: boolean }[];
  nextOffset: number | null;
};
export function FilesContent({ tab, visible }: PluginContentProps) {
  const configuredFolder = (tab.config as { folder?: string } | null)?.folder;
  const [listing, setListing] = useState<Listing>();
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [order, setOrder] = useState('nameAscending');
  const generation = useRef(0);
  const pending = useRef(false);
  const content = useRef<HTMLDivElement>(null);
  const loadMore = useRef<HTMLDivElement>(null);
  async function browse(
    folder?: string,
    offset = 0,
    sortOrder = order,
    includeHidden = showHidden,
  ) {
    if (offset > 0 && pending.current) return;
    const request = ++generation.current;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      const result = await invoke<Listing>('list_plugin_files', {
        id: tab.id,
        folder: folder ?? null,
        offset,
        order: sortOrder,
        showHidden: includeHidden,
      });
      if (request === generation.current) {
        setListing((previous) => {
          if (offset === 0 || previous?.folder !== result.folder) return result;
          const existing = new Set(previous.entries.map((entry) => entry.path));
          return {
            ...result,
            entries: [
              ...previous.entries,
              ...result.entries.filter((entry) => !existing.has(entry.path)),
            ],
          };
        });
        if (offset === 0) {
          setPath(result.folder);
          content.current?.scrollTo(0, 0);
        }
      }
    } catch (error) {
      if (request === generation.current) setError(String(error));
    } finally {
      if (request === generation.current) {
        pending.current = false;
        setBusy(false);
      }
    }
  }
  useEffect(() => {
    void browse();
    return () => {
      ++generation.current;
    };
  }, [tab.id, configuredFolder]);
  useEffect(() => {
    if (!visible) return;
    const toggleHidden = (event: KeyboardEvent) => {
      if (
        event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === 'h'
      ) {
        event.preventDefault();
        if (event.repeat) return;
        setShowHidden(!showHidden);
        void browse(listing?.folder, 0, order, !showHidden);
      }
    };
    window.addEventListener('keydown', toggleHidden);
    return () => window.removeEventListener('keydown', toggleHidden);
  }, [visible, showHidden, listing?.folder, order, tab.id]);
  useEffect(() => {
    if (!visible || busy || error || listing?.nextOffset == null || !loadMore.current) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          void browse(listing.folder, listing.nextOffset!);
        }
      },
      { root: content.current, rootMargin: '0px 0px 200px 0px' },
    );
    observer.observe(loadMore.current);
    return () => observer.disconnect();
  }, [visible, busy, error, listing, order, showHidden]);
  return (
    <section
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel files-panel"
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!busy) void browse(path);
        }}
        className="plugin-toolbar"
      >
        <input aria-label="Folder" value={path} onChange={(event) => setPath(event.target.value)} />
        <select
          aria-label="Sort order (folders first)"
          value={order}
          disabled={busy}
          onChange={(event) => {
            setOrder(event.target.value);
            void browse(listing?.folder, 0, event.target.value);
          }}
        >
          <option value="nameAscending">Name A–Z</option>
          <option value="nameDescending">Name Z–A</option>
          <option value="dateDescending">Newest</option>
          <option value="dateAscending">Oldest</option>
          <option value="sizeAscending">Smallest</option>
          <option value="sizeDescending">Largest</option>
        </select>
        <button disabled={busy}>Go</button>
        <button type="button" disabled={busy} onClick={() => void browse(listing?.folder)}>
          Refresh
        </button>
      </form>
      <div className="files-content" ref={content}>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="file-list-heading" aria-hidden="true">
          <span>Name</span>
          <span>Kind</span>
        </div>
        <ul className="file-list" aria-label="Folder contents" aria-busy={busy}>
          {listing?.parent && (
            <li>
              <button
                type="button"
                className="file-row"
                aria-label="Parent folder"
                disabled={busy}
                onClick={() => void browse(listing.parent!)}
              >
                <span className="file-icon" aria-hidden="true">
                  ↑
                </span>
                <span className="file-name">../</span>
              </button>
            </li>
          )}
          {listing?.entries.map((entry) => (
            <li key={entry.path}>
              <button
                type="button"
                className="file-row"
                disabled={busy}
                onClick={() => {
                  if (entry.directory) void browse(entry.path);
                  else
                    void invoke('open_plugin_file', { id: tab.id, path: entry.path }).catch(
                      (error) => setError(String(error)),
                    );
                }}
              >
                <svg
                  className={`file-icon ${entry.directory ? 'folder-icon' : ''}`}
                  width="20"
                  height="20"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  {entry.directory ? (
                    <path d="M3 7V5a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7Zm0 1h18" />
                  ) : (
                    <>
                      <path d="M14 3H5v18h14V8l-5-5Z" />
                      <path d="M14 3v5h5M8 12h8M8 16h6" />
                    </>
                  )}
                </svg>
                <span className="file-name">{entry.name}</span>
                {entry.symlink && (
                  <span className="file-symlink" aria-label="Symbolic link">
                    ↗
                  </span>
                )}
                <span className="file-kind">
                  {entry.directory ? 'Folder' : 'File'}
                  {entry.symlink ? ' link' : ''}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {listing && !busy && listing.entries.length === 0 && (
          <p className="muted file-list-empty">No entries in this folder.</p>
        )}
        <div ref={loadMore} className="files-load-more">
          {busy && <p role="status">Loading files…</p>}
          {error && listing?.nextOffset != null && (
            <button onClick={() => void browse(listing.folder, listing.nextOffset!)}>
              Retry loading
            </button>
          )}
        </div>
        <p className="muted">
          Browsing is read-only. Opening a file launches its associated application. Folders appear
          first. Refresh restarts the listing. Hidden files are {showHidden ? 'shown' : 'hidden'}{' '}
          (Ctrl+H to toggle).
        </p>
      </div>
    </section>
  );
}
