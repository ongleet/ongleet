import { useEffect, useRef, useState } from 'react';
import { FileIcon } from '../../components/FileIcon';
import { FolderIcon } from '../../components/FolderIcon';
import { useSdk, type Listing, type SortOrder } from '../../sdk';

type Props = {
  entry: Listing['entries'][number];
  selected?: string;
  openFile: (path: string) => Promise<void>;
  showHidden: boolean;
  order: SortOrder;
  search: string;
  refresh: Listing;
  disabled: boolean;
};

export function FileBranch(props: Props) {
  const { entry, selected, openFile, showHidden, order, search, refresh, disabled } = props;
  const sdk = useSdk();
  const [expanded, setExpanded] = useState(!!selected?.startsWith(entry.path + '/'));
  const [listing, setListing] = useState<Listing>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const generation = useRef(0);
  async function load(offset = 0) {
    const request = ++generation.current;
    setBusy(true);
    setError('');
    try {
      const result = await sdk.fs.list({
        folder: entry.path,
        offset,
        order,
        showHidden,
        query: search,
      });
      if (request !== generation.current) return;
      setListing((previous) =>
        offset && previous
          ? {
              ...result,
              entries: [
                ...previous.entries,
                ...result.entries.filter(
                  (item) => !previous.entries.some((old) => old.path === item.path),
                ),
              ],
            }
          : result,
      );
    } catch (error) {
      if (request === generation.current) setError(String(error));
    } finally {
      if (request === generation.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (expanded) void load();
    return () => {
      ++generation.current;
    };
  }, [expanded, entry.path, showHidden, order, search, refresh]);
  return (
    <li>
      {entry.directory ? (
        <details open={expanded} onToggle={(event) => setExpanded(event.currentTarget.open)}>
          <summary title={entry.path}>
            <FolderIcon />
            <span className="file-tree-label">
              {entry.name}
              {entry.symlink ? ' ↗' : ''}
            </span>
          </summary>
          <ul className="file-list">
            {listing?.entries.map((child) => (
              <FileBranch {...props} key={child.path} entry={child} />
            ))}
          </ul>
          {busy && <p role="status">Loading files…</p>}
          {error && (
            <p role="alert" className="error">
              {error}
            </p>
          )}
          {!busy && (!listing || listing.nextOffset != null || error) && (
            <button type="button" onClick={() => void load(error ? 0 : (listing?.nextOffset ?? 0))}>
              {error ? 'Retry loading' : 'Load more'}
            </button>
          )}
          {!busy && listing?.entries.length === 0 && (
            <p className="muted file-list-empty">No entries in this folder.</p>
          )}
        </details>
      ) : (
        <button
          type="button"
          title={entry.path}
          disabled={disabled}
          aria-current={selected === entry.path ? 'true' : undefined}
          onClick={() => void openFile(entry.path)}
        >
          <FileIcon />
          <span className="file-tree-label">{entry.name}</span>
          {entry.symlink && <span aria-label="Symbolic link">↗</span>}
        </button>
      )}
    </li>
  );
}
