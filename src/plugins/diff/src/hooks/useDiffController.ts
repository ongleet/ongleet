import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { t } from '../../../../i18n';
import { useSdk } from '../../../../sdk';
import type { PluginContentProps } from '../../../types';
import { diffDiagnostic, useDiffRenderDiagnostic } from '../render-diagnostics';
import { createLoadingState } from '../loading-state';
import { useDiffDiscussions } from './useDiffDiscussions';
import { useDiffScroll } from './useDiffScroll';
import { DiffSelection } from '../selection-state';
import { ReviewStore } from '../review-state';
import { settings, type Settings } from '../settings';
import { GitComparison, Lane, type Comparison, type FileChange, type CommitOption } from '../git';
import { filesInTreeOrder } from '../file-tree';
import { updateDiffTag } from '../status-tag';

/** Own comparison refreshes, cancellation, patch caching and tab-scoped review state. */
export function useDiffController({ tab, session, visible }: PluginContentProps) {
  const sdk = useSdk();
  const discussions = useDiffDiscussions(sdk, session.id, tab.id);
  const { comments, commentError, setCommentError, compose } = discussions;
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
  const { fileElements, viewer, select, captureAnchor } = useDiffScroll(
    comparison,
    visible,
    selection,
  );
  const [error, setError] = useState('');
  const [tagError, setTagError] = useState('');
  const [loading, setLoading] = useState(false);
  const refreshBusy = useRef(false);
  const refreshGeneration = useRef(0);
  const loadingState = useMemo(createLoadingState, []);
  useLayoutEffect(() => {
    loadingState.set(loading);
  }, [loading, loadingState]);
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

  /** Replace prior Git work and publish current patches while retaining review and scroll context. */
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
          captureAnchor(next);
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

  return {
    config,
    setConfig,
    comparison,
    patches,
    reviews,
    setReview,
    references,
    commits,
    commitError,
    selection,
    fileElements,
    viewer,
    select,
    error,
    tagError,
    loading,
    loadingState,
    filter,
    setFilter,
    refresh,
    contextKey,
    contextLines,
    ...discussions,
  };
}

export type DiffController = ReturnType<typeof useDiffController>;
