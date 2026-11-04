import { useState } from 'react';
import type { Location, Thread } from './comments';
export function Composer({
  submit,
  label = 'Comment',
}: {
  submit: (text: string) => void;
  label?: string;
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
          /* The shared store reports errors; retain the draft. */
        }
      }}
    >
      <textarea
        aria-label={label}
        value={text}
        onChange={(e) => setText(e.target.value)}
        required
      />
      <button>{label}</button>
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
    <section className="diff-discussion" aria-label={`Thread ${thread.id}`}>
      <small>User and Agent label origin; identity is not verified.</small>
      <p>
        {location.path} · {location.side ?? 'file'} {location.line ?? ''}
      </p>
      {thread.orphan && <pre>{thread.original.context.join('\n')}</pre>}
      {thread.comments.map((comment) => (
        <div key={comment.id}>
          <small>
            {comment.origin} · {new Date(comment.at).toLocaleString()}
          </small>
          <p className="diff-comment-text">{comment.text}</p>
          <details
            onToggle={(e) => {
              if (e.currentTarget.open)
                void targets()
                  .then(setRecipients)
                  .catch((error) => setStatus(String(error)));
            }}
          >
            <summary>Send to…</summary>
            {recipients.map((target) => (
              <button
                key={target.id}
                onClick={() => {
                  setStatus('Sending…');
                  void send(comment.id, target.id)
                    .then(() => setStatus('Write acknowledged; agent processing is unknown.'))
                    .catch((error) =>
                      setStatus(
                        `Send failed; delivery may be ambiguous. No retry was made. ${String(error)}`,
                      ),
                    );
                }}
              >
                {target.name}
              </button>
            ))}
            {!recipients.length && <p>No available recipients.</p>}
          </details>
        </div>
      ))}
      <Composer label="Reply" submit={reply} />
      <button onClick={remove}>Delete thread</button>
      {status && <p role="status">{status}</p>}
    </section>
  );
}
