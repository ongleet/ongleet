import { useCallback, useMemo, useRef, useState } from 'react';
import type { PluginSdk } from '../../../../sdk';
import { CommentStore, commentTargets, deliverComment, type Location } from '../comments';
import { DiscussionIndex } from '../discussion-index';
import { Discussion } from '../components/Discussions';

/** Retain discussion drafts and publish persisted changes for this tab. */
export function useDiffDiscussions(sdk: PluginSdk, sessionId: number, tabId: number) {
  const comments = useMemo(() => new CommentStore(sessionId, tabId), [sessionId, tabId]);
  const index = useMemo(() => new DiscussionIndex(), [comments]);
  const discussionIndex = index.read(comments.threads);
  const [, renderComments] = useState(0);
  const [commentError, setCommentError] = useState('');
  const [compose, setCompose] = useState<Location>();
  const composeRevision = useRef('');

  const changeComments = useCallback(<T,>(operation: () => T): T => {
    try {
      const value = operation();
      renderComments((v) => v + 1);
      setCommentError('');
      return value;
    } catch (error) {
      setCommentError(String(error));
      throw error;
    }
  }, []);

  const targets = useCallback(
    () => commentTargets(sdk.tabs, sessionId, tabId),
    [sdk, sessionId, tabId],
  );

  const sendComment = useCallback(
    (threadId: string, commentId: string, targetTabId: number) =>
      deliverComment(sdk.tabs, sessionId, tabId, comments.get(threadId), commentId, targetTabId),
    [sdk, sessionId, tabId, comments],
  );

  const discussion = useCallback(
    (thread: import('../comments').Thread) => (
      <Discussion
        key={thread.id}
        thread={thread}
        sessionId={sessionId}
        tabId={tabId}
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
    ),
    [targets, sendComment, changeComments, comments, sessionId, tabId],
  );

  return {
    comments,
    discussionIndex,
    commentError,
    setCommentError,
    compose,
    setCompose,
    composeRevision,
    changeComments,
    sendComment,
    discussion,
  };
}
