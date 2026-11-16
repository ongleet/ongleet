import { diffDiagnostic, useDiffRenderDiagnostic } from './render-diagnostics';
import { t, getLocale } from '../../../i18n';
import { Button, Disclosure, DisclosureTrigger, Input, TabPanel } from '../../../components/Controls';
import { SelectionSend } from '../../../components/SelectionSend';
import { captureDiffSelection } from './selection';
import { CommentStore, commentTargets, deliverComment, type Location } from './comments';
import { Composer, Discussion } from './Discussions';
import { DiffSelection } from './selection-state';
import { ReviewStore } from './review-state';
import {
  memo,
  createContext,
  useContext,
  Fragment,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { parseDiff, Hunk, Decoration, getChangeKey } from 'react-diff-view';
import 'react-diff-view/style/index.css';
import '../assets/diff.css';
import '../../../components/file-tree.css';
import { useSdk, useTabInteractions, SdkError } from '../../../sdk';
import type { PluginContentProps } from '../../types';
import { updateDiffTag } from './status-tag';
import { FileReview } from './FileReview';
import { buildFileTree, filesInTreeOrder, type FileTreeNode } from './file-tree';
import { FileIcon } from '../../../components/FileIcon';
import { FolderIcon } from '../../../components/FolderIcon';
import { DiffSidebar } from './Sidebar';
import { HighlightedDiff } from './syntax';
import { ComparisonFields } from './ConfigurationEditor';
import { settings, type Settings } from './settings';
import { GitComparison, Lane, type Comparison, type FileChange, type CommitOption } from './git';
function createLoadingState() {
  let value = false;
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => value,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    set: (next: boolean) => {
      if (next === value) return;
      value = next;
      for (const listener of listeners) listener();
    },
  };
}
const DiffLoading = createContext<ReturnType<typeof createLoadingState> | null>(null);
function ExpandContextButton({ expand }: { expand: () => void }) {
  const state = useContext(DiffLoading)!;
  const loading = useSyncExternalStore(state.subscribe, state.getSnapshot);
  return (
    <Button
      type="button"
      className="diff-expand-context"
      aria-label={t('Show 20 more context lines')}
      title={t('Show 20 more context lines')}
      disabled={loading}
      onClick={expand}
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
      >
        <path d="M12 9V3m-3 3 3-3 3 3M12 15v6m-3-3 3 3 3-3M4 12h16" />
      </svg>
    </Button>
  );
}
const Viewer = memo(
  function Viewer({
    patch,
    expand,
    comment,
    threads,
  }: {
    discussionRevision: string;
    threads: { side: 'old' | 'new'; line: number; node: React.ReactNode }[];
    comment: (side: 'old' | 'new', line: number) => void;
    patch: string;
    expand: () => void;
  }) {
    useDiffRenderDiagnostic('Viewer', { patch, threads });
    const parsed = useMemo(() => {
      try {
        return { files: parseDiff(patch), error: null };
      } catch (error) {
        return { files: [], error };
      }
    }, [patch]);
    try {
      if (parsed.error) throw parsed.error;
      if (!patch) return <p>{t('This file changed during the comparison. Refresh to update.')}</p>;
      const files = parsed.files;
      if (!files.length || (patch.includes('\n@@ ') && !files.some((file) => file.hunks.length)))
        throw new Error(t('Patch parser returned no text hunks.'));
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
              <HighlightedDiff
                key={i}
                path={file.type === 'delete' ? file.oldPath : file.newPath}
                widgets={Object.fromEntries(
                  file.hunks.flatMap((hunk) =>
                    hunk.changes.flatMap((change) => {
                      const matching = threads.filter((thread) =>
                        change.type === 'normal'
                          ? (thread.side === 'old'
                              ? change.oldLineNumber
                              : change.newLineNumber) === thread.line
                          : (thread.side === 'old'
                              ? change.type === 'delete'
                              : change.type === 'insert') && change.lineNumber === thread.line,
                      );
                      return matching.length
                        ? [
                            [
                              getChangeKey(change),
                              <>
                                {matching.map((thread, i) => (
                                  <Fragment key={i}>{thread.node}</Fragment>
                                ))}
                              </>,
                            ],
                          ]
                        : [];
                    }),
                  ),
                )}
                renderGutter={({ change, side, renderDefault }) => {
                  if (
                    (change.type === 'insert' && side === 'old') ||
                    (change.type === 'delete' && side === 'new')
                  )
                    return renderDefault();
                  const line =
                    change.type === 'normal'
                      ? side === 'old'
                        ? change.oldLineNumber
                        : change.newLineNumber
                      : change.lineNumber;
                  return (
                    <>
                      <Button
                        className="diff-line-comment"
                        data-comment-side={side}
                        data-comment-line={line}
                        aria-label={t('Comment on {value0} line {value1}', {
                          value0: side,
                          value1: line,
                        })}
                        onClick={() => comment(side, line)}
                      >
                        <span className="diff-line-number">{renderDefault()}</span>
                        <span className="diff-line-comment-icon" aria-hidden="true">
                          <span className="diff-comment-bubble" />
                        </span>
                      </Button>
                    </>
                  );
                }}
                viewType="unified"
                diffType={file.type}
                hunks={file.hunks}
              >
                {(hunks) =>
                  hunks.map((hunk) => (
                    <Fragment key={hunk.content}>
                      <Decoration>
                        <ExpandContextButton expand={expand} />
                        {hunk.content}
                      </Decoration>
                      <Hunk hunk={hunk} />
                    </Fragment>
                  ))
                }
              </HighlightedDiff>
            ) : (
              <div key={i}>
                <p>
                  {file.isBinary
                    ? t('Binary file changed.')
                    : t('Metadata-only change (permissions, rename, symlink or submodule).')}
                </p>
                <pre>{patch}</pre>
              </div>
            ),
          )}
        </>
      );
    } catch (error) {
      return (
        <p role="alert">
          {t('Cannot render patch: ')}
          {String(error)}
        </p>
      );
    }
  },
  (previous, next) =>
    previous.patch === next.patch && previous.discussionRevision === next.discussionRevision,
);

export function DiffContent({ tab, session, visible }: PluginContentProps) {
  const sdk = useSdk();
  const comments = useMemo(() => new CommentStore(session.id, tab.id), [session.id, tab.id]);
  const [, renderComments] = useState(0);
  const [commentError, setCommentError] = useState('');
  const [compose, setCompose] = useState<Location>();
  const composeRevision = useRef('');
  const changeComments = <T,>(operation: () => T): T => {
    try {
      const value = operation();
      renderComments((v) => v + 1);
      setCommentError('');
      return value;
    } catch (error) {
      setCommentError(String(error));
      throw error;
    }
  };
  const targets = () => commentTargets(sdk.tabs, session.id, tab.id);
  const sendComment = (threadId: string, commentId: string, targetTabId: number) =>
    deliverComment(sdk.tabs, session.id, tab.id, comments.get(threadId), commentId, targetTabId);
  const discussion = (thread: import('./comments').Thread) => (
    <Discussion
      key={thread.id}
      thread={thread}
      targets={targets}
      send={(commentId, target) => sendComment(thread.id, commentId, target)}
      reply={(text) => {
        changeComments(() => comments.reply(thread.id, text, 'User'));
      }}
      edit={(commentId, text) => {
        changeComments(() => comments.edit(thread.id, commentId, text));
      }}
      remove={() => {
        try {
          changeComments(() => comments.delete(thread.id));
        } catch {}
      }}
    />
  );
  const scopedGit = (current: () => boolean) =>
    new GitComparison((request) => {
      if (!current()) throw new Error(t('Comparison cancelled.'));
      return sdk.process.run(request);
    });
  const lane = useMemo(() => new Lane(sdk.process.cancel), [sdk]);
  const [config, setConfig] = useState<Settings>(() => settings(tab.config));
  const contextLines = useRef<Record<string, number>>({});
  const contextKey = (value: Settings, file: FileChange) =>
    JSON.stringify([
      value.folder,
      value.mode,
      value.baseRef,
      value.targetRef,
      value.compareFrom,
      value.includeUncommitted,
      file.key,
    ]);
  const pendingConfiguration = useRef('');
  const observedConfiguration = useRef(JSON.stringify(config));
  const configRef = useRef(config);
  configRef.current = config;
  const [comparison, setComparison] = useState<Comparison>();
  useEffect(() => {
    sdk.tab.setTitle(comparison ? `${tab.name} (${comparison.files.length})` : null);
  }, [sdk, tab.name, comparison?.files.length]);
  const selection = useMemo(() => new DiffSelection(), []);
  const [patches, setPatches] = useState<Record<string, { text?: string; error?: string }>>({});
  const patchSnapshot = useRef<
    | {
        identity: string;
        contexts: Record<string, number>;
        patches: Record<string, { text?: string; error?: string }>;
      }
    | undefined
  >(undefined);
  const reviews = useRef(new ReviewStore());
  const [, renderReviews] = useState(0);
  const comparisonIdentity = (value: Comparison) =>
    JSON.stringify([value.root, value.config, value.base, value.target]);
  const setReview = (
    paths: string[],
    field: 'viewed' | 'expanded',
    value: boolean,
    revision?: string,
  ) => {
    const result = reviews.current.set(paths, field, value, revision);
    renderReviews((value) => value + 1);
    return result;
  };
  const [references, setReferences] = useState<string[]>([]);
  const [commits, setCommits] = useState<CommitOption[]>([]);
  const [commitError, setCommitError] = useState('');
  const fileElements = useRef(new Map<string, HTMLElement>());
  const viewer = useRef<HTMLDivElement>(null);
  const refreshAnchor = useRef<{ key: string; offset: number; left: number } | null>(null);
  useLayoutEffect(() => {
    const anchor = refreshAnchor.current;
    refreshAnchor.current = null;
    const container = viewer.current;
    const element = anchor && fileElements.current.get(anchor.key);
    if (!anchor || !container || !element) return;
    container.scrollTop +=
      element.getBoundingClientRect().top - container.getBoundingClientRect().top - anchor.offset;
    container.scrollLeft = anchor.left;
  }, [comparison]);
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
      if (current) selection.set(current.key);
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
  const refreshBusy = useRef(false);
  const refreshGeneration = useRef(0);
  const loadingState = useMemo(createLoadingState, []);
  useLayoutEffect(() => {
    loadingState.set(loading);
  }, [loading, loadingState]);
  useTabInteractions({
    state: (args) => {
      const offset = (args.offset as number | undefined) ?? 0;
      const limit = (args.limit as number | undefined) ?? 50;
      const files: unknown[] = [];
      let bytes = 0;
      for (const file of (comparison?.files ?? []).slice(offset, offset + limit)) {
        const value = {
          path: file.path,
          oldPath: file.oldPath,
          status: file.status,
          ...reviews.current.get(file.key),
          patchAvailable: !!patches[file.key]?.text && !patches[file.key]?.error,
        };
        const size = new TextEncoder().encode(JSON.stringify(value)).length;
        if (bytes + size > 16 * 1024) break;
        bytes += size;
        files.push(value);
      }
      const total = comparison?.files.length ?? 0;
      return {
        summary: comparison ? `${total} changed files` : 'Comparison unavailable',
        data: {
          comparison: comparison
            ? {
                root: comparison.root,
                config: comparison.config,
                base: comparison.base,
                target: comparison.target,
              }
            : null,
          revision: reviews.current.revision,
          ...comments.page(
            (args.commentOffset as number) ?? 0,
            (args.commentLimit as number) ?? 20,
          ),
          totalFiles: total,
          changeCounts: Object.fromEntries(
            ['A', 'M', 'D', 'R', 'T', 'C', 'U'].map((status) => [
              status,
              (comparison?.files ?? []).filter((file) => file.status[0] === status).length,
            ]),
          ),
          files,
          nextOffset: offset + files.length < total ? offset + files.length : null,
          loading,
          error: error.slice(0, 4096) || null,
        },
        observedAt: comparison ? new Date(comparison.time).toISOString() : null,
        stale: loading || !!error || !comparison,
      };
    },
    actions: {
      'add-comment': (args) => {
        if (!comparison || loading || error)
          throw new SdkError('unavailable', 'Comparison unavailable or refreshing');
        if (args.revision !== reviews.current.revision)
          throw new SdkError('revision_conflict', 'Comparison changed; read state again');
        const file = comparison.files.find((f) => f.path === args.path);
        if (!file) throw new SdkError('invalid_arguments', 'Unknown file');
        return changeComments(() =>
          comments.add(
            comments.anchor(
              comparison.root,
              { ...file, patch: patches[file.key]?.text, failed: !!patches[file.key]?.error },
              args.side as 'old' | 'new' | undefined,
              args.line as number | undefined,
            ),
            args.text as string,
            'Agent',
          ),
        );
      },
      'reply-comment': (args) =>
        changeComments(() => comments.reply(args.threadId as string, args.text as string, 'Agent')),
      'delete-comment-thread': (args) =>
        changeComments(() => comments.delete(args.threadId as string)),
      'send-comment': (args) =>
        sendComment(args.threadId as string, args.commentId as string, args.targetTabId as number),
      'set-files-expanded': (args) => {
        if (!comparison || loading || error)
          throw new SdkError('unavailable', 'Comparison unavailable or refreshing');
        return setReview(args.paths as string[], 'expanded', args.expanded as boolean);
      },
      'set-files-viewed': (args) => {
        if (!comparison || loading || error)
          throw new SdkError('unavailable', 'Comparison unavailable or refreshing');
        return setReview(
          args.paths as string[],
          'viewed',
          args.viewed as boolean,
          args.revision as string,
        );
      },
    },
  });
  const [filter, setFilter] = useState('');
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    const next = settings(tab.config);
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
  useDiffRenderDiagnostic('DiffContent', {
    comparison,
    patches,
    loading,
    config,
    compose,
    filter,
    references,
    commits,
    commentError,
    error,
    tagError,
    commitError,
  });
  const refresh = () => {
    const generation = ++refreshGeneration.current;
    refreshBusy.current = true;
    diffDiagnostic('refresh-start');
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
          try {
            const history = await git.commits(next.root);
            if (current()) {
              setCommits(history);
              setCommitError('');
            }
          } catch (error) {
            if (current()) {
              setCommits([]);
              setCommitError(String(error));
            }
          }
          const nextPatches: Record<string, { text?: string; error?: string }> = {};
          const contexts = Object.fromEntries(
            next.files.map((file) => [
              file.key,
              contextLines.current[contextKey(next.config, file)] ?? 3,
            ]),
          );
          const identity = JSON.stringify([next.root, next.base, next.target]);
          const cached =
            next.target && patchSnapshot.current?.identity === identity
              ? patchSnapshot.current
              : undefined;
          let bytes = 0;
          const remaining: FileChange[] = [];
          for (const file of next.files) {
            const saved = cached?.patches[file.key];
            if (saved?.text !== undefined && cached?.contexts[file.key] === contexts[file.key]) {
              nextPatches[file.key] = saved;
              bytes += new TextEncoder().encode(saved.text).length;
            } else remaining.push(file);
          }
          const limitError = t(
            'Comparison exceeds the 8 MiB display limit. Narrow the comparison to view more.',
          );
          for await (const result of git.patches(
            next,
            remaining,
            (file) => contexts[file.key],
            () => current() && bytes < 8 * 1024 * 1024,
          )) {
            if (!current()) return;
            const size = new TextEncoder().encode(result.text ?? '').length;
            if (bytes + size > 8 * 1024 * 1024) {
              nextPatches[result.file.key] = { error: limitError };
              bytes = 8 * 1024 * 1024;
            } else {
              bytes += size;
              nextPatches[result.file.key] = result.error
                ? { error: result.error }
                : { text: result.text };
            }
          }
          for (const file of remaining) nextPatches[file.key] ??= { error: limitError };
          if (!current()) return;
          // Capture at commit time: the reader may have scrolled while Git was running.
          const container = viewer.current;
          if (container && container.clientHeight) {
            const top = container.getBoundingClientRect().top;
            const anchor = [...fileElements.current].find(
              ([key, element]) =>
                next.files.some((file) => file.key === key) &&
                element.getBoundingClientRect().bottom > top + 4,
            );
            if (anchor)
              refreshAnchor.current = {
                key: anchor[0],
                offset: anchor[1].getBoundingClientRect().top - top,
                left: container.scrollLeft,
              };
          }
          // React retains keyed articles and memoized unchanged patches.
          // Commit together so refresh never collapses the existing scroll surface.
          reviews.current.reconcile(
            comparisonIdentity(next),
            next.files.map((file) => ({
              path: file.path,
              key: file.key,
              patch: nextPatches[file.key]?.text,
              failed: !!nextPatches[file.key]?.error,
            })),
          );
          try {
            comments.reconcile(
              next.root,
              next.files.map((file) => ({
                ...file,
                patch: nextPatches[file.key]?.text,
                failed: !!nextPatches[file.key]?.error,
              })),
            );
          } catch (error) {
            setCommentError(String(error));
          }
          patchSnapshot.current = { identity, contexts, patches: nextPatches };
          setComparison(next);
          setPatches(nextPatches);
          selection.set(
            next.files.some((file) => file.key === selection.getSnapshot())
              ? selection.getSnapshot()
              : '',
          );
        } catch (e) {
          if (current()) setError(String(e));
        } finally {
          if (current()) setLoading(false);
          if (generation === refreshGeneration.current) refreshBusy.current = false;
        }
      })
      .catch((e) => {
        if (generation !== refreshGeneration.current) return;
        refreshBusy.current = false;
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
    const schedule = (event?: Event) => {
      if (event && (event.target !== window || document.hidden || refreshBusy.current)) return;
      diffDiagnostic('refresh-scheduled', { reason: event ? 'window-focus' : 'visibility' });
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        if (!event || !refreshBusy.current) refreshRef.current();
      }, 100);
    };
    schedule();
    window.addEventListener('focus', schedule);
    return () => {
      window.removeEventListener('focus', schedule);
      clearTimeout(timer.current);
      void lane.stop();
    };
  }, [visible, lane]);
  function select(file: FileChange) {
    selection.set(file.key);
    fileElements.current.get(file.key)?.scrollIntoView({ block: 'start' });
  }
  const composer = compose && (
    <div className="diff-discussion diff-new-thread">
      <p>
        {compose.path} · {compose.side ?? t('file')} {compose.line ?? ''}
      </p>
      <Composer
        autoFocus
        submit={(text) => {
          changeComments(() => {
            if (loading || error || composeRevision.current !== reviews.current.revision)
              throw new SdkError(
                'revision_conflict',
                'Comparison changed; reopen the comment control',
              );
            return comments.add(compose, text, 'User');
          });
          setCompose(undefined);
        }}
      />
      <Button onClick={() => setCompose(undefined)}>{t('Cancel')}</Button>
    </div>
  );
  const labels: Record<string, string> = {
    A: t('Added'),
    M: t('Modified'),
    D: t('Deleted'),
    R: t('Renamed'),
    T: t('Type changed'),
    C: t('Copied'),
    U: t('Unmerged'),
  };
  return (
    <TabPanel
      hidden={!visible}
      className="plugin-panel diff-panel"
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
        <ComparisonFields
          config={config}
          onChange={setConfig}
          references={references}
          commits={commits}
          commitError={commitError}
        />
        <SelectionSend
          sessionId={session.id}
          tabId={tab.id}
          visible={visible}
          capture={() => {
            if (!viewer.current || !comparison) throw new Error(t('Comparison unavailable.'));
            return captureDiffSelection(
              viewer.current,
              tab.id,
              comparison.root,
              JSON.stringify({
                base: comparison.base,
                target: comparison.target ?? 'working directory',
                mode: comparison.config.mode,
              }),
            );
          }}
        />
        <Button type="submit">{t('Compare')}</Button>
        <Button type="button" className="diff-refresh" onClick={refresh}>
          <span className="diff-spinner-slot">
            {loading && (
              <span className="diff-spinner" role="status" aria-label={t('Loading comparison')} />
            )}
          </span>
          {t('Refresh')}
        </Button>
      </form>
      {(commentError || comments.error) && <p role="alert">{commentError || comments.error}</p>}
      {tagError && (
        <p role="alert">
          {t('Could not update the session change count: ')}
          {tagError}
        </p>
      )}
      {error && (
        <p role="alert">
          {error}
          {t(' Check the folder, Git installation and revision expressions, then Compare again.')}
        </p>
      )}
      {!comparison && comments.threads.length > 0 && (
        <section>
          <h3>{t('Saved comments · comparison unavailable')}</h3>
          {comments.threads.map(discussion)}
        </section>
      )}
      {comparison && (
        <div className="diff-layout">
          <DiffSidebar>
            <label>
              {t('Filter filenames')}
              <Input value={filter} onChange={(e) => setFilter(e.target.value)} />
            </label>
            <nav className="file-tree" aria-label={t('Changed files')}>
              <FileTree
                files={comparison.files.filter((f) =>
                  `${f.oldPath}\n${f.path}`.toLowerCase().includes(filter.toLowerCase()),
                )}
                selection={selection}
                visible={visible}
                onSelect={select}
                labels={labels}
              />
            </nav>
            {!comparison.files.length && <p>{t('No changes.')}</p>}
          </DiffSidebar>
          <div ref={viewer} className="diff-viewer" tabIndex={0} aria-label={t('Unified diff')}>
            <DiffLoading.Provider value={loadingState}>
              <div className="diff-page">
                {comparison.files.map((file) => (
                  <article
                    className="diff-file"
                    data-diff-path={file.path}
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
                      {...reviews.current.get(file.key)}
                      setExpanded={(value) => setReview([file.path], 'expanded', value)}
                      setViewed={(value) =>
                        setReview([file.path], 'viewed', value, reviews.current.revision)
                      }
                      comment={() => {
                        try {
                          setReview([file.path], 'expanded', true);
                          composeRevision.current = reviews.current.revision;
                          setCompose(
                            comments.anchor(comparison.root, {
                              ...file,
                              patch: patches[file.key]?.text,
                              failed: !!patches[file.key]?.error,
                            }),
                          );
                        } catch (error) {
                          setCommentError(String(error));
                        }
                      }}
                      path={
                        file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path
                      }
                      status={labels[file.status[0]]}
                      statusCode={file.status[0]}
                      patch={patches[file.key]?.text}
                      failed={!!patches[file.key]?.error}
                    >
                      {comments.threads
                        .filter(
                          (t) => !t.orphan && !t.location.side && t.location.path === file.path,
                        )
                        .map(discussion)}
                      {compose?.path === file.path && !compose.side && composer}
                      {patches[file.key]?.error ? (
                        <p role="alert">{patches[file.key].error}</p>
                      ) : patches[file.key]?.text !== undefined ? (
                        <Viewer
                          discussionRevision={JSON.stringify([
                            comparison.root,
                            getLocale(),
                            comments.threads.filter((thread) => thread.location.path === file.path),
                            compose?.path === file.path ? compose : null,
                          ])}
                          threads={[
                            ...comments.threads
                              .filter(
                                (t) =>
                                  !t.orphan && t.location.path === file.path && t.location.side,
                              )
                              .map((t) => ({
                                side: t.location.side!,
                                line: t.location.line!,
                                node: discussion(t),
                              })),
                            ...(compose?.path === file.path && compose.side
                              ? [{ side: compose.side, line: compose.line!, node: composer }]
                              : []),
                          ]}
                          patch={patches[file.key].text!}
                          comment={(side, line) => {
                            try {
                              composeRevision.current = reviews.current.revision;
                              setCompose(
                                comments.anchor(
                                  comparison.root,
                                  { ...file, patch: patches[file.key]?.text },
                                  side,
                                  line,
                                ),
                              );
                            } catch (error) {
                              setCommentError(String(error));
                            }
                          }}
                          expand={() => {
                            const key = contextKey(comparison.config, file);
                            contextLines.current[key] = (contextLines.current[key] ?? 3) + 20;
                            refresh();
                          }}
                        />
                      ) : (
                        <p>
                          {loading
                            ? t('Loading patch…')
                            : t('Patch unavailable. Refresh to retry.')}
                        </p>
                      )}
                    </FileReview>
                  </article>
                ))}
                {comments.threads.some((t) => t.orphan) && (
                  <section>
                    <h3>{t('Orphan comments')}</h3>
                    {comments.threads.filter((t) => t.orphan).map(discussion)}
                  </section>
                )}
                {!comparison.files.length && <p>{t('No changes.')}</p>}
              </div>
            </DiffLoading.Provider>
          </div>
        </div>
      )}
      {comparison && (
        <footer className="plugin-statusbar diff-statusbar" aria-label={t('Diff status')}>
          <div className="diff-summary">
            {
              {
                branch: 'Branch changes',
                refs: 'Between two refs',
              }[comparison.config.mode]
            }{' '}
            · {comparison.baseLabel} ({comparison.base.slice(0, 12)}) →{' '}
            {comparison.target
              ? `${comparison.targetLabel} (${comparison.target.slice(0, 12)})`
              : t('Working directory')}{' '}
            {t('· Last successful refresh ')}
            {new Date(comparison.time).toLocaleTimeString(getLocale())}
            {error && t(' · Stale')}
          </div>
          <span>
            {comparison.files.length}
            {t(' changed files ·')}{' '}
            {
              comparison.files.filter((file) =>
                `${file.oldPath}\n${file.path}`.toLowerCase().includes(filter.toLowerCase()),
              ).length
            }{' '}
            {t('matching filter')}
          </span>
          {(loading || error) && (
            <span>
              {loading
                ? t('Loading comparison…')
                : t('Refresh failed · Showing previous comparison')}
            </span>
          )}
        </footer>
      )}
    </TabPanel>
  );
}

function FileTree({
  files,
  selection,
  visible,
  onSelect,
  labels,
}: {
  files: FileChange[];
  selection: DiffSelection;
  visible: boolean;
  onSelect: (file: FileChange) => void;
  labels: Record<string, string>;
}) {
  const selected = useSyncExternalStore(selection.subscribe, selection.getSnapshot);
  useDiffRenderDiagnostic('FileTree', { files, selected, visible });
  const root = useMemo(() => buildFileTree(files), [files]);
  const render = (node: FileTreeNode) => (
    <ul>
      {[...node.directories]
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, child]) => (
          <li key={name}>
            <Disclosure open>
              <DisclosureTrigger title={name}>
                <FolderIcon />
                <span className="diff-tree-label file-tree-label">{name}</span>
              </DisclosureTrigger>
              {render(child)}
            </Disclosure>
          </li>
        ))}
      {node.files.map((file) => (
        <li key={file.key}>
          <Button
            title={file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path}
            aria-pressed={selected === file.key}
            aria-current={selected === file.key ? 'true' : undefined}
            onClick={() => onSelect(file)}
          >
            <FileIcon status={file.status[0]} statusLabel={labels[file.status[0]]} />
            <span className="diff-tree-label file-tree-label">{file.path.split('/').at(-1)}</span>
          </Button>
        </li>
      ))}
    </ul>
  );
  return render(root);
}
