import { t, getLocale } from '../../i18n';
import { Button, Disclosure, DisclosureTrigger, Textarea } from '../../components/Controls';
import { useState } from 'react';
import type { Location, Thread } from './comments';
export function Composer({
  submit,
  label = 'Comment',
  autoFocus = false,
}: {
  submit: (text: string) => void;
  label?: string;
  autoFocus?: boolean;
}) {
  const [text, setText] = useState('');
  return (
    <form
      className="diff-comment-composer"
      onSubmit={(e) => {
        e.preventDefault();
        try {
          submit(text);
          setText('');
        } catch {
          /* Keep the draft; the shared store reports the error. */
        }
      }}
    >
      <Textarea
        aria-label={t(label)}
        placeholder={label === 'Reply' ? t('Write a reply…') : t('Write a comment or question…')}
        autoFocus={autoFocus}
        value={text}
        onChange={(e) => setText(e.target.value)}
        required
      />
      <div className="diff-composer-actions">
        <Button type="submit">{label === 'Comment' ? t('Add comment') : t('Reply')}</Button>
      </div>
    </form>
  );
}
export function Discussion({
  thread,
  reply,
  remove,
  targets,
  send,
}: {
  thread: Thread;
  reply: (text: string) => void;
  remove: () => void;
  targets: () => Promise<{ id: number; name: string }[]>;
  send: (commentId: string, target: number) => Promise<unknown>;
}) {
  const [recipients, setRecipients] = useState<{ id: number; name: string }[]>([]);
  const [status, setStatus] = useState('');
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
      <div className="diff-messages">
        {thread.comments.map((comment) => (
          <article className="diff-message" data-origin={comment.origin} key={comment.id}>
            <div className="diff-message-meta">
              <span
                className="diff-message-author"
                title={t('Origin label; identity is not verified')}
              >
                {comment.origin}
              </span>
              <time dateTime={comment.at}>{new Date(comment.at).toLocaleString(getLocale())}</time>
            </div>
            <p className="diff-comment-text">{comment.text}</p>
            <Disclosure
              className="diff-message-send"
              onToggle={(e) => {
                if (e.currentTarget.open)
                  void targets()
                    .then(setRecipients)
                    .catch((error) => setStatus(String(error)));
              }}
            >
              <DisclosureTrigger>{t('Send to…')}</DisclosureTrigger>
              <div className="diff-message-recipients">
                {recipients.map((target) => (
                  <Button
                    type="button"
                    key={target.id}
                    onClick={() => {
                      setStatus('Sending…');
                      void send(comment.id, target.id)
                        .then(() => setStatus('Write acknowledged; agent processing is unknown.'))
                        .catch((error) =>
                          setStatus(
                            t(
                              'Send failed; delivery may be ambiguous. No retry was made. {value0}',
                              { value0: String(error) },
                            ),
                          ),
                        );
                    }}
                  >
                    {target.name}
                  </Button>
                ))}
                {!recipients.length && <span>{t('No available recipients.')}</span>}
              </div>
            </Disclosure>
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
