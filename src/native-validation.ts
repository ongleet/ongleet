import type { PluginSdk } from './sdk';
// Imported only by the explicit native-validation Vite build. The normal
// application bundle exposes no testing bridge or terminal handles.
type Probe = {
  state: () => {
    sessionId?: number;
    replaying: boolean;
    cols?: number;
    rows?: number;
    scrollback?: number;
    viewportY?: number;
    alternate?: boolean;
  };
  snapshot: () => string;
  selectAll: () => void;
};
let connectionGate: { promise: Promise<void>; release: () => void; tabId?: number } | undefined;
export function waitForConnection(id: number) {
  if (connectionGate && connectionGate.tabId === undefined) {
    connectionGate.tabId = id;
    return connectionGate.promise;
  }
}
const terminals = new Map<number, Probe>();
export function registerTerminal(id: number, probe: Probe) {
  terminals.set(id, probe);
  return () => {
    if (terminals.get(id) === probe) terminals.delete(id);
  };
}
Object.assign(window, {
  __terminalValidation: {
    holdNextConnection: () => {
      if (connectionGate) throw new Error('A connection is already held');
      let release!: () => void;
      const promise = new Promise<void>((resolve) => {
        release = resolve;
      });
      connectionGate = { promise, release };
    },
    pendingConnection: () => connectionGate?.tabId,
    releaseConnection: () => {
      connectionGate?.release();
      connectionGate = undefined;
    },
    state: (id: number) => terminals.get(id)?.state(),
    snapshot: (id: number) => terminals.get(id)?.snapshot(),
    selectAll: (id: number) => terminals.get(id)?.selectAll(),
  },
});

const sdks = new Map<number, PluginSdk>();
export function registerSdk(id: number, sdk: PluginSdk) {
  sdks.set(id, sdk);
  return () => {
    if (sdks.get(id) === sdk) sdks.delete(id);
  };
}
Object.assign(window, {
  __sdkValidation: {
    ready: (id: number) => sdks.has(id),
    call: (
      id: number,
      service: 'session' | 'fs' | 'storage' | 'process' | 'pty' | 'desktop',
      method: string,
      args: unknown[],
    ) => {
      const sdk = sdks.get(id);
      if (!sdk) throw new Error('SDK attachment unavailable');
      const operation = (sdk[service] as Record<string, (...args: never[]) => unknown>)[method];
      if (typeof operation !== 'function') throw new Error('Unknown SDK operation');
      return operation(...(args as never[]));
    },
  },
});
