import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { t } from '../i18n';
import { Button, Disclosure, DisclosureTrigger, Textarea } from './Controls';
import type { QueueMessage } from './agent-queue';
import { agentQueues } from './agent-queue-service';

const activityLabels = {
  working: 'Agent working',
  question: 'Agent awaiting your reply',
  approval: 'Agent awaiting approval',
  idle: 'Agent idle',
  stopped: 'Agent stopped',
  unknown: 'Agent activity unavailable',
  unavailable: 'Agent unavailable',
};
const pauseLabels = {
  manual: 'Queue paused',
  restored: 'Saved queue paused; resume explicitly',
  unavailable: 'Queue paused: agent activity unavailable',
  ambiguous: 'Queue paused: delivery uncertain; no retry was made',
  storage: 'Queue paused: messages could not be saved',
};
export function useAgentQueue(sessionId: number, targetId?: number) {
  const queues = agentQueues();
  useSyncExternalStore(
    useCallback(
      (listener: () => void) =>
        targetId === undefined ? () => {} : queues.subscribeTarget(sessionId, targetId, listener),
      [queues, sessionId, targetId],
    ),
    useCallback(
      () => (targetId === undefined ? 0 : queues.targetVersion(sessionId, targetId)),
      [queues, sessionId, targetId],
    ),
  );
  useEffect(() => {
    if (targetId !== undefined) queues.ensure(sessionId, targetId);
  }, [queues, sessionId, targetId]);
  return targetId === undefined ? undefined : queues.get(sessionId, targetId);
}

function QueuedMessage({
  item,
  sessionId,
  targetId,
  disabled,
  report,
}: {
  item: QueueMessage;
  sessionId: number;
  targetId: number;
  disabled: boolean;
  report: (error: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(item.text);
  function act(operation: () => void | Promise<void>) {
    try {
      void Promise.resolve(operation()).catch((error) =>
        report(error instanceof Error ? t(error.message) : String(error)),
      );
    } catch (error) {
      report(error instanceof Error ? t(error.message) : String(error));
    }
  }
  return (
    <li className="agent-queue-message">
      <span>{item.label}</span>
      {item.state === 'sending' && <strong>{t('Sending…')}</strong>}
      {item.state === 'uncertain' && (
        <strong>{t('Delivery uncertain; check the agent before removing this message.')}</strong>
      )}
      {editing ? (
        <>
          <p>{t('Editing pauses the queue; resume when ready.')}</p>
          <Textarea
            aria-label={t('Queued message')}
            value={text}
            onChange={(event) => setText(event.target.value)}
            disabled={disabled}
          />
          <Button
            type="button"
            disabled={disabled}
            onClick={() =>
              act(() => {
                agentQueues().edit(sessionId, targetId, item.id, text);
                setEditing(false);
              })
            }
          >
            {t('Save')}
          </Button>
          <Button type="button" onClick={() => setEditing(false)}>
            {t('Cancel')}
          </Button>
        </>
      ) : (
        <pre>{item.text}</pre>
      )}
      <div className="agent-queue-actions">
        {item.state === 'pending' && (
          <>
            <Button
              type="button"
              disabled={disabled}
              onClick={() => {
                act(() => {
                  agentQueues().pause(sessionId, targetId);
                  setText(item.text);
                  setEditing(true);
                });
              }}
            >
              {t('Edit')}
            </Button>
            <Button
              type="button"
              disabled={disabled}
              onClick={() => act(() => agentQueues().sendNow(sessionId, targetId, item.id))}
            >
              {t('Send now')}
            </Button>
          </>
        )}
        <Button
          type="button"
          disabled={item.state === 'sending'}
          onClick={() => act(() => agentQueues().remove(sessionId, targetId, item.id))}
        >
          {t('Remove from queue')}
        </Button>
      </div>
    </li>
  );
}

/** One view of the same recipient FIFO from every sending tab. */
export function AgentQueuePanel({ sessionId, targetId }: { sessionId: number; targetId: number }) {
  const queue = useAgentQueue(sessionId, targetId);
  const [error, setError] = useState('');
  if (!queue) return null;
  const sending = queue.items.some((item) => item.state === 'sending');
  const uncertain = queue.items.some((item) => item.state === 'uncertain');
  return (
    <div className="agent-queue-panel">
      <span className="agent-delivery-status" role="status" data-activity={queue.activity}>
        <span className="agent-activity-dot" aria-hidden="true" />
        {queue.name ?? t('Agent tab {id}', { id: targetId })} · {t(activityLabels[queue.activity])}
        {queue.lastSent && <> · {t('Message sent')}</>}
      </span>
      {queue.phase === 'awaiting-work' && queue.items.some((item) => item.state === 'pending') && (
        <p>{t('Waiting for agent activity before sending the next message')}</p>
      )}
      {queue.items.length > 0 && (
        <Disclosure>
          <DisclosureTrigger>
            {t('Queue ({count})', { count: queue.items.length })}
          </DisclosureTrigger>
          {queue.paused && <p role="status">{t(pauseLabels[queue.paused])}</p>}
          <Button
            type="button"
            disabled={uncertain}
            onClick={() => {
              setError('');
              try {
                queue.paused
                  ? agentQueues().resume(sessionId, targetId)
                  : agentQueues().pause(sessionId, targetId);
              } catch (error) {
                setError(error instanceof Error ? t(error.message) : String(error));
              }
            }}
          >
            {queue.paused ? t('Resume queue') : t('Pause queue')}
          </Button>
          <ol>
            {queue.items.map((item) => (
              <QueuedMessage
                key={item.id}
                item={item}
                sessionId={sessionId}
                targetId={targetId}
                disabled={sending || uncertain}
                report={setError}
              />
            ))}
          </ol>
        </Disclosure>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
