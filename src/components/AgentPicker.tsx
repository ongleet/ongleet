import { AgentQueuePanel } from './AgentQueuePanel';
import { agentQueues } from './agent-queue-service';
import { useSyncExternalStore } from 'react';
import { t } from '../i18n';
import { Select } from './Controls';
import type { AgentTarget } from './agent-recipient';

/** Shared compact recipient control for selections and discussion messages. */
export function AgentPicker({
  sessionId,
  targets,
  target,
  onChange,
  disabled = false,
  loading = false,
  onFocus,
}: {
  sessionId: number;
  targets: AgentTarget[];
  target: number | undefined;
  onChange: (target: number | undefined) => void;
  disabled?: boolean;
  loading?: boolean;
  onFocus?: () => void;
}) {
  const queues = agentQueues();
  useSyncExternalStore(queues.subscribe, () => queues.pendingTargets(sessionId));
  const missing = queues
    .list(sessionId)
    .filter((queue) => !targets.some((target) => target.id === queue.targetId));
  const options = [
    ...targets,
    ...missing.map((queue) => ({
      id: queue.targetId,
      name: queue.name
        ? t('{agent} (unavailable)', { agent: queue.name })
        : t('Agent tab {id} (unavailable)', { id: queue.targetId }),
    })),
  ];
  return (
    <div className="agent-recipient">
      <label className="agent-picker">
        {t('Agent')}
        <Select
          aria-label={t('Agent')}
          onFocus={onFocus}
          value={target ?? ''}
          disabled={disabled || loading || !options.length}
          onChange={(event) =>
            onChange(event.target.value ? Number(event.target.value) : undefined)
          }
        >
          <option value="" disabled>
            {loading
              ? t('Finding agents…')
              : options.length
                ? t('Choose an agent')
                : t('No agent configured in this session')}
          </option>
          {options.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {agent.name}
            </option>
          ))}
        </Select>
      </label>
      {target !== undefined && <AgentQueuePanel sessionId={sessionId} targetId={target} />}
    </div>
  );
}
