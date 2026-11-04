import { useEffect, useRef, useState } from 'react';
import { useSdk, useTabInteractions, type Listing, type SortOrder } from '../../sdk';
import { TextEditor, type TextDocument } from './TextEditor';
import { readDraft } from './editor-drafts';
import { FileSidebar } from '../../components/FileSidebar';
import '../../components/file-tree.css';
import { FileBranch } from './FileBranch';
import { FileDetails, editorLimitation, type UnsupportedFile } from './FileDetails';
import { fileEditors } from './editor-guards';
import type { PluginContentProps } from '../types';
export function FilesContent({ tab, visible }: PluginContentProps) {
  const sdk = useSdk();
  const config = tab.config as { folder?: string; editorFile?: string | null } | null;
  const configuredFolder = config?.folder;
  const [restoringEditor, setRestoringEditor] = useState(!!config?.editorFile);
  const [restoreError, setRestoreError] = useState('');
  const [editing, setEditing] = useState<TextDocument>();
  const [details, setDetails] = useState<UnsupportedFile>();
  const selectedPath = editing?.path ?? details?.path;
  useEffect(() => {
    sdk.tab.setTitle(selectedPath ? selectedPath.split('/').pop()! : null);
  }, [sdk, selectedPath]);
  const opening = useRef(false);
  const [listing, setListing] = useState<Listing>();
  const [path, setPath] = useState(configuredFolder ?? '');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useTabInteractions({
    state: () => {
      const editor = fileEditors.get(tab.id);
      return {
        summary: selectedPath ? `File ${selectedPath.slice(0, 512)}` : 'File browser',
        data: {
          folder: listing?.folder ?? configuredFolder ?? null,
          selectedFile: selectedPath ?? null,
          dirty: editor?.dirty ?? false,
          conflict: editor?.conflict ?? false,
          loading: busy || restoringEditor || !!editor?.busy,
          error: (error || restoreError).slice(0, 4096) || null,
        },
        observedAt: new Date().toISOString(),
        stale: busy || restoringEditor || !!error || !!restoreError,
      };
    },
  });
  const [showHidden, setShowHidden] = useState(false);
  const [search, setSearch] = useState('');
  const [order, setOrder] = useState<SortOrder>('nameAscending');
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
      const result = await sdk.fs.list({
        folder: folder ?? listing?.folder ?? configuredFolder ?? path,
        offset,
        order: sortOrder,
        showHidden: includeHidden,
        query: search,
      });
      if (request === generation.current) {
        if (offset === 0) await sdk.storage.configure({ folder: result.folder });
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
    ++generation.current;
    pending.current = true;
    setBusy(true);
    const timer = window.setTimeout(() => void browse(), 150);
    return () => {
      window.clearTimeout(timer);
      ++generation.current;
    };
  }, [tab.id, configuredFolder, search]);
  useEffect(() => {
    const file = config?.editorFile;
    if (!file) return;
    let cancelled = false;
    setRestoringEditor(true);
    void sdk.fs
      .readText(file)
      .then((document) => {
        if (cancelled) return;
        if (document) setEditing(document);
        else setDetails({ path: file, reason: 'This file is not supported UTF-8 text.' });
      })
      .catch((error) => {
        if (cancelled) return;
        try {
          const draft = readDraft(tab.id, file);
          if (draft) {
            setEditing(draft.baseline);
            return;
          }
        } catch {
          // Retain the disk error and leave inaccessible draft data untouched.
        }
        const reason = editorLimitation(error);
        if (reason) {
          setDetails({ path: file, reason });
          return;
        }
        setRestoreError(`Could not reopen ${file}: ${String(error)}`);
      })
      .finally(() => {
        if (!cancelled) setRestoringEditor(false);
      });
    return () => {
      cancelled = true;
    };
    // Restore once per opening; metadata refreshes must not replace a live editor.
  }, [tab.id]);
  async function leaveEditor() {
    await sdk.storage.configure({ editorFile: null });
    setRestoreError('');
    setEditing(undefined);
    setDetails(undefined);
  }
  useEffect(() => {
    if (!visible || restoringEditor) return;
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
  }, [visible, showHidden, listing?.folder, order, tab.id, search, editing, restoringEditor]);
  useEffect(() => {
    if (
      !visible ||
      restoringEditor ||
      busy ||
      error ||
      listing?.nextOffset == null ||
      !loadMore.current
    )
      return;
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
  }, [visible, busy, error, listing, order, showHidden, search, editing, restoringEditor]);
  async function openFile(path: string) {
    if (opening.current) return;
    opening.current = true;
    if (selectedPath === path) {
      opening.current = false;
      return;
    }
    const current = fileEditors.get(tab.id);
    if (current && (current.busy || !current.canSwitch())) {
      setError('Save or close the current file before opening another file.');
      opening.current = false;
      return;
    }
    setBusy(true);
    setError('');
    try {
      let document: TextDocument | null = null;
      let reason = 'This file is not supported UTF-8 text.';
      try {
        document = await sdk.fs.readText(path);
      } catch (error) {
        const limitation = editorLimitation(error);
        if (!limitation) throw error;
        reason = limitation;
      }
      const latest = fileEditors.get(tab.id);
      if (latest && (latest.busy || !latest.canSwitch())) {
        throw new Error('Save or close the current file before opening another file.');
      }
      await sdk.storage.configure({ editorFile: document?.path ?? path });
      setRestoreError('');
      setEditing(document ?? undefined);
      setDetails(document ? undefined : { path, reason });
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
      opening.current = false;
    }
  }
  return (
    <section
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel files-panel"
    >
      <div className="files-workspace">
        <FileSidebar
          storageKey="ongleet.files.sidebarWidth"
          label="Session file explorer"
          className="files-sidebar"
        >
          <div className="files-browser">
            {restoringEditor && <p role="status">Reopening text file…</p>}
            {restoreError && (
              <div className="error" role="alert">
                <p>{restoreError}</p>
                <button
                  onClick={() =>
                    void leaveEditor().catch((error) => setRestoreError(String(error)))
                  }
                >
                  Keep browsing
                </button>
              </div>
            )}
            <form
              onSubmit={(event) => {
                event.preventDefault();
                if (!busy && !restoringEditor) void browse(path);
              }}
              className="plugin-toolbar"
            >
              <div className="files-path">
                <input
                  aria-label="Folder"
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      if (!busy && !restoringEditor) void browse(path);
                    }
                  }}
                />
                <button
                  className="files-refresh"
                  type="button"
                  aria-label="Refresh folder"
                  title="Refresh folder"
                  disabled={busy || restoringEditor}
                  onClick={() => void browse(listing?.folder)}
                >
                  <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden="true"
                    style={{ display: 'block' }}
                  >
                    <path d="M20 7v5h-5M4 17v-5h5" />
                    <path d="M6.1 6.1A8 8 0 0 1 20 12M4 12a8 8 0 0 0 13.9 5.9" />
                  </svg>
                </button>
              </div>
              <input
                className="files-search"
                type="search"
                aria-label="Search filenames in current folder"
                placeholder="Search files…"
                disabled={restoringEditor || opening.current}
                maxLength={1024}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') event.preventDefault();
                }}
              />
              <select
                aria-label="Sort order (folders first)"
                value={order}
                disabled={busy || restoringEditor}
                onChange={(event) => {
                  setOrder(event.target.value as SortOrder);
                  void browse(listing?.folder, 0, event.target.value as SortOrder);
                }}
              >
                <option value="nameAscending">Name A–Z</option>
                <option value="nameDescending">Name Z–A</option>
                <option value="dateDescending">Newest</option>
                <option value="dateAscending">Oldest</option>
                <option value="sizeAscending">Smallest</option>
                <option value="sizeDescending">Largest</option>
              </select>
            </form>

            <div className="files-content file-tree" ref={content}>
              {error && (
                <p role="alert" className="error">
                  {error}
                </p>
              )}
              <ul className="file-list" aria-label="Folder contents" aria-busy={busy}>
                {listing?.entries.map((entry) => (
                  <FileBranch
                    key={entry.path}
                    entry={entry}
                    selected={selectedPath}
                    openFile={openFile}
                    showHidden={showHidden}
                    order={order}
                    search={search}
                    refresh={listing}
                    disabled={restoringEditor}
                  />
                ))}
              </ul>
              {listing && !busy && listing.entries.length === 0 && (
                <p className="muted file-list-empty">
                  {search ? 'No matching files.' : 'No entries in this folder.'}
                </p>
              )}
              <div ref={loadMore} className="files-load-more">
                {busy && <p role="status">Loading files…</p>}
                {error && listing?.nextOffset != null && (
                  <button onClick={() => void browse(listing.folder, listing.nextOffset!)}>
                    Retry loading
                  </button>
                )}
              </div>
            </div>
            <footer className="plugin-statusbar" aria-label="Files status">
              <span>
                {listing
                  ? `${listing.entries.length} entries loaded${listing.nextOffset != null ? ' · More available' : ''}`
                  : 'No folder loaded'}
                {busy ? ' · Loading…' : error ? ' · Refresh failed' : ''}
              </span>
              <span>Ctrl+H: {showHidden ? 'hide' : 'show'} hidden files</span>
            </footer>
          </div>
        </FileSidebar>
        <div className="files-editor-pane">
          {editing ? (
            <TextEditor
              key={editing.path}
              id={tab.id}
              document={editing}
              visible={visible}
              back={leaveEditor}
            />
          ) : details ? (
            <FileDetails key={details.path} file={details} close={leaveEditor} />
          ) : (
            <p className="muted files-editor-empty">Select a file from the explorer to open it.</p>
          )}
        </div>
      </div>
    </section>
  );
}
