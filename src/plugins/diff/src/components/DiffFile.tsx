import type { ReactNode } from 'react';
import { t, getLocale } from '../../../../i18n';
import { FileReview } from './FileReview';
import { PatchViewer } from './PatchViewer';
import type { Comparison, FileChange } from '../git';
import type { DiffController } from '../hooks/useDiffController';

/** Render one keyed file review with file-level and line-level discussions. */
export function DiffFile({
  controller,
  comparison,
  file,
  labels,
  composer,
}: {
  controller: DiffController;
  comparison: Comparison;
  file: FileChange;
  labels: Record<string, string>;
  composer: ReactNode;
}) {
  const {
    fileElements,
    reviews,
    setReview,
    composeRevision,
    setCompose,
    comments,
    patches,
    setCommentError,
    compose,
    discussion,
    contextKey,
    contextLines,
    refresh,
    loading,
  } = controller;
  return (
    <article
      className="diff-file"
      data-diff-path={file.path}
      ref={(element) => {
        if (element) fileElements.current.set(file.key, element);
        else fileElements.current.delete(file.key);
      }}
    >
      <FileReview
        {...reviews.current.get(file.key)}
        setExpanded={(value) => setReview([file.path], 'expanded', value)}
        setViewed={(value) => setReview([file.path], 'viewed', value, reviews.current.revision)}
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
        path={file.oldPath !== file.path ? `${file.oldPath} → ${file.path}` : file.path}
        status={labels[file.status[0]]}
        statusCode={file.status[0]}
        patch={patches[file.key]?.text}
        failed={!!patches[file.key]?.error}
      >
        {comments.threads
          .filter((t) => !t.orphan && !t.location.side && t.location.path === file.path)
          .map(discussion)}
        {compose?.path === file.path && !compose.side && composer}
        {patches[file.key]?.error ? (
          <p role="alert">{patches[file.key].error}</p>
        ) : patches[file.key]?.text !== undefined ? (
          <PatchViewer
            discussionRevision={JSON.stringify([
              comparison.root,
              getLocale(),
              comments.threads.filter((thread) => thread.location.path === file.path),
              compose?.path === file.path ? compose : null,
            ])}
            threads={[
              ...comments.threads
                .filter((t) => !t.orphan && t.location.path === file.path && t.location.side)
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
          <p>{loading ? t('Loading patch…') : t('Patch unavailable. Refresh to retry.')}</p>
        )}
      </FileReview>
    </article>
  );
}
