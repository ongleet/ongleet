export type AgentTarget = { id: number; name: string };
const prefix = 'ongleet.agent-recipient.v1.';

/** A missing remembered recipient requires an explicit choice when several remain. */
export function preferredAgent(targets: AgentTarget[], sessionId: number, threadKey?: string) {
  if (targets.length === 1) return targets[0].id;
  try {
    const thread = threadKey ? localStorage.getItem(`${prefix}${sessionId}.${threadKey}`) : null;
    const saved = thread ?? localStorage.getItem(`${prefix}${sessionId}`);
    return targets.find((target) => String(target.id) === saved)?.id;
  } catch {
    return undefined;
  }
}

/** Remember only acknowledged deliveries; storage failure must not turn success into a retry. */
export function rememberAgent(sessionId: number, target: number, threadKey?: string) {
  try {
    localStorage.setItem(`${prefix}${sessionId}`, String(target));
    if (threadKey) localStorage.setItem(`${prefix}${sessionId}.${threadKey}`, String(target));
    if (typeof window !== 'undefined') window.dispatchEvent(new Event('ongleet-agent-recipient'));
  } catch {
    // Delivery remains successful when local preferences cannot be persisted.
  }
}
