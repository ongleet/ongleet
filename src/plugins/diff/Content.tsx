import { memo, Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { parseDiff, Diff, Hunk, Decoration } from 'react-diff-view';
import 'react-diff-view/style/index.css';
import './diff.css';
import { useSdk } from '../../sdk';
import type { PluginContentProps } from '../types';
import { updateDiffTag } from './status-tag';
import { FileReview } from './FileReview';
import { buildFileTree, filesInTreeOrder, type FileTreeNode } from './file-tree';
import { DiffSidebar } from './Sidebar';
import { ComparisonFields } from './ConfigurationEditor';
import { defaults, type Settings } from './settings';
import { GitComparison, Lane, type Comparison, type FileChange } from './git';
const Viewer = memo(function Viewer({ patch }: { patch: string }) {
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
});

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
  useEffect(() => {
    sdk.tab.setTitle(comparison ? `${tab.name} (${comparison.files.length})` : null);
  }, [sdk, tab.name, comparison?.files.length]);
  const [selected, setSelected] = useState('');
  const [patches, setPatches] = useState<Record<string, { text?: string; error?: string }>>({});
  const [references, setReferences] = useState<string[]>([]);
  const fileElements = useRef(new Map<string, HTMLElement>());
  const viewer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const container = viewer.current;
    if (!visible || !comparison || !container) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const top = container.getBoundingClientRect().top + 4;
      const current = comparison.files.find((file) => {
        const element = fileElements.current.get(file.key);
        return element && element.getBoundingClientRect().bottom > top;
      });
      if (current) setSelected(current.key);
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    container.addEventListener('scroll', schedule, { passive: true });
    const observer = new ResizeObserver(schedule);
    observer.observe(container);
    if (container.firstElementChild) observer.observe(container.firstElementChild);
    schedule();
    return () => {
      container.removeEventListener('scroll', schedule);
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [comparison, visible]);
  const [error, setError] = useState('');
  const [tagError, setTagError] = useState('');
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState('');
  const tree = useRef<HTMLElement>(null);
  useEffect(() => {
    const container = tree.current;
    if (!visible || !container) return;
    const row = container.querySelector<HTMLElement>('button[aria-current="true"]');
    if (!row) return;
    // Reveal collapsed ancestors, then scroll only the tree, preserving viewer position.
    for (
      let parent = row.parentElement;
      parent && parent !== container;
      parent = parent.parentElement
    ) {
      if (parent instanceof HTMLDetailsElement) parent.open = true;
    }
    const bounds = container.getBoundingClientRect();
    const item = row.getBoundingClientRect();
    const top = bounds.top + container.clientTop;
    const bottom = top + container.clientHeight;
    if (item.top < top) container.scrollTop += item.top - top;
    else if (item.bottom > bottom) container.scrollTop += item.bottom - bottom;
  }, [selected, filter, visible, comparison]);
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
    setTagError('');
    void lane
      .replace(async (current) => {
        try {
          const git = scopedGit(current);
          const result = await git.compare(configRef.current);
          const next = { ...result, files: filesInTreeOrder(result.files) };
          if (!current()) return;
          pendingConfiguration.current = JSON.stringify(next.config);
          await sdk.storage.configure(next.config);
          if (!current()) return;
          try {
            await updateDiffTag(sdk.session, tab.id, next.files.length, current);
          } catch (error) {
            if (current()) setTagError(String(error));
          }
          if (!current()) return;
          try {
            const refs = await git.references(next.root);
            if (current()) setReferences(refs);
          } catch {
            if (current()) setReferences([]);
          }
          // Stream patches serially, keeping IPC and total retained text bounded.
          const nextPatches: Record<string, { text?: string; error?: string }> = {};
          let bytes = 0;
          for (const file of next.files) {
            if (!current()) return;
            try {
              if (bytes >= 8 * 1024 * 1024)
                throw new Error(
                  'Comparison exceeds the 8 MiB display limit. Narrow the comparison to view more.',
                );
              const text = await git.patch(next, file);
              const size = new TextEncoder().encode(text).length;
              if (bytes + size > 8 * 1024 * 1024)
                throw new Error(
                  'Comparison exceeds the 8 MiB display limit. Narrow the comparison to view more.',
                );
              bytes += size;
              if (!current()) return;
              nextPatches[file.key] = { text };
            } catch (error) {
              if (!current()) return;
              nextPatches[file.key] = { error: String(error) };
            }
          }
          if (!current()) return;
          // Commit together so refresh never collapses the existing scroll surface.
          setComparison(next);
          setPatches(nextPatches);
          setSelected((previous) =>
            next.files.some((file) => file.key === previous) ? previous : '',
          );
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
    setSelected(file.key);
    fileElements.current.get(file.key)?.scrollIntoView({ block: 'start' });
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
        <ComparisonFields config={config} onChange={setConfig} references={references} />
        <button type="submit">Compare</button>
        <button type="button" className="diff-refresh" onClick={refresh}>
          <span className="diff-spinner-slot">
            {loading && (
              <span className="diff-spinner" role="status" aria-label="Loading comparison" />
            )}
          </span>
          Refresh
        </button>
      </form>
      {tagError && <p role="alert">Could not update the session change count: {tagError}</p>}
      {error && (
        <p role="alert">
          {error} Check the folder, Git installation and revision expressions, then Compare again.
        </p>
      )}
      {comparison && (
        <div className="diff-layout">
          <DiffSidebar>
            <label>
              Filter filenames
              <input value={filter} onChange={(e) => setFilter(e.target.value)} />
            </label>
            <nav ref={tree} aria-label="Changed files">
              <FileTree
                files={comparison.files.filter((f) =>
                  `${f.oldPath}\n${f.path}`.toLowerCase().includes(filter.toLowerCase()),
                )}
                selected={selected}
                onSelect={select}
                labels={labels}
              />
            </nav>
            {!comparison.files.length && <p>No tracked changes.</p>}
          </DiffSidebar>
          <div ref={viewer} className="diff-viewer" tabIndex={0} aria-label="Unified diff">
            <div className="diff-page">
              {comparison.files.map((file) => (
                <article
                  className="diff-file"
                  key={JSON.stringify([
                    comparison.root,
                    comparison.config.mode,
                    comparison.config.baseRef,
                    comparison.config.targetRef,
                    file.key,
                  ])}
                  ref={(element) => {
                    if (element) fileElements.current.set(file.key, element);
                    else fileElements.current.delete(file.key);
                  }}
                >
                  <FileReview
                    path={file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path}
                    status={labels[file.status[0]]}
                    patch={patches[file.key]?.text}
                    failed={!!patches[file.key]?.error}
                  >
                    {patches[file.key]?.error ? (
                      <p role="alert">{patches[file.key].error}</p>
                    ) : patches[file.key]?.text !== undefined ? (
                      <Viewer patch={patches[file.key].text!} />
                    ) : (
                      <p>{loading ? 'Loading patch…' : 'Patch unavailable. Refresh to retry.'}</p>
                    )}
                  </FileReview>
                </article>
              ))}
              {!comparison.files.length && <p>No tracked changes.</p>}
            </div>
          </div>
        </div>
      )}
      {comparison && (
        <footer className="plugin-statusbar diff-statusbar" aria-label="Diff status">
          <div className="diff-summary">
            {
              {
                auto: 'Automatic',
                workingDirectory: 'Working directory',
                refs: 'Two refs',
                mergeBase: 'Merge base',
              }[comparison.config.mode]
            }{' '}
            · {comparison.baseLabel} ({comparison.base.slice(0, 12)}) →{' '}
            {comparison.target
              ? `${comparison.targetLabel} (${comparison.target.slice(0, 12)})`
              : 'Working directory'}{' '}
            · Last successful refresh {new Date(comparison.time).toLocaleTimeString()}
            {error && ' · Stale'}
          </div>
          <span>
            {comparison.files.length} changed files ·{' '}
            {
              comparison.files.filter((file) =>
                `${file.oldPath}\n${file.path}`.toLowerCase().includes(filter.toLowerCase()),
              ).length
            }{' '}
            matching filter
          </span>
          {(loading || error) && (
            <span>
              {loading ? 'Loading comparison…' : 'Refresh failed · Showing previous comparison'}
            </span>
          )}
        </footer>
      )}
    </section>
  );
}

function FileTree({
  files,
  selected,
  onSelect,
  labels,
}: {
  files: FileChange[];
  selected: string;
  onSelect: (file: FileChange) => void;
  labels: Record<string, string>;
}) {
  const root = buildFileTree(files);
  const render = (node: FileTreeNode) => (
    <ul>
      {[...node.directories]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, child]) => (
          <li key={name}>
            <details open>
              <summary title={name}>
                <span className="diff-tree-label">▱ {name}</span>
              </summary>
              {render(child)}
            </details>
          </li>
        ))}
      {node.files.map((file) => (
        <li key={file.key}>
          <button
            title={file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path}
            aria-pressed={selected === file.key}
            aria-current={selected === file.key ? 'true' : undefined}
            onClick={() => onSelect(file)}
          >
            <span className="diff-status" aria-label={labels[file.status[0]]}>
              {file.status[0]}
            </span>
            <span className="diff-tree-label">{file.path.split('/').at(-1)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
  return render(root);
}
