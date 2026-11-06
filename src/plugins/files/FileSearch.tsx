import { useEffect, useRef, useState } from 'react';
import { Button } from '../../components/Controls';
import { useSdk, type FileSearch as SearchResults } from '../../sdk';

export function FileSearch({
  folder,
  query,
  showHidden,
  selected,
  disabled,
  openFile,
  refresh,
  onBusyChange,
}: {
  refresh: object;
  onBusyChange: (busy: boolean) => void;
  folder: string;
  query: string;
  showHidden: boolean;
  selected?: string;
  disabled: boolean;
  openFile: (path: string) => Promise<unknown>;
}) {
  const sdk = useSdk();
  const [result, setResult] = useState<SearchResults>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const queue = useRef(Promise.resolve());
  useEffect(() => {
    onBusyChange(busy);
    return () => onBusyChange(false);
  }, [busy, onBusyChange]);
  useEffect(() => {
    let cancelled = false;
    setError('');
    setBusy(!!query);
    if (!query) {
      setResult(undefined);
      return;
    }
    const timer = window.setTimeout(() => {
      queue.current = queue.current.then(async () => {
        if (cancelled) return;
        try {
          const found = await sdk.fs.searchFiles({ folder, query, showHidden });
          if (!cancelled) setResult(found);
        } catch (error) {
          if (!cancelled) setError(String(error));
        } finally {
          if (!cancelled) setBusy(false);
        }
      });
    }, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [sdk, folder, query, showHidden, refresh]);
  if (!query) return null;
  return (
    <div className="files-search-results" aria-label="File search results" aria-busy={busy}>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {result && (
        <p role="status" className="muted">
          {result.matches.length} matching files
          {result.truncated ? ' · Search limit reached; narrow your search' : ''}
          {result.skipped > 0 ? ` · ${result.skipped} entries could not be searched` : ''}
        </p>
      )}
      <ul className="file-list">
        {result?.matches.map((match) => (
          <li key={match.path}>
            <Button
              type="button"
              disabled={disabled}
              aria-pressed={selected === match.path}
              title={match.path}
              onClick={() => void openFile(match.path).catch((error) => setError(String(error)))}
            >
              <span className="file-tree-label">{match.relativePath}</span>
            </Button>
          </li>
        ))}
      </ul>
    </div>
  );
}
