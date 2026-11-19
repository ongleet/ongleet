import type { PluginSdk } from '../sdk';
import { AgentQueues } from './agent-queue';
import { observeAgent } from './agent-activity';
import { sendToTab } from './tab-send';
import { rememberAgent } from './agent-recipient';

// Lazy construction keeps storage access out of module evaluation and test imports.
let queues: AgentQueues | undefined;
export function agentQueues() {
  return (queues ??= new AgentQueues(
    {
      getItem: (key) => localStorage.getItem(key),
      setItem: (key, value) => localStorage.setItem(key, value),
    },
    () => crypto.randomUUID(),
  ));
}

/** Host attachment ownership keeps queued messages independent of their composers. */
export function attachAgentQueues(sdk: PluginSdk, sessionId: number, tabId: number) {
  return agentQueues().attach(tabId, sessionId, {
    observe: (session, target, changed) =>
      observeAgent(sdk, session, target, (agent) => changed(agent.activity, agent.name)),
    send: (session, target, item, forced) =>
      sendToTab(sdk.tabs, session, tabId, target, item.text, !forced),
    acknowledged: (session, target, item) => rememberAgent(session, target, item.threadKey),
  });
}
