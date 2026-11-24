import { agentQueues } from '../../../../components/agent-queue-service';
import { useCallback, useMemo, useRef, useState } from 'react';
import type { PluginSdk } from '../../../../sdk';
import {
  CommentStore,
  commentTargets,
  commentMessage,
  deliverComment,
  type Location,
} from '../comments';
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

  // The public interaction still acknowledges a direct write, rather than queue acceptance.
  const sendComment = useCallback(
    (threadId: string, commentId: string, targetTabId: number) =>
      deliverComment(sdk.tabs, sessionId, tabId, comments.get(threadId), commentId, targetTabId),
    [sdk, sessionId, tabId, comments],
  );

  const queueComment = useCallback(
    async (threadId: string, commentId: string, targetTabId: number) => {
      const thread = comments.get(threadId);
      agentQueues().enqueue(sessionId, targetTabId, {
        sourceId: tabId,
        text: commentMessage(tabId, thread, commentId),
        label: `Diff: ${thread.location.path}`,
        threadKey: `${tabId}.${threadId}`,
      });
    },
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
        send={(commentId, target) => queueComment(thread.id, commentId, target)}
        reply={(text) => {
          changeComments(() => comments.reply(thread.id, text, 'User'));
        }}
        edit={(commentId, text) => {
          changeComments(() => comments.edit(thread.id, commentId, text));
        }}
        removeComment={(commentId) => {
          try {
            changeComments(() => comments.deleteComment(thread.id, commentId));
          } catch {}
        }}
        remove={() => {
          try {
            changeComments(() => comments.delete(thread.id));
          } catch {}
        }}
      />
    ),
    [targets, queueComment, changeComments, comments, sessionId, tabId],
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
