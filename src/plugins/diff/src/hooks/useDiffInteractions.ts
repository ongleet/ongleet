import { useTabInteractions, SdkError } from '../../../../sdk';
import type { DiffController } from './useDiffController';

/** Expose bounded comparison snapshots and revision-checked review and discussion actions. */
export function useDiffInteractions(controller: DiffController) {
  const {
    comparison,
    reviews,
    patches,
    comments,
    loading,
    error,
    changeComments,
    sendComment,
    setReview,
  } = controller;
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
}
