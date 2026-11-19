import type { AgentActivity, PluginSdk } from '../sdk';

export type ObservedAgent = {
  name?: string;
  activity: AgentActivity | 'unknown' | 'unavailable';
};

/** Observe metadata events without reading terminal output or polling the agent. */
export function observeAgent(
  sdk: Pick<PluginSdk, 'tabs' | 'storage'>,
  sessionId: number,
  targetId: number,
  changed: (agent: ObservedAgent) => void,
) {
  let active = true;
  let reading = false;
  let dirty = false;
  let unsubscribe: (() => void) | undefined;
  async function refresh() {
    if (!active) return;
    dirty = true;
    if (reading) return;
    reading = true;
    try {
      while (active && dirty) {
        dirty = false;
        try {
          const list = await sdk.tabs.list({ sessionId });
          // An event during this read requires another snapshot before publication.
          if (!active || dirty) continue;
          const tab = list.tabs.find((tab) => tab.id === targetId && tab.sessionId === sessionId);
          changed({
            name: tab?.name,
            activity: !tab?.isOpen ? 'unavailable' : (tab.codexActivity ?? 'unknown'),
          });
        } catch {
          if (active && !dirty) changed({ activity: 'unknown' });
        }
      }
    } finally {
      reading = false;
    }
  }
  // Subscribe first so a change between initial read and subscription cannot be lost.
  void sdk.storage
    .subscribe(() => void refresh())
    .then((dispose) => {
      if (!active) dispose();
      else {
        unsubscribe = dispose;
        void refresh();
      }
    })
    .catch(() => {
      if (active) changed({ activity: 'unknown' });
    });
  return () => {
    active = false;
    unsubscribe?.();
  };
}
