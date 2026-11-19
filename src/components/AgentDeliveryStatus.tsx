import { useEffect, useState } from 'react';
import { t } from '../i18n';
import { useSdk } from '../sdk';
import { observeAgent, type ObservedAgent } from './agent-activity';

const labels = {
  working: 'Agent working',
  question: 'Agent awaiting your reply',
  approval: 'Agent awaiting approval',
  idle: 'Agent idle',
  stopped: 'Agent stopped',
  unknown: 'Agent activity unavailable',
  unavailable: 'Agent unavailable',
};

/** Report recipient activity, independently of acknowledgement of a particular request. */
export function AgentDeliveryStatus({
  sessionId,
  targetId,
}: {
  sessionId: number;
  targetId: number;
}) {
  const sdk = useSdk();
  const [agent, setAgent] = useState<ObservedAgent>();
  useEffect(() => {
    setAgent(undefined);
    return observeAgent(sdk, sessionId, targetId, setAgent);
  }, [sdk, sessionId, targetId]);
  return (
    <span
      className="agent-delivery-status"
      role="status"
      data-activity={agent?.activity ?? 'unknown'}
      title={t('Current agent activity; this does not confirm completion of this message.')}
    >
      <span className="agent-activity-dot" aria-hidden="true" />
      {agent?.name ? t('Message sent to {agent}', { agent: agent.name }) : t('Message sent')}
      {' · '}
      {agent ? t(labels[agent.activity]) : t('Checking agent activity…')}
    </span>
  );
}
