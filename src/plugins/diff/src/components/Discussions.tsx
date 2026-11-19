import { useAgentQueue } from '../../../../components/AgentQueuePanel';
import { AgentPicker } from '../../../../components/AgentPicker';
import { preferredAgent } from '../../../../components/agent-recipient';
import { diffDiagnostic, useDiffRenderDiagnostic } from '../render-diagnostics';
import { t, getLocale } from '../../../../i18n';
import { Button, Textarea } from '../../../../components/Controls';
import { useEffect, useRef, useState } from 'react';
import type { Location, Thread } from '../comments';

/** Keep unsent comment text local until submission so typing does not rerender the comparison. */
export function Composer({
  submit,
  label = 'Comment',
  autoFocus = false,
  initialText = '',
  cancel,
}: {
  submit: (text: string) => void;
  label?: string;
  autoFocus?: boolean;
  initialText?: string;
  cancel?: () => void;
}) {
  // Keep typing native: the draft is only needed when the form is submitted.
  const textarea = useRef<HTMLTextAreaElement>(null);
  useDiffRenderDiagnostic('Composer', { submit, label, autoFocus, initialText, cancel });
  return (
    <form
      className="diff-comment-composer"
      onSubmit={(e) => {
        e.preventDefault();
        const input = textarea.current;
        if (!input) return;
        try {
          submit(input.value);
          input.value = '';
        } catch {
          /* Keep the draft; the shared store reports the error. */
        }
      }}
    >
      <Textarea
        aria-label={t(label)}
        placeholder={label === 'Reply' ? t('Write a reply…') : t('Write a comment or question…')}
        autoFocus={autoFocus}
        ref={textarea}
        defaultValue={initialText}
        onInput={() => diffDiagnostic('textarea-input')}
        required
      />
      <div className="diff-composer-actions">
        <Button type="submit">{label === 'Comment' ? t('Add comment') : t(label)}</Button>
        {cancel && (
          <Button type="button" onClick={cancel}>
            {t('Cancel')}
          </Button>
        )}
      </div>
    </form>
  );
}

/** Render a thread with reply, edit, deletion and explicit recipient actions. */
export function Discussion({
  thread,
  sessionId,
  tabId,
  reply,
  edit,
  remove,
  targets,
  send,
}: {
  thread: Thread;
  sessionId: number;
  tabId: number;
  reply: (text: string) => void;
  edit: (commentId: string, text: string) => void;
  remove: () => void;
  targets: () => Promise<{ id: number; name: string }[]>;
  send: (commentId: string, target: number) => Promise<unknown>;
}) {
  const [recipients, setRecipients] = useState<{ id: number; name: string }[]>([]);
  const [target, setTarget] = useState<number>();
  const queue = useAgentQueue(sessionId, target);
  const queueing =
    !queue ||
    queue.activity !== 'idle' ||
    queue.phase !== 'ready' ||
    !!queue.paused ||
    queue.items.length > 0;
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const writing = useRef(false);
  const threadKey = `${tabId}.${thread.id}`;
  useEffect(() => {
    let active = true;
    setLoading(true);
    const refresh = () => {
      void targets()
        .then((list) => {
          if (!active) return;
          setRecipients(list);
          setTarget(preferredAgent(list, sessionId, threadKey));
        })
        .catch((error) => {
          if (active) setStatus(String(error));
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    };
    refresh();
    window.addEventListener('ongleet-agent-recipient', refresh);
    window.addEventListener('focus', refresh);
    return () => {
      active = false;
      window.removeEventListener('ongleet-agent-recipient', refresh);
      window.removeEventListener('focus', refresh);
    };
  }, [targets, sessionId, threadKey]);
  async function deliver(commentId: string) {
    if (target === undefined || writing.current) return;
    writing.current = true;
    setBusy(true);
    setStatus(t('Sending…'));
    try {
      await send(commentId, target);
      setStatus('');
    } catch (error) {
      setStatus(error instanceof Error ? t(error.message) : String(error));
    } finally {
      writing.current = false;
      setBusy(false);
    }
  }
  const [status, setStatus] = useState('');
  const [editing, setEditing] = useState<string | null>(null);
  useDiffRenderDiagnostic('Discussion', {
    thread,
    recipients,
    status,
    editing,
    reply,
    edit,
    remove,
    targets,
    send,
  });
  const location: Location = thread.orphan ? thread.original : thread.location;
  return (
    <section className="diff-discussion" aria-label={t('Thread {value0}', { value0: thread.id })}>
      <div className="diff-discussion-heading">
        <span>
          {location.side
            ? t('{side} line {line}', {
                side: location.side === 'old' ? t('Old') : t('New'),
                line: location.line!,
              })
            : t('File discussion')}
        </span>
        <span className="diff-discussion-count">
          {thread.comments.length} {thread.comments.length === 1 ? t('message') : t('messages')}
        </span>
      </div>
      {thread.orphan && (
        <>
          <p className="diff-discussion-location">{location.path}</p>
          <pre>{thread.original.context.join('\n')}</pre>
        </>
      )}
      <AgentPicker
        sessionId={sessionId}
        onFocus={() => {
          void targets()
            .then((list) => {
              setRecipients(list);
              setTarget((current) =>
                list.some((agent) => agent.id === current)
                  ? current
                  : preferredAgent(list, sessionId, threadKey),
              );
            })
            .catch((error) => setStatus(String(error)));
        }}
        targets={recipients}
        target={target}
        onChange={setTarget}
        disabled={busy}
        loading={loading}
      />
      <div className="diff-messages">
        {thread.comments.map((comment) => (
          <article
            className="diff-message"
            data-origin={comment.origin}
            data-comment-id={comment.id}
            tabIndex={-1}
            key={comment.id}
          >
            <div className="diff-message-meta">
              <span
                className="diff-message-author"
                title={t('Origin label; identity is not verified')}
              >
                {comment.origin}
              </span>
              <time dateTime={comment.at}>{new Date(comment.at).toLocaleString(getLocale())}</time>
            </div>
            {editing === comment.id ? (
              <Composer
                key={comment.id}
                label="Save comment"
                initialText={comment.text}
                autoFocus
                cancel={() => setEditing(null)}
                submit={(text) => {
                  edit(comment.id, text);
                  setEditing(null);
                }}
              />
            ) : (
              <>
                <p className="diff-comment-text">{comment.text}</p>
                <Button type="button" onClick={() => setEditing(comment.id)}>
                  {t('Edit comment')}
                </Button>
              </>
            )}
            <Button
              type="button"
              disabled={busy || loading || target === undefined}
              onClick={() => void deliver(comment.id)}
            >
              {queueing ? t('Add to queue') : t('Send to agent')}
            </Button>
          </article>
        ))}
      </div>
      <Composer label="Reply" submit={reply} />
      <Button type="button" className="diff-delete-thread" onClick={remove}>
        {t('Delete thread')}
      </Button>
      {status && (
        <p className="diff-discussion-status" role="status">
          {status}
        </p>
      )}
    </section>
  );
}
