import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { parseDiff, Diff, Hunk, Decoration } from 'react-diff-view';
import 'react-diff-view/style/index.css';
import './diff.css';
import { useSdk } from '../../sdk';
import type { PluginContentProps } from '../types';
import { ComparisonFields } from './ConfigurationEditor';
import { defaults, type Settings } from './settings';
import { GitComparison, Lane, type Comparison, type FileChange } from './git';
function Viewer({ patch }: { patch: string }) {
  try {
    if (!patch) return <p>This file changed during the comparison. Refresh to update.</p>;
    const files = parseDiff(patch);
    if (!files.length || (patch.includes('\n@@ ') && !files.some((file) => file.hunks.length)))
      throw new Error('Patch parser returned no text hunks.');
    const metadata = patch
      .split('\n')
      .filter((line) =>
        /^(old mode|new mode|new file mode|deleted file mode|similarity index|rename from|rename to)/.test(
          line,
        ),
      )
      .join('\n');
    return (
      <>
        {metadata && <pre>{metadata}</pre>}
        {files.map((file, i) =>
          file.hunks.length ? (
            <Diff key={i} viewType="unified" diffType={file.type} hunks={file.hunks}>
              {(hunks) =>
                hunks.map((hunk) => (
                  <Fragment key={hunk.content}>
                    <Decoration>{hunk.content}</Decoration>
                    <Hunk hunk={hunk} />
                  </Fragment>
                ))
              }
            </Diff>
          ) : (
            <div key={i}>
              <p>
                {file.isBinary
                  ? 'Binary file changed.'
                  : 'Metadata-only change (permissions, rename, symlink or submodule).'}
              </p>
              <pre>{patch}</pre>
            </div>
          ),
        )}
      </>
    );
  } catch (error) {
    return <p role="alert">Cannot render patch: {String(error)}</p>;
  }
}
export function DiffContent({ tab, visible }: PluginContentProps) {
  const sdk = useSdk();
  const scopedGit = (current: () => boolean) =>
    new GitComparison((request) => {
      if (!current()) throw new Error('Comparison cancelled.');
      return sdk.process.run(request);
    });
  const lane = useMemo(() => new Lane(sdk.process.cancel), [sdk]);
  const [config, setConfig] = useState<Settings>(() => ({
    ...defaults,
    ...(tab.config as Partial<Settings>),
  }));
  const pendingConfiguration = useRef('');
  const observedConfiguration = useRef(JSON.stringify(config));
  const configRef = useRef(config);
  configRef.current = config;
  const [comparison, setComparison] = useState<Comparison>();
  const comparisonRef = useRef(comparison);
  comparisonRef.current = comparison;
  const [selected, setSelected] = useState('');
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const [patch, setPatch] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState('');
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    const next = { ...defaults, ...(tab.config as Partial<Settings>) };
    const key = JSON.stringify(next);
    if (key === observedConfiguration.current) return;
    observedConfiguration.current = key;
    if (JSON.stringify(next) === pendingConfiguration.current) {
      pendingConfiguration.current = '';
      return;
    }
    if (JSON.stringify(next) !== JSON.stringify(configRef.current)) {
      setConfig(next);
      configRef.current = next;
      if (visible) {
        clearTimeout(timer.current);
        timer.current = setTimeout(() => refreshRef.current(), 100);
      }
    }
  }, [tab.config]);
  const refresh = () => {
    setLoading(true);
    setError('');
    void lane
      .replace(async (current) => {
        try {
          const git = scopedGit(current);
          const next = await git.compare(configRef.current);
          if (!current()) return;
          const file = next.files.find((f) => f.key === selectedRef.current) ?? next.files[0];
          const text = file ? await git.patch(next, file) : '';
          if (!current()) return;
          pendingConfiguration.current = JSON.stringify(next.config);
          await sdk.storage.configure(next.config);
          if (!current()) return;
          setComparison(next);
          setSelected(file?.key ?? '');
          setPatch(text);
        } catch (e) {
          if (current()) setError(String(e));
        } finally {
          if (current()) setLoading(false);
        }
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  };
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  useEffect(() => {
    if (!visible) {
      void lane.stop();
      setLoading(false);
      return;
    }
    const schedule = () => {
      clearTimeout(timer.current);
      timer.current = setTimeout(() => refreshRef.current(), 100);
    };
    schedule();
    window.addEventListener('focus', schedule);
    return () => {
      clearTimeout(timer.current);
      window.removeEventListener('focus', schedule);
      void lane.stop();
    };
  }, [visible, lane]);
  function select(file: FileChange) {
    const c = comparisonRef.current;
    if (!c) return;
    setLoading(true);
    setError('');
    setSelected(file.key);
    setPatch('');
    void lane.replace(async (current) => {
      try {
        const text = await scopedGit(current).patch(c, file);
        if (current()) setPatch(text);
      } catch (e) {
        if (current()) setError(String(e));
      } finally {
        if (current()) setLoading(false);
      }
    });
  }
  const labels: Record<string, string> = {
    A: 'Added',
    M: 'Modified',
    D: 'Deleted',
    R: 'Renamed',
    T: 'Type changed',
    C: 'Copied',
    U: 'Unmerged',
  };
  return (
    <section
      hidden={!visible}
      className="plugin-panel diff-panel"
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
    >
      <form
        className="diff-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          refresh();
        }}
      >
        <ComparisonFields config={config} onChange={setConfig} />
        <button type="submit">Compare</button>
        <button type="button" onClick={refresh}>
          Refresh
        </button>
      </form>
      {comparison && (
        <div className="diff-summary">
          {comparison.root} ·{' '}
          {
            { workingDirectory: 'Working directory', refs: 'Two refs', mergeBase: 'Merge base' }[
              comparison.config.mode
            ]
          }{' '}
          · {comparison.config.baseRef} ({comparison.base.slice(0, 12)}) →{' '}
          {comparison.target
            ? `${comparison.config.targetRef} (${comparison.target.slice(0, 12)})`
            : 'Working directory'}{' '}
          · Last successful refresh {new Date(comparison.time).toLocaleTimeString()}
          {error && ' · Stale'}
        </div>
      )}
      {loading && <p role="status">Loading comparison…</p>}
      {error && (
        <p role="alert">
          {error} Check the folder, Git installation and revision expressions, then Compare again.
        </p>
      )}
      {comparison && (
        <div className="diff-layout">
          <aside>
            <label>
              Filter filenames
              <input value={filter} onChange={(e) => setFilter(e.target.value)} />
            </label>
            <nav aria-label="Changed files">
              {comparison.files
                .filter((f) =>
                  `${f.oldPath}\n${f.path}`.toLowerCase().includes(filter.toLowerCase()),
                )
                .map((file) => (
                  <button
                    key={file.key}
                    aria-pressed={selected === file.key}
                    onClick={() => select(file)}
                  >
                    {labels[file.status[0]]}:{' '}
                    {file.oldPath !== file.path ? `${file.oldPath} → ` : ''}
                    {file.path}
                  </button>
                ))}
            </nav>
            {!comparison.files.length && <p>No tracked changes.</p>}
          </aside>
          <div className="diff-viewer" tabIndex={0} aria-label="Unified diff">
            {patch && <Viewer patch={patch} />}
            {!patch && !loading && selected && (
              <p>This file changed or has no patch. Refresh to update.</p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
