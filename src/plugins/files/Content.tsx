import { t } from '../../i18n';
import { Button, Input, Select, TabPanel } from '../../components/Controls';
import { useEffect, useRef, useState } from 'react';
import {
  useSdk,
  useSessionSettings,
  useTabInteractions,
  SdkError,
  type Listing,
  type SortOrder,
} from '../../sdk';
import { browserDefaults, type BrowserPreferences } from './session-settings';
import { TextEditor, type TextDocument } from './TextEditor';
import { readDraft } from './editor-drafts';
import { FileSidebar } from '../../components/FileSidebar';
import '../../components/file-tree.css';
import { FileBranch } from './FileBranch';
import { FileSearch } from './FileSearch';
import { FileDetails, editorLimitation, type UnsupportedFile } from './FileDetails';
import { fileEditors } from './editor-guards';
import type { PluginContentProps } from '../types';
export function FilesContent({ tab, session, visible }: PluginContentProps) {
  const sdk = useSdk();
  const config = tab.config as {
    folder?: string;
    editorFile?: string | null;
    browserPreferences?: BrowserPreferences | null;
  } | null;
  const shared = useSessionSettings(sdk, browserDefaults);
  const preferences = { ...shared.value, ...config?.browserPreferences };
  const { showHidden, order } = preferences;
  function updatePreferences(patch: Partial<BrowserPreferences>) {
    const saving = config?.browserPreferences
      ? sdk.storage.configure({ browserPreferences: { ...preferences, ...patch } })
      : shared.patch(patch);
    void saving.catch((error) => setError(String(error)));
  }
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
    actions: {
      'open-path': (args, { signal }) => openFile(args.path as string, signal),
    },
  });
  const [search, setSearch] = useState('');
  const [searchBusy, setSearchBusy] = useState(false);
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
        query: '',
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
    if (shared.loading) return;
    ++generation.current;
    pending.current = true;
    setBusy(true);
    const timer = window.setTimeout(() => void browse(), 150);
    return () => {
      window.clearTimeout(timer);
      ++generation.current;
    };
  }, [tab.id, configuredFolder, shared.loading, showHidden, order]);
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
        updatePreferences({ showHidden: !showHidden });
      }
    };
    window.addEventListener('keydown', toggleHidden);
    return () => window.removeEventListener('keydown', toggleHidden);
  }, [
    visible,
    showHidden,
    listing?.folder,
    order,
    tab.id,
    search,
    editing,
    restoringEditor,
    config?.browserPreferences,
    shared.patch,
  ]);
  useEffect(() => {
    if (
      !visible ||
      !!search ||
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
  async function openFile(path: string, signal?: AbortSignal) {
    const checkCancelled = () => {
      if (signal?.aborted) throw new SdkError('cancelled', 'Opening file cancelled');
    };
    checkCancelled();
    if (opening.current || restoringEditor)
      throw new SdkError('busy', 'A file is already opening or restoring');
    if (selectedPath === path) return { path, view: editing ? 'editor' : 'details' };
    const checkEditor = () => {
      const current = fileEditors.get(tab.id);
      if (current && (current.busy || !current.canSwitch()))
        throw new SdkError('busy', 'Save or close the current file before opening another file.');
    };
    checkEditor();
    opening.current = true;
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
      checkCancelled();
      checkEditor();
      await sdk.storage.configure({ editorFile: document?.path ?? path });
      checkCancelled();
      setRestoreError('');
      setEditing(document ?? undefined);
      setDetails(document ? undefined : { path, reason });
      return { path: document?.path ?? path, view: document ? 'editor' : 'details' };
    } finally {
      setBusy(false);
      opening.current = false;
    }
  }
  return (
    <TabPanel
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
            {shared.error && (
              <p className="error" role="alert">
                {shared.error}
              </p>
            )}
            {restoringEditor && <p role="status">{t('Reopening text file…')}</p>}
            {restoreError && (
              <div className="error" role="alert">
                <p>{restoreError}</p>
                <Button
                  onClick={() =>
                    void leaveEditor().catch((error) => setRestoreError(String(error)))
                  }
                >
                  {t('Keep browsing')}
                </Button>
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
                <Input
                  aria-label={t('Folder')}
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      if (!busy && !restoringEditor) void browse(path);
                    }
                  }}
                />
                <Button
                  className="files-refresh"
                  type="button"
                  aria-label={t('Refresh folder')}
                  title={t('Refresh folder')}
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
                </Button>
              </div>
              <div className="files-search-field" data-loading={!!search && searchBusy}>
                <Input
                  className="files-search"
                  type="search"
                  aria-label={t('Search filenames and paths in all subfolders')}
                  placeholder={t('Search all files by name or path…')}
                  disabled={restoringEditor || opening.current}
                  maxLength={256}
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') event.preventDefault();
                  }}
                />
                {search && searchBusy && (
                  <span
                    className="files-search-spinner"
                    role="status"
                    aria-label={t('Searching files')}
                  />
                )}
              </div>
              <Select
                aria-label={t('Sort order (folders first)')}
                value={order}
                disabled={busy || restoringEditor}
                onChange={(event) => {
                  updatePreferences({ order: event.target.value as SortOrder });
                }}
              >
                <option value="nameAscending">{t('Name A–Z')}</option>
                <option value="nameDescending">{t('Name Z–A')}</option>
                <option value="dateDescending">{t('Newest')}</option>
                <option value="dateAscending">{t('Oldest')}</option>
                <option value="sizeAscending">{t('Smallest')}</option>
                <option value="sizeDescending">{t('Largest')}</option>
              </Select>
            </form>

            <div className="files-content file-tree" ref={content}>
              {error && (
                <p role="alert" className="error">
                  {error}
                </p>
              )}
              {listing && (
                <FileSearch
                  refresh={listing}
                  onBusyChange={setSearchBusy}
                  folder={listing.folder}
                  query={search}
                  showHidden={showHidden}
                  selected={selectedPath}
                  disabled={restoringEditor || busy}
                  openFile={openFile}
                />
              )}
              {!search && (
                <ul className="file-list" aria-label={t('Folder contents')} aria-busy={busy}>
                  {listing?.entries.map((entry) => (
                    <FileBranch
                      key={entry.path}
                      entry={entry}
                      selected={selectedPath}
                      openFile={async (path) => {
                        try {
                          await openFile(path);
                        } catch (error) {
                          setError(String(error));
                        }
                      }}
                      showHidden={showHidden}
                      order={order}
                      search={search}
                      refresh={listing}
                      disabled={restoringEditor}
                    />
                  ))}
                </ul>
              )}
              {!search && listing && !busy && listing.entries.length === 0 && (
                <p className="muted file-list-empty">{t('No entries in this folder.')}</p>
              )}
              <div ref={loadMore} className="files-load-more">
                {busy && <p role="status">{t('Loading files…')}</p>}
                {error && listing?.nextOffset != null && (
                  <Button onClick={() => void browse(listing.folder, listing.nextOffset!)}>
                    {t('Retry loading')}
                  </Button>
                )}
              </div>
            </div>
            <footer className="plugin-statusbar" aria-label={t('Files status')}>
              <span>
                {listing
                  ? t('{value0} entries loaded{value1}', {
                      value0: listing.entries.length,
                      value1: listing.nextOffset != null ? t(' · More available') : '',
                    })
                  : t('No folder loaded')}
                {busy ? t(' · Loading…') : error ? t(' · Refresh failed') : ''}
              </span>
              <span>
                {t('Ctrl+H: ')}
                {showHidden ? t('hide') : t('show')}
                {t(' hidden files')}
              </span>
            </footer>
          </div>
        </FileSidebar>
        <div className="files-editor-pane">
          {editing ? (
            <TextEditor
              key={editing.path}
              id={tab.id}
              sessionId={session.id}
              document={editing}
              visible={visible}
              back={leaveEditor}
            />
          ) : details ? (
            <FileDetails key={details.path} file={details} close={leaveEditor} />
          ) : (
            <p className="muted files-editor-empty">
              {t('Select a file from the explorer to open it.')}
            </p>
          )}
        </div>
      </div>
    </TabPanel>
  );
}
