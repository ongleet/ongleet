import { useCallback, useMemo, useRef, type ReactNode } from 'react';
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
    discussionIndex,
    patches,
    setCommentError,
    compose,
    discussion,
    loading,
  } = controller;
  const fileDiscussions = discussionIndex.forFile(file.path);
  const locale = getLocale();
  const lineCompose = compose?.path === file.path && compose.side ? compose : undefined;
  const lineComposer = lineCompose ? composer : undefined;
  const threads = useMemo(
    () => [
      ...fileDiscussions.line.map((thread) => ({
        key: `thread:${thread.id}`,
        side: thread.location.side!,
        line: thread.location.line!,
        node: discussion(thread),
      })),
      ...(lineCompose
        ? [
            {
              key: 'composer',
              side: lineCompose.side!,
              line: lineCompose.line!,
              node: lineComposer,
            },
          ]
        : []),
    ],
    [fileDiscussions.line, discussion, lineCompose, lineComposer, locale],
  );
  // Memoized patches dispatch through current comparison inputs.
  const latest = useRef({ controller, comparison, file });
  latest.current = { controller, comparison, file };
  const commentLine = useCallback((side: 'old' | 'new', line: number) => {
    const { controller: current, comparison, file } = latest.current;
    try {
      current.composeRevision.current = current.reviews.current.revision;
      current.setCompose(
        current.comments.anchor(
          comparison.root,
          { ...file, patch: current.patches[file.key]?.text },
          side,
          line,
        ),
      );
    } catch (error) {
      current.setCommentError(String(error));
    }
  }, []);
  const expand = useCallback(() => {
    const { controller: current, comparison, file } = latest.current;
    const key = current.contextKey(comparison.config, file);
    current.contextLines.current[key] = (current.contextLines.current[key] ?? 3) + 20;
    current.refresh();
  }, []);
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
        {fileDiscussions.file.map(discussion)}
        {compose?.path === file.path && !compose.side && composer}
        {patches[file.key]?.error ? (
          <p role="alert">{patches[file.key].error}</p>
        ) : patches[file.key]?.text !== undefined ? (
          <PatchViewer
            locale={locale}
            threads={threads}
            patch={patches[file.key].text!}
            comment={commentLine}
            expand={expand}
          />
        ) : (
          <p>{loading ? t('Loading patch…') : t('Patch unavailable. Refresh to retry.')}</p>
        )}
      </FileReview>
    </article>
  );
}
