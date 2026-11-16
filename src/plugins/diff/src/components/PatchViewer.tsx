import { memo, Fragment, useContext, useMemo, useSyncExternalStore } from 'react';
import { parseDiff, Hunk, Decoration, getChangeKey } from 'react-diff-view';
import { Button } from '../../../../components/Controls';
import { t } from '../../../../i18n';
import { useDiffRenderDiagnostic } from '../render-diagnostics';
import { DiffLoading } from '../loading-state';
import { HighlightedDiff } from './syntax';

/** Subscribe only the context-expansion control to shared loading state. */
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

/** Memoize parsed patches and render line discussions alongside selectable code. */
export const PatchViewer = memo(
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
                patch={patch}
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
