import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PluginContentProps } from '../types';
type Listing = { folder: string; parent: string | null; entries: { name: string; path: string; directory: boolean; symlink: boolean }[]; nextOffset: number | null };
export function FilesContent({ tab, visible }: PluginContentProps) {
  const configuredFolder = (tab.config as { folder?: string } | null)?.folder;
  const [listing, setListing] = useState<Listing>();
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  async function browse(folder?: string, offset = 0) {
    const request = ++generation.current;
    setBusy(true); setError('');
    try {
      const result = await invoke<Listing>('list_plugin_files', { id: tab.id, folder: folder ?? null, offset });
      if (request === generation.current) { setListing(result); setPath(result.folder); }
    } catch (error) { if (request === generation.current) setError(String(error)); }
    finally { if (request === generation.current) setBusy(false); }
  }
  useEffect(() => { void browse(); return () => { ++generation.current; }; }, [tab.id, configuredFolder]);
  return <section role="tabpanel" id={`tab-panel-${tab.id}`} aria-labelledby={`tab-${tab.id}`} hidden={!visible} className="plugin-panel">
    <form onSubmit={event => { event.preventDefault(); if (!busy) void browse(path); }} className="plugin-toolbar">
      <button type="button" disabled={busy || !listing?.parent} onClick={() => void browse(listing!.parent!)}>Parent</button>
      <input aria-label="Folder" value={path} onChange={event => setPath(event.target.value)} />
      <button disabled={busy}>Go</button><button type="button" disabled={busy} onClick={() => void browse(listing?.folder)}>Refresh</button>
    </form>
    {error && <p role="alert" className="error">{error}</p>}
    {busy && <p role="status">Loading folder…</p>}
    <ul className="file-list">{listing?.entries.map(entry => <li key={entry.path}><button disabled={busy} onClick={() => {
      if (entry.directory) void browse(entry.path);
      else void invoke('open_plugin_file', { id: tab.id, path: entry.path }).catch(error => setError(String(error)));
    }}>{entry.directory ? '▸ ' : ''}{entry.name}{entry.symlink ? ' ↗' : ''}</button></li>)}</ul>
    <div className="plugin-toolbar"><button disabled={busy || !listing} onClick={() => void browse(listing!.folder)}>First page</button>
      <button disabled={busy || listing?.nextOffset == null} onClick={() => void browse(listing!.folder, listing!.nextOffset!)}>Next page</button></div>
    <p className="muted">Browsing is read-only. Opening a file launches its associated application. Pages use filesystem order; refresh restarts the listing.</p>
  </section>;
}
